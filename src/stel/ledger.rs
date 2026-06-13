//! STEL L4 session ledger — append-only in-memory decision/execution records.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::controller::EconomicsBreakdown;
use super::handler::estimate_tokens;
use super::types::{
    AdmissionDecision, StelDecision, StelLedgerEvent, StelPlan,
};

/// In-memory append-only ledger for one MCP server session (no persistence in this slice).
#[derive(Debug, Default)]
pub struct SessionLedger {
    events: Mutex<Vec<StelLedgerEvent>>,
}

impl SessionLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, event: StelLedgerEvent) {
        self.events.lock().expect("session ledger lock").push(event);
    }

    pub fn len(&self) -> usize {
        self.events.lock().expect("session ledger lock").len()
    }

    pub fn last(&self) -> Option<StelLedgerEvent> {
        self.events.lock().expect("session ledger lock").last().cloned()
    }

    pub fn events(&self) -> Vec<StelLedgerEvent> {
        self.events.lock().expect("session ledger lock").clone()
    }
}

/// Inputs captured after L3 serve or enforced bypass.
#[derive(Clone, Debug)]
pub struct LedgerCaptureInput<'a> {
    pub plan: &'a StelPlan,
    pub decision: &'a StelDecision,
    pub economics: &'a EconomicsBreakdown,
    pub selected_tool: &'a str,
    pub legacy_executed: bool,
    pub output_body: &'a str,
}

/// Compact machine-readable metadata embedded in the trust envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
pub struct LedgerEnvelopeMeta {
    pub plan_id: String,
    pub route_tool: String,
    pub decision: String,
    pub bypass: bool,
    pub legacy_executed: bool,
    pub schema_tokens: u32,
    pub invoke_tokens: u32,
    pub predicted_net: i32,
    pub output_bytes: u64,
    pub output_tokens: u32,
}

/// Build a schema-aligned [`StelLedgerEvent`] from a completed `symforge` invocation.
pub fn build_ledger_event(input: &LedgerCaptureInput<'_>) -> StelLedgerEvent {
    let output_tokens = estimate_tokens(input.output_body);
    let symforge_cost = output_tokens
        .saturating_add(input.economics.predicted_schema_tokens)
        .saturating_add(input.economics.predicted_invoke_tokens);
    let net_vs_manual = input.economics.predicted_manual_tokens as i32 - symforge_cost as i32;

    StelLedgerEvent {
        ts_ms: ledger_timestamp_ms(),
        plan_id: input.plan.plan_id.clone(),
        surface: "symforge".to_string(),
        intent: input.plan.intent,
        decision: input.decision.decision,
        tools_called: if input.legacy_executed {
            vec![input.selected_tool.to_string()]
        } else {
            vec![]
        },
        predicted_response_tokens: input.economics.predicted_response_tokens,
        actual_response_tokens: output_tokens,
        manual_baseline_tokens: input.economics.predicted_manual_tokens,
        net_vs_manual,
        equivalence: None,
        route_confidence: input.plan.confidence,
    }
}

/// Format compact ledger metadata for the trust envelope `ledger:` line.
pub fn format_ledger_envelope_line(
    event: &StelLedgerEvent,
    meta: &LedgerEnvelopeMeta,
) -> String {
    let json = serde_json::to_string(meta).expect("ledger meta serializes");
    let _ = event;
    format!("ledger: {json}")
}

/// Build envelope metadata and ledger event together.
pub fn capture_ledger(
    input: &LedgerCaptureInput<'_>,
) -> (StelLedgerEvent, LedgerEnvelopeMeta) {
    let output_tokens = estimate_tokens(input.output_body);
    let output_bytes = input.output_body.len() as u64;
    let event = build_ledger_event(input);
    let meta = LedgerEnvelopeMeta {
        plan_id: event.plan_id.clone(),
        route_tool: input.selected_tool.to_string(),
        decision: input.decision.decision.as_str().to_string(),
        bypass: input.decision.decision == AdmissionDecision::Bypass,
        legacy_executed: input.legacy_executed,
        schema_tokens: input.economics.predicted_schema_tokens,
        invoke_tokens: input.economics.predicted_invoke_tokens,
        predicted_net: input.economics.predicted_net_vs_manual,
        output_bytes,
        output_tokens,
    };
    (event, meta)
}

fn ledger_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::controller::{evaluate_plan, estimate_economics};
    use crate::stel::planner::build_plan;
    use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest};

    fn serve_plan() -> StelPlan {
        StelPlan {
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
        }
    }

    #[test]
    fn serve_ledger_records_tool_execution() {
        let plan = serve_plan();
        let request = StelRequest {
            query: "who references cfg_if".to_string(),
            ..Default::default()
        };
        let decision = evaluate_plan(&request, &plan);
        let economics = estimate_economics(&plan);
        let body = "Chosen tool: find_references\n\nrefs";
        let (event, meta) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: "find_references",
            legacy_executed: true,
            output_body: body,
        });
        assert_eq!(event.decision, AdmissionDecision::Serve);
        assert_eq!(event.tools_called, vec!["find_references".to_string()]);
        assert!(meta.legacy_executed);
        assert!(!meta.bypass);
        assert_eq!(meta.route_tool, "find_references");
        assert!(meta.output_bytes > 0);
    }

    #[test]
    fn pff_bypass_ledger_skips_legacy_execution() {
        let request = StelRequest {
            query: "review entire lib.rs for security".to_string(),
            ..Default::default()
        };
        let plan = build_plan(&request);
        let decision = evaluate_plan(&request, &plan);
        let economics = super::super::controller::economics_for_bypass(
            decision.bypass.as_ref().expect("pff bypass"),
        );
        let body = "Decision: bypass\nSymForge did not execute a legacy tool";
        let (event, meta) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: plan.steps[0].tool.as_str(),
            legacy_executed: false,
            output_body: body,
        });
        assert_eq!(event.decision, AdmissionDecision::Bypass);
        assert!(event.tools_called.is_empty());
        assert!(meta.bypass);
        assert!(!meta.legacy_executed);
    }

    #[test]
    fn session_ledger_appends_events() {
        let ledger = SessionLedger::new();
        let plan = serve_plan();
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        let economics = estimate_economics(&plan);
        let (event, _) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: "find_references",
            legacy_executed: true,
            output_body: "body",
        });
        ledger.push(event);
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger.last().unwrap().plan_id, "plan-serve");
    }
}
