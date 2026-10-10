//! MCP `symforge` (the compact facade) for an embedded source. The shared L1
//! planner and L2 economics decide the route; each planned primitive runs on
//! this source's native query lane; the shared executor, ledger and envelope
//! render the answer. This mirrors `SymForgeServer::symforge_stel_handler` on
//! its daemon-less path: there is no daemon to route a foreign project to, so
//! a `project` selector is checked against this source and never forwarded.

use serde_json::Value;

use super::embed_query::{Budget, EmbeddedQuerySnapshot};
use super::embed_session::QuerySession;
use crate::embed::parity::stel::{OutcomeClass, StelRequest, SymforgeAnswer, SymforgeStep};
use crate::embed::parity::{
    QueryObservation, QueryOutput, QueryPolicy, QueryRefusalKind, QueryRequest,
};
use crate::index_lifecycle::guidance::smart_query;
use crate::stel::runtime::{self, FacadeLedgerInput};

/// The native request one planned primitive routes to, decoded from the
/// planner's MCP arguments. A `project` argument was already checked against
/// this source, so it is not forwarded.
pub(super) fn step_request(tool: &str, args: &Value) -> Result<QueryRequest, QueryRefusalKind> {
    use crate::embed::parity::guidance::ExploreRequest;

    let mut args = args.clone();
    if let Value::Object(map) = &mut args {
        map.remove("project");
    }
    fn decode<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, QueryRefusalKind> {
        serde_json::from_value(args).map_err(|_| QueryRefusalKind::InvalidRequest)
    }
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| args.get(key).and_then(Value::as_u64);
    Ok(match tool {
        "search_text" => QueryRequest::TextSearch(decode(args)?),
        "search_symbols" => QueryRequest::SymbolSearch(decode(args)?),
        "search_files" => QueryRequest::FileSearch(decode(args)?),
        "search_knowledge" => QueryRequest::SearchKnowledge(decode(args)?),
        "get_file_context" => QueryRequest::FileContext(decode(args)?),
        "get_file_content" => QueryRequest::FileContent(decode(args)?),
        "get_symbol" => QueryRequest::SymbolRead(decode(args)?),
        "get_symbol_context" => QueryRequest::SymbolContext(decode(args)?),
        "find_references" => QueryRequest::ReferenceSearch(decode(args)?),
        "find_dependents" => QueryRequest::DependentSearch(decode(args)?),
        "what_changed" => QueryRequest::WhatChanged(decode(args)?),
        "diff_symbols" => QueryRequest::DiffSymbols(decode(args)?),
        "detect_impact" => QueryRequest::DetectImpact(decode(args)?),
        "analyze_file_impact" => QueryRequest::FileImpact(decode(args)?),
        "get_repo_map" => QueryRequest::RepoMap(decode(args)?),
        "ask" => QueryRequest::Ask(decode(args)?),
        "explore" => {
            let mut request = ExploreRequest::new(text("query").unwrap_or_default());
            if let Some(depth) = number("depth") {
                request.depth =
                    u32::try_from(depth).map_err(|_| QueryRefusalKind::InvalidRequest)?;
            }
            if let Some(limit) = number("limit") {
                request.limit =
                    u32::try_from(limit).map_err(|_| QueryRefusalKind::InvalidRequest)?;
            }
            request.max_tokens = number("max_tokens");
            request.path_prefix = text("path_prefix");
            request.language = text("language");
            QueryRequest::Explore(request)
        }
        "conventions" => QueryRequest::Conventions,
        "context_inventory" => QueryRequest::ContextInventory,
        "investigation_suggest" => QueryRequest::InvestigationSuggest {
            focus: text("focus"),
        },
        "symforge_retrieve" => QueryRequest::Retrieve {
            handle: text("hash").unwrap_or_default(),
            offset: 0,
        },
        _ => return Err(QueryRefusalKind::UnsupportedOption),
    })
}

