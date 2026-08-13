//! Whole mutation authority for the V11 index lifecycle (T024).
//!
//! Every identity here is drawn from one process-global monotonic counter, so no
//! identity is ever reused and no two identities of different kinds can collide.
//! Identities are diagnostics for ordering only: nothing in this module grants a
//! mutation because a counter compared equal. A mutation is authorized by a
//! sealed [`CurrentMutationGrantAuthority`] that was consumed from an exact live
//! `Current` publication, or it is refused.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-global monotonic identity source. Never restarts, never reuses.
static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

fn next_identity() -> NonZeroU64 {
    let raw = NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed);
    NonZeroU64::new(raw).expect("identity counter starts at 1 and only increases")
}

macro_rules! identity_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Mint a fresh never-reused identity.
            pub fn fresh() -> Self {
                Self(next_identity())
            }
        }
    };
}

identity_newtype!(
    /// Identity of a source binding to one physical root.
    BindingIdentity
);
identity_newtype!(
    /// Stable identity of a filesystem observer registration.
    ObserverToken
);
identity_newtype!(
    /// Identity of an in-progress candidate build.
    CandidateIdentity
);
identity_newtype!(
    /// Identity of a promoted generation's authority.
    GenerationIdentity
);
identity_newtype!(
    /// Identity of one publication of source runtime state.
    PublicationIdentity
);
identity_newtype!(
    /// Identity of an untrusted on-disk snapshot seed.
    SnapshotIdentity
);

/// Monotonic per-source mutation epoch. An ordering aid, never an authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MutationEpoch(u64);

impl MutationEpoch {
    /// The epoch a source occupies before any mutation has ever been granted.
    pub const fn initial() -> Self {
        Self(0)
    }

    fn advanced(self) -> Self {
        Self(self.0 + 1)
    }

