//! STEL L3 enforcement — respect L2 admission; in-process multi-step serve chain.

use serde_json;

use crate::protocol::result_status::OutcomeClass;
use crate::protocol::tools::{classify_compact_tool_output, compact_tool_output_is_success};

use super::controller::DEGRADE_DEFAULT_MAX_TOKENS;

/// Competent-manual token budget for Phase 2 H3 explore golden rows (4000-char window).
pub const H3_EXPLORE_MANUAL_TOKENS: u32 = 1000;
/// Reserve STEL envelope + serve routing meta when capping explore on compact serve.
pub const COMPACT_SERVE_EXPLORE_OVERHEAD_TOKENS: u32 = 250;
/// Max explore tool tokens on compact `symforge` serve so full response stays within H3 window.
pub const COMPACT_SERVE_EXPLORE_MAX_TOKENS: u32 =
    H3_EXPLORE_MANUAL_TOKENS.saturating_sub(COMPACT_SERVE_EXPLORE_OVERHEAD_TOKENS);
/// TX-01 / FM-CAP: compact serve `find_references` file budget (schema max).
pub const COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT: u32 = 100;
/// TX-01: per-file hit budget on compact serve trace (handler default when unset).
pub const COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE: u32 = 10;
use super::planner::confidence_label;
use super::types::{
    AdmissionDecision, StelBypassBody, StelCacheBody, StelDecision, StelPlan, StelPlanStep,
};

/// Outcome of one in-process legacy tool dispatch during a multi-step serve chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServedStepResult {
    pub tool: String,
    pub body: String,
}

/// Whether L3 should skip legacy tool dispatch (`bypass` or `cache_hit`).
pub fn should_skip_legacy_dispatch(decision: &StelDecision) -> bool {
    matches!(
        decision.decision,
        AdmissionDecision::Bypass if decision.bypass.is_some()
    ) || matches!(
        decision.decision,
        AdmissionDecision::CacheHit if decision.cache.is_some()
    )
}

/// Back-compat alias — enforced bypass/cache_hit paths must not execute legacy tools.
pub fn is_enforced_bypass(decision: &StelDecision) -> bool {
    should_skip_legacy_dispatch(decision)
}

/// Whether the bypass body is policy P-FF (whole-file host read).
pub fn is_pff_bypass_body(bypass: &StelBypassBody) -> bool {
    bypass.reason.contains("policy=P-FF")
}

/// Whether L2 chose a degrade admission (caps applied before L3 dispatch).
pub fn is_degrade(decision: &StelDecision) -> bool {
    decision.decision == AdmissionDecision::Degrade
}

/// Human-readable bypass body plus machine-readable [`StelBypassBody`] JSON.
pub fn format_bypass_body(decision: &StelDecision) -> String {
    let bypass = decision
        .bypass
        .as_ref()
        .expect("enforced bypass requires StelBypassBody");
    format_bypass_body_from(bypass, &decision.decision_reason)
}

fn format_host_read_line(bypass: &StelBypassBody) -> String {
    match bypass.end_line {
        Some(end) => format!(
            "Host read: `{}` lines {}-{end}",
            bypass.path, bypass.start_line
        ),
        None => format!("Host read: `{}` (whole file)", bypass.path),
    }
}

fn format_bypass_body_from(bypass: &StelBypassBody, decision_reason: &str) -> String {
    let json = serde_json::to_string_pretty(bypass).expect("StelBypassBody serializes");
    let host_read = format_host_read_line(bypass);
    let guidance = if bypass.end_line.is_none() {
        format!(
            "Open `{path}` in your editor and review the file directly for whole-file tasks.",
            path = bypass.path
        )
    } else {
        format!(
            "Open `{path}` in your editor and read the suggested line range directly.",
            path = bypass.path
        )
    };
    format!(
        "Decision: bypass\n\
         Economics: bypass ({decision_reason})\n\
         Action: {}\n\
         {host_read}\n\
         Predicted manual tokens: {}\n\
         Predicted SymForge tokens avoided: {}\n\
         \n\
         SymForge did not execute a legacy tool for this request.\n\
         {guidance}\n\
         \n\
         --- bypass payload ---\n\
         {json}",
        bypass.action, bypass.predicted_manual_tokens, bypass.predicted_symforge_tokens,
    )
}

/// Human-readable cache-hit body plus machine-readable [`StelCacheBody`] JSON.
pub fn format_cache_hit_body(decision: &StelDecision) -> String {
    let cache = decision
        .cache
        .as_ref()
        .expect("cache_hit requires StelCacheBody");
    format_cache_hit_body_from(cache, &decision.decision_reason)
}