/// The exact paths MCP's planned read primitives freshen when the facade
/// dispatches them.
pub(super) fn freshen_paths(request: &StelRequest) -> Vec<String> {
    crate::stel::planner::build_plan(request)
        .steps
        .iter()
        .filter_map(|step| step_request(&step.tool, &step.args).ok())
        .filter_map(|request| request.freshen_path().map(str::to_owned))
        .collect()
}

fn answer(outcome: OutcomeClass, rendered: String, steps: Vec<SymforgeStep>) -> QueryOutput {
    QueryOutput::Symforge(SymforgeAnswer {
        outcome,
        rendered,
        steps,
    })
}

fn refusal(text: impl Into<String>) -> Result<QueryOutput, QueryRefusalKind> {
    Ok(answer(
        OutcomeClass::InvalidRequest,
        text.into(),
        Vec::new(),
    ))
}

/// The outcome MCP's classifier would report for a primitive whose native
/// lane refused instead of rendering an answer.
fn refused_outcome(kind: QueryRefusalKind) -> OutcomeClass {
    match kind {
        QueryRefusalKind::NotFound => OutcomeClass::NotFound,
        QueryRefusalKind::AmbiguousSymbol => OutcomeClass::Ambiguous,
        QueryRefusalKind::InvalidRequest
        | QueryRefusalKind::UnsupportedOption
        | QueryRefusalKind::InvalidSpan => OutcomeClass::InvalidRequest,
        _ => OutcomeClass::InternalFailure,
    }
}

