//! One source capture and one session operation for a routed native question.
use super::embed_query::{Budget, EmbeddedQuerySnapshot};
use super::guidance::{routing, search_render, smart_query, source};
use crate::embed::parity::ask::{AskRequest, AskResult, AskRoute};
use crate::embed::parity::{
    QueryObservation, QueryOutput, QueryPolicy, QueryRefusalKind, QueryRequest,
};

pub(super) fn validate(request: &AskRequest) -> Result<(), QueryRefusalKind> {
    if smart_query::strip_leading_articles(request.query.trim()).is_empty()
        || request.max_tokens == Some(0)
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    crate::knowledge::guard_query(&request.query)
        .map_err(|_| QueryRefusalKind::AdmissionUnavailable)
}

fn routed_request(
    intent: &smart_query::QueryIntent,
    max_tokens: Option<u64>,
) -> (AskRoute, Option<QueryRequest>) {
    use crate::embed::parity::{
        changes, guidance, knowledge, read_context, reference, search, symbol_context,
    };
    use smart_query::QueryIntent as Intent;
    let explore = |query: &str| {
        let mut request = guidance::ExploreRequest::new(query);
        request.depth = 2;
        QueryRequest::Explore(request)
    };
    match intent {
        Intent::FindCallers { symbol, path } => (
            AskRoute::FindCallers,
            Some(QueryRequest::ReferenceSearch(
                reference::ReferenceSearchRequest {
                    name: symbol.clone(),
                    path: path.clone(),
                    compact: Some(true),
                    ..Default::default()
                },
            )),
        ),
        Intent::FindSymbol { name, kind } => (
            AskRoute::FindSymbol,
            Some(QueryRequest::SymbolSearch(search::SymbolSearchRequest {
                query: Some(name.clone()),
                kind: kind.clone(),
                ..Default::default()
            })),
        ),
        Intent::FindFile { hint } => (
            AskRoute::FindFile,
            Some(QueryRequest::FileSearch(search::FileSearchRequest {
                query: hint.clone(),
                ..Default::default()
            })),
        ),
        Intent::FindChanges => (
            AskRoute::FindChanges,
            Some(QueryRequest::WhatChanged(changes::WhatChangedRequest {
                uncommitted: Some(true),
                code_only: Some(true),
                include_symbol_diff: Some(true),
                ..Default::default()
            })),
        ),
        Intent::Understand { concept } => (AskRoute::Understand, Some(explore(concept))),
        Intent::UnderstandSymbol { symbol } => (
            AskRoute::UnderstandSymbol,
            Some(QueryRequest::SymbolContext(
                symbol_context::SymbolContextRequest {
                    name: symbol.clone(),
                    verbosity: Some("compact".into()),
                    ..Default::default()
                },
            )),
        ),
        Intent::UnderstandImplementations { name } | Intent::FindImplementations { name } => (
            if matches!(intent, Intent::UnderstandImplementations { .. }) {
                AskRoute::UnderstandImplementations
            } else {
                AskRoute::FindImplementations
            },
            Some(QueryRequest::ReferenceSearch(
                reference::ReferenceSearchRequest {
                    name: name.clone(),
                    mode: Some("implementations".into()),
                    ..Default::default()
                },
            )),
        ),
        Intent::SearchCode { pattern } => (
            AskRoute::SearchCode,
            Some(QueryRequest::TextSearch(search::TextSearchRequest {
                query: Some(pattern.clone()),
                ..Default::default()
            })),
        ),
        Intent::SearchKnowledge { query } => (
            AskRoute::SearchKnowledge,
            Some(QueryRequest::SearchKnowledge(
                knowledge::search::SearchKnowledgeRequest {
                    query: query.clone(),
                    source_scope: Some(knowledge::KnowledgeSourceScope::Current),
                    authority_scope: Some(
                        crate::knowledge::search_contract::KnowledgeAuthorityScope::Default,
                    ),
                    path_prefix: None,
                    project: None,
                    projects: None,
                    limit: None,
                    // MCP forwards the outer budget to this route.
                    max_tokens,
                },
            )),
        ),
        Intent::RepositoryOrientation => (
            AskRoute::RepositoryOrientation,
            Some(QueryRequest::RepoMap(read_context::RepoMapRequest {
                detail: Some("compact".into()),
                max_tokens,
                ..Default::default()
            })),
        ),
        Intent::FindDependents { target } => (
            AskRoute::FindDependents,
            Some(QueryRequest::DependentSearch(
                reference::DependentSearchRequest {
                    path: target.clone(),
                    compact: Some(true),
                    ..Default::default()
                },
            )),
        ),
        Intent::ToolHelp { .. } => (AskRoute::ToolHelp, None),
        Intent::Explore { query } => (AskRoute::Explore, Some(explore(query))),
    }
}

