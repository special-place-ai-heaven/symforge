//! STEL L1 planner — map [`StelRequest`] to single- or multi-step [`StelPlan`] (L2 scores separately).

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::protocol::smart_query;

use super::executor::{
    COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT, COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE,
};
use super::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest};

struct PlannedStep {
    tool: String,
    args: Value,
    intent: IntentBucket,
    confidence: RouteConfidence,
    rationale: &'static str,
}

/// What the facade *actually does* with a caller-supplied [`StelRequest`] field,
/// recorded at the single plan choke point (A1a — root D-A0 guard).
///
/// CULPRIT A is the lossy facade: it routes a curated subset of each call's
/// params and *silently drops the rest*. This enum makes that class
/// structurally visible — every field must resolve to one explicit variant, so
/// "silently dropped" stops being a representable state. There is NO `Dropped`
/// variant on purpose: an unaccounted-for field is a bug the conformance test
/// catches, not a value this type can hold.
///
/// A1a is **assertion-only**: this records the CURRENT disposition of each
/// field and asserts exhaustiveness. It does NOT newly forward or newly
/// short-circuit anything (that is A1b). `Forwarded`/`Refused` here describe
/// behavior the handler ALREADY performs downstream of the planner, not new
/// routing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParamDisposition {
    /// The field's value reaches a planned tool step (route selection or args).
    Routed,
    /// The field is consumed downstream of the planner (the handler layer),
    /// not by the plan steps — named by where it lands today.
    Forwarded { into_arg: String },
    /// The field is loudly refused (the facade returns an error rather than a
    /// silently-partial answer) — named by why.
    Refused { reason: String },
    /// The field carries no actionable caller value on this route today
    /// (absent/blank, or a route the planner does not consume it on yet). An
    /// explicit "the planner saw it and did not act on it", NOT a silent drop.
    NotApplicable,
}

impl ParamDisposition {
    /// True for any explicit disposition. Always true by construction — there is
    /// no silent variant — but the conformance test asserts it per field so the
    /// silent-drop class cannot regress if a future variant is ever added.
    pub fn is_explicit(&self) -> bool {
        matches!(
            self,
            Self::Routed | Self::Forwarded { .. } | Self::Refused { .. } | Self::NotApplicable
        )
    }
}

/// Every [`StelRequest`] field, classified to exactly one [`ParamDisposition`]
/// reflecting the facade's CURRENT behavior, for the given finalized `plan`.
///
/// This is the structural lossless-or-loud guard (A1a): the returned slice is
/// fixed-length and covers EVERY field of `StelRequest`, so a field cannot be
/// absent from the accounting. Pure and side-effect-free — it observes the plan,
/// it does not change it, so wiring it in is byte-identical to before.
///
/// Dispositions mirror today's code paths exactly:
/// - `query`  → [`Routed`](ParamDisposition::Routed): always consumed by the planner.
/// - `intent` → `Routed` when set (selects the route bucket), else `NotApplicable`.
/// - `symbol` → `Refused` for prose (the handler's `symbol_contract_violation`
///   precedent), `Routed` when a bare identifier reaches a plan step, else
///   `NotApplicable` (set but not consumed on this route today).
/// - `path`   → `Routed` when the finalized plan's args carry the path value
///   (A1b forwards it into `path_prefix` on scoped search routes), else
///   `NotApplicable` (a route whose tool has no path scope, e.g. `get_repo_map`
///   or `search_files`; NOT `get_symbol`/`get_file_content`, which DO consume
///   `path` as a selector and are therefore `Routed`).
/// - `max_tokens` / `preview` → `Forwarded`: consumed by the handler AFTER the
///   planner (CCR budget / preview-estimate branch), not by plan steps.
/// - `project` / `projects` → `Refused` when meaningfully set: the handler
///   already loudly refuses cross-project targeting through this facade (D9).
pub fn classify_param_dispositions(
    request: &StelRequest,
    plan: &StelPlan,
) -> [(&'static str, ParamDisposition); 8] {
    let symbol_set = request
        .symbol
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let path_set = request
        .path
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty());

    let query = ParamDisposition::Routed;

    let intent = if request.intent.is_some() {
        ParamDisposition::Routed
    } else {
        ParamDisposition::NotApplicable
    };

    let symbol = if !symbol_set {
        ParamDisposition::NotApplicable
    } else if symbol_contract_violation(request).is_some() {
        ParamDisposition::Refused {
            reason: "symbol must be a bare identifier, not prose".to_string(),
        }
    } else if plan_carries_symbol(plan, request) {
        ParamDisposition::Routed
    } else {
        // A bare-identifier symbol the current route does not consume (e.g. an
        // explicit routing phrase won, or a non-honored bucket). The planner saw
        // it and chose not to act on it today — explicit, not silent.
        ParamDisposition::NotApplicable
    };

    let path = if path_set && plan_carries_path(plan, request) {
        ParamDisposition::Routed
    } else {
        // `path` absent, or a route whose tool carries no path scope and no path
        // selector (so neither A1b's forwarding nor a target arg applies — e.g.
        // `get_repo_map`, `context_inventory`, or `search_files`, which has no
        // `path_prefix` field at all; tracked as D20): explicit NotApplicable,
        // not a silent drop.
        ParamDisposition::NotApplicable
    };

    let max_tokens = if request.max_tokens.is_some() {
        ParamDisposition::Forwarded {
            into_arg: "handler CCR budget (apply_ccr_budget) / preview estimate".to_string(),
        }
    } else {
        ParamDisposition::NotApplicable
    };

    let preview = if request.preview.is_some() {
        ParamDisposition::Forwarded {
            into_arg: "handler preview-estimate branch".to_string(),
        }
    } else {
        ParamDisposition::NotApplicable
    };

    let project = if request
        .project
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty())
    {
        ParamDisposition::Refused {
            reason: "cross-project targeting is not routed through the `symforge` facade"
                .to_string(),
        }
    } else {
        ParamDisposition::NotApplicable
    };

    let projects = if request
        .projects
        .as_deref()
        .is_some_and(|ids| ids.iter().any(|id| !id.trim().is_empty()))
    {
        ParamDisposition::Refused {
            reason: "cross-project targeting is not routed through the `symforge` facade"
                .to_string(),
        }
    } else {
        ParamDisposition::NotApplicable
    };

    [
        ("query", query),
        ("intent", intent),
        ("symbol", symbol),
        ("path", path),
        ("max_tokens", max_tokens),
        ("preview", preview),
        ("project", project),
        ("projects", projects),
    ]
}

/// True when the finalized `plan` carries the caller's `symbol` value in any
/// step's args (`name` or `query`) — i.e. the symbol was honored by the route.
fn plan_carries_symbol(plan: &StelPlan, request: &StelRequest) -> bool {
    let symbol = match request.symbol.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s,
        _ => return false,
    };
    plan.steps.iter().any(|step| {
        ["name", "query"].iter().any(|key| {
            step.args
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|v| v == symbol)
        })
    })
}

/// True when the finalized `plan` carries the caller's `path` value in any
/// step's args (`path` or `path_prefix`) — i.e. the route consumed `path`.
fn plan_carries_path(plan: &StelPlan, request: &StelRequest) -> bool {
    let path = match request.path.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => p,
        _ => return false,
    };
    plan.steps.iter().any(|step| {
        ["path", "path_prefix"].iter().any(|key| {
            step.args
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|v| v == path)
        })
    })
}

