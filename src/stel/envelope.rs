//! `StelTrustEnvelope` text formatter (L0 response header).

use super::types::AdmissionDecision;

/// Inputs for the normative trust envelope block in `docs/stel-schema.md`.
///
/// Honesty contract (010 US1/US5): all token figures here are *estimates*, never
/// measured token counts. `response_tokens`/`predicted_tokens` are `chars/4`
/// approximations. `est_net_vs_manual` is a heuristic prediction: on a read whose
/// real target byte size is known it is GROUNDED in those bytes (010 FR-014, the
/// byte-grounded estimator), and on every other step it falls back to the
/// planner's `400/800` per-step floor — either way it predicts from sizes, it
/// does not measure the model's actual token usage, so it stays labeled
/// `est.`/`heuristic` (grounding != measurement). `session_tokens_served` is a
/// monotonic gross running total of work performed this session, never a net of
/// savings.
#[derive(Clone, Debug, PartialEq)]
pub struct TrustEnvelopeInput {
    /// e.g. `trace → find_references (exact)`
    pub plan_summary: String,
    pub decision: AdmissionDecision,
    /// Estimated response tokens (`chars/4` approximation, not measured).
    pub response_tokens: u32,
    /// Heuristic predicted net vs manual — grounded in the real target byte size
    /// when known (010 FR-014), else the `400/800` per-step floor; never measured.
    pub est_net_vs_manual: i32,
    pub schema_tokens: u32,
    pub invoke_tokens: u32,
    /// Heuristic predicted response tokens (grounded-or-floor estimate, not measured).
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
///
/// The full multi-line block is the default. An operator who finds the per-call
/// block noisy can set `SYMFORGE_STEL_COMPACT=1` to collapse it to one honest
/// line (route · decision · est. served tokens); the full economics returns by
/// unsetting the flag. (Plan 009a; a *default*-compact form is a follow-up that
/// must update the honesty-surface assertions across the test corpus + schema.)
pub fn format_trust_envelope(input: &TrustEnvelopeInput) -> String {
    format_trust_envelope_inner(input, stel_compact_envelope_enabled())
}

/// Whether the operator opted into the one-line envelope via `SYMFORGE_STEL_COMPACT`.
fn stel_compact_envelope_enabled() -> bool {
    matches!(
        std::env::var("SYMFORGE_STEL_COMPACT").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

/// Render the envelope. `compact=false` is the full 010 block; `compact=true` is
/// the opt-in one-liner. Pure in `compact` so both forms are unit-testable
/// without mutating process env.
fn format_trust_envelope_inner(input: &TrustEnvelopeInput, compact: bool) -> String {
    if compact {
        // One-line opt-in form: keeps the load-bearing honesty — the route, the
        // admission decision, and that the served-token figure is an estimate —
        // and drops the per-call economics detail.
        return format!(
            "── stel · {plan} · {decision} · ~{served} tok served (est.) ──",
            plan = input.plan_summary,
            decision = decision_label(input.decision),
            served = input.response_tokens,
        );
    }
    // Heuristic predicted net vs a manual baseline — grounded in the real target
    // byte size when known (010 FR-014), else the planner's `400/800` floor;
    // never a measured token count. Labeled `est.`; a negative prediction reads
    // `more` not a positive "saved". On a `reject` SymForge
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

    #[test]
    fn compact_envelope_inner_is_one_honest_line() {
        // Plan 009a: the opt-in compact form is a single line that keeps the
        // load-bearing honesty (route, decision, est. served tokens) and drops
        // the per-call economics detail; the full form stays multi-line.
        let input = TrustEnvelopeInput {
            plan_summary: "find → search_text".to_string(),
            decision: AdmissionDecision::Serve,
            response_tokens: 1234,
            est_net_vs_manual: 50,
            schema_tokens: 10,
            invoke_tokens: 20,
            predicted_tokens: 800,
            predict_error_pct: 12.0,
            session_tokens_served: 5000,
            calibration: "deferred",
            ledger_line: Some("ledger: {}".to_string()),
        };
        let compact = format_trust_envelope_inner(&input, true);
        assert_eq!(
            compact.lines().count(),
            1,
            "compact envelope is one line: {compact}"
        );
        assert!(compact.contains("serve"), "keeps the decision: {compact}");
        assert!(
            compact.contains("~1234 tok served (est.)"),
            "keeps est. served tokens: {compact}"
        );
        assert!(
            !compact.contains("session_tokens_served") && !compact.contains("predicted:"),
            "drops the per-call detail: {compact}"
        );
        let full = format_trust_envelope_inner(&input, false);
        assert!(
            full.lines().count() > 1 && full.contains("session_tokens_served:"),
            "full form stays the multi-line block: {full}"
        );
    }
}
