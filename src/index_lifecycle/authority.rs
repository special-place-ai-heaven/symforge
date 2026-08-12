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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingAuthority {
    identity: BindingIdentity,
    physical_root: crate::index_lifecycle::physical_root::PhysicalRootIdentity,
}

impl BindingAuthority {
    /// Bind a source to a physical root under a fresh never-reused identity.
    pub fn bind(
        physical_root: crate::index_lifecycle::physical_root::PhysicalRootIdentity,
    ) -> Self {
        Self {
            identity: BindingIdentity::fresh(),
            physical_root,
        }
    }

    /// The binding's identity.
    pub fn identity(&self) -> BindingIdentity {
        self.identity
    }

    /// The physical root this binding authorizes, and no other.
    pub fn physical_root(&self) -> crate::index_lifecycle::physical_root::PhysicalRootIdentity {
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
    Loading,
    Current(CurrentPublication),
    Refreshing {
        retained: GenerationIdentity,
        binding: BindingAuthority,
    },
    Blocked {
        retained: Option<GenerationIdentity>,
    },
    Stopping {
        retained: Option<GenerationIdentity>,
    },
}

impl SourcePhase {
    fn name(&self) -> PhaseName {
        match self {
            Self::Loading => PhaseName::Loading,
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
    /// A side effect was attempted before the source published non-`Current`.
    SideEffectBeforeNonCurrentPublication,
    /// The authority presented does not match the live source as a whole.
    WholeAuthorityMismatch,
    /// A transition was attempted while a permit was still outstanding.
    OutstandingPermit,
}

/// The live runtime of one source: the sole owner of its phase, epoch, and the
/// record of permits it has issued.
#[derive(Debug)]
pub struct SourceRuntime {
    phase: SourcePhase,
    mutation_epoch: MutationEpoch,
    permits_issued: u64,
}

impl SourceRuntime {
    /// Start a source in `Loading` with no queryable generation.
    pub fn loading() -> Self {
        Self {
            phase: SourcePhase::Loading,
            mutation_epoch: MutationEpoch::initial(),
            permits_issued: 0,
        }
    }

    /// Start a source already `Current` on the given publication.
    pub fn current(publication: CurrentPublication) -> Self {
        Self {
            phase: SourcePhase::Current(publication),
            mutation_epoch: MutationEpoch::initial(),
            permits_issued: 0,
        }
    }

    /// Start a source in `Refreshing`, retaining one generation.
    pub fn refreshing(binding: BindingAuthority, retained: GenerationIdentity) -> Self {
        Self {
            phase: SourcePhase::Refreshing { retained, binding },
            mutation_epoch: MutationEpoch::initial(),
            permits_issued: 0,
        }
    }

    /// Start a source in `Blocked`.
    pub fn blocked(retained: Option<GenerationIdentity>) -> Self {
        Self {
            phase: SourcePhase::Blocked { retained },
            mutation_epoch: MutationEpoch::initial(),
            permits_issued: 0,
        }
    }

    /// Start a source in `Stopping`.
    pub fn stopping(retained: Option<GenerationIdentity>) -> Self {
        Self {
            phase: SourcePhase::Stopping { retained },
            mutation_epoch: MutationEpoch::initial(),
            permits_issued: 0,
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
    pub fn permits_issued(&self) -> u64 {
        self.permits_issued
    }

    /// The generation this phase retains, if any.
    ///
    /// Strict queryability is closed: only `Current` holds a query-granting
    /// generation. `Loading` retains none, `Refreshing` exactly one, and
    /// `Blocked`/`Stopping` zero or one for recovery and accounting. A retained
    /// generation is never queryable — it is reported here for accounting only.
    pub fn retained_generation(&self) -> Option<GenerationIdentity> {
        match &self.phase {
            SourcePhase::Loading => None,
            SourcePhase::Current(publication) => Some(publication.generation()),
            SourcePhase::Refreshing { retained, .. } => Some(*retained),
            SourcePhase::Blocked { retained } | SourcePhase::Stopping { retained } => *retained,
        }
    }

    /// The binding a non-`Current` phase is still bound to, if any.
    pub fn retained_binding(&self) -> Option<&BindingAuthority> {
        match &self.phase {
            SourcePhase::Current(publication) => Some(publication.binding()),
            SourcePhase::Refreshing { binding, .. } => Some(binding),
            SourcePhase::Loading | SourcePhase::Blocked { .. } | SourcePhase::Stopping { .. } => {
                None
            }
        }
    }

    /// The live `Current` publication, if the source is `Current`.
    pub fn live_publication(&self) -> Option<&CurrentPublication> {
        match &self.phase {
            SourcePhase::Current(publication) => Some(publication),
            _ => None,
        }
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
                return Err(AuthorityRefusal::PhaseNotCurrent { phase: other.name() });
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
        let epoch = self.mutation_epoch.advanced();
        let publication = PublicationIdentity::fresh();
        self.mutation_epoch = epoch;
        self.phase = SourcePhase::Refreshing {
            retained: live.generation(),
            binding: live.binding().clone(),
        };
        self.permits_issued += 1;

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
