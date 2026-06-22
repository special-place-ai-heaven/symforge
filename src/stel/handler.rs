//! Phase 1 S4+ — `symforge` response envelope wiring (L1 planner + L2 economics + L3 bypass).

use super::controller::{EconomicsBreakdown, build_estimate, estimate_economics};
use super::envelope::{TrustEnvelopeInput, format_trust_envelope};
use super::types::{AdmissionDecision, StelDecision, StelEstimate, StelPlan, StelRequest};

/// Estimated token count from UTF-8 body length (`chars/4` approximation).
///
/// This is NOT a measured token count — it is the coarse `len/4` heuristic.
/// Every figure derived from it (envelope `served`, ledger `output_tokens`,
/// session totals) is an estimate and MUST be surfaced as such, never as an
/// exact/measured token count (010 N-4 / FR-001).
pub fn estimate_tokens(text: &str) -> u32 {
    (text.len() / 4).min(u32::MAX as usize) as u32
}

/// Stub plan summary until L1 ships (`intent → tool (confidence)`).
pub fn stub_plan_summary(intent_label: &str, tool_name: &str, confidence_label: &str) -> String {
    format!("{intent_label} → {tool_name} ({confidence_label})")
}

/// Metrics for trust-envelope formatting after L2 admission.
#[derive(Clone, Debug, PartialEq)]
pub struct DecisionEnvelopeMetrics {
    pub plan_summary: String,
    pub decision: AdmissionDecision,
    pub economics: EconomicsBreakdown,
    pub response_tokens: u32,
    /// Monotonic gross total of tokens served this session (only ever grows;
    /// NOT a net saving). Honest name per 010 FR-002 / TR-05.
    pub session_tokens_served: i64,
    pub predict_error_pct: f32,
    pub ledger_line: Option<String>,
}

/// Back-compat alias for older call sites/tests.
pub type StubServeMetrics = DecisionEnvelopeMetrics;

/// Build the trust envelope from L2 admission and economics.
pub fn envelope_for_decision(metrics: &DecisionEnvelopeMetrics) -> String {
    format_trust_envelope(&TrustEnvelopeInput {
        plan_summary: metrics.plan_summary.clone(),
        decision: metrics.decision,
        response_tokens: metrics.response_tokens,
        est_net_vs_manual: metrics.economics.predicted_net_vs_manual,
        schema_tokens: metrics.economics.predicted_schema_tokens,
        invoke_tokens: metrics.economics.predicted_invoke_tokens,
        predicted_tokens: metrics.economics.predicted_response_tokens,
        predict_error_pct: metrics.predict_error_pct,
        session_tokens_served: metrics.session_tokens_served,
        // Auto-tuning calibration is permanently deferred (the `CalibrationState`
        // seam is inert — N-1); honest label is `deferred`, never `pending`,
        // which would imply transient/in-progress work (010 TR-10 / N-1).
        calibration: "deferred",
        ledger_line: metrics.ledger_line.clone(),
    })
}

/// Back-compat wrapper defaulting to `serve`.
pub fn envelope_for_stub_serve(metrics: &DecisionEnvelopeMetrics) -> String {
    envelope_for_decision(metrics)
}

/// Build envelope metrics from L2 output and optional post-execution response size.
pub fn metrics_for_decision(
    plan_summary: String,
    decision: &StelDecision,
    plan: &StelPlan,
    response_tokens: u32,
    session_tokens_served: i64,
) -> DecisionEnvelopeMetrics {
    let economics = if decision.decision == AdmissionDecision::Bypass {
        decision
            .bypass
            .as_ref()
            .map(super::controller::economics_for_bypass)
            .unwrap_or_else(|| estimate_economics(plan))
    } else {
        estimate_economics(plan)
    };
    let predict_error_pct = if economics.predicted_response_tokens == 0 {
        0.0
    } else {
        let delta = response_tokens as i32 - economics.predicted_response_tokens as i32;
        (delta.abs() as f32 / economics.predicted_response_tokens as f32) * 100.0
    };
    DecisionEnvelopeMetrics {
        plan_summary,
        decision: decision.decision,
        economics,
        response_tokens,
        session_tokens_served,
        predict_error_pct,
        ledger_line: None,
    }
}

/// Attach ledger metadata and build the final `symforge` response string.
pub fn finalize_symforge_output(
    mut metrics: DecisionEnvelopeMetrics,
    ledger_line: String,
    body: &str,
) -> String {
    metrics.ledger_line = Some(ledger_line);
    let envelope = envelope_for_decision(&metrics);
    prepend_envelope(&envelope, body)
}

