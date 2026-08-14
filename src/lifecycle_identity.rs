//! Process-wide identity minting, shared by the lifecycle runtime and the
//! protocol provenance types.
//!
//! **Why this module exists at all.** `identity_newtype!` and its counter began
//! in `src/index_lifecycle/authority.rs`, and Feature 020 V11's provenance types
//! (`src/protocol/claim_provenance.rs`) need the SAME identities. Two options
//! were rejected:
//!
//!   * Redeclaring the macro under `protocol` would create a SECOND counter, so
//!     two identities minted "fresh" could compare unequal while both claiming
//!     to be unique. Two identity spaces is a worse defect than a name mismatch,
//!     because nothing about it is visible at the call site.
//!   * Importing from `src/index_lifecycle/` would create a
//!     `protocol -> index_lifecycle` call edge. That directory is DARK for the
//!     whole preactivation period, and `index_lifecycle/mod.rs` states its
//!     darkness as "`grep -rn index_lifecycle src/` returns no hit outside it".
//!     T051 proves that property; a protocol import would end it.
//!
//! So the primitives live HERE, under neither tree. `authority.rs` and
//! `claim_provenance.rs` both use them, one counter, no call edge between them.
//!
//! **This module is `pub(crate)`, deliberately.** The frozen public-API census
//! (`derivePublicApiAtoms`, `scripts/validate-lifecycle-oracle-traceability.cjs`)
//! adds one atom per `^\s*pub\s+mod\s+NAME\s*;` line in `src/lib.rs`. A
//! `pub mod lifecycle_identity;` there would add `symforge::lifecycle_identity`
//! and WIDEN the surface that Slice 3 must leave frozen. `pub(crate) mod` does
//! not match that pattern, so it adds nothing. Modules that need to expose one
//! of these types publicly re-export it; a `pub use` outside `src/embed.rs` is
//! not counted either.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// The one counter. Every identity in the process is drawn from it, so an
/// identity minted by the lifecycle runtime can never collide with one minted
/// by a provenance receipt.
static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_identity() -> NonZeroU64 {
    let raw = NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed);
    NonZeroU64::new(raw).expect("identity counter starts at 1 and only increases")
}

macro_rules! identity_newtype {
    // No PartialOrd/Ord: with a monotonic counter, ordering identities exposes
    // MINT ORDER, an inference channel nothing should read. An earlier draft
    // added Ord so a test could sort identities; the test uses a HashSet now
    // and the derive set matches the original authority.rs newtypes exactly.
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Mint a fresh never-reused identity.
            pub fn fresh() -> Self {
                Self($crate::lifecycle_identity::next_identity())
            }

            /// Raw counter value, crate-only, for the embed boundary's
            /// kind-prefixed RENDERING and nothing else — deriving order or
            /// permission from it is the inference channel the derive set
            /// deliberately excludes. The allow is macro-wide: only the
            /// identities the boundary renders consume it today, and a
            /// per-kind opt-in would fork the macro for a lint.
            #[allow(dead_code)]
            pub(crate) fn raw_for_render(&self) -> u64 {
                self.0.get()
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

// ── Feature 020 V11 provenance identities (T043) ───────────────────────────

identity_newtype!(
    /// Identity of one atomic authority: a generation, or a single disk,
    /// worktree-scope, or Git observation.
    AuthorityIdentity
);
identity_newtype!(
    /// Identity of one claim's provenance structure. Caches, CCR, and
    /// persistence key on this, so bounded rendering must never move it.
    ProvenanceIdentity
);
identity_newtype!(
    /// Identity of one normalized operation request.
    OperationIdentity
);
identity_newtype!(
    /// Identity of one immutable ranking snapshot.
    EvaluationIdentity
);
identity_newtype!(
    /// Identity of one sealed worktree traversal.
    WorktreeScanId
);
identity_newtype!(
    /// Identity of the runtime publication that produced a claim.
    ProducingRuntimeIdentity
);

/// The closed operation vocabulary, VERBATIM from the frozen contract:
/// `contracts/public-api-v11.json` `type:embed:OperationKind` fixes exactly
/// these seven variants. An earlier draft invented four provenance-shape
/// variants under this name; that both diverged from the contract this module's
/// own header declares authoritative AND squatted the name T047's runtime
/// vocabulary owns. Provenance SHAPES are named by
/// [`ClaimProvenance::kind_name`], not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperationKind {
    AcquireRuntime,
    CloseSource,
    OpenEmbeddedSource,
    RefreshSource,
    SearchSymbols,
    SearchText,
    ShutdownRuntime,
}

impl OperationKind {
    /// Every variant, once. The Cartesian oracle iterates this so a new
    /// operation cannot be added without entering the matrix.
    pub const ALL: [Self; 7] = [
        Self::AcquireRuntime,
        Self::CloseSource,
        Self::OpenEmbeddedSource,
        Self::RefreshSource,
        Self::SearchSymbols,
        Self::SearchText,
        Self::ShutdownRuntime,
    ];

    /// Stable display name. Part of the closed contract, not a debug string.
    pub fn kind_name(self) -> &'static str {
        match self {
            Self::AcquireRuntime => "AcquireRuntime",
            Self::CloseSource => "CloseSource",
            Self::OpenEmbeddedSource => "OpenEmbeddedSource",
            Self::RefreshSource => "RefreshSource",
            Self::SearchSymbols => "SearchSymbols",
            Self::SearchText => "SearchText",
            Self::ShutdownRuntime => "ShutdownRuntime",
        }
    }
}