pub(super) fn project(
    snapshot: &EmbeddedQuerySnapshot,
    request: &StelRequest,
    budget: &mut Budget,
    observations: &mut Vec<QueryObservation>,
    session: Option<&QuerySession>,
    policy: QueryPolicy,
) -> Result<QueryOutput, QueryRefusalKind> {
    use crate::stel::controller::{build_estimate, evaluate_plan_tuned};
    use crate::stel::executor::{
        ServedStepResult, apply_compact_serve_caps, apply_degrade_to_plan, chain_failure_decision,
        format_bypass_body, format_cache_hit_body, format_multi_step_serve_body,
        format_partial_multi_step_serve_body, format_single_step_serve_body, is_degrade,
        route_tool_label, serve_chain_outcome_class, serve_step_outcome,
        should_skip_legacy_dispatch, tools_executed,
    };
    use crate::stel::handler::{self, metrics_for_decision_tuned};
    use crate::stel::planner::{build_plan, plan_summary_line};

    if request.query.trim().is_empty() {
        return refusal("query is required: pass a natural-language phrase");
    }
    if smart_query::strip_leading_articles(request.query.trim()).is_empty() {
        return refusal("query requires a non-empty question.");
    }
    if let Some(message) = crate::stel::planner::symbol_contract_violation(request) {
        return refusal(message);
    }
    if request.projects.is_some() {
        return refusal(
            "cross-project targeting is not routed through the `symforge` facade; call \
             search_symbols / search_text / find_references directly with project/projects.",
        );
    }
    let facade_project = request
        .project
        .as_deref()
        .filter(|project| !project.trim().is_empty());
    super::embed_changes::check_project(snapshot, facade_project)?;
    if let Some(path) = request.path.as_deref().filter(|p| !p.trim().is_empty()) {
        if !runtime::facade_path_is_repo_relative(path) {
            return refusal(
                "`path:` requires a repo-relative path: it is a within-project filter, not an \
                 absolute filesystem path or project selector.",
            );
        }
        if !runtime::path_is_within_bound_project(path, &snapshot.root) {
            return refusal(format!(
                "Path is outside the repository root: {path}. The selected project root is \
                 {}; `path:` is a within-project filter, not a project selector (use \
                 `index_folder {{ path }}` to retarget the workspace).",
                crate::paths::normalized_path_string(&snapshot.root)
            ));
        }
    }

    let live = &snapshot.generation.live;
    let bound_root = format!(
        "project_root: {}",
        crate::paths::normalized_path_string(&snapshot.root)
    );
    let mut plan = build_plan(request);
    runtime::ground_plan_economics(live, &mut plan);
    let tuned = runtime::active_tuning_for_economics(budget.stel_store.as_deref());
    let decision = match session {
        Some(session) => session.with_context(|context| {
            evaluate_plan_tuned(request, &plan, Some(context), tuned.as_ref())
        }),
        None => evaluate_plan_tuned(request, &plan, None, tuned.as_ref()),
    };
    let first_tool = plan
        .steps
        .first()
        .map(|step| step.tool.clone())
        .ok_or(QueryRefusalKind::InvalidRequest)?;
    let plan_summary = plan_summary_line(&plan);
    let session_tokens_served = session.map_or(0, QuerySession::served_tokens) as i64;

    if request.preview == Some(true) {
        let estimate = build_estimate(request, &plan, &decision);
        let body = handler::format_preview_estimate(&estimate);
        let metrics = metrics_for_decision_tuned(
            plan_summary,
            &decision,
            &plan,
            0,
            session_tokens_served,
            tuned.as_ref(),
        );
        let envelope = handler::envelope_for_decision(&metrics);
        let output = handler::prepend_envelope(&envelope, &body);
        budget.required(output.len())?;
        return Ok(answer(OutcomeClass::Found, output, Vec::new()));
    }

    if should_skip_legacy_dispatch(&decision) {
        let mut body = if decision.decision == crate::stel::AdmissionDecision::CacheHit {
            format_cache_hit_body(&decision)
        } else {
            format_bypass_body(&decision)
        };
        for note in crate::stel::planner::served_param_disclosures(request, &plan) {
            body.push_str("\n\n");
            body.push_str(&note);
        }
        let (output, event) = runtime::render_facade_answer(FacadeLedgerInput {
            surface: "symforge",
            plan: &plan,
            decision: &decision,
            plan_summary,
            session_tokens_served,
            body: &body,
            legacy_executed: false,
            selected_tool: &first_tool,
            tools_called: None,
            tuned: tuned.as_ref(),
        });
        budget.stel_event = Some(event);
        let output = format!("{output}\n{bound_root}");
        budget.required(output.len())?;
        return Ok(answer(OutcomeClass::Found, output, Vec::new()));
    }

    let exec_plan = if is_degrade(&decision) {
        apply_degrade_to_plan(&plan, &decision)
    } else if decision.decision == crate::stel::AdmissionDecision::Serve {
        apply_compact_serve_caps(&plan, &decision)
    } else {
        plan.clone()
    };
    let is_fusion_union = crate::stel::is_find_fusion_plan(&exec_plan);
    if let Some(project) = facade_project
        && let Some(step) = exec_plan
            .steps
            .iter()
            .find(|step| !runtime::facade_step_accepts_project(&step.tool))
    {
        return refusal(format!(
            "project '{project}' cannot be routed through this plan: planned step `{}` has no \
             project selector. Call the primitive tools directly with `project`.",
            step.tool
        ));
    }

    let mut step_results = Vec::new();
    let mut steps = Vec::new();
    let mut outcome_class = OutcomeClass::Found;
    let mut chain_failed = false;
    for step in &exec_plan.steps {
        let args = runtime::inject_find_fusion_cochange_anchor(live, &step.tool, &step.args, true);
        let nested = step_request(&step.tool, &args)?;
        let projected = super::embed_query::validate_request(&nested).and_then(|()| {
            super::embed_query::project(snapshot, &nested, budget, observations, session, policy)
        });
        // Each step answers once; the outer facade answer is what the
        // session caches, so a child's cache or handle never stands for it.
        budget.cache_output = None;
        budget.reused_handle = None;
        let (mut body, output, step_outcome) = match projected {
            Ok(output) => {
                let body = match render_search(snapshot, session, policy, &nested, &output) {
                    Some(rendered) => rendered?,
                    None => render_output(session, &nested, &output)?,
                };
                let step_outcome = serve_step_outcome(&step.tool, &body);
                (body, Some(Box::new(output)), step_outcome)
            }
            Err(QueryRefusalKind::Cancelled) => return Err(QueryRefusalKind::Cancelled),
            Err(QueryRefusalKind::DeadlineExceeded) => {
                return Err(QueryRefusalKind::DeadlineExceeded);
            }
            Err(QueryRefusalKind::BudgetTooSmall) => {
                return Err(QueryRefusalKind::BudgetTooSmall);
            }
            // MCP's `explore` answers a stopword-only query with this text
            // as a served body; the shared engine's refusal is the same one.
            Err(QueryRefusalKind::InvalidRequest) if step.tool == "explore" => {
                let body = "Explore requires a non-empty query.".to_string();
                let step_outcome = serve_step_outcome(&step.tool, &body);
                (body, None, step_outcome)
            }
            Err(kind) => (
                format!("Error: {} refused: {kind:?}", step.tool),
                None,
                refused_outcome(kind),
            ),
        };
        if step.tool == "search_knowledge" {
            body =
                crate::index_lifecycle::guidance::compression::rewrite_footer_for_symforge_facade(
                    body,
                );
        }
        step_results.push(ServedStepResult {
            tool: step.tool.clone(),
            body,
        });
        steps.push(SymforgeStep {
            tool: step.tool.clone(),
            request: nested,
            output,
            outcome: step_outcome,
        });
        if step.tool == "search_knowledge" && step_outcome == OutcomeClass::EmptyResult {
            outcome_class = OutcomeClass::EmptyResult;
            continue;
        }
        if step_outcome != OutcomeClass::Found {
            let fatal = !is_fusion_union
                || matches!(
                    step_outcome,
                    OutcomeClass::InternalFailure | OutcomeClass::InvalidRequest
                );
            if fatal {
                outcome_class = serve_chain_outcome_class(step_outcome);
                chain_failed = true;
                break;
            }
        }
    }

    if is_fusion_union && !chain_failed && !steps.is_empty() {
        let all_empty = steps.iter().all(|step| {
            matches!(
                step.outcome,
                OutcomeClass::EmptyResult | OutcomeClass::NotFound
            )
        });
        if all_empty {
            outcome_class = OutcomeClass::EmptyResult;
        }
    }

    let mut effective_decision = decision.clone();
    if chain_failed {
        let failed_index = steps.len().saturating_sub(1);
        effective_decision = chain_failure_decision(
            &plan,
            &decision,
            failed_index,
            &steps[failed_index].tool,
            steps[failed_index].outcome,
        );
    }
    let chain_failure_note = chain_failed.then(|| effective_decision.decision_reason.clone());
    let mut body = if exec_plan.steps.len() == 1 {
        format_single_step_serve_body(
            &plan,
            &effective_decision,
            &exec_plan.steps[0],
            &step_results[0].body,
        )
    } else if chain_failed {
        format_partial_multi_step_serve_body(
            &plan,
            &effective_decision,
            &step_results,
            chain_failure_note.as_deref(),
        )
    } else {
        format_multi_step_serve_body(&plan, &effective_decision, &step_results)
    };
    if plan.intent == crate::stel::IntentBucket::Impact
        && !chain_failed
        && exec_plan.steps.len() == 1
    {
        let temporal = super::embed_temporal::temporal(snapshot, policy, budget)?;
        runtime::append_impact_intent_cochanges(&temporal, &mut body, &exec_plan.steps[0].args);
    }
    for note in crate::stel::planner::served_param_disclosures(request, &plan) {
        body.push_str("\n\n");
        body.push_str(&note);
    }
    let tools = tools_executed(&step_results);
    let route_label = route_tool_label(&tools);
    let tools_called = (tools.len() > 1).then_some(tools);
    let (output, event) = runtime::render_facade_answer(FacadeLedgerInput {
        surface: "symforge",
        plan: &plan,
        decision: &effective_decision,
        plan_summary,
        session_tokens_served,
        body: &body,
        legacy_executed: !step_results.is_empty(),
        selected_tool: &route_label,
        tools_called,
        tuned: tuned.as_ref(),
    });
    budget.stel_event = Some(event);
    let output = format!("{output}\n{bound_root}");
    budget.required(output.len())?;
    Ok(answer(outcome_class, output, steps))
}

