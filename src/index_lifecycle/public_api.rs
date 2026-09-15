//! Feature 020 V11, T048 — the dark wrap table and the export delta.
//!
//! This module is the EMBED BOUNDARY's dark rehearsal: for every top-level
//! introduced atom it records HOW the internal type reaches the contract shape
//! — a wrapper, a keyword flip, never a 1:1 re-export of an internal path
//! (D12/D13). The wrappers that runtime-checkable oracles can pin are built
//! here; the rest carry their obligation in [`wrap_table`] so T049's harness
//! and the Slice 4 activation cut inherit a work list, not a guess.
//!
//! **Identity rendering (E3 ruling).** Contract accessors return `&str`; the
//! internal identities are opaque newtypes. Wrappers render KIND-PREFIXED
//! strings — `op-42`, `auth-17`, `hash-9f…` — STORED at construction, so the
//! `&str` borrows from the wrapper and is stable across calls. The renderer
//! owns the scheme; nothing drives it off `Display` of the newtype.
//!
//! **The sentinel (E2 ruling).** A refusal that examined no authority renders
//! [`EVIDENCE_ABSENT`] — a token the identity renderer can never emit, so
//! absence is never confusable with a real identity and nothing is minted.
//!
//! **Dark behavior is honest refusal.** No generation is bound to anything in
//! Slice 3, so the search and refresh wrappers REFUSE rather than fabricate
//! empty results — an empty result would be a claim about content that does
//! not exist.

use std::collections::BTreeSet;
use std::fmt;

use crate::lifecycle_identity::{OperationReceipt, SourceRefusal};
// T049: the contract's three enum atoms ARE the lifecycle_identity enums —
// minted contract-verbatim, so the wrap is nameability, not reshaping. This
// module is the boundary that makes them nameable; `lifecycle_identity` stays
// `pub(crate)` and invisible to the census.
pub use crate::lifecycle_identity::{OperationKind, RetryAdvice, SourceRefusalKind};

use super::embedded::ReceiptWaitError;
use super::process_runtime::ProcessIndexRuntime;

/// The closed sentinel for a refusal that examined no authority. Kind-prefixed
/// identities always render as `<kind>-<digits>`, so this token is outside the
/// renderer's image by construction.
pub const EVIDENCE_ABSENT: &str = "evidence-absent";

/// Auto-trait opt-out carried by every V11 handle type (T049). The frozen
/// contract pins the five handles `Send + Sync + Unpin` but NOT
/// `UnwindSafe`/`RefUnwindSafe` — the activated runtime's internals will not
/// be — so the dark stand-ins must already refuse those two impls, or the
/// activation cut would change the public trait surface.
pub(crate) type NotUnwindSafe = std::marker::PhantomData<Box<dyn std::any::Any + Send + Sync>>;

// ── Identity rendering ─────────────────────────────────────────────────────

fn render_operation_identity(receipt: &OperationReceipt) -> String {
    format!("op-{}", receipt.identity().raw_for_render())
}

fn render_argument_hash(receipt: &OperationReceipt) -> String {
    format!("hash-{:012x}", receipt.canonical_argument_hash().raw())
}

// ── The refusal wrapper ────────────────────────────────────────────────────

/// The contract-shaped operation receipt: `&str` identity accessors over
/// strings rendered once, at wrap time.
#[derive(Debug, Clone)]
pub struct EmbedOperationReceipt {
    identity: String,
    canonical_argument_hash: String,
    operation_kind: OperationKind,
    schema_version: u32,
}