/// `StelEstimate` JSON body for `preview: true` (L2 preview).
pub fn format_preview_body(request: &StelRequest) -> String {
    use super::planner::build_plan;
    let plan = build_plan(request);
    let decision = super::controller::evaluate_plan(request, &plan);
    format_preview_estimate(&build_estimate(request, &plan, &decision))
}

/// Serialize a computed [`StelEstimate`].
pub fn format_preview_estimate(estimate: &StelEstimate) -> String {
    serde_json::to_string_pretty(estimate).expect("StelEstimate must serialize")
}

/// `StelEstimate` JSON when L1 has already built a [`StelPlan`] (legacy id hook).
pub fn format_preview_body_for_plan(request: &StelRequest, plan_id: &str) -> String {
    use super::controller::evaluate_plan;
    use super::planner::build_plan;
    let plan = build_plan(request);
    let mut plan = plan;
    plan.plan_id = plan_id.to_string();
    let decision = evaluate_plan(request, &plan);
    format_preview_estimate(&build_estimate(request, &plan, &decision))
}

/// Prepend the STEL trust envelope block to the tool body.
pub fn prepend_envelope(envelope: &str, body: &str) -> String {
    format!("{envelope}\n\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::types::IntentBucket;

    /// Test-only RAII guard forcing the FULL trust envelope.
    ///
    /// The live envelope is COMPACT by default; this unit verifies that the L2
    /// decision and economics (schema/invoke tokens) flow into the FULL block, so
    /// it opts back into full via `SYMFORGE_STEL_FULL`. Restored on drop. Matches
    /// the in-crate env-guard convention (`cli::update::SymforgeHomeGuard`): the
    /// lib test suite runs `--test-threads=1` and this is the only test that sets
    /// the variable, so there is no concurrent env access.
    struct StelFullEnvelopeGuard {
        prev: Option<std::ffi::OsString>,
    }

    impl StelFullEnvelopeGuard {
        #[allow(unsafe_code)] // test-only env mutation under --test-threads=1.
        fn set() -> Self {
            let prev = std::env::var_os("SYMFORGE_STEL_FULL");
            // SAFETY: the lib test suite runs single-threaded and this is the sole
            // setter of SYMFORGE_STEL_FULL, so no other thread reads/writes env.
            unsafe { std::env::set_var("SYMFORGE_STEL_FULL", "1") };
            Self { prev }
        }
    }

    impl Drop for StelFullEnvelopeGuard {
        #[allow(unsafe_code)] // test-only env restore under --test-threads=1.
        fn drop(&mut self) {
            // SAFETY: see `StelFullEnvelopeGuard::set`.
            match &self.prev {
                Some(prev) => unsafe { std::env::set_var("SYMFORGE_STEL_FULL", prev) },
                None => unsafe { std::env::remove_var("SYMFORGE_STEL_FULL") },
            }
        }
    }

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
            project: None,
            projects: None,
        };
        let body = format_preview_body(&request);
        let parsed: StelEstimate = serde_json::from_str(&body).expect("preview JSON");
        assert_eq!(parsed.decision, AdmissionDecision::Serve);
        assert!(parsed.recommended);
        assert_eq!(parsed.predicted_schema_tokens, 45);
    }

    #[test]
    fn envelope_reflects_l2_decision_and_economics() {
        use crate::stel::controller::{COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS};
        use crate::stel::planner::build_plan;
        // Verifies the FULL block carries the L2 decision + economics; force full
        // because the live default is now the compact one-liner (which has no
        // `decision:`/`schema`/`invoke` fields).
        let _full = StelFullEnvelopeGuard::set();
        let request = StelRequest {
            query: "who references cfg_if".to_string(),
            ..Default::default()
        };
        let plan = build_plan(&request);
        let decision = crate::stel::controller::evaluate_plan(&request, &plan);
        let metrics = metrics_for_decision(
            "trace → find_references (exact)".to_string(),
            &decision,
            &plan,
            420,
            0,
        );
        let envelope = envelope_for_decision(&metrics);
        assert!(envelope.contains("decision: serve"));
        assert!(envelope.contains(&format!("schema {COMPACT_SCHEMA_TOKENS}")));
        assert!(envelope.contains(&format!("invoke {COMPACT_INVOKE_TOKENS}")));
    }

    #[test]
    fn stub_plan_summary_matches_schema_shape() {
        let summary = stub_plan_summary("trace", "find_references", "exact");
        assert_eq!(summary, "trace → find_references (exact)");
    }
}
