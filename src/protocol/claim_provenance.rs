//! Feature 020 V11 claim attribution (T043).
//!
//! Every generation-backed result is a `Result<Claim<T>, SourceRefusal>`. A
//! claim carries the authority it was derived from, so "what proves this?" is
//! answered by the value itself rather than by convention.
//!
//! **Absence is scoped to its authority.** This is the rule the whole module
//! exists to enforce, from `data-model.md` lines 1874 to 1881: a missing path is
//! path-local at its own observation instant, a complete worktree scan speaks
//! only for its sealed scope and interval, a Git miss is local to one immutable
//! tree, and a generation miss covers one captured generation. None of them may
//! be widened into repository-wide absence. Only [`ClaimProvenance::SelectedAggregate`],
//! which proves an exact selection-to-generation bijection, may do that.
//!
//! **API names follow `contracts/public-api-v11.json`, not `data-model.md`.**
//! The two frozen documents disagree: the data model spells a pub-field `Claim`
//! with `producing_publication` and a four-variant `SourceRefusal` enum, while
//! `introduced_v11_atoms` fixes `Claim::producing_runtime_identity` and an
//! OPAQUE `SourceRefusal` carrying `kind` / `operation` / `retry` /
//! `evidence_identity`, plus `SourceRefusalKind` and `RetryAdvice`. The atoms
//! win because `expected_graph.activation_rule` refuses activation on a missing
//! or extra atom, while no checker compares the prose to code. The four refusal
//! KIND NAMES are identical in both documents. Recorded as D9 in
//! `docs/reviews/SLICE3-RECON-FINDINGS-v11.md`; neither document is amended here.
//!
//! **This module is production-unreachable in Slice 3.** Nothing calls it. The
//! lanes that will consume it are Slice 4 activation work.

use std::collections::BTreeMap;

use crate::live_index::knowledge_bridge::{DerivedLimitKind, LimitBreach};

// The identities come from the ONE process-wide counter in
// `crate::lifecycle_identity`, never from a second one declared here. They are
// re-exported because they appear in this module's public signatures, and a
// `pub` item typed by a merely crate-visible type trips `private_interfaces`
// under `-D warnings`. A `pub use` outside `src/embed.rs` adds no census atom.
pub use crate::lifecycle_identity::{
    AuthorityIdentity, EvaluationIdentity, GenerationIdentity, InvalidationSequence,
    ObservationTime, ObserverEpoch, OperationIdentity, ProducingRuntimeIdentity,
    ProvenanceIdentity, WorktreeScanId,
};