pub(super) fn project(
    snapshot: &EmbeddedQuerySnapshot,
    request: &AskRequest,
    budget: &mut Budget,
    observations: &mut Vec<QueryObservation>,
    session: Option<&super::embed_session::QuerySession>,
    policy: QueryPolicy,
) -> Result<QueryOutput, QueryRefusalKind> {
    super::embed_changes::check_project(snapshot, request.project.as_deref())?;
    let original = request.query.trim();
    let normalized = smart_query::strip_leading_articles(original);
    let (intent, matched) = routing::resolve(&snapshot.generation.live, normalized);
    let assessment = smart_query::assess_route(&intent, matched);
    let (route, routed_request) = routed_request(&intent, request.max_tokens);
    let mut result = AskResult {
        route,
        confidence: smart_query::route_confidence_label(assessment.confidence).into(),
        invocation: smart_query::route_invocation(&intent),
        rationale: assessment.rationale.into(),
        suggested_next_step: assessment.suggested_next_step.map(str::to_owned),
        routed_request: routed_request.map(Box::new),
        output: None,
        rendered: String::new(),
    };
    budget.required(super::embed_usage::decoded_bytes(&result) as usize)?;
    let rendered = if let Some(nested) = result.routed_request.as_deref() {
        super::embed_query::validate_request(nested)?;
        let output =
            super::embed_query::project(snapshot, nested, budget, observations, session, policy)?;
        let rendered = render_routed(nested, &output)?;
        result.output = Some(Box::new(output));
        rendered
    } else if let smart_query::QueryIntent::ToolHelp { topic } = &intent {
        smart_query::render_tool_catalog_for_topic(topic.as_deref())
    } else {
        return Err(QueryRefusalKind::InvalidRequest);
    };
    // A child may prepare a larger retrieval payload, but the session receipt
    // must always refer to the outer operation that the caller actually made.
    let full_child = budget.cache_output.take();
    let full_rendered =
        smart_query::render_answer(&intent, assessment, original, normalized, &rendered);
    let bounded = budget.text(&full_rendered)?;
    result.rendered = bounded;
    let mut cached = result.clone();
    if let Some(output) = full_child {
        cached.output = Some(Box::new(output));
    }
    budget.cache_output = Some(QueryOutput::Ask(cached));
    budget.cache_truncated |= budget.truncated;
    let (rendered, capped) =
        source::enforce_token_budget_flagged(result.rendered, request.max_tokens);
    budget.truncated |= capped;
    result.rendered = rendered;
    // A routed cache hit still commits one Ask operation and receives an outer
    // handle; reusing a child handle would mislabel both its schema and history.
    budget.reused_handle = None;
    Ok(QueryOutput::Ask(result))
}