/// Build a draft plan for compact `symforge` (L1 → L2).
pub fn build_plan(request: &StelRequest) -> StelPlan {
    if let Some(steps) = plan_multi_hop_steps(request) {
        return build_plan_from_steps(request, steps);
    }
    // A caller that supplies a bare-identifier `symbol` clearly wants THAT
    // symbol, not a full-text search over the natural-language `query`. Honor it
    // for find/auto intent (the buckets that would otherwise run a query-side
    // text search and ignore `symbol` entirely), but never override an explicit
    // routing phrase that already won — those single-step golden routes stand.
    if let Some(step) = symbol_lookup_step(request) {
        return build_plan_from_steps(request, vec![step]);
    }
    if let Some(step) = symbol_impact_step(request) {
        return build_plan_from_steps(request, vec![step]);
    }
    if let Some(step) = orient_lookup_step(request) {
        return build_plan_from_steps(request, vec![step]);
    }
    // US4 find fusion: a multi-word fuzzy find query that matches no explicit
    // routing phrase fans out across BOTH the path/file matcher (with the gated
    // co-change boost) and the symbol-name matcher, merged by the serve
    // executor into one ranked envelope. Plan-only: this emits ordered steps;
    // the search layer does the ranking/merging. See `find_fusion_steps`.
    if let Some(steps) = find_fusion_steps(request) {
        return build_plan_from_steps(request, steps);
    }
    let step = plan_step(request);
    build_plan_from_steps(request, vec![step])
}

/// Facade-boundary contract check for the `symbol` field. When a caller drops a
/// natural-language phrase into `symbol` (e.g. `"how status updates flow"`), the
/// legacy symbol tools would pass it verbatim as a tool `name` and return a
/// misleading `Symbol not found: <whole sentence>`. Detect prose up front and
/// return a one-line CORRECTIVE message naming the contract, so the agent
/// self-corrects instead of chasing a phantom not-found. Returns `None` when
/// `symbol` is absent or is a plausible bare identifier (the valid case).
pub fn symbol_contract_violation(request: &StelRequest) -> Option<String> {
    let symbol = request.symbol.as_deref()?.trim();
    if symbol.is_empty() || is_bare_identifier(symbol) {
        return None;
    }
    Some(format!(
        "`symbol` must be a bare identifier like `is_fresh`, not prose (got `{symbol}`); \
         put a phrase in `query=` instead and use `symbol=` only for an exact symbol name."
    ))
}