    /// Diagnostic value. Callers must not derive permission from it.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Authority binding one source to exactly one physical root.
///
/// Cloneable: a binding is a description of which root is authoritative, not a
/// consumable permission. Equality is exact on both the binding identity and the
/// root lease identity, so a rebind never compares equal to its predecessor.
///
/// **A clone shares its liveness with the original**, because a clone IS this
/// binding rather than a copy of it. Revocation therefore reaches authority that
/// was already handed out — the registry can refuse to hand out a stopped slot's
/// binding, but it cannot reach a clone a holder took before the stop, and a
/// permit granted from such a clone would write to disk under a slot the
/// registry had already retired.
#[derive(Debug, Clone)]
pub struct BindingAuthority {
    identity: BindingIdentity,
    physical_root: crate::live_index::index_lifecycle::physical_root::PhysicalRootIdentity,
    /// Shared across clones on purpose; see the type comment.
    revoked: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Identity, not liveness. Two handles on one binding are the same binding
/// whether or not it has since been revoked, and comparing the flag would make
/// a revocation look like a rebind.
impl PartialEq for BindingAuthority {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity && self.physical_root == other.physical_root
    }
}

impl Eq for BindingAuthority {}

impl BindingAuthority {
    /// Bind a source to a physical root under a fresh never-reused identity.
    pub fn bind(
        physical_root: crate::live_index::index_lifecycle::physical_root::PhysicalRootIdentity,
    ) -> Self {
        Self {
            identity: BindingIdentity::fresh(),
            physical_root,
            revoked: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Retire this binding and every clone of it. Idempotent, never undone.
    pub fn revoke(&self) {
        self.revoked
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Whether this binding still authorizes anything.
    pub fn is_live(&self) -> bool {
        !self.revoked.load(std::sync::atomic::Ordering::Acquire)
    }

    /// The binding's identity.
    pub fn identity(&self) -> BindingIdentity {
        self.identity
    }

    /// The physical root this binding authorizes, and no other.
    pub fn physical_root(
        &self,
    ) -> crate::live_index::index_lifecycle::physical_root::PhysicalRootIdentity {
        self.physical_root
    }
}

/// Authority for one in-progress candidate build.
///
/// A candidate is never a mutation grant: it has not published, so it cannot
/// authorize a source-disk side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateAuthority {
    identity: CandidateIdentity,
    binding: BindingAuthority,
    observer_cut: ObserverToken,
}

impl CandidateAuthority {
    /// Open a candidate under a binding at an observer cut.
    pub fn open(binding: BindingAuthority, observer_cut: ObserverToken) -> Self {
        Self {
            identity: CandidateIdentity::fresh(),
            binding,
            observer_cut,
        }
    }

    /// The candidate's identity.
    pub fn identity(&self) -> CandidateIdentity {
        self.identity
    }

    /// The binding this candidate was opened under.
    pub fn binding(&self) -> &BindingAuthority {
        &self.binding
    }

    /// The observer cut this candidate is building through.
    pub fn observer_cut(&self) -> ObserverToken {
        self.observer_cut
    }
}

/// Generation-bound mutation authority.
///
/// Carries the exact generation, binding, and epoch a mutation is bound to. It
/// is produced only from a consumed [`CurrentMutationGrantAuthority`] and is
/// validated as a whole: no consumer may compare one field and infer the rest.
#[derive(Debug, PartialEq, Eq)]
pub struct MutationAuthority {
    generation: GenerationIdentity,
    binding: BindingAuthority,
    epoch: MutationEpoch,
    publication: PublicationIdentity,
}

impl MutationAuthority {
    /// The generation this authority is bound to.
    pub fn generation(&self) -> GenerationIdentity {
        self.generation
    }

    /// The binding this authority is bound to.
    pub fn binding(&self) -> &BindingAuthority {
        &self.binding
    }

    /// The mutation epoch this authority was granted at.
    pub fn epoch(&self) -> MutationEpoch {
        self.epoch
    }

    /// The exact `Current` publication this authority was consumed from.
    pub fn publication(&self) -> PublicationIdentity {
        self.publication
    }
}

/// Proof that the granting source published a non-`Current` state.
///
/// The permit refuses every source-disk side effect until it holds one of these,
/// so a side effect can never precede the publication that makes the source
/// non-queryable.
#[derive(Debug, PartialEq, Eq)]
pub struct NonCurrentPublicationProof {
    publication: PublicationIdentity,
    epoch: MutationEpoch,
}

impl NonCurrentPublicationProof {
    /// The non-`Current` publication that was actually stored.
    pub fn publication(&self) -> PublicationIdentity {
        self.publication
    }

    /// The epoch that publication carried.
    pub fn epoch(&self) -> MutationEpoch {
        self.epoch
    }
}

/// Sealed, non-cloneable authority to grant exactly one mutation permit.
///
/// Constructible only inside this module, and only by
/// [`SourceRuntime::request_mutation_grant`] after it has validated an exact live
/// `Current` publication and published non-`Current`.
#[derive(Debug)]
pub struct CurrentMutationGrantAuthority {
    authority: MutationAuthority,
    published_non_current: NonCurrentPublicationProof,
}

impl CurrentMutationGrantAuthority {
    /// The whole generation-bound authority this grant carries.
    pub fn authority(&self) -> &MutationAuthority {
        &self.authority
    }

    /// Proof the source was published non-`Current` before this grant existed.
    pub fn published_non_current(&self) -> &NonCurrentPublicationProof {
        &self.published_non_current
    }

    /// Consume the grant, yielding its parts. Consumption is by move, so a grant
    /// can authorize at most one permit.
    pub(crate) fn into_parts(self) -> (MutationAuthority, NonCurrentPublicationProof) {
        (self.authority, self.published_non_current)
    }
}

/// The live `Current` publication of a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentPublication {
    publication: PublicationIdentity,
    generation: GenerationIdentity,
    binding: BindingAuthority,
    observer_cut: ObserverToken,
}

impl CurrentPublication {
    /// Promote a verified generation to `Current` under a fresh publication.
    pub fn promote(binding: BindingAuthority, observer_cut: ObserverToken) -> Self {
        Self {
            publication: PublicationIdentity::fresh(),
            generation: GenerationIdentity::fresh(),
            binding,
            observer_cut,
        }
    }

    /// The publication identity a mutation grant must name exactly.
    pub fn publication(&self) -> PublicationIdentity {
        self.publication
    }

    /// The generation this publication makes queryable.
    pub fn generation(&self) -> GenerationIdentity {
        self.generation
    }

    /// The binding this publication was promoted under.
    pub fn binding(&self) -> &BindingAuthority {
        &self.binding
    }