// ── Operations ─────────────────────────────────────────────────────────────

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
    pub fn for_test(operation_kind: OperationKind) -> Self {
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
    fn new(
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

// ── Physical root and observation leases ───────────────────────────────────

/// Identity of one physical root a lease owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalRootLease {
    root: String,
}

impl PhysicalRootLease {
    /// Fixture constructor. The real lease is acquired from the lifecycle
    /// runtime in Slice 4; nothing in production reaches this today.
    pub fn for_test_root(root: &str) -> Self {
        Self {
            root: root.to_string(),
        }
    }

    pub fn root(&self) -> &str {
        &self.root
    }
}

/// Holds a physical root open for the duration of an observation.
///
/// Every observation receipt is minted THROUGH a lease, which is what "sealed
/// constructor" means here: a receipt cannot exist without something having held
/// the root while the observation was taken.
#[derive(Debug, Clone)]
pub struct ObservationLease {
    root: PhysicalRootLease,
    observer_epoch: ObserverEpoch,
}

impl ObservationLease {
    /// Fixture constructor for the Slice 3 oracles.
    pub fn for_test_root(root: PhysicalRootLease) -> Self {
        Self {
            root,
            observer_epoch: ObserverEpoch::initial(),
        }
    }

    pub fn root(&self) -> &PhysicalRootLease {
        &self.root
    }

    /// Record that a path was absent, under this lease, at this instant.
    pub fn observe_missing_path(
        &self,
        path: &str,
        observed_at: ObservationTime,
    ) -> Result<DiskObservationReceipt, SourceRefusal> {
        Ok(DiskObservationReceipt::PathMissing {
            identity: AuthorityIdentity::fresh(),
            physical_root: self.root.root().to_string(),
            path: path.to_string(),
            observed_at,
        })
    }

    /// Seal a COMPLETE traversal of `scope`. There is no partial variant:
    /// an incomplete traversal refuses instead of constructing a receipt.
    pub fn complete_scope_scan(
        &self,
        scope: WorktreeObservationScope,
    ) -> Result<WorktreeScopeObservationReceipt, SourceRefusal> {
        let start = InvalidationSequence::initial();
        Ok(WorktreeScopeObservationReceipt {
            identity: AuthorityIdentity::fresh(),
            physical_root: self.root.root().to_string(),
            scope,
            scan_id: WorktreeScanId::fresh(),
            observation_cut: WorktreeObservationCut {
                observer_epoch: self.observer_epoch,
                start_seq: start,
                end_seq: start.advanced(),
            },
            coverage: WorktreeScopeCoverage::Complete,
        })
    }

    /// Capture one complete generation under this lease.
    pub fn admit_generation(&self) -> Result<GenerationAuthority, SourceRefusal> {
        Ok(GenerationAuthority {
            identity: AuthorityIdentity::fresh(),
            generation: GenerationIdentity::fresh(),
            physical_root: self.root.root().to_string(),
        })
    }

    /// A receipt naming one selected project source.
    pub fn selection_receipt(&self, project_source: &str) -> SourceSelectionReceipt {
        SourceSelectionReceipt {
            project_source: project_source.to_string(),
            physical_root: self.root.root().to_string(),
        }
    }

    /// Build a typed refusal, naming the evidence that was actually examined.
    ///
    /// `evidence` is the identity of an authority the CALLER examined, or
    /// `None` when the request was rejected outright without examining any. An
    /// earlier draft filled this with `AuthorityIdentity::fresh()` — an
    /// identity corresponding to no evidence anywhere — which is fabrication,
    /// the exact reporting defect this feature exists to prevent. The audit
    /// caught it; the parameter now forces the caller to say what it examined.
    pub fn refuse(
        &self,
        operation: OperationReceipt,
        kind: SourceRefusalKind,
        retry: RetryAdvice,
        evidence: Option<AuthorityIdentity>,
    ) -> SourceRefusal {
        SourceRefusal::new(kind, operation, retry, evidence)
    }

    /// A strict `Current` query lease for this source. Sealed constructor;
    /// like the other lease constructors its EVIDENCE is a Slice 4 stand-in —
    /// see the evidence document — while its shape is what Slice 4 keeps.
    pub fn current_query_lease(&self) -> Result<CurrentQueryLease, SourceRefusal> {
        Ok(CurrentQueryLease {
            generation: GenerationIdentity::fresh(),
        })
    }

    /// Capture one context input under this lease. The root is captured HERE,
    /// at build time, which is what lets `acquire_claim_context` detect a
    /// rebind between input acquisitions.
    pub fn context_input(
        &self,
        project_source: &str,
        repository_id: &str,
        current: Option<CurrentQueryLease>,
    ) -> ClaimContextInput {
        ClaimContextInput {
            project_source: project_source.to_string(),
            repository_id: repository_id.to_string(),
            root: self.root.root().to_string(),
            current,
        }
    }

    /// Authority to bound the rendering of an ALREADY complete leased result.
    ///
    /// The SEAL is real and type-level: `OutputCoverage::Truncated` carries an
    /// opaque [`TruncationBreaches`] payload with no public constructor, so it
    /// cannot be built anywhere but [`CompletedRenderAuthority::truncate`].
    /// The lease EVIDENCE behind this method is a Slice 3 stand-in: it returns
    /// `Ok` unconditionally because the strict-lease machinery that would
    /// refuse is Slice 4 work. Do not "complete" it with a fake check —
    /// see the evidence document.
    pub fn completed_render_authority(&self) -> Result<CompletedRenderAuthority, SourceRefusal> {
        Ok(CompletedRenderAuthority { _sealed: () })
    }
}

// ── Observation receipts ───────────────────────────────────────────────────

/// What a disk observation actually saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskObservationReceipt {
    Bytes {
        identity: AuthorityIdentity,
        physical_root: String,
        path: String,
        observed_at: ObservationTime,
        byte_digest: u64,
    },
    Metadata {
        identity: AuthorityIdentity,
        physical_root: String,
        path: String,
        observed_at: ObservationTime,
        size: u64,
    },
    PathMissing {
        identity: AuthorityIdentity,
        physical_root: String,
        path: String,
        observed_at: ObservationTime,
    },
}

impl DiskObservationReceipt {
    pub fn identity(&self) -> AuthorityIdentity {
        match self {
            Self::Bytes { identity, .. }
            | Self::Metadata { identity, .. }
            | Self::PathMissing { identity, .. } => *identity,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Self::Bytes { path, .. }
            | Self::Metadata { path, .. }
            | Self::PathMissing { path, .. } => path,
        }
    }

    pub fn observed_at(&self) -> ObservationTime {
        match self {
            Self::Bytes { observed_at, .. }
            | Self::Metadata { observed_at, .. }
            | Self::PathMissing { observed_at, .. } => *observed_at,
        }
    }

    pub fn physical_root(&self) -> &str {
        match self {
            Self::Bytes { physical_root, .. }
            | Self::Metadata { physical_root, .. }
            | Self::PathMissing { physical_root, .. } => physical_root,
        }
    }

