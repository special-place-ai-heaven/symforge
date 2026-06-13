//! STEL L2 economics controller — evaluate [`StelPlan`] → [`StelDecision`] / [`StelEstimate`].
//!
//! Phase 1 slice: economics scoring; P-FF bypass is enforced in [`super::executor`].

use super::types::{
    AdmissionDecision, GoldenRouteRow, StelBypassBody, StelDecision, StelEstimate, StelPlan,
    StelRequest,
};

/// Compact-3 worst-case schema tax per call (A-006 conservative path; no amortization credit).
pub const COMPACT_SCHEMA_TOKENS: u32 = 45;
/// Compact `symforge` invoke overhead per call (schema example + Phase 0 doctrine).
pub const COMPACT_INVOKE_TOKENS: u32 = 80;
/// Minimum predicted net vs manual before `serve` is recommended (schema example).
pub const SERVE_MARGIN_TOKENS: i32 = 50;

/// Token economics breakdown for one planned invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EconomicsBreakdown {
    pub predicted_response_tokens: u32,
    pub predicted_manual_tokens: u32,
    pub predicted_schema_tokens: u32,
    pub predicted_invoke_tokens: u32,
    pub predicted_symforge_tokens: u32,
    pub predicted_net_vs_manual: i32,
}

/// Evaluate L2 admission for a draft plan (does not execute L3).
pub fn evaluate_plan(request: &StelRequest, plan: &StelPlan) -> StelDecision {
    if let Some(bypass) = detect_pff_bypass(request) {
        let economics = economics_for_bypass(&bypass);
        return StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Bypass,
            decision_reason: format!(
                "{}; predicted_net={}",
                bypass.reason, economics.predicted_net_vs_manual
            ),
            effective_max_tokens: request.max_tokens,
            degrade_flags: vec![],
            steps: None,
            bypass: Some(bypass),
            cache: None,
        };
    }

    let economics = estimate_economics(plan);
    let recommended = economics.predicted_net_vs_manual > SERVE_MARGIN_TOKENS;
    let decision = if recommended {
        AdmissionDecision::Serve
    } else {
        // Metadata classification only — enforcement deferred to a later slice.
        AdmissionDecision::Bypass
    };
    let decision_reason = if recommended {
        format!(
            "predicted_net={} > margin={}",
            economics.predicted_net_vs_manual, SERVE_MARGIN_TOKENS
        )
    } else {
        format!(
            "predicted_net={} <= margin={} (non-P-FF bypass metadata; L3 not gated)",
            economics.predicted_net_vs_manual, SERVE_MARGIN_TOKENS
        )
    };

    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision,
        decision_reason,
        effective_max_tokens: request.max_tokens,
        degrade_flags: vec![],
        steps: Some(plan.steps.clone()),
        bypass: None,
        cache: None,
    }
}

/// Evaluate L2 admission for a structural edit plan (no NL P-FF bypass path).
pub fn evaluate_edit_plan(plan: &StelPlan) -> StelDecision {
    let economics = estimate_economics(plan);
    let recommended = economics.predicted_net_vs_manual > SERVE_MARGIN_TOKENS;
    let decision = if recommended {
        AdmissionDecision::Serve
    } else {
        AdmissionDecision::Bypass
    };
    let decision_reason = if recommended {
        format!(
            "predicted_net={} > margin={}",
            economics.predicted_net_vs_manual, SERVE_MARGIN_TOKENS
        )
    } else {
        format!(
            "predicted_net={} <= margin={} (non-P-FF bypass metadata; L3 not gated)",
            economics.predicted_net_vs_manual, SERVE_MARGIN_TOKENS
        )
    };
    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision,
        decision_reason,
        effective_max_tokens: None,
        degrade_flags: vec![],
        steps: Some(plan.steps.clone()),
        bypass: None,
        cache: None,
    }
}

