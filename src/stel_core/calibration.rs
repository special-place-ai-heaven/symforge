//! Observational calibration summary derived from in-memory [`SessionLedger`] events,
//! plus the feature-013 auto-tune: derive corrected token-estimate constants from
//! accumulated predicted-vs-actual error and accept them only when they reduce
//! held-out prediction error (US2).
//!
//! The observational summary is read-only. The auto-tune (`derive_tuning_candidate`
//! / `validate_candidate` / `compute_calibration_verdict`) is pure and deterministic:
//! given a fixed corpus it produces the same candidate and the same accept/reject
//! decision, so held-out validation and tests are stable (FR-012). Tuning corrects
//! the planner's static token *estimate* constants only; it never touches routing,
//! policy, or safety (FR-007).

use super::consts::{
    COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS, STATIC_MANUAL_FLOOR, STATIC_RESPONSE_FLOOR,
};
use super::ledger_store::{CURRENT_ESTIMATOR_VERSION, StoredLedgerRecord, TunedEstimateConstants};
use super::types::{AdmissionDecision, StelLedgerEvent};

/// Minimum ledger rows before offline review is considered sample-adequate.
pub const TUNING_REVIEW_MIN_EVENTS: usize = 5;

/// Minimum CURRENT-estimator-version samples before a tuning candidate may be
/// derived (FR-004 documented minimum). Set above [`TUNING_REVIEW_MIN_EVENTS`]:
/// a candidate is split into a derive slice + a held-out validation slice
/// (FR-005), so we need enough samples that BOTH slices are non-trivial. 12
/// gives at least a 6/6 split — small enough to reach in real dogfooding, large
/// enough that the held-out MAE is not dominated by a single event.
pub const TUNING_MIN_SAMPLES: usize = 12;

/// SC-002 "meaningful margin" (research R5): a tuning candidate is accepted only
/// when it reduces held-out mean absolute prediction error (MAE) by AT LEAST this
/// fraction (relative) versus the constants currently in force. 0.20 == 20%.
///
/// This is the REJECT gate's bar: non-improving, marginally-improving (< 20%),
/// and worse candidates are all rejected, so calibration never makes the
/// predictor worse and never over-promotes the surface to `tuned` for a
/// rounding-error gain. Tunable: raise once real per-project data accrues.
pub const SC002_MAE_REDUCTION_MARGIN: f64 = 0.20;

/// Hysteresis floor for re-tuning (edge-case "tuning oscillation"): once a tuning
/// is in force, a NEW candidate must beat the IN-FORCE constants by at least this
/// relative MAE margin to replace them, so repeated re-tuning converges instead
/// of flipping constants each session on noise. Equal to the accept margin: a
/// re-tune must clear the same bar a first tune does, measured against whatever
/// is currently in force (which `validate_candidate` already compares against).
pub const RETUNE_HYSTERESIS_MARGIN: f64 = SC002_MAE_REDUCTION_MARGIN;

/// One observed predicted-vs-actual response-token outcome — the unit the
/// auto-tune learns from. Decoupled from the storage row / in-memory event so
/// `derive_tuning_candidate` / `validate_candidate` are pure functions over a
/// fixed corpus (FR-012), unit-testable without a live store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictionSample {
    /// Response tokens the estimator predicted for the event.
    pub predicted_response: u32,
    /// Response tokens actually served (`chars/4` of the real body).
    pub actual_response: u32,
}

impl From<&StoredLedgerRecord> for PredictionSample {
    fn from(r: &StoredLedgerRecord) -> Self {
        Self {
            predicted_response: r.predicted_response_tokens,
            actual_response: r.actual_response_tokens,
        }
    }
}

impl From<&StelLedgerEvent> for PredictionSample {
    fn from(e: &StelLedgerEvent) -> Self {
        Self {
            predicted_response: e.predicted_response_tokens,
            actual_response: e.actual_response_tokens,
        }
    }
}