    /// The observer cut this publication is complete through.
    pub fn observer_cut(&self) -> ObserverToken {
        self.observer_cut
    }
}

/// What a caller presents when asking for a mutation grant.
///
/// Only [`MutationGrantInput::LiveCurrent`] naming the exact live publication can
/// ever be accepted. Every other shape is a source of authority that has not
/// published a queryable generation, and is refused by construction rather than
/// by inspecting its contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationGrantInput {
    /// The publication the caller believes is live and `Current`.
    LiveCurrent(PublicationIdentity),
    /// An in-progress candidate build.
    Candidate(CandidateIdentity),
    /// An untrusted on-disk snapshot seed.
    Snapshot(SnapshotIdentity),
    /// A generation retained by a non-`Current` state for recovery or accounting.
    RetainedGeneration(GenerationIdentity),
    /// A publication that was `Current` at some earlier point.
    StalePublication(PublicationIdentity),
}

impl MutationGrantInput {
    fn provenance(&self) -> Provenance {
        match self {
            Self::LiveCurrent(_) => Provenance::LiveCurrent,
            Self::Candidate(_) => Provenance::Candidate,
            Self::Snapshot(_) => Provenance::Snapshot,
            Self::RetainedGeneration(_) => Provenance::RetainedGeneration,
            Self::StalePublication(_) => Provenance::StalePublication,
        }
    }
}

/// Named provenance of a refused grant input, for refusal reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// A publication claimed to be the live `Current` one.
    LiveCurrent,
    /// An in-progress candidate.
    Candidate,
    /// An on-disk snapshot seed.
    Snapshot,
    /// A generation retained by a non-`Current` state.
    RetainedGeneration,
    /// A previously-`Current` publication.
    StalePublication,
}

/// Named source phase, for refusal reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseName {
    /// No generation retained, none queryable.
    Loading,
    /// Exactly one generation queryable.
    Current,
    /// One generation retained, none queryable.
    Refreshing,
    /// Zero or one generation retained for recovery, none queryable.
    Blocked,
    /// Revoked; zero or one generation retained for accounting.
    Stopping,
}

/// The live runtime phase of one source.
#[derive(Debug)]
enum SourcePhase {
    Loading {
        binding: BindingAuthority,
        observer_phase: ObserverPhase,
        work: NonCurrentWork,
    },
    Current(CurrentPublication),
    Refreshing {
        retained: GenerationIdentity,
        binding: BindingAuthority,
        publication: PublicationIdentity,
        observer_phase: ObserverPhase,
        /// Permits outstanding against this source.
        ///
        /// Plural, per the frozen model. Slice 1 tracked a single
        /// `PermitDrainSignal` and refused a transition on one outstanding
        /// permit, which cannot express a source draining several at once.
        active_permits: ActivePermits,
        work: NonCurrentWork,
    },
    Blocked {
        binding: BindingAuthority,
        observer_phase: ObserverPhase,
        retained: Option<GenerationIdentity>,
        cause: BlockedCause,
    },
    Stopping {
        retained: Option<GenerationIdentity>,
        /// Teardown capacity already charged for this source's revocation.
        ///
        /// The frozen model requires a `Stopping` source to carry the residency
        /// it has committed, so a revocation cannot be started that the process
        /// cannot afford to finish. Slice 2's capacity work refunds against this;
        /// until `capacity.rs` lands it records the charge as an opaque receipt
        /// rather than pretending no charge exists.
        committed_source_revocation_residency: Option<RevocationResidency>,
    },
}

/// Where a source's filesystem observer stands.
///
/// A source may hold no observer at all, hold one, be handing one over, or have
/// lost one and be waiting to retry. Slice 1 carried only a bare `ObserverToken`
/// where a token existed and had no way to say "absent" or "mid-handoff".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserverPhase {
    /// No observer has been registered yet.
    Absent,
    /// An observer is registered and delivering.
    Active {
        /// The stable token identifying this registration.
        token: ObserverToken,
    },
    /// An observer is being handed over and is no longer authoritative.
    Draining {
        /// The token being drained.
        token: ObserverToken,
    },
    /// The observer was lost; a replacement has not yet been registered.
    ObserverFree {
        /// Identifies the handoff attempt, so a retry is not mistaken for a new one.
        handoff: ObserverToken,
    },
}

