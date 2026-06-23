//! Feature 013 US2 (D8-ROOT redesign) — the LIVE predictor improves from
//! observed error, proven against the SAME residual the live path errs on.
//!
//! Calibration learns ONE multiplicative `response_correction_factor` applied to
//! the predictor's PREDICTED RESPONSE OUTPUT — whatever sub-model produced it
//! (byte-grounded read/edit OR the plan-only floor) — validated against the REAL
//! held-out residual `|round(predicted * factor) - actual|`. A `Tuned` verdict is
//! therefore a win the LIVE `estimate_economics_tuned` actually receives, on BOTH
//! the byte-grounded path (the dominant warm-daemon topology) and the floor path.
//!
//! Deterministic corpus replay (FR-004/FR-005/FR-012, SC-002). Server-gated: the
//! whole `stel` module is `#[cfg(feature = "server")]`.
#![cfg(feature = "server")]

// D14: `derive_tuning_candidate` is no longer importable here — it is `pub(crate)`
// (train-slice-only). The integration suite exercises derivation reproducibility
// and the factor cap THROUGH `compute_calibration_verdict`, the leakage-safe public
// API that owns the out-of-time split, which is the only correct external entry.
use symforge::stel::calibration::{
    CORRECTION_FACTOR_CAP, CalibrationVerdict, NO_CORRECTION_FACTOR, PredictionSample,
    SC002_MAE_REDUCTION_MARGIN, TUNING_MIN_CORPUS, TUNING_MIN_SAMPLES, apply_factor,
    compute_calibration_verdict, validate_candidate,
};
use symforge::stel::controller::{
    COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS, STATIC_RESPONSE_FLOOR, active_tuning_in_force,
    estimate_economics, estimate_economics_tuned, evaluate_plan_tuned, index_ref_for_target,
};
use symforge::stel::ledger::{LedgerCaptureInput, build_ledger_event};
use symforge::stel::ledger_store::{
    CURRENT_ESTIMATOR_VERSION, StelLedgerStore, TunedEstimateConstants,
};
use symforge::stel::types::{
    AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent, StelPlan, StelPlanStep,
    StelRequest,
};

// ---------------------------------------------------------------------------
// Deterministic fixture corpora
// ---------------------------------------------------------------------------

/// A corpus where the predictor's OWN output was `predicted` for every event but
/// the true served size was `predicted * bias`, so the systematic response
/// correction factor is exactly `bias`. `ts_ms` increments so the out-of-time
/// split (D11) has a well-defined older/newer ordering. Deterministic (FR-012).
fn biased_corpus(n: usize, predicted: u32, bias: f64) -> Vec<PredictionSample> {
    (0..n)
        .map(|i| PredictionSample {
            ts_ms: 1_000 + i as u64,
            predicted_response: predicted,
            actual_response: (f64::from(predicted) * bias).round() as u32,
        })
        .collect()
}

/// MIXED corpus: half byte-grounded predictions (`grounded_pred`), half plan-only
/// floor predictions (`floor_pred`), BOTH under the same systematic `bias`. This
/// is the warm-daemon production reality the old floor-only model ignored — most
/// served reads are byte-grounded. `ts_ms` interleaves so neither sub-model is
/// confined to one time-half (the bias, not the sub-model, drives the split).
fn mixed_biased_corpus(
    pairs: usize,
    grounded_pred: u32,
    floor_pred: u32,
    bias: f64,
) -> Vec<PredictionSample> {
    let mut corpus = Vec::with_capacity(pairs * 2);
    for i in 0..pairs {
        corpus.push(PredictionSample {
            ts_ms: 1_000 + (2 * i) as u64,
            predicted_response: grounded_pred,
            actual_response: (f64::from(grounded_pred) * bias).round() as u32,
        });
        corpus.push(PredictionSample {
            ts_ms: 1_000 + (2 * i + 1) as u64,
            predicted_response: floor_pred,
            actual_response: (f64::from(floor_pred) * bias).round() as u32,
        });
    }
    corpus
}

/// Mean absolute REAL residual of a corpus when each sample's OWN prediction is
/// corrected by `factor` — the quantity the live predictor errs on (NOT a flat
/// floor). With `factor == 1.0` this is the raw predictor's residual.
fn real_residual_mae(corpus: &[PredictionSample], factor: f64) -> f64 {
    let total: f64 = corpus
        .iter()
        .map(|s| {
            let corrected = apply_factor(s.predicted_response, factor);
            (f64::from(corrected) - f64::from(s.actual_response)).abs()
        })
        .sum();
    total / corpus.len() as f64
}