impl EmbedOperationReceipt {
    fn wrap(receipt: &OperationReceipt) -> Self {
        Self {
            identity: render_operation_identity(receipt),
            canonical_argument_hash: render_argument_hash(receipt),
            operation_kind: receipt.operation_kind(),
            schema_version: receipt.schema_version(),
        }
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn canonical_argument_hash(&self) -> &str {
        &self.canonical_argument_hash
    }

    pub fn operation_kind(&self) -> OperationKind {
        self.operation_kind
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

/// The contract-shaped refusal: opaque, `&str` evidence with the closed
/// sentinel for none, `Display` + `Error` implemented as the contract's
/// trait_impls demand.
#[derive(Debug)]
pub struct EmbedSourceRefusal {
    kind: SourceRefusalKind,
    operation: EmbedOperationReceipt,
    retry: RetryAdvice,
    evidence_identity: String,
}

impl EmbedSourceRefusal {
    pub(crate) fn wrap(refusal: &SourceRefusal) -> Self {
        Self {
            kind: refusal.kind(),
            operation: EmbedOperationReceipt::wrap(&refusal.operation()),
            retry: refusal.retry(),
            evidence_identity: match refusal.evidence_identity() {
                Some(identity) => format!("auth-{}", identity.raw_for_render()),
                None => EVIDENCE_ABSENT.to_string(),
            },
        }
    }

    pub fn kind(&self) -> SourceRefusalKind {
        self.kind
    }

    /// Stable display name of the kind, for oracles and diagnostics.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            SourceRefusalKind::AdmissionUnavailable => "AdmissionUnavailable",
            SourceRefusalKind::InvalidSelection => "InvalidSelection",
            SourceRefusalKind::SelectionUnavailable => "SelectionUnavailable",
            SourceRefusalKind::SourceUnavailable => "SourceUnavailable",
        }
    }

    pub fn operation(&self) -> &EmbedOperationReceipt {
        &self.operation
    }

    pub fn retry(&self) -> RetryAdvice {
        self.retry
    }

    pub fn evidence_identity(&self) -> &str {
        &self.evidence_identity
    }
}

impl fmt::Display for EmbedSourceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "source refusal: {} (operation {}, evidence {})",
            self.kind_name(),
            self.operation.identity(),
            self.evidence_identity
        )
    }
}

impl std::error::Error for EmbedSourceRefusal {}

