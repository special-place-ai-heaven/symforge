// Server-only integration test: depends on `#[cfg(feature = "server")]` `stel`
// machinery. Gating the whole file keeps
// `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Surface-honesty regression (010 US1 / T007, SC-001).
//!
//! Renders the two highest-traffic LLM-facing surfaces — the `symforge` trust
//! envelope and the `status` banner — over a realistic fixture and asserts the
//! honesty contract: no field named `net`/`saved`/`validated`/`active`/`pending`
//! presents an ungrounded constant or a gross counter as a measured result.
//!
//! This test is the executable form of FR-001/FR-002/FR-003. It fails on the
//! pre-010 surfaces (`session_net_vs_manual: +N`, `N saved vs manual`,
//! `l*: active`, `calibration: pending`) and passes after the honest relabel.
//! It asserts LABELS only — it does not assert any economics/route behavior,
//! so it cannot drift into a behavior test.

use symforge::stel::{
    AdmissionDecision, SessionLedger, StelStatusContext, StelStatusDetail, StelStatusRequest,
    TrustEnvelopeInput, build_plan, evaluate_plan, format_stel_status, format_trust_envelope,
    metrics_for_decision, plan_summary_line, summarize_calibration,
};

// Shared env guard: `force_full_stel_envelope` opts these honesty regressions
// back into the FULL trust envelope (the live default is now COMPACT). The
// module carries `#![allow(unsafe_code)]` and an RAII restore-on-drop guard; the
// suite runs `--test-threads=1`, so there is no cross-test env bleed.
#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

/// Build the trust envelope exactly as the live `symforge` handler does, using
/// the real planner + L2 controller (so the `400/800`-derived heuristic figures
/// flow through unmodified) and a gross session running total.
///
/// Forces the FULL envelope: this exercises the live env-gated path and the
/// honesty assertions target the full block, which compact (the new default)
/// omits by design. The render runs on the live env-gated path, so it holds
/// `COMPACT_ENV_LOCK` for the env-mutation window — deterministic even without
/// `--test-threads=1`. The env guard and lock drop together once the (now
/// immutable) `String` is captured for assertion.
fn render_live_envelope(query: &str, session_tokens_served: i64) -> String {
    let _env_lock = stel_surface_env::COMPACT_ENV_LOCK.blocking_lock();
    let _full = stel_surface_env::force_full_stel_envelope();
    let request = symforge::stel::StelRequest {
        query: query.to_string(),
        ..Default::default()
    };
    let plan = build_plan(&request);
    let decision = evaluate_plan(&request, &plan);
    let plan_summary = plan_summary_line(&plan);
    // response_tokens=420 mirrors a typical served body (chars/4 estimate).
    let metrics = metrics_for_decision(plan_summary, &decision, &plan, 420, session_tokens_served);
    symforge::stel::envelope_for_decision(&metrics)
}

fn render_full_status() -> String {
    let ledger = SessionLedger::new();
    let ctx = StelStatusContext::from_server("compact", "symforge", true, 128, 512, &ledger, 4096);
    format_stel_status(
        &StelStatusRequest {
            detail: Some(StelStatusDetail::Full),
            reset_calibration: None,
        },
        &ctx,
    )
}

/// Render the `detail:full` status with an explicit durable-ledger state, as the
/// stdio path does after attaching the durable store (feature 013 US1 T020/T023).
/// `from_server` defaults to `Unavailable`; `with_durable_ledger` overrides it,
/// mirroring `durable_ledger_summary_for_status` on the live server.
fn render_full_status_with_durable(state: symforge::stel::DurableLedgerState) -> String {
    let ledger = SessionLedger::new();
    let ctx = StelStatusContext::from_server("compact", "symforge", true, 128, 512, &ledger, 4096)
        .with_durable_ledger(state);
    format_stel_status(
        &StelStatusRequest {
            detail: Some(StelStatusDetail::Full),
            reset_calibration: None,
        },
        &ctx,
    )
}

#[test]
fn envelope_never_presents_a_gross_counter_as_net_savings() {
    let envelope = render_live_envelope("who references cfg_if", 4096);

    // FR-002 / TR-05: the monotonic gross running total is named for what it is
    // and carries no `+net` sign implying a saving.
    assert!(
        envelope.contains("session_tokens_served:"),
        "session running total must be named `session_tokens_served`:\n{envelope}"
    );
    assert!(
        !envelope.contains("session_net_vs_manual"),
        "the mislabeled `session_net_vs_manual` field must not survive:\n{envelope}"
    );
    assert!(
        !envelope.contains("session_tokens_served: +"),
        "a gross counter must not be printed with a `+net` savings sign:\n{envelope}"
    );
}