// ---------------------------------------------------------------------------
// A byte-grounded plan (a step WITH index_refs) and a floor plan (no refs).
// The grounded plan exercises the path the old floor-only model bypassed.
// ---------------------------------------------------------------------------

/// A plan-only step (no `index_refs`) — `estimate_economics` takes the FLOOR
/// path (static 400/800), which the correction now also scales at the plan sum.
fn floor_plan() -> StelPlan {
    StelPlan {
        plan_id: "plan-floor".to_string(),
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

/// A BYTE-GROUNDED read plan: the step carries real `IndexRef` byte sizes, so the
/// per-step response is the structured-serve fraction of the competent-manual
/// baseline — NOT the static floor. This is the dominant warm-daemon path. A
/// 40_000-char target yields a windowed manual baseline (4_000 chars -> 1_000
/// tokens) and a grounded response of 1_000 * 3/5 = 600 tokens.
fn grounded_read_plan(raw_chars: u64) -> StelPlan {
    StelPlan {
        plan_id: "plan-grounded".to_string(),
        intent: IntentBucket::Trace,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "get_symbol".to_string(),
            args: serde_json::json!({ "name": "x" }),
            est_response_tokens: STATIC_RESPONSE_FLOOR,
            est_manual_tokens: 800,
            index_refs: vec![index_ref_for_target("src/x.rs", raw_chars)],
        }],
        suggested_followup: None,
    }
}

// ===========================================================================
// T024 — accept path: a MIXED byte-grounded+floor biased corpus reaches Tuned
// because the correction reduces the REAL held-out residual by >= the margin.
// ===========================================================================

#[test]
fn mixed_grounded_and_floor_biased_corpus_reaches_tuned_on_real_residual() {
    // Grounded predictions ~600, floor predictions 400, BOTH 1.5x under-predicted
    // (actuals ~900 / ~600). The systematic factor is 1.5; correcting it drives
    // the REAL residual to ~0 on both sub-models -> a >20% reduction -> Tuned.
    let corpus = mixed_biased_corpus(30, 600, 400, 1.5);
    let (verdict, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);

    let (sample_size, error_before, error_after) = match verdict {
        CalibrationVerdict::Tuned {
            sample_size,
            error_before,
            error_after,
        } => (sample_size, error_before, error_after),
        other => panic!("mixed biased corpus must reach Tuned, got {other:?}"),
    };
    assert_eq!(sample_size, corpus.len());
    assert!(
        error_before > 0.0,
        "the raw predictor's residual must be positive"
    );

    // The reduction is on the REAL residual the live predictor errs on, and it
    // clears the margin.
    let relative = (error_before - error_after) / error_before;
    assert!(
        relative >= SC002_MAE_REDUCTION_MARGIN,
        "real held-out residual reduction {relative:.3} must clear the \
         {SC002_MAE_REDUCTION_MARGIN} margin (before={error_before:.1}, after={error_after:.1})"
    );

    // The accepted candidate carries the artifact and the recovered ~1.5 factor.
    let candidate = candidate.expect("accepted candidate must be returned");
    assert!(
        (candidate.response_correction_factor - 1.5).abs() < 0.05,
        "recovered factor {} must be near the true 1.5 bias",
        candidate.response_correction_factor
    );
    assert_eq!(candidate.error_before, error_before);
    assert_eq!(candidate.error_after, error_after);
    assert_eq!(candidate.estimator_version, CURRENT_ESTIMATOR_VERSION);
}

// ===========================================================================
// D8 PROOF — the LIVE byte-grounded predictor moves toward actual.
// This is the defect's heart: the validated win MUST reach the live path that
// most served reads take (byte-grounded), not a floor the live path bypasses.
// ===========================================================================