// ── The search and view shapes, VERBATIM from the contract ─────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSearchRequest {
    pub query: Option<String>,
    pub path_prefix: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolMatch {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSearchResult {
    pub matches: Vec<SymbolMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSearchRequest {
    pub query: String,
    pub path_prefix: Option<String>,
    pub limit: u32,
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub path: String,
    pub line: u32,
    pub byte_start: u64,
    pub byte_end: u64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSearchResult {
    pub matches: Vec<TextMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexCensus {
    pub total_files: u64,
    pub total_symbols: u64,
    pub files: Vec<CensusFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CensusFile {
    pub path: String,
    pub language: String,
    pub symbol_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexProgress {
    pub files_discovered: u64,
    pub files_parsed: u64,
    pub symbols_found: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KnowledgeSearchRequest {
    pub query: String,
    pub path_prefix: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KnowledgeMatch {
    pub path: String,
    pub heading_path: Vec<String>,
    pub preview: String,
    pub content_hash: String,
    pub provenance_ids: Vec<String>,
    pub relationship_evidence: Vec<String>,
    pub authority: String,
    pub coverage: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KnowledgeSearchResult {
    pub matches: Vec<KnowledgeMatch>,
    pub truncated: bool,
    pub withheld_count: u64,
    pub withheld_reasons: Vec<String>,
}

/// The contract-shaped ticket for work queued on an embedded source worker.
#[derive(Debug)]
pub struct EmbedRefreshTicket {
    ticket_identity: String,
    operation: EmbedOperationReceipt,
    requested_source_version: u64,
    _not_unwind_safe: NotUnwindSafe,
}

impl EmbedRefreshTicket {
    pub fn ticket_identity(&self) -> &str {
        &self.ticket_identity
    }

    pub fn operation(&self) -> &EmbedOperationReceipt {
        &self.operation
    }

    pub fn requested_source_version(&self) -> u64 {
        self.requested_source_version
    }
}

// ── The claim family, contract-shaped (T049 wrap list) ─────────────────────
//
// Constructors remain private to the lifecycle boundary. A successful query
// mints these values only after capturing a current embedded publication.

/// The contract-shaped atomic authority: `&str` identity over a string
/// rendered at wrap time, plus the stable kind name.
#[derive(Debug)]
pub struct EmbedAtomicAuthority {
    identity: String,
    kind_name: &'static str,
}

impl EmbedAtomicAuthority {
    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn kind_name(&self) -> &'static str {
        self.kind_name
    }
}

/// The contract-shaped claim provenance: the closed authority set that was
/// actually examined. The count is measured from the set, never stored
/// separately — two fields that can disagree are one field too many.
#[derive(Debug)]
pub struct EmbedClaimProvenance {
    authorities: Vec<EmbedAtomicAuthority>,
    identity: String,
    kind_name: &'static str,
}

impl EmbedClaimProvenance {
    pub fn authorities(&self) -> &[EmbedAtomicAuthority] {
        &self.authorities
    }

    pub fn authority_count(&self) -> usize {
        self.authorities.len()
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn kind_name(&self) -> &'static str {
        self.kind_name
    }
}

/// The contract-shaped evaluation provenance.
#[derive(Debug)]
pub struct EmbedEvaluationProvenance {
    identity: String,
}

impl EmbedEvaluationProvenance {
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// The contract-shaped claim: a value carrying HOW it was produced.
#[derive(Debug)]
pub struct EmbedClaim<T> {
    value: T,
    provenance: EmbedClaimProvenance,
    operation: EmbedOperationReceipt,
    evaluation: Option<EmbedEvaluationProvenance>,
    producing_runtime_identity: String,
}

impl<T> EmbedClaim<T> {
    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn provenance(&self) -> &EmbedClaimProvenance {
        &self.provenance
    }

    pub fn operation(&self) -> &EmbedOperationReceipt {
        &self.operation
    }

    pub fn evaluation(&self) -> Option<&EmbedEvaluationProvenance> {
        self.evaluation.as_ref()
    }

    pub fn producing_runtime_identity(&self) -> &str {
        &self.producing_runtime_identity
    }
}

// ── The source spec and the report shapes (T049 wrap list) ─────────────────

/// The contract-shaped source spec: how an embedder names a source to open.
#[derive(Debug)]
pub struct EmbeddedSourceSpec {
    root: std::path::PathBuf,
}

impl EmbeddedSourceSpec {
    /// The contract constructor: the current worktree rooted at `root`.
    pub fn current_worktree(root: std::path::PathBuf) -> Self {
        Self { root }
    }
}

/// The contract-shaped shutdown report: observed counts only.
#[derive(Debug)]
pub struct ShutdownReport {
    pub closed_sources: u64,
    pub joined_workers: u64,
}

/// The contract-shaped source-close report. Distinct from the internal
/// `embedded::SourceCloseReport` (T047's observation record): this is the
/// PUBLIC field-for-field contract record the boundary's `wait` returns.
#[derive(Debug)]
pub struct SourceCloseReport {
    pub already_terminal: bool,
    pub terminal_source_version: u64,
}

/// The contract-shaped shutdown receipt with observed close/join counts.
#[derive(Debug)]
pub struct EmbedShutdownReceipt {
    report: ShutdownReport,
    _not_unwind_safe: NotUnwindSafe,
}

impl EmbedShutdownReceipt {
    /// Return the teardown result already observed by synchronous shutdown.
    pub fn wait(&self, deadline: std::time::Instant) -> Result<ShutdownReport, ReceiptWaitError> {
        let _ = deadline;
        Ok(ShutdownReport {
            closed_sources: self.report.closed_sources,
            joined_workers: self.report.joined_workers,
        })
    }
}

/// The public runtime phase, contract-verbatim, OWNED BY THE BOUNDARY (C7
/// ruling): `runtime::SourceRuntimePhase` is an internal path, and a public
/// field typed by it would be a D12 path-identity leak through the embed
/// surface. Same six variants; the mapping is total and explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRuntimePhase {
    Blocked,
    Current,
    Loading,
    Refreshing,
    Stopped,
    Stopping,
}

/// The public view of one source's runtime state, contract field-for-field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRuntimeView {
    pub binding_identity: String,
    pub current_publication_identity: Option<String>,
    pub observer_epoch: u64,
    pub phase: SourceRuntimePhase,
    pub source_version: u64,
}

/// The honest dark refusal every unbound operation returns: no generation is
/// bound, nothing was examined.
pub(crate) fn dark_unbound_refusal(kind: OperationKind) -> EmbedSourceRefusal {
    EmbedSourceRefusal::wrap(&SourceRefusal::for_runtime(
        SourceRefusalKind::SourceUnavailable,
        OperationReceipt::for_dark_refusal(kind),
        RetryAdvice::OnEvent,
        None,
    ))
}

pub(crate) fn bound_source_refusal(
    kind: SourceRefusalKind,
    operation: OperationKind,
    retry: RetryAdvice,
    normalized_arguments: &[u8],
) -> EmbedSourceRefusal {
    EmbedSourceRefusal::wrap(&SourceRefusal::for_runtime(
        kind,
        OperationReceipt::normalized(operation, normalized_arguments),
        retry,
        None,
    ))
}

pub(crate) fn live_claim<T>(
    value: T,
    operation_kind: OperationKind,
    normalized_arguments: &[u8],
    binding_identity: &str,
    publication_identity: &str,
    source_version: u64,
) -> EmbedClaim<T> {
    let operation = EmbedOperationReceipt::wrap(&OperationReceipt::normalized(
        operation_kind,
        normalized_arguments,
    ));
    EmbedClaim {
        value,
        provenance: EmbedClaimProvenance {
            authorities: vec![
                EmbedAtomicAuthority {
                    identity: binding_identity.to_string(),
                    kind_name: "source-binding",
                },
                EmbedAtomicAuthority {
                    identity: publication_identity.to_string(),
                    kind_name: "verified-publication",
                },
            ],
            identity: format!(
                "embed-provenance-{binding_identity}-{publication_identity}-v{source_version}"
            ),
            kind_name: "live-index-query",
        },
        operation,
        evaluation: Some(EmbedEvaluationProvenance {
            identity: format!("embed-evaluation-{publication_identity}"),
        }),
        producing_runtime_identity: binding_identity.to_string(),
    }
}

pub(crate) fn refresh_ticket(
    normalized_arguments: &[u8],
    requested_source_version: u64,
) -> EmbedRefreshTicket {
    let operation = EmbedOperationReceipt::wrap(&OperationReceipt::normalized(
        OperationKind::RefreshSource,
        normalized_arguments,
    ));
    EmbedRefreshTicket {
        ticket_identity: format!("refresh-{}", operation.identity()),
        operation,
        requested_source_version,
        _not_unwind_safe: std::marker::PhantomData,
    }
}

// ── The runtime wrapper ────────────────────────────────────────────────────

/// The contract-shaped process runtime: zero-argument `acquire` delegating to
/// the explicit-budget `incarnate` with the named provisional constant.
///
/// `Clone` and `Drop` are CONTRACT-pinned direct impls, not conveniences: the
/// frozen trait_impls record both, so the dark stand-in carries them or the
/// activation cut would change the public trait surface.
#[derive(Debug, Clone)]
pub struct ProcessRuntimeApi {
    _inner: std::sync::Arc<ProcessIndexRuntime>,
    owner: std::sync::Arc<super::embedded::EmbeddedRuntimeOwner>,
    _not_unwind_safe: NotUnwindSafe,
}

impl Drop for ProcessRuntimeApi {
    fn drop(&mut self) {
        // The contract pins a literal `Drop` on this atom. `owner` is shared
        // by clones; dropping its final runtime wrapper synchronously closes
        // every source and joins every embedded worker.
    }
}

impl ProcessRuntimeApi {
    /// Attach this API to the one process runtime and allocate an independent
    /// runtime owner over its shared embedded-source registry.
    pub fn acquire() -> Result<Self, EmbedSourceRefusal> {
        super::activation::activate_surface(super::process_runtime::SurfaceKind::Embed);
        let inner = super::activation::process_index_runtime();
        let owner = super::embedded::EmbeddedRuntimeOwner::new(inner.embedded_factory());
        Ok(Self {
            _inner: inner,
            owner,
            _not_unwind_safe: std::marker::PhantomData,
        })
    }

    /// Resolve, admit, and asynchronously index the source named by `spec`.
    /// A second open of the same canonical source refuses until its sole
    /// handle closes.
    pub fn open_embedded_source(
        &self,
        spec: EmbeddedSourceSpec,
    ) -> Result<super::embedded::EmbeddedSourceHandle, EmbedSourceRefusal> {
        let normalized = format!("current_worktree={:?}", spec.root);
        let binding = match crate::discovery::resolve_root_candidate(
            &spec.root,
            crate::domain::RootCandidateSource::McpClientRoot,
            crate::domain::RootRequestMode::Automatic,
        ) {
            crate::domain::RootResolution::Bound(binding) => binding,
            crate::domain::RootResolution::Unbound { .. } => {
                return Err(bound_source_refusal(
                    SourceRefusalKind::InvalidSelection,
                    OperationKind::OpenEmbeddedSource,
                    RetryAdvice::Operator,
                    normalized.as_bytes(),
                ));
            }
        };
        let state_placement = crate::discovery::resolve_state_placement(&binding);
        self.owner
            .factory()
            .open_bound(binding, state_placement, self.owner.identity())
            .map_err(|refusal| match refusal {
                super::embedded::EmbeddedOpenError::SourceAlreadyOpen => bound_source_refusal(
                    SourceRefusalKind::SelectionUnavailable,
                    OperationKind::OpenEmbeddedSource,
                    RetryAdvice::OnEvent,
                    normalized.as_bytes(),
                ),
                super::embedded::EmbeddedOpenError::AdmissionUnavailable => bound_source_refusal(
                    SourceRefusalKind::AdmissionUnavailable,
                    OperationKind::OpenEmbeddedSource,
                    RetryAdvice::Automatic,
                    normalized.as_bytes(),
                ),
                super::embedded::EmbeddedOpenError::WorkerUnavailable => bound_source_refusal(
                    SourceRefusalKind::SourceUnavailable,
                    OperationKind::OpenEmbeddedSource,
                    RetryAdvice::Automatic,
                    normalized.as_bytes(),
                ),
            })
    }

    /// Close every source owned by this runtime and join its workers.
    pub fn begin_shutdown(&self) -> EmbedShutdownReceipt {
        let report = self.owner.shutdown();
        EmbedShutdownReceipt {
            report: ShutdownReport {
                closed_sources: report.closed_sources,
                joined_workers: report.joined_workers,
            },
            _not_unwind_safe: std::marker::PhantomData,
        }
    }

    /// Fixture probe: the wrapper's honest dark refusal, for shape oracles.
    #[cfg(all(test, feature = "server"))]
    pub fn refusal_probe_for_test(&self) -> Result<(), EmbedSourceRefusal> {
        Err(dark_unbound_refusal(OperationKind::SearchSymbols))
    }
}

// ── The wrap table and the export delta ────────────────────────────────────

/// One top-level introduced atom and how its contract shape is satisfied.
/// Obligations are the module's own judgment — NEVER path identity:
///
/// * `"wrapped-here"` — a contract-shaped wrapper exists in this module or on
///   the SEAM-pinned handle, exercised by the shape oracle.
/// * `"verbatim-reexport"` — the C7 ruling's third word: the internal type
///   was MINTED contract-verbatim (the `lifecycle_identity` enums) and this
///   module makes it nameable by an actual `pub use` — which the delta
///   oracle verifies against the source, never trusting this self-report.
///   Distinct from the banned `"direct-reexport"`: nothing pre-existing
///   leaks; the type exists only because the contract named it.
/// * `"wrap-planned-t049"` — RETIRED vocabulary: T048 recorded the nine
///   shape-diverging types (the D13 list) under it so they could not be
///   forgotten; T049 discharged all nine into `"wrapped-here"` wrappers.
/// * `"keyword-flip"` — `server_api`: the `pub(crate)` module whose
///   activation was one keyword — executed at C5; the module is public and
///   wired to the crate dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrapEntry {
    pub atom: &'static str,
    pub obligation: &'static str,
}

/// The V11 public-API seam (the frozen postactivation anchor
/// `src/index_lifecycle/public_api.rs::V11PublicApi`): the boundary's own
/// record of how each introduced atom is satisfied. `embed.rs` re-exports
/// the wrappers; the export-delta oracle reads the table through this
/// seam, so the anchor is load-bearing, not a marker.
pub struct V11PublicApi;

impl V11PublicApi {
    /// The closed wrap table over exactly the top-level introduced atoms.
    pub fn wrap_table() -> &'static [WrapEntry] {
        wrap_table()
    }
}

/// The closed table over exactly the top-level introduced atoms.
pub fn wrap_table() -> &'static [WrapEntry] {
    const TABLE: &[WrapEntry] = &[
        WrapEntry {
            atom: "symforge::embed::AtomicAuthority",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::CensusFile",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::Claim",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::ClaimProvenance",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::EmbeddedSourceHandle",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::EmbeddedSourceSpec",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::EvaluationProvenance",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::IndexCensus",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::IndexProgress",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::KnowledgeMatch",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::KnowledgeSearchRequest",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::KnowledgeSearchResult",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::OperationKind",
            obligation: "verbatim-reexport",
        },
        WrapEntry {
            atom: "symforge::embed::OperationReceipt",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::ProcessIndexRuntime",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::ReceiptWaitError",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::RefreshTicket",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::RetryAdvice",
            obligation: "verbatim-reexport",
        },
        WrapEntry {
            atom: "symforge::embed::ShutdownReceipt",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::ShutdownReport",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SourceCloseReceipt",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SourceCloseReport",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SourceRefusal",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SourceRefusalKind",
            obligation: "verbatim-reexport",
        },
        WrapEntry {
            atom: "symforge::embed::SourceRuntimePhase",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SourceRuntimeView",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SymbolMatch",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SymbolSearchRequest",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SymbolSearchResult",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::TextMatch",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::TextSearchRequest",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::TextSearchResult",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::server_api",
            obligation: "keyword-flip",
        },
        WrapEntry {
            atom: "symforge::server_api::ServerBootstrapError",
            obligation: "keyword-flip",
        },
        WrapEntry {
            atom: "symforge::server_api::ServerExit",
            obligation: "keyword-flip",
        },
        WrapEntry {
            atom: "symforge::server_api::run",
            obligation: "keyword-flip",
        },
    ];
    TABLE
}

/// Render the export delta as closed JSON: the frozen contract's SHA, every
/// introduced atom, the live census it is measured against, the per-atom
/// obligations, and the two forbidden citizens D12 records. Deterministic:
/// every collection is sorted.
pub fn render_export_delta(contract_text: &str, lib_text: &str) -> String {
    let contract_sha = crate::hash::digest_hex(contract_text.as_bytes());
    let contract: serde_json::Value =
        serde_json::from_str(contract_text).expect("frozen contract parses");
    let atoms: Vec<String> = contract["migration_v10"]["introduced_v11_atoms"]
        .as_array()
        .expect("atoms")
        .iter()
        .map(|a| a.as_str().expect("atom").to_string())
        .collect();
    let live_mods: BTreeSet<String> = lib_text
        .lines()
        .filter_map(parse_pub_mod)
        .map(|name| format!("symforge::{name}"))
        .collect();
    // C14: the subtraction the artifact CLAIMS is the subtraction the
    // renderer PERFORMS — exact-match only: an atom drops out when it
    // appears VERBATIM in the live pub-mod census. A first draft keyed this
    // on the top-level module and wrongly subtracted all embed item atoms
    // because V10's `pub mod embed` exists — module existence is not item
    // existence.
    let introduced_minus_live: Vec<&String> = atoms
        .iter()
        .filter(|atom| !live_mods.contains(*atom))
        .collect();

    let obligations: Vec<serde_json::Value> = wrap_table()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "atom": entry.atom,
                "obligation": entry.obligation,
            })
        })
        .collect();

    let delta = serde_json::json!({
        "kind": "symforge-feature-020-export-delta",
        "schema_version": 1,
        "contract_sha256": contract_sha,
        "computed_as": "introduced_v11_atoms listed verbatim; introduced_minus_live is that list with every atom that already appears verbatim in the live pub-mod census of src/lib.rs subtracted",
        "live_census_pub_mods": live_mods.iter().collect::<Vec<_>>(),
        "introduced_atoms": atoms,
        "introduced_minus_live": introduced_minus_live,
        "obligations": obligations,
        "forbidden_at_activation": [
            {
                "citizen": "symforge::protocol::format::claim_provenance",
                "rule": "D12: the internal provenance mount must not surface as a public module path; the embed boundary wraps it"
            },
            {
                "citizen": "symforge::live_index::knowledge_bridge::LimitBreach through TruncationBreaches",
                "rule": "D12/D13: a deep internal type must not leak through the embed surface; the boundary wraps or unwinds it"
            }
        ],
        "server_api": {
            "form": "cfg feature=server gated pub mod server_api in src/lib.rs, wired to the crate dispatcher",
            "activation": "executed at C5: the pub(crate) keyword flipped behind the already-present server cfg gate, and the census carries the four server_api atoms in server graphs only - the embed-v11 projection excludes this module, so no embed cell may ever grow them"
        }
    });
    serde_json::to_string_pretty(&delta).expect("delta serializes")
}

