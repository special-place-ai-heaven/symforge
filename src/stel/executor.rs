//! STEL L3 enforcement — respect L2 admission for the smallest safe case (P-FF bypass).

use serde_json;

use super::types::{AdmissionDecision, StelBypassBody, StelDecision};

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

fn format_bypass_body_from(bypass: &StelBypassBody, decision_reason: &str) -> String {
    let json = serde_json::to_string_pretty(bypass).expect("StelBypassBody serializes");
    format!(
        "Decision: bypass\n\
         Economics: bypass ({decision_reason})\n\
         Action: {}\n\
         Host read: `{}` lines {}-{}\n\
         Predicted manual tokens: {}\n\
         Predicted SymForge tokens avoided: {}\n\
         \n\
         SymForge did not execute a legacy tool for this request.\n\
         Open `{path}` in your editor and review the file directly for whole-file tasks.\n\
         \n\
         --- bypass payload ---\n\
         {json}",
        bypass.action,
        bypass.path,
        bypass.start_line,
        bypass.end_line,
        bypass.predicted_manual_tokens,
        bypass.predicted_symforge_tokens,
        path = bypass.path,
    )
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
        assert!(body.contains("Host read: `lib.rs`"));
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
}