/// Build preview economics for `preview: true` (L1+L2 only).
pub fn build_estimate(
    request: &StelRequest,
    plan: &StelPlan,
    decision: &StelDecision,
) -> StelEstimate {
    let economics = if decision.decision == AdmissionDecision::Bypass {
        decision
            .bypass
            .as_ref()
            .map(economics_for_bypass)
            .unwrap_or_else(|| estimate_economics(plan))
    } else {
        estimate_economics(plan)
    };
    let recommended = decision.decision == AdmissionDecision::Serve
        && economics.predicted_net_vs_manual > SERVE_MARGIN_TOKENS;
    let _ = request;
    StelEstimate {
        plan_id: plan.plan_id.clone(),
        decision: decision.decision,
        predicted_response_tokens: economics.predicted_response_tokens,
        predicted_manual_tokens: economics.predicted_manual_tokens,
        predicted_schema_tokens: economics.predicted_schema_tokens,
        predicted_invoke_tokens: economics.predicted_invoke_tokens,
        predicted_net_vs_manual: economics.predicted_net_vs_manual,
        recommended,
    }
}

/// Sum step estimates + compact surface overhead into a manual-vs-symforge comparison.
pub fn estimate_economics(plan: &StelPlan) -> EconomicsBreakdown {
    let predicted_response_tokens: u32 =
        plan.steps.iter().map(|step| step.est_response_tokens).sum();
    let predicted_manual_tokens: u32 = plan.steps.iter().map(|step| step.est_manual_tokens).sum();
    let predicted_symforge_tokens = predicted_response_tokens
        .saturating_add(COMPACT_SCHEMA_TOKENS)
        .saturating_add(COMPACT_INVOKE_TOKENS);
    let predicted_net_vs_manual = predicted_manual_tokens as i32 - predicted_symforge_tokens as i32;
    EconomicsBreakdown {
        predicted_response_tokens,
        predicted_manual_tokens,
        predicted_schema_tokens: COMPACT_SCHEMA_TOKENS,
        predicted_invoke_tokens: COMPACT_INVOKE_TOKENS,
        predicted_symforge_tokens,
        predicted_net_vs_manual,
    }
}

pub fn economics_for_bypass(bypass: &StelBypassBody) -> EconomicsBreakdown {
    let predicted_symforge_tokens = bypass
        .predicted_symforge_tokens
        .saturating_add(COMPACT_SCHEMA_TOKENS)
        .saturating_add(COMPACT_INVOKE_TOKENS);
    let predicted_net_vs_manual =
        bypass.predicted_manual_tokens as i32 - predicted_symforge_tokens as i32;
    EconomicsBreakdown {
        predicted_response_tokens: 0,
        predicted_manual_tokens: bypass.predicted_manual_tokens,
        predicted_schema_tokens: COMPACT_SCHEMA_TOKENS,
        predicted_invoke_tokens: COMPACT_INVOKE_TOKENS,
        predicted_symforge_tokens,
        predicted_net_vs_manual,
    }
}

/// Policy P-FF: whole-file review queries bypass SymForge serve (A-012 / golden corpus).
pub fn detect_pff_bypass(request: &StelRequest) -> Option<StelBypassBody> {
    let query = request.query.trim();
    let lower = query.to_ascii_lowercase();
    if !is_pff_query(&lower) {
        return None;
    }
    let path = extract_pff_path(query, &lower)?;
    Some(StelBypassBody {
        action: "host_read".to_string(),
        path: path.clone(),
        start_line: 1,
        end_line: None,
        predicted_manual_tokens: 320,
        predicted_symforge_tokens: COMPACT_SCHEMA_TOKENS + COMPACT_INVOKE_TOKENS + 256,
        reason: format!("policy=P-FF whole-file review; host_read `{path}`"),
    })
}

fn is_pff_query(lower: &str) -> bool {
    const PHRASES: &[&str] = &[
        "entire ",
        "whole ",
        "full file",
        "complete ",
        "line by line",
        "audit full ",
        "review entire ",
        "read complete ",
    ];
    PHRASES.iter().any(|phrase| lower.contains(phrase))
}

fn extract_pff_path(query: &str, lower: &str) -> Option<String> {
    for token in query.split_whitespace().rev() {
        let cleaned = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/');
        if cleaned.contains('.') && !cleaned.eq_ignore_ascii_case("line") {
            return Some(cleaned.to_string());
        }
    }
    for token in lower.split_whitespace() {
        if token.contains('.') {
            return Some(token.to_string());
        }
    }
    None
}

