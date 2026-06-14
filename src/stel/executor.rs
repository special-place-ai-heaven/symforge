//! STEL L3 enforcement — respect L2 admission; in-process multi-step serve chain.

use serde_json;

use super::planner::confidence_label;
use super::types::{AdmissionDecision, StelBypassBody, StelDecision, StelPlan, StelPlanStep};

/// Outcome of one in-process legacy tool dispatch during a multi-step serve chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServedStepResult {
    pub tool: String,
    pub body: String,
}

/// Whether L3 should skip legacy tool dispatch (P-FF bypass only in this slice).
pub fn is_enforced_bypass(decision: &StelDecision) -> bool {
    decision.decision == AdmissionDecision::Bypass && decision.bypass.is_some()
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
    format!(
        "Decision: bypass\n\
         Economics: bypass ({decision_reason})\n\
         Action: {}\n\
         {host_read}\n\
         Predicted manual tokens: {}\n\
         Predicted SymForge tokens avoided: {}\n\
         \n\
         SymForge did not execute a legacy tool for this request.\n\
         Open `{path}` in your editor and review the file directly for whole-file tasks.\n\
         \n\
         --- bypass payload ---\n\
         {json}",
        bypass.action,
        bypass.predicted_manual_tokens,
        bypass.predicted_symforge_tokens,
        path = bypass.path,
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
    format!(
        "Step {}: Route confidence: {}\nChosen tool: {}\nInvocation: {}\nRationale: {}\nEconomics: {} ({})",
        step_index + 1,
        confidence_label(plan.confidence),
        step.tool,
        invocation,
        rationale,
        decision.decision.as_str(),
        decision.decision_reason,
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

/// Multi-step serve body — fail-fast chain output with per-step routing metadata.
pub fn format_multi_step_serve_body(
    plan: &StelPlan,
    decision: &StelDecision,
    step_results: &[ServedStepResult],
) -> String {
    assert_eq!(
        plan.steps.len(),
        step_results.len(),
        "step results must align with plan"
    );
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

/// Whether a dispatched tool body indicates mid-chain failure (fail fast).
pub fn serve_step_failed(tool_body: &str) -> bool {
    tool_body.starts_with("Error:")
        || tool_body.starts_with("Invalid")
        || tool_body.starts_with("Index not loaded.")
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
    fn negative_net_without_pff_body_is_not_enforced() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
        let plan = StelPlan {
            plan_id: "x".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Fallback,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({}),
                est_response_tokens: 900,
                est_manual_tokens: 100,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Bypass);
        assert!(!is_enforced_bypass(&decision));
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
}
