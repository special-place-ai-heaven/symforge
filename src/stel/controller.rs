//! STEL L2 economics controller — evaluate [`StelPlan`] → [`StelDecision`] / [`StelEstimate`].
//!
//! Phase 2 P2-S3: normative admission states (`serve | degrade | bypass | cache_hit`).

use crate::protocol::session::SessionContext;

use super::types::{
    AdmissionDecision, GoldenRouteRow, RouteConfidence, StelBypassBody, StelCacheBody,
    StelDecision, StelEstimate, StelPlan, StelPlanStep, StelRequest,
};

/// Compact-3 worst-case schema tax per call (A-006 conservative path; no amortization credit).
pub const COMPACT_SCHEMA_TOKENS: u32 = 45;
/// Compact `symforge` invoke overhead per call (schema example + Phase 0 doctrine).
pub const COMPACT_INVOKE_TOKENS: u32 = 80;
/// Minimum predicted net vs manual before `serve` (schema `margin_high`).
pub const SERVE_MARGIN_TOKENS: i32 = 50;
/// Predicted net at or below this (but above zero) triggers `degrade` (`margin_low`).
pub const DEGRADE_MARGIN_LOW_TOKENS: i32 = SERVE_MARGIN_TOKENS;
/// Default capped response budget applied on degrade when request omits `max_tokens`.
pub const DEGRADE_DEFAULT_MAX_TOKENS: u32 = 400;

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
    evaluate_plan_with_session(request, plan, None)
}

/// Evaluate L2 admission with optional in-process session context for cache-hit detection.
pub fn evaluate_plan_with_session(
    request: &StelRequest,
    plan: &StelPlan,
    session: Option<&SessionContext>,
) -> StelDecision {
    if let Some(bypass) = detect_pff_bypass(request) {
        return decision_from_pff_bypass(plan, request, bypass);
    }

    if let Some(session) = session
        && let Some(cache) = detect_session_cache_hit(plan, session)
    {
        return decision_from_cache_hit(plan, request, cache);
    }

    let economics = estimate_economics(plan);
    let net = economics.predicted_net_vs_manual;

    if net <= 0 {
        let bypass = economics_bypass_body(plan, &economics);
        return StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Bypass,
            decision_reason: format!(
                "predicted_net={net} <= 0; economics bypass via host_read `{}`",
                bypass.path
            ),
            effective_max_tokens: request.max_tokens,
            degrade_flags: vec![],
            steps: None,
            bypass: Some(bypass),
            cache: None,
        };
    }

    let mandatory_degrade =
        plan.confidence == RouteConfidence::Fallback && net < SERVE_MARGIN_TOKENS;
    let economics_degrade = net <= DEGRADE_MARGIN_LOW_TOKENS;

    if mandatory_degrade || economics_degrade {
        let mut degrade_flags = Vec::new();
        if mandatory_degrade {
            degrade_flags.push("fallback_mandatory".to_string());
        }
        if plan
            .steps
            .iter()
            .any(|step| step.tool == "get_file_context")
        {
            degrade_flags.push("outline_only".to_string());
        } else {
            degrade_flags.push("max_tokens_cap".to_string());
        }
        degrade_flags.sort();
        degrade_flags.dedup();

        let effective_max_tokens = request
            .max_tokens
            .or(Some(DEGRADE_DEFAULT_MAX_TOKENS))
            .map(|cap| cap.min(DEGRADE_DEFAULT_MAX_TOKENS));

        let reason = if mandatory_degrade {
            format!(
                "predicted_net={net} < margin={}; fallback confidence requires degrade",
                SERVE_MARGIN_TOKENS
            )
        } else {
            format!(
                "predicted_net={net} <= margin_low={}",
                DEGRADE_MARGIN_LOW_TOKENS
            )
        };

        return StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Degrade,
            decision_reason: reason,
            effective_max_tokens,
            degrade_flags,
            steps: Some(plan.steps.clone()),
            bypass: None,
            cache: None,
        };
    }

    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision: AdmissionDecision::Serve,
        decision_reason: format!("predicted_net={net} > margin={}", SERVE_MARGIN_TOKENS),
        effective_max_tokens: request.max_tokens,
        degrade_flags: vec![],
        steps: Some(plan.steps.clone()),
        bypass: None,
        cache: None,
    }
}

fn decision_from_pff_bypass(
    plan: &StelPlan,
    request: &StelRequest,
    bypass: StelBypassBody,
) -> StelDecision {
    let economics = economics_for_bypass(&bypass);
    StelDecision {
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
    }
}

fn decision_from_cache_hit(
    plan: &StelPlan,
    request: &StelRequest,
    cache: StelCacheBody,
) -> StelDecision {
    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision: AdmissionDecision::CacheHit,
        decision_reason: format!(
            "session cache hit for {} `{}` (prior_tokens={})",
            cache.kind, cache.path, cache.prior_tokens
        ),
        effective_max_tokens: request.max_tokens,
        degrade_flags: vec![],
        steps: None,
        bypass: None,
        cache: Some(cache),
    }
}