impl ObserverPhase {
    /// The token this phase stands on, when one exists.
    ///
    /// `Absent` and `ObserverFree` deliberately return `None` rather than
    /// inventing a token: a source with no live observer must not be able to
    /// present one.
    pub fn token(&self) -> Option<ObserverToken> {
        match self {
            Self::Active { token } | Self::Draining { token } => Some(*token),
            Self::Absent | Self::ObserverFree { .. } => None,
        }
    }
}

/// What a non-`Current` source is doing about becoming `Current` again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonCurrentWork {
    /// Known stale, nothing started yet.
    Dirty,
    /// Cannot start until capacity is granted.
    WaitingForCapacity,
    /// A candidate is being built under this authority.
    Building {
        /// The candidate doing the work.
        candidate: CandidateIdentity,
    },
    /// A built candidate is being verified.
    Verifying {
        /// The candidate under verification.
        candidate: CandidateIdentity,
    },
    /// A previous attempt failed and a retry is pending.
    RetryWait {
        /// How many attempts have been made.
        attempt: u32,
    },
}

/// The permits outstanding against one source.
///
/// A newtype rather than a bare count: the frozen model requires a transition to
/// refuse while ANY permit is outstanding, and a count that could be decremented
/// twice would silently authorize an install over a live permit.
///
/// `issued` is sticky. A mutation-entered refresh records a permit and stays
/// unqueryable even after every permit retires, until a successor `Current`
/// installs. Drain still cares only about the outstanding set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivePermits {
    outstanding: Vec<PublicationIdentity>,
    issued: bool,
}

impl ActivePermits {
    /// No permits outstanding, and none ever recorded.
    pub fn none() -> Self {
        Self::default()
    }

    /// Record a permit issued under `grant`.
    pub fn record(&mut self, grant: PublicationIdentity) {
        self.issued = true;
        if !self.outstanding.contains(&grant) {
            self.outstanding.push(grant);
        }
    }

    /// Retire a permit. Returns whether it was outstanding, so a caller cannot
    /// retire a permit twice and drive the count below the truth.
    ///
    /// Does not clear [`Self::ever_issued`]: queryability is about how the
    /// refresh was entered, not about the outstanding set draining.
    pub fn retire(&mut self, grant: PublicationIdentity) -> bool {
        let before = self.outstanding.len();
        self.outstanding.retain(|entry| *entry != grant);
        self.outstanding.len() != before
    }

    /// Whether anything is still outstanding.
    pub fn is_drained(&self) -> bool {
        self.outstanding.is_empty()
    }

    /// Whether this source has ever recorded a mutation permit.
    pub fn ever_issued(&self) -> bool {
        self.issued
    }

    /// The permits still outstanding, so a caller can retire them by name
    /// rather than having to have remembered each identity it was issued.
    pub fn outstanding(&self) -> impl Iterator<Item = PublicationIdentity> + '_ {
        self.outstanding.iter().copied()
    }

    /// How many permits are outstanding.
    pub fn len(&self) -> usize {
        self.outstanding.len()
    }

    /// Whether no permit is currently outstanding.
    pub fn is_empty(&self) -> bool {
        self.outstanding.is_empty()
    }
}

/// Why a source is `Blocked`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockedCause {
    /// A candidate failed verification and no retry can help without operator action.
    VerificationFailed,
    /// The observer could not be re-registered.
    ObserverUnavailable,
    /// Capacity was refused and the request cannot be satisfied.
    CapacityRefused,
}

/// Teardown capacity committed to a revocation.
///
/// Opaque until `capacity.rs` lands. It exists now so `Stopping` can carry the
/// charge the frozen model requires rather than omitting the field and implying
/// no charge was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevocationResidency {
    charge: PublicationIdentity,
}

impl RevocationResidency {
    /// Record a committed teardown charge under a never-reused identity.
    pub fn committed() -> Self {
        Self {
            charge: PublicationIdentity::fresh(),
        }
    }

    /// The identity of the charge, for refund reconciliation.
    pub fn charge(self) -> PublicationIdentity {
        self.charge
    }
}

impl SourcePhase {
    fn name(&self) -> PhaseName {
        match self {
            Self::Loading { .. } => PhaseName::Loading,
            Self::Current(_) => PhaseName::Current,
            Self::Refreshing { .. } => PhaseName::Refreshing,
            Self::Blocked { .. } => PhaseName::Blocked,
            Self::Stopping { .. } => PhaseName::Stopping,
        }
    }
}

