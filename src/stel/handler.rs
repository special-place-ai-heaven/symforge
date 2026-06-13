//! Phase 1 S4 — minimal `symforge` response envelope wiring (no L1 planner / L2 controller).

use std::time::{SystemTime, UNIX_EPOCH};

use super::envelope::{TrustEnvelopeInput, format_trust_envelope};
use super::types::{AdmissionDecision, StelEstimate, StelRequest};

/// Token estimate from UTF-8 body length (~4 chars per token).
pub fn estimate_tokens(text: &str) -> u32 {
    (text.len() / 4).min(u32::MAX as usize) as u32
}

/// Stub plan summary until L1 ships (`intent → tool (confidence)`).
pub fn stub_plan_summary(intent_label: &str, tool_name: &str, confidence_label: &str) -> String {
    format!("{intent_label} → {tool_name} ({confidence_label})")
}

/// Metrics for the S4 stub serve path (economics gate deferred to S6).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StubServeMetrics {
    pub plan_summary: String,
    pub response_tokens: u32,
    pub session_net_vs_manual: i64,
}

/// Build the trust envelope for a stub `serve` decision (L2 controller not wired yet).
pub fn envelope_for_stub_serve(metrics: &StubServeMetrics) -> String {
    format_trust_envelope(&TrustEnvelopeInput {
        plan_summary: metrics.plan_summary.clone(),
        decision: AdmissionDecision::Serve,
        response_tokens: metrics.response_tokens,
        net_vs_manual: 0,
        schema_tokens: 45,
        invoke_tokens: 80,
        predicted_tokens: metrics.response_tokens,
        predict_error_pct: 0.0,
        session_net_vs_manual: metrics.session_net_vs_manual,
        calibration: "pending",
    })
}

fn preview_plan_id(request: &StelRequest) -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("preview-{ms:x}-{}", request.query.len())
}

/// `StelEstimate` JSON body for `preview: true` (L2 preview stub).
pub fn format_preview_body(request: &StelRequest) -> String {
    let estimate = StelEstimate {
        plan_id: preview_plan_id(request),
        decision: AdmissionDecision::Serve,
        predicted_response_tokens: 400,
        predicted_manual_tokens: 800,
        predicted_schema_tokens: 45,
        predicted_invoke_tokens: 80,
        predicted_net_vs_manual: 275,
        recommended: true,
    };
    serde_json::to_string_pretty(&estimate).expect("StelEstimate must serialize")
}

/// Prepend the STEL trust envelope block to the tool body.
pub fn prepend_envelope(envelope: &str, body: &str) -> String {
    format!("{envelope}\n\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::types::IntentBucket;

    #[test]
    fn prepend_envelope_places_header_before_body() {
        let envelope = "── stel ──\nplan: auto → ask (inferred)\n──";
        let body = "tool output";
        let combined = prepend_envelope(envelope, body);
        assert!(combined.starts_with("── stel ──"));
        assert!(combined.ends_with("tool output"));
    }

    #[test]
    fn preview_body_is_valid_stel_estimate_json() {
        let request = StelRequest {
            query: "who calls foo".to_string(),
            intent: Some(IntentBucket::Auto),
            path: None,
            symbol: None,
            max_tokens: None,
            preview: Some(true),
        };
        let body = format_preview_body(&request);
        let parsed: StelEstimate = serde_json::from_str(&body).expect("preview JSON");
        assert_eq!(parsed.decision, AdmissionDecision::Serve);
        assert!(parsed.recommended);
    }

    #[test]
    fn stub_plan_summary_matches_schema_shape() {
        let summary = stub_plan_summary("trace", "find_references", "exact");
        assert_eq!(summary, "trace → find_references (exact)");
    }
}