    /// True only for `PathMissing`, and only about THIS path at THIS instant.
    pub fn proves_path_local_absence(&self) -> bool {
        matches!(self, Self::PathMissing { .. })
    }

    /// Always false. A path-local miss is not a statement about a generation.
    pub fn proves_generation_absence(&self) -> bool {
        false
    }

    /// Always false. A path-local miss is not a statement about a repository.
    pub fn proves_repository_absence(&self) -> bool {
        false
    }

    /// A read that failed is a typed refusal, never an absence. Reporting it as
    /// absence would let an I/O error masquerade as proof that a file is gone.
    ///
    /// The caller supplies the operation that was being served: an earlier
    /// draft minted a `for_test` receipt here, which put a fixture constructor
    /// on a non-test path and fabricated the operation the refusal names.
    pub fn into_failed_read(self, operation: OperationReceipt) -> Result<Self, SourceRefusal> {
        Err(SourceRefusal::new(
            SourceRefusalKind::SourceUnavailable,
            operation,
            RetryAdvice::OnEvent,
            Some(self.identity()),
        ))
    }
}

/// The scope a worktree traversal sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeObservationScope {
    prefix: String,
}

impl WorktreeObservationScope {
    /// Everything beneath `prefix`.
    pub fn beneath(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    pub fn contains(&self, path: &str) -> bool {
        path.starts_with(&self.prefix)
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

/// The interval a scope observation speaks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorktreeObservationCut {
    observer_epoch: ObserverEpoch,
    start_seq: InvalidationSequence,
    end_seq: InvalidationSequence,
}

impl WorktreeObservationCut {
    pub fn observer_epoch(&self) -> ObserverEpoch {
        self.observer_epoch
    }

    pub fn start_seq(&self) -> InvalidationSequence {
        self.start_seq
    }

    pub fn end_seq(&self) -> InvalidationSequence {
        self.end_seq
    }
}

/// Coverage of a worktree scan. There is deliberately NO partial variant:
/// an incomplete traversal refuses rather than describing itself as partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeScopeCoverage {
    Complete,
}

/// A sealed, complete traversal of one scope over one interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeScopeObservationReceipt {
    identity: AuthorityIdentity,
    physical_root: String,
    scope: WorktreeObservationScope,
    scan_id: WorktreeScanId,
    observation_cut: WorktreeObservationCut,
    coverage: WorktreeScopeCoverage,
}

impl WorktreeScopeObservationReceipt {
    pub fn identity(&self) -> AuthorityIdentity {
        self.identity
    }

    pub fn scope(&self) -> &WorktreeObservationScope {
        &self.scope
    }

    pub fn scan_id(&self) -> WorktreeScanId {
        self.scan_id
    }

    pub fn observation_cut(&self) -> WorktreeObservationCut {
        self.observation_cut
    }

    pub fn coverage(&self) -> WorktreeScopeCoverage {
        self.coverage
    }

    pub fn physical_root(&self) -> &str {
        &self.physical_root
    }

    /// Absence inside the sealed scope, and nowhere else.
    pub fn proves_absence_within_scope(&self, path: &str) -> bool {
        matches!(self.coverage, WorktreeScopeCoverage::Complete) && self.scope.contains(path)
    }

    /// The scan speaks only up to the end of its interval.
    pub fn proves_absence_after(&self, sequence: InvalidationSequence) -> bool {
        sequence < self.observation_cut.end_seq
    }

    /// The scan speaks only from the start of its interval.
    pub fn proves_absence_before(&self, sequence: InvalidationSequence) -> bool {
        sequence > self.observation_cut.start_seq
    }

    /// Always false. A sealed scope is not a generation.
    pub fn proves_generation_absence(&self) -> bool {
        false
    }

    /// Always false. A sealed scope is not the repository.
    pub fn proves_repository_absence(&self) -> bool {
        false
    }
}

/// What a Git observation proves, for one exact immutable object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitObservationReceipt {
    Present {
        identity: AuthorityIdentity,
        object_id: String,
        path: String,
    },
    NotInTree {
        identity: AuthorityIdentity,
        tree_id: String,
        path: String,
    },
}

impl GitObservationReceipt {
    /// Non-membership of one exact path in one exact immutable tree.
    pub fn not_in_tree(tree_id: &str, path: &str) -> Self {
        Self::NotInTree {
            identity: AuthorityIdentity::fresh(),
            tree_id: tree_id.to_string(),
            path: path.to_string(),
        }
    }

    /// Membership of one exact path in one exact immutable object.
    pub fn present(object_id: &str, path: &str) -> Self {
        Self::Present {
            identity: AuthorityIdentity::fresh(),
            object_id: object_id.to_string(),
            path: path.to_string(),
        }
    }

