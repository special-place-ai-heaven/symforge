//! STEL (SymForge Token Economics Layer) — Phase 1 product module.
//!
//! Checkpoint: `31d9bf1` on `v8/stel-architecture` — see [`docs/phase1-stel-checkpoint.md`].
//!
//! Shipped layers on compact `symforge`:
//! - **L0:** MCP compact surface (`symforge` | `symforge_edit` | `status`); production list via
//!   [`surface_list::compact_surface_tools`]. Phase 0 harness relay + frozen schemas in
//!   [`crate::protocol::surface_probe`].
//! - **L1:** [`planner::build_plan`] — `StelRequest` → single-step [`StelPlan`].
//! - **L2:** [`controller::evaluate_plan`] — economics → [`StelDecision`] / [`StelEstimate`].
//! - **L3:** [`executor::is_enforced_bypass`] — P-FF bypass skips legacy tool dispatch.
//! - **L4:** [`ledger::SessionLedger`] — in-memory [`StelLedgerEvent`] rows + envelope `ledger:` line.
//!
//! Deferred: calibration feedback, `symforge_edit` handler, multi-step plans, persistence.

pub mod controller;
pub mod envelope;
pub mod executor;
pub mod golden_replay;
pub mod handler;
pub mod ledger;
pub mod planner;
pub mod status;
pub mod surface;
pub mod surface_list;
pub mod types;

pub use envelope::{TrustEnvelopeInput, format_trust_envelope};
pub use golden_replay::{
    GOLDEN_ROUTES_FIXTURE, ReplayValidation, S4_EXIT_ROW_IDS, S4_REPLAY_CORPUS,
    corpus_for_row_id, load_golden_rows, parse_golden_rows, s4_exit_rows,
    validate_s4_replay_output,
};
pub use controller::{
    build_estimate, detect_pff_bypass, estimate_economics, evaluate_plan, EconomicsBreakdown,
    COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS, SERVE_MARGIN_TOKENS,
};
pub use executor::{format_bypass_body, is_enforced_bypass};
pub use ledger::{
    LedgerCaptureInput, LedgerEnvelopeMeta, SessionLedger, build_ledger_event, capture_ledger,
    format_ledger_envelope_line,
};
pub use handler::{
    DecisionEnvelopeMetrics, StubServeMetrics, envelope_for_decision, envelope_for_stub_serve,
    estimate_tokens, finalize_symforge_output, format_preview_body, format_preview_body_for_plan,
    format_preview_estimate, metrics_for_decision, prepend_envelope, stub_plan_summary,
};
pub use planner::{build_plan, confidence_label, plan_summary_line};
pub use status::{
    DEFERRED_ITEMS, PHASE0_EVIDENCE_COMMIT, PHASE0_GO_COMMIT, StelStatusContext,
    format_stel_status,
};
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
