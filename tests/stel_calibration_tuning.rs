//! Feature 013 US2 — the predictor improves from observed error.
//!
//! Deterministic corpus replay for the auto-tune (FR-004/FR-005/FR-012, SC-002).
//! Server-gated: the whole `stel` module is `#[cfg(feature = "server")]`, so the
//! file is gated to keep `--no-default-features --features embed --all-targets`
//! compiling (the auto-tune lives inside the server-gated module — embed
//! durability is a separate deferred structural task; see spec "Build findings").
#![cfg(feature = "server")]

use symforge::stel::calibration::{
    CalibrationVerdict, PredictionSample, SC002_MAE_REDUCTION_MARGIN, TUNING_MIN_SAMPLES,
    compute_calibration_verdict, derive_tuning_candidate, validate_candidate,
};
use symforge::stel::controller::{STATIC_RESPONSE_FLOOR, estimate_economics};
use symforge::stel::ledger_store::{
    CURRENT_ESTIMATOR_VERSION, StelLedgerStore, TunedEstimateConstants,
};
use symforge::stel::types::{
    AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent, StelPlan, StelPlanStep,
};

// ---------------------------------------------------------------------------
// Deterministic fixture corpora
// ---------------------------------------------------------------------------

/// A corpus where the estimator predicted `predicted` for every event but the
/// true served size was `predicted * bias`, so the systematic correction factor
/// is exactly `bias`. Deterministic: same args -> same corpus (FR-012).
fn biased_corpus(n: usize, predicted: u32, bias: f64) -> Vec<PredictionSample> {
    (0..n)
        .map(|_| PredictionSample {
            predicted_response: predicted,
            actual_response: (f64::from(predicted) * bias).round() as u32,
        })
        .collect()
}

/// Mean absolute error of a corpus under a fixed predicted `floor`.
fn mae(corpus: &[PredictionSample], floor: u32) -> f64 {
    let total: f64 = corpus
        .iter()
        .map(|s| (f64::from(floor) - f64::from(s.actual_response)).abs())
        .sum();
    total / corpus.len() as f64
}

// ---------------------------------------------------------------------------
// T024 — accept path: biased corpus reduces held-out MAE by >= the margin
// ---------------------------------------------------------------------------

#[test]
fn biased_corpus_reaches_tuned_with_margin_reduction() {
    // Actuals are systematically 2x the prediction. The verdict must reach
    // `Tuned`, and the held-out MAE must drop by AT LEAST the SC-002 margin
    // versus the static 400 floor (research R5, asserted as a specific margin —
    // not merely "strictly drops").
    let corpus = biased_corpus(60, 400, 2.0);
    let (verdict, candidate) = compute_calibration_verdict(&corpus, STATIC_RESPONSE_FLOOR);

    let (sample_size, error_before, error_after) = match verdict {
        CalibrationVerdict::Tuned {
            sample_size,
            error_before,
            error_after,
        } => (sample_size, error_before, error_after),
        other => panic!("biased corpus must reach Tuned, got {other:?}"),
    };
    assert_eq!(sample_size, 60);
    assert!(error_before > 0.0, "in-force MAE must be positive");
    let relative = (error_before - error_after) / error_before;
    assert!(
        relative >= SC002_MAE_REDUCTION_MARGIN,
        "held-out MAE reduction {relative:.3} must clear the {SC002_MAE_REDUCTION_MARGIN} margin \
         (before={error_before:.1}, after={error_after:.1})"
    );

    // The accepted candidate carries the artifact and corrects the floor upward.
    let candidate = candidate.expect("accepted candidate must be returned");
    assert!(
        candidate.response_floor > STATIC_RESPONSE_FLOOR,
        "under-prediction must raise the floor: {} -> {}",
        STATIC_RESPONSE_FLOOR,
        candidate.response_floor
    );
    assert_eq!(candidate.error_before, error_before);
    assert_eq!(candidate.error_after, error_after);
    assert_eq!(candidate.estimator_version, CURRENT_ESTIMATOR_VERSION);
}

// ---------------------------------------------------------------------------
// T024 — no-bias path: an unbiased corpus produces NO adjustment
// ---------------------------------------------------------------------------

