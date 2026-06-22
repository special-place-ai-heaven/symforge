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

use super::consts::{COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS};
use super::ledger_store::{CURRENT_ESTIMATOR_VERSION, StoredLedgerRecord, TunedEstimateConstants};
use super::types::{AdmissionDecision, StelLedgerEvent};

/// Minimum ledger rows before offline review is considered sample-adequate.
pub const TUNING_REVIEW_MIN_EVENTS: usize = 5;

/// Minimum CURRENT-estimator-version samples that must land in EACH of the
/// derive slice and the held-out validation slice (FR-004 documented minimum).
///
/// D10 fix: this is the per-SLICE floor, not the whole-corpus floor. A candidate
/// is split into a derive half + a held-out half (FR-005), so the FULL corpus
/// must carry at least `2 * TUNING_MIN_SAMPLES` rows before a tuning is even
/// attempted (see [`TUNING_MIN_CORPUS`]); below that the surface stays
/// `Accumulating` against the TRUE threshold (`2 * MIN`), so `n <= min` always
/// holds and the absurd `accumulating (18/12)` (n > min) can never render. 12
/// per slice — small enough to reach in real dogfooding, large enough that a
/// half's MAE is not dominated by a single event.
pub const TUNING_MIN_SAMPLES: usize = 12;

/// Minimum FULL-corpus size before a tuning candidate may be derived (D10).
///
/// The corpus is split in half (older -> derive, newer -> validate), and EACH
/// half must carry at least [`TUNING_MIN_SAMPLES`] rows, so the whole corpus
/// needs `2 * TUNING_MIN_SAMPLES`. This is the threshold the `Accumulating
/// { n, min }` surface renders, so the displayed `min` is the TRUE bar a tune
/// crosses (no `n > min`).
pub const TUNING_MIN_CORPUS: usize = 2 * TUNING_MIN_SAMPLES;

/// SC-002 "meaningful margin" (research R5): a tuning candidate is accepted only
/// when it reduces held-out mean absolute prediction error (MAE) by AT LEAST this
/// fraction (relative) versus the constants currently in force. 0.20 == 20%.
///
/// This is the REJECT gate's bar: non-improving, marginally-improving (< 20%),
/// and worse candidates are all rejected, so calibration never makes the
/// predictor worse and never over-promotes the surface to `tuned` for a
/// rounding-error gain. Tunable: raise once real per-project data accrues.
pub const SC002_MAE_REDUCTION_MARGIN: f64 = 0.20;

// D13 (feature 013): `RETUNE_HYSTERESIS_MARGIN` was a defined+documented no-op —
// the validate gate already requires a candidate to beat the IN-FORCE correction
// (the active factor when a tuning is applied, else the identity 1.0) by the
// SC-002 margin, which IS the hysteresis: a re-tune must out-perform what is
// already live, not merely the static baseline. There is no second, higher bar
// to apply, so the constant aliased `SC002_MAE_REDUCTION_MARGIN` and added
// nothing. Deleted rather than dressed up as genuine hysteresis it never was.

/// One observed predicted-vs-actual response-token outcome — the unit the
/// auto-tune learns from. Decoupled from the storage row / in-memory event so
/// `derive_tuning_candidate` / `validate_candidate` are pure functions over a
/// fixed corpus (FR-012), unit-testable without a live store.
///
/// `predicted_response` is the predictor's PREDICTED RESPONSE OUTPUT for the
/// event — whatever sub-model produced it (byte-grounded read/edit OR the
/// plan-only floor) — and `actual_response` is what was really served. The
/// calibration learns the systematic ratio between them. `ts_ms` carries the
/// event time so the verdict can split OUT-OF-TIME (D11: train on the older
/// half, validate on the newer half) to catch estimator/codebase drift.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictionSample {
    /// Event time (ms since epoch), used for the deterministic out-of-time split.
    pub ts_ms: u64,
    /// Response tokens the estimator predicted for the event (its raw output).
    pub predicted_response: u32,
    /// Response tokens actually served (`chars/4` of the real body).
    pub actual_response: u32,
}