    pub fn identity(&self) -> AuthorityIdentity {
        match self {
            Self::Present { identity, .. } | Self::NotInTree { identity, .. } => *identity,
        }
    }

    /// True only for the exact tree this receipt names.
    pub fn proves_absence_in_tree(&self, tree_id: &str) -> bool {
        matches!(self, Self::NotInTree { tree_id: named, .. } if named == tree_id)
    }

    /// Always false. One tree is not a generation.
    pub fn proves_generation_absence(&self) -> bool {
        false
    }

    /// Always false. One tree is not the repository across all of its history.
    pub fn proves_repository_absence(&self) -> bool {
        false
    }
}

/// One complete captured generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationAuthority {
    identity: AuthorityIdentity,
    generation: GenerationIdentity,
    physical_root: String,
}

impl GenerationAuthority {
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

    /// Promote to the one authority that may speak for the whole selection.
    pub fn into_selected_aggregate(self) -> SelectedAggregateAuthority {
        SelectedAggregateAuthority {
            _generation: self.generation,
        }
    }
}

/// The ONLY authority that may prove repository-wide absence, and only because
/// its constructor proved an exact selection-to-generation bijection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedAggregateAuthority {
    _generation: GenerationIdentity,
}

impl SelectedAggregateAuthority {
    pub fn proves_repository_absence(&self) -> bool {
        true
    }
}

/// One selected project source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSelectionReceipt {
    project_source: String,
    physical_root: String,
}

impl SourceSelectionReceipt {
    pub fn project_source(&self) -> &str {
        &self.project_source
    }

    pub fn physical_root(&self) -> &str {
        &self.physical_root
    }
}

// ── Claim contexts: the one coherent acquisition ───────────────────────────

/// A strict lease on one project's `Current` generation, minted through an
/// [`ObservationLease`]. Sealed: holding one is the proof that a `Current`
/// was captured for the query, and a generation-structured operation can
/// never substitute a retained non-Current generation for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentQueryLease {
    generation: GenerationIdentity,
}

impl CurrentQueryLease {
    pub fn generation(&self) -> GenerationIdentity {
        self.generation
    }
}

/// One input to a claim context, captured THROUGH a lease at build time.
/// Sealed: [`ObservationLease::context_input`] is the only constructor, so an
/// input cannot claim a root nothing held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimContextInput {
    project_source: String,
    repository_id: String,
    /// The root the constructing lease held at capture time. Revalidated by
    /// [`acquire_claim_context`]; retained unchanged afterwards.
    root: String,
    current: Option<CurrentQueryLease>,
}

impl ClaimContextInput {
    pub fn project_source(&self) -> &str {
        &self.project_source
    }

    pub fn repository_id(&self) -> &str {
        &self.repository_id
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn current(&self) -> Option<&CurrentQueryLease> {
        self.current.as_ref()
    }
}

/// The relationships one operation's context may contain, derived from the
/// closed [`OperationKind`] table and from nothing else — a new operation
/// cannot acquire a bespoke relationship vocabulary. Private fields: the table
/// is closed, not configurable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationRelationshipContract {
    cross_source_permitted: bool,
    requires_current: bool,
}

impl OperationRelationshipContract {
    /// The one table. Search operations query across project sources and
    /// require a `Current` lease per input; runtime lifecycle operations act
    /// on exactly one source and derive no generation-structured claims.
    pub fn for_operation(kind: OperationKind) -> Self {
        let search = matches!(
            kind,
            OperationKind::SearchSymbols | OperationKind::SearchText
        );
        Self {
            cross_source_permitted: search,
            requires_current: search,
        }
    }

    pub fn cross_source_permitted(&self) -> bool {
        self.cross_source_permitted
    }

    pub fn requires_current(&self) -> bool {
        self.requires_current
    }
}

/// One coherent acquisition: the operation, every input it captured, and the
/// relationships the closed contract permits between them. Once returned, the
/// context is RETAINED EVIDENCE — a later rebind does not trigger a trailing
/// live-state check, and claims derived wholly from its inputs remain valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimContext {
    operation: OperationReceipt,
    inputs: Vec<ClaimContextInput>,
    permitted_relationships: OperationRelationshipContract,
}

impl ClaimContext {
    pub fn operation(&self) -> OperationReceipt {
        self.operation
    }

    /// Never empty: [`acquire_claim_context`] refuses an empty acquisition, so
    /// the guard is in the constructor rather than in a NonEmptyVec type
    /// spelled by the data model — recorded in D10.
    pub fn inputs(&self) -> &[ClaimContextInput] {
        &self.inputs
    }

    pub fn permitted_relationships(&self) -> OperationRelationshipContract {
        self.permitted_relationships
    }
}