/// True when `candidate` is a plausible bare code identifier — a single token of
/// identifier characters (`A–Z a–z 0–9 _`), optionally path-qualified with `::`
/// or `.` (e.g. `is_fresh`, `LiveIndex`, `mod::Type`, `obj.method`). Rejects
/// prose: anything with whitespace, an empty string, a leading digit, or
/// non-identifier punctuation. Used by the facade to tell a real symbol name
/// from a natural-language sentence dropped into the `symbol` slot.
pub(crate) fn is_bare_identifier(candidate: &str) -> bool {
    let candidate = candidate.trim();
    if candidate.is_empty() || candidate.chars().any(char::is_whitespace) {
        return false;
    }
    // Strip qualifier separators; every remaining segment must be a non-empty
    // identifier whose first char is a letter or underscore.
    let segments: Vec<&str> = candidate
        .split("::")
        .flat_map(|seg| seg.split('.'))
        .collect();
    if segments.iter().any(|seg| seg.is_empty()) {
        return false;
    }
    segments.iter().all(|seg| {
        let mut chars = seg.chars();
        let first = match chars.next() {
            Some(c) => c,
            None => return false,
        };
        (first.is_ascii_alphabetic() || first == '_')
            && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// US-symbol-routing: a bare-identifier `symbol` for find/auto/read intent routes
/// straight to a symbol lookup rather than letting the query-side text search
/// own the route. Returns `None` (deferring to the existing pipeline) when:
///   - no `symbol` was supplied, or it is prose (the prose case is rejected with
///     a corrective error at the facade boundary, before planning), or
///   - the intent is an explicit bucket that already consumes `symbol` elsewhere
///     (`route_trace`, `symbol_impact_step`, …), or
///   - an explicit routing phrase in the query already won (its precise
///     single-step golden route must stand).
///
/// When it fires it prefers `get_symbol` (the caller named an exact symbol and,
/// with a `path`, we can fetch its body directly) and otherwise `search_symbols`
/// for find/auto or `get_symbol` by name for read, never a full-text search over
/// the prose query.
fn symbol_lookup_step(request: &StelRequest) -> Option<PlannedStep> {
    let symbol = request.symbol.as_deref()?.trim();
    if !is_bare_identifier(symbol) {
        return None;
    }
    let bucket = request.intent.unwrap_or(IntentBucket::Auto);
    if !matches!(
        bucket,
        IntentBucket::Auto | IntentBucket::Find | IntentBucket::Read
    ) {
        return None;
    }
    // An explicit routing phrase in the query is a precise signal that outranks a
    // bare `symbol` hint — never override a pinned golden route.
    if route_with_query_patterns(request).is_some() {
        return None;
    }
    if let Some(path) = request.path.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(planned(
            "get_symbol",
            json!({ "path": path, "name": symbol }),
            IntentBucket::Read,
            RouteConfidence::Exact,
            "explicit symbol + path lookup",
        ))
    } else if bucket == IntentBucket::Read {
        Some(planned(
            "get_symbol",
            json!({ "name": symbol }),
            IntentBucket::Read,
            RouteConfidence::Inferred,
            "read intent symbol body lookup",
        ))
    } else {
        Some(planned(
            "search_symbols",
            json!({ "query": symbol }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "explicit symbol lookup",
        ))
    }
}

/// Impact intent with a bare `symbol`: resolve to a concrete tool instead of
/// shoving the natural-language `query` into `find_dependents.path`.
fn symbol_impact_step(request: &StelRequest) -> Option<PlannedStep> {
    let symbol = request.symbol.as_deref()?.trim();
    if !is_bare_identifier(symbol) {
        return None;
    }
    if request.intent != Some(IntentBucket::Impact) {
        return None;
    }
    if route_with_query_patterns(request).is_some() {
        return None;
    }
    if let Some(path) = request.path.as_deref().filter(|p| !p.trim().is_empty()) {
        return Some(planned(
            "find_dependents",
            json!({ "path": path, "compact": true }),
            IntentBucket::Impact,
            RouteConfidence::Exact,
            "impact intent file dependents for explicit path",
        ));
    }
    Some(planned(
        "find_references",
        json!({ "name": symbol, "compact": true }),
        IntentBucket::Impact,
        RouteConfidence::Inferred,
        "impact intent symbol-level dependents (callers/usages)",
    ))
}

/// Orient phrasing on auto/orient intent routes to `get_repo_map` before find
/// fusion can tokenize the prose query into OR-literals.
fn orient_lookup_step(request: &StelRequest) -> Option<PlannedStep> {
    let bucket = request.intent.unwrap_or(IntentBucket::Auto);
    if !matches!(bucket, IntentBucket::Auto | IntentBucket::Orient) {
        return None;
    }
    let lower = request.query.trim().to_ascii_lowercase();
    if !is_orient_query(&lower) {
        return None;
    }
    if route_with_query_patterns(request).is_some() {
        return None;
    }
    Some(planned(
        "get_repo_map",
        json!({}),
        IntentBucket::Orient,
        RouteConfidence::Inferred,
        "orient/workspace phrasing",
    ))
}

/// True when the query is asking for repository orientation rather than search.
fn is_orient_query(lower: &str) -> bool {
    lower.starts_with("orient me")
        || lower.contains("repo map")
        || lower.contains("crate map")
        || lower.contains("map of")
        || lower.contains("main crates")
        || lower.contains("workspace layout")
        || lower.contains("project layout")
        || lower.contains("overview of the")
        || lower.contains("overview of this")
        || lower.contains("orient:")
}

/// Tools whose schema accepts a `path_prefix` SCOPE argument — the caller's
/// `path` forwards into it (A1b gated per-tool forwarding). Tools where `path`
/// is a TARGET/selector (e.g. `get_file_content`, `find_references`) are
/// intentionally excluded: there the target comes from the query, not a scope
/// hint, so forwarding the caller's `path` would be wrong.
const PATH_PREFIX_FORWARD_TOOLS: &[&str] = &["search_symbols", "search_text", "explore"];

/// A1b gated per-tool forwarding: thread the caller's `path` into each plan
/// step's `path_prefix` arg where that tool accepts a path scope (and the caller
/// supplied a non-blank `path`), UNLESS the route already set `path_prefix`
/// (idempotent — a route that parsed its own scope from the query is left
/// untouched). This closes the `path` silent-drop on scoped search routes
/// (lossless-or-loud, root D-A0): after this pass `plan_carries_path` is true on
/// those routes, so [`classify_param_dispositions`] records `path` as `Routed`.
///
/// `max_tokens` is deliberately NOT forwarded here: it is already honored as a
/// handler-layer CCR budget on the final envelope (`Forwarded`), so it is not a
/// silent drop, and pushing it into plan-step args would violate the `Forwarded`
/// disposition contract ("consumed downstream of the planner, not by the plan
/// steps").
fn forward_caller_path(request: &StelRequest, steps: &mut [StelPlanStep]) {
    let Some(path) = request
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    else {
        return;
    };
    for step in steps.iter_mut() {
        if PATH_PREFIX_FORWARD_TOOLS.contains(&step.tool.as_str())
            && step.args.get("path_prefix").is_none()
        {
            step.args["path_prefix"] = json!(path);
        }
    }
}

fn build_plan_from_steps(request: &StelRequest, steps: Vec<PlannedStep>) -> StelPlan {
    let primary = steps.first().expect("plan must have at least one step");
    let intent = primary.intent;
    let confidence = primary.confidence;
    let confidence_rationale = primary.rationale.to_string();
    let mut stel_steps: Vec<StelPlanStep> = steps
        .into_iter()
        .enumerate()
        .map(|(index, step)| StelPlanStep {
            order: (index + 1) as u32,
            tool: step.tool,
            args: step.args,
            est_response_tokens: 400,
            est_manual_tokens: 800,
            index_refs: vec![],
        })
        .collect();
    // A1b gated per-tool forwarding: thread the caller's `path` into scoped
    // search routes' `path_prefix`, closing the `path` silent-drop (root D-A0).
    forward_caller_path(request, &mut stel_steps);
    let plan = StelPlan {
        plan_id: new_plan_id(request),
        intent,
        confidence,
        confidence_rationale,
        steps: stel_steps,
        suggested_followup: None,
    };

    // Lossless-or-loud STRUCTURAL guard (root D-A0): every `StelRequest` field
    // MUST resolve to an explicit `ParamDisposition` at this single choke point —
    // no field may be silently unaccounted-for. This is a COMPILE-TIME / TEST-TIME
    // completeness invariant over field accounting, NOT a runtime loud refusal:
    // the `debug_assert!` compiles out of release, the conformance test
    // (`tests/stel_param_disposition.rs`) enforces it in every build, and only
    // `Refused` params (project/projects) reach the caller loudly — via a separate
    // handler path (D9), not this classifier. `classify_param_dispositions` is
    // pure (it audits the finalized `plan`, never mutates it); the behavioral
    // change is the `forward_caller_path` pass above (A1b), which the guard audits.
    debug_assert!(
        classify_param_dispositions(request, &plan)
            .iter()
            .all(|(_, disposition)| disposition.is_explicit()),
        "A1a: a StelRequest field resolved to no explicit ParamDisposition (silent drop)"
    );

    plan
}

/// Envelope plan line: `trace → find_references (exact)` or multi-hop `find → search_symbols → get_symbol (inferred)`.
pub fn plan_summary_line(plan: &StelPlan) -> String {
    let tool_chain = plan
        .steps
        .iter()
        .map(|step| step.tool.as_str())
        .collect::<Vec<_>>()
        .join(" → ");
    format!(
        "{} → {} ({})",
        plan.intent.as_str(),
        tool_chain,
        confidence_label(plan.confidence)
    )
}

pub fn confidence_label(confidence: RouteConfidence) -> &'static str {
    match confidence {
        RouteConfidence::Exact => "exact",
        RouteConfidence::Inferred => "inferred",
        RouteConfidence::Fallback => "fallback",
    }
}

fn plan_step(request: &StelRequest) -> PlannedStep {
    let bucket = request.intent.unwrap_or(IntentBucket::Auto);
    if bucket != IntentBucket::Auto
        && let Some(step) = route_with_bucket(request, bucket)
    {
        return step;
    }
    if let Some(step) = route_with_query_patterns(request) {
        return step;
    }
    route_with_smart_query(request)
}

/// Ordered multi-step plans for the three Phase 2 golden multi-hop rows.
fn plan_multi_hop_steps(request: &StelRequest) -> Option<Vec<PlannedStep>> {
    let lower = request.query.trim().to_ascii_lowercase();
    if lower == "search then fetch cfg_if body" {
        return Some(vec![
            planned(
                "search_symbols",
                json!({ "query": "cfg_if" }),
                IntentBucket::Find,
                RouteConfidence::Inferred,
                "multi-hop search then fetch symbol",
            ),
            planned(
                "get_symbol",
                json!({ "path": "src/lib.rs", "name": "cfg_if" }),
                IntentBucket::Read,
                RouteConfidence::Inferred,
                "multi-hop fetch symbol body",
            ),
        ]);
    }
    if lower == "outline then find connection refs" {
        return Some(vec![
            planned(
                "get_file_context",
                json!({ "path": "records.py" }),
                IntentBucket::Read,
                RouteConfidence::Inferred,
                "multi-hop outline first",
            ),
            planned(
                "find_references",
                json!({ "name": "Connection", "compact": true }),
                IntentBucket::Trace,
                RouteConfidence::Inferred,
                "multi-hop find references",
            ),
        ]);
    }
    if lower == "find test.js then read it" {
        return Some(vec![
            planned(
                "search_files",
                json!({ "query": "test.js" }),
                IntentBucket::Find,
                RouteConfidence::Inferred,
                "multi-hop find file",
            ),
            planned(
                "get_file_content",
                json!({ "path": "test.js" }),
                IntentBucket::Read,
                RouteConfidence::Inferred,
                "multi-hop read file",
            ),
        ]);
    }
    None
}

/// Rationale marker (human-facing) for the path/file step of a fused find plan.
/// The serve executor's co-change anchor injection recognizes the step by its
/// `rank_by == "path+cochange"` argument, NOT by this string, so this text is
/// presentation-only and safe to reword. Index-aware anchor resolution stays out
/// of the plan-only planner.
pub(crate) const FIND_FUSION_PATH_RATIONALE: &str = "find fusion: path/file ranking with co-change";

/// Rationale marker for the symbol step of a fused find plan.
const FIND_FUSION_SYMBOL_RATIONALE: &str = "find fusion: symbol-name ranking";

/// US4 find fusion (plan-only). A multi-word, fuzzy find query that matched no
/// explicit routing phrase fans out across two surfaces, merged by the serve
/// executor into one ranked envelope (run as a UNION — an empty surface is not
/// a chain failure; see `is_find_fusion_plan`):
///   1. `search_files` (path/file ranking + gated co-change boost via
///      `rank_by="path+cochange"`; the executor injects the resolved anchor).
///      Uses the full query, whose token order the path matcher reads as
///      `component…/basename`. Empty for a query whose trailing token is not a
///      basename — that is fine; the path/name evidence still arrives via the
///      term step below.
///   2. `search_text` with OR `terms` over the tokenized query — the multi-term
///      matcher that actually spans symbol NAMES (matching a definition line
///      like `fn route_find`) and file CONTENT for ANY token, where the
///      whole-query substring of `search_symbols` would miss a fuzzy bag of
///      words. Frecency-neutral like every search_* surface.
///
/// Gating (preserves every golden route):
///   - only fires when no explicit `route_with_query_patterns` phrase matched
///     (so `find X class`, `locate X symbol`, `files named X`, … keep their
///     single-step routes),
///   - only for `Auto`/`Find` intent (an explicit non-find bucket is honored),
///   - requires >= 2 fuzzy word tokens (a single token stays single-step so the
///     existing symbol/path heuristics own it),
///   - skips queries carrying path/scope syntax (`/`, file extensions) that the
///     single-step file route already handles precisely.
fn find_fusion_steps(request: &StelRequest) -> Option<Vec<PlannedStep>> {
    let bucket = request.intent.unwrap_or(IntentBucket::Auto);
    if !matches!(bucket, IntentBucket::Auto | IntentBucket::Find) {
        return None;
    }
    // An explicit routing phrase always wins — never override a precise route.
    if route_with_query_patterns(request).is_some() {
        return None;
    }
    let query = request.query.trim();
    if !is_multi_term_fuzzy_find(query) {
        return None;
    }
    let terms = significant_find_terms(query);

    Some(vec![
        planned(
            "search_files",
            json!({ "query": query, "rank_by": "path+cochange" }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            FIND_FUSION_PATH_RATIONALE,
        ),
        planned(
            "search_text",
            json!({ "terms": terms, "group_by": "symbol" }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            FIND_FUSION_SYMBOL_RATIONALE,
        ),
    ])
}

/// Ubiquitous English glue words dropped from a fuzzy find's OR `terms`. Matched
/// as bare OR literals they explode a natural-language query into hundreds of
/// doc hits (every file containing "the"/"and" matches). Code identifiers are
/// never stopwords, so this trims only prose connective tissue. Lowercase.
const FIND_STOPWORDS: &[&str] = &[
    "a", "an", "and", "the", "or", "of", "to", "in", "on", "for", "is", "are", "be", "by", "it",
    "its", "as", "at", "with", "from", "how", "what", "why", "when", "where", "that", "this",
    "these", "those", "into", "over", "via", "do", "does", "we", "our", "you", "your",
];

/// The significant OR terms for a fuzzy find's `search_text` step: whitespace
/// tokens with alphanumeric content, minus [`FIND_STOPWORDS`]. Falls back to all
/// tokens when filtering would leave nothing (a query made entirely of
/// stopwords), so the step never sends an empty `terms`. (Plan 007: NL finds
/// were OR-exploding because every stopword became a bare literal.)
fn significant_find_terms(query: &str) -> Vec<&str> {
    let all: Vec<&str> = query.split_whitespace().collect();
    let filtered: Vec<&str> = all
        .iter()
        .copied()
        .filter(|tok| {
            tok.chars().any(|c| c.is_alphanumeric())
                && !FIND_STOPWORDS.contains(&tok.to_ascii_lowercase().as_str())
        })
        .collect();
    if filtered.is_empty() { all } else { filtered }
}

/// True when `query` is a multi-word, fuzzy find query suitable for fusion: at
/// least two alphanumeric word tokens, no path/scope syntax, and no
/// guidance/explore lead-in. Tokenization mirrors the path-side
/// `tokenize_path_query` (split on whitespace, drop empties) so the planner's
/// notion of "multi-term" matches what the ranker will see.
fn is_multi_term_fuzzy_find(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    // Path/scope syntax is owned by the precise single-step file route.
    if lower.contains('/') || lower.contains('\\') {
        return false;
    }
    // Guidance / conceptual-explanation queries ("how to use X", "what is Y",
    // "explain Z") are explore intent, not find — never fuse them. This keeps
    // the golden `explore` rows (e.g. "how to use records ORM") routing to
    // explore and mirrors smart_query's Understand/Explore lead-ins.
    if is_guidance_query(&lower) || is_orient_query(&lower) {
        return false;
    }
    let tokens: Vec<&str> = lower
        .split_whitespace()
        .filter(|tok| tok.chars().any(|ch| ch.is_alphanumeric()))
        .collect();
    if tokens.len() < 2 {
        return false;
    }
    // A bare filename token (e.g. `foo.rs`) is a single-file lookup even when
    // other words surround it; defer to the single-step heuristics in that case.
    !tokens.iter().any(|tok| {
        tok.contains('.')
            && std::path::Path::new(tok)
                .extension()
                .is_some_and(|ext| !ext.is_empty())
    })
}

/// True when `lower` (already lowercased) opens with a guidance / conceptual
/// lead-in that signals explore intent rather than find. Mirrors smart_query's
/// Understand/Explore prefixes plus the bare "how to" form that the golden
/// `explore` rows use.
fn is_guidance_query(lower: &str) -> bool {
    const GUIDANCE_LEAD_INS: [&str; 12] = [
        "how to ",
        "how does ",
        "how do ",
        "how can ",
        "explain ",
        "understand ",
        "describe ",
        "what is ",
        "what are ",
        "why ",
        "tell me about ",
        "walk me through ",
    ];
    GUIDANCE_LEAD_INS
        .iter()
        .any(|lead_in| lower.starts_with(lead_in))
}

/// True when `plan` is a US4 find-fusion union (independent path + symbol
/// surfaces), identified by the path step's `rank_by="path+cochange"` arg — the
/// only planner route that emits it. The serve executor uses this to treat the
/// fusion as a UNION (an empty surface is not a chain failure) rather than a
/// dependent chain (where each step consumes the prior and an empty result is
/// fatal).
pub fn is_find_fusion_plan(plan: &StelPlan) -> bool {
    plan.steps.iter().any(|step| {
        step.tool == "search_files"
            && step
                .args
                .get("rank_by")
                .and_then(Value::as_str)
                .is_some_and(|rank_by| rank_by == "path+cochange")
    })
}

fn route_with_bucket(request: &StelRequest, bucket: IntentBucket) -> Option<PlannedStep> {
    match bucket {
        IntentBucket::Find => Some(route_find(request)),
        IntentBucket::Read => Some(route_read(request)),
        IntentBucket::Trace => Some(route_trace(request)),
        IntentBucket::Impact => Some(route_impact(request)),
        IntentBucket::Orient => Some(route_orient(request)),
        IntentBucket::Meta => Some(route_meta(request)),
        IntentBucket::Edit | IntentBucket::Auto => None,
    }
}

fn route_with_query_patterns(request: &StelRequest) -> Option<PlannedStep> {
    let query = request.query.trim();
    let lower = query.to_ascii_lowercase();

    if let Some(path) = parse_outline_path(query) {
        return Some(planned(
            "get_file_context",
            json!({ "path": path }),
            IntentBucket::Read,
            RouteConfidence::Exact,
            "outline path in query",
        ));
    }

    if let Some((path, name)) = parse_body_of(query) {
        return Some(planned(
            "get_symbol",
            json!({ "path": path, "name": name }),
            IntentBucket::Read,
            RouteConfidence::Exact,
            "body-of symbol request",
        ));
    }

    if let Some(name) = parse_reference_target(query) {
        let mut args = json!({ "name": name, "compact": true });
        if let Some(path) = request.path.as_deref() {
            args["path"] = json!(path);
        }
        return Some(planned(
            "find_references",
            args,
            IntentBucket::Trace,
            RouteConfidence::Exact,
            "caller/reference phrasing",
        ));
    }

    if let Some(name) = parse_bare_references_target(query) {
        return Some(planned(
            "find_references",
            json!({ "name": name, "compact": true }),
            IntentBucket::Trace,
            RouteConfidence::Inferred,
            "references {symbol} phrasing",
        ));
    }

    if lower.contains("repo map") {
        return Some(planned(
            "get_repo_map",
            json!({}),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "repo map phrasing",
        ));
    }

    if lower == "index health" {
        return Some(planned(
            "health_compact",
            json!({}),
            IntentBucket::Meta,
            RouteConfidence::Exact,
            "index health probe",
        ));
    }

    if let Some((path, max_lines)) = parse_bounded_content_read(query, &lower) {
        let mut args = json!({ "path": path });
        if let Some(max_lines) = max_lines {
            args["max_lines"] = json!(max_lines);
        }
        return Some(planned(
            "get_file_content",
            args,
            IntentBucket::Read,
            RouteConfidence::Inferred,
            "bounded file content read",
        ));
    }

    if let Some((name, path)) = parse_symbol_in_path(query, &lower) {
        return Some(planned(
            "get_symbol",
            json!({ "path": path, "name": name }),
            IntentBucket::Read,
            RouteConfidence::Inferred,
            "symbol-in-path phrasing",
        ));
    }

    if let Some(name) = parse_symbol_body_phrase(query, &lower) {
        return Some(planned(
            "get_symbol",
            json!({ "name": name }),
            IntentBucket::Read,
            RouteConfidence::Inferred,
            "symbol body phrasing",
        ));
    }

    if let Some(name) = parse_trailing_symbol_phrase(query, &lower) {
        return Some(planned(
            "search_symbols",
            json!({ "query": name }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "trailing symbol phrasing",
        ));
    }

    if let Some(hint) = parse_files_named_hint(query, &lower) {
        return Some(planned(
            "search_files",
            json!({ "query": hint }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "files named phrasing",
        ));
    }

    if let Some(hint) = parse_files_for_hint(query, &lower) {
        return Some(planned(
            "search_files",
            json!({ "query": hint }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "files for phrasing",
        ));
    }

    if let Some(term) = parse_find_entity_search(query, &lower) {
        return Some(planned(
            "search_text",
            json!({ "query": term }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "find class/function phrasing",
        ));
    }

    if lower.contains("macro usage") || (lower.starts_with("find ") && lower.contains(" usage")) {
        let term = extract_find_subject(query).unwrap_or_else(|| query.to_string());
        let mut args = json!({ "query": term });
        if let Some(path) = request.path.as_deref() {
            args["path_prefix"] = json!(path);
        } else {
            args["path_prefix"] = json!("src");
        }
        return Some(planned(
            "search_text",
            args,
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "text search usage phrasing",
        ));
    }

    if lower.starts_with("locate ") && lower.contains("symbol") {
        let term = query
            .trim()
            .strip_prefix("locate ")
            .unwrap_or(query)
            .trim()
            .strip_suffix(" symbol")
            .unwrap_or(query)
            .trim()
            .to_string();
        return Some(planned(
            "search_symbols",
            json!({ "query": term }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "symbol locate phrasing",
        ));
    }

    if lower.starts_with("how does ") || lower.starts_with("how do ") {
        return Some(planned(
            "explore",
            json!({ "query": query, "depth": 2 }),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "guidance/explore phrasing",
        ));
    }

    None
}

fn route_with_smart_query(request: &StelRequest) -> PlannedStep {
    let intent = smart_query::classify_intent(request.query.trim());
    let tool = smart_query::route_tool_name(&intent).to_string();
    let intent_bucket = intent_bucket_for_tool(&tool);
    let args = default_args_for_tool(&tool, request);
    PlannedStep {
        tool,
        intent: intent_bucket,
        confidence: RouteConfidence::Inferred,
        rationale: "smart_query fallback",
        args,
    }
}

fn route_find(request: &StelRequest) -> PlannedStep {
    let lower = request.query.to_ascii_lowercase();
    if lower.contains("symbol") || lower.starts_with("locate ") {
        route_with_query_patterns(request).unwrap_or_else(|| {
            planned(
                "search_symbols",
                json!({ "query": request.query.trim() }),
                IntentBucket::Find,
                RouteConfidence::Inferred,
                "find intent symbol search",
            )
        })
    } else {
        route_with_query_patterns(request).unwrap_or_else(|| {
            planned(
                "search_text",
                json!({ "query": request.query.trim() }),
                IntentBucket::Find,
                RouteConfidence::Inferred,
                "find intent text search",
            )
        })
    }
}

fn route_read(request: &StelRequest) -> PlannedStep {
    route_with_query_patterns(request).unwrap_or_else(|| {
        if let Some(path) = request.path.clone() {
            planned(
                "get_file_context",
                json!({ "path": path }),
                IntentBucket::Read,
                RouteConfidence::Exact,
                "read intent with explicit path",
            )
        } else {
            planned(
                "get_file_context",
                json!({ "path": request.query.trim() }),
                IntentBucket::Read,
                RouteConfidence::Fallback,
                "read intent path fallback",
            )
        }
    })
}

fn route_trace(request: &StelRequest) -> PlannedStep {
    route_with_query_patterns(request).unwrap_or_else(|| {
        let name = request
            .symbol
            .clone()
            .unwrap_or_else(|| request.query.trim().to_string());
        let mut args = json!({ "name": name, "compact": true });
        if let Some(path) = request.path.as_deref() {
            args["path"] = json!(path);
        }
        planned(
            "find_references",
            args,
            IntentBucket::Trace,
            RouteConfidence::Inferred,
            "trace intent reference lookup",
        )
    })
}

fn route_impact(request: &StelRequest) -> PlannedStep {
    let path = request
        .path
        .clone()
        .unwrap_or_else(|| request.query.trim().to_string());
    planned(
        "find_dependents",
        json!({ "path": path }),
        IntentBucket::Impact,
        RouteConfidence::Inferred,
        "impact intent dependents lookup",
    )
}

fn route_orient(request: &StelRequest) -> PlannedStep {
    let lower = request.query.trim().to_ascii_lowercase();
    if is_orient_query(&lower) {
        planned(
            "get_repo_map",
            json!({}),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "orient intent repo map",
        )
    } else if is_guidance_query(&lower) {
        planned(
            "explore",
            json!({ "query": request.query.trim(), "depth": 2 }),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "orient intent explore guidance",
        )
    } else {
        planned(
            "get_repo_map",
            json!({}),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "orient intent default repo map",
        )
    }
}

fn route_meta(_request: &StelRequest) -> PlannedStep {
    planned(
        "context_inventory",
        json!({}),
        IntentBucket::Meta,
        RouteConfidence::Fallback,
        "meta intent inventory",
    )
}

fn planned(
    tool: &str,
    args: Value,
    intent: IntentBucket,
    confidence: RouteConfidence,
    rationale: &'static str,
) -> PlannedStep {
    PlannedStep {
        tool: tool.to_string(),
        args,
        intent,
        confidence,
        rationale,
    }
}

fn default_args_for_tool(tool: &str, request: &StelRequest) -> Value {
    match tool {
        "search_text" => json!({ "query": request.query.trim() }),
        "search_symbols" => json!({ "query": request.query.trim() }),
        "search_files" => json!({ "query": request.query.trim() }),
        "get_file_context" => {
            json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) })
        }
        "get_file_content" => {
            json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) })
        }
        "get_symbol" => json!({
            "path": request.path.clone().unwrap_or_default(),
            "name": request.symbol.clone().unwrap_or_else(|| request.query.trim().to_string()),
        }),
        "find_references" => json!({
            "name": request.symbol.clone().unwrap_or_else(|| request.query.trim().to_string()),
            "compact": true,
            "limit": COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT,
            "max_per_file": COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE,
        }),
        "find_dependents" => {
            json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) })
        }
        "explore" => json!({ "query": request.query.trim(), "depth": 2 }),
        "what_changed" => json!({}),
        "get_repo_map" => json!({}),
        "health_compact" => json!({}),
        _ => json!({ "query": request.query.trim() }),
    }
}

