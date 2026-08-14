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

use crate::lifecycle_identity::{
    OperationKind, OperationReceipt, RetryAdvice, SourceRefusal, SourceRefusalKind,
};

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

/// The public view of one source's runtime state, contract field-for-field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRuntimeView {
    pub binding_identity: String,
    pub current_publication_identity: Option<String>,
    pub observer_epoch: u64,
    pub phase: super::runtime::SourceRuntimePhase,
    pub source_version: u64,
}

/// The honest dark refusal every unbound operation returns: no generation is
/// bound, nothing was examined.
pub(crate) fn dark_unbound_refusal(kind: OperationKind) -> EmbedSourceRefusal {
    EmbedSourceRefusal::wrap(&SourceRefusal::for_runtime(
        SourceRefusalKind::SourceUnavailable,
        OperationReceipt::for_test(kind),
        RetryAdvice::OnEvent,
        None,
    ))
}

// ── The runtime wrapper ────────────────────────────────────────────────────

/// The contract-shaped process runtime: zero-argument `acquire` delegating to
/// the explicit-budget `incarnate` with the named provisional constant.
#[derive(Debug)]
pub struct ProcessRuntimeApi {
    _inner: std::sync::Arc<ProcessIndexRuntime>,
}

impl ProcessRuntimeApi {
    /// The atom's shape: no receiver, no arguments. The budget question is an
    /// activation decision; the dark form admits with the provisional
    /// constant and cannot fail, so the `Result` shape is carried for the
    /// contract while the refusing evidence arrives with Slice 4.
    pub fn acquire() -> Result<Self, EmbedSourceRefusal> {
        Ok(Self {
            _inner: ProcessIndexRuntime::incarnate(PROVISIONAL_ACQUIRE_PROCESS_BYTES),
        })
    }

    /// Fixture probe: the wrapper's honest dark refusal, for shape oracles.
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
/// * `"wrap-planned-t049"` — the internal type's shape diverges from the
///   contract record (the D13 list) and its wrapper lands with the T049
///   harness or the activation cut; recorded so it cannot be forgotten.
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
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::Claim",
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::ClaimProvenance",
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::EmbeddedSourceHandle",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::EmbeddedSourceSpec",
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::EvaluationProvenance",
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::OperationKind",
            obligation: "wrapped-here",
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
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::RetryAdvice",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::ShutdownReceipt",
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::ShutdownReport",
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::SourceCloseReceipt",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SourceCloseReport",
            obligation: "wrap-planned-t049",
        },
        WrapEntry {
            atom: "symforge::embed::SourceRefusal",
            obligation: "wrapped-here",
        },
        WrapEntry {
            atom: "symforge::embed::SourceRefusalKind",
            obligation: "wrapped-here",
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
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("pub mod ")
                .and_then(|rest| rest.strip_suffix(';'))
                .map(|name| format!("symforge::{name}"))
        })
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
        "computed_as": "public-api-v11.json introduced_v11_atoms minus the live pub-mod census of src/lib.rs",
        "live_census_pub_mods": live_mods.iter().collect::<Vec<_>>(),
        "introduced_atoms": atoms,
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
            "form": "pub(crate) mod server_api in src/lib.rs, std-only stub",
            "activation": "one keyword: pub(crate) becomes pub; the census gains the four server_api atoms at that instant and never before"
        }
    });
    serde_json::to_string_pretty(&delta).expect("delta serializes")
}