fn render_routed(request: &QueryRequest, output: &QueryOutput) -> Result<String, QueryRefusalKind> {
    use crate::embed::parity::search as dto;
    use crate::live_index::search as engine;
    Ok(match (request, output) {
        (_, QueryOutput::ReferenceSearch(value)) => value.rendered.clone(),
        (_, QueryOutput::DependentSearch(value)) => value.rendered.clone(),
        (_, QueryOutput::SymbolContext(value)) => value.rendered.clone(),
        (_, QueryOutput::SearchKnowledge(value)) => value.rendered.clone(),
        (_, QueryOutput::RepoMap(value)) => value.rendered.clone(),
        (_, QueryOutput::WhatChanged(value)) => value.rendered.clone(),
        (QueryRequest::SymbolSearch(request), QueryOutput::SymbolSearch(value)) => {
            let rows = engine::SymbolSearchResult {
                file_count: value.file_count as usize,
                overflow_count: value.overflow_count as usize,
                hits: value
                    .symbols
                    .iter()
                    .map(|hit| engine::SymbolSearchHit {
                        tier: match hit.tier {
                            dto::SymbolMatchTier::Exact => engine::SymbolMatchTier::Exact,
                            dto::SymbolMatchTier::Prefix => engine::SymbolMatchTier::Prefix,
                            dto::SymbolMatchTier::Substring => engine::SymbolMatchTier::Substring,
                        },
                        name: hit.symbol.name.clone(),
                        path: hit.symbol.path.clone(),
                        kind: hit.symbol.kind.clone(),
                        line: hit.symbol.start_line,
                    })
                    .collect(),
            };
            search_render::search_symbols_result_view(
                &rows,
                request.query.as_deref().unwrap_or_default(),
            )
        }
        (QueryRequest::TextSearch(request), QueryOutput::TextSearch(value)) => {
            let dto::TextRows::Files(files) = &value.rows else {
                return Err(QueryRefusalKind::InvalidRequest);
            };
            let query = request.query.as_deref().unwrap_or_default();
            let rows = engine::TextSearchResult {
                label: if value.structural {
                    format!("structural '{query}'")
                } else if value.regex {
                    format!("regex '{query}'")
                } else {
                    format!("'{query}'")
                },
                total_matches: value.total_matches as usize,
                overflow_count: value.overflow_count as usize,
                suppressed_by_noise: value.suppressed_by_noise as usize,
                excluded_knowledge_files: value.excluded_knowledge_files as usize,
                withheld_policy_files: value.withheld_policy_files as usize,
                withheld_size_files: value.withheld_size_files as usize,
                files: files
                    .iter()
                    .map(|file| engine::TextFileMatches {
                        path: file.path.clone(),
                        matches: file
                            .matches
                            .iter()
                            .map(|row| engine::TextLineMatch {
                                line_number: row.line as usize,
                                line: row.preview.clone(),
                                enclosing_symbol: row.enclosing_symbol.as_ref().map(|symbol| {
                                    engine::EnclosingMatchSymbol {
                                        name: symbol.name.clone(),
                                        kind: symbol.kind.clone(),
                                        line_range: (
                                            symbol.start_line.saturating_sub(1),
                                            symbol.end_line.saturating_sub(1),
                                        ),
                                    }
                                }),
                            })
                            .collect(),
                        rendered_lines: file.context.as_ref().map(|lines| {
                            lines
                                .iter()
                                .map(|line| match line {
                                    dto::ContextLine::Separator => {
                                        engine::TextDisplayLine::Separator
                                    }
                                    dto::ContextLine::Line {
                                        line,
                                        text,
                                        is_match,
                                    } => engine::TextDisplayLine::Line(engine::TextRenderedLine {
                                        line_number: *line as usize,
                                        line: text.clone(),
                                        is_match: *is_match,
                                    }),
                                })
                                .collect()
                        }),
                        callers: file.callers.as_ref().map(|rows| {
                            rows.iter()
                                .map(|row| engine::CallerEntry {
                                    file: row.path.clone(),
                                    symbol: row.symbol.clone(),
                                    line: row.line,
                                })
                                .collect()
                        }),
                    })
                    .collect(),
            };
            search_render::search_text_result_view(
                Ok(rows),
                None,
                None,
                None,
                search_render::SearchSuggestionContext {
                    regex: value.regex,
                    include_tests: request.include_tests.unwrap_or(false),
                    multi_word_literal: !value.regex && query.trim().contains(char::is_whitespace),
                },
            )
        }
        (QueryRequest::FileSearch(request), QueryOutput::FileSearch(value)) => {
            use crate::live_index::{SearchFilesHit, SearchFilesTier as Tier, SearchFilesView};
            let rows = if value.hits.is_empty() {
                SearchFilesView::NotFound {
                    query: request.query.clone(),
                }
            } else {
                SearchFilesView::Found {
                    query: request.query.clone(),
                    total_matches: value.total_matches as usize,
                    overflow_count: value.overflow_count as usize,
                    hits: value
                        .hits
                        .iter()
                        .map(|row| SearchFilesHit {
                            tier: match row.tier {
                                dto::FileMatchTier::CoChange => Tier::CoChange,
                                dto::FileMatchTier::StrongPath => Tier::StrongPath,
                                dto::FileMatchTier::Basename => Tier::Basename,
                                dto::FileMatchTier::LoosePath => Tier::LoosePath,
                                dto::FileMatchTier::MetadataOnly => Tier::MetadataOnly,
                            },
                            path: row.path.clone(),
                            coupling_score: row.coupling_score,
                            shared_commits: row.shared_commits,
                            metadata_reason: row.metadata_reason.clone(),
                        })
                        .collect(),
                }
            };
            search_render::search_files_result_view(&rows)
        }
        (_, QueryOutput::Exploration(value)) => {
            let hits: Vec<_> = value
                .symbols
                .iter()
                .map(|row| (row.name.clone(), row.kind.clone(), row.path.clone()))
                .collect();
            let scores: Vec<_> = value
                .symbols
                .iter()
                .map(|row| row.score_millionths as f32 / 1_000_000.0)
                .collect();
            let enriched: Vec<_> = value
                .symbols
                .iter()
                .map(|row| {
                    (
                        row.name.clone(),
                        row.kind.clone(),
                        row.path.clone(),
                        row.signature.clone(),
                        row.dependent_files.clone(),
                    )
                })
                .collect();
            let implementations: Vec<_> = value
                .symbols
                .iter()
                .filter(|row| !row.implementations.is_empty())
                .map(|row| (row.name.clone(), row.implementations.clone()))
                .collect();
            let dependencies: Vec<_> = value
                .symbols
                .iter()
                .filter(|row| !row.type_dependencies.is_empty())
                .map(|row| (row.name.clone(), row.type_dependencies.clone()))
                .collect();
            let text: Vec<_> = value
                .text_matches
                .iter()
                .map(|row| (row.path.clone(), row.preview.clone(), row.line as usize))
                .collect();
            let related: Vec<_> = value
                .related_files
                .iter()
                .map(|row| (row.path.clone(), row.matches as usize))
                .collect();
            search_render::explore_result_view(search_render::ExploreResultViewInput {
                label: &value.label,
                symbol_hits: &hits,
                text_hits: &text,
                related_files: &related,
                enriched_symbols: &enriched,
                symbol_impls: &implementations,
                symbol_deps: &dependencies,
                derived_seed_terms: &value.derived_seed_terms,
                derived_symbols: &value.derived_symbols,
                derived_seed_files: &value.derived_seed_files,
                enriched_imports: &value.enriched_imports,
                symbol_scores: &scores,
                depth: value.depth,
            })
        }
        _ => return Err(QueryRefusalKind::InvalidRequest),
    })
}