/// Why an authority request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityRefusal {
    /// The source is not `Current`, so no queryable generation exists to grant from.
    PhaseNotCurrent {
        /// The phase actually observed.
        phase: PhaseName,
    },
    /// The presented input is not a live `Current` publication.
    ProvenanceNotLiveCurrent {
        /// The provenance actually presented.
        provenance: Provenance,
    },
    /// The named publication is not the one that is live.
    PublicationIdentityMismatch {
        /// The publication the caller named.
        presented: PublicationIdentity,
        /// The publication that is actually live.
        live: PublicationIdentity,
    },
    /// The permit's pinned physical root is no longer installed.
    PhysicalRootReplaced,
    /// The permit has already reached a terminal state.
    PermitAlreadyTerminal,
    /// A side effect was attempted on a permit that is already in flight.
    SideEffectAlreadyInFlight,
    /// The authority presented does not match the live source as a whole.
    WholeAuthorityMismatch,
    /// The binding the authority names has been revoked, so it authorizes
    /// nothing regardless of how the holder obtained it.
    BindingRevoked {
        /// The retired binding, for diagnosis.
        binding: BindingIdentity,
    },
    /// A transition was attempted while a permit was still outstanding.
    OutstandingPermit,
}

/// The live runtime of one source: the sole owner of its phase, epoch, and the
/// record of permits it has issued.
#[derive(Debug)]
pub struct SourceRuntime {
    phase: SourcePhase,
    mutation_epoch: MutationEpoch,
    grants_issued: u64,
}

impl SourceRuntime {
    /// Start a source in `Loading` with no queryable generation.
    pub fn loading(binding: BindingAuthority) -> Self {
        Self {
            phase: SourcePhase::Loading {
                binding,
                observer_phase: ObserverPhase::Absent,
                work: NonCurrentWork::Dirty,
            },
            mutation_epoch: MutationEpoch::initial(),
            grants_issued: 0,
        }
    }

    /// Start a source already `Current` on the given publication.
    pub fn current(publication: CurrentPublication) -> Self {
        Self {
            phase: SourcePhase::Current(publication),
            mutation_epoch: MutationEpoch::initial(),
            grants_issued: 0,
        }
    }

    /// Start a source in `Refreshing`, retaining one generation.
    pub fn refreshing(binding: BindingAuthority, retained: GenerationIdentity) -> Self {
        Self {
            phase: SourcePhase::Refreshing {
                retained,
                binding,
                publication: PublicationIdentity::fresh(),
                observer_phase: ObserverPhase::Absent,
                active_permits: ActivePermits::none(),
                work: NonCurrentWork::Dirty,
            },
            mutation_epoch: MutationEpoch::initial(),
            grants_issued: 0,
        }
    }

    /// Start a source in `Blocked`.
    pub fn blocked(binding: BindingAuthority, retained: Option<GenerationIdentity>) -> Self {
        Self {
            phase: SourcePhase::Blocked {
                binding,
                observer_phase: ObserverPhase::Absent,
                retained,
                cause: BlockedCause::VerificationFailed,
            },
            mutation_epoch: MutationEpoch::initial(),
            grants_issued: 0,
        }
    }

    /// Start a source in `Stopping`, carrying the teardown capacity it committed.
    pub fn stopping(
        retained: Option<GenerationIdentity>,
        committed_source_revocation_residency: Option<RevocationResidency>,
    ) -> Self {
        Self {
            phase: SourcePhase::Stopping {
                retained,
                committed_source_revocation_residency,
            },
            mutation_epoch: MutationEpoch::initial(),
            grants_issued: 0,
        }
    }

    /// The observer phase this source stands on.
    ///
    /// `Current` sources carry their observer cut inside the publication, so this
    /// reports `Active` on the publication's cut rather than inventing a
    /// separate one.
    pub fn observer_phase(&self) -> ObserverPhase {
        match &self.phase {
            SourcePhase::Current(publication) => ObserverPhase::Active {
                token: publication.observer_cut(),
            },
            SourcePhase::Loading { observer_phase, .. }
            | SourcePhase::Refreshing { observer_phase, .. }
            | SourcePhase::Blocked { observer_phase, .. } => observer_phase.clone(),
            SourcePhase::Stopping { .. } => ObserverPhase::Absent,
        }
    }

