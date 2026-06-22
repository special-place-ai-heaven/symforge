//! STEL L2 economics controller — evaluate [`StelPlan`] → [`StelDecision`] / [`StelEstimate`].
//!
//! Phase 2 P2-S3: normative admission states (`serve | degrade | bypass | cache_hit`).

use crate::protocol::format::{competent_manual_baseline_chars, estimate_tokens_from_chars};
use crate::protocol::session::SessionContext;

use super::ledger_store::TunedEstimateConstants;
use super::types::{
    AdmissionDecision, GoldenRouteRow, IndexRef, RouteConfidence, StelBypassBody, StelCacheBody,
    StelDecision, StelEstimate, StelPlan, StelPlanStep, StelRequest,
};

// D3-ROOT extract-up: the four token-economics floors moved to the
// protocol-free `crate::stel_core::consts` (the ONLY tie that bound the
// `calibration` math to this server-only controller). Re-export them here at
// their original `stel::controller::…` paths so every existing caller
// (`crate::stel::controller::COMPACT_SCHEMA_TOKENS`, the `stel::mod` re-export,
// `protocol::mod`, `handler`, the calibration-tuning integration test) and the
// controller's own production code below resolve unchanged.
pub use crate::stel_core::consts::{
    COMPACT_INVOKE_TOKENS, COMPACT_SCHEMA_TOKENS, STATIC_MANUAL_FLOOR, STATIC_RESPONSE_FLOOR,
};

/// Minimum predicted net vs manual before `serve` (schema `margin_high`).
pub const SERVE_MARGIN_TOKENS: i32 = 50;
/// Predicted net at or below this (but above zero) triggers `degrade` (`margin_low`).
pub const DEGRADE_MARGIN_LOW_TOKENS: i32 = SERVE_MARGIN_TOKENS;
/// Default capped response budget applied on degrade when request omits `max_tokens`.
pub const DEGRADE_DEFAULT_MAX_TOKENS: u32 = 400;

/// Numerator of the grounded structured-response fraction: SymForge's structured
/// serve (symbol body / outline / windowed slice) is smaller than the competent
/// windowed manual read it replaces. Calibrated conservatively at ~60% of the
/// competent-manual baseline so a grounded response prediction never *over*-claims
/// savings (a smaller predicted response would inflate `predicted_net`).
const GROUNDED_RESPONSE_FRACTION_NUM: u64 = 3;
/// Denominator of the grounded structured-response fraction (see numerator).
const GROUNDED_RESPONSE_FRACTION_DEN: u64 = 5;

/// Heuristic token cost of an edit response's fixed footer — the replace
/// confirmation line (`path — replaced kind \`name\` (N → M bytes)`), the
/// `Write semantics:` line, and a small allowance for stale-reference / impact
/// lines. Added to a preview's echoed-body cost, and IS the whole predicted
/// response for a committed apply (which does NOT re-emit the body). Coarse
/// estimate, not a measured count.
const EDIT_RESPONSE_FOOTER_TOKENS: u64 = 60;

/// Token economics breakdown for one planned invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EconomicsBreakdown {
    pub predicted_response_tokens: u32,
    pub predicted_manual_tokens: u32,
    pub predicted_schema_tokens: u32,
    pub predicted_invoke_tokens: u32,
    pub predicted_symforge_tokens: u32,
    pub predicted_net_vs_manual: i32,
}

/// Evaluate L2 admission for a draft plan (does not execute L3).
pub fn evaluate_plan(request: &StelRequest, plan: &StelPlan) -> StelDecision {
    evaluate_plan_with_session(request, plan, None)
}

/// Evaluate L2 admission with optional in-process session context for cache-hit detection.
///
/// Static-floor economics: identical to the pre-013 behaviour. The tuned variant
/// [`evaluate_plan_tuned`] threads a validated tuning into the same logic; this
/// is the `tuned = None` entry every existing caller (and golden-replay) takes,
/// so routing/policy is byte-exact unless a tuning is explicitly in force.
pub fn evaluate_plan_with_session(
    request: &StelRequest,
    plan: &StelPlan,
    session: Option<&SessionContext>,
) -> StelDecision {
    evaluate_plan_tuned(request, plan, session, None)
}