fn format_cache_hit_body_from(cache: &StelCacheBody, decision_reason: &str) -> String {
    let json = serde_json::to_string_pretty(cache).expect("StelCacheBody serializes");
    let target = if cache.name.is_empty() {
        format!("file `{}`", cache.path)
    } else {
        format!("symbol `{}` in `{}`", cache.name, cache.path)
    };
    format!(
        "Decision: cache_hit\n\
         Economics: cache_hit ({decision_reason})\n\
         Session cache: {} {target} (prior_tokens={}, session_age_secs={})\n\
         \n\
         SymForge did not re-execute a legacy tool for this request.\n\
         Reuse the content already loaded in this session.\n\
         \n\
         --- cache payload ---\n\
         {json}",
        cache.kind, cache.prior_tokens, cache.session_age_secs,
    )
}

/// Apply L2 degrade caps to a plan before L3 dispatch.
pub fn apply_degrade_to_plan(plan: &StelPlan, decision: &StelDecision) -> StelPlan {
    let mut degraded = plan.clone();
    let cap = decision
        .effective_max_tokens
        .unwrap_or(DEGRADE_DEFAULT_MAX_TOKENS);
    let outline_only = decision
        .degrade_flags
        .iter()
        .any(|flag| flag == "outline_only");
    let max_tokens_cap = decision
        .degrade_flags
        .iter()
        .any(|flag| flag == "max_tokens_cap");

    for step in &mut degraded.steps {
        let Some(args) = step.args.as_object_mut() else {
            continue;
        };
        if outline_only && step.tool == "get_file_context" {
            args.insert("sections".to_string(), serde_json::json!(["outline"]));
        }
        if max_tokens_cap && supports_max_tokens_cap(&step.tool) {
            let existing = args.get("max_tokens").and_then(|value| value.as_u64());
            let capped = existing
                .map(|value| value.min(u64::from(cap)))
                .unwrap_or(u64::from(cap));
            args.insert("max_tokens".to_string(), serde_json::json!(capped));
        }
    }
    degraded
}

/// Apply compact-surface serve caps before L3 dispatch (H3 explore guidance budget).
pub fn apply_compact_serve_caps(plan: &StelPlan, decision: &StelDecision) -> StelPlan {
    if decision.decision != AdmissionDecision::Serve {
        return plan.clone();
    }
    let mut capped = plan.clone();
    for step in &mut capped.steps {
        let Some(args) = step.args.as_object_mut() else {
            continue;
        };
        match step.tool.as_str() {
            "explore" => {
                let cap = u64::from(COMPACT_SERVE_EXPLORE_MAX_TOKENS);
                let capped_val = args
                    .get("max_tokens")
                    .and_then(|value| value.as_u64())
                    .map(|value| value.min(cap))
                    .unwrap_or(cap);
                args.insert("max_tokens".to_string(), serde_json::json!(capped_val));
            }
            "find_references" => {
                args.insert(
                    "limit".to_string(),
                    serde_json::json!(COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT),
                );
                args.insert(
                    "max_per_file".to_string(),
                    serde_json::json!(COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE),
                );
            }
            _ => {}
        }
    }
    capped
}

fn supports_max_tokens_cap(tool: &str) -> bool {
    matches!(
        tool,
        "get_file_context"
            | "get_file_content"
            | "get_symbol"
            | "get_symbol_context"
            | "search_symbols"
            | "search_text"
            | "find_references"
            | "find_dependents"
            | "get_repo_map"
            | "explore"
            | "ask"
    )
}

/// Routing metadata for one planned step in a serve chain.
pub fn format_serve_step_meta(
    plan: &StelPlan,
    step: &StelPlanStep,
    step_index: usize,
    decision: &StelDecision,
) -> String {
    let invocation = serde_json::to_string(&step.args).unwrap_or_else(|_| "{}".to_string());
    let rationale = if step_index == 0 {
        plan.confidence_rationale.as_str()
    } else {
        "multi-hop chain step"
    };
    let economics =
        if decision.decision == AdmissionDecision::Degrade && !decision.degrade_flags.is_empty() {
            format!(
                "{} ({}) flags=[{}]",
                decision.decision.as_str(),
                decision.decision_reason,
                decision.degrade_flags.join(",")
            )
        } else {
            format!(
                "{} ({})",
                decision.decision.as_str(),
                decision.decision_reason
            )
        };
    format!(
        "Step {}: Route confidence: {}\nChosen tool: {}\nInvocation: {}\nRationale: {}\nEconomics: {economics}",
        step_index + 1,
        confidence_label(plan.confidence),
        step.tool,
        invocation,
        rationale,
    )
}