#[test]
fn unbiased_corpus_produces_no_adjustment() {
    // Actuals == predicted == the static floor. No systematic bias to correct;
    // the verdict must stay Accumulating (never a harmful tune).
    let corpus = biased_corpus(60, 400, 1.0);
    let (verdict, candidate) = compute_calibration_verdict(&corpus, STATIC_RESPONSE_FLOOR);
    assert!(
        matches!(verdict, CalibrationVerdict::Accumulating { .. }),
        "unbiased corpus must NOT tune, got {verdict:?}"
    );
    assert!(
        candidate.is_none(),
        "no candidate may be applied on no-bias data"
    );
}

// ---------------------------------------------------------------------------
// T024 — reject path: a worse-than-baseline candidate is rejected
// ---------------------------------------------------------------------------

#[test]
fn worse_than_baseline_candidate_is_rejected() {
    // Held-out actuals cluster near the static 400 floor (unbiased). A candidate
    // that moved the floor to 800 makes the prediction WORSE on held-out data and
    // must be rejected by the gate (FR-005: calibration never makes it worse).
    let held_out = biased_corpus(20, 400, 1.0);
    let worse = TunedEstimateConstants {
        response_floor: 800,
        manual_floor: 1600,
        schema_tokens: 90,
        invoke_tokens: 160,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: TUNING_MIN_SAMPLES as u32,
        error_before: 0.0,
        error_after: 0.0,
        tuned_at_ms: 0,
    };
    assert!(mae(&held_out, worse.response_floor) > mae(&held_out, STATIC_RESPONSE_FLOOR));
    assert!(
        !validate_candidate(&worse, &held_out, STATIC_RESPONSE_FLOOR),
        "a candidate that increases held-out MAE must be rejected"
    );
}

#[test]
fn marginal_improvement_below_margin_is_rejected() {
    // A candidate that improves held-out MAE by LESS than the margin is rejected:
    // the bar is a meaningful margin, not any improvement (research R5).
    let held_out = biased_corpus(20, 400, 1.1); // actuals ~440, in-force MAE ~40
    let marginal = TunedEstimateConstants {
        response_floor: 407, // closes ~17.5% of the gap -> below 20%
        manual_floor: 814,
        schema_tokens: 46,
        invoke_tokens: 81,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: TUNING_MIN_SAMPLES as u32,
        error_before: 0.0,
        error_after: 0.0,
        tuned_at_ms: 0,
    };
    let before = mae(&held_out, STATIC_RESPONSE_FLOOR);
    let after = mae(&held_out, marginal.response_floor);
    let relative = (before - after) / before;
    assert!(
        relative < SC002_MAE_REDUCTION_MARGIN,
        "fixture must be below the margin to exercise the reject path (got {relative:.3})"
    );
    assert!(!validate_candidate(
        &marginal,
        &held_out,
        STATIC_RESPONSE_FLOOR
    ));
}

// ---------------------------------------------------------------------------
// T024 — reproducibility: same corpus -> same constants (FR-012)
// ---------------------------------------------------------------------------

#[test]
fn tuning_is_reproducible() {
    let a = derive_tuning_candidate(&biased_corpus(30, 400, 1.6)).expect("candidate a");
    let b = derive_tuning_candidate(&biased_corpus(30, 400, 1.6)).expect("candidate b");
    assert_eq!(a, b, "a fixed corpus must yield identical constants");

    let (va, _) = compute_calibration_verdict(&biased_corpus(40, 400, 2.0), STATIC_RESPONSE_FLOOR);
    let (vb, _) = compute_calibration_verdict(&biased_corpus(40, 400, 2.0), STATIC_RESPONSE_FLOOR);
    assert_eq!(va, vb, "a fixed corpus must yield an identical verdict");
}

#[test]
fn below_minimum_is_accumulating_not_deferred() {
    // Between 1 and the minimum, the verdict is Accumulating(n/min), never Tuned.
    let corpus = biased_corpus(TUNING_MIN_SAMPLES - 1, 400, 2.0);
    let (verdict, candidate) = compute_calibration_verdict(&corpus, STATIC_RESPONSE_FLOOR);
    assert_eq!(
        verdict,
        CalibrationVerdict::Accumulating {
            n: TUNING_MIN_SAMPLES - 1,
            min: TUNING_MIN_SAMPLES,
        }
    );
    assert!(candidate.is_none());
}

