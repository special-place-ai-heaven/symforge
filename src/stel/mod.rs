//! STEL (SymForge Token Economics Layer) — Phase 1 product module.
//!
//! Checkpoint: `31d9bf1` on `v8/stel-architecture` — see [`docs/phase1-stel-checkpoint.md`].
//!
//! Shipped layers on compact `symforge`:
//! - **L0:** MCP compact surface (`symforge` | `symforge_edit` | `status`); production list via
//!   [`surface_list::compact_surface_tools`]. Phase 0 harness relay + frozen schemas in
//!   [`crate::protocol::surface_probe`].
//! - **L1:** [`planner::build_plan`] — `StelRequest` → single- or multi-step [`StelPlan`].
//! - **L2:** [`controller::evaluate_plan`] — economics → [`StelDecision`] / [`StelEstimate`].
//! - **L3:** [`executor::should_skip_legacy_dispatch`] — bypass/cache_hit skip legacy dispatch; degrade caps; multi-step chain on `serve`.
//! - **L4:** [`ledger::SessionLedger`] — in-memory [`StelLedgerEvent`] rows + envelope `ledger:` line.
//!
//! Deferred: calibration auto-tuning/persistence, symforge_edit apply path.

pub mod a029;
pub mod calibration;
pub mod controller;
pub mod edit_apply;
pub mod edit_planner;
pub mod envelope;
pub mod executor;
pub mod gates;
pub mod golden_replay;
pub mod handler;
pub mod ledger;
#[cfg(feature = "server")]
pub mod ledger_store;
pub mod planner;
pub mod status;
pub mod surface;
pub mod surface_list;
pub mod types;

pub use a029::{
    A029_T2_PASS_THRESHOLD, A029SpikeResults, A029T2Row, A029Verdict, T2Equivalence,
    classify_t2_equivalence, evaluate_a029_verdict, normalize_spike_results,
};
pub use calibration::{
    StelCalibrationSummary, TUNING_REVIEW_MIN_EVENTS, format_calibration_section,
    summarize_calibration,
};
pub use controller::{
    COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS, EconomicsBreakdown, SERVE_MARGIN_TOKENS,
    build_estimate, detect_pff_bypass, estimate_economics, evaluate_edit_plan, evaluate_plan,
    evaluate_plan_with_session,
};
pub use edit_apply::{
    PreApplyOutcome, ResolvedEditSymbol, apply_requested, format_already_applied_body,
    format_apply_metadata, run_pre_apply_gates,
};
pub use edit_planner::{
    EditValidationError, build_edit_plan, edit_plan_summary_line, validate_edit_request,
};
pub use envelope::{TrustEnvelopeInput, format_trust_envelope};
pub use executor::{
    COMPACT_SERVE_EXPLORE_MAX_TOKENS, COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT,
    COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE, ServedStepResult, apply_compact_serve_caps,
    apply_degrade_to_plan, chain_failure_decision, extract_served_step_bodies, format_bypass_body,
    format_cache_hit_body, format_multi_step_serve_body, format_partial_multi_step_serve_body,
    format_serve_step_meta, format_single_step_serve_body, is_degrade, is_enforced_bypass,
    is_pff_bypass_body, route_tool_label, serve_chain_outcome_class, serve_step_failed,
    serve_step_outcome, should_skip_legacy_dispatch, tools_executed,
};
pub use gates::{
    BatteryResults, BatteryRow, BatteryRowStel, GateStatus, Phase2GateReport, Phase2GateStatuses,
    compute_phase2_gates, format_gate_report_markdown, h3_scope_rows, is_small_file_task_id,
    normalize_battery_results, phase2_minimum_gates_pass,
};
pub use golden_replay::{
    DEFERRED_MULTI_HOP_ROW_IDS, GOLDEN_ROUTES_FIXTURE, GoldenCorpusClassification,
    GoldenReplayCategory, MULTI_HOP_GOLDEN_ROW_IDS, MULTI_HOP_REPLAY_CORPUS_ROOT, ReplayValidation,
    S4_EXIT_ROW_IDS, S4_REPLAY_CORPUS, classify_golden_corpus, classify_golden_row,
    corpus_for_row_id, corpus_marker_for_row_id, load_golden_rows,
    multi_hop_replay_corpus_for_row_id, multi_hop_replay_corpus_marker, parse_golden_rows,
    request_for_golden_row, s4_exit_rows, supported_pff_rows, supported_serve_rows,
    validate_pff_replay_output, validate_s4_replay_output, validate_serve_replay_output,
};
pub use handler::{
    DecisionEnvelopeMetrics, StubServeMetrics, envelope_for_decision, envelope_for_stub_serve,
    estimate_tokens, finalize_symforge_output, format_preview_body, format_preview_body_for_plan,
    format_preview_estimate, metrics_for_decision, prepend_envelope, stub_plan_summary,
};
pub use ledger::{
    LedgerCaptureInput, LedgerEnvelopeMeta, SessionLedger, build_ledger_event, capture_ledger,
    format_ledger_envelope_line,
};
pub use planner::{build_plan, confidence_label, plan_summary_line};
pub use status::{
    DEFERRED_ITEMS, PHASE0_EVIDENCE_COMMIT, PHASE0_GO_COMMIT, StelStatusContext, format_stel_status,
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