/// Hash of the normalized request arguments. Binds a claim to the exact
/// question asked, so a cached answer cannot be replayed for a different one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalArgumentHash(u64);

impl CanonicalArgumentHash {
    /// Build from already-normalized argument bytes.
    pub fn of_normalized(bytes: &[u8]) -> Self {
        // FNV-1a. Deterministic across runs, which a DefaultHasher is not.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        Self(hash)
    }

    /// Diagnostic value.
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// One normalized operation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationReceipt {
    identity: OperationIdentity,
    operation_kind: OperationKind,
    schema_version: u32,
    canonical_argument_hash: CanonicalArgumentHash,
}

impl OperationReceipt {
    /// The schema version every V11 receipt is minted at.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Bind a normalized request.
    pub fn normalized(operation_kind: OperationKind, normalized_arguments: &[u8]) -> Self {
        Self {
            identity: OperationIdentity::fresh(),
            operation_kind,
            schema_version: Self::SCHEMA_VERSION,
            canonical_argument_hash: CanonicalArgumentHash::of_normalized(normalized_arguments),
        }
    }

    /// Fixture constructor for oracles that do not vary the arguments.
    #[cfg(any(test, feature = "server"))]
    pub fn for_test(operation_kind: OperationKind) -> Self {
        Self::for_dark_refusal(operation_kind)
    }

    /// Mint the receipt for a DARK refusal lane (C5 ruling). The lane did
    /// not examine its arguments, so hashing them would claim a binding that
    /// did not happen — the canonical hash covers the OPERATION KIND alone,
    /// and the D-ledger records that argument identity is NOT claimed on
    /// these lanes. Slice 4 replaces this with `normalized` at the point a
    /// lane actually reads its arguments.
    pub(crate) fn for_dark_refusal(operation_kind: OperationKind) -> Self {
        Self::normalized(operation_kind, operation_kind.kind_name().as_bytes())
    }

    pub fn identity(&self) -> OperationIdentity {
        self.identity
    }

    pub fn operation_kind(&self) -> OperationKind {
        self.operation_kind
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn canonical_argument_hash(&self) -> CanonicalArgumentHash {
        self.canonical_argument_hash
    }
}

// ── Refusals ───────────────────────────────────────────────────────────────

/// The closed refusal algebra. Identical names in both frozen documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceRefusalKind {
    AdmissionUnavailable,
    InvalidSelection,
    SelectionUnavailable,
    SourceUnavailable,
}

/// What, if anything, would make a retry worth attempting. Advice only: it
/// never authorizes the retry it describes.
///
/// Variants VERBATIM from `contracts/public-api-v11.json`
/// `type:embed:RetryAdvice`. An earlier draft invented
/// `{Never, AfterRebind, AfterRefresh}` in the same commit that declared the
/// atoms authoritative; the audit caught the contradiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RetryAdvice {
    /// The same request may simply be retried.
    Automatic,
    /// The request is wrong in a way repetition cannot fix.
    Never,
    /// Retry after an observable event: a completed refresh, an installed
    /// successor generation, a selection change.
    OnEvent,
    /// Retry requires an operator action, such as a rebind to the correct root.
    Operator,
}

impl RetryAdvice {
    /// Every variant, once, for the Cartesian oracle.
    pub const ALL: [Self; 4] = [Self::Automatic, Self::Never, Self::OnEvent, Self::Operator];
}

/// A typed refusal. Opaque by construction: callers read it through the
/// accessors the activation contract names, and cannot assemble one themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRefusal {
    kind: SourceRefusalKind,
    operation: OperationReceipt,
    retry: RetryAdvice,
    evidence_identity: Option<AuthorityIdentity>,
}

impl SourceRefusal {
    pub(crate) fn new(
        kind: SourceRefusalKind,
        operation: OperationReceipt,
        retry: RetryAdvice,
        evidence_identity: Option<AuthorityIdentity>,
    ) -> Self {
        Self {
            kind,
            operation,
            retry,
            evidence_identity,
        }
    }