#[test]
fn live_byte_grounded_prediction_moves_toward_actual_after_tuning() {
    // Live byte-grounded prediction for a 40_000-char target: response = 600 tok.
    let plan = grounded_read_plan(40_000);
    let raw = estimate_economics(&plan);
    assert_eq!(
        raw.predicted_response_tokens, 600,
        "byte-grounded read predicts the structured fraction (600), NOT the 400 floor"
    );

    // The real served size was systematically 1.5x (900 tok). Derive a tuning from
    // a MIXED corpus carrying that bias on BOTH sub-models.
    let corpus = mixed_biased_corpus(30, 600, 400, 1.5);
    let (verdict, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);
    assert!(matches!(verdict, CalibrationVerdict::Tuned { .. }));
    let tuned = candidate.expect("biased corpus must produce a candidate");

    // Feed the tuning into the LIVE byte-grounded path. The correction is applied
    // to the grounded 600, moving it toward the real 900 — the win REACHES the
    // path the warm daemon actually serves (the old floor model never did this).
    let tuned_econ = estimate_economics_tuned(&plan, Some(&tuned));
    let actual = 900i64;
    let raw_err = (i64::from(raw.predicted_response_tokens) - actual).abs();
    let tuned_err = (i64::from(tuned_econ.predicted_response_tokens) - actual).abs();
    assert!(
        tuned_econ.predicted_response_tokens > raw.predicted_response_tokens,
        "tuned byte-grounded prediction must rise toward actual: {} -> {}",
        raw.predicted_response_tokens,
        tuned_econ.predicted_response_tokens
    );
    assert!(
        tuned_err < raw_err,
        "tuned byte-grounded prediction must be closer to actual ({actual}): \
         raw_err={raw_err}, tuned_err={tuned_err} \
         (raw={}, tuned={})",
        raw.predicted_response_tokens,
        tuned_econ.predicted_response_tokens
    );

    // D9 — schema/invoke/manual are LEFT UNCHANGED by the correction (only the
    // response output is corrected). This INVERTS the old test, which asserted the
    // schema/invoke overheads were scaled by the bias factor (the corruption).
    assert_eq!(
        tuned_econ.predicted_schema_tokens, COMPACT_SCHEMA_TOKENS,
        "schema overhead must NOT be scaled by the correction (D9)"
    );
    assert_eq!(
        tuned_econ.predicted_invoke_tokens, COMPACT_INVOKE_TOKENS,
        "invoke overhead must NOT be scaled by the correction (D9)"
    );
    assert_eq!(
        tuned_econ.predicted_manual_tokens, raw.predicted_manual_tokens,
        "the manual baseline must NOT be scaled by the correction (D9)"
    );
}

#[test]
fn live_floor_prediction_also_corrected() {
    // The floor path ALSO gets the same correction (both paths corrected).
    let plan = floor_plan();
    let raw = estimate_economics(&plan);
    assert_eq!(raw.predicted_response_tokens, STATIC_RESPONSE_FLOOR);

    let corpus = mixed_biased_corpus(30, 600, 400, 1.5);
    let (_v, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);
    let tuned = candidate.expect("candidate");

    let tuned_econ = estimate_economics_tuned(&plan, Some(&tuned));
    // round(400 * 1.5) == 600.
    assert_eq!(
        tuned_econ.predicted_response_tokens,
        apply_factor(STATIC_RESPONSE_FLOOR, tuned.response_correction_factor),
        "the floor path applies the same response correction"
    );
    assert!(tuned_econ.predicted_response_tokens > raw.predicted_response_tokens);
    // Overheads still fixed.
    assert_eq!(tuned_econ.predicted_schema_tokens, COMPACT_SCHEMA_TOKENS);
    assert_eq!(tuned_econ.predicted_invoke_tokens, COMPACT_INVOKE_TOKENS);
}

// ===========================================================================
// T024 — no-bias path: an UNBIASED corpus produces NO adjustment.
// If the dominant byte-grounded path is already ~calibrated, factor ~= 1.0, the
// gain is below the margin, and the surface stays Accumulating (no false tuned).
// ===========================================================================

#[test]
fn unbiased_corpus_produces_no_adjustment() {
    // Actuals == the predictor's own output (no systematic bias). The verdict must
    // stay Accumulating (never a harmful tune).
    let corpus = mixed_biased_corpus(30, 600, 400, 1.0);
    let (verdict, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);
    assert!(
        matches!(verdict, CalibrationVerdict::Accumulating { .. }),
        "unbiased corpus must NOT tune, got {verdict:?}"
    );
    assert!(
        candidate.is_none(),
        "no candidate may be applied on no-bias data"
    );
}

// ===========================================================================
// T024 — reject path: a worse correction is REJECTED on the REAL residual.
// ===========================================================================

#[test]
fn worse_correction_is_rejected_on_real_residual() {
    // Held-out actuals already match the predictor's output (unbiased). A 2.0
    // correction makes the REAL residual WORSE on held-out data -> rejected.
    let held_out = mixed_biased_corpus(15, 600, 400, 1.0);
    let worse = TunedEstimateConstants {
        response_correction_factor: 2.0,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: TUNING_MIN_SAMPLES as u32,
        error_before: 0.0,
        error_after: 0.0,
        tuned_at_ms: 0,
    };
    assert!(
        real_residual_mae(&held_out, worse.response_correction_factor)
            > real_residual_mae(&held_out, NO_CORRECTION_FACTOR),
        "a 2.0 correction must increase the real residual on unbiased data"
    );
    assert!(
        !validate_candidate(&worse, &held_out, NO_CORRECTION_FACTOR),
        "a correction that increases the real held-out residual must be rejected"
    );
}