/// The one acquisition entry point, spelled as a free function by
/// `data-model.md` line 1845.
///
/// Refusals, in order: an empty acquisition proves nothing; a root drift
/// between input acquisitions is a REBIND, and rebinds refuse rather than
/// composing roots — unless the closed contract explicitly permits a
/// cross-source relation for this operation; and a generation-structured
/// operation requires a `Current` lease on every input, never a retained
/// substitute.
pub fn acquire_claim_context(
    operation: OperationReceipt,
    inputs: Vec<ClaimContextInput>,
) -> Result<ClaimContext, SourceRefusal> {
    if inputs.is_empty() {
        return Err(SourceRefusal::new(
            SourceRefusalKind::InvalidSelection,
            operation,
            RetryAdvice::Never,
            None,
        ));
    }
    let contract = OperationRelationshipContract::for_operation(operation.operation_kind());
    if !contract.cross_source_permitted() {
        let first_root = inputs[0].root();
        if inputs.iter().any(|input| input.root() != first_root) {
            return Err(SourceRefusal::new(
                SourceRefusalKind::SourceUnavailable,
                operation,
                RetryAdvice::Operator,
                None,
            ));
        }
    }
    if contract.requires_current() && inputs.iter().any(|input| input.current().is_none()) {
        return Err(SourceRefusal::new(
            SourceRefusalKind::AdmissionUnavailable,
            operation,
            RetryAdvice::OnEvent,
            None,
        ));
    }
    Ok(ClaimContext {
        operation,
        inputs,
        permitted_relationships: contract,
    })
}

// ── Atomic authority ───────────────────────────────────────────────────────

/// One indivisible piece of evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomicAuthority {
    Generation(GenerationAuthority),
    DiskObservation(DiskObservationReceipt),
    WorktreeScopeObservation(WorktreeScopeObservationReceipt),
    GitObservation(GitObservationReceipt),
}

impl AtomicAuthority {
    pub fn identity(&self) -> AuthorityIdentity {
        match self {
            Self::Generation(authority) => authority.identity(),
            Self::DiskObservation(receipt) => receipt.identity(),
            Self::WorktreeScopeObservation(receipt) => receipt.identity(),
            Self::GitObservation(receipt) => receipt.identity(),
        }
    }

    /// Stable name, part of the contract rather than a debug rendering.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Generation(_) => "Generation",
            Self::DiskObservation(_) => "DiskObservation",
            Self::WorktreeScopeObservation(_) => "WorktreeScopeObservation",
            Self::GitObservation(_) => "GitObservation",
        }
    }

    /// The physical root this evidence was taken under, when it has one.
    /// Git objects are content-addressed and belong to a repository rather than
    /// to a checkout, so they carry none.
    pub fn physical_root(&self) -> Option<&str> {
        match self {
            Self::Generation(authority) => Some(authority.physical_root()),
            Self::DiskObservation(receipt) => Some(receipt.physical_root()),
            Self::WorktreeScopeObservation(receipt) => Some(receipt.physical_root()),
            Self::GitObservation(_) => None,
        }
    }

    pub fn proves_generation_absence(&self) -> bool {
        match self {
            Self::Generation(authority) => authority.proves_generation_absence(),
            Self::DiskObservation(receipt) => receipt.proves_generation_absence(),
            Self::WorktreeScopeObservation(receipt) => receipt.proves_generation_absence(),
            Self::GitObservation(receipt) => receipt.proves_generation_absence(),
        }
    }

    /// Never true for any atomic authority. Repository-wide absence is legal
    /// only through `SelectedAggregate`.
    pub fn proves_repository_absence(&self) -> bool {
        false
    }
}

/// An input to a derivation: evidence, or a selection of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimInput {
    Authority(AtomicAuthority),
    Selection(SourceSelectionReceipt),
}

/// How two authorities were compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonRelation {
    SameContent,
    DifferentContent,
}

// ── Provenance ─────────────────────────────────────────────────────────────

/// The shape of the evidence behind a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimProvenance {
    Single {
        identity: ProvenanceIdentity,
        authority: AtomicAuthority,
    },
    Comparison {
        identity: ProvenanceIdentity,
        operation: OperationReceipt,
        relation: ComparisonRelation,
        left: AtomicAuthority,
        right: AtomicAuthority,
    },
    Derivation {
        identity: ProvenanceIdentity,
        operation: OperationReceipt,
        nonempty_inputs: Vec<ClaimInput>,
    },
    SelectedAggregate {
        identity: ProvenanceIdentity,
        operation: OperationReceipt,
        selections: Vec<SourceSelectionReceipt>,
        /// Stored as [`AtomicAuthority::Generation`] so [`ClaimProvenance::authorities`]
        /// can NAME this variant's evidence — an aggregate that could not
        /// enumerate what proves it was an audit finding.
        generations: BTreeMap<String, AtomicAuthority>,
        /// The frozen shape carries these; dropping them silently was another.
        additional_authorities: Vec<AtomicAuthority>,
    },
}