/// MCP's search-tool text for a search step: the same shared engine rendered
/// over the same publication the native lane answered from, with the result
/// envelope MCP's search tools prepend. `None` for every other step.
pub(super) fn render_search(
    snapshot: &EmbeddedQuerySnapshot,
    session: Option<&QuerySession>,
    policy: QueryPolicy,
    request: &QueryRequest,
    output: &QueryOutput,
) -> Option<Result<String, QueryRefusalKind>> {
    // MCP's `apply_ccr_budget`: through the session's CCR store when there is
    // one, else the same token cut without a retrievable handle.
    let budget = |tool: &str, rendered: String, max_tokens: Option<u64>| match session {
        Some(session) => session.apply_ccr_budget(snapshot, tool, rendered, max_tokens),
        None => super::guidance::source::enforce_token_budget(
            rendered,
            super::guidance::compression::resolve_tool_max_tokens(tool, max_tokens),
        ),
    };
    use super::guidance::reference_read::with_withheld_note;
    use super::guidance::{file_search, search};
    let generation = &snapshot.generation;
    let rendered = match (request, output) {
        (QueryRequest::TextSearch(input), QueryOutput::TextSearch(value)) => {
            let shared = super::embed_search::text_input(input);
            let executed = match search::execute_text_search(generation, &shared) {
                Ok(executed) => executed,
                Err(_) => return Some(Err(QueryRefusalKind::InvalidRequest)),
            };
            let mut rendered = search::render_search_text_output(
                generation,
                executed.result,
                executed.effective_query.as_deref(),
                executed.structural,
                shared.group_by.as_deref(),
                shared.terms.as_deref(),
                &executed.options,
                executed.is_regex,
                executed.auto_detected_regex,
                executed.auto_corrected_regex,
                &value.untracked_paths,
            );
            if executed.auto_corrected_regex {
                rendered.push_str(&format!(
                    "\n(auto-corrected double-escaped regex: `{}` → `{}`)",
                    shared.query.as_deref().unwrap_or_default(),
                    executed.effective_query.as_deref().unwrap_or_default()
                ));
            }
            budget(
                "search_text",
                with_withheld_note(&generation.live, input.path_prefix.as_deref(), rendered),
                input.max_tokens,
            )
        }
        (QueryRequest::SymbolSearch(input), QueryOutput::SymbolSearch(_)) => {
            let shared = super::embed_search::symbol_input(input);
            let executed = match search::execute_symbol_search(&generation.live, &shared) {
                Ok(executed) => executed,
                Err(_) => return Some(Err(QueryRefusalKind::InvalidRequest)),
            };
            let rendered = search::render_symbol_search(
                generation,
                &executed,
                shared.query.as_deref().unwrap_or("").trim(),
            );
            budget(
                "search_symbols",
                with_withheld_note(&generation.live, input.path_prefix.as_deref(), rendered),
                input.max_tokens,
            )
        }
        (QueryRequest::FileSearch(input), QueryOutput::FileSearch(value))
            if input.resolve != Some(true) && input.changed_with.is_none() =>
        {
            let shared = super::embed_file_search::input(input);
            let ranked = file_search::rank_files(
                generation,
                Some(&snapshot.root),
                snapshot.project_state.as_ref(),
                &shared,
                policy.allow_derived_state_preparation && snapshot.project_state.is_some(),
            );
            file_search::render_ranked_files(
                generation,
                ranked,
                &shared,
                shared.include_vendor.unwrap_or(false),
                shared.include_personal_tooling.unwrap_or(false),
                || value.untracked_paths.clone(),
            )
        }
        _ => return None,
    };
    Some(Ok(rendered))
}