/// The honest calibration state machine surfaced on `status detail: full` and the
/// opt-in full envelope (feature 013, data-model `CalibrationVerdict`).
///
/// Named distinctly from the inert per-tool EMA [`crate::stel::types::CalibrationState`]
/// (types.rs) to avoid the collision. `Tuned` carries the held-out before/after
/// error artifact that justifies the word — the surface never reads `tuned`
/// without it (FR-009, SC-005).
#[derive(Clone, Debug, PartialEq)]
pub enum CalibrationVerdict {
    /// No / insufficient current-estimator-version samples — auto-tuning deferred.
    Deferred,
    /// Samples gathering toward [`TUNING_MIN_SAMPLES`]; `n` collected, `min` needed.
    Accumulating { n: usize, min: usize },
    /// A derived candidate reduced held-out MAE by the SC-002 margin and is in
    /// force. `error_before`/`error_after` are the held-out MAE figures (the
    /// artifact); `sample_size` is the count it was derived+validated on.
    Tuned {
        sample_size: usize,
        error_before: f64,
        error_after: f64,
    },
}

// ===========================================================================
// Auto-tune (US2): derive corrected constants from observed error, validate on
// held-out data, surface the honest verdict. Pure + deterministic (FR-012).
// ===========================================================================

/// Lower / upper clamp on the multiplicative correction factor a single tuning
/// step may apply to the static floors (bounded step, edge-case "tuning
/// oscillation / instability"). A factor outside `[1/CAP, CAP]` is clamped, so
/// one pass can never swing a constant wildly on a small / noisy sample; the
/// next pass refines from the clamped value. 4.0 covers the observed 40-194%
/// dogfood prediction error (R5) without admitting absurd swings.
const CORRECTION_FACTOR_CAP: f64 = 4.0;

/// Robust per-sample bias estimate: the MEDIAN of `actual / predicted` ratios
/// over the corpus, or `None` if no sample has a usable (non-zero) prediction.
///
/// Why median, not mean: the median has a 50% breakdown point, so a handful of
/// pathological events (a giant or tiny response) cannot drag the correction —
/// the estimator the spec targets is systematically biased, and the median
/// recovers that systematic factor while ignoring outliers. It is deterministic
/// (a fixed corpus yields one median), satisfying FR-012. Samples whose
/// `predicted_response == 0` are skipped (the ratio is undefined), not treated
/// as infinite bias.
fn median_actual_over_predicted(samples: &[PredictionSample]) -> Option<f64> {
    let mut ratios: Vec<f64> = samples
        .iter()
        .filter(|s| s.predicted_response > 0)
        .map(|s| f64::from(s.actual_response) / f64::from(s.predicted_response))
        .collect();
    if ratios.is_empty() {
        return None;
    }
    // Deterministic order: total_cmp gives a total order on f64 (no NaN here, but
    // it is panic-free and stable) so the median is reproducible.
    ratios.sort_by(f64::total_cmp);
    let mid = ratios.len() / 2;
    let median = if ratios.len() % 2 == 1 {
        ratios[mid]
    } else {
        (ratios[mid - 1] + ratios[mid]) / 2.0
    };
    Some(median)
}

/// Scale a static floor by a bounded correction `factor`, rounding to nearest and
/// clamping into `u32`. The bound (`CORRECTION_FACTOR_CAP`) is applied here so the
/// step is bounded regardless of caller (edge-case oscillation guard).
fn apply_correction(floor: u32, factor: f64) -> u32 {
    let bounded = factor.clamp(1.0 / CORRECTION_FACTOR_CAP, CORRECTION_FACTOR_CAP);
    let scaled = (f64::from(floor) * bounded).round();
    if scaled <= 0.0 {
        0
    } else if scaled >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        scaled as u32
    }
}

