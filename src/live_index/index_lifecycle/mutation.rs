//! Non-cloneable source mutation permits (T026).
//!
//! A permit is produced only by consuming a [`CurrentMutationGrantAuthority`],
//! and it validates that grant against its pinned root lease as one whole: the
//! generation, binding, epoch, and root must belong to the same authority. No
//! consumer compares one field and infers the rest.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    armed: AtomicBool,
    ended: AtomicBool,
    termination: std::sync::Mutex<Option<Termination>>,
}

impl PermitDrainSignal {
    /// A fresh signal for one permit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing is outstanding: either no permit was ever attached to
    /// this signal, or the permit that was has ended by some terminal path.
    ///
    /// A signal that no permit ever used reports ended, which is what lets
    /// `transition::apply` take this by value rather than as an `Option`. An
    /// optional drain is not a drain: passing `None` skipped the check entirely
    /// and installed over a live permit, making ordering 3 a calling convention
    /// instead of a property of the API.
    pub fn has_ended(&self) -> bool {
        !self.armed.load(Ordering::Acquire) || self.ended.load(Ordering::Acquire)
    }

    /// Attach a permit to this signal. Called only when a permit is created.
    fn arm(&self) {
        self.armed.store(true, Ordering::Release);
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

/// The holder's declaration that its permit performed no source-disk side
/// effect.
///
/// **This is a declaration, not a proof.** An earlier doc comment here claimed
/// it was "constructible only by the lane that actually observed the absence",
/// which was false: the constructor is public and any caller can produce one. It
/// therefore cannot distinguish an observed absence from an asserted one, and
/// naming it a proof made it read like the sibling
/// [`NonCurrentPublicationProof`], which really does derive its force from
/// private fields and a single construction site.
///
/// It is kept as a distinct type so the declaration is explicit at the call
/// site rather than implied by calling a bare method. When Slice 4 introduces
/// the write lane that can actually observe the absence, construction should
/// move behind it and this type can become a real proof.
#[derive(Debug)]
pub struct NoSideEffectProof {
    _seal: (),
}

impl NoSideEffectProof {
    /// Declare that nothing was written.
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

        drain.arm();
        Ok(Self {
            authority,
            published_non_current,
            lease,
            state: PermitState::Granted,
            drain,
        })
    }

    /// Replace a path beneath this permit's own pinned lease.
    ///
    /// Prefer this over calling `replace_beneath` with a lease of your own: here
    /// the lease that writes is the pinned one by construction, so there is no
    /// opportunity to write under a root the authority never named.
    pub fn replace_beneath(
        &mut self,
        relative: &std::path::Path,
        contents: &[u8],
    ) -> Result<WriteReceipt, AuthorityRefusal> {
        if self.state != PermitState::InFlight {
            return Err(AuthorityRefusal::PermitAlreadyTerminal);
        }
        super::physical_root::replace_beneath(&self.lease, relative, contents)
            .map_err(AuthorityRefusal::from)
    }

    /// The whole authority this permit carries.
    pub fn authority(&self) -> &MutationAuthority {
        &self.authority
    }

    /// The lease this permit is pinned to.
    pub fn lease(&self) -> &Arc<PhysicalRootLease> {
        &self.lease
    }

    /// Proof that the source published non-`Current` before this permit existed.
    ///
    /// Holding a permit at all is the evidence, since the proof is constructible
    /// only inside the grant path. It is exposed so a caller can name the exact
    /// publication that made the source non-queryable rather than asserting that
    /// one happened.
    pub fn published_non_current(&self) -> &NonCurrentPublicationProof {
        &self.published_non_current
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
            // Naming the state the permit is actually in. Reporting
            // `PermitAlreadyTerminal` for an in-flight permit was correct in
            // outcome and wrong in what it claimed to have observed.
            PermitState::InFlight => return Err(AuthorityRefusal::SideEffectAlreadyInFlight),
            PermitState::Granted => {}
        }

        // There is deliberately no runtime comparison of the proof's epoch
        // against the authority's here. Both are assigned from the same value
        // inside `request_mutation_grant`, so such a check can never fail: it
        // would read as ordering verification while verifying nothing, which is
        // worse than no check because it implies an observation that is not
        // being made. The ordering is enforced by construction instead --
        // `NonCurrentPublicationProof` is constructible only inside the grant
        // path, after the source has published non-`Current` -- so holding this
        // permit at all is the evidence.
        if !self.lease.is_live() {
            return Err(AuthorityRefusal::PhysicalRootReplaced);
        }

        self.state = PermitState::InFlight;
        Ok(())
    }

    /// Commit an observed write.
    ///
    /// The receipt must name this permit's own lease. Discarding it and
    /// reporting `Committed` regardless was the defect three independent
    /// reviewers found: a permit pinned to root A could be handed a receipt for
    /// a write that landed under root B and would report success for it. A
    /// permit may only attest to a side effect its own authority produced.
    pub fn commit(&mut self, receipt: WriteReceipt) -> Result<RefreshTicket, AuthorityRefusal> {
        if self.state != PermitState::InFlight {
            return Err(AuthorityRefusal::PermitAlreadyTerminal);
        }
        if receipt.lease() != self.lease.identity() {
            return Err(AuthorityRefusal::WholeAuthorityMismatch);
        }
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