#[test]
fn marginal_improvement_below_margin_is_rejected() {
    // A correction that closes < 20% of the real residual is rejected: the bar is
    // a meaningful margin, not any improvement (research R5). Predictor 400,
    // actuals ~440 (real residual ~40 under identity). A factor of 1.0175 closes
    // ~17.5% -> below the 20% bar.
    let held_out = biased_corpus(20, 400, 1.1); // actuals ~440
    let marginal = TunedEstimateConstants {
        response_correction_factor: 1.0175,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: TUNING_MIN_SAMPLES as u32,
        error_before: 0.0,
        error_after: 0.0,
        tuned_at_ms: 0,
    };
    let before = real_residual_mae(&held_out, NO_CORRECTION_FACTOR);
    let after = real_residual_mae(&held_out, marginal.response_correction_factor);
    let relative = (before - after) / before;
    assert!(
        relative < SC002_MAE_REDUCTION_MARGIN,
        "fixture must be below the margin to exercise the reject path (got {relative:.3})"
    );
    assert!(!validate_candidate(
        &marginal,
        &held_out,
        NO_CORRECTION_FACTOR
    ));
}

// ===========================================================================
// D10 — the sample gate accounts for the split: the FULL corpus must carry
// 2*MIN before tuning, and Accumulating renders the TRUE threshold (n <= min).
// ===========================================================================

#[test]
fn corpus_in_min_to_twice_min_stays_accumulating_against_true_threshold() {
    // n in [MIN, 2*MIN): NOT enough for both slices, so Accumulating against the
    // TRUE 2*MIN threshold. n <= min always holds (no `accumulating (18/12)`).
    for n in TUNING_MIN_SAMPLES..TUNING_MIN_CORPUS {
        let corpus = biased_corpus(n, 400, 2.0);
        let (verdict, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);
        match verdict {
            CalibrationVerdict::Accumulating { n: got_n, min } => {
                assert_eq!(got_n, n);
                assert_eq!(
                    min, TUNING_MIN_CORPUS,
                    "the surface must render the TRUE 2*MIN threshold"
                );
                assert!(got_n <= min, "n ({got_n}) must never exceed min ({min})");
            }
            other => panic!("n={n} in [MIN, 2*MIN) must be Accumulating, got {other:?}"),
        }
        assert!(candidate.is_none(), "no tuning below the corpus gate");
    }

    // At exactly 2*MIN the gate opens (each slice gets MIN).
    let at_gate = biased_corpus(TUNING_MIN_CORPUS, 400, 2.0);
    let (verdict, candidate) = compute_calibration_verdict(&at_gate, NO_CORRECTION_FACTOR);
    assert!(
        matches!(verdict, CalibrationVerdict::Tuned { .. }),
        "at exactly 2*MIN with a clear bias the gate opens, got {verdict:?}"
    );
    assert!(candidate.is_some());
}

// ===========================================================================
// D11 — out-of-time split: a correction that fit the OLDER half but no longer
// holds on the NEWER half (estimator/codebase drift) is REJECTED.
// ===========================================================================

#[test]
fn out_of_time_split_rejects_drifted_correction() {
    // Older half: 2x under-prediction (factor ~2.0). Newer half: already unbiased
    // (drift — the bias is gone). Training on the older half yields a 2.0 factor
    // that makes the recent residual WORSE -> rejected by the out-of-time split.
    let mut corpus = Vec::new();
    for i in 0..TUNING_MIN_SAMPLES {
        corpus.push(PredictionSample {
            ts_ms: 1_000 + i as u64, // older
            predicted_response: 400,
            actual_response: 800, // 2x bias
        });
    }
    for i in 0..TUNING_MIN_SAMPLES {
        corpus.push(PredictionSample {
            ts_ms: 9_000 + i as u64, // newer
            predicted_response: 400,
            actual_response: 400, // drift: no bias anymore
        });
    }
    let (verdict, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);
    assert!(
        matches!(verdict, CalibrationVerdict::Accumulating { .. }),
        "a drifted correction must be rejected by the out-of-time split, got {verdict:?}"
    );
    assert!(candidate.is_none());

    // Control: if the bias is CONSISTENT across both halves, the same shape tunes.
    let consistent = biased_corpus(TUNING_MIN_CORPUS, 400, 2.0);
    let (verdict, _candidate) = compute_calibration_verdict(&consistent, NO_CORRECTION_FACTOR);
    assert!(
        matches!(verdict, CalibrationVerdict::Tuned { .. }),
        "a consistent bias must still tune (control), got {verdict:?}"
    );
}

