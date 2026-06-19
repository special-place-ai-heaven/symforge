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
    let ctx = StelStatusContext::from_server(
        "compact",
        "symforge",
        Some("E:/project/symforge".to_string()),
        true,
        128,
        512,
        &ledger,
        4096,
    );
    format_stel_status(
        &StelStatusRequest {
            detail: Some(StelStatusDetail::Full),
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
        calibration: "deferred",
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
