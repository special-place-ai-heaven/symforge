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