fn detect_session_cache_hit(plan: &StelPlan, session: &SessionContext) -> Option<StelCacheBody> {
    let step = plan.steps.first()?;
    let age = session.session_age_secs();
    match step.tool.as_str() {
        "get_symbol" => {
            let name = step.args.get("name")?.as_str()?.trim();
            if name.is_empty() {
                return None;
            }
            let path = step
                .args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            if !path.is_empty() && session.has_symbol(path, name) {
                return Some(StelCacheBody {
                    kind: "symbol".to_string(),
                    path: path.to_string(),
                    name: name.to_string(),
                    prior_tokens: session.symbol_prior_tokens(path, name).unwrap_or(0),
                    session_age_secs: age,
                });
            }
            None
        }
        "get_file_context" | "get_file_content" => {
            let path = step.args.get("path")?.as_str()?.trim();
            if path.is_empty() || !session.has_file(path) {
                return None;
            }
            Some(StelCacheBody {
                kind: "file".to_string(),
                path: path.to_string(),
                name: String::new(),
                prior_tokens: session.file_prior_tokens(path).unwrap_or(0),
                session_age_secs: age,
            })
        }
        _ => None,
    }
}

fn economics_bypass_body(plan: &StelPlan, economics: &EconomicsBreakdown) -> StelBypassBody {
    let path = plan_primary_path(plan).unwrap_or_else(|| "unknown".to_string());
    StelBypassBody {
        action: "host_read".to_string(),
        path: path.clone(),
        start_line: 1,
        end_line: Some(80),
        predicted_manual_tokens: economics.predicted_manual_tokens,
        predicted_symforge_tokens: economics.predicted_symforge_tokens,
        reason: format!(
            "predicted_net={} <= 0; host_read `{path}` cheaper than serve",
            economics.predicted_net_vs_manual
        ),
    }
}

fn plan_primary_path(plan: &StelPlan) -> Option<String> {
    plan.steps.first().and_then(primary_path_from_step)
}

fn primary_path_from_step(step: &StelPlanStep) -> Option<String> {
    step.args
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .or_else(|| {
            step.args
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
}

/// Evaluate L2 admission for a structural edit plan (no NL P-FF bypass path).
pub fn evaluate_edit_plan(plan: &StelPlan) -> StelDecision {
    let economics = estimate_economics(plan);
    let net = economics.predicted_net_vs_manual;
    if net <= 0 {
        let bypass = economics_bypass_body(plan, &economics);
        return StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Bypass,
            decision_reason: format!("predicted_net={net} <= 0; edit economics bypass"),
            effective_max_tokens: None,
            degrade_flags: vec![],
            steps: Some(plan.steps.clone()),
            bypass: Some(bypass),
            cache: None,
        };
    }
    if net <= DEGRADE_MARGIN_LOW_TOKENS {
        return StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Degrade,
            decision_reason: format!(
                "predicted_net={net} <= margin_low={}",
                DEGRADE_MARGIN_LOW_TOKENS
            ),
            effective_max_tokens: Some(DEGRADE_DEFAULT_MAX_TOKENS),
            degrade_flags: vec!["max_tokens_cap".to_string()],
            steps: Some(plan.steps.clone()),
            bypass: None,
            cache: None,
        };
    }
    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision: AdmissionDecision::Serve,
        decision_reason: format!("predicted_net={net} > margin={}", SERVE_MARGIN_TOKENS),
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

    fn low_net_plan() -> StelPlan {
        StelPlan {
            plan_id: "low-net".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Inferred,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
                est_response_tokens: 900,
                est_manual_tokens: 100,
                index_refs: vec![],
            }],
            suggested_followup: None,
        }
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
    fn negative_net_non_pff_bypass_has_host_read_body() {
        let plan = low_net_plan();
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Bypass);
        let bypass = decision.bypass.as_ref().expect("economics bypass body");
        assert_eq!(bypass.path, "src/lib.rs");
        assert!(decision.decision_reason.contains("predicted_net="));
    }

    #[test]
    fn marginal_net_degrades_with_outline_only_flag() {
        let plan = StelPlan {
            plan_id: "degrade".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Inferred,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
                est_response_tokens: 400,
                est_manual_tokens: 530,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Degrade);
        assert!(decision.degrade_flags.contains(&"outline_only".to_string()));
        assert!(decision.effective_max_tokens.is_some());
    }

    #[test]
    fn fallback_confidence_forces_degrade_below_margin_high() {
        let plan = StelPlan {
            plan_id: "fallback".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Fallback,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_content".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
                est_response_tokens: 400,
                est_manual_tokens: 530,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Degrade);
        assert!(
            decision
                .degrade_flags
                .contains(&"fallback_mandatory".to_string())
        );
    }

    #[test]
    fn session_cache_hit_for_prefetched_symbol() {
        let session = SessionContext::new();
        session.record_symbol("src/lib.rs", "cfg_if", 128);
        let plan = StelPlan {
            plan_id: "cache".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Exact,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_symbol".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs", "name": "cfg_if" }),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan_with_session(&request, &plan, Some(&session));
        assert_eq!(decision.decision, AdmissionDecision::CacheHit);
        let cache = decision.cache.as_ref().expect("cache body");
        assert_eq!(cache.kind, "symbol");
        assert_eq!(cache.prior_tokens, 128);
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
