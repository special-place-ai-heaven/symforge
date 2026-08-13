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

/// The closed set of operation shapes. Transport mapping derives from this one
/// table, so a new operation cannot acquire a bespoke refusal vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperationKind {
    Retrieval,
    Comparison,
    Derivation,
    SelectedAggregate,
}

impl OperationKind {
    /// Stable display name. Part of the closed contract, not a debug string.
    pub fn kind_name(self) -> &'static str {
        match self {
            Self::Retrieval => "Retrieval",
            Self::Comparison => "Comparison",
            Self::Derivation => "Derivation",
            Self::SelectedAggregate => "SelectedAggregate",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RetryAdvice {
    /// The request is wrong in a way repetition cannot fix.
    Never,
    /// A rebind to the correct root could succeed.
    AfterRebind,
    /// A completed refresh could succeed.
    AfterRefresh,
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

    /// Build a typed refusal against this lease's evidence.
    pub fn refuse(
        &self,
        operation: OperationReceipt,
        kind: SourceRefusalKind,
        retry: RetryAdvice,
    ) -> SourceRefusal {
        SourceRefusal::new(kind, operation, retry, Some(AuthorityIdentity::fresh()))
    }

    /// Authority to bound the rendering of an ALREADY complete leased result.
    ///
    /// `OutputCoverage::Truncated` cannot be constructed without one of these,
    /// which is what "post-lease only" means: truncation describes a response,
    /// never a generation.
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
    pub fn into_failed_read(self) -> Result<Self, SourceRefusal> {
        Err(SourceRefusal::new(
            SourceRefusalKind::SourceUnavailable,
            OperationReceipt::for_test(OperationKind::Retrieval),
            RetryAdvice::AfterRefresh,
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
        generations: BTreeMap<String, GenerationAuthority>,
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
                RetryAdvice::AfterRebind,
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
                        RetryAdvice::AfterRebind,
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
    /// or unmatched entry refuses.
    pub fn selected_aggregate(
        operation: OperationReceipt,
        selections: Vec<SourceSelectionReceipt>,
        generations: Vec<(String, GenerationAuthority)>,
    ) -> Result<Self, SourceRefusal> {
        if selections.is_empty() {
            return Err(SourceRefusal::new(
                SourceRefusalKind::InvalidSelection,
                operation,
                RetryAdvice::Never,
                None,
            ));
        }
        let captured: BTreeMap<String, GenerationAuthority> = generations.into_iter().collect();
        if captured.len() != selections.len()
            || !selections
                .iter()
                .all(|selection| captured.contains_key(selection.project_source()))
        {
            return Err(SourceRefusal::new(
                SourceRefusalKind::SelectionUnavailable,
                operation,
                RetryAdvice::AfterRefresh,
                None,
            ));
        }
        Ok(Self::SelectedAggregate {
            identity: ProvenanceIdentity::fresh(),
            operation,
            selections,
            generations: captured,
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
    /// a claim must be able to name each thing it was derived from.
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
            Self::SelectedAggregate { .. } => Vec::new(),
        };
        items.into_iter()
    }

    pub fn authority_count(&self) -> usize {
        match self {
            Self::Single { .. } => 1,
            Self::Comparison { .. } => 2,
            Self::Derivation {
                nonempty_inputs, ..
            } => nonempty_inputs.len(),
            Self::SelectedAggregate { generations, .. } => generations.len(),
        }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputCoverage {
    Complete,
    Truncated { breaches: Vec<LimitBreach> },
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
        OutputCoverage::Truncated {
            breaches: breaches
                .into_iter()
                .map(|(kind, omitted)| LimitBreach { kind, omitted })
                .collect(),
        }
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

    /// Bound this claim's RENDERING. Deliberately does not touch provenance
    /// identity: truncation describes a response, and caches, CCR, and
    /// persistence key on provenance identity. Moving it here would make a
    /// bounded render look like different evidence.
    pub fn render_bounded(self, _coverage: OutputCoverage) -> Self {
        self
    }
}

// ── Retrieval voice ────────────────────────────────────────────────────────

/// A knowledge voice a retrieval may select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeVoice {
    Intent,
    NeedsReview,
    Unknown,
    HistoryOnly,
    Suppressed,
    /// Consistency is an AUTHORITY HYGIENE verdict, never a retrieval filter.
    Consistency,
}

impl KnowledgeVoice {
    pub fn is_consistency(self) -> bool {
        matches!(self, Self::Consistency)
    }
}

/// Which voices retrieval may select.
#[derive(Debug)]
pub struct KnowledgeVoiceFilter;

impl KnowledgeVoiceFilter {
    /// Consistency is absent by construction, not by filtering after the fact.
    /// A stale document must not be able to acquire generation authority by
    /// being selected as a consistency voice.
    pub fn selectable_voices() -> Vec<KnowledgeVoice> {
        vec![
            KnowledgeVoice::Intent,
            KnowledgeVoice::NeedsReview,
            KnowledgeVoice::Unknown,
        ]
    }
}
