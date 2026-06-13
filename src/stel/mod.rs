//! STEL (SymForge Token Economics Layer) — Phase 1 product module.
//!
//! **S2 (this slice):** schema-aligned types + compact surface registry + envelope formatter.
//!
//! Integration boundaries (wired in later slices):
//! - **L0 MCP:** `protocol` compact handlers accept [`SymforgeCallInput`] and prepend
//!   [`format_trust_envelope`] (S4). Phase 0 harness relay uses `_probe_*` fields on the same
//!   tool name. Measurement schemas remain in [`crate::protocol::surface_probe`].
//! - **L1:** [`planner::build_plan`] maps [`StelRequest`] → [`StelPlan`] (S5).
//! - **L2:** controller consumes [`StelPlan`] → [`StelDecision`] (S6).
//! - **L3:** legacy tool dispatch via [`crate::protocol::SymForgeServer`] (S4+).
//! - **L4:** append [`StelLedgerEvent`] + [`CalibrationState`] feedback (S7).

pub mod envelope;
pub mod golden_replay;
pub mod handler;
pub mod planner;
pub mod surface;
pub mod surface_list;
pub mod types;

pub use envelope::{TrustEnvelopeInput, format_trust_envelope};
pub use golden_replay::{
    GOLDEN_ROUTES_FIXTURE, ReplayValidation, S4_EXIT_ROW_IDS, S4_REPLAY_CORPUS,
    corpus_for_row_id, load_golden_rows, parse_golden_rows, s4_exit_rows,
    validate_s4_replay_output,
};
pub use handler::{
    StubServeMetrics, envelope_for_stub_serve, estimate_tokens, format_preview_body,
    format_preview_body_for_plan, prepend_envelope, stub_plan_summary,
};
pub use planner::{build_plan, confidence_label, plan_summary_line};
pub use surface::{COMPACT_SURFACE_TOOL_COUNT, COMPACT_TOOL_NAMES, CompactSurfaceTool};
pub use surface_list::{
    compact_surface_list_schema_bytes, compact_surface_tools, symforge_edit_schema_bytes,
};
pub use types::{
    AdmissionDecision, CalibrationState, CoreToolName, GoldenRouteRow, IndexRef, IntentBucket,
    RouteConfidence, StelBypassBody, StelCacheBody, StelDecision, StelEditIntent, StelEditRequest,
    StelEstimate, StelExecution, StelExecutionTotals, StelLedgerEvent, StelPlan, StelPlanStep,
    StelRequest, StelStatusDetail, StelStatusRequest, StelStepExecution, SymforgeCallInput,
};