/// Mean absolute prediction error (MAE) of a corpus under a given
/// `response_floor`, in tokens.
///
/// The error model mirrors the live predictor: each sample's prediction came
/// from the per-step response floor, so a candidate `response_floor` is scored
/// by predicting `response_floor` for every sample and comparing to the actual.
/// Lower is better; this is the quantity the held-out gate (FR-005) reduces.
fn held_out_mae(samples: &[PredictionSample], response_floor: u32) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let total: f64 = samples
        .iter()
        .map(|s| (f64::from(response_floor) - f64::from(s.actual_response)).abs())
        .sum();
    Some(total / samples.len() as f64)
}

/// Derive a candidate tuned-constant set from CURRENT-estimator-version samples
/// (FR-004). Pure + deterministic.
///
/// Algorithm (documented, robust, deterministic):
/// 1. Compute the robust systematic bias `f = median(actual_i / predicted_i)`
///    over the corpus (median: outlier-robust, deterministic — see
///    [`median_actual_over_predicted`]).
/// 2. Apply the bounded factor `f` to every static floor
///    ([`STATIC_RESPONSE_FLOOR`], [`STATIC_MANUAL_FLOOR`], [`COMPACT_SCHEMA_TOKENS`],
///    [`COMPACT_INVOKE_TOKENS`]). The estimator's bias is a single multiplicative
///    factor on its shared `chars/4` token model, so the same correction applies
///    to every token figure it produces; the bounded step (`CORRECTION_FACTOR_CAP`)
///    keeps any single pass from swinging wildly (oscillation guard).
///
/// Returns `None` below [`TUNING_MIN_SAMPLES`] (FR-004 minimum gate) or when no
/// sample carries a usable prediction. The candidate's `error_before`/`error_after`
/// are left at 0.0 here — they are the held-out artifact filled in by the verdict
/// computation only AFTER validation accepts it; a bare candidate is never
/// `tuned` on its own.
pub fn derive_tuning_candidate(samples: &[PredictionSample]) -> Option<TunedEstimateConstants> {
    if samples.len() < TUNING_MIN_SAMPLES {
        return None;
    }
    let factor = median_actual_over_predicted(samples)?;
    Some(TunedEstimateConstants {
        response_floor: apply_correction(STATIC_RESPONSE_FLOOR, factor),
        manual_floor: apply_correction(STATIC_MANUAL_FLOOR, factor),
        schema_tokens: apply_correction(COMPACT_SCHEMA_TOKENS, factor),
        invoke_tokens: apply_correction(COMPACT_INVOKE_TOKENS, factor),
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: u32::try_from(samples.len()).unwrap_or(u32::MAX),
        error_before: 0.0,
        error_after: 0.0,
        tuned_at_ms: 0,
    })
}

/// Validate a `candidate` against a `held_out` slice it was NOT derived from
/// (FR-005, the REJECT gate). Returns `true` ONLY if the candidate's
/// `response_floor` reduces held-out MAE by at least the SC-002 margin
/// ([`SC002_MAE_REDUCTION_MARGIN`]) versus `in_force_response_floor` (the
/// constant currently applied — the static [`STATIC_RESPONSE_FLOOR`] when no
/// tuning is active, or the active tuning's `response_floor` on a re-tune, which
/// is where [`RETUNE_HYSTERESIS_MARGIN`] applies).
///
/// Non-improving, marginally-improving (< margin), and worse candidates all
/// return `false`: calibration never makes the predictor worse, and never
/// promotes the surface to `tuned` for a rounding-error gain.
pub fn validate_candidate(
    candidate: &TunedEstimateConstants,
    held_out: &[PredictionSample],
    in_force_response_floor: u32,
) -> bool {
    let Some(mae_before) = held_out_mae(held_out, in_force_response_floor) else {
        return false;
    };
    let Some(mae_after) = held_out_mae(held_out, candidate.response_floor) else {
        return false;
    };
    if mae_before <= 0.0 {
        // The in-force constants are already exact on held-out data; nothing to
        // improve, so no candidate can clear a >0 relative-reduction bar.
        return false;
    }
    let relative_reduction = (mae_before - mae_after) / mae_before;
    relative_reduction >= SC002_MAE_REDUCTION_MARGIN
}

