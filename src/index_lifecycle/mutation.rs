//! Non-cloneable source mutation permits (T026).
//!
//! A permit is produced only by consuming a [`CurrentMutationGrantAuthority`],
//! and it validates that grant against its pinned root lease as one whole: the
//! generation, binding, epoch, and root must belong to the same authority. No
//! consumer compares one field and infers the rest.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::authority::{
    AuthorityRefusal, CurrentMutationGrantAuthority, MutationAuthority, NonCurrentPublicationProof,
};
use super::physical_root::{PhysicalRootLease, WriteReceipt};

/// How a permit ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    /// A side effect happened and was committed.
    Committed,
    /// No side effect happened, proven.
    NoSideEffect,
    /// The permit was dropped without reaching a declared terminal path.
    Drained,
}

/// Live state of a permit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PermitState {
    Granted,
    InFlight,
    Terminal(Termination),
}

/// The signal a source watches to learn that its outstanding permit ended.
///
/// A dropped permit must not strand the source in `Refreshing` forever, so the
/// drop path reports here rather than relying on the holder to be well behaved.
#[derive(Debug, Default)]
pub struct PermitDrainSignal {
    ended: AtomicBool,
    termination: std::sync::Mutex<Option<Termination>>,
}

impl PermitDrainSignal {
    /// A fresh signal for one permit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the permit has ended by any path.
    pub fn has_ended(&self) -> bool {
        self.ended.load(Ordering::Acquire)
    }

    /// How the permit ended, once it has.
    pub fn termination(&self) -> Option<Termination> {
        *self
            .termination
            .lock()
            .expect("permit drain signal mutex is never held across a panic")
    }

    fn record(&self, termination: Termination) {
        let mut slot = self
            .termination
            .lock()
            .expect("permit drain signal mutex is never held across a panic");
        if slot.is_none() {
            *slot = Some(termination);
            self.ended.store(true, Ordering::Release);
        }
    }
}

/// Proof that a permit performed no source-disk side effect.
#[derive(Debug)]
pub struct NoSideEffectProof {
    _seal: (),
}

impl NoSideEffectProof {
    /// Attest that nothing was written. Constructible only by the lane that
    /// actually observed the absence.
    pub fn observed() -> Self {
        Self { _seal: () }
    }
}

/// A ticket requiring the source to return through a fresh candidate at the
/// latest observer cut. A permit never restores its predecessor to `Current`.
#[derive(Debug, PartialEq, Eq)]
pub struct RefreshTicket {
    epoch: super::authority::MutationEpoch,
    termination: Termination,
}

impl RefreshTicket {
    /// The mutation epoch the source must rebuild past.
    pub fn epoch(&self) -> super::authority::MutationEpoch {
        self.epoch
    }

    /// How the permit that produced this ticket ended.
    pub fn termination(&self) -> Termination {
        self.termination
    }
}

/// A tracked, non-cloneable permit to mutate exactly one source's disk state.
///
/// Deliberately not `Clone`: a permit is the single outstanding authority for
/// one mutation, and duplicating it would duplicate that authority.
#[derive(Debug)]
pub struct SourceMutationPermit {
    authority: MutationAuthority,
    published_non_current: NonCurrentPublicationProof,
    lease: Arc<PhysicalRootLease>,
    state: PermitState,
    drain: Arc<PermitDrainSignal>,
}

impl SourceMutationPermit {
    /// Consume a grant into a permit pinned to `lease`.
    ///
    /// Refuses unless the grant's binding names exactly this lease's root: an
    /// authority for root A cannot be paired with a lease on root B.
    pub fn grant(
        grant: CurrentMutationGrantAuthority,
        lease: Arc<PhysicalRootLease>,
        drain: Arc<PermitDrainSignal>,
    ) -> Result<Self, AuthorityRefusal> {
        let (authority, published_non_current) = grant.into_parts();

        if authority.binding().physical_root() != lease.identity() {
            return Err(AuthorityRefusal::WholeAuthorityMismatch);
        }
        if !lease.is_live() {
            return Err(AuthorityRefusal::PhysicalRootReplaced);
        }

        Ok(Self {
            authority,
            published_non_current,
            lease,
            state: PermitState::Granted,
            drain,
        })
    }

    /// The whole authority this permit carries.
    pub fn authority(&self) -> &MutationAuthority {
        &self.authority
    }

    /// The lease this permit is pinned to.
    pub fn lease(&self) -> &Arc<PhysicalRootLease> {
        &self.lease
    }

    /// Whether the permit has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, PermitState::Terminal(_))
    }

    /// Begin the side effect.
    ///
    /// Refuses unless the granting source already published non-`Current` and
    /// the pinned root is still installed.
    pub fn start_side_effect(&mut self) -> Result<(), AuthorityRefusal> {
        match self.state {
            PermitState::Terminal(_) => return Err(AuthorityRefusal::PermitAlreadyTerminal),
            PermitState::InFlight => return Err(AuthorityRefusal::PermitAlreadyTerminal),
            PermitState::Granted => {}
        }

        if self.published_non_current.epoch() != self.authority.epoch() {
            return Err(AuthorityRefusal::SideEffectBeforeNonCurrentPublication);
        }
        if !self.lease.is_live() {
            return Err(AuthorityRefusal::PhysicalRootReplaced);
        }

        self.state = PermitState::InFlight;
        Ok(())
    }

    /// Commit an observed write.
    pub fn commit(&mut self, receipt: WriteReceipt) -> Result<RefreshTicket, AuthorityRefusal> {
        if self.state != PermitState::InFlight {
            return Err(AuthorityRefusal::PermitAlreadyTerminal);
        }
        let _ = receipt;
        self.finish(Termination::Committed)
    }

    /// Terminate with proof that nothing was written.
    pub fn no_side_effect(
        &mut self,
        proof: NoSideEffectProof,
    ) -> Result<RefreshTicket, AuthorityRefusal> {
        if matches!(self.state, PermitState::Terminal(_)) {
            return Err(AuthorityRefusal::PermitAlreadyTerminal);
        }
        let _ = proof;
        self.finish(Termination::NoSideEffect)
    }

    fn finish(&mut self, termination: Termination) -> Result<RefreshTicket, AuthorityRefusal> {
        self.state = PermitState::Terminal(termination);
        self.drain.record(termination);
        Ok(RefreshTicket {
            epoch: self.authority.epoch(),
            termination,
        })
    }
}

impl Drop for SourceMutationPermit {
    fn drop(&mut self) {
        if !matches!(self.state, PermitState::Terminal(_)) {
            self.drain.record(Termination::Drained);
        }
    }
}