// ---------------------------------------------------------------------------
// T024 — derivation reproducibility + bounds (FR-012, oscillation guard)
// ---------------------------------------------------------------------------

#[test]
fn tuning_is_reproducible() {
    // D14: reproducibility is exercised through the leakage-safe public verdict API
    // (which owns the out-of-time split) rather than the now-crate-private
    // `derive_tuning_candidate`. A fixed corpus must yield an identical verdict AND
    // an identical derived factor on the accepted candidate.
    let (va, ca) = compute_calibration_verdict(
        &biased_corpus(TUNING_MIN_CORPUS, 400, 2.0),
        NO_CORRECTION_FACTOR,
    );
    let (vb, cb) = compute_calibration_verdict(
        &biased_corpus(TUNING_MIN_CORPUS, 400, 2.0),
        NO_CORRECTION_FACTOR,
    );
    assert_eq!(va, vb, "a fixed corpus must yield an identical verdict");
    let fa = ca.expect("biased corpus tunes").response_correction_factor;
    let fb = cb.expect("biased corpus tunes").response_correction_factor;
    assert_eq!(fa, fb, "a fixed corpus must yield an identical factor");
}

#[test]
fn factor_is_bounded_to_the_cap() {
    // A wildly under-predicting corpus (actuals 10x the prediction) must clamp the
    // correction at the CAP (no absurd swing). Exercised end-to-end through the
    // public verdict API (D14): even clamped at the CAP the correction closes
    // enough of the real residual (100 -> 400 vs an actual 1000: residual 900 ->
    // 600, a 33% reduction) to clear the SC-002 bar, so the verdict accepts and the
    // accepted candidate carries the capped factor.
    let (verdict, candidate) = compute_calibration_verdict(
        &biased_corpus(TUNING_MIN_CORPUS, 100, 10.0),
        NO_CORRECTION_FACTOR,
    );
    assert!(
        matches!(verdict, CalibrationVerdict::Tuned { .. }),
        "a 10x-biased corpus must tune (capped, but still a real improvement): {verdict:?}"
    );
    assert_eq!(
        candidate.expect("candidate").response_correction_factor,
        CORRECTION_FACTOR_CAP,
        "the derived factor must clamp at the CAP, not admit a 10.0 swing"
    );
}

#[test]
fn below_corpus_minimum_is_accumulating_not_deferred() {
    // Between 1 and the corpus minimum, Accumulating(n / 2*MIN), never Tuned.
    let corpus = biased_corpus(TUNING_MIN_CORPUS - 1, 400, 2.0);
    let (verdict, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);
    assert_eq!(
        verdict,
        CalibrationVerdict::Accumulating {
            n: TUNING_MIN_CORPUS - 1,
            min: TUNING_MIN_CORPUS,
        }
    );
    assert!(candidate.is_none());
}

#[test]
fn zero_samples_is_deferred() {
    let (verdict, candidate) = compute_calibration_verdict(&[], NO_CORRECTION_FACTOR);
    assert_eq!(verdict, CalibrationVerdict::Deferred);
    assert!(candidate.is_none());
}

// ---------------------------------------------------------------------------
// T032 / T025 — a stale-version tuning falls back (R3 in-force rule).
// ---------------------------------------------------------------------------

#[test]
fn version_mismatch_falls_back_to_uncorrected() {
    let plan = grounded_read_plan(40_000);
    let raw = estimate_economics(&plan);

    let stale = TunedEstimateConstants {
        response_correction_factor: 1.5,
        estimator_version: "some-old-estimator".to_string(),
        sample_size: 50,
        error_before: 300.0,
        error_after: 10.0,
        tuned_at_ms: 1,
    };
    // The in-force selector rejects a non-matching version -> None -> uncorrected.
    let in_force = active_tuning_in_force(Some(stale), CURRENT_ESTIMATOR_VERSION);
    assert!(
        in_force.is_none(),
        "stale-version tuning must not be in force"
    );

    let econ = estimate_economics_tuned(&plan, in_force.as_ref());
    assert_eq!(
        econ.predicted_response_tokens, raw.predicted_response_tokens,
        "a stale-version tuning must leave the prediction uncorrected"
    );
    assert_eq!(econ.predicted_schema_tokens, raw.predicted_schema_tokens);
    assert_eq!(econ.predicted_invoke_tokens, raw.predicted_invoke_tokens);
}