#[test]
fn zero_samples_is_deferred() {
    let (verdict, candidate) = compute_calibration_verdict(&[], STATIC_RESPONSE_FLOOR);
    assert_eq!(verdict, CalibrationVerdict::Deferred);
    assert!(candidate.is_none());
}

// ---------------------------------------------------------------------------
// T032 / T025 — tuned constants move estimate_economics toward actuals, and a
// version mismatch falls back to the static floor.
// ---------------------------------------------------------------------------

/// A plan-only step (no `index_refs`) so `estimate_economics` takes the FLOOR
/// path the auto-tune corrects (a byte-grounded step is unchanged by tuning).
fn plan_only_plan() -> StelPlan {
    StelPlan {
        plan_id: "plan-tune".to_string(),
        intent: IntentBucket::Trace,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "find_references".to_string(),
            args: serde_json::json!({ "name": "x" }),
            est_response_tokens: STATIC_RESPONSE_FLOOR,
            est_manual_tokens: 800,
            index_refs: vec![],
        }],
        suggested_followup: None,
    }
}

#[test]
fn tuned_constants_move_economics_toward_actuals() {
    // Static prediction uses the 400/800 + 45/80 floors.
    let plan = plan_only_plan();
    let static_econ = estimate_economics(&plan);
    assert_eq!(static_econ.predicted_response_tokens, STATIC_RESPONSE_FLOOR);
    assert_eq!(static_econ.predicted_schema_tokens, 45);
    assert_eq!(static_econ.predicted_invoke_tokens, 80);

    // A tuning derived from a 2x-under-prediction corpus (actuals ~800) raises
    // the floor to ~800; fed into the tuned economics path the prediction moves
    // from 400 toward the real 800.
    let corpus = biased_corpus(40, 400, 2.0);
    let (_verdict, candidate) = compute_calibration_verdict(&corpus, STATIC_RESPONSE_FLOOR);
    let tuned = candidate.expect("biased corpus must produce a candidate");

    let tuned_econ = symforge::stel::controller::estimate_economics_tuned(&plan, Some(&tuned));
    assert!(
        tuned_econ.predicted_response_tokens > static_econ.predicted_response_tokens,
        "tuned response prediction must rise toward the actual: {} -> {}",
        static_econ.predicted_response_tokens,
        tuned_econ.predicted_response_tokens
    );
    // The tuned floor (~800) is closer to the real served size (~800) than 400.
    let actual = 800i64;
    let static_err = (i64::from(static_econ.predicted_response_tokens) - actual).abs();
    let tuned_err = (i64::from(tuned_econ.predicted_response_tokens) - actual).abs();
    assert!(
        tuned_err < static_err,
        "tuned prediction must be closer to actual ({actual}): static_err={static_err}, tuned_err={tuned_err}"
    );
    // Schema / invoke also move under the same correction.
    assert!(tuned_econ.predicted_schema_tokens > static_econ.predicted_schema_tokens);
    assert!(tuned_econ.predicted_invoke_tokens > static_econ.predicted_invoke_tokens);
}

#[test]
fn version_mismatch_falls_back_to_static_floor() {
    // A tuned set whose estimator_version does NOT match current must NOT apply
    // (R3 in-force rule); the economics path falls back to the static floor.
    let plan = plan_only_plan();
    let static_econ = estimate_economics(&plan);

    let stale = TunedEstimateConstants {
        response_floor: 800,
        manual_floor: 1600,
        schema_tokens: 90,
        invoke_tokens: 160,
        estimator_version: "some-old-estimator".to_string(),
        sample_size: 50,
        error_before: 300.0,
        error_after: 10.0,
        tuned_at_ms: 1,
    };
    // The in-force selector must reject a non-matching version, yielding None,
    // so the tuned path collapses to the static result.
    let in_force =
        symforge::stel::controller::active_tuning_in_force(Some(stale), CURRENT_ESTIMATOR_VERSION);
    assert!(
        in_force.is_none(),
        "stale-version tuning must not be in force"
    );

    let econ = symforge::stel::controller::estimate_economics_tuned(&plan, in_force.as_ref());
    assert_eq!(
        econ.predicted_response_tokens,
        static_econ.predicted_response_tokens
    );
    assert_eq!(
        econ.predicted_schema_tokens,
        static_econ.predicted_schema_tokens
    );
    assert_eq!(
        econ.predicted_invoke_tokens,
        static_econ.predicted_invoke_tokens
    );
}

