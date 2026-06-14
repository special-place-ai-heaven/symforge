//! Observational calibration summary derived from in-memory [`SessionLedger`] events.
//!
//! Read-only: does not adjust L2 margins, fudge multipliers, or route decisions.

use super::controller::{COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS};
use super::types::{AdmissionDecision, StelLedgerEvent};

/// Minimum ledger rows before offline review is considered sample-adequate.
pub const TUNING_REVIEW_MIN_EVENTS: usize = 5;

/// Aggregated session calibration metrics (observational only).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StelCalibrationSummary {
    pub total_events: usize,
    pub serve_count: usize,
    pub bypass_count: usize,
    pub pff_bypass_count: usize,
    pub legacy_executed_count: usize,
    pub total_schema_tokens: u64,
    pub total_invoke_tokens: u64,
    pub total_predicted_net: i64,
    pub total_predicted_response_tokens: u64,
    pub total_actual_response_tokens: u64,
    pub tuning_note: String,
}

/// Summarize ledger events for observational calibration feedback.
pub fn summarize_calibration(events: &[StelLedgerEvent]) -> StelCalibrationSummary {
    let mut summary = StelCalibrationSummary {
        total_events: events.len(),
        serve_count: 0,
        bypass_count: 0,
        pff_bypass_count: 0,
        legacy_executed_count: 0,
        total_schema_tokens: 0,
        total_invoke_tokens: 0,
        total_predicted_net: 0,
        total_predicted_response_tokens: 0,
        total_actual_response_tokens: 0,
        tuning_note: String::new(),
    };

    for event in events {
        match event.decision {
            AdmissionDecision::Serve => summary.serve_count += 1,
            AdmissionDecision::Bypass => {
                summary.bypass_count += 1;
                if event.tools_called.is_empty() {
                    summary.pff_bypass_count += 1;
                }
            }
            _ => {}
        }
        if !event.tools_called.is_empty() {
            summary.legacy_executed_count += 1;
        }
        summary.total_schema_tokens += u64::from(COMPACT_SCHEMA_TOKENS);
        summary.total_invoke_tokens += u64::from(COMPACT_INVOKE_TOKENS);
        summary.total_predicted_net += i64::from(event.net_vs_manual);
        summary.total_predicted_response_tokens += u64::from(event.predicted_response_tokens);
        summary.total_actual_response_tokens += u64::from(event.actual_response_tokens);
    }

    summary.tuning_note = tuning_sufficiency_note(summary.total_events);
    summary
}

fn tuning_sufficiency_note(total_events: usize) -> String {
    match total_events {
        0 => "insufficient: no ledger events; auto-tuning deferred".to_string(),
        n if n < TUNING_REVIEW_MIN_EVENTS => {
            format!("insufficient: {n} events (<{TUNING_REVIEW_MIN_EVENTS}); observational only")
        }
        n => format!(
            "observational: {n} events adequate for offline review; auto-tuning still deferred"
        ),
    }
}

/// Stable text block embedded in `status` `detail: full` output.
pub fn format_calibration_section(summary: &StelCalibrationSummary) -> String {
    let lines = [
        "── calibration (observational) ──".to_string(),
        format!("events: {}", summary.total_events),
        format!("serve: {}", summary.serve_count),
        format!("bypass: {}", summary.bypass_count),
        format!("pff_bypass: {}", summary.pff_bypass_count),
        format!("legacy_executed: {}", summary.legacy_executed_count),
        format!("schema_tokens: {}", summary.total_schema_tokens),
        format!("invoke_tokens: {}", summary.total_invoke_tokens),
        format!("predicted_net_total: {}", summary.total_predicted_net),
        format!(
            "predicted_response_tokens: {}",
            summary.total_predicted_response_tokens
        ),
        format!(
            "actual_response_tokens: {}",
            summary.total_actual_response_tokens
        ),
        format!("tuning: {}", summary.tuning_note),
        "──".to_string(),
    ];
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::controller::{economics_for_bypass, estimate_economics, evaluate_plan};
    use crate::stel::ledger::{LedgerCaptureInput, capture_ledger};
    use crate::stel::planner::build_plan;
    use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest};

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
        assert_eq!(summary.bypass_count, 0);
        assert_eq!(summary.pff_bypass_count, 0);
        assert_eq!(summary.legacy_executed_count, 0);
        assert_eq!(summary.total_schema_tokens, 0);
        assert_eq!(summary.total_invoke_tokens, 0);
        assert_eq!(summary.total_predicted_net, 0);
        assert!(summary.tuning_note.contains("no ledger events"));
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
        assert!(summary.tuning_note.contains("insufficient"));
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
        assert!(section.contains("legacy_executed: 1"));
        assert!(section.contains("tuning:"));
    }
}