#[test]
fn no_tuning_is_byte_identical_to_uncorrected() {
    // tuned=None must be byte-identical to the static path on BOTH plans (the
    // pre-013 / golden-replay invariant).
    for plan in [
        floor_plan(),
        grounded_read_plan(40_000),
        grounded_read_plan(600),
    ] {
        let raw = estimate_economics(&plan);
        let none = estimate_economics_tuned(&plan, None);
        assert_eq!(raw, none, "tuned=None must equal the uncorrected economics");
    }
}

// ===========================================================================
// T037 — FR-011 operator reset: clearing accumulated calibration returns the
// durable state to Deferred WITHOUT rebuilding the index.
// ===========================================================================

fn biased_ledger_event(ts_ms: u64, predicted: u32, bias: f64) -> StelLedgerEvent {
    let actual = (f64::from(predicted) * bias).round() as u32;
    StelLedgerEvent {
        ts_ms,
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
            min: TUNING_MIN_CORPUS,
        }
    }
}

#[test]
fn operator_reset_returns_state_to_deferred() {
    let store = StelLedgerStore::open_in_memory("sess-reset").expect("ledger store");

    // Accumulate biased samples + persist an accepted tuning, so the durable
    // state is genuinely Tuned (not just empty).
    for i in 0..40 {
        store.record(&biased_ledger_event(1_000 + i, 400, 2.0));
    }
    let samples: Vec<PredictionSample> = store
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 10_000)
        .expect("samples")
        .iter()
        .map(PredictionSample::from)
        .collect();
    let (verdict, candidate) = compute_calibration_verdict(&samples, NO_CORRECTION_FACTOR);
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

// ===========================================================================
// END-TO-END — persist a derived tuning, reload it, and confirm the LIVE
// predictor applies the correction after a store round-trip (D8 full loop).
// ===========================================================================

#[test]
fn persisted_tuning_corrects_live_prediction_after_reload() {
    let store = StelLedgerStore::open_in_memory("sess-e2e").expect("ledger store");
    // Record a mixed biased corpus into the durable store.
    for i in 0..30 {
        store.record(&biased_ledger_event(1_000 + 2 * i, 600, 1.5)); // grounded-ish
        store.record(&biased_ledger_event(1_000 + 2 * i + 1, 400, 1.5)); // floor-ish
    }
    let samples: Vec<PredictionSample> = store
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 10_000)
        .expect("samples")
        .iter()
        .map(PredictionSample::from)
        .collect();
    let (verdict, candidate) = compute_calibration_verdict(&samples, NO_CORRECTION_FACTOR);
    assert!(matches!(verdict, CalibrationVerdict::Tuned { .. }));
    store
        .store_active_tuning(&candidate.expect("candidate"))
        .expect("persist");

    // Reload the tuning and apply the R3 in-force rule, then feed the LIVE path.
    let reloaded = store
        .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
        .expect("load")
        .expect("present");
    let in_force = active_tuning_in_force(Some(reloaded), CURRENT_ESTIMATOR_VERSION);
    assert!(in_force.is_some());

    let plan = grounded_read_plan(40_000);
    let raw = estimate_economics(&plan); // 600
    let tuned_econ = estimate_economics_tuned(&plan, in_force.as_ref());
    assert!(
        tuned_econ.predicted_response_tokens > raw.predicted_response_tokens,
        "the reloaded, in-force tuning must correct the live byte-grounded prediction: {} -> {}",
        raw.predicted_response_tokens,
        tuned_econ.predicted_response_tokens
    );
    // Overheads remain fixed across the round-trip (D9).
    assert_eq!(tuned_econ.predicted_schema_tokens, COMPACT_SCHEMA_TOKENS);
    assert_eq!(tuned_econ.predicted_invoke_tokens, COMPACT_INVOKE_TOKENS);
}

