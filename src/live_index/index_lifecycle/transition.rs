//! Writer-validated Freeze -> Drain -> Install transitions (T027).
//!
//! Reload, rebind, and physical-root replacement all take the same ordered path.
//! Freeze publishes non-`Current` so nothing new can be granted; Drain refuses to
//! proceed while a permit is still outstanding; Install revokes the outgoing
//! lease before the incoming one exists. Because Install revokes first, a permit
//! that survived from the previous root can no longer resolve a path under it.

use std::sync::Arc;

use super::authority::{
    AuthorityRefusal, BindingAuthority, CurrentPublication, ObserverToken, SourceRuntime,
};
use super::mutation::PermitDrainSignal;
use super::physical_root::PhysicalRootLease;

/// Which transition is being applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    /// Rebuild the same binding on the same root.
    Reload,
    /// Move the source to a different binding.
    Rebind,
    /// Replace the physical root beneath the source.
    PhysicalRootReplacement,
}

/// One observed step of a transition, recorded as it happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStep {
    /// The source was published non-`Current`.
    Freeze,
    /// Outstanding permits were confirmed ended.
    Drain,
    /// The outgoing lease was revoked and the incoming binding installed.
    Install,
}

/// What a transition actually did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionReceipt {
    kind: TransitionKind,
    steps: Vec<TransitionStep>,
}

impl TransitionReceipt {
    /// The transition that was applied.
    pub fn kind(&self) -> TransitionKind {
        self.kind
    }

    /// The ordered steps that were observed.
    pub fn steps(&self) -> &[TransitionStep] {
        &self.steps
    }
}

/// Apply a writer-validated transition.
///
/// `outstanding` is the drain signal of the permit the source last issued, if
/// any. The transition refuses rather than installing over a live permit.
pub fn apply(
    runtime: &mut SourceRuntime,
    kind: TransitionKind,
    outgoing: &Arc<PhysicalRootLease>,
    incoming: BindingAuthority,
    observer_cut: ObserverToken,
    outstanding: &PermitDrainSignal,
) -> Result<TransitionReceipt, AuthorityRefusal> {
    let mut steps = Vec::new();

    // Drain is checked BEFORE Freeze so a refusal leaves no trace. Freezing
    // first meant an `Err(OutstandingPermit)` had already moved the phase and
    // advanced the epoch, so a caller retrying on `Err` was operating on a
    // source that had changed underneath it -- the same "refusal leaves no
    // trace" discipline this slice applies to grants, which it was not applying
    // to transitions. Checking first costs nothing: an outstanding permit
    // implies its own grant already published non-`Current`, so freezing before
    // the check bought no additional safety.
    if !outstanding.has_ended() {
        return Err(AuthorityRefusal::OutstandingPermit);
    }

    // Freeze: stop granting before anything else moves. This mutates the source
    // in place rather than replacing it, so the monotonic mutation epoch and the
    // permit record survive the transition. Constructing a fresh `SourceRuntime`
    // here would rewind the epoch to its initial value on every reload and
    // rebind, which would let a stale authority compare equal to a later one.
    runtime.freeze();
    steps.push(TransitionStep::Freeze);

    // Observed again after the freeze, so the recorded Drain step is an
    // observation rather than an inference from the precondition above. Nothing
    // can acquire a permit in between -- the source is non-`Current` now, so
    // `request_mutation_grant` refuses -- but the step claims an observation, so
    // it makes one.
    if !outstanding.has_ended() {
        return Err(AuthorityRefusal::OutstandingPermit);
    }
    steps.push(TransitionStep::Drain);

    // Install: revoke the outgoing root before the incoming binding is live, so
    // no surviving permit can resolve a path under the replaced root.
    outgoing.revoke();
    runtime.install(CurrentPublication::promote(incoming, observer_cut));
    steps.push(TransitionStep::Install);

    Ok(TransitionReceipt { kind, steps })
}