/// Golden-row helper: L2 decision should match fixture `expected_decision` for representative rows.
pub fn decision_matches_golden(
    row: &GoldenRouteRow,
    plan: &StelPlan,
    request: &StelRequest,
) -> bool {
    let decision = evaluate_plan(request, plan);
    decision.decision == row.expected_decision
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::stel::golden_replay::{GOLDEN_ROUTES_FIXTURE, load_golden_rows};
    use crate::stel::planner::build_plan;
    use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};

    fn row_by_id<'a>(rows: &'a [GoldenRouteRow], id: &str) -> &'a GoldenRouteRow {
        rows.iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("missing golden row {id}"))
    }

    fn request_for_row(row: &GoldenRouteRow) -> StelRequest {
        let mut request = row.to_request();
        request.intent = row.intent;
        request
    }

    #[test]
    fn serve_rows_classify_as_serve() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        let rows = load_golden_rows(&path).expect("golden fixture");
        for id in [
            "cfg-if/t1_search",
            "cfg-if/t4_refs",
            "records/t4_refs",
            "compression/t5_dependents",
        ] {
            let row = row_by_id(&rows, id);
            let request = request_for_row(row);
            let plan = build_plan(&request);
            let decision = evaluate_plan(&request, &plan);
            assert_eq!(
                decision.decision,
                AdmissionDecision::Serve,
                "{id}: {:?}",
                decision.decision_reason
            );
            assert!(row.eligible_h6.unwrap_or(false));
            let economics = estimate_economics(&plan);
            assert!(
                economics.predicted_net_vs_manual > SERVE_MARGIN_TOKENS,
                "{id} net {}",
                economics.predicted_net_vs_manual
            );
        }
    }

    #[test]
    fn pff_rows_classify_as_bypass() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        let rows = load_golden_rows(&path).expect("golden fixture");
        for id in [
            "cfg-if/pff_whole_lib",
            "records/pff_whole_module",
            "is-plain/pff_whole_index",
            "compression/pff_whole_service",
        ] {
            let row = row_by_id(&rows, id);
            let request = request_for_row(row);
            let plan = build_plan(&request);
            let decision = evaluate_plan(&request, &plan);
            assert_eq!(decision.decision, AdmissionDecision::Bypass, "{id}");
            assert!(decision.bypass.is_some(), "{id} bypass body");
            let bypass = decision.bypass.as_ref().expect("bypass body");
            assert_eq!(
                bypass.end_line, None,
                "{id} P-FF bypass must request whole-file host read"
            );
            assert_eq!(row.eligible_h6, Some(false));
            let estimate = build_estimate(&request, &plan, &decision);
            assert!(!estimate.recommended, "{id} should not recommend serve");
        }
    }

    #[test]
    fn eligible_h6_serve_rows_remain_eligible() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        let rows = load_golden_rows(&path).expect("golden fixture");
        let eligible: Vec<_> = rows
            .iter()
            .filter(|row| {
                row.eligible_h6 == Some(true) && row.expected_decision == AdmissionDecision::Serve
            })
            .collect();
        assert!(eligible.len() >= 20);
        for row in eligible.iter().take(6) {
            let request = request_for_row(row);
            let plan = build_plan(&request);
            assert!(
                decision_matches_golden(row, &plan, &request),
                "eligible serve row {} decision mismatch",
                row.id
            );
        }
    }

    #[test]
    fn preview_estimate_carries_schema_and_invoke_costs() {
        let request = StelRequest {
            query: "who references cfg_if".to_string(),
            ..Default::default()
        };
        let plan = build_plan(&request);
        let decision = evaluate_plan(&request, &plan);
        let estimate = build_estimate(&request, &plan, &decision);
        assert_eq!(estimate.predicted_schema_tokens, COMPACT_SCHEMA_TOKENS);
        assert_eq!(estimate.predicted_invoke_tokens, COMPACT_INVOKE_TOKENS);
        assert_eq!(estimate.decision, AdmissionDecision::Serve);
        assert!(estimate.recommended);
    }

    #[test]
    fn fallback_confidence_still_serves_when_net_positive() {
        let plan = StelPlan {
            plan_id: "test".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Fallback,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({}),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Serve);
    }
}