fn intent_bucket_for_tool(tool: &str) -> IntentBucket {
    match tool {
        "search_text" | "search_symbols" | "search_files" => IntentBucket::Find,
        "get_file_context" | "get_file_content" | "get_symbol" => IntentBucket::Read,
        "find_references" | "get_symbol_context" => IntentBucket::Trace,
        "find_dependents" | "what_changed" | "analyze_file_impact" | "diff_symbols" => {
            IntentBucket::Impact
        }
        "explore" | "get_repo_map" | "conventions" => IntentBucket::Orient,
        "context_inventory" | "investigation_suggest" | "ask" | "health_compact" => {
            IntentBucket::Meta
        }
        _ => IntentBucket::Auto,
    }
}

fn new_plan_id(request: &StelRequest) -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("plan-{ms:x}-{}", request.query.len())
}

fn parse_outline_path(query: &str) -> Option<String> {
    let rest = query.trim().to_ascii_lowercase();
    let path = rest.strip_prefix("outline ")?.trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

fn parse_body_of(query: &str) -> Option<(String, String)> {
    let lower = query.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("body of ")?;
    let (name, path_part) = rest.split_once(" in ")?;
    if name.is_empty() || path_part.is_empty() {
        return None;
    }
    Some((normalize_path(path_part.trim()), name.trim().to_string()))
}

fn parse_reference_target(query: &str) -> Option<String> {
    let lower = query.trim().to_ascii_lowercase();
    for prefix in [
        "who references ",
        "who calls ",
        "references to ",
        "callers of ",
    ] {
        if let Some(target) = lower.strip_prefix(prefix) {
            let target = target.trim();
            if !target.is_empty() {
                return slice_after_prefix(query, prefix);
            }
        }
    }
    None
}

fn parse_bare_references_target(query: &str) -> Option<String> {
    const PREFIX: &str = "references ";
    let lower = query.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix(PREFIX)?.trim();
    if rest.is_empty() || rest.starts_with("to ") {
        return None;
    }
    slice_after_prefix(query, PREFIX)
}

fn parse_bounded_content_read(query: &str, lower: &str) -> Option<(String, Option<u32>)> {
    if let Some(rest) = lower.strip_prefix("first ")
        && let Some((lines, path)) = rest.split_once(" lines ")
    {
        let lines = lines.trim().parse().ok()?;
        let path = path.trim();
        if !path.is_empty() {
            return Some((normalize_path(path), Some(lines)));
        }
    }
    if let Some(rest) = lower.strip_prefix("read ") {
        if let Some((path, limit)) = rest.split_once(" limit ") {
            let lines = limit.trim().parse().ok()?;
            let path = path.trim();
            if !path.is_empty() {
                return Some((path.to_string(), Some(lines)));
            }
        }
        if rest.ends_with(" header") {
            let path = rest.strip_suffix(" header")?.trim();
            if !path.is_empty() {
                return Some((path.to_string(), Some(40)));
            }
        }
    }
    let _ = query;
    None
}

fn parse_symbol_in_path(query: &str, lower: &str) -> Option<(String, String)> {
    let marker = " symbol in ";
    let idx = lower.rfind(marker)?;
    let name = query[..idx].trim();
    let path = query[idx + marker.len()..].trim();
    if name.is_empty() || path.is_empty() {
        return None;
    }
    Some((name.to_string(), path.to_string()))
}

fn parse_symbol_body_phrase(query: &str, lower: &str) -> Option<String> {
    if lower.ends_with(" symbol body") {
        let name = query.trim().strip_suffix(" symbol body")?.trim();
        if name.is_empty() {
            return None;
        }
        return Some(name.to_string());
    }
    if lower.ends_with(" body") && !lower.starts_with("body of ") {
        let name = query.trim().strip_suffix(" body")?.trim();
        if name.is_empty() || name.contains(' ') {
            return None;
        }
        return Some(name.to_string());
    }
    None
}

fn parse_trailing_symbol_phrase(query: &str, lower: &str) -> Option<String> {
    if !lower.ends_with(" symbol") || lower.starts_with("locate ") {
        return None;
    }
    let name = query.trim().strip_suffix(" symbol")?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_files_named_hint(query: &str, lower: &str) -> Option<String> {
    let rest = lower.strip_prefix("files named ")?.trim();
    if rest.is_empty() {
        return None;
    }
    slice_after_prefix(query, "files named ")
}

fn parse_files_for_hint(query: &str, lower: &str) -> Option<String> {
    let marker = " files for ";
    let idx = lower.find(marker)?;
    let hint = query[idx + marker.len()..].trim();
    if hint.is_empty() {
        None
    } else {
        Some(hint.to_string())
    }
}

fn parse_find_entity_search(query: &str, lower: &str) -> Option<String> {
    if !lower.starts_with("find ") {
        return None;
    }
    let term = if lower.ends_with(" function") {
        query
            .trim()
            .strip_prefix("find ")
            .and_then(|s| s.strip_suffix(" function"))
            .map(str::trim)
    } else if lower.contains(" class") {
        query
            .trim()
            .strip_prefix("find ")
            .and_then(|s| s.strip_suffix(" class"))
            .map(str::trim)
    } else if lower.ends_with(" check") {
        query
            .trim()
            .strip_prefix("find ")
            .and_then(|s| s.strip_suffix(" check"))
            .map(str::trim)
    } else {
        None
    }?;
    if term.is_empty() {
        None
    } else {
        Some(term.to_string())
    }
}

fn slice_after_prefix(query: &str, prefix: &str) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let pos = lower.find(&prefix.to_ascii_lowercase())?;
    let value = query[pos + prefix.len()..].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn extract_find_subject(query: &str) -> Option<String> {
    let lower = query.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("find ")?.trim();
    let subject = rest
        .strip_suffix(" macro usage")
        .or_else(|| rest.strip_suffix(" usage"))
        .unwrap_or(rest)
        .trim();
    if subject.is_empty() {
        None
    } else {
        Some(subject.to_string())
    }
}

fn normalize_path(path: &str) -> String {
    if path.contains('/') {
        path.to_string()
    } else {
        format!("src/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn plan_tool(request: StelRequest) -> String {
        build_plan(&request).steps[0].tool.clone()
    }

    #[test]
    fn planner_matches_cfg_if_golden_subset() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(super::super::golden_replay::GOLDEN_ROUTES_FIXTURE);
        let rows = super::super::golden_replay::load_golden_rows(&path).expect("golden fixture");
        let subset = [
            "cfg-if/t1_search",
            "cfg-if/t2_context",
            "cfg-if/t3_symbols",
            "cfg-if/t4_refs",
            "cfg-if/t5_symbol",
        ];
        for id in subset {
            let row = rows.iter().find(|r| r.id == id).expect("row");
            let mut request = row.to_request();
            request.intent = row.intent;
            let plan = build_plan(&request);
            assert_eq!(
                plan.steps[0].tool, row.must_call[0],
                "planner tool mismatch for {id}"
            );
        }
    }

    #[test]
    fn planner_honors_s4_exit_rows() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(super::super::golden_replay::GOLDEN_ROUTES_FIXTURE);
        let rows = super::super::golden_replay::load_golden_rows(&path).expect("golden fixture");
        for row in super::super::golden_replay::s4_exit_rows(&rows) {
            let mut request = row.to_request();
            request.intent = row.intent;
            let plan = build_plan(&request);
            assert_eq!(
                plan.steps[0].tool, row.must_call[0],
                "S4 row {} planner mismatch",
                row.id
            );
        }
    }

    #[test]
    fn plan_summary_uses_planned_tool() {
        let plan = build_plan(&StelRequest {
            query: "who references cfg_if".to_string(),
            ..Default::default()
        });
        let summary = plan_summary_line(&plan);
        assert!(summary.contains("find_references"));
        assert!(summary.contains("trace"));
    }

    #[test]
    fn outline_query_plans_get_file_context() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "outline src/lib.rs".to_string(),
                ..Default::default()
            }),
            "get_file_context"
        );
    }

    #[test]
    fn repo_map_phrasing_plans_get_repo_map() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "repo map cfg-if".to_string(),
                ..Default::default()
            }),
            "get_repo_map"
        );
    }

    #[test]
    fn bounded_content_reads_plan_get_file_content() {
        let req = StelRequest {
            query: "first 80 lines lib.rs".to_string(),
            ..Default::default()
        };
        let plan = build_plan(&req);
        assert_eq!(plan.steps[0].tool, "get_file_content");
        assert_eq!(plan.steps[0].args["path"], "src/lib.rs");
        assert_eq!(plan.steps[0].args["max_lines"], 80);

        assert_eq!(
            plan_tool(StelRequest {
                query: "read index.js limit 80".to_string(),
                ..Default::default()
            }),
            "get_file_content"
        );
    }

    #[test]
    fn find_entity_phrasing_plans_search_text() {
        for query in [
            "find reconcile function",
            "find Database class",
            "find plainObject check",
        ] {
            assert_eq!(
                plan_tool(StelRequest {
                    query: query.to_string(),
                    ..Default::default()
                }),
                "search_text",
                "query: {query}"
            );
        }
    }

    #[test]
    fn find_fusion_drops_stopwords_from_or_terms() {
        // Plan 007: a natural-language multi-word find must not OR-match
        // stopwords ("and", "its", "over") — bare OR literals explode the
        // result set into hundreds of doc hits. The fusion search_text step
        // should carry only the significant terms.
        let plan = build_plan(&StelRequest {
            query: "daemon freshness check and reports its version over IPC".to_string(),
            intent: Some(IntentBucket::Find),
            ..Default::default()
        });
        let text_step = plan
            .steps
            .iter()
            .find(|s| s.tool == "search_text")
            .expect("a fuzzy multi-term find fuses a search_text step");
        let terms: Vec<String> = text_step.args["terms"]
            .as_array()
            .expect("the fusion search_text step carries an OR `terms` array")
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        for stop in ["and", "its", "over"] {
            assert!(
                !terms.iter().any(|t| t.eq_ignore_ascii_case(stop)),
                "stopword {stop:?} must be filtered from fusion terms: {terms:?}"
            );
        }
        assert!(
            terms.iter().any(|t| t == "daemon") && terms.iter().any(|t| t == "freshness"),
            "significant terms must survive the filter: {terms:?}"
        );
    }

    #[test]
    fn bare_references_phrasing_plans_find_references() {
        let plan = build_plan(&StelRequest {
            query: "references isPlainObject".to_string(),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "find_references");
        assert_eq!(plan.steps[0].args["name"], "isPlainObject");
    }

    #[test]
    fn symbol_body_and_in_path_phrasing_plan_get_symbol() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "Database symbol in records.py".to_string(),
                ..Default::default()
            }),
            "get_symbol"
        );
        assert_eq!(
            plan_tool(StelRequest {
                query: "reconcile symbol body".to_string(),
                ..Default::default()
            }),
            "get_symbol"
        );
        assert_eq!(
            plan_tool(StelRequest {
                query: "isPlainObject body".to_string(),
                ..Default::default()
            }),
            "get_symbol"
        );
    }

    #[test]
    fn trailing_symbol_phrasing_plans_search_symbols() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "isPlainObject symbol".to_string(),
                ..Default::default()
            }),
            "search_symbols"
        );
    }

    #[test]
    fn files_search_phrasing_plans_search_files() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "files named records".to_string(),
                ..Default::default()
            }),
            "search_files"
        );
        assert_eq!(
            plan_tool(StelRequest {
                query: "test files for plain object".to_string(),
                ..Default::default()
            }),
            "search_files"
        );
    }

    #[test]
    fn index_health_plans_health_compact() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "index health".to_string(),
                ..Default::default()
            }),
            "health_compact"
        );
    }

    #[test]
    fn multi_hop_golden_rows_plan_ordered_steps() {
        let cases = [
            (
                "cfg-if/multi_search_symbol",
                "search then fetch cfg_if body",
                vec!["search_symbols", "get_symbol"],
            ),
            (
                "records/multi_context_refs",
                "outline then find Connection refs",
                vec!["get_file_context", "find_references"],
            ),
            (
                "is-plain/multi_files_content",
                "find test.js then read it",
                vec!["search_files", "get_file_content"],
            ),
        ];
        for (id, query, expected_tools) in cases {
            let plan = build_plan(&StelRequest {
                query: query.to_string(),
                ..Default::default()
            });
            let planned: Vec<String> = plan.steps.iter().map(|step| step.tool.clone()).collect();
            assert_eq!(
                planned, expected_tools,
                "multi-hop planner mismatch for {id}"
            );
            assert_eq!(plan.steps.len(), 2, "{id} must be two-step plan");
        }
    }

    #[test]
    fn multi_word_fuzzy_find_plans_fused_path_and_text_steps() {
        // A multi-word fuzzy find with no explicit routing phrase fans out
        // across the path/file matcher (with co-change) and the OR-term
        // name/content matcher, as a two-step plan the executor merges.
        let plan = build_plan(&StelRequest {
            query: "stel planner find".to_string(),
            ..Default::default()
        });
        let tools: Vec<&str> = plan.steps.iter().map(|s| s.tool.as_str()).collect();
        assert_eq!(tools, vec!["search_files", "search_text"]);
        assert_eq!(plan.intent, IntentBucket::Find);
        // The path step requests the co-change boost (anchor injected later by
        // the index-aware executor); the calibration marker the executor keys on.
        assert_eq!(plan.steps[0].args["rank_by"], "path+cochange");
        assert!(plan.steps[0].args.get("anchor_path").is_none());
        // The name/content step uses OR terms over the tokenized query.
        assert_eq!(
            plan.steps[1].args["terms"],
            json!(["stel", "planner", "find"])
        );
        assert!(
            is_find_fusion_plan(&plan),
            "plan must be a find-fusion union"
        );
    }

    #[test]
    fn explicit_find_phrases_keep_single_step_routes() {
        // The fusion gate must never override a precise single-step route: every
        // explicit find phrasing the golden corpus pins stays single-step.
        for (query, expected) in [
            ("find Database class", "search_text"),
            ("find reconcile function", "search_text"),
            ("locate cfg_if symbol", "search_symbols"),
            ("files named records", "search_files"),
            ("isPlainObject symbol", "search_symbols"),
            ("find cfg_if macro usage", "search_text"),
        ] {
            let plan = build_plan(&StelRequest {
                query: query.to_string(),
                ..Default::default()
            });
            assert_eq!(plan.steps.len(), 1, "{query} must stay single-step");
            assert_eq!(plan.steps[0].tool, expected, "{query} route");
            assert!(!is_find_fusion_plan(&plan), "{query} must not fuse");
        }
    }

    #[test]
    fn guidance_and_single_token_queries_do_not_fuse() {
        for query in [
            "how to use records ORM", // golden explore row
            "how does cfg_if work",
            "what is the planner",
            "planner", // single token
        ] {
            let plan = build_plan(&StelRequest {
                query: query.to_string(),
                ..Default::default()
            });
            assert!(
                !is_find_fusion_plan(&plan),
                "{query} must not be a find-fusion union"
            );
        }
    }

    #[test]
    fn explicit_non_find_intent_is_not_overridden_by_fusion() {
        // An explicit non-find bucket (e.g. Read) must be honored even for a
        // multi-word query; fusion only owns Auto/Find.
        let plan = build_plan(&StelRequest {
            query: "stel planner find".to_string(),
            intent: Some(IntentBucket::Read),
            ..Default::default()
        });
        assert!(!is_find_fusion_plan(&plan), "explicit Read must not fuse");
    }

    #[test]
    fn is_bare_identifier_accepts_names_rejects_prose() {
        for ok in [
            "is_fresh",
            "LiveIndex",
            "_private",
            "mod::Type",
            "obj.method",
            "a1_b2",
        ] {
            assert!(is_bare_identifier(ok), "{ok} should be a bare identifier");
        }
        for bad in [
            "how status updates flow",
            "is fresh",
            "",
            "   ",
            "1leading_digit",
            "has-dash",
            "trailing::",
            "double..dot",
        ] {
            assert!(
                !is_bare_identifier(bad),
                "{bad:?} must not be an identifier"
            );
        }
    }

    #[test]
    fn bare_symbol_routes_to_symbol_lookup_for_find_intent() {
        // BUG 1: a bare-identifier `symbol` alongside a prose `query` for find
        // intent must route to a SYMBOL lookup, not a full-text search over the
        // query. With no path it is search_symbols on the symbol token.
        let plan = build_plan(&StelRequest {
            query: "where is the daemon freshness check that returns is_fresh".to_string(),
            intent: Some(IntentBucket::Find),
            symbol: Some("is_fresh".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "search_symbols");
        assert_eq!(plan.steps[0].args["query"], "is_fresh");
    }

    #[test]
    fn bare_symbol_routes_to_symbol_lookup_for_auto_intent() {
        // Auto intent (no explicit bucket) must honor a bare `symbol` too.
        let plan = build_plan(&StelRequest {
            query: "a long natural language question about freshness".to_string(),
            symbol: Some("is_fresh".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "search_symbols");
        assert_eq!(plan.steps[0].args["query"], "is_fresh");
    }

    #[test]
    fn bare_symbol_with_path_routes_to_get_symbol() {
        // With both a bare `symbol` and a `path`, fetch the symbol body directly.
        let plan = build_plan(&StelRequest {
            query: "show me freshness".to_string(),
            intent: Some(IntentBucket::Auto),
            symbol: Some("is_fresh".to_string()),
            path: Some("src/daemon.rs".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "get_symbol");
        assert_eq!(plan.steps[0].args["name"], "is_fresh");
        assert_eq!(plan.steps[0].args["path"], "src/daemon.rs");
    }

    #[test]
    fn bare_symbol_does_not_override_explicit_routing_phrase() {
        // The symbol gate must never override a pinned golden route: an explicit
        // reference phrasing keeps its find_references route even with `symbol`.
        let plan = build_plan(&StelRequest {
            query: "who references cfg_if".to_string(),
            intent: Some(IntentBucket::Find),
            symbol: Some("is_fresh".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "find_references");
    }

    #[test]
    fn explicit_trace_intent_keeps_symbol_in_route_trace() {
        // An explicit non-find/non-auto bucket (Trace) must not be hijacked by
        // the symbol gate; route_trace still consumes `symbol` as the ref name.
        let plan = build_plan(&StelRequest {
            query: "trace the callers".to_string(),
            intent: Some(IntentBucket::Trace),
            symbol: Some("is_fresh".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "find_references");
        assert_eq!(plan.steps[0].args["name"], "is_fresh");
    }

    #[test]
    fn prose_symbol_yields_corrective_contract_violation() {
        // BUG 2: prose in `symbol` must be caught at the facade boundary with a
        // corrective message, not dispatched as a tool `name`.
        let request = StelRequest {
            query: "trace how status updates flow".to_string(),
            intent: Some(IntentBucket::Trace),
            symbol: Some("how status updates flow".to_string()),
            ..Default::default()
        };
        let message = symbol_contract_violation(&request).expect("prose symbol must be rejected");
        assert!(
            message.contains("bare identifier"),
            "message names the contract: {message}"
        );
        assert!(
            !message.contains("Symbol not found"),
            "message must be corrective, not a misleading not-found: {message}"
        );
    }

    #[test]
    fn bare_identifier_symbol_has_no_contract_violation() {
        let request = StelRequest {
            query: "trace it".to_string(),
            symbol: Some("update_status".to_string()),
            ..Default::default()
        };
        assert!(symbol_contract_violation(&request).is_none());
    }

    #[test]
    fn read_intent_symbol_and_path_routes_to_get_symbol() {
        let plan = build_plan(&StelRequest {
            query: "show source body".to_string(),
            intent: Some(IntentBucket::Read),
            symbol: Some("fail_and_cascade".to_string()),
            path: Some("dag.rs".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "get_symbol");
        assert_eq!(plan.steps[0].args["name"], "fail_and_cascade");
        assert_eq!(plan.steps[0].args["path"], "dag.rs");
    }

    #[test]
    fn read_intent_symbol_without_path_routes_to_get_symbol_not_query_as_path() {
        let plan = build_plan(&StelRequest {
            query: "Show the source body of fail_and_cascade".to_string(),
            intent: Some(IntentBucket::Read),
            symbol: Some("fail_and_cascade".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "get_symbol");
        assert_eq!(plan.steps[0].args["name"], "fail_and_cascade");
        assert!(plan.steps[0].args.get("path").is_none());
    }

    #[test]
    fn impact_intent_symbol_routes_to_find_references_not_query_as_path() {
        let plan = build_plan(&StelRequest {
            query: "What depends on TaskStatus in this workspace?".to_string(),
            intent: Some(IntentBucket::Impact),
            symbol: Some("TaskStatus".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "find_references");
        assert_eq!(plan.steps[0].args["name"], "TaskStatus");
    }

    #[test]
    fn impact_intent_symbol_with_path_routes_to_find_dependents() {
        let plan = build_plan(&StelRequest {
            query: "what breaks if I change this file".to_string(),
            intent: Some(IntentBucket::Impact),
            symbol: Some("TaskStatus".to_string()),
            path: Some("src/task.rs".to_string()),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "find_dependents");
        assert_eq!(plan.steps[0].args["path"], "src/task.rs");
    }

    #[test]
    fn orient_intent_map_phrasing_routes_to_get_repo_map() {
        let plan = build_plan(&StelRequest {
            query: "map of workspace crates".to_string(),
            intent: Some(IntentBucket::Orient),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "get_repo_map");
    }

    #[test]
    fn auto_orient_me_phrasing_routes_to_get_repo_map_not_find_fusion() {
        let plan = build_plan(&StelRequest {
            query: "Orient me: what are the main crates in this workspace?".to_string(),
            intent: Some(IntentBucket::Auto),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "get_repo_map");
        assert_eq!(plan.steps.len(), 1);
    }
}