// ===========================================================================
// D15 — RE-TUNE CONSISTENCY: a SECOND tuning, recorded while a non-1.0 factor is
// already in force, must stay consistent. This is the coverage that hid D15: it
// goes through the REAL economics -> ledger record path (build_ledger_event from
// estimate_economics_tuned), so the recorded `predicted_response_tokens` is
// whatever the fix selects (RAW after fix, CORRECTED before). On the pre-fix,
// record-CORRECTED code this test FAILS (the second tune under-corrects and its
// before/after is a doubly-applied baseline); on the record-RAW fix it PASSES.
//
// Numbers (deterministic): byte-grounded raw prediction = 600 tok.
//   - Tune #1: true served = 600*1.5 = 900 (systematic 1.5x under-prediction).
//     in_force = 1.0 -> derive learns f_true = 1.5 -> live apply_factor(600,1.5)
//     = 900 = actual (converged).
//   - Tune #2 (drift): the codebase shifts so true served grows to 600*2.0 = 1200.
//     The second-regime batch is recorded while f0 = 1.5 is in force and is large
//     enough that the NEW regime dominates the recent window (the old 1.5x sample
//     is a minority of BOTH out-of-time split halves, so the older/train half also
//     derives ~2.0 — D11's split is honoured, not bypassed). Post-fix the recorded
//     prediction is the RAW 600 (not the corrected 900), so derive learns the
//     ABSOLUTE f_true = 2.0, live apply_factor(600,2.0) = 1200 = actual (converged),
//     and the validate baseline error_before = |apply_factor(600,1.5) - 1200| =
//     |900-1200| = 300 — the TRUE live residual under the active 1.5 tuning, NOT a
//     doubly-applied fabrication.
//
// On the PRE-FIX record-CORRECTED code the second-regime samples record predicted =
// apply_factor(600,1.5) = 900 (not 600), so derive learns median(1200/900) = 1.333
// (a DELTA), live apply_factor(600,1.333) = 800 (under-corrected, NOT 1200), and the
// in-force baseline scores |apply_factor(900,1.5)=1350 - 1200| = 150 (a doubly-
// applied fabrication, NOT 300). The convergence + error_before assertions below
// therefore FAIL pre-fix and PASS post-fix.
// ===========================================================================

/// A body whose `chars/4` token estimate is exactly `tokens` — lets the real
/// ledger record path (`build_ledger_event`) produce a chosen `actual_response`.
fn body_of_tokens(tokens: u32) -> String {
    "x".repeat(tokens as usize * 4)
}

/// Record ONE event through the REAL production path: ground the live economics
/// under the in-force tuning (`tuned`), then build the ledger event from that
/// `EconomicsBreakdown` exactly as `finalize_symforge_with_ledger` does. The
/// recorded `predicted_response_tokens` is therefore whichever field the D15 fix
/// selects (RAW vs corrected) — this is what the test exercises. `actual_tokens`
/// is realised as a body of the matching length so the residual is exact.
fn record_live_event(
    store: &StelLedgerStore,
    ts_ms: u64,
    plan: &StelPlan,
    tuned: Option<&TunedEstimateConstants>,
    actual_tokens: u32,
) {
    let request = StelRequest {
        query: "trace a symbol".to_string(),
        ..Default::default()
    };
    let decision = evaluate_plan_tuned(&request, plan, None, tuned);
    let economics = estimate_economics_tuned(plan, tuned);
    let body = body_of_tokens(actual_tokens);
    let mut event = build_ledger_event(&LedgerCaptureInput {
        plan,
        decision: &decision,
        economics: &economics,
        selected_tool: "get_symbol",
        tools_called: None,
        legacy_executed: true,
        output_body: &body,
        surface: "symforge",
    });
    // Pin the timestamp so the out-of-time split is deterministic (the live path
    // stamps wall-clock ms; the test fixes it for reproducibility, FR-012).
    event.ts_ms = ts_ms;
    store.record(&event);
}

/// Read the durable corpus and run the verdict against a given in-force factor —
/// the exact computation the server performs each tuning pass.
fn verdict_against(
    store: &StelLedgerStore,
    in_force_factor: f64,
) -> (CalibrationVerdict, Option<TunedEstimateConstants>) {
    let samples: Vec<PredictionSample> = store
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 10_000)
        .expect("samples")
        .iter()
        .map(PredictionSample::from)
        .collect();
    compute_calibration_verdict(&samples, in_force_factor)
}