/// The MCP text of one primitive's native answer.
fn render_output(
    session: Option<&QuerySession>,
    request: &QueryRequest,
    output: &QueryOutput,
) -> Result<String, QueryRefusalKind> {
    Ok(match output {
        QueryOutput::FileContent(value) => value.rendered.clone(),
        QueryOutput::FileContext(value) => value.rendered.clone(),
        QueryOutput::SymbolRead(value) => value.rendered.clone(),
        QueryOutput::InspectMatch(value) => value.rendered.clone(),
        QueryOutput::DiffSymbols(value) => value.rendered.clone(),
        QueryOutput::FileImpact(value) => value.rendered.clone(),
        QueryOutput::Ask(value) => value.rendered.clone(),
        QueryOutput::InvestigationSuggestion(value) => value.text.clone(),
        QueryOutput::RetrievedOutput(value) => String::from_utf8_lossy(&value.bytes).into_owned(),
        QueryOutput::ContextInventory(_) => match session {
            Some(session) => session.render_inventory(),
            None => return Err(QueryRefusalKind::SessionRequired),
        },
        QueryOutput::Conventions(value) => {
            crate::index_lifecycle::guidance::conventions::format_conventions(
                &crate::index_lifecycle::guidance::conventions::ProjectConventions {
                    language: value.language.clone(),
                    error_handling: value.error_handling.clone(),
                    naming: value.naming.clone(),
                    test_patterns: value.test_patterns.clone(),
                    common_imports: value.common_imports.clone(),
                    file_organization: value.file_organization.clone(),
                    complexity: value.complexity.clone(),
                },
            )
        }
        QueryOutput::DetectImpact(value) => detect_impact_text(value),
        _ => super::embed_ask::render_routed(request, output)?,
    })
}