/// Compute the honest [`CalibrationVerdict`] for a corpus of current-estimator
/// samples (FR-009), running the real derive + held-out validate (FR-004/FR-005)
/// over a DETERMINISTIC train/held-out split.
///
/// `in_force_response_floor` is the response floor currently applied (static, or
/// the active tuning's on a re-tune — the hysteresis anchor). Returns the
/// accepted candidate alongside the verdict so the caller can persist it
/// (T031); on reject / insufficient data the candidate is `None`.
///
/// Split: the corpus is partitioned by index parity — even indices derive, odd
/// indices validate. A fixed corpus yields a fixed split and therefore a fixed
/// verdict (FR-012). The candidate is derived ONLY from the train slice and
/// validated ONLY on the held-out slice, so the artifact is honest (no leakage).
pub fn compute_calibration_verdict(
    samples: &[PredictionSample],
    in_force_response_floor: u32,
) -> (CalibrationVerdict, Option<TunedEstimateConstants>) {
    let n = samples.len();
    if n == 0 {
        return (CalibrationVerdict::Deferred, None);
    }
    if n < TUNING_MIN_SAMPLES {
        return (
            CalibrationVerdict::Accumulating {
                n,
                min: TUNING_MIN_SAMPLES,
            },
            None,
        );
    }

    // Deterministic train / held-out split by index parity (no shuffle, no RNG).
    let train: Vec<PredictionSample> = samples.iter().step_by(2).copied().collect();
    let held_out: Vec<PredictionSample> = samples.iter().skip(1).step_by(2).copied().collect();

    let Some(mut candidate) = derive_tuning_candidate(&train) else {
        return (
            CalibrationVerdict::Accumulating {
                n,
                min: TUNING_MIN_SAMPLES,
            },
            None,
        );
    };

    if !validate_candidate(&candidate, &held_out, in_force_response_floor) {
        // Derived but did not clear the held-out bar: stay Accumulating (never a
        // false Tuned). A worse-than-baseline candidate lands here.
        return (
            CalibrationVerdict::Accumulating {
                n,
                min: TUNING_MIN_SAMPLES,
            },
            None,
        );
    }

    // Accepted: record the held-out before/after artifact ON the candidate so the
    // surface can read `tuned (before -> after)` honestly. error_before is the
    // in-force MAE; error_after is the candidate's MAE — both on held-out data.
    let error_before = held_out_mae(&held_out, in_force_response_floor).unwrap_or(0.0);
    let error_after = held_out_mae(&held_out, candidate.response_floor).unwrap_or(0.0);
    candidate.error_before = error_before;
    candidate.error_after = error_after;
    candidate.sample_size = u32::try_from(n).unwrap_or(u32::MAX);

    (
        CalibrationVerdict::Tuned {
            sample_size: n,
            error_before,
            error_after,
        },
        Some(candidate),
    )
}

/// Render the honest one-line calibration verdict for the surface (FR-009).
///
/// `deferred` / `accumulating (n/min)` / `tuned (error: before% -> after%)`.
/// `tuned` is emitted ONLY for [`CalibrationVerdict::Tuned`], which always
/// carries the before/after artifact, so the word never appears without it
/// (SC-005). The before/after figures are rendered as a percentage of the
/// in-force MAE reduction so the operator sees the magnitude of the win.
pub fn render_calibration_verdict(verdict: &CalibrationVerdict) -> String {
    match verdict {
        CalibrationVerdict::Deferred => "deferred".to_string(),
        CalibrationVerdict::Accumulating { n, min } => {
            format!("accumulating ({n}/{min})")
        }
        CalibrationVerdict::Tuned {
            sample_size,
            error_before,
            error_after,
        } => {
            // Relative reduction as a percent, guarded against a zero baseline.
            let reduction_pct = if *error_before > 0.0 {
                ((error_before - error_after) / error_before) * 100.0
            } else {
                0.0
            };
            format!(
                "tuned (error: {error_before:.1} -> {error_after:.1} tok, -{reduction_pct:.0}% MAE; n={sample_size})"
            )
        }
    }
}