/// Evaluate L2 admission with an optional validated tuning in force (feature 013,
/// T032 / FR-006). When `tuned` is `Some`, the economics that drive the
/// serve/degrade/bypass branches use the tuned constants, so the adaptive
/// decision reflects better-grounded numbers; when `None`, the static floors
/// apply and the decision is byte-identical to the pre-013 path. Routing
/// correctness, policy/deny, and safety guards are untouched — only the
/// token-estimate inputs change (FR-007). The caller must pass only an in-force
/// tuning (see [`active_tuning_in_force`]).
pub fn evaluate_plan_tuned(
    request: &StelRequest,
    plan: &StelPlan,
    session: Option<&SessionContext>,
    tuned: Option<&TunedEstimateConstants>,
) -> StelDecision {
    if let Some(bypass) = detect_pff_bypass(request) {
        return decision_from_pff_bypass(plan, request, bypass);
    }

    if let Some(session) = session
        && let Some(cache) = detect_session_cache_hit(plan, session)
    {
        return decision_from_cache_hit(plan, request, cache);
    }

    let economics = estimate_economics_tuned(plan, tuned);
    let net = economics.predicted_net_vs_manual;

    if net <= 0 {
        let bypass = economics_bypass_body(plan, &economics);
        return StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Bypass,
            decision_reason: format!(
                "predicted_net={net} <= 0; economics bypass via host_read `{}`",
                bypass.path
            ),
            effective_max_tokens: request.max_tokens,
            degrade_flags: vec![],
            steps: None,
            bypass: Some(bypass),
            cache: None,
        };
    }

    let mandatory_degrade =
        plan.confidence == RouteConfidence::Fallback && net < SERVE_MARGIN_TOKENS;
    let economics_degrade = net <= DEGRADE_MARGIN_LOW_TOKENS;

    if mandatory_degrade || economics_degrade {
        let mut degrade_flags = Vec::new();
        if mandatory_degrade {
            degrade_flags.push("fallback_mandatory".to_string());
        }
        if plan
            .steps
            .iter()
            .any(|step| step.tool == "get_file_context")
        {
            degrade_flags.push("outline_only".to_string());
        } else {
            degrade_flags.push("max_tokens_cap".to_string());
        }
        degrade_flags.sort();
        degrade_flags.dedup();

        let effective_max_tokens = request
            .max_tokens
            .or(Some(DEGRADE_DEFAULT_MAX_TOKENS))
            .map(|cap| cap.min(DEGRADE_DEFAULT_MAX_TOKENS));

        let reason = if mandatory_degrade {
            format!(
                "predicted_net={net} < margin={}; fallback confidence requires degrade",
                SERVE_MARGIN_TOKENS
            )
        } else {
            format!(
                "predicted_net={net} <= margin_low={}",
                DEGRADE_MARGIN_LOW_TOKENS
            )
        };

        return StelDecision {
            plan_id: plan.plan_id.clone(),
            decision: AdmissionDecision::Degrade,
            decision_reason: reason,
            effective_max_tokens,
            degrade_flags,
            steps: Some(plan.steps.clone()),
            bypass: None,
            cache: None,
        };
    }

    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision: AdmissionDecision::Serve,
        decision_reason: format!("predicted_net={net} > margin={}", SERVE_MARGIN_TOKENS),
        effective_max_tokens: request.max_tokens,
        degrade_flags: vec![],
        steps: Some(plan.steps.clone()),
        bypass: None,
        cache: None,
    }
}

fn decision_from_pff_bypass(
    plan: &StelPlan,
    request: &StelRequest,
    bypass: StelBypassBody,
) -> StelDecision {
    let economics = economics_for_bypass(&bypass);
    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision: AdmissionDecision::Bypass,
        decision_reason: format!(
            "{}; predicted_net={}",
            bypass.reason, economics.predicted_net_vs_manual
        ),
        effective_max_tokens: request.max_tokens,
        degrade_flags: vec![],
        steps: None,
        bypass: Some(bypass),
        cache: None,
    }
}

fn decision_from_cache_hit(
    plan: &StelPlan,
    request: &StelRequest,
    cache: StelCacheBody,
) -> StelDecision {
    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision: AdmissionDecision::CacheHit,
        decision_reason: format!(
            "session cache hit for {} `{}` (prior_tokens={})",
            cache.kind, cache.path, cache.prior_tokens
        ),
        effective_max_tokens: request.max_tokens,
        degrade_flags: vec![],
        steps: None,
        bypass: None,
        cache: Some(cache),
    }
}

fn detect_session_cache_hit(plan: &StelPlan, session: &SessionContext) -> Option<StelCacheBody> {
    let step = plan.steps.first()?;
    session
        .try_cache_hit_from_stel_step(&step.tool, &step.args)
        .map(|meta| StelCacheBody {
            kind: meta.kind.to_string(),
            path: meta.path,
            name: meta.name,
            prior_tokens: meta.prior_tokens,
            session_age_secs: meta.session_age_secs,
        })
}

fn economics_bypass_body(plan: &StelPlan, economics: &EconomicsBreakdown) -> StelBypassBody {
    let path = plan_primary_path(plan).unwrap_or_else(|| "unknown".to_string());
    StelBypassBody {
        action: "host_read".to_string(),
        path: path.clone(),
        start_line: 1,
        end_line: Some(80),
        predicted_manual_tokens: economics.predicted_manual_tokens,
        predicted_symforge_tokens: economics.predicted_symforge_tokens,
        reason: format!(
            "predicted_net={} <= 0; host_read `{path}` cheaper than serve",
            economics.predicted_net_vs_manual
        ),
    }
}

fn plan_primary_path(plan: &StelPlan) -> Option<String> {
    plan.steps.first().and_then(primary_path_from_step)
}

