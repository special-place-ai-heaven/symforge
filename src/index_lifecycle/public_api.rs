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

/// The provisional process-byte budget behind the contract's zero-argument
/// `acquire`. A NAMED constant, recorded in the D-ledger as provisional and
/// not policy: the real budget source is an activation decision, and this is
/// deliberately NOT `configured_inflight_byte_budget`, which is live V10 env
/// policy that must not leak into a dark constructor.
pub const PROVISIONAL_ACQUIRE_PROCESS_BYTES: u64 = 256 * 1024 * 1024;

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

/// The contract-shaped refresh ticket. Dark handles can never mint one — the
/// type exists so the refusal-returning signature is contract-true and the
/// Slice 4 wiring changes evidence, not shape.
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
// Dark handles refuse every operation, so nothing in Slice 3 can mint a
// claim, an authority, or an evaluation — these types exist so the
// `Result<Claim<..>, ..>` signatures are contract-true and the Slice 4
// wiring changes evidence, not shape. No constructor is public; none is
// needed until something can honestly observe what these types report.

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

/// The contract-shaped shutdown receipt. The DARK runtime spawns nothing and
/// closes no holder's source, so the wait completes immediately and reports
/// the counts it observed — zeros, honestly, not a claim of teardown work
/// nothing performed. Slice 4 wires the real lifecycle behind this shape.
#[derive(Debug)]
pub struct EmbedShutdownReceipt {
    _not_unwind_safe: NotUnwindSafe,
}

impl EmbedShutdownReceipt {
    /// The contract wait. Nothing is spawned in the dark modules, so the
    /// deadline can never be reached; the parameter keeps its contract name
    /// and the dark lane records that it does not read it.
    pub fn wait(&self, deadline: std::time::Instant) -> Result<ShutdownReport, ReceiptWaitError> {
        let _ = deadline;
        Ok(ShutdownReport {
            closed_sources: 0,
            joined_workers: 0,
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
    factory: std::sync::Arc<super::embedded::EmbeddedSourceFactory>,
    _not_unwind_safe: NotUnwindSafe,
}

impl Drop for ProcessRuntimeApi {
    fn drop(&mut self) {
        // The contract pins a literal `Drop` on this atom. The dark runtime
        // owns no workers, so there is nothing to join yet; Slice 4 gives
        // this body its real teardown.
    }
}

impl ProcessRuntimeApi {
    /// The atom's shape: no receiver, no arguments. The budget question is an
    /// activation decision; the dark form admits with the provisional
    /// constant and cannot fail, so the `Result` shape is carried for the
    /// contract while the refusing evidence arrives with Slice 4.
    pub fn acquire() -> Result<Self, EmbedSourceRefusal> {
        Ok(Self {
            _inner: ProcessIndexRuntime::incarnate(PROVISIONAL_ACQUIRE_PROCESS_BYTES),
            factory: super::embedded::EmbeddedSourceFactory::new(),
            _not_unwind_safe: std::marker::PhantomData,
        })
    }

    /// Open the sole handle for the source `spec` names (T049). Dark behavior
    /// is the registration-level truth: sole-handle admission works, and a
    /// second open of a source already held refuses — the selected source is
    /// unavailable until its holder closes, hence `SelectionUnavailable` with
    /// `OnEvent` retry. No authority is examined at registration level, so
    /// the evidence renders the closed sentinel.
    pub fn open_embedded_source(
        &self,
        spec: EmbeddedSourceSpec,
    ) -> Result<super::embedded::EmbeddedSourceHandle, EmbedSourceRefusal> {
        let key = super::registry::ProjectKey::new(spec.root.to_string_lossy());
        self.factory.open(key).map_err(|refusal| {
            // D18, ratified and NARROWED: open() refuses only SourceAlreadyOpen
            // (M14 pinned that), so the two arms that mapped refusals open()
            // cannot produce are deleted rather than given dead kind mappings.
            // The held_by identity is an EmbeddedIdentity, not an
            // AuthorityIdentity — surfacing it as refusal evidence would MINT,
            // so the sentinel stands.
            let super::embedded::EmbedRefusal::SourceAlreadyOpen { .. } = refusal else {
                unreachable!("EmbeddedSourceFactory::open refuses only SourceAlreadyOpen")
            };
            EmbedSourceRefusal::wrap(&SourceRefusal::for_runtime(
                SourceRefusalKind::SelectionUnavailable,
                OperationReceipt::for_dark_refusal(OperationKind::OpenEmbeddedSource),
                RetryAdvice::OnEvent,
                None,
            ))
        })
    }

    /// Begin process shutdown (T049). The dark runtime closes no holder's
    /// source and joins no workers — the receipt's wait reports the observed
    /// zeros rather than teardown work nothing performed.
    pub fn begin_shutdown(&self) -> EmbedShutdownReceipt {
        EmbedShutdownReceipt {
            _not_unwind_safe: std::marker::PhantomData,
        }
    }

    /// Fixture probe: the wrapper's honest dark refusal, for shape oracles.
    #[cfg(any(test, feature = "server"))]
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
/// * `"keyword-flip"` — `server_api`: a real `pub(crate)` module whose
///   activation is one keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrapEntry {
    pub atom: &'static str,
    pub obligation: &'static str,
}

/// The closed table over exactly the top-level introduced atoms.
pub fn wrap_table() -> &'static [WrapEntry] {
    const TABLE: &[WrapEntry] = &[
        WrapEntry {
            atom: "symforge::embed::AtomicAuthority",
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
            "form": "cfg feature=server gated pub(crate) mod server_api in src/lib.rs, std-only stub",
            "activation": "one keyword behind the already-present server cfg gate: pub(crate) becomes pub, and the census gains the four server_api atoms in server graphs only - the embed-v11 projection excludes this module, so no embed cell may ever grow them"
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