    /// What a non-`Current` source is doing about becoming `Current` again.
    pub fn work(&self) -> Option<NonCurrentWork> {
        match &self.phase {
            SourcePhase::Loading { work, .. } | SourcePhase::Refreshing { work, .. } => {
                Some(work.clone())
            }
            SourcePhase::Current(_)
            | SourcePhase::Blocked { .. }
            | SourcePhase::Stopping { .. } => None,
        }
    }

    /// The permits outstanding against this source.
    ///
    /// Only a `Refreshing` source can hold permits: a grant moves the source off
    /// `Current`, and no other phase issues one.
    pub fn active_permits(&self) -> ActivePermits {
        match &self.phase {
            SourcePhase::Refreshing { active_permits, .. } => active_permits.clone(),
            _ => ActivePermits::none(),
        }
    }

    /// The teardown capacity a `Stopping` source has committed, if any.
    pub fn committed_revocation_residency(&self) -> Option<RevocationResidency> {
        match &self.phase {
            SourcePhase::Stopping {
                committed_source_revocation_residency,
                ..
            } => *committed_source_revocation_residency,
            _ => None,
        }
    }

    /// The current phase name.
    pub fn phase(&self) -> PhaseName {
        self.phase.name()
    }

    /// The current mutation epoch. Diagnostic only.
    pub fn mutation_epoch(&self) -> MutationEpoch {
        self.mutation_epoch
    }

    /// How many permits this source has ever issued. A refused request must not
    /// change this.
    pub fn grants_issued(&self) -> u64 {
        self.grants_issued
    }

    /// The generation this phase retains, if any. Accounting, not permission.
    ///
    /// `Loading` retains none, `Refreshing` exactly one, and `Blocked`/`Stopping`
    /// zero or one for recovery and accounting. Retention says nothing about
    /// whether a reader may be served: ask [`Self::queryable_generation`], which
    /// is where A20 lives. This doc previously stated the pre-A20 rule — that a
    /// retained generation is never queryable — twelve lines above the code that
    /// implements the amendment, which is precisely the claim Slice 4 would have
    /// read when it wired reads.
    pub fn retained_generation(&self) -> Option<GenerationIdentity> {
        match &self.phase {
            SourcePhase::Loading { .. } => None,
            SourcePhase::Current(publication) => Some(publication.generation()),
            SourcePhase::Refreshing { retained, .. } => Some(*retained),
            SourcePhase::Blocked { retained, .. } | SourcePhase::Stopping { retained, .. } => {
                *retained
            }
        }
    }

    /// The generation a reader may be served from, if any (F020-V11-A20).
    ///
    /// Queryability closes on COMPLETENESS, not recency. `Current` is queryable.
    /// So is the single generation a **reload-entered** `Refreshing` retains: it
    /// is the complete verified generation that was `Current` immediately before
    /// the refresh began, so serving it exposes no partial state, and refusing
    /// to serve it would take the source offline for the whole of a rebuild
    /// while buying no safety at all.
    ///
    /// A **mutation-entered** `Refreshing` is the other door. It is reached
    /// through `request_mutation_grant`, which freezes precisely so the source
    /// stops being queryable before a source-disk side effect is authorized.
    /// There the retained generation describes files a permit has replaced (or
    /// is replacing), so it is complete and must still not be served — even
    /// after every permit retires — until a successor `Current` installs.
    /// FR-043 forbids any terminal path from restoring the prior publication;
    /// draining the outstanding set is not an install.
    ///
    /// `Blocked` and `Stopping` return `None` even when they retain something.
    /// Neither has a successor in flight, so its retention is a remnant rather
    /// than a refresh, and serving it would be serving the last thing that
    /// happened to work rather than a generation something is actively replacing.
    /// `Loading` has nothing.
    pub fn queryable_generation(&self) -> Option<GenerationIdentity> {
        match &self.phase {
            SourcePhase::Current(publication) => Some(publication.generation()),
            SourcePhase::Refreshing {
                retained,
                active_permits,
                ..
            } => (!active_permits.ever_issued()).then_some(*retained),
            SourcePhase::Loading { .. }
            | SourcePhase::Blocked { .. }
            | SourcePhase::Stopping { .. } => None,
        }
    }

    /// Whether a reader may currently be served at all.
    pub fn is_queryable(&self) -> bool {
        self.queryable_generation().is_some()
    }