/// Aggregated session calibration metrics (observational only).
///
/// `PartialEq` (not `Eq`): the `verdict` carries `f64` held-out error figures.
#[derive(Clone, Debug, PartialEq)]
pub struct StelCalibrationSummary {
    pub total_events: usize,
    pub serve_count: usize,
    pub degrade_count: usize,
    pub bypass_count: usize,
    pub cache_hit_count: usize,
    pub pff_bypass_count: usize,
    pub legacy_executed_count: usize,
    pub total_schema_tokens: u64,
    pub total_invoke_tokens: u64,
    pub total_predicted_net: i64,
    pub total_predicted_response_tokens: u64,
    pub total_actual_response_tokens: u64,
    pub tuning_note: String,
    /// Honest auto-tune verdict (feature 013 US2). Computed from the events'
    /// predicted-vs-actual response tokens via the real derive + held-out
    /// validate; defaults to `Deferred`/`Accumulating` until a candidate clears
    /// the SC-002 bar. The surface renders this, never a hard-coded "deferred".
    pub verdict: CalibrationVerdict,
}

/// Summarize ledger events for observational calibration feedback.
pub fn summarize_calibration(events: &[StelLedgerEvent]) -> StelCalibrationSummary {
    let mut summary = StelCalibrationSummary {
        total_events: events.len(),
        serve_count: 0,
        degrade_count: 0,
        bypass_count: 0,
        cache_hit_count: 0,
        pff_bypass_count: 0,
        legacy_executed_count: 0,
        total_schema_tokens: 0,
        total_invoke_tokens: 0,
        total_predicted_net: 0,
        total_predicted_response_tokens: 0,
        total_actual_response_tokens: 0,
        tuning_note: String::new(),
        verdict: CalibrationVerdict::Deferred,
    };

    for event in events {
        match event.decision {
            AdmissionDecision::Serve => summary.serve_count += 1,
            AdmissionDecision::Degrade => summary.degrade_count += 1,
            AdmissionDecision::CacheHit => summary.cache_hit_count += 1,
            AdmissionDecision::Bypass => summary.bypass_count += 1,
            _ => {}
        }
        if event.pff_bypass == Some(true) {
            summary.pff_bypass_count += 1;
        }
        if !event.tools_called.is_empty() {
            summary.legacy_executed_count += 1;
        }
        summary.total_schema_tokens += u64::from(COMPACT_SCHEMA_TOKENS);
        summary.total_invoke_tokens += u64::from(COMPACT_INVOKE_TOKENS);
        summary.total_predicted_net += i64::from(event.net_vs_manual);
        summary.total_predicted_response_tokens += u64::from(event.predicted_response_tokens);
        summary.total_actual_response_tokens += u64::from(event.actual_response_tokens);
    }

    // T030: replace the hard-coded "auto-tuning still deferred" seam with the
    // REAL verdict, computed by running derive + held-out validate over the
    // session events' predicted-vs-actual response tokens against the static
    // in-force floor. The session ledger records only current-estimator rows, so
    // every event is a valid current-version sample here. The verdict (and its
    // rendered note) is honest: `tuned` appears only when a candidate cleared the
    // held-out SC-002 bar.
    let samples: Vec<PredictionSample> = events.iter().map(PredictionSample::from).collect();
    let (verdict, _candidate) =
        compute_calibration_verdict(&samples, super::consts::STATIC_RESPONSE_FLOOR);
    summary.tuning_note = render_calibration_verdict(&verdict);
    summary.verdict = verdict;
    summary
}