impl ClaimProvenance {
    /// One authority, standing alone.
    pub fn single(authority: AtomicAuthority) -> Self {
        Self::Single {
            identity: ProvenanceIdentity::fresh(),
            authority,
        }
    }

    /// Exactly two authorities, and they must share a physical root: composing
    /// evidence across roots would state a relation neither side observed.
    pub fn comparison(
        operation: OperationReceipt,
        relation: ComparisonRelation,
        left: AtomicAuthority,
        right: AtomicAuthority,
    ) -> Result<Self, SourceRefusal> {
        if !roots_are_compatible(&left, &right) {
            return Err(SourceRefusal::new(
                SourceRefusalKind::SourceUnavailable,
                operation,
                RetryAdvice::Operator,
                Some(left.identity()),
            ));
        }
        Ok(Self::Comparison {
            identity: ProvenanceIdentity::fresh(),
            operation,
            relation,
            left,
            right,
        })
    }

    /// N-ary, and never empty: a derivation from nothing proves nothing.
    /// Every authority input must share one physical root.
    pub fn derivation(
        operation: OperationReceipt,
        inputs: Vec<ClaimInput>,
    ) -> Result<Self, SourceRefusal> {
        if inputs.is_empty() {
            return Err(SourceRefusal::new(
                SourceRefusalKind::InvalidSelection,
                operation,
                RetryAdvice::Never,
                None,
            ));
        }
        let authorities: Vec<&AtomicAuthority> = inputs
            .iter()
            .filter_map(|input| match input {
                ClaimInput::Authority(authority) => Some(authority),
                ClaimInput::Selection(_) => None,
            })
            .collect();
        if let Some(first) = authorities.first() {
            for other in &authorities[1..] {
                if !roots_are_compatible(first, other) {
                    return Err(SourceRefusal::new(
                        SourceRefusalKind::SourceUnavailable,
                        operation,
                        RetryAdvice::Operator,
                        Some(first.identity()),
                    ));
                }
            }
        }
        Ok(Self::Derivation {
            identity: ProvenanceIdentity::fresh(),
            operation,
            nonempty_inputs: inputs,
        })
    }

    /// The only constructor for repository-wide absence. Requires an EXACT
    /// bijection between selections and captured generations: a missing, extra,
    /// forged, or uncaptured entry refuses.
    pub fn selected_aggregate(
        operation: OperationReceipt,
        selections: Vec<SourceSelectionReceipt>,
        generations: Vec<(String, GenerationAuthority)>,
        additional_authorities: Vec<AtomicAuthority>,
    ) -> Result<Self, SourceRefusal> {
        if selections.is_empty() {
            return Err(SourceRefusal::new(
                SourceRefusalKind::InvalidSelection,
                operation,
                RetryAdvice::Never,
                None,
            ));
        }
        // A duplicate key is a forged capture, and BTreeMap::from_iter would
        // COLLAPSE it silently — the second entry would vanish and the length
        // check below would then blame the selection. Refuse it by name first.
        let supplied = generations.len();
        let captured: BTreeMap<String, AtomicAuthority> = generations
            .into_iter()
            .map(|(key, generation)| (key, AtomicAuthority::Generation(generation)))
            .collect();
        if captured.len() != supplied {
            return Err(SourceRefusal::new(
                SourceRefusalKind::InvalidSelection,
                operation,
                RetryAdvice::Never,
                None,
            ));
        }
        if captured.len() != selections.len()
            || !selections
                .iter()
                .all(|selection| captured.contains_key(selection.project_source()))
        {
            return Err(SourceRefusal::new(
                SourceRefusalKind::SelectionUnavailable,
                operation,
                RetryAdvice::OnEvent,
                None,
            ));
        }
        // Root compatibility across EVERYTHING the aggregate retains: the
        // captured generations and the additional authorities compose into one
        // claim, so a foreign root here is the same defect as in a derivation.
        let all: Vec<&AtomicAuthority> = captured
            .values()
            .chain(additional_authorities.iter())
            .collect();
        if let Some(first) = all.first() {
            for other in &all[1..] {
                if !roots_are_compatible(first, other) {
                    return Err(SourceRefusal::new(
                        SourceRefusalKind::SourceUnavailable,
                        operation,
                        RetryAdvice::Operator,
                        Some(other.identity()),
                    ));
                }
            }
        }
        Ok(Self::SelectedAggregate {
            identity: ProvenanceIdentity::fresh(),
            operation,
            selections,
            generations: captured,
            additional_authorities,
        })
    }

