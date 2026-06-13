//! STEL (SymForge Token Economics Layer) — Phase 1 product module.
//!
//! **S2 (this slice):** schema-aligned types + compact surface registry + envelope formatter.
//!
//! Integration boundaries (wired in later slices):
//! - **L0 MCP:** `protocol` compact handlers will accept [`StelRequest`] and emit
//!   [`format_trust_envelope`] (S3–S4). Phase 0 measurement relay stays in
//!   [`crate::protocol::surface_probe`] until S3 migrates `tools/list` schemas.
//! - **L1:** extend [`crate::protocol::smart_query`] into a plan builder (S5).
//! - **L2:** controller consumes [`StelPlan`] → [`StelDecision`] (S6).
//! - **L3:** legacy tool dispatch via [`crate::protocol::SymForgeServer`] (S4+).
//! - **L4:** append [`StelLedgerEvent`] + [`CalibrationState`] feedback (S7).

pub mod envelope;
pub mod surface;
pub mod types;

pub use envelope::{TrustEnvelopeInput, format_trust_envelope};
pub use surface::{COMPACT_SURFACE_TOOL_COUNT, COMPACT_TOOL_NAMES, CompactSurfaceTool};
pub use types::{
    AdmissionDecision, CalibrationState, CoreToolName, GoldenRouteRow, IndexRef, IntentBucket,
    RouteConfidence, StelBypassBody, StelCacheBody, StelDecision, StelEstimate, StelExecution,
    StelExecutionTotals, StelLedgerEvent, StelPlan, StelPlanStep, StelRequest, StelStepExecution,
};