#[test]
fn envelope_labels_every_token_figure_as_estimated_or_heuristic() {
    let envelope = render_live_envelope("who references cfg_if", 4096);

    // FR-001: predicted/served figures are estimates, never measured savings.
    assert!(
        envelope.contains("(est. chars/4)"),
        "served tokens must be labeled an estimate:\n{envelope}"
    );
    assert!(
        envelope.contains("vs manual (heuristic)"),
        "the vs-manual comparison must be labeled heuristic:\n{envelope}"
    );
    assert!(
        envelope.contains("(heuristic)"),
        "predicted tokens must be labeled heuristic:\n{envelope}"
    );
    // The bare ` saved vs manual` phrasing (a measured-saving claim) is gone.
    assert!(
        !envelope.contains(" saved vs manual"),
        "no figure may claim a measured `saved vs manual`:\n{envelope}"
    );
}

#[test]
fn envelope_calibration_is_deferred_not_pending() {
    let envelope = render_live_envelope("who references cfg_if", 4096);

    // N-1 / TR-10: the auto-tuning seam is inert, so `deferred`, never `pending`
    // (which would imply transient in-progress work).
    assert!(
        envelope.contains("calibration: deferred"),
        "calibration must read `deferred`:\n{envelope}"
    );
    assert!(
        !envelope.contains("calibration: pending"),
        "`pending` implies transient work; the seam is permanently deferred:\n{envelope}"
    );
}

#[test]
fn rejected_decision_never_prints_a_positive_saving() {
    // TR-11: a positive predicted net on a reject must not read as a saving.
    // This asserts the FULL block (`n/a (rejected) vs manual`), so force full —
    // the compact default omits the per-call comparison. Live env-gated path, so
    // hold `COMPACT_ENV_LOCK` for the env-mutation window (deterministic even
    // without `--test-threads=1`); lock + env guard drop after the render.
    let _env_lock = stel_surface_env::COMPACT_ENV_LOCK.blocking_lock();
    let _full = stel_surface_env::force_full_stel_envelope();
    let envelope = format_trust_envelope(&TrustEnvelopeInput {
        plan_summary: "trace → find_references (exact)".to_string(),
        decision: AdmissionDecision::Reject,
        response_tokens: 100,
        est_net_vs_manual: 213,
        schema_tokens: 45,
        invoke_tokens: 80,
        predicted_tokens: 400,
        predict_error_pct: 0.0,
        session_tokens_served: 213,
        calibration: "deferred".to_string(),
        ledger_line: None,
    });
    assert!(envelope.contains("decision: reject"));
    assert!(
        envelope.contains("n/a (rejected) vs manual"),
        "a reject must show `n/a (rejected)`, not a positive saving:\n{envelope}"
    );
    assert!(!envelope.contains("213 fewer"));
}

#[test]
fn status_banner_uses_no_blanket_active_or_pending_literal() {
    let status = render_full_status();

    // FR-003 / TR-10: subsystem labels are honest enumerated static states, not
    // a blanket `active`/`pending` that implies runtime liveness it cannot prove.
    assert!(
        !status.contains(": active"),
        "no subsystem may report a blanket `active`:\n{status}"
    );
    assert!(
        !status.contains(": pending"),
        "no subsystem may report a blanket `pending`:\n{status}"
    );
    // Honest static labels are present.
    assert!(status.contains("l1_planner: wired"), "{status}");
    assert!(status.contains("l4_ledger: in_memory"), "{status}");
}

#[test]
fn status_deferred_list_drops_shipped_ledger_persistence() {
    let status = render_full_status();

    // FR-004: the durable ledger DOES ship in serve mode, so listing
    // `ledger_persistence` as deferred is false.
    assert!(
        !status.contains("ledger_persistence"),
        "`ledger_persistence` ships in serve mode; not deferred:\n{status}"
    );
}

