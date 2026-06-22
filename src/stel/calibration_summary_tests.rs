//! Server-only fixture-driven tests for the observational calibration summary.
//!
//! D3-ROOT extract-up: `crate::stel_core::calibration` holds the protocol-free
//! calibration code + its PURE-MATH tests. The fixture-driven tests below build
//! `StelLedgerEvent`s through `controller::evaluate_plan`, `ledger::capture_ledger`,
//! and `planner::build_plan` — all server-only — so they live here under the
//! server-gated `stel` tree rather than in `stel_core` (where they would drag
//! the protocol stack into the embed build). Moved verbatim from the original
//! `stel::calibration` test module; behavior-preserving.

use crate::stel::calibration::{
    CalibrationVerdict, TUNING_MIN_SAMPLES, format_calibration_section, summarize_calibration,
};
use crate::stel::controller::{
    COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS, economics_for_bypass, estimate_economics,
    evaluate_plan,
};
use crate::stel::ledger::{LedgerCaptureInput, capture_ledger};
use crate::stel::planner::build_plan;
use crate::stel::types::{
    IntentBucket, RouteConfidence, StelLedgerEvent, StelPlan, StelPlanStep, StelRequest,
};

fn serve_event() -> StelLedgerEvent {
    let plan = StelPlan {
        plan_id: "plan-serve".to_string(),
        intent: IntentBucket::Trace,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "find_references".to_string(),
            args: serde_json::json!({ "name": "cfg_if" }),
            est_response_tokens: 400,
            est_manual_tokens: 800,
            index_refs: vec![],
        }],
        suggested_followup: None,
    };
    let request = StelRequest {
        query: "who references cfg_if".to_string(),
        ..Default::default()
    };
    let decision = evaluate_plan(&request, &plan);
    let economics = estimate_economics(&plan);
    capture_ledger(&LedgerCaptureInput {
        plan: &plan,
        decision: &decision,
        economics: &economics,
        selected_tool: "find_references",
        tools_called: None,
        legacy_executed: true,
        output_body: "Chosen tool: find_references\n\nrefs",
        surface: "symforge",
    })
    .0
}

fn pff_bypass_event() -> StelLedgerEvent {
    let request = StelRequest {
        query: "review entire lib.rs for security".to_string(),
        ..Default::default()
    };
    let plan = build_plan(&request);
    let decision = evaluate_plan(&request, &plan);
    let economics = economics_for_bypass(decision.bypass.as_ref().expect("pff bypass"));
    capture_ledger(&LedgerCaptureInput {
        plan: &plan,
        decision: &decision,
        economics: &economics,
        selected_tool: plan.steps[0].tool.as_str(),
        tools_called: None,
        legacy_executed: false,
        output_body: "Decision: bypass\nSymForge did not execute a legacy tool",
        surface: "symforge",
    })
    .0
}

#[test]
fn empty_ledger_summary_is_zeroed_with_insufficient_note() {
    let summary = summarize_calibration(&[]);
    assert_eq!(summary.total_events, 0);
    assert_eq!(summary.serve_count, 0);
    assert_eq!(summary.degrade_count, 0);
    assert_eq!(summary.bypass_count, 0);
    assert_eq!(summary.cache_hit_count, 0);
    assert_eq!(summary.pff_bypass_count, 0);
    assert_eq!(summary.legacy_executed_count, 0);
    assert_eq!(summary.total_schema_tokens, 0);
    assert_eq!(summary.total_invoke_tokens, 0);
    assert_eq!(summary.total_predicted_net, 0);
    // T030: zero events -> honest `deferred` verdict (no hard-coded note).
    assert_eq!(summary.verdict, CalibrationVerdict::Deferred);
    assert_eq!(summary.tuning_note, "deferred");
}

#[test]
fn serve_only_ledger_aggregates_economics() {
    let event = serve_event();
    let summary = summarize_calibration(&[event]);
    assert_eq!(summary.total_events, 1);
    assert_eq!(summary.serve_count, 1);
    assert_eq!(summary.bypass_count, 0);
    assert_eq!(summary.pff_bypass_count, 0);
    assert_eq!(summary.legacy_executed_count, 1);
    assert_eq!(
        summary.total_schema_tokens,
        u64::from(COMPACT_SCHEMA_TOKENS)
    );
    assert_eq!(
        summary.total_invoke_tokens,
        u64::from(COMPACT_INVOKE_TOKENS)
    );
    assert!(summary.total_predicted_net != 0);
    assert!(summary.total_actual_response_tokens > 0);
    // T030: one event is below the tuning minimum -> `accumulating (1/min)`,
    // never `tuned` and never a hard-coded "insufficient" string.
    assert_eq!(
        summary.verdict,
        CalibrationVerdict::Accumulating {
            n: 1,
            min: TUNING_MIN_SAMPLES
        }
    );
    assert!(summary.tuning_note.starts_with("accumulating (1/"));
}

#[test]
fn mixed_serve_and_bypass_ledger_counts_pff() {
    let serve = serve_event();
    let bypass = pff_bypass_event();
    let summary = summarize_calibration(&[serve, bypass]);
    assert_eq!(summary.total_events, 2);
    assert_eq!(summary.serve_count, 1);
    assert_eq!(summary.bypass_count, 1);
    assert_eq!(summary.pff_bypass_count, 1);
    assert_eq!(summary.legacy_executed_count, 1);
    assert_eq!(
        summary.total_schema_tokens,
        u64::from(COMPACT_SCHEMA_TOKENS) * 2
    );
    assert_eq!(
        summary.total_invoke_tokens,
        u64::from(COMPACT_INVOKE_TOKENS) * 2
    );
}

#[test]
fn calibration_section_is_stable_text() {
    let summary = summarize_calibration(&[serve_event()]);
    let section = format_calibration_section(&summary);
    assert!(section.contains("── calibration (observational) ──"));
    assert!(section.contains("serve: 1"));
    assert!(section.contains("degrade: 0"));
    assert!(section.contains("cache_hit: 0"));
    assert!(section.contains("legacy_executed: 1"));
    assert!(section.contains("tuning:"));
    // T033: the honest verdict line is present and, for one sample, reads
    // `accumulating` — never `tuned`/`validated`/`saved`/`active`.
    assert!(section.contains("calibration: accumulating"));
    for forbidden in ["tuned", "validated", "saved", ": active"] {
        assert!(
            !section.contains(forbidden),
            "single-sample section must not read `{forbidden}`:\n{section}"
        );
    }
}