impl From<&StoredLedgerRecord> for PredictionSample {
    fn from(r: &StoredLedgerRecord) -> Self {
        Self {
            ts_ms: r.ts_ms,
            predicted_response: r.predicted_response_tokens,
            actual_response: r.actual_response_tokens,
        }
    }
}

impl From<&StelLedgerEvent> for PredictionSample {
    fn from(e: &StelLedgerEvent) -> Self {
        Self {
            ts_ms: e.ts_ms,
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
/// step may apply to the predictor's response output (bounded step, edge-case
/// "tuning oscillation / instability"). A factor outside `[1/CAP, CAP]` is
/// clamped, so one pass can never swing the prediction wildly on a small / noisy
/// sample; the next pass refines from the clamped value. 4.0 covers the observed
/// 40-194% dogfood prediction error (R5) without admitting absurd swings.
pub const CORRECTION_FACTOR_CAP: f64 = 4.0;

/// The identity correction: "no tuning in force". Applying it leaves the
/// predictor's response output unchanged, so the held-out MAE under it is the raw
/// predictor's residual — the `mae_before` baseline a candidate must beat, and
/// the in-force anchor when no validated tuning is active.
pub const NO_CORRECTION_FACTOR: f64 = 1.0;

/// Bound a raw correction factor into `[1/CAP, CAP]` (oscillation guard). The
/// derivation stores the BOUNDED factor so the live path and the held-out
/// scoring apply the identical correction — there is one source of truth.
fn bound_factor(factor: f64) -> f64 {
    factor.clamp(1.0 / CORRECTION_FACTOR_CAP, CORRECTION_FACTOR_CAP)
}

/// Apply a correction `factor` to one raw predicted-response figure, rounding to
/// nearest and clamping into `u32`. This is the SAME transform the live
/// predictor applies (`controller::estimate_economics_tuned`), so the held-out
/// MAE the validate gate scores equals the residual the live predictor receives.
///
/// `factor` is assumed already bounded (see [`bound_factor`]); callers that take
/// it straight off a derived candidate pass the stored bounded value.
pub fn apply_factor(predicted_response: u32, factor: f64) -> u32 {
    let scaled = (f64::from(predicted_response) * factor).round();
    if scaled <= 0.0 {
        0
    } else if scaled >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        scaled as u32
    }
}

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

/// Mean absolute prediction error (MAE) of a corpus when each sample's OWN raw
/// predicted-response is corrected by `factor`, in tokens (D8-ROOT).
///
/// This is the REAL residual the live predictor errs on: every sample carries
/// the prediction its sub-model actually produced (byte-grounded OR floor), and
/// we score `|round(predicted_i * factor) - actual_i|` — NOT a flat floor
/// predicted for every sample. With `factor == 1.0` this is the raw predictor's
/// residual (the `mae_before` baseline). Lower is better; this is the quantity
/// the held-out gate (FR-005) reduces, and it EQUALS the quantity the live path
/// receives once the factor is applied. Returns `None` for an empty slice.
fn held_out_mae(samples: &[PredictionSample], factor: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let total: f64 = samples
        .iter()
        .map(|s| {
            let corrected = apply_factor(s.predicted_response, factor);
            (f64::from(corrected) - f64::from(s.actual_response)).abs()
        })
        .sum();
    Some(total / samples.len() as f64)
}

/// Derive a candidate `response_correction_factor` from CURRENT-estimator-version
/// samples (FR-004, D8-ROOT). Pure + deterministic.
///
/// Algorithm (documented, robust, deterministic):
/// 1. Compute the robust systematic bias `f = median(actual_i / predicted_i)`
///    over the samples (median: outlier-robust, deterministic — see
///    [`median_actual_over_predicted`]). Each ratio compares the predictor's OWN
///    output (byte-grounded OR floor) to what was really served, so `f` is the
///    one multiplicative correction the predictor's response output needs.
/// 2. Bound `f` into `[1/CAP, CAP]` (oscillation guard, [`CORRECTION_FACTOR_CAP`])
///    and store it. The bounded factor is the single source of truth the live
///    path and the held-out scoring both apply, so the validated quantity equals
///    the live residual.
///
/// Does NOT touch the static floors or the fixed `schema`/`invoke`/`manual`
/// figures (D9): those are not the predictor's response output and carry no
/// validated correction.
///
/// Returns `None` below [`TUNING_MIN_SAMPLES`] (the per-SLICE minimum — a
/// derive/validate half must each clear it; the whole-corpus gate is
/// [`TUNING_MIN_CORPUS`] in [`compute_calibration_verdict`]) or when no sample
/// carries a usable (non-zero) prediction. The candidate's
/// `error_before`/`error_after` are left at 0.0 here — they are the held-out
/// artifact filled in by the verdict computation only AFTER validation accepts
/// it; a bare candidate is never `tuned` on its own.
///
/// D14 (hardening): this is `pub(crate)` — a TRAIN-SLICE-ONLY primitive. It
/// derives a factor from WHATEVER corpus it is handed, with no leakage-free
/// train/held-out split (that split lives only in [`compute_calibration_verdict`]).
/// Exposing it publicly invited a caller deriving AND validating on the same full
/// corpus — a leak that fabricates an optimistic verdict. Crate-internal callers
/// (only [`compute_calibration_verdict`], which feeds it the OLDER half) cannot
/// make that mistake; external callers must go through the verdict API, which owns
/// the split. There is no live leak today; this prevents a future one.
pub(crate) fn derive_tuning_candidate(
    samples: &[PredictionSample],
) -> Option<TunedEstimateConstants> {
    if samples.len() < TUNING_MIN_SAMPLES {
        return None;
    }
    let factor = bound_factor(median_actual_over_predicted(samples)?);
    Some(TunedEstimateConstants {
        response_correction_factor: factor,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: u32::try_from(samples.len()).unwrap_or(u32::MAX),
        error_before: 0.0,
        error_after: 0.0,
        tuned_at_ms: 0,
    })
}

/// Validate a `candidate` against a `held_out` slice it was NOT derived from
/// (FR-005, the REJECT gate, D8-ROOT). Returns `true` ONLY if the candidate's
/// `response_correction_factor`, applied to each held-out sample's OWN raw
/// prediction, reduces the REAL held-out MAE by at least the SC-002 margin
/// ([`SC002_MAE_REDUCTION_MARGIN`]) versus `in_force_factor` (the correction
/// currently applied — `1.0` when no tuning is active, or the active tuning's
/// `response_correction_factor` on a re-tune, which is the genuine hysteresis
/// anchor: a re-tune must beat what is already LIVE, not just the raw baseline).
///
/// Because both `mae_before` and `mae_after` are scored on the SAME residual the
/// live predictor errs on (each sample's own prediction, corrected), a `true`
/// here is an improvement the live predictor actually receives — never the old
/// flat-floor mirage. Non-improving, marginally-improving (< margin), and worse
/// candidates all return `false`: calibration never makes the predictor worse,
/// and never promotes the surface to `tuned` for a rounding-error gain.
pub fn validate_candidate(
    candidate: &TunedEstimateConstants,
    held_out: &[PredictionSample],
    in_force_factor: f64,
) -> bool {
    let Some(mae_before) = held_out_mae(held_out, in_force_factor) else {
        return false;
    };
    let Some(mae_after) = held_out_mae(held_out, candidate.response_correction_factor) else {
        return false;
    };
    if mae_before <= 0.0 {
        // The in-force correction is already exact on held-out data; nothing to
        // improve, so no candidate can clear a >0 relative-reduction bar.
        return false;
    }
    let relative_reduction = (mae_before - mae_after) / mae_before;
    relative_reduction >= SC002_MAE_REDUCTION_MARGIN
}

/// Compute the honest [`CalibrationVerdict`] for a corpus of current-estimator
/// samples (FR-009), running the real derive + held-out validate (FR-004/FR-005)
/// over a DETERMINISTIC OUT-OF-TIME split.
///
/// `in_force_factor` is the response correction currently applied (`1.0` when no
/// tuning is active, or the active tuning's `response_correction_factor` on a
/// re-tune — the hysteresis anchor). Returns the accepted candidate alongside the
/// verdict so the caller can persist it (T031); on reject / insufficient data the
/// candidate is `None`.
///
/// Gate (D10): the FULL corpus must carry at least [`TUNING_MIN_CORPUS`]
/// (`2 * TUNING_MIN_SAMPLES`) rows, so EACH half clears [`TUNING_MIN_SAMPLES`].
/// Below that the verdict is `Accumulating { n, min: TUNING_MIN_CORPUS }` — the
/// rendered `min` is the TRUE threshold a tune crosses, so `n <= min` always
/// holds (no `accumulating (18/12)`).
///
/// Split (D11): the corpus is ordered by `ts_ms` and partitioned OUT-OF-TIME —
/// the OLDER half derives, the NEWER half validates. This catches estimator /
/// codebase drift (a correction fit on stale data that no longer holds on recent
/// data is rejected) instead of the index-parity split that let train and test
/// see the same distribution. A fixed corpus yields a fixed order and therefore a
/// fixed split and verdict (FR-012). The candidate is derived ONLY from the older
/// slice and validated ONLY on the newer slice, so the artifact is honest (no
/// leakage).
pub fn compute_calibration_verdict(
    samples: &[PredictionSample],
    in_force_factor: f64,
) -> (CalibrationVerdict, Option<TunedEstimateConstants>) {
    let n = samples.len();
    if n == 0 {
        return (CalibrationVerdict::Deferred, None);
    }
    let accumulating = || {
        (
            CalibrationVerdict::Accumulating {
                n,
                min: TUNING_MIN_CORPUS,
            },
            None,
        )
    };
    if n < TUNING_MIN_CORPUS {
        return accumulating();
    }

    // Deterministic OUT-OF-TIME split (no shuffle, no RNG): order by event time,
    // train on the older half, validate on the newer half. A stable sort keeps
    // equal-`ts_ms` rows in their original (insertion) order for reproducibility.
    let mut ordered = samples.to_vec();
    ordered.sort_by_key(|s| s.ts_ms);
    let split = ordered.len() / 2;
    let (train, held_out) = ordered.split_at(split);

    let Some(mut candidate) = derive_tuning_candidate(train) else {
        return accumulating();
    };

    if !validate_candidate(&candidate, held_out, in_force_factor) {
        // Derived but did not clear the held-out bar: stay Accumulating (never a
        // false Tuned). A worse-than-baseline candidate lands here.
        return accumulating();
    }

    // Accepted: record the held-out before/after artifact ON the candidate so the
    // surface can read `tuned (before -> after)` honestly. error_before is the
    // in-force-correction MAE; error_after is the candidate's MAE — both the REAL
    // residual on held-out data (each sample's own prediction, corrected).
    let error_before = held_out_mae(held_out, in_force_factor).unwrap_or(0.0);
    let error_after = held_out_mae(held_out, candidate.response_correction_factor).unwrap_or(0.0);
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
    // No tuning is in force for the observational summary, so the in-force
    // correction is the identity factor 1.0 (the raw predictor's residual).
    let (verdict, _candidate) = compute_calibration_verdict(&samples, NO_CORRECTION_FACTOR);
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

    // -- Auto-tune pure math (US2 T027/T028/T029, D8-ROOT redesign) ---------

    /// Build a biased corpus where the predictor's OWN output is `predicted` for
    /// every event but the actual served size was `predicted * bias`, so the true
    /// response-correction factor is exactly `bias`. `ts_ms` increments so the
    /// out-of-time split (D11) has a well-defined older/newer ordering.
    fn biased_corpus(n: usize, predicted: u32, bias: f64) -> Vec<PredictionSample> {
        (0..n)
            .map(|i| PredictionSample {
                ts_ms: 1_000 + i as u64,
                predicted_response: predicted,
                actual_response: (f64::from(predicted) * bias).round() as u32,
            })
            .collect()
    }

    /// A `TunedEstimateConstants` carrying just a `factor` (the other fields are
    /// artifact/audit, irrelevant to a validate call).
    fn factor_candidate(factor: f64) -> TunedEstimateConstants {
        TunedEstimateConstants {
            response_correction_factor: factor,
            estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
            sample_size: 12,
            error_before: 0.0,
            error_after: 0.0,
            tuned_at_ms: 0,
        }
    }

    #[test]
    fn derive_returns_none_below_minimum() {
        let corpus = biased_corpus(TUNING_MIN_SAMPLES - 1, 400, 2.0);
        assert!(derive_tuning_candidate(&corpus).is_none());
    }

    #[test]
    fn derive_recovers_systematic_factor() {
        // Actuals are 2x the predictor's output -> the correction factor is 2.0.
        let corpus = biased_corpus(TUNING_MIN_SAMPLES, 400, 2.0);
        let candidate = derive_tuning_candidate(&corpus).expect("candidate above minimum");
        assert_eq!(candidate.response_correction_factor, 2.0);
        assert_eq!(candidate.estimator_version, CURRENT_ESTIMATOR_VERSION);
    }

    #[test]
    fn derive_bounds_factor_to_cap() {
        // A wildly under-predicting corpus (actuals 10x) must clamp at the CAP,
        // not admit a 10.0 swing (oscillation guard).
        let corpus = biased_corpus(TUNING_MIN_SAMPLES, 100, 10.0);
        let candidate = derive_tuning_candidate(&corpus).expect("candidate");
        assert_eq!(candidate.response_correction_factor, CORRECTION_FACTOR_CAP);
    }

    #[test]
    fn apply_factor_corrects_a_prediction() {
        // The transform the live predictor mirrors: round(predicted * factor).
        assert_eq!(apply_factor(400, 2.0), 800);
        assert_eq!(apply_factor(400, NO_CORRECTION_FACTOR), 400);
        assert_eq!(apply_factor(333, 1.5), 500); // 499.5 -> 500 (round to nearest)
    }

    #[test]
    fn derive_is_deterministic() {
        let a = derive_tuning_candidate(&biased_corpus(20, 400, 1.7)).unwrap();
        let b = derive_tuning_candidate(&biased_corpus(20, 400, 1.7)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn validate_accepts_when_held_out_real_residual_drops_by_margin() {
        // Held-out actuals cluster at 2x the predictor's 400 output (real residual
        // ~400 under the identity correction). A factor of 2.0 drives the real
        // residual to ~0 -> a >20% reduction -> accepted.
        let held_out = biased_corpus(10, 400, 2.0);
        let candidate = factor_candidate(2.0);
        assert!(validate_candidate(
            &candidate,
            &held_out,
            NO_CORRECTION_FACTOR
        ));
    }

    #[test]
    fn validate_rejects_worse_candidate() {
        // Actuals already match the predictor's output (unbiased). A factor of 2.0
        // INCREASES the real held-out residual -> rejected (never makes it worse).
        let held_out = biased_corpus(10, 400, 1.0);
        let worse = factor_candidate(2.0);
        // Real residual under identity is ~0; under 2.0 it explodes.
        assert!(
            held_out_mae(&held_out, 2.0).unwrap()
                > held_out_mae(&held_out, NO_CORRECTION_FACTOR).unwrap()
        );
        assert!(!validate_candidate(&worse, &held_out, NO_CORRECTION_FACTOR));
    }

    #[test]
    fn validate_rejects_marginal_candidate() {
        // A factor that closes < 20% of the real residual is rejected (the SC-002
        // bar is a margin, not "strictly drops"). Predictor outputs 400, actuals
        // ~440 (real residual ~40 under identity). A factor of 1.0175 -> predicted
        // ~407 -> residual ~33 -> ~17.5% reduction, below the 20% bar.
        let held_out = biased_corpus(10, 400, 1.1); // actuals ~440
        let marginal = factor_candidate(1.0175);
        let before = held_out_mae(&held_out, NO_CORRECTION_FACTOR).unwrap();
        let after = held_out_mae(&held_out, marginal.response_correction_factor).unwrap();
        let relative = (before - after) / before;
        assert!(
            relative < SC002_MAE_REDUCTION_MARGIN,
            "fixture must be below the margin to exercise reject (got {relative:.3})"
        );
        assert!(!validate_candidate(
            &marginal,
            &held_out,
            NO_CORRECTION_FACTOR
        ));
    }

    #[test]
    fn verdict_tuned_on_biased_accumulating_on_unbiased() {
        // Biased corpus (actuals 2x the predictor's output) reaches Tuned, and the
        // held-out artifact is the REAL residual reduction.
        let biased = biased_corpus(TUNING_MIN_CORPUS, 400, 2.0);
        let (verdict, candidate) = compute_calibration_verdict(&biased, NO_CORRECTION_FACTOR);
        match verdict {
            CalibrationVerdict::Tuned {
                error_before,
                error_after,
                ..
            } => assert!(
                error_after < error_before,
                "tuned must reduce the real held-out residual"
            ),
            other => panic!("biased corpus must reach Tuned, got {other:?}"),
        }
        assert!(candidate.is_some(), "accepted candidate must be returned");

        // Unbiased corpus (actuals == the predictor's output) stays Accumulating
        // -> no harmful tuning (factor ~1.0, gain < margin).
        let unbiased = biased_corpus(TUNING_MIN_CORPUS, 400, 1.0);
        let (verdict, candidate) = compute_calibration_verdict(&unbiased, NO_CORRECTION_FACTOR);
        assert!(
            matches!(verdict, CalibrationVerdict::Accumulating { .. }),
            "unbiased corpus must NOT tune, got {verdict:?}"
        );
        assert!(candidate.is_none());
    }

    #[test]
    fn accumulating_renders_true_threshold_no_n_gt_min() {
        // D10: a corpus in [MIN, 2*MIN) is Accumulating against the TRUE 2*MIN
        // threshold, so n <= min always holds (no `accumulating (18/12)`).
        for n in TUNING_MIN_SAMPLES..TUNING_MIN_CORPUS {
            let corpus = biased_corpus(n, 400, 2.0);
            let (verdict, candidate) = compute_calibration_verdict(&corpus, NO_CORRECTION_FACTOR);
            match verdict {
                CalibrationVerdict::Accumulating { n: got_n, min } => {
                    assert_eq!(got_n, n);
                    assert_eq!(
                        min, TUNING_MIN_CORPUS,
                        "min must be the true 2*MIN threshold"
                    );
                    assert!(got_n <= min, "n ({got_n}) must never exceed min ({min})");
                }
                other => panic!("n={n} in [MIN, 2*MIN) must be Accumulating, got {other:?}"),
            }
            assert!(candidate.is_none(), "no tuning below the corpus gate");
        }
    }

    #[test]
    fn out_of_time_split_rejects_drifted_correction() {
        // D11: train on the OLDER half, validate on the NEWER half. If the bias the
        // older half exhibits has DRIFTED away by the newer half, the correction
        // fit on stale data must NOT validate on recent data -> stays Accumulating.
        // Older half: actuals 2x (factor ~2.0); newer half: already unbiased (~1x),
        // so a 2.0 correction makes the recent residual WORSE -> rejected.
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
    }

    #[test]
    fn render_never_emits_tuned_without_artifact() {
        assert_eq!(
            render_calibration_verdict(&CalibrationVerdict::Deferred),
            "deferred"
        );
        assert_eq!(
            render_calibration_verdict(&CalibrationVerdict::Accumulating { n: 3, min: 24 }),
            "accumulating (3/24)"
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