#[test]
fn calibration_summary_surface_is_observational_not_validated() {
    // The rendered calibration section is honestly observational and must not
    // claim a `validated`/`tuned` state (the auto-tune seam is inert, N-1).
    let summary = summarize_calibration(&[]);
    let section = symforge::stel::format_calibration_section(&summary);
    assert!(
        section.contains("(observational)"),
        "calibration section must be labeled observational:\n{section}"
    );
    assert!(
        !section.to_lowercase().contains("validated"),
        "calibration must not claim a validated state:\n{section}"
    );
    assert!(
        section.contains("deferred"),
        "auto-tuning must read as deferred:\n{section}"
    );
}

// ===========================================================================
// T023 (feature 013 US1) — the durable-ledger state rendered on the stdio
// `status detail:full` path distinguishes Durable / Disabled{reason} /
// Unavailable HONESTLY, and never presents a durable-accumulation figure as
// measured when the store is Disabled. (FR-003, SC-005, Principle III)
// ===========================================================================

#[test]
fn durable_ledger_durable_state_reports_its_real_accumulation_figure() {
    use symforge::stel::{DurableLedgerState, DurableLedgerSummary};

    // A healthy durable store (the stdio path after T020 attach, store open) must
    // surface its REAL cumulative figures — this is the honest "events accumulate
    // across restarts" signal (SC-003), only ever shown when actually Durable.
    let status =
        render_full_status_with_durable(DurableLedgerState::Durable(DurableLedgerSummary {
            total_events: 42,
            total_net_vs_manual: 1337,
            session_count: 3,
        }));
    assert!(
        status.contains("durable_ledger: events=42 net_vs_manual=1337 sessions=3"),
        "a Durable store must report its real cumulative figures:\n{status}"
    );
    // It must NOT read `unavailable`/`disabled` when it is genuinely durable.
    assert!(
        !status.contains("durable_ledger: unavailable"),
        "a durable store must not read unavailable:\n{status}"
    );
    assert!(
        !status.contains("durable_ledger: disabled"),
        "a durable store must not read disabled:\n{status}"
    );
}

#[test]
fn durable_ledger_disabled_state_names_the_reason_and_claims_no_accumulation() {
    use symforge::stel::DurableLedgerState;

    // A wired-but-failing store (FR-003 honest degrade) must read `disabled`
    // WITH the reason (broken, not "off"), and MUST NOT present any durable
    // accumulation figure as measured — the whole honesty point of SC-005.
    let status = render_full_status_with_durable(DurableLedgerState::Disabled {
        reason: "summary query failed: disk I/O error".to_string(),
    });
    assert!(
        status.contains("durable_ledger: disabled (summary query failed: disk I/O error)"),
        "a Disabled store must name its failure reason distinguishably:\n{status}"
    );
    // No `events=` accumulation figure may appear for a Disabled store — a
    // Disabled store has no measured durable accumulation to present.
    assert!(
        !status.contains("durable_ledger: events="),
        "a Disabled store must NOT present a durable-accumulation figure as measured:\n{status}"
    );
}

#[test]
fn durable_ledger_unavailable_is_distinct_from_disabled() {
    use symforge::stel::DurableLedgerState;

    // No store wired (e.g. the daemon-proxy `status` path, or stdio when the data
    // dir could not be ensured) reads `unavailable` — structurally distinct from
    // a wired-but-broken `disabled`. "Off" must never read identically to
    // "broken" (N-3 / FR-008 honesty invariant carried onto the stdio surface).
    let unavailable = render_full_status_with_durable(DurableLedgerState::Unavailable);
    assert!(
        unavailable.contains("durable_ledger: unavailable"),
        "a never-configured store must read unavailable:\n{unavailable}"
    );
    assert!(
        !unavailable.contains("durable_ledger: disabled"),
        "unavailable must not collapse into disabled:\n{unavailable}"
    );
    assert!(
        !unavailable.contains("durable_ledger: events="),
        "an unavailable store presents no accumulation figure:\n{unavailable}"
    );

    // The three states render to three DISTINCT lines — the surface never blurs
    // durable / broken / off.
    let durable = render_full_status_with_durable(DurableLedgerState::Durable(
        symforge::stel::DurableLedgerSummary {
            total_events: 1,
            total_net_vs_manual: 2,
            session_count: 1,
        },
    ));
    let disabled = render_full_status_with_durable(DurableLedgerState::Disabled {
        reason: "open failed".to_string(),
    });
    let durable_line = durable
        .lines()
        .find(|l| l.starts_with("durable_ledger:"))
        .unwrap();
    let disabled_line = disabled
        .lines()
        .find(|l| l.starts_with("durable_ledger:"))
        .unwrap();
    let unavailable_line = unavailable
        .lines()
        .find(|l| l.starts_with("durable_ledger:"))
        .unwrap();
    assert_ne!(durable_line, disabled_line);
    assert_ne!(durable_line, unavailable_line);
    assert_ne!(disabled_line, unavailable_line);
}