    /// Crate-visible mint for the dark runtime, which cannot reach the
    /// provenance lease constructors and must still refuse honestly.
    pub(crate) fn for_runtime(
        kind: SourceRefusalKind,
        operation: OperationReceipt,
        retry: RetryAdvice,
        evidence_identity: Option<AuthorityIdentity>,
    ) -> Self {
        Self::new(kind, operation, retry, evidence_identity)
    }

    pub fn kind(&self) -> SourceRefusalKind {
        self.kind
    }

    pub fn operation(&self) -> OperationReceipt {
        self.operation
    }

    pub fn retry(&self) -> RetryAdvice {
        self.retry
    }

    /// Identity of the evidence the refusal was decided on. Present whenever a
    /// refusal was reached by examining an authority rather than by rejecting a
    /// malformed request outright.
    pub fn evidence_identity(&self) -> Option<AuthorityIdentity> {
        self.evidence_identity
    }
}

/// One complete captured generation — THE authority type shared by provenance
/// capture and the V11 lifecycle runtime (E7 ruling: one type, one home).
///
/// It lives HERE, not in `claim_provenance`, because `index_lifecycle` compiles
/// under the embed feature while `protocol` does not: the runtime holding an
/// `Arc<GenerationAuthority>` inside `VerifiedGeneration` must reach it without
/// a lifecycle→protocol edge. `claim_provenance` RE-EXPORTS it, so every
/// existing oracle path keeps resolving, and its provenance-specific methods
/// stay in `claim_provenance` as a same-crate inherent impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationAuthority {
    identity: AuthorityIdentity,
    generation: GenerationIdentity,
    physical_root: String,
}

impl GenerationAuthority {
    /// Sealed capture constructor: crate-only, used by the provenance lease
    /// and the dark runtime. Nothing outside the crate can mint one.
    pub(crate) fn captured(
        identity: AuthorityIdentity,
        generation: GenerationIdentity,
        physical_root: String,
    ) -> Self {
        Self {
            identity,
            generation,
            physical_root,
        }
    }

    pub fn identity(&self) -> AuthorityIdentity {
        self.identity
    }

    pub fn generation(&self) -> GenerationIdentity {
        self.generation
    }

    pub fn physical_root(&self) -> &str {
        &self.physical_root
    }

    /// A generation miss covers ONE captured generation, not the repository.
    pub fn proves_generation_absence(&self) -> bool {
        true
    }

    pub fn proves_repository_absence(&self) -> bool {
        false
    }
}

/// A monotonic observation instant.
///
/// Deliberately NOT a wall clock. Absence proofs are scoped to "the instant this
/// was observed", and a wall clock can repeat, jump backwards across a
/// resync, or tie between two observations. A monotonic counter cannot, so
/// `observed_at` orders and compares exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObservationTime(NonZeroU64);

impl ObservationTime {
    /// Take a fresh observation instant.
    pub fn fresh() -> Self {
        Self(next_identity())
    }
}

/// Monotonic epoch of a filesystem observer registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObserverEpoch(u64);

impl ObserverEpoch {
    /// The epoch an observer occupies before it has seen any invalidation.
    pub const fn initial() -> Self {
        Self(0)
    }

    /// The next epoch. Never rewinds.
    pub const fn advanced(self) -> Self {
        Self(self.0 + 1)
    }

    /// Diagnostic value. Callers must not derive permission from it.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Position in an observer's invalidation stream. Seals the interval a
/// worktree-scope observation may speak for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvalidationSequence(u64);

impl InvalidationSequence {
    /// The sequence before any invalidation has been recorded.
    pub const fn initial() -> Self {
        Self(0)
    }

    /// The next position. Never rewinds.
    pub const fn advanced(self) -> Self {
        Self(self.0 + 1)
    }

    /// Diagnostic value. Callers must not derive permission from it.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_counter_serves_every_identity_kind() {
        // The property this module exists for: identities minted through
        // DIFFERENT newtypes still come from one space, so their raw values
        // never collide. A second counter under `protocol` would break this
        // silently, which is why the macro is not duplicated there.
        let a = GenerationIdentity::fresh().0;
        let b = AuthorityIdentity::fresh().0;
        let c = OperationIdentity::fresh().0;
        let d = ObservationTime::fresh().0;

        let mut raws = vec![a, b, c, d];
        raws.sort_unstable();
        raws.dedup();
        assert_eq!(raws.len(), 4, "one shared counter must not repeat a value");
    }

    #[test]
    fn a_fresh_identity_never_repeats_within_its_own_kind() {
        let first = GenerationIdentity::fresh();
        let second = GenerationIdentity::fresh();
        assert_ne!(first, second);
    }

    #[test]
    fn monotonic_positions_never_rewind() {
        let epoch = ObserverEpoch::initial();
        assert!(epoch.advanced() > epoch);
        let sequence = InvalidationSequence::initial();
        assert!(sequence.advanced() > sequence);
    }
}
