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
/// The one-line COMPACT form is the DEFAULT: with no flag set, every live MCP
/// call prepends a single honest line (route · decision · est. served tokens),
/// keeping the load-bearing honesty while dropping the per-call economics noise.
/// An operator who wants the full multi-line economics block back can set
/// `SYMFORGE_STEL_FULL=1` (the on-request / contract form). That is the ONLY var
/// that changes behavior here. `SYMFORGE_STEL_COMPACT` is now a no-op — compact is
/// already the default, so the var is neither read nor required.
///
/// The compact one-liner body is UNCHANGED — it always keeps the route, the
/// admission decision, and the `(est.)`-labeled served-token figure. This
/// finishes the author-sanctioned default-compact follow-up: the honesty-surface
/// assertions across the test corpus + schema were updated to force the full
/// render where they verify the full contract, so no honesty assertion is lost.
pub fn format_trust_envelope(input: &TrustEnvelopeInput) -> String {
    format_trust_envelope_inner(input, !stel_full_envelope_enabled())
}

/// Whether the operator opted into the full multi-line block.
///
/// Compact is now the default, so the gate is inverted: only an explicit
/// `SYMFORGE_STEL_FULL=1|true|TRUE` (the on-request / contract form) restores the
/// full economics block. `SYMFORGE_STEL_COMPACT` is a no-op (compact is already
/// the default) and is intentionally not read here.
fn stel_full_envelope_enabled() -> bool {
    matches!(
        std::env::var("SYMFORGE_STEL_FULL").ok().as_deref(),
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
    use std::sync::Mutex;

    // Serializes the env-mutating public-entry tests so they are deterministic
    // even without `--test-threads=1`. The pure `_inner`-based tests never touch
    // process env and do not need it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Test-only RAII guard for `SYMFORGE_STEL_FULL`: set or unset on construction,
    /// restore the prior value on drop. Mirrors the in-crate env-guard convention
    /// (`cli::update::SymforgeHomeGuard`, the tools.rs `EnvVarGuard`). Callers hold
    /// `ENV_LOCK` for the guard's lifetime so the env mutation is serialized.
    struct StelFullEnvGuard {
        previous: Option<std::ffi::OsString>,
    }

    impl StelFullEnvGuard {
        #[allow(unsafe_code)] // test-only env mutation, serialized under ENV_LOCK.
        fn set(value: &str) -> Self {
            let previous = std::env::var_os("SYMFORGE_STEL_FULL");
            // SAFETY: serialized under ENV_LOCK; no concurrent env readers.
            unsafe { std::env::set_var("SYMFORGE_STEL_FULL", value) };
            Self { previous }
        }

        #[allow(unsafe_code)] // test-only env mutation, serialized under ENV_LOCK.
        fn unset() -> Self {
            let previous = std::env::var_os("SYMFORGE_STEL_FULL");
            // SAFETY: serialized under ENV_LOCK; no concurrent env readers.
            unsafe { std::env::remove_var("SYMFORGE_STEL_FULL") };
            Self { previous }
        }
    }

    impl Drop for StelFullEnvGuard {
        #[allow(unsafe_code)] // test-only env restore, serialized under ENV_LOCK.
        fn drop(&mut self) {
            // SAFETY: see `StelFullEnvGuard::set`.
            match &self.previous {
                Some(previous) => unsafe { std::env::set_var("SYMFORGE_STEL_FULL", previous) },
                None => unsafe { std::env::remove_var("SYMFORGE_STEL_FULL") },
            }
        }
    }

    fn sample_input() -> TrustEnvelopeInput {
        TrustEnvelopeInput {
            plan_summary: "trace → find_references (exact)".to_string(),
            decision: AdmissionDecision::Serve,
            response_tokens: 420,
            est_net_vs_manual: 380,
            schema_tokens: 45,
            invoke_tokens: 80,
            predicted_tokens: 400,
            predict_error_pct: 5.0,
            session_tokens_served: 1240,
            calibration: "deferred",
            ledger_line: Some("ledger: {}".to_string()),
        }
    }

    #[test]
    fn envelope_matches_schema_example_shape() {
        // Asserts the FULL contract shape: route through the pure full path so the
        // assertion is deterministic regardless of the (now compact) live default.
        let text = format_trust_envelope_inner(
            &TrustEnvelopeInput {
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
            },
            false,
        );

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
        // because SymForge did not deliver a serve result. Asserts the FULL block
        // through the pure full path (deterministic vs the compact live default).
        let text = format_trust_envelope_inner(
            &TrustEnvelopeInput {
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
            },
            false,
        );

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

    #[test]
    fn default_render_is_the_compact_one_liner() {
        // Default-compact follow-up: with no env opt-in, the live default render
        // is the single honest one-liner. Asserted through the pure compact path
        // (`_inner(.., true)`) — this IS what `format_trust_envelope` returns when
        // `SYMFORGE_STEL_FULL` is unset — so the assertion is deterministic and
        // never touches process env.
        let input = TrustEnvelopeInput {
            plan_summary: "trace → find_references (exact)".to_string(),
            decision: AdmissionDecision::Serve,
            response_tokens: 420,
            est_net_vs_manual: 380,
            schema_tokens: 45,
            invoke_tokens: 80,
            predicted_tokens: 400,
            predict_error_pct: 5.0,
            session_tokens_served: 1240,
            calibration: "deferred",
            ledger_line: Some("ledger: {}".to_string()),
        };
        let default_render = format_trust_envelope_inner(&input, true);

        assert_eq!(
            default_render.lines().count(),
            1,
            "default render is a single line: {default_render}"
        );
        // Load-bearing honesty survives the default: the decision label and the
        // `(est.)`-qualified served-token figure are present.
        assert!(
            default_render.contains("serve"),
            "default render keeps the decision label: {default_render}"
        );
        assert!(
            default_render.contains("(est.)"),
            "default render keeps the est. served-token label: {default_render}"
        );
        // The per-call economics detail and any measured-savings phrasing are
        // dropped — never a measured saving, never the gross session counter.
        for forbidden in [
            "session_tokens_served",
            "predicted:",
            " saved ",
            "fewer vs manual",
            "vs manual",
        ] {
            assert!(
                !default_render.contains(forbidden),
                "default render must drop `{forbidden}`: {default_render}"
            );
        }
    }

    #[test]
    fn public_entry_default_is_compact_full_is_opt_in() {
        // Exercises the PUBLIC `format_trust_envelope` (not `_inner`), so it pins
        // the shipped gate at envelope.rs: with `SYMFORGE_STEL_FULL` UNSET the live
        // default render is the single compact line; with it set to `1` the full
        // multi-line `── stel ──` economics block returns. Reverting the gate would
        // fail this. Serialized under ENV_LOCK so it is deterministic even without
        // `--test-threads=1`; the guards restore the prior env on drop.
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let input = sample_input();

        {
            let _unset = StelFullEnvGuard::unset();
            let default_render = format_trust_envelope(&input);
            assert_eq!(
                default_render.lines().count(),
                1,
                "unset SYMFORGE_STEL_FULL must yield the compact one-liner: {default_render}"
            );
            assert!(
                default_render.contains("(est.)"),
                "compact default keeps the est. served-token label: {default_render}"
            );
            assert!(
                !default_render.contains("session_tokens_served:"),
                "compact default drops the gross session counter: {default_render}"
            );
        }

        {
            let _full = StelFullEnvGuard::set("1");
            let full_render = format_trust_envelope(&input);
            assert!(
                full_render.lines().count() > 1,
                "SYMFORGE_STEL_FULL=1 must restore the multi-line block: {full_render}"
            );
            assert!(
                full_render.starts_with("── stel ──\n"),
                "full render is the `── stel ──` block: {full_render}"
            );
            assert!(
                full_render.contains("session_tokens_served:"),
                "full render carries the per-call economics: {full_render}"
            );
        }
    }

    #[test]
    fn public_entry_compact_flag_is_a_noop() {
        // Documents that `SYMFORGE_STEL_COMPACT` is now a NO-OP: compact is already
        // the default, the var is never read, so setting it alone (with
        // SYMFORGE_STEL_FULL unset) still yields the compact one-liner. Only
        // SYMFORGE_STEL_FULL changes behavior.
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _unset_full = StelFullEnvGuard::unset();
        let compact_guard = CompactEnvGuard::set("1");
        let render = format_trust_envelope(&sample_input());
        drop(compact_guard);
        assert_eq!(
            render.lines().count(),
            1,
            "SYMFORGE_STEL_COMPACT alone is a no-op; compact stays the default: {render}"
        );
    }

    /// Test-only RAII guard for the (no-op) `SYMFORGE_STEL_COMPACT` var, used only
    /// to prove it does not change behavior. Serialized under ENV_LOCK by callers.
    struct CompactEnvGuard {
        previous: Option<std::ffi::OsString>,
    }

    impl CompactEnvGuard {
        #[allow(unsafe_code)] // test-only env mutation, serialized under ENV_LOCK.
        fn set(value: &str) -> Self {
            let previous = std::env::var_os("SYMFORGE_STEL_COMPACT");
            // SAFETY: serialized under ENV_LOCK; no concurrent env readers.
            unsafe { std::env::set_var("SYMFORGE_STEL_COMPACT", value) };
            Self { previous }
        }
    }

    impl Drop for CompactEnvGuard {
        #[allow(unsafe_code)] // test-only env restore, serialized under ENV_LOCK.
        fn drop(&mut self) {
            // SAFETY: see `CompactEnvGuard::set`.
            match &self.previous {
                Some(previous) => unsafe { std::env::set_var("SYMFORGE_STEL_COMPACT", previous) },
                None => unsafe { std::env::remove_var("SYMFORGE_STEL_COMPACT") },
            }
        }
    }
}