// ===========================================================================
// T034 (feature 013 US2) — every CalibrationVerdict renders honestly: `tuned`
// carries before/after error + sample size, no surface reads
// `validated`/`saved`/`active`, and the served figure stays `(est.)` even when
// tuned constants are in force. (FR-009, FR-010, SC-005)
// ===========================================================================

#[test]
fn calibration_verdict_renders_honestly_in_every_state() {
    use symforge::stel::{CalibrationVerdict, render_calibration_verdict};

    // Deferred / Accumulating must NOT read `tuned`/`validated`/`saved`/`active`.
    for verdict in [
        CalibrationVerdict::Deferred,
        CalibrationVerdict::Accumulating { n: 4, min: 12 },
    ] {
        let line = render_calibration_verdict(&verdict);
        for forbidden in ["tuned", "validated", "saved", "active"] {
            assert!(
                !line.contains(forbidden),
                "non-tuned verdict must not read `{forbidden}`: {line}"
            );
        }
    }

    // Tuned MUST carry the before/after error artifact and the sample size — the
    // word `tuned` never appears without it (SC-005).
    let tuned = render_calibration_verdict(&CalibrationVerdict::Tuned {
        sample_size: 60,
        error_before: 400.0,
        error_after: 12.0,
    });
    assert!(tuned.starts_with("tuned (error: 400.0 -> 12.0 tok"));
    assert!(
        tuned.contains("n=60"),
        "tuned must surface the sample size: {tuned}"
    );
    for forbidden in ["validated", "saved", "active"] {
        assert!(
            !tuned.contains(forbidden),
            "tuned must not read `{forbidden}`: {tuned}"
        );
    }
}

#[test]
fn full_status_calibration_section_is_honest_for_each_verdict() {
    use symforge::stel::{CalibrationVerdict, StelStatusContext};

    let make = |verdict: CalibrationVerdict| {
        let ledger = SessionLedger::new();
        let ctx = StelStatusContext::from_server("compact", "symforge", true, 1, 1, &ledger, 0)
            .with_calibration_verdict(verdict);
        format_stel_status(
            &StelStatusRequest {
                detail: Some(StelStatusDetail::Full),
                reset_calibration: None,
            },
            &ctx,
        )
    };

    let deferred = make(CalibrationVerdict::Deferred);
    assert!(deferred.contains("calibration: deferred"), "{deferred}");
    assert!(
        !deferred.contains("tuned"),
        "deferred never reads tuned:\n{deferred}"
    );

    let accumulating = make(CalibrationVerdict::Accumulating { n: 3, min: 12 });
    assert!(
        accumulating.contains("calibration: accumulating (3/12)"),
        "{accumulating}"
    );
    assert!(
        !accumulating.contains("tuned"),
        "accumulating never reads tuned:\n{accumulating}"
    );

    let tuned = make(CalibrationVerdict::Tuned {
        sample_size: 50,
        error_before: 300.0,
        error_after: 40.0,
    });
    assert!(
        tuned.contains("calibration: tuned (error: 300.0 -> 40.0 tok"),
        "tuned section must carry the before/after artifact:\n{tuned}"
    );
    assert!(tuned.contains("n=50"), "{tuned}");
    // Never `validated`/`saved`/`active` in any rendered state.
    for forbidden in [": validated", ": saved", ": active"] {
        assert!(
            !tuned.contains(forbidden),
            "tuned section must not read `{forbidden}`:\n{tuned}"
        );
    }
}

