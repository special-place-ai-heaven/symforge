//! MCP `status`, `symforge` and `symforge_edit` parity: the compact STEL
//! surface over the planner, economics, ledger and calibration runtime the
//! MCP handlers run, routed to this source's native lanes.

pub use crate::index_lifecycle::guidance::outcome::OutcomeClass;
pub use crate::stel::types::{
    IntentBucket, StelEditIntent, StelEditOp, StelEditRequest, StelRequest, StelStatusDetail,
    StelStatusRequest,
};

/// A `status` line MCP renders from a process an embedded host does not have.
/// Reported with its reason, never silently omitted.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StelNotApplicable {
    pub section: String,
    pub reason: String,
}

/// MCP `status`: the shared STEL readout for this source, followed by one
/// `<section>: not_applicable (<reason>)` line per entry of `not_applicable`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StelStatusReport {
    pub rendered: String,
    pub not_applicable: Vec<StelNotApplicable>,
}

/// One planned primitive the facade executed on this source's native lane.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SymforgeStep {
    /// The MCP tool the shared planner chose.
    pub tool: String,
    /// The native request that tool routed to.
    pub request: crate::embed::parity::QueryRequest,
    /// The step's answer, or `None` when the lane refused it.
    pub output: Option<Box<crate::embed::parity::QueryOutput>>,
    pub outcome: OutcomeClass,
}

/// MCP `symforge` (compact facade): the trust envelope, the served body and
/// the bound-root line, with the outcome class MCP reports for it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SymforgeAnswer {
    pub outcome: OutcomeClass,
    pub rendered: String,
    pub steps: Vec<SymforgeStep>,
}

/// The host-issued write authority an applying `symforge_edit` runs under,
/// and the admitted worktrees its `working_directory` may route into.
pub struct SymforgeEditAuthority<'a> {
    pub authority: &'a crate::embed::parity::edit::EditApplyAuthority,
    pub admitted: &'a [crate::embed::parity::edit::AdmittedEditTarget<'a>],
}

/// What an applying `symforge_edit` committed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymforgeEditApplied {
    /// Repository-relative path of the edited file.
    pub path: String,
    /// Absolute path the bytes were written to (a worktree when rerouted).
    pub wrote_to: String,
    pub post_image_hash: String,
    pub refresh_ticket_identity: Option<String>,
}

/// MCP `symforge_edit`: the trust envelope, routing summary and edit body,
/// with the outcome class MCP reports. `replayed` marks an idempotent retry
/// answered from its verified replay record without writing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymforgeEditAnswer {
    pub outcome: OutcomeClass,
    /// MCP's host-visible error flag: every outcome but `Found`.
    pub is_error: bool,
    pub rendered: String,
    pub replayed: bool,
    pub applied: Option<SymforgeEditApplied>,
}