// ---------------------------------------------------------------------------
// T037 — FR-011 operator reset: clearing accumulated calibration returns the
// durable state to Deferred WITHOUT rebuilding the index.
// ---------------------------------------------------------------------------

fn biased_ledger_event(predicted: u32, bias: f64) -> StelLedgerEvent {
    let actual = (f64::from(predicted) * bias).round() as u32;
    StelLedgerEvent {
        ts_ms: 1_718_000_000_000,
        plan_id: "reset-test".to_string(),
        surface: "symforge".to_string(),
        intent: IntentBucket::Trace,
        decision: AdmissionDecision::Serve,
        tools_called: vec!["find_references".to_string()],
        predicted_response_tokens: predicted,
        actual_response_tokens: actual,
        manual_baseline_tokens: 800,
        net_vs_manual: 420,
        equivalence: None,
        route_confidence: RouteConfidence::Exact,
        pff_bypass: None,
        cache_hit: None,
        degrade_flags: vec![],
    }
}

/// Helper: the durable verdict the status surface would render, computed the same
/// way the server's `durable_calibration_verdict` does (samples + active tuning).
fn durable_verdict(store: &StelLedgerStore) -> CalibrationVerdict {
    let records = store
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 10_000)
        .expect("samples");
    let n = records.len();
    if let Some(active) = store
        .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
        .expect("load tuning")
        && active.error_before > active.error_after
    {
        return CalibrationVerdict::Tuned {
            sample_size: active.sample_size as usize,
            error_before: active.error_before,
            error_after: active.error_after,
        };
    }
    if n == 0 {
        CalibrationVerdict::Deferred
    } else {
        CalibrationVerdict::Accumulating {
            n,
            min: TUNING_MIN_SAMPLES,
        }
    }
}

#[test]
fn operator_reset_returns_state_to_deferred() {
    let store = StelLedgerStore::open_in_memory("sess-reset").expect("ledger store");

    // Accumulate biased samples + persist an accepted tuning, so the durable
    // state is genuinely Tuned (not just empty).
    for _ in 0..40 {
        store.record(&biased_ledger_event(400, 2.0));
    }
    let samples: Vec<PredictionSample> = store
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 10_000)
        .expect("samples")
        .iter()
        .map(PredictionSample::from)
        .collect();
    let (verdict, candidate) = compute_calibration_verdict(&samples, STATIC_RESPONSE_FLOOR);
    assert!(matches!(verdict, CalibrationVerdict::Tuned { .. }));
    store
        .store_active_tuning(&candidate.expect("candidate"))
        .expect("persist tuning");

    // Pre-reset: the surface reads Tuned.
    assert!(matches!(
        durable_verdict(&store),
        CalibrationVerdict::Tuned { .. }
    ));

    // FR-011 reset: clear accumulated calibration for the current estimator.
    let cleared = store
        .clear_calibration_for_estimator(CURRENT_ESTIMATOR_VERSION)
        .expect("reset");
    assert_eq!(cleared, 40, "all current-version samples must be cleared");

    // Post-reset: state returns to Deferred (no samples, no active tuning).
    assert_eq!(durable_verdict(&store), CalibrationVerdict::Deferred);
    assert!(
        store
            .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
            .expect("load")
            .is_none(),
        "active tuning must be cleared by reset"
    );

    // Idempotent: a second reset clears nothing and still succeeds.
    let cleared_again = store
        .clear_calibration_for_estimator(CURRENT_ESTIMATOR_VERSION)
        .expect("reset again");
    assert_eq!(cleared_again, 0);
    assert_eq!(durable_verdict(&store), CalibrationVerdict::Deferred);
}