#[test]
fn served_figure_stays_estimate_under_tuned_constants() {
    use symforge::stel::controller::estimate_economics_tuned;
    use symforge::stel::ledger_store::{CURRENT_ESTIMATOR_VERSION, TunedEstimateConstants};
    use symforge::stel::{
        AdmissionDecision, IntentBucket, RouteConfidence, StelPlan, StelPlanStep,
    };

    // A plan-only step whose floor a validated tuning replaces.
    let plan = StelPlan {
        plan_id: "p".to_string(),
        intent: IntentBucket::Trace,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "t".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "find_references".to_string(),
            args: serde_json::json!({"name": "x"}),
            est_response_tokens: 400,
            est_manual_tokens: 800,
            index_refs: vec![],
        }],
        suggested_followup: None,
    };
    let tuned = TunedEstimateConstants {
        response_correction_factor: 2.0,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: 60,
        error_before: 300.0,
        error_after: 12.0,
        tuned_at_ms: 1,
    };
    let econ = estimate_economics_tuned(&plan, Some(&tuned));

    // The envelope built from tuned economics still labels the served figure an
    // estimate — grounding in history is not measurement (FR-010).
    let _env_lock = stel_surface_env::COMPACT_ENV_LOCK.blocking_lock();
    let _full = stel_surface_env::force_full_stel_envelope();
    let envelope = format_trust_envelope(&TrustEnvelopeInput {
        plan_summary: "trace → find_references (exact)".to_string(),
        decision: AdmissionDecision::Serve,
        response_tokens: econ.predicted_response_tokens,
        est_net_vs_manual: econ.predicted_net_vs_manual,
        schema_tokens: econ.predicted_schema_tokens,
        invoke_tokens: econ.predicted_invoke_tokens,
        predicted_tokens: econ.predicted_response_tokens,
        predict_error_pct: 0.0,
        session_tokens_served: 0,
        calibration: "tuned".to_string(),
        ledger_line: None,
    });
    assert!(
        envelope.contains("(est. chars/4)"),
        "served figure must stay an estimate even under tuned constants:\n{envelope}"
    );
    assert!(
        envelope.contains("(heuristic)"),
        "predicted figure stays heuristic under tuned constants:\n{envelope}"
    );
    assert!(
        !envelope.contains(" saved vs manual"),
        "no measured-saving claim under tuned constants:\n{envelope}"
    );
}

#[test]
fn live_envelope_calibration_is_tuned_only_with_a_validated_tuning() {
    use symforge::stel::handler::metrics_for_decision_tuned;
    use symforge::stel::ledger_store::{CURRENT_ESTIMATOR_VERSION, TunedEstimateConstants};
    use symforge::stel::{build_plan, envelope_for_decision, evaluate_plan};

    let request = symforge::stel::StelRequest {
        query: "who references cfg_if".to_string(),
        ..Default::default()
    };
    let plan = build_plan(&request);
    let decision = evaluate_plan(&request, &plan);
    let summary = plan_summary_line(&plan);

    let _env_lock = stel_surface_env::COMPACT_ENV_LOCK.blocking_lock();
    let _full = stel_surface_env::force_full_stel_envelope();

    // No tuning in force -> the envelope honestly reads `deferred`, never `tuned`.
    let deferred_metrics =
        metrics_for_decision_tuned(summary.clone(), &decision, &plan, 420, 0, None);
    let deferred_env = envelope_for_decision(&deferred_metrics);
    assert!(
        deferred_env.contains("calibration: deferred"),
        "no tuning in force must read deferred:\n{deferred_env}"
    );
    assert!(
        !deferred_env.contains("calibration: tuned"),
        "the envelope must never read tuned without a tuning in force:\n{deferred_env}"
    );

    // A validated tuning in force -> the envelope reads `tuned` WITH the
    // before/after artifact (SC-005: never `tuned` without it).
    let tuned = TunedEstimateConstants {
        response_correction_factor: 2.0,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: 60,
        error_before: 300.0,
        error_after: 12.0,
        tuned_at_ms: 1,
    };
    let tuned_metrics = metrics_for_decision_tuned(summary, &decision, &plan, 420, 0, Some(&tuned));
    let tuned_env = envelope_for_decision(&tuned_metrics);
    assert!(
        tuned_env.contains("calibration: tuned (error: 300.0 -> 12.0 tok"),
        "a validated tuning must read tuned WITH the before/after artifact:\n{tuned_env}"
    );
    assert!(
        tuned_env.contains("n=60"),
        "tuned envelope line carries the sample size:\n{tuned_env}"
    );
    // Even under tuned constants, the served figure stays an estimate (FR-010).
    assert!(tuned_env.contains("(est. chars/4)"), "{tuned_env}");
}
