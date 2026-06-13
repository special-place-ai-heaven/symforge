//! `StelTrustEnvelope` text formatter (L0 response header).

use super::types::AdmissionDecision;

/// Inputs for the normative trust envelope block in `docs/stel-schema.md`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrustEnvelopeInput {
    /// e.g. `trace → find_references (exact)`
    pub plan_summary: String,
    pub decision: AdmissionDecision,
    pub response_tokens: u32,
    pub net_vs_manual: i32,
    pub schema_tokens: u32,
    pub invoke_tokens: u32,
    pub predicted_tokens: u32,
    pub predict_error_pct: f32,
    pub session_net_vs_manual: i64,
    pub calibration: &'static str,
    pub ledger_line: Option<String>,
}

fn decision_label(decision: AdmissionDecision) -> &'static str {
    match decision {
        AdmissionDecision::Serve => "serve",
        AdmissionDecision::Degrade => "degrade",
        AdmissionDecision::Bypass => "bypass",
        AdmissionDecision::CacheHit => "cache_hit",
        AdmissionDecision::Reject => "reject",
    }
}

/// Format the human-readable STEL trust envelope prepended to every `symforge` body.
pub fn format_trust_envelope(input: &TrustEnvelopeInput) -> String {
    let saved = input.net_vs_manual;
    let saved_label = if saved >= 0 {
        format!("{saved} saved")
    } else {
        format!("{} wasted", saved.abs())
    };

    format!(
        "── stel ──\n\
         plan: {plan}\n\
         decision: {decision}\n\
         tokens: {served} served · {saved_label} vs manual · schema {schema} · invoke {invoke}\n\
         predicted: {predicted} · error: {error:.1}%\n\
         session_net_vs_manual: {session:+}\n\
         calibration: {calibration}{ledger}\n\
         ──",
        plan = input.plan_summary,
        decision = decision_label(input.decision),
        served = input.response_tokens,
        saved_label = saved_label,
        schema = input.schema_tokens,
        invoke = input.invoke_tokens,
        predicted = input.predicted_tokens,
        error = input.predict_error_pct,
        session = input.session_net_vs_manual,
        calibration = input.calibration,
        ledger = input
            .ledger_line
            .as_ref()
            .map(|line| format!("\n{line}"))
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_matches_schema_example_shape() {
        let text = format_trust_envelope(&TrustEnvelopeInput {
            plan_summary: "trace → find_references (exact)".to_string(),
            decision: AdmissionDecision::Serve,
            response_tokens: 420,
            net_vs_manual: 380,
            schema_tokens: 45,
            invoke_tokens: 80,
            predicted_tokens: 400,
            predict_error_pct: 5.0,
            session_net_vs_manual: 1240,
            calibration: "ok",
            ledger_line: None,
        });

        assert!(text.starts_with("── stel ──\n"));
        assert!(text.contains("plan: trace → find_references (exact)"));
        assert!(text.contains("decision: serve"));
        assert!(text.contains("420 served · 380 saved vs manual"));
        assert!(text.contains("session_net_vs_manual: +1240"));
        assert!(text.ends_with("──"));
    }
}