/// Stable text block embedded in `status` `detail: full` output.
pub fn format_calibration_section(summary: &StelCalibrationSummary) -> String {
    let lines = [
        "── calibration (observational) ──".to_string(),
        format!("events: {}", summary.total_events),
        format!("serve: {}", summary.serve_count),
        format!("degrade: {}", summary.degrade_count),
        format!("bypass: {}", summary.bypass_count),
        format!("cache_hit: {}", summary.cache_hit_count),
        format!("pff_bypass: {}", summary.pff_bypass_count),
        format!("legacy_executed: {}", summary.legacy_executed_count),
        format!("schema_tokens: {}", summary.total_schema_tokens),
        format!("invoke_tokens: {}", summary.total_invoke_tokens),
        format!("predicted_net_total: {}", summary.total_predicted_net),
        format!(
            "predicted_response_tokens: {}",
            summary.total_predicted_response_tokens
        ),
        format!(
            "actual_response_tokens: {}",
            summary.total_actual_response_tokens
        ),
        // T033: the honest auto-tune verdict line. `tuning_note` carries the same
        // rendered verdict; `calibration:` is the canonical field the honesty
        // regression keys on (deferred / accumulating (n/min) / tuned (...)).
        format!(
            "calibration: {}",
            render_calibration_verdict(&summary.verdict)
        ),
        format!("tuning: {}", summary.tuning_note),
        "──".to_string(),
    ];
    lines.join("\n")
}

// D3-ROOT extract-up: the protocol-free auto-tune PURE-MATH tests live here in
// `stel_core` so they compile under `any(server, embed)`. The fixture-driven
// observational-summary tests (`serve_event` / `pff_bypass_event` builders +
// the `summarize_calibration` / `format_calibration_section` assertions) need
// `stel::{controller, ledger, planner}` — all server-only — so they CANNOT live
// here without dragging the protocol stack into the embed build. They were
// moved verbatim (behavior-preserving) to `crate::stel::calibration_summary_tests`,
// a server-gated sibling. See `src/stel/calibration_summary_tests.rs`.
#[cfg(test)]
mod tests {
    use super::*;

    // -- Auto-tune pure math (US2 T027/T028/T029) ---------------------------

    /// Build a biased corpus: the estimator predicts `predicted` for every event
    /// but the actual is `predicted * bias`, so the true correction factor is
    /// `bias`. `n` events.
    fn biased_corpus(n: usize, predicted: u32, bias: f64) -> Vec<PredictionSample> {
        (0..n)
            .map(|_| PredictionSample {
                predicted_response: predicted,
                actual_response: (f64::from(predicted) * bias).round() as u32,
            })
            .collect()
    }

    #[test]
    fn derive_returns_none_below_minimum() {
        let corpus = biased_corpus(TUNING_MIN_SAMPLES - 1, 400, 2.0);
        assert!(derive_tuning_candidate(&corpus).is_none());
    }

    #[test]
    fn derive_recovers_systematic_factor() {
        // Actuals are 2x predicted -> response_floor should ~double (400 -> 800).
        let corpus = biased_corpus(TUNING_MIN_SAMPLES, 400, 2.0);
        let candidate = derive_tuning_candidate(&corpus).expect("candidate above minimum");
        assert_eq!(candidate.response_floor, 800);
        assert_eq!(candidate.manual_floor, 1600);
        assert_eq!(candidate.schema_tokens, 90);
        assert_eq!(candidate.invoke_tokens, 160);
        assert_eq!(candidate.estimator_version, CURRENT_ESTIMATOR_VERSION);
    }

