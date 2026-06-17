//! `StelTrustEnvelope` text formatter (L0 response header).

use super::types::AdmissionDecision;

/// Inputs for the normative trust envelope block in `docs/stel-schema.md`.
///
/// Honesty contract (010 US1): all token figures here are *estimates*
/// (`response_tokens`/`predicted_tokens` are `chars/4` approximations;
/// `est_net_vs_manual` is a heuristic prediction derived from the planner's
/// `400/800` per-step constants, not a measured saving). The envelope text
/// labels them accordingly. `session_tokens_served` is a monotonic gross
/// running total of work performed this session, never a net of savings.
#[derive(Clone, Debug, PartialEq)]
pub struct TrustEnvelopeInput {
    /// e.g. `trace → find_references (exact)`
    pub plan_summary: String,
    pub decision: AdmissionDecision,
    /// Estimated response tokens (`chars/4` approximation, not measured).
    pub response_tokens: u32,
    /// Heuristic predicted net vs manual (from `400/800` constants, not measured).
    pub est_net_vs_manual: i32,
    pub schema_tokens: u32,
    pub invoke_tokens: u32,
    /// Heuristic predicted response tokens (planner constant, not measured).
    pub predicted_tokens: u32,
    pub predict_error_pct: f32,
    /// Monotonic gross total of tokens served this session (only ever grows;
    /// NOT a net saving). Named for what it is (010 FR-002, TR-05/TR-11).
    pub session_tokens_served: i64,
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
///
/// All token figures are explicitly labeled as estimates/heuristics: SymForge
/// does not measure real token counts, so presenting any of them as a measured
/// saving would violate the 010 honesty contract (FR-001/FR-002).
pub fn format_trust_envelope(input: &TrustEnvelopeInput) -> String {
    // Heuristic predicted net vs a manual baseline (derived from the planner's
    // `400/800` constants, never measured). Labeled `est.`; a negative
    // prediction reads `more` not a positive "saved". On a `reject` SymForge
    // did NOT deliver a serve result, so a positive "fewer vs manual"
    // prediction would imply a saving that never happened (010 TR-11) — show
    // `n/a (rejected)` instead.
    let est_net = input.est_net_vs_manual;
    let net_label = if input.decision == AdmissionDecision::Reject {
        "n/a (rejected)".to_string()
    } else if est_net >= 0 {
        format!("est. {est_net} fewer")
    } else {
        format!("est. {} more", est_net.abs())
    };

    format!(
        "── stel ──\n\
         plan: {plan}\n\
         decision: {decision}\n\
         tokens: ~{served} served (est. chars/4) · {net_label} vs manual (heuristic) · schema {schema} · invoke {invoke}\n\
         predicted: ~{predicted} (heuristic) · error: {error:.1}%\n\
         session_tokens_served: {session}\n\
         calibration: {calibration}{ledger}\n\
         ──",
        plan = input.plan_summary,
        decision = decision_label(input.decision),
        served = input.response_tokens,
        net_label = net_label,
        schema = input.schema_tokens,
        invoke = input.invoke_tokens,
        predicted = input.predicted_tokens,
        error = input.predict_error_pct,
        session = input.session_tokens_served,
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
            est_net_vs_manual: 380,
            schema_tokens: 45,
            invoke_tokens: 80,
            predicted_tokens: 400,
            predict_error_pct: 5.0,
            session_tokens_served: 1240,
            calibration: "ok",
            ledger_line: None,
        });

        assert!(text.starts_with("── stel ──\n"));
        assert!(text.contains("plan: trace → find_references (exact)"));
        assert!(text.contains("decision: serve"));
        assert!(text.contains("~420 served (est. chars/4)"));
        assert!(text.contains("est. 380 fewer vs manual (heuristic)"));
        // Honest contract: a gross running total is named for what it is and
        // carries no `+net` sign implying savings.
        assert!(text.contains("session_tokens_served: 1240"));
        assert!(!text.contains("session_net_vs_manual"));
        assert!(text.ends_with("──"));
    }

    #[test]
    fn reject_decision_never_prints_a_positive_saving() {
        // TR-11: a positive predicted net must not read as a saving on a reject,
        // because SymForge did not deliver a serve result.
        let text = format_trust_envelope(&TrustEnvelopeInput {
            plan_summary: "trace → find_references (exact)".to_string(),
            decision: AdmissionDecision::Reject,
            response_tokens: 100,
            est_net_vs_manual: 213,
            schema_tokens: 45,
            invoke_tokens: 80,
            predicted_tokens: 400,
            predict_error_pct: 0.0,
            session_tokens_served: 213,
            calibration: "deferred",
            ledger_line: None,
        });

        assert!(text.contains("decision: reject"));
        assert!(text.contains("n/a (rejected) vs manual"));
        assert!(!text.contains("fewer vs manual"));
        assert!(!text.contains("213 saved"));
    }
}