    /// The binding a non-`Current` phase is still bound to, if any.
    /// Every phase except `Stopping` is bound to a physical root.
    ///
    /// `Loading` and `Blocked` previously reported `None` here, which read as
    /// "this source has no binding" when in fact both carry one — a `Loading`
    /// source is bound before it has a generation, and a `Blocked` source stays
    /// bound so an operator can act on the right root. Only `Stopping` has
    /// genuinely surrendered its binding.
    pub fn retained_binding(&self) -> Option<&BindingAuthority> {
        match &self.phase {
            SourcePhase::Current(publication) => Some(publication.binding()),
            SourcePhase::Refreshing { binding, .. }
            | SourcePhase::Loading { binding, .. }
            | SourcePhase::Blocked { binding, .. } => Some(binding),
            SourcePhase::Stopping { .. } => None,
        }
    }

    /// Why a `Blocked` source is blocked.
    ///
    /// `None` for every other phase rather than a default cause: a source that
    /// is not blocked has no reason to report.
    pub fn blocked_cause(&self) -> Option<BlockedCause> {
        match &self.phase {
            SourcePhase::Blocked { cause, .. } => Some(cause.clone()),
            _ => None,
        }
    }

    /// The live `Current` publication, if the source is `Current`.
    pub fn live_publication(&self) -> Option<&CurrentPublication> {
        match &self.phase {
            SourcePhase::Current(publication) => Some(publication),
            _ => None,
        }
    }

    /// The identity of the publication this source currently stands on, when it
    /// stands on one.
    ///
    /// `Blocked` and `Stopping` report their immutable phase without inventing a
    /// publication identity they never stored.
    pub fn published_identity(&self) -> Option<PublicationIdentity> {
        match &self.phase {
            SourcePhase::Current(publication) => Some(publication.publication()),
            SourcePhase::Refreshing { publication, .. } => Some(*publication),
            SourcePhase::Loading { .. }
            | SourcePhase::Blocked { .. }
            | SourcePhase::Stopping { .. } => None,
        }
    }

    /// Publish non-`Current`, advancing the mutation epoch, and return the
    /// identity of the publication actually stored.
    ///
    /// Epoch and permit record are carried across, never reset: the epoch is
    /// monotonic for the life of the source, so a reload or rebind cannot rewind
    /// it and let a stale authority compare equal to a later one.
    /// Returns `None` when the phase had nothing to publish.
    ///
    /// `Loading`, `Blocked` and `Stopping` store no publication, so an earlier
    /// version returning a freshly minted identity for them was attesting to a
    /// publication nothing had stored — the same defect this slice already fixed
    /// once. It also advanced the epoch for a freeze that did not happen; it now
    /// leaves the epoch alone on that path too.
    pub fn freeze(&mut self) -> Option<PublicationIdentity> {
        // Observer phase, outstanding permits and in-flight work are CARRIED
        // ACROSS a re-freeze, not reset. A source already `Refreshing` with a
        // live permit that froze again would otherwise publish an empty
        // `active_permits`, and the very next transition would see a drained
        // source and install over that permit -- the defect this slice widened
        // the model to make expressible.
        let (retained, binding, observer_phase, active_permits, work) = match &self.phase {
            SourcePhase::Current(current) => (
                current.generation(),
                current.binding().clone(),
                ObserverPhase::Active {
                    token: current.observer_cut(),
                },
                ActivePermits::none(),
                NonCurrentWork::Dirty,
            ),
            SourcePhase::Refreshing {
                retained,
                binding,
                observer_phase,
                active_permits,
                work,
                ..
            } => (
                *retained,
                binding.clone(),
                observer_phase.clone(),
                active_permits.clone(),
                work.clone(),
            ),
            SourcePhase::Loading { .. }
            | SourcePhase::Blocked { .. }
            | SourcePhase::Stopping { .. } => {
                return None;
            }
        };
        let publication = PublicationIdentity::fresh();
        self.mutation_epoch = self.mutation_epoch.advanced();
        self.phase = SourcePhase::Refreshing {
            retained,
            binding,
            publication,
            observer_phase,
            active_permits,
            work,
        };
        Some(publication)
    }

    /// Record that a permit was issued against this source's current freeze.
    ///
    /// Returns whether it was recorded: only a `Refreshing` source can hold
    /// permits, because a grant is what moved it off `Current`.
    pub fn record_permit(&mut self, grant: PublicationIdentity) -> bool {
        match &mut self.phase {
            SourcePhase::Refreshing { active_permits, .. } => {
                active_permits.record(grant);
                true
            }
            _ => false,
        }
    }