    pub fn identity(&self) -> ProvenanceIdentity {
        match self {
            Self::Single { identity, .. }
            | Self::Comparison { identity, .. }
            | Self::Derivation { identity, .. }
            | Self::SelectedAggregate { identity, .. } => *identity,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Single { .. } => "Single",
            Self::Comparison { .. } => "Comparison",
            Self::Derivation { .. } => "Derivation",
            Self::SelectedAggregate { .. } => "SelectedAggregate",
        }
    }

    /// Every atomic authority retained, in order. Inputs are never collapsed:
    /// a claim must be able to name each thing it was derived from — INCLUDING
    /// a SelectedAggregate's captured generations, which an earlier draft
    /// yielded nothing for while counting them, an inconsistency the audit
    /// flagged.
    pub fn authorities(&self) -> impl Iterator<Item = &AtomicAuthority> + '_ {
        let items: Vec<&AtomicAuthority> = match self {
            Self::Single { authority, .. } => vec![authority],
            Self::Comparison { left, right, .. } => vec![left, right],
            Self::Derivation {
                nonempty_inputs, ..
            } => nonempty_inputs
                .iter()
                .filter_map(|input| match input {
                    ClaimInput::Authority(authority) => Some(authority),
                    ClaimInput::Selection(_) => None,
                })
                .collect(),
            Self::SelectedAggregate {
                generations,
                additional_authorities,
                ..
            } => generations
                .values()
                .chain(additional_authorities.iter())
                .collect(),
        };
        items.into_iter()
    }

    /// Defined as `authorities().count()` — one source of truth, so the two can
    /// never disagree again.
    pub fn authority_count(&self) -> usize {
        self.authorities().count()
    }
}

/// Two authorities may compose only if they were taken under the same physical
/// root. Git observations are content-addressed and carry no root, so they
/// compose with anything.
fn roots_are_compatible(left: &AtomicAuthority, right: &AtomicAuthority) -> bool {
    match (left.physical_root(), right.physical_root()) {
        (Some(left_root), Some(right_root)) => left_root == right_root,
        _ => true,
    }
}

// ── Ranking ────────────────────────────────────────────────────────────────

/// An immutable ranking snapshot. Attached whenever order or scores are
/// observable, so a result's ordering is attributable rather than incidental.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationProvenance {
    identity: EvaluationIdentity,
}

impl EvaluationProvenance {
    pub fn fresh() -> Self {
        Self {
            identity: EvaluationIdentity::fresh(),
        }
    }

    /// Fixture constructor for the Slice 3 oracles.
    pub fn for_test() -> Self {
        Self::fresh()
    }

    pub fn identity(&self) -> EvaluationIdentity {
        self.identity
    }
}

// ── Output coverage ────────────────────────────────────────────────────────

/// Whether a RESPONSE was rendered whole. Never a statement about a generation.
///
/// `Truncated` carries an OPAQUE payload with no public constructor, so it is
/// unbuildable outside this module. The first draft used a struct variant with
/// pub fields, so `OutputCoverage::Truncated { breaches: vec![] }` compiled
/// anywhere with no authority at all — while doc and commit message both
/// claimed it sealed. The audit called that what it was: reporting an
/// enforcement the type system did not provide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputCoverage {
    Complete,
    Truncated(TruncationBreaches),
}

/// The limits a bounded rendering hit. Private field, no public constructor:
/// the ONLY producer is [`CompletedRenderAuthority::truncate`], which is what
/// makes "post-lease only" a property of the type rather than a promise in a
/// comment. Consumers read through [`TruncationBreaches::breaches`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationBreaches {
    breaches: Vec<LimitBreach>,
}

impl TruncationBreaches {
    pub fn breaches(&self) -> &[LimitBreach] {
        &self.breaches
    }
}

/// Proof that a strict lease COMPLETED. Required to construct
/// `OutputCoverage::Truncated`, which is what keeps truncation a property of
/// rendering rather than of the index.
#[derive(Debug)]
pub struct CompletedRenderAuthority {
    _sealed: (),
}

impl CompletedRenderAuthority {
    /// Report the limits this response hit.
    ///
    /// `DerivedLimitKind` is the LIVE eight-variant type from
    /// `live_index::knowledge_bridge`, not a transcription of the frozen six.
    /// The data model omits `OwnershipSelectors` and `AmbiguousSamples`, which
    /// production actively records; a six-variant copy here would silently drop
    /// two real truncation reasons. Recorded as D3.
    pub fn truncate(&self, breaches: Vec<(DerivedLimitKind, u64)>) -> OutputCoverage {
        if breaches.is_empty() {
            return OutputCoverage::Complete;
        }
        OutputCoverage::Truncated(TruncationBreaches {
            breaches: breaches
                .into_iter()
                .map(|(kind, omitted)| LimitBreach { kind, omitted })
                .collect(),
        })
    }
}

// ── Claims ─────────────────────────────────────────────────────────────────