    #[test]
    fn derive_is_deterministic() {
        let a = derive_tuning_candidate(&biased_corpus(20, 400, 1.7)).unwrap();
        let b = derive_tuning_candidate(&biased_corpus(20, 400, 1.7)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn validate_accepts_when_held_out_mae_drops_by_margin() {
        // In force: static 400; actuals cluster near 800. A candidate of 800
        // drives held-out MAE to ~0 -> a >20% reduction -> accepted.
        let held_out = biased_corpus(10, 400, 2.0);
        let candidate = derive_tuning_candidate(&biased_corpus(TUNING_MIN_SAMPLES, 400, 2.0))
            .expect("candidate");
        assert!(validate_candidate(
            &candidate,
            &held_out,
            STATIC_RESPONSE_FLOOR
        ));
    }

    #[test]
    fn validate_rejects_worse_candidate() {
        // Actuals ARE the static floor (unbiased ~400). A candidate that moved the
        // floor to 800 INCREASES held-out MAE -> rejected.
        let held_out = biased_corpus(10, 400, 1.0);
        let worse = TunedEstimateConstants {
            response_floor: 800,
            manual_floor: 1600,
            schema_tokens: 90,
            invoke_tokens: 160,
            estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
            sample_size: 12,
            error_before: 0.0,
            error_after: 0.0,
            tuned_at_ms: 0,
        };
        assert!(!validate_candidate(
            &worse,
            &held_out,
            STATIC_RESPONSE_FLOOR
        ));
    }

    #[test]
    fn validate_rejects_marginal_candidate() {
        // A candidate that improves MAE by < 20% must be rejected (the SC-002 bar
        // is a margin, not "strictly drops"). In force 400, actuals ~440; the
        // exact candidate 440 would drop MAE 100% but a candidate of 410 only
        // closes 25% — construct one that closes < 20%.
        let held_out = biased_corpus(10, 400, 1.1); // actuals ~440, in-force MAE ~40
        // Candidate floor 407 -> MAE ~33 -> ~17.5% reduction, below the 20% bar.
        let marginal = TunedEstimateConstants {
            response_floor: 407,
            manual_floor: 814,
            schema_tokens: 46,
            invoke_tokens: 81,
            estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
            sample_size: 12,
            error_before: 0.0,
            error_after: 0.0,
            tuned_at_ms: 0,
        };
        assert!(!validate_candidate(
            &marginal,
            &held_out,
            STATIC_RESPONSE_FLOOR
        ));
    }

    #[test]
    fn verdict_tuned_on_biased_accumulating_on_unbiased() {
        // Biased corpus (actuals 2x) reaches Tuned with a held-out artifact.
        let biased = biased_corpus(40, 400, 2.0);
        let (verdict, candidate) = compute_calibration_verdict(&biased, STATIC_RESPONSE_FLOOR);
        match verdict {
            CalibrationVerdict::Tuned {
                error_before,
                error_after,
                ..
            } => assert!(error_after < error_before, "tuned must reduce held-out MAE"),
            other => panic!("biased corpus must reach Tuned, got {other:?}"),
        }
        assert!(candidate.is_some(), "accepted candidate must be returned");

        // Unbiased corpus (actuals == predicted == static floor) stays Accumulating
        // -> no harmful tuning applied.
        let unbiased = biased_corpus(40, 400, 1.0);
        let (verdict, candidate) = compute_calibration_verdict(&unbiased, STATIC_RESPONSE_FLOOR);
        assert!(
            matches!(verdict, CalibrationVerdict::Accumulating { .. }),
            "unbiased corpus must NOT tune, got {verdict:?}"
        );
        assert!(candidate.is_none());
    }

    #[test]
    fn render_never_emits_tuned_without_artifact() {
        assert_eq!(
            render_calibration_verdict(&CalibrationVerdict::Deferred),
            "deferred"
        );
        assert_eq!(
            render_calibration_verdict(&CalibrationVerdict::Accumulating { n: 3, min: 12 }),
            "accumulating (3/12)"
        );
        let tuned = render_calibration_verdict(&CalibrationVerdict::Tuned {
            sample_size: 40,
            error_before: 400.0,
            error_after: 10.0,
        });
        assert!(tuned.starts_with("tuned (error: 400.0 -> 10.0 tok"));
        assert!(tuned.contains("n=40"));
    }
}