/// Parse one `pub mod NAME;` census line, tolerant of interior whitespace —
/// aligned with the checker's regex rather than a stricter literal prefix.
fn parse_pub_mod(line: &str) -> Option<&str> {
    let mut words = line.split_whitespace();
    if words.next() != Some("pub") || words.next() != Some("mod") {
        return None;
    }
    let name = words.next()?.strip_suffix(';')?;
    if words.next().is_some() || name.is_empty() {
        return None;
    }
    Some(name)
}

/// Feature 020 V11, T048 — the dark wrapper contract-shape oracle.
///
/// Moved in-crate verbatim from `tests/public_api_delta_v11.rs` at the start
/// of the Slice 4 activation cut so `refusal_probe_for_test` could tighten to
/// `all(test, feature = "server")` and stop shipping in the release binary
/// (the same precondition discharge as `runtime.rs::dark_runtime_oracles`).
/// The export-delta oracle stays external — it consumes no fixture door.
#[cfg(all(test, feature = "server"))]
mod dark_wrapper_oracles {
    use super::super::embedded::EmbeddedSourceFactory;
    use super::super::registry::ProjectKey;
    use super::{
        EVIDENCE_ABSENT, EmbedOperationReceipt, EmbedSourceRefusal, ProcessRuntimeApi,
        SymbolSearchRequest, TextSearchRequest,
    };