/// MCP `detect_impact`'s text: the shared summary over the same payload the
/// MCP handler builds.
fn detect_impact_text(value: &crate::embed::parity::detect_impact::DetectImpactResult) -> String {
    let page = |page: &crate::embed::parity::detect_impact::ImpactPage| {
        serde_json::json!({
            "total": page.total,
            "returned": page.returned,
            "truncated": page.truncated,
        })
    };
    let payload = serde_json::json!({
        "changed_files": value.changed_files,
        "changed_symbols": value
            .changed_symbols
            .iter()
            .map(|s| serde_json::json!({"name": s.name, "path": s.path, "kind": s.kind}))
            .collect::<Vec<_>>(),
        "blast_radius": value
            .blast_radius
            .iter()
            .map(|b| serde_json::json!({"symbol": b.symbol, "hop": b.hop, "risk": b.risk}))
            .collect::<Vec<_>>(),
        "risk_summary": {
            "critical": value.risk_summary.critical,
            "high": value.risk_summary.high,
            "medium": value.risk_summary.medium,
            "low": value.risk_summary.low,
        },
        "pagination": {
            "changed_files": page(&value.pagination.changed_files),
            "changed_symbols": page(&value.pagination.changed_symbols),
            "blast_radius": page(&value.pagination.blast_radius),
        },
        "source_filter": {
            "applied": value.source_filter.applied,
            "excluded_paths": value.source_filter.excluded_paths,
            "hint": value.source_filter.hint,
        },
    });
    crate::index_lifecycle::guidance::changes::detect_impact_result(
        &payload,
        value.requested_depth,
        value.effective_depth,
        value.base_branch.as_deref(),
        value.staleness_note.as_deref(),
    )
}