#[test]
fn second_tune_with_factor_in_force_stays_consistent() {
    let store = StelLedgerStore::open_in_memory("sess-retune").expect("ledger store");
    let plan = grounded_read_plan(40_000);
    // The byte-grounded RAW prediction this plan produces, independent of tuning.
    const RAW: u32 = 600;
    assert_eq!(
        estimate_economics(&plan).predicted_response_tokens,
        RAW,
        "fixture sanity: the grounded read predicts 600 raw tokens"
    );

    // ---- TUNE #1: no tuning in force (f0 = 1.0), true served = 1.5x = 900. ----
    let mut ts = 1_000u64;
    for _ in 0..TUNING_MIN_CORPUS {
        record_live_event(&store, ts, &plan, None, 900);
        ts += 1;
    }
    let (v1, c1) = verdict_against(&store, NO_CORRECTION_FACTOR);
    assert!(
        matches!(v1, CalibrationVerdict::Tuned { .. }),
        "tune #1 must accept a 1.5x systematic bias: {v1:?}"
    );
    let tune1 = c1.expect("tune #1 candidate");
    assert!(
        (tune1.response_correction_factor - 1.5).abs() < 0.05,
        "tune #1 must learn the ABSOLUTE factor ~1.5, got {}",
        tune1.response_correction_factor
    );
    store.store_active_tuning(&tune1).expect("persist tune #1");

    // (a) After tune #1 the LIVE prediction converges to the actual 900.
    let live1 = apply_factor(RAW, tune1.response_correction_factor);
    assert_eq!(
        live1, 900,
        "live apply_factor(600, f1) must converge to the actual 900 after tune #1"
    );

    // ---- TUNE #2: f0 = 1.5 is IN FORCE; the codebase drifts so true served ----
    // ---- grows to 2.0x = 1200. Record the second regime WITH tune1 in force. ----
    // Post-fix, the recorded prediction is the RAW 600 (not the corrected 900),
    // so the next derive learns the ABSOLUTE 2.0 — not a 1200/900 = 1.333 delta.
    // The new regime is recorded in volume (4*MIN_CORPUS) so it dominates the recent
    // window: the older/train split half is majority 2.0x and derives ~2.0, honoring
    // D11's out-of-time split (a stale 1.5x-only train half would correctly reject).
    for _ in 0..(4 * TUNING_MIN_CORPUS) {
        record_live_event(&store, ts, &plan, Some(&tune1), 1200);
        ts += 1;
    }

    // The server validates the next pass against the in-force factor (1.5).
    let in_force_factor = tune1.response_correction_factor;
    let (v2, c2) = verdict_against(&store, in_force_factor);
    assert!(
        matches!(v2, CalibrationVerdict::Tuned { .. }),
        "tune #2 must accept the drift to 2.0x against the in-force 1.5: {v2:?}"
    );
    let tune2 = c2.expect("tune #2 candidate");

    // (a) tune #2 learns the ABSOLUTE drift factor ~2.0 (NOT the 1.333 delta the
    // record-corrected bug would learn), and the LIVE prediction converges to 1200.
    assert!(
        (tune2.response_correction_factor - 2.0).abs() < 0.05,
        "tune #2 must learn the ABSOLUTE factor ~2.0 (record-raw), got {} \
         (the record-CORRECTED bug learns ~1.333 here)",
        tune2.response_correction_factor
    );
    let live2 = apply_factor(RAW, tune2.response_correction_factor);
    assert_eq!(
        live2, 1200,
        "live apply_factor(600, f2) must converge to the actual 1200 after tune #2 \
         (the record-corrected bug under-corrects to apply_factor(600,1.333)=800)"
    );

    // (b) tune #2's error_before/error_after are the TRUE live residuals. Under the
    // in-force 1.5 the live prediction is 900 vs an actual 1200 -> residual 300.
    // The record-corrected bug instead reports |apply_factor(900,1.5)-1200| = 150,
    // a doubly-applied fabrication. error_after under the absolute 2.0 is ~0.
    if let CalibrationVerdict::Tuned {
        error_before,
        error_after,
        ..
    } = v2
    {
        assert!(
            (error_before - 300.0).abs() < 1.0,
            "error_before must be the TRUE live residual under f0=1.5 (300), got {error_before} \
             (the record-corrected bug fabricates ~150)"
        );
        assert!(
            error_after < 1.0,
            "error_after under the absolute 2.0 correction must be ~0, got {error_after}"
        );
    } else {
        panic!("tune #2 verdict must be Tuned");
    }
    store.store_active_tuning(&tune2).expect("persist tune #2");

    // (c) A no-further-bias third pass STAYS at the current tuning (no spurious
    // re-tune): record more events that the in-force 2.0 already predicts exactly
    // (raw 600 -> corrected 1200 == actual), then re-run the verdict against the
    // in-force 2.0. mae_before is ~0, so no candidate can clear the margin.
    for _ in 0..TUNING_MIN_CORPUS {
        record_live_event(&store, ts, &plan, Some(&tune2), 1200);
        ts += 1;
    }
    let (v3, c3) = verdict_against(&store, tune2.response_correction_factor);
    assert!(
        matches!(v3, CalibrationVerdict::Accumulating { .. }),
        "a no-further-bias third pass must STAY at the current tuning, not spuriously \
         re-tune: {v3:?}"
    );
    assert!(
        c3.is_none(),
        "no candidate may be promoted when the in-force tuning is already exact"
    );
}