fn primary_path_from_step(step: &StelPlanStep) -> Option<String> {
    step.args
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .or_else(|| {
            step.args
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
}

/// Evaluate L2 admission for a structural edit plan (no NL P-FF bypass path).
///
/// A structural edit always `serve`s. Unlike a READ — where a non-positive net
/// means "host-read the file directly, it is cheaper" — an EDIT has no host-side
/// substitute: only running the tool performs the mutation. So the read ladder's
/// `bypass` ("just host_read this path") and `degrade` (cap the response budget)
/// are incoherent for a mutation, and the prior code never reached them anyway
/// while the flat 520/900 floor pinned net at a permanent ~255. Now that the
/// edit's economics are grounded in the new-body byte length (`grounded_edit_tokens`),
/// a tiny edit's net legitimately goes non-positive; we surface that grounded net
/// honestly in `decision_reason` but still `serve`, because the agent must run the
/// tool to apply the change. The grounded `EconomicsBreakdown` still flows into the
/// envelope (`predicted_tokens` / `est_net_vs_manual`) via `metrics_for_decision`.
pub fn evaluate_edit_plan(plan: &StelPlan) -> StelDecision {
    let economics = estimate_economics(plan);
    let net = economics.predicted_net_vs_manual;
    StelDecision {
        plan_id: plan.plan_id.clone(),
        decision: AdmissionDecision::Serve,
        decision_reason: format!("predicted_net={net} (est.); structural edit always serves"),
        effective_max_tokens: None,
        degrade_flags: vec![],
        steps: Some(plan.steps.clone()),
        bypass: None,
        cache: None,
    }
}

/// Build preview economics for `preview: true` (L1+L2 only).
pub fn build_estimate(
    request: &StelRequest,
    plan: &StelPlan,
    decision: &StelDecision,
) -> StelEstimate {
    let economics = if decision.decision == AdmissionDecision::Bypass {
        decision
            .bypass
            .as_ref()
            .map(economics_for_bypass)
            .unwrap_or_else(|| estimate_economics(plan))
    } else {
        estimate_economics(plan)
    };
    let recommended = decision.decision == AdmissionDecision::Serve
        && economics.predicted_net_vs_manual > SERVE_MARGIN_TOKENS;
    let _ = request;
    StelEstimate {
        plan_id: plan.plan_id.clone(),
        decision: decision.decision,
        predicted_response_tokens: economics.predicted_response_tokens,
        predicted_manual_tokens: economics.predicted_manual_tokens,
        predicted_schema_tokens: economics.predicted_schema_tokens,
        predicted_invoke_tokens: economics.predicted_invoke_tokens,
        predicted_net_vs_manual: economics.predicted_net_vs_manual,
        recommended,
    }
}

/// Sum step estimates + compact surface overhead into a manual-vs-symforge comparison.
///
/// US5 grounding (010 FR-014, D2): when a step carries real [`IndexRef`] byte
/// sizes (populated by the index-aware serve path before the gate), the
/// per-step response/manual token estimates are derived from those real bytes
/// via the byte-grounded estimator ([`competent_manual_baseline_chars`]) rather
/// than the planner's `400/800` plan-only constants. This is why predictions now
/// vary with the actual work and why the adaptive economics branches
/// (`bypass`/`degrade`) become reachable for small/cheap inputs (a tiny file's
/// manual baseline is below SymForge's fixed schema+invoke overhead, so
/// `predicted_net` correctly goes non-positive). A step with no `index_refs`
/// (plan-only callers, preview, unit fixtures) keeps the deterministic per-step
/// `est_response_tokens`/`est_manual_tokens` floor unchanged. The result is still
/// an *estimate* (predicted from real bytes, not a measured token count) — the
/// envelope keeps the `est_`/`heuristic` label (relabel != measure).
pub fn estimate_economics(plan: &StelPlan) -> EconomicsBreakdown {
    // The static-floor path: identical to the pre-013 behaviour, so golden-replay
    // and every existing prediction are byte-exact unless a validated tuning is
    // explicitly threaded through `estimate_economics_tuned`.
    estimate_economics_tuned(plan, None)
}

/// Select the tuned constants that are IN FORCE for the current estimator (R3).
///
/// Returns `Some` only when `tuned` is present AND its `estimator_version`
/// matches `current_version`; otherwise `None`, so a stale-version tuned set
/// never silently applies (FR-006 in-force rule). Pure — unit-testable without a
/// store.
pub fn active_tuning_in_force(
    tuned: Option<TunedEstimateConstants>,
    current_version: &str,
) -> Option<TunedEstimateConstants> {
    tuned.filter(|c| c.estimator_version == current_version)
}

/// Economics with an optional validated tuning in force (feature 013, T032,
/// D8-ROOT).
///
/// When `tuned` is `Some`, the calibration's single
/// `response_correction_factor` is applied to the FINAL `predicted_response`
/// AFTER grounding-or-floor — so BOTH the byte-grounded read/edit path AND the
/// plan-only floor path are corrected by the SAME factor the held-out validation
/// scored against. The fixed `schema`(45) / `invoke`(80) overheads and the manual
/// baseline are LEFT UNCHANGED (D9): they are not the predictor's response
/// output and carry no validated correction. When `tuned` is `None` the factor is
/// the identity and the result is byte-identical to the pre-013 path.
///
/// The correction is applied to the SUMMED per-step response (one factor over the
/// whole plan's response output), mirroring `calibration::apply_factor`, so the
/// live residual equals the validated residual.
///
/// The caller is responsible for passing only an IN-FORCE tuning (matching the
/// current estimator version) — see [`active_tuning_in_force`].
pub fn estimate_economics_tuned(
    plan: &StelPlan,
    tuned: Option<&TunedEstimateConstants>,
) -> EconomicsBreakdown {
    let (predicted_response_raw, predicted_manual_tokens) = plan
        .steps
        .iter()
        .map(grounded_step_tokens)
        .fold((0u32, 0u32), |(resp, manual), (step_resp, step_manual)| {
            (
                resp.saturating_add(step_resp),
                manual.saturating_add(step_manual),
            )
        });

    // D8-ROOT: apply the validated response-correction factor to the FINAL
    // predicted response (byte-grounded OR floor — whichever produced it). The
    // schema/invoke/manual figures are NOT scaled (D9).
    let predicted_response_tokens = match tuned {
        Some(c) => crate::stel_core::calibration::apply_factor(
            predicted_response_raw,
            c.response_correction_factor,
        ),
        None => predicted_response_raw,
    };

    let schema_tokens = COMPACT_SCHEMA_TOKENS;
    let invoke_tokens = COMPACT_INVOKE_TOKENS;
    let predicted_symforge_tokens = predicted_response_tokens
        .saturating_add(schema_tokens)
        .saturating_add(invoke_tokens);
    let predicted_net_vs_manual = predicted_manual_tokens as i32 - predicted_symforge_tokens as i32;
    EconomicsBreakdown {
        predicted_response_tokens,
        predicted_manual_tokens,
        predicted_schema_tokens: schema_tokens,
        predicted_invoke_tokens: invoke_tokens,
        predicted_symforge_tokens,
        predicted_net_vs_manual,
    }
}

/// Per-step `(predicted_response_tokens, predicted_manual_tokens)`.
///
/// Grounded read path: when `step.index_refs` is non-empty the manual baseline is
/// the competent windowed read of the real target bytes, and the predicted
/// response is the structured-serve fraction of that baseline (SymForge's symbol/
/// outline/slice is smaller than a windowed manual read).
///
/// Grounded edit path: a `replace_symbol_body` step carries the NEW symbol source
/// (`body`) byte length as its `IndexRef` (see `build_edit_plan`). Its response
/// model differs from a read's structured fraction — see [`grounded_edit_tokens`]
/// — so edits branch here instead of reusing the read fraction, while still
/// sharing the byte-grounded MANUAL baseline machinery.
///
/// Falls back to the plan-only `est_*` constants when no real size is known
/// (preserves determinism for plan-only callers and existing fixtures).
///
/// D8-ROOT: this computes the predictor's RAW per-step output (byte-grounded or
/// floor). The validated `response_correction_factor` is applied ONCE to the
/// summed plan response in [`estimate_economics_tuned`], not per-step here, so
/// both sub-models share the same single correction the held-out validation
/// scored against. The manual baseline is never corrected (D9).
fn grounded_step_tokens(step: &StelPlanStep) -> (u32, u32) {
    if step.index_refs.is_empty() {
        // Plan-only FLOOR path: the static per-step `est_*` constants (400/800),
        // byte-exact with the pre-013 behaviour. The response correction (if a
        // tuning is in force) is applied to the plan sum by the caller.
        return (step.est_response_tokens, step.est_manual_tokens);
    }
    let total_raw_chars: usize = step
        .index_refs
        .iter()
        .map(|index_ref| index_ref.raw_chars as usize)
        .sum();
    // All compact `symforge_edit` ops carry the NEW-content byte length as their
    // IndexRef (`build_edit_plan`): the replacement body, the inserted symbol
    // source, or the within-symbol replacement text. They share the edit response
    // model (preview echoes the new content + footer; apply echoes only a footer),
    // so route every edit tool through the same grounded edit estimator rather
    // than the read fraction. Keeping them on `grounded_edit_tokens` keeps
    // insert/within predictions honest and byte-scaled, not flat-floored.
    if matches!(
        step.tool.as_str(),
        "replace_symbol_body" | "insert_symbol" | "edit_within_symbol"
    ) {
        return grounded_edit_tokens(step, total_raw_chars);
    }
    grounded_tokens_from_raw_chars(total_raw_chars)
}

/// Map an edit step's NEW-body byte length to `(predicted_response, predicted_manual)`.
///
/// MANUAL baseline: editing a symbol by hand means reading its span and rewriting
/// it; the new-body length is the plan-time proxy for that span (the old on-disk
/// span is not resolved until apply). Flows through the SAME
/// `competent_manual_baseline_chars` + `estimate_tokens_from_chars` estimator the
/// read path uses, so there is one source of truth for the manual baseline.
///
/// RESPONSE: scales with what the edit echoes back, which depends on preview vs
/// apply (read from the step's planned `dry_run` arg):
/// - PREVIEW (`dry_run == true`): echoes the full new body plus a diff/footer ⇒
///   `tokens(body) + footer`.
/// - APPLY (`dry_run == false`): echoes only a confirmation + impact footer; the
///   body is NOT re-emitted ⇒ `footer`.
///
/// So a preview always predicts more response than the equivalent apply, and both
/// scale with body size only through the preview's echoed body — exactly the
/// honesty fix for the old flat 520/900 floor.
fn grounded_edit_tokens(step: &StelPlanStep, body_chars: usize) -> (u32, u32) {
    let manual_chars = competent_manual_baseline_chars(body_chars);
    let manual_tokens = estimate_tokens_from_chars(manual_chars);
    let is_preview = step
        .args
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let response_tokens = if is_preview {
        estimate_tokens_from_chars(body_chars).saturating_add(EDIT_RESPONSE_FOOTER_TOKENS)
    } else {
        EDIT_RESPONSE_FOOTER_TOKENS
    };
    (clamp_u32(response_tokens), clamp_u32(manual_tokens))
}

/// Map real target byte length to `(predicted_response_tokens, predicted_manual_tokens)`.
///
/// `manual` = competent windowed read of the real bytes (what a disciplined
/// agent would read by hand); `response` = the structured-serve fraction of that
/// baseline. Both flow through the existing byte-grounded estimator so there is a
/// single source of truth for the manual baseline (no duplicate estimator).
fn grounded_tokens_from_raw_chars(raw_chars: usize) -> (u32, u32) {
    let manual_chars = competent_manual_baseline_chars(raw_chars);
    let manual_tokens = estimate_tokens_from_chars(manual_chars);
    let response_tokens = manual_tokens.saturating_mul(GROUNDED_RESPONSE_FRACTION_NUM)
        / GROUNDED_RESPONSE_FRACTION_DEN;
    (clamp_u32(response_tokens), clamp_u32(manual_tokens))
}

/// Saturating `u64` -> `u32` (token counts never legitimately exceed `u32::MAX`).
fn clamp_u32(value: u64) -> u32 {
    value.min(u32::MAX as u64) as u32
}

/// Build a grounded [`IndexRef`] for a resolved target. Used by the index-aware
/// serve path to stamp real byte sizes onto plan steps before the L2 gate.
pub fn index_ref_for_target(path: impl Into<String>, raw_chars: u64) -> IndexRef {
    IndexRef {
        path: path.into(),
        raw_chars,
    }
}

pub fn economics_for_bypass(bypass: &StelBypassBody) -> EconomicsBreakdown {
    let predicted_symforge_tokens = bypass
        .predicted_symforge_tokens
        .saturating_add(COMPACT_SCHEMA_TOKENS)
        .saturating_add(COMPACT_INVOKE_TOKENS);
    let predicted_net_vs_manual =
        bypass.predicted_manual_tokens as i32 - predicted_symforge_tokens as i32;
    EconomicsBreakdown {
        predicted_response_tokens: 0,
        predicted_manual_tokens: bypass.predicted_manual_tokens,
        predicted_schema_tokens: COMPACT_SCHEMA_TOKENS,
        predicted_invoke_tokens: COMPACT_INVOKE_TOKENS,
        predicted_symforge_tokens,
        predicted_net_vs_manual,
    }
}

/// Policy P-FF: whole-file review queries bypass SymForge serve (A-012 / golden corpus).
pub fn detect_pff_bypass(request: &StelRequest) -> Option<StelBypassBody> {
    let query = request.query.trim();
    let lower = query.to_ascii_lowercase();
    if !is_pff_query(&lower) {
        return None;
    }
    let path = extract_pff_path(query, &lower)?;
    Some(StelBypassBody {
        action: "host_read".to_string(),
        path: path.clone(),
        start_line: 1,
        end_line: None,
        predicted_manual_tokens: 320,
        predicted_symforge_tokens: COMPACT_SCHEMA_TOKENS + COMPACT_INVOKE_TOKENS + 256,
        reason: format!("policy=P-FF whole-file review; host_read `{path}`"),
    })
}

fn is_pff_query(lower: &str) -> bool {
    const PHRASES: &[&str] = &[
        "entire ",
        "whole ",
        "full file",
        "complete ",
        "line by line",
        "audit full ",
        "review entire ",
        "read complete ",
    ];
    PHRASES.iter().any(|phrase| lower.contains(phrase))
}

fn extract_pff_path(query: &str, lower: &str) -> Option<String> {
    for token in query.split_whitespace().rev() {
        let cleaned = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/');
        if cleaned.contains('.') && !cleaned.eq_ignore_ascii_case("line") {
            return Some(cleaned.to_string());
        }
    }
    for token in lower.split_whitespace() {
        if token.contains('.') {
            return Some(token.to_string());
        }
    }
    None
}

/// Golden-row helper: L2 decision should match fixture `expected_decision` for representative rows.
pub fn decision_matches_golden(
    row: &GoldenRouteRow,
    plan: &StelPlan,
    request: &StelRequest,
) -> bool {
    let decision = evaluate_plan(request, plan);
    decision.decision == row.expected_decision
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::stel::golden_replay::{GOLDEN_ROUTES_FIXTURE, load_golden_rows};
    use crate::stel::planner::build_plan;
    use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};

    fn row_by_id<'a>(rows: &'a [GoldenRouteRow], id: &str) -> &'a GoldenRouteRow {
        rows.iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("missing golden row {id}"))
    }

    fn request_for_row(row: &GoldenRouteRow) -> StelRequest {
        let mut request = row.to_request();
        request.intent = row.intent;
        request
    }

    fn low_net_plan() -> StelPlan {
        StelPlan {
            plan_id: "low-net".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Inferred,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
                est_response_tokens: 900,
                est_manual_tokens: 100,
                index_refs: vec![],
            }],
            suggested_followup: None,
        }
    }

    #[test]
    fn serve_rows_classify_as_serve() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        let rows = load_golden_rows(&path).expect("golden fixture");
        for id in [
            "cfg-if/t1_search",
            "cfg-if/t4_refs",
            "records/t4_refs",
            "compression/t5_dependents",
        ] {
            let row = row_by_id(&rows, id);
            let request = request_for_row(row);
            let plan = build_plan(&request);
            let decision = evaluate_plan(&request, &plan);
            assert_eq!(
                decision.decision,
                AdmissionDecision::Serve,
                "{id}: {:?}",
                decision.decision_reason
            );
            assert!(row.eligible_h6.unwrap_or(false));
            let economics = estimate_economics(&plan);
            assert!(
                economics.predicted_net_vs_manual > SERVE_MARGIN_TOKENS,
                "{id} net {}",
                economics.predicted_net_vs_manual
            );
        }
    }

    #[test]
    fn pff_rows_classify_as_bypass() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        let rows = load_golden_rows(&path).expect("golden fixture");
        for id in [
            "cfg-if/pff_whole_lib",
            "records/pff_whole_module",
            "is-plain/pff_whole_index",
            "compression/pff_whole_service",
        ] {
            let row = row_by_id(&rows, id);
            let request = request_for_row(row);
            let plan = build_plan(&request);
            let decision = evaluate_plan(&request, &plan);
            assert_eq!(decision.decision, AdmissionDecision::Bypass, "{id}");
            assert!(decision.bypass.is_some(), "{id} bypass body");
            let bypass = decision.bypass.as_ref().expect("bypass body");
            assert_eq!(
                bypass.end_line, None,
                "{id} P-FF bypass must request whole-file host read"
            );
            assert_eq!(row.eligible_h6, Some(false));
            let estimate = build_estimate(&request, &plan, &decision);
            assert!(!estimate.recommended, "{id} should not recommend serve");
        }
    }

    #[test]
    fn negative_net_non_pff_bypass_has_host_read_body() {
        let plan = low_net_plan();
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Bypass);
        let bypass = decision.bypass.as_ref().expect("economics bypass body");
        assert_eq!(bypass.path, "src/lib.rs");
        assert!(decision.decision_reason.contains("predicted_net="));
    }

    #[test]
    fn marginal_net_degrades_with_outline_only_flag() {
        let plan = StelPlan {
            plan_id: "degrade".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Inferred,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
                est_response_tokens: 400,
                est_manual_tokens: 530,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Degrade);
        assert!(decision.degrade_flags.contains(&"outline_only".to_string()));
        assert!(decision.effective_max_tokens.is_some());
    }

    #[test]
    fn fallback_confidence_forces_degrade_below_margin_high() {
        let plan = StelPlan {
            plan_id: "fallback".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Fallback,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_content".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
                est_response_tokens: 400,
                est_manual_tokens: 530,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Degrade);
        assert!(
            decision
                .degrade_flags
                .contains(&"fallback_mandatory".to_string())
        );
    }

    #[test]
    fn session_cache_hit_for_prefetched_symbol() {
        let session = SessionContext::new();
        session.record_symbol_fetch(
            "src/lib.rs",
            "cfg_if",
            crate::protocol::session::hash_symbol_params(None, None, None),
            128,
        );
        let plan = StelPlan {
            plan_id: "cache".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Exact,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_symbol".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs", "name": "cfg_if" }),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan_with_session(&request, &plan, Some(&session));
        assert_eq!(decision.decision, AdmissionDecision::CacheHit);
        let cache = decision.cache.as_ref().expect("cache body");
        assert_eq!(cache.kind, "symbol");
        assert_eq!(cache.prior_tokens, 128);
    }

    #[test]
    fn eligible_h6_serve_rows_remain_eligible() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        let rows = load_golden_rows(&path).expect("golden fixture");
        let eligible: Vec<_> = rows
            .iter()
            .filter(|row| {
                row.eligible_h6 == Some(true) && row.expected_decision == AdmissionDecision::Serve
            })
            .collect();
        assert!(eligible.len() >= 20);
        for row in eligible.iter().take(6) {
            let request = request_for_row(row);
            let plan = build_plan(&request);
            assert!(
                decision_matches_golden(row, &plan, &request),
                "eligible serve row {} decision mismatch",
                row.id
            );
        }
    }

    #[test]
    fn preview_estimate_carries_schema_and_invoke_costs() {
        let request = StelRequest {
            query: "who references cfg_if".to_string(),
            ..Default::default()
        };
        let plan = build_plan(&request);
        let decision = evaluate_plan(&request, &plan);
        let estimate = build_estimate(&request, &plan, &decision);
        assert_eq!(estimate.predicted_schema_tokens, COMPACT_SCHEMA_TOKENS);
        assert_eq!(estimate.predicted_invoke_tokens, COMPACT_INVOKE_TOKENS);
        assert_eq!(estimate.decision, AdmissionDecision::Serve);
        assert!(estimate.recommended);
    }

    #[test]
    fn fallback_confidence_still_serves_when_net_positive() {
        let plan = StelPlan {
            plan_id: "test".to_string(),
            intent: IntentBucket::Read,
            confidence: RouteConfidence::Fallback,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({}),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        assert_eq!(decision.decision, AdmissionDecision::Serve);
    }

    /// Build a single-step read plan grounded in a real target byte size.
    fn grounded_read_plan(raw_chars: u64, confidence: RouteConfidence) -> StelPlan {
        StelPlan {
            plan_id: format!("grounded-{raw_chars}"),
            intent: IntentBucket::Read,
            confidence,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "get_file_context".to_string(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
                // Plan-only floor that grounding must override when index_refs present.
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![IndexRef {
                    path: "src/lib.rs".to_string(),
                    raw_chars,
                }],
            }],
            suggested_followup: None,
        }
    }

    /// T035 (SC-005, US5 AC-1): the same operation over a small and a large real
    /// input yields DIFFERENT grounded predictions — never the pre-grounding
    /// fixed constant. The prediction scales with the actual target byte length.
    #[test]
    fn grounded_predictions_vary_with_real_input_size() {
        let small = estimate_economics(&grounded_read_plan(600, RouteConfidence::Inferred));
        let large = estimate_economics(&grounded_read_plan(40_000, RouteConfidence::Inferred));

        // Predictions are not the fixed 400/800-derived constant.
        assert_ne!(
            small.predicted_manual_tokens, large.predicted_manual_tokens,
            "manual baseline must vary with real size"
        );
        assert_ne!(
            small.predicted_net_vs_manual, large.predicted_net_vs_manual,
            "predicted net must vary with real size"
        );
        // Larger input ⇒ larger manual baseline ⇒ a more favorable (higher) net.
        assert!(
            large.predicted_manual_tokens > small.predicted_manual_tokens,
            "bigger file ⇒ bigger manual baseline (small={} large={})",
            small.predicted_manual_tokens,
            large.predicted_manual_tokens
        );
        assert!(
            large.predicted_net_vs_manual > small.predicted_net_vs_manual,
            "bigger file ⇒ higher predicted net (small={} large={})",
            small.predicted_net_vs_manual,
            large.predicted_net_vs_manual
        );

        // A step with NO index_refs keeps the deterministic plan-only floor
        // (preserves determinism for plan-only callers / preview / fixtures).
        let plan_only = StelPlan {
            steps: vec![StelPlanStep {
                index_refs: vec![],
                ..grounded_read_plan(600, RouteConfidence::Inferred).steps[0].clone()
            }],
            ..grounded_read_plan(600, RouteConfidence::Inferred)
        };
        let floor = estimate_economics(&plan_only);
        assert_eq!(floor.predicted_response_tokens, 400);
        assert_eq!(floor.predicted_manual_tokens, 800);
    }

    /// T036 (US5 AC-2, TR-04b): a sufficiently small grounded request makes a
    /// non-serve economics branch REACHABLE — a tiny file's competent-manual
    /// baseline falls below SymForge's fixed schema+invoke overhead, so
    /// `predicted_net <= 0` and the gate correctly BYPASSES (host-read is cheaper
    /// than serving). This branch was unreachable while every step carried the
    /// 400/800 constant (net was permanently ~275 ⇒ always serve).
    #[test]
    fn grounded_small_request_reaches_economics_bypass() {
        // 180-char file: below the 200-char small-file threshold ⇒ manual
        // baseline is the whole tiny file (~45 tokens), well under the 125-token
        // schema+invoke floor ⇒ predicted_net is non-positive.
        let tiny = grounded_read_plan(180, RouteConfidence::Inferred);
        let economics = estimate_economics(&tiny);
        assert!(
            economics.predicted_net_vs_manual <= 0,
            "tiny grounded request must predict non-positive net, got {}",
            economics.predicted_net_vs_manual
        );
        let decision = evaluate_plan(&StelRequest::default(), &tiny);
        assert_eq!(
            decision.decision,
            AdmissionDecision::Bypass,
            "tiny grounded request must reach economics bypass: {}",
            decision.decision_reason
        );
        assert!(
            decision.bypass.is_some(),
            "economics bypass must carry a host-read body"
        );
    }

    /// Build a single-step `replace_symbol_body` edit plan grounded in a real
    /// new-body byte length, mirroring what `build_edit_plan` stamps. `dry_run`
    /// selects preview (`true`) vs committed apply (`false`).
    fn grounded_edit_plan(body_chars: u64, dry_run: bool) -> StelPlan {
        StelPlan {
            plan_id: format!("edit-{body_chars}-{dry_run}"),
            intent: IntentBucket::Edit,
            confidence: RouteConfidence::Exact,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "replace_symbol_body".to_string(),
                args: serde_json::json!({
                    "path": "src/lib.rs",
                    "name": "foo",
                    "new_body": "x",
                    "dry_run": dry_run,
                }),
                // Plan-only floor that edit grounding must override.
                est_response_tokens: 520,
                est_manual_tokens: 900,
                index_refs: vec![IndexRef {
                    path: "src/lib.rs".to_string(),
                    raw_chars: body_chars,
                }],
            }],
            suggested_followup: None,
        }
    }

    /// Edit-economics grounding (this plan): a LARGE-body edit preview predicts
    /// proportionally MORE response tokens than a SMALL-body edit preview — the
    /// prediction is no longer the flat 520 constant. This is the regression the
    /// dogfood agents flagged (a 57%–507% predicted-vs-actual error from the flat
    /// floor); the response now scales with the echoed new body.
    #[test]
    fn grounded_edit_preview_response_scales_with_body_size() {
        let small = estimate_economics(&grounded_edit_plan(400, true));
        let large = estimate_economics(&grounded_edit_plan(40_000, true));

        // Not the flat plan-only floor of 520.
        assert_ne!(
            small.predicted_response_tokens, 520,
            "grounded edit must override the flat 520 response floor"
        );
        assert_ne!(
            small.predicted_response_tokens, large.predicted_response_tokens,
            "edit response prediction must vary with new-body size"
        );
        assert!(
            large.predicted_response_tokens > small.predicted_response_tokens,
            "bigger edit body ⇒ more echoed response (small={} large={})",
            small.predicted_response_tokens,
            large.predicted_response_tokens
        );
        // Manual baseline (read+rewrite the span) also scales with body size.
        assert!(
            large.predicted_manual_tokens > small.predicted_manual_tokens,
            "bigger edit body ⇒ bigger manual baseline (small={} large={})",
            small.predicted_manual_tokens,
            large.predicted_manual_tokens
        );
    }

    /// Edit-economics grounding (this plan): for the SAME new body, a PREVIEW
    /// predicts MORE response tokens than a committed APPLY — the preview echoes
    /// the full new body, the apply echoes only a confirmation/impact footer and
    /// does NOT re-emit the body. The flat 520 floor could never express this.
    #[test]
    fn grounded_edit_preview_predicts_more_response_than_apply() {
        let preview = estimate_economics(&grounded_edit_plan(40_000, true));
        let apply = estimate_economics(&grounded_edit_plan(40_000, false));

        assert!(
            preview.predicted_response_tokens > apply.predicted_response_tokens,
            "preview (echoes body) must predict more response than apply (footer only): \
             preview={} apply={}",
            preview.predicted_response_tokens,
            apply.predicted_response_tokens
        );
        // Apply response is just the fixed footer, independent of body size.
        let apply_small = estimate_economics(&grounded_edit_plan(400, false));
        assert_eq!(
            apply.predicted_response_tokens, apply_small.predicted_response_tokens,
            "committed apply does not re-emit the body ⇒ response is body-size invariant"
        );
        // The shared manual baseline still scales with body size for BOTH modes.
        assert_eq!(
            preview.predicted_manual_tokens, apply.predicted_manual_tokens,
            "manual baseline depends on the symbol span, not on preview-vs-apply"
        );
    }

    /// A structural edit always `serve`s even when its grounded net goes
    /// non-positive: a mutation has no host-read substitute, so the read ladder's
    /// `bypass`/`degrade` are incoherent here. (Pre-grounding this was moot — the
    /// flat 520/900 floor pinned net at ~255, so the dead branches never ran.)
    #[test]
    fn grounded_tiny_edit_still_serves_no_bypass() {
        // 8-byte body ⇒ manual ~2 tokens ⇒ net deeply negative, yet edits serve.
        let decision = evaluate_edit_plan(&grounded_edit_plan(8, true));
        assert_eq!(
            decision.decision,
            AdmissionDecision::Serve,
            "tiny edit must still serve (no host-read substitute): {}",
            decision.decision_reason
        );
        assert!(
            decision.bypass.is_none(),
            "edit decision must never carry a host-read bypass body"
        );
    }

    /// T036 / N-2: a low-confidence Fallback route with a grounded marginal net
    /// reaches the `mandatory_degrade` branch — the third dead branch the
    /// constant kept unreachable. Low-confidence routes now get an economic
    /// guardrail (degrade) instead of being served at full budget.
    #[test]
    fn grounded_fallback_marginal_reaches_mandatory_degrade() {
        // Choose a raw size whose grounded net lands in (0, SERVE_MARGIN): a
        // ~1600-char file ⇒ manual ~400 tokens, response ~240, net = 400 - (240+125)
        // = 35 ⇒ 0 < 35 < 50 = SERVE_MARGIN. On a Fallback route that triggers
        // mandatory_degrade (and economics_degrade), never serve, never bypass.
        let plan = grounded_read_plan(1600, RouteConfidence::Fallback);
        let economics = estimate_economics(&plan);
        assert!(
            economics.predicted_net_vs_manual > 0
                && economics.predicted_net_vs_manual < SERVE_MARGIN_TOKENS,
            "grounded net must be marginal (0 < net < {SERVE_MARGIN_TOKENS}), got {}",
            economics.predicted_net_vs_manual
        );
        let decision = evaluate_plan(&StelRequest::default(), &plan);
        assert_eq!(
            decision.decision,
            AdmissionDecision::Degrade,
            "marginal fallback must degrade: {}",
            decision.decision_reason
        );
        assert!(
            decision
                .degrade_flags
                .contains(&"fallback_mandatory".to_string()),
            "N-2: low-confidence fallback must carry the mandatory degrade guardrail: {:?}",
            decision.degrade_flags
        );
    }
}