/// Single-step serve body (routing meta + tool output).
pub fn format_single_step_serve_body(
    plan: &StelPlan,
    decision: &StelDecision,
    step: &StelPlanStep,
    tool_body: &str,
) -> String {
    format!(
        "{}\n\n{}",
        format_serve_step_meta(plan, step, 0, decision),
        tool_body
    )
}

/// Multi-step serve body — completed chain with per-step routing metadata.
pub fn format_multi_step_serve_body(
    plan: &StelPlan,
    decision: &StelDecision,
    step_results: &[ServedStepResult],
) -> String {
    format_partial_multi_step_serve_body(plan, decision, step_results, None)
}

/// Partial or complete multi-step body. Appends a chain-failure footer when provided.
pub fn format_partial_multi_step_serve_body(
    plan: &StelPlan,
    decision: &StelDecision,
    step_results: &[ServedStepResult],
    chain_failure: Option<&str>,
) -> String {
    let mut sections = Vec::new();
    for (index, result) in step_results.iter().enumerate() {
        sections.push(format_serve_step_meta(
            plan,
            &plan.steps[index],
            index,
            decision,
        ));
        sections.push(String::new());
        sections.push(result.body.clone());
    }
    if let Some(reason) = chain_failure {
        sections.push(String::new());
        sections.push(format!("Multi-hop chain failed: {reason}"));
    }
    sections.join("\n")
}

/// Tools executed during a serve chain (for ledger + battery extension).
pub fn tools_executed(step_results: &[ServedStepResult]) -> Vec<String> {
    step_results
        .iter()
        .map(|result| result.tool.clone())
        .collect()
}

/// Compact route label for ledger metadata when multiple tools ran in-process.
pub fn route_tool_label(tools: &[String]) -> String {
    tools.join("+")
}

/// Structured outcome for one in-process legacy tool dispatch.
pub fn serve_step_outcome(tool: &str, tool_body: &str) -> OutcomeClass {
    classify_compact_tool_output(tool, tool_body)
}

/// Whether a dispatched tool body indicates mid-chain failure (fail fast).
pub fn serve_step_failed(tool: &str, tool_body: &str) -> bool {
    !compact_tool_output_is_success(tool, tool_body)
}

/// Map a failed legacy tool outcome to MCP result status for compact `symforge`.
pub fn serve_chain_outcome_class(outcome: OutcomeClass) -> OutcomeClass {
    match outcome {
        OutcomeClass::InternalFailure => OutcomeClass::InternalFailure,
        OutcomeClass::InvalidRequest => OutcomeClass::InvalidRequest,
        OutcomeClass::NotFound | OutcomeClass::Ambiguous | OutcomeClass::EmptyResult => {
            OutcomeClass::InvalidRequest
        }
        OutcomeClass::Found => OutcomeClass::Found,
    }
}

/// Build the L2 decision recorded when an in-process serve chain fails mid-flight.
pub fn chain_failure_decision(
    plan: &StelPlan,
    base: &StelDecision,
    failed_step_index: usize,
    failed_tool: &str,
    outcome: OutcomeClass,
) -> StelDecision {
    let mut decision = base.clone();
    decision.decision = AdmissionDecision::Reject;
    decision.decision_reason = format!(
        "multi-hop chain failed at step {} tool={} outcome={}",
        failed_step_index + 1,
        failed_tool,
        outcome.as_str()
    );
    decision.steps = Some(plan.steps[..=failed_step_index].to_vec());
    decision.bypass = None;
    decision.cache = None;
    decision
}