    /// Retire a permit that has reached a terminal path.
    ///
    /// Returns whether this call retired it. A second retire of the same permit
    /// returns `false` rather than draining the source twice, so a
    /// double-terminated permit cannot make a still-busy source look drained.
    pub fn retire_permit(&mut self, grant: PublicationIdentity) -> bool {
        match &mut self.phase {
            SourcePhase::Refreshing { active_permits, .. } => active_permits.retire(grant),
            _ => false,
        }
    }

    /// Record what a non-`Current` source is now doing.
    pub fn set_work(&mut self, next: NonCurrentWork) -> bool {
        match &mut self.phase {
            SourcePhase::Loading { work, .. } | SourcePhase::Refreshing { work, .. } => {
                *work = next;
                true
            }
            _ => false,
        }
    }

    /// Record an observer phase change for a non-`Current` source.
    pub fn set_observer_phase(&mut self, next: ObserverPhase) -> bool {
        match &mut self.phase {
            SourcePhase::Loading { observer_phase, .. }
            | SourcePhase::Refreshing { observer_phase, .. }
            | SourcePhase::Blocked { observer_phase, .. } => {
                *observer_phase = next;
                true
            }
            _ => false,
        }
    }

    /// Install a new `Current` publication, preserving the monotonic epoch and
    /// the grant record.
    ///
    /// Deliberately `pub(crate)`: republishing `Current` makes the source
    /// queryable again, and a permit outstanding from the previous freeze would
    /// then be able to act against a queryable source. `transition::apply` is
    /// the only caller, and it reaches this line only after Drain has confirmed
    /// nothing is outstanding.
    pub(crate) fn install(&mut self, publication: CurrentPublication) {
        self.phase = SourcePhase::Current(publication);
    }

    /// Request the one grant that can authorize a mutation permit.
    ///
    /// On refusal, neither the mutation epoch nor the permit record advances and
    /// the phase is unchanged: a rejected request leaves no trace that a later
    /// step could mistake for permission.
    ///
    /// On acceptance, the source publishes `Refreshing` and advances its epoch
    /// *before* the grant exists, so no holder of a grant can perform a source
    /// side effect while the source is still queryable.
    pub fn request_mutation_grant(
        &mut self,
        input: MutationGrantInput,
    ) -> Result<CurrentMutationGrantAuthority, AuthorityRefusal> {
        let live = match &self.phase {
            SourcePhase::Current(publication) => publication.clone(),
            other => {
                return Err(AuthorityRefusal::PhaseNotCurrent {
                    phase: other.name(),
                });
            }
        };

        let presented = match input {
            MutationGrantInput::LiveCurrent(presented) => presented,
            other => {
                return Err(AuthorityRefusal::ProvenanceNotLiveCurrent {
                    provenance: other.provenance(),
                });
            }
        };

        if presented != live.publication() {
            return Err(AuthorityRefusal::PublicationIdentityMismatch {
                presented,
                live: live.publication(),
            });
        }

        // Publish non-Current before the grant exists. Order is the invariant:
        // the source stops being queryable first, then a mutation is authorized.
        // `freeze` performs and records that publication, and the proof below
        // names the identity it actually stored -- not a freshly minted one that
        // nothing published.
        // The phase was matched as `Current` above and nothing between here and
        // there can change it, so `freeze` has something to publish.
        let publication = self
            .freeze()
            .expect("a Current source always has a publication to freeze");
        // Record the permit this grant will become, on the source itself. The
        // freeze alone leaves `issued` false — that is the reload-entered
        // refresh A20 keeps serving. Recording the permit is what marks the
        // refresh mutation-entered, and that bit stays set after retire, so
        // reads stay closed until a successor `Current` installs. A grant that
        // did not record one would leave the source serving the very files its
        // holder is about to replace.
        self.record_permit(publication);
        let epoch = self.mutation_epoch;
        self.grants_issued += 1;

        Ok(CurrentMutationGrantAuthority {
            authority: MutationAuthority {
                generation: live.generation(),
                binding: live.binding().clone(),
                epoch,
                publication: live.publication(),
            },
            published_non_current: NonCurrentPublicationProof { publication, epoch },
        })
    }
}