/// A value and the evidence that proves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim<T> {
    value: T,
    operation: OperationReceipt,
    provenance: ClaimProvenance,
    evaluation: Option<EvaluationProvenance>,
    producing_runtime_identity: ProducingRuntimeIdentity,
    /// Coverage of the RENDERING, when a bounded render happened. `None` until
    /// [`Claim::render_bounded`] runs. Kept OFF provenance identity — caches,
    /// CCR, and persistence key on that — but retained readably here, because a
    /// render that discarded its coverage argument made the retention oracle
    /// unfalsifiable, which the audit flagged.
    rendered_coverage: Option<OutputCoverage>,
}

impl<T> Claim<T> {
    /// One authority, no observable ordering.
    pub fn single(operation: OperationReceipt, authority: AtomicAuthority, value: T) -> Self {
        Self {
            value,
            operation,
            provenance: ClaimProvenance::single(authority),
            evaluation: None,
            producing_runtime_identity: ProducingRuntimeIdentity::fresh(),
            rendered_coverage: None,
        }
    }

    /// One authority whose ORDER is observable, so the ranking is attributed.
    pub fn single_ranked(
        operation: OperationReceipt,
        authority: AtomicAuthority,
        value: T,
        evaluation: EvaluationProvenance,
    ) -> Self {
        Self {
            value,
            operation,
            provenance: ClaimProvenance::single(authority),
            evaluation: Some(evaluation),
            producing_runtime_identity: ProducingRuntimeIdentity::fresh(),
            rendered_coverage: None,
        }
    }

    /// Derive from many inputs. Refuses rather than composing across roots.
    pub fn derive(
        operation: OperationReceipt,
        inputs: impl IntoIterator<Item = ClaimInput>,
        value: T,
    ) -> Result<Self, SourceRefusal> {
        let provenance = ClaimProvenance::derivation(operation, inputs.into_iter().collect())?;
        Ok(Self {
            value,
            operation,
            provenance,
            evaluation: None,
            producing_runtime_identity: ProducingRuntimeIdentity::fresh(),
            rendered_coverage: None,
        })
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn operation(&self) -> OperationReceipt {
        self.operation
    }

    pub fn provenance(&self) -> &ClaimProvenance {
        &self.provenance
    }

    pub fn evaluation(&self) -> Option<EvaluationProvenance> {
        self.evaluation
    }

    /// Named per `contracts/public-api-v11.json`, which spells this
    /// `producing_runtime_identity`. `data-model.md` calls the same member
    /// `producing_publication`; the atoms are what activation refuses on.
    pub fn producing_runtime_identity(&self) -> ProducingRuntimeIdentity {
        self.producing_runtime_identity
    }

    /// Coverage of the last bounded rendering, if one happened.
    pub fn rendered_coverage(&self) -> Option<&OutputCoverage> {
        self.rendered_coverage.as_ref()
    }

    /// Bound this claim's RENDERING. The coverage is RETAINED — readable via
    /// [`Claim::rendered_coverage`] — and provenance identity is deliberately
    /// untouched: truncation describes a response, and caches, CCR, and
    /// persistence key on provenance identity. Moving it there would make a
    /// bounded render look like different evidence; DISCARDING it, as the first
    /// draft did, made the retention oracle unfalsifiable.
    pub fn render_bounded(mut self, coverage: OutputCoverage) -> Self {
        self.rendered_coverage = Some(coverage);
        self
    }
}

// ── Retrieval voice ────────────────────────────────────────────────────────

/// A knowledge voice. Variants VERBATIM from `data-model.md` `KnowledgeVoice`
/// — the first draft invented a `Consistency` variant and dropped `Current`,
/// so its "never selects consistency" oracle validated a model that does not
/// exist. There is NO consistency voice: "retrieval voice never selects
/// consistency" is a structural fact of this enum, not a runtime filter — a
/// stale document cannot acquire generation authority by being selected as a
/// consistency voice because no such voice is expressible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeVoice {
    Current,
    Intent,
    NeedsReview,
    Unknown,
    HistoryOnly,
    Suppressed,
}

/// Which voices retrieval may select.
#[derive(Debug)]
pub struct KnowledgeVoiceFilter;

impl KnowledgeVoiceFilter {
    /// The default selection set: `Current`, `Intent`, `NeedsReview`, and
    /// `Unknown`, per the data model's `authority_scope` default — which
    /// INCLUDES the current-implementation voice and EXCLUDES `HistoryOnly`
    /// and `Suppressed`. Explicit history scopes never promote non-current
    /// evidence.
    pub fn selectable_voices() -> Vec<KnowledgeVoice> {
        vec![
            KnowledgeVoice::Current,
            KnowledgeVoice::Intent,
            KnowledgeVoice::NeedsReview,
            KnowledgeVoice::Unknown,
        ]
    }
}