/// Extract `(tool, body)` pairs from compact `symforge` serve output for replay validation.
pub fn extract_served_step_bodies(output: &str) -> Vec<(String, String)> {
    let mut steps = Vec::new();
    for segment in output.split("Chosen tool: ").skip(1) {
        let Some((tool_line, rest)) = segment.split_once('\n') else {
            continue;
        };
        let tool = tool_line.trim().to_string();
        let body = rest
            .split_once("\n\n")
            .map(|(_, body)| {
                body.split("\n\nStep ")
                    .next()
                    .unwrap_or(body)
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        steps.push((tool, body));
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::controller::evaluate_plan;
    use crate::stel::planner::build_plan;
    use crate::stel::types::StelRequest;

    fn pff_decision(query: &str) -> StelDecision {
        let request = StelRequest {
            query: query.to_string(),
            ..Default::default()
        };
        let plan = build_plan(&request);
        evaluate_plan(&request, &plan)
    }

    #[test]
    fn enforced_bypass_requires_pff_body() {
        let decision = pff_decision("review entire lib.rs for security");
        assert!(is_enforced_bypass(&decision));
        let body = format_bypass_body(&decision);
        assert!(body.contains("Decision: bypass"));
        assert!(body.contains("Host read: `lib.rs` (whole file)"));
        assert!(body.contains("did not execute a legacy tool"));
        assert!(!body.contains("Chosen tool:"));
    }

    #[test]
    fn negative_net_economics_bypass_skips_legacy_dispatch() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
        let plan = StelPlan {
            plan_id: "x".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Fallback,
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
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Bypass);
        assert!(is_enforced_bypass(&decision));
        let bypass = decision.bypass.as_ref().expect("economics bypass body");
        assert!(!is_pff_bypass_body(bypass));
        let body = format_bypass_body(&decision);
        assert!(body.contains("lines 1-80"));
        assert!(!body.contains("(whole file)"));
    }

    #[test]
    fn cache_hit_skips_legacy_dispatch() {
        use crate::protocol::session::SessionContext;
        use crate::stel::controller::evaluate_plan_with_session;
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};

        let session = SessionContext::new();
        session.record_symbol("src/lib.rs", "cfg_if", 96);
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
        assert!(should_skip_legacy_dispatch(&decision));
        let body = format_cache_hit_body(&decision);
        assert!(body.contains("Decision: cache_hit"));
        assert!(body.contains("did not re-execute a legacy tool"));
    }

    #[test]
    fn apply_degrade_caps_outline_only_for_file_context() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
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
        let degraded = apply_degrade_to_plan(&plan, &decision);
        let args = degraded.steps[0].args.as_object().expect("object args");
        assert_eq!(args["sections"], serde_json::json!(["outline"]));
    }

    #[test]
    fn apply_compact_serve_caps_explore_max_tokens_for_h3_window() {
        use crate::stel::types::{
            AdmissionDecision, IntentBucket, RouteConfidence, StelDecision, StelPlan, StelPlanStep,
        };
        let plan = StelPlan {
            plan_id: "serve-explore".to_string(),
            intent: IntentBucket::Orient,
            confidence: RouteConfidence::Inferred,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "explore".to_string(),
                args: serde_json::json!({ "query": "how to use records ORM", "depth": 2 }),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let decision = StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Serve,
            decision_reason: "test".to_string(),
            effective_max_tokens: None,
            degrade_flags: vec![],
            steps: Some(plan.steps.clone()),
            bypass: None,
            cache: None,
        };
        let capped = apply_compact_serve_caps(&plan, &decision);
        let args = capped.steps[0].args.as_object().expect("object args");
        assert_eq!(
            args["max_tokens"],
            serde_json::json!(COMPACT_SERVE_EXPLORE_MAX_TOKENS)
        );
    }

    #[test]
    fn apply_compact_serve_caps_find_references_tx01_file_limit() {
        use crate::stel::types::{
            AdmissionDecision, IntentBucket, RouteConfidence, StelDecision, StelPlan, StelPlanStep,
        };
        let plan = StelPlan {
            plan_id: "serve-refs".to_string(),
            intent: IntentBucket::Trace,
            confidence: RouteConfidence::Exact,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "find_references".to_string(),
                args: serde_json::json!({ "name": "spawn", "compact": true }),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let decision = StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Serve,
            decision_reason: "test".to_string(),
            effective_max_tokens: None,
            degrade_flags: vec![],
            steps: Some(plan.steps.clone()),
            bypass: None,
            cache: None,
        };
        let capped = apply_compact_serve_caps(&plan, &decision);
        let args = capped.steps[0].args.as_object().expect("object args");
        assert_eq!(
            args["limit"],
            serde_json::json!(COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT)
        );
        assert_eq!(
            args["max_per_file"],
            serde_json::json!(COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE)
        );
    }

    #[test]
    fn apply_compact_serve_caps_skips_find_references_on_non_serve() {
        use crate::stel::types::{
            AdmissionDecision, IntentBucket, RouteConfidence, StelDecision, StelPlan, StelPlanStep,
        };
        let plan = StelPlan {
            plan_id: "bypass-refs".to_string(),
            intent: IntentBucket::Trace,
            confidence: RouteConfidence::Exact,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "find_references".to_string(),
                args: serde_json::json!({ "name": "spawn", "compact": true }),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let decision = StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Bypass,
            decision_reason: "test".to_string(),
            effective_max_tokens: None,
            degrade_flags: vec![],
            steps: None,
            bypass: None,
            cache: None,
        };
        let capped = apply_compact_serve_caps(&plan, &decision);
        let args = capped.steps[0].args.as_object().expect("object args");
        assert!(args.get("limit").is_none());
    }

    #[test]
    fn apply_degrade_caps_max_tokens_for_content_reads() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
        let plan = StelPlan {
            plan_id: "degrade".to_string(),
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
        let degraded = apply_degrade_to_plan(&plan, &decision);
        let args = degraded.steps[0].args.as_object().expect("object args");
        assert_eq!(args["max_tokens"], serde_json::json!(400));
    }

    #[test]
    fn serve_step_failed_detects_core_read_tool_failures() {
        assert!(serve_step_failed(
            "get_symbol",
            "File not found: src/missing.rs"
        ));
        assert!(serve_step_failed(
            "find_references",
            "Symbol not found: Connection"
        ));
        assert!(serve_step_failed(
            "search_symbols",
            "No symbols matching `missing`"
        ));
        assert!(serve_step_failed(
            "get_file_content",
            "File not found: test.js"
        ));
        assert!(!serve_step_failed(
            "search_symbols",
            "1. cfg_if (src/lib.rs)"
        ));
    }

    #[test]
    fn chain_failure_decision_is_reject_not_serve() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
        let plan = StelPlan {
            plan_id: "multi".to_string(),
            intent: IntentBucket::Find,
            confidence: RouteConfidence::Inferred,
            confidence_rationale: "multi-hop".to_string(),
            steps: vec![
                StelPlanStep {
                    order: 1,
                    tool: "search_symbols".to_string(),
                    args: serde_json::json!({ "query": "cfg_if" }),
                    est_response_tokens: 400,
                    est_manual_tokens: 800,
                    index_refs: vec![],
                },
                StelPlanStep {
                    order: 2,
                    tool: "get_symbol".to_string(),
                    args: serde_json::json!({ "name": "cfg_if" }),
                    est_response_tokens: 400,
                    est_manual_tokens: 800,
                    index_refs: vec![],
                },
            ],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let base = evaluate_plan(&request, &plan);
        let failed = chain_failure_decision(&plan, &base, 1, "get_symbol", OutcomeClass::NotFound);
        assert_eq!(failed.decision, AdmissionDecision::Reject);
        assert!(failed.decision_reason.contains("get_symbol"));
    }

    #[test]
    fn multi_step_serve_body_lists_each_chosen_tool() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
        let plan = StelPlan {
            plan_id: "multi".to_string(),
            intent: IntentBucket::Find,
            confidence: RouteConfidence::Inferred,
            confidence_rationale: "multi-hop".to_string(),
            steps: vec![
                StelPlanStep {
                    order: 1,
                    tool: "search_symbols".to_string(),
                    args: serde_json::json!({ "query": "cfg_if" }),
                    est_response_tokens: 400,
                    est_manual_tokens: 800,
                    index_refs: vec![],
                },
                StelPlanStep {
                    order: 2,
                    tool: "get_symbol".to_string(),
                    args: serde_json::json!({ "name": "cfg_if" }),
                    est_response_tokens: 400,
                    est_manual_tokens: 800,
                    index_refs: vec![],
                },
            ],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        let body = format_multi_step_serve_body(
            &plan,
            &decision,
            &[
                ServedStepResult {
                    tool: "search_symbols".to_string(),
                    body: "symbols".to_string(),
                },
                ServedStepResult {
                    tool: "get_symbol".to_string(),
                    body: "symbol body".to_string(),
                },
            ],
        );
        assert!(body.contains("Chosen tool: search_symbols"));
        assert!(body.contains("Chosen tool: get_symbol"));
    }

    #[test]
    fn extract_served_step_bodies_finds_per_step_output() {
        let output = "Step 1: Route confidence: inferred\nChosen tool: search_symbols\nInvocation: {}\nRationale: x\nEconomics: serve (ok)\n\nresults\n\nStep 2: Route confidence: inferred\nChosen tool: get_symbol\nInvocation: {}\nRationale: y\nEconomics: serve (ok)\n\nbody";
        let steps = extract_served_step_bodies(output);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].0, "search_symbols");
        assert_eq!(steps[0].1, "results");
        assert_eq!(steps[1].0, "get_symbol");
        assert_eq!(steps[1].1, "body");
    }
}