    #[test]
    fn dark_wrappers_match_contract_shapes() {
        // acquire takes NO arguments per the atom. Since C5 it attaches to
        // the ONE process capacity runtime through the activation ceremony
        // (the provisional private-incarnation constant retired with it).
        let runtime = ProcessRuntimeApi::acquire().expect("the acquisition admits");

        // The refusal wrapper: kind-prefixed identity strings, stored at wrap
        // time; Display and Error implemented; the sentinel reserved for
        // refusals that examined nothing.
        let refusal: EmbedSourceRefusal = runtime
            .refusal_probe_for_test()
            .expect_err("the probe yields the wrapper's honest dark refusal");
        let evidence = refusal.evidence_identity();
        assert_eq!(
            evidence, EVIDENCE_ABSENT,
            "a refusal that examined no authority renders the closed sentinel"
        );
        assert!(
            !evidence.starts_with("auth-"),
            "the sentinel is a token the identity renderer cannot emit"
        );
        let operation: &EmbedOperationReceipt = refusal.operation();
        assert!(
            operation.identity().starts_with("op-"),
            "operation identities are kind-prefixed, got {}",
            operation.identity()
        );
        assert!(
            operation
                .identity()
                .trim_start_matches("op-")
                .parse::<u64>()
                .is_ok(),
            "the prefix is followed by the counter digits"
        );
        let first = operation.identity().to_string();
        assert_eq!(
            operation.identity(),
            first,
            "the rendered string is STORED at wrap time — stable across calls"
        );
        // Display + Error are contract trait impls, exercised not just
        // derived.
        let rendered = format!("{refusal}");
        assert!(
            rendered.contains("SourceUnavailable"),
            "Display names the refusal kind: {rendered}"
        );
        let _as_error: &dyn std::error::Error = &refusal;

        // The four V11 handle methods, under their contract shapes, refusing
        // honestly in the dark rather than fabricating empty results.
        let factory = EmbeddedSourceFactory::new();
        let handle = factory
            .open(ProjectKey::new("src-a"))
            .expect("open admits a fresh key");

        let view = handle.runtime_view();
        assert!(
            view.binding_identity.starts_with("source-"),
            "the view's binding identity is kind-prefixed: {}",
            view.binding_identity
        );
        assert!(
            view.current_publication_identity.is_none(),
            "a dark handle has NO publication; inventing one would be \
             fabricated completion"
        );
        assert_eq!(view.observer_epoch, 0, "no observer has been registered");

        let refusal = handle
            .search_symbols(&SymbolSearchRequest {
                query: Some("anchor".to_string()),
                path_prefix: None,
                limit: 10,
            })
            .expect_err("no generation is bound, so a symbol search refuses");
        assert_eq!(refusal.kind_name(), "SourceUnavailable");

        let refusal = handle
            .search_text(&TextSearchRequest {
                query: "anchor".to_string(),
                path_prefix: None,
                limit: 10,
                case_sensitive: false,
            })
            .expect_err("no generation is bound, so a text search refuses");
        assert_eq!(refusal.kind_name(), "SourceUnavailable");

        let refusal = handle
            .request_refresh()
            .expect_err("a dark refresh cannot run, so the ticket is refused honestly");
        assert_eq!(refusal.kind_name(), "SourceUnavailable");
    }
}
