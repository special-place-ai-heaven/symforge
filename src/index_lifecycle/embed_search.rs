//! Bounded typed projections of shared search plans.
use super::embed_query::{
    Budget, EmbeddedQuerySnapshot, symbol_row, symbol_size, text_line, validate_kind, validate_path,
};
use super::guidance::search_contract;
use crate::embed::parity::search::*;
use crate::embed::parity::{QueryOutput, QueryRefusalKind};
use crate::live_index::search as engine;
use std::collections::{BTreeSet, HashMap};

fn text_input(input: &TextSearchRequest) -> search_contract::SearchTextInput {
    search_contract::SearchTextInput {
        query: input.query.clone(),
        terms: input.terms.clone(),
        regex: input.regex,
        structural: input.structural,
        path_prefix: input.path_prefix.clone(),
        language: input.language.clone(),
        limit: input.limit,
        max_per_file: input.max_per_file,
        include_generated: input.include_generated,
        include_tests: input.include_tests,
        include_vendor: input.include_vendor,
        include_personal_tooling: input.include_personal_tooling,
        glob: input.glob.clone(),
        exclude_glob: input.exclude_glob.clone(),
        context: input.context,
        case_sensitive: input.case_sensitive,
        whole_word: input.whole_word,
        follow_refs: input.follow_refs,
        follow_refs_limit: input.follow_refs_limit,
        ranked: input.ranked,
        estimate: input.estimate,
        max_tokens: input.max_tokens,
        group_by: input.group_by.map(|group| {
            match group {
                TextGrouping::File => "file",
                TextGrouping::Symbol => "symbol",
                TextGrouping::Usage => "usage",
                TextGrouping::Names => "names",
            }
            .into()
        }),
        project: None,
        projects: None,
    }
}
fn symbol_input(input: &SymbolSearchRequest) -> search_contract::SearchSymbolsInput {
    search_contract::SearchSymbolsInput {
        query: input.query.clone(),
        kind: input.kind.clone(),
        path_prefix: input.path_prefix.clone(),
        language: input.language.clone(),
        limit: input.limit,
        include_generated: input.include_generated,
        include_tests: input.include_tests,
        include_vendor: input.include_vendor,
        include_personal_tooling: input.include_personal_tooling,
        estimate: input.estimate,
        max_tokens: input.max_tokens,
        project: None,
        projects: None,
    }
}
fn common_validation(
    prefix: Option<&str>,
    query: Option<&str>,
    limit: Option<u32>,
    max_tokens: Option<u64>,
) -> Result<(), QueryRefusalKind> {
    if let Some(prefix) = prefix {
        validate_path(prefix, true)?;
    }
    if let Some(query) = query {
        crate::knowledge::guard_query(query).map_err(|_| QueryRefusalKind::AdmissionUnavailable)?;
    }
    if limit.is_some_and(|limit| limit == 0 || limit > 10_000) || max_tokens == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}
pub(super) fn validate_text(input: &TextSearchRequest) -> Result<(), QueryRefusalKind> {
    common_validation(
        input.path_prefix.as_deref(),
        input.query.as_deref(),
        input.limit,
        input.max_tokens,
    )?;
    if let Some(terms) = &input.terms {
        for term in terms {
            crate::knowledge::guard_query(term)
                .map_err(|_| QueryRefusalKind::AdmissionUnavailable)?;
        }
    }
    if input
        .max_per_file
        .is_some_and(|limit| limit == 0 || limit > 10_000)
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    search_contract::search_text_options_from_input(&text_input(input))
        .map_err(|_| QueryRefusalKind::UnsupportedOption)?;
    Ok(())
}
pub(super) fn validate_symbol(input: &SymbolSearchRequest) -> Result<(), QueryRefusalKind> {
    common_validation(
        input.path_prefix.as_deref(),
        input.query.as_deref(),
        input.limit,
        input.max_tokens,
    )?;
    validate_kind(input.kind.as_deref())?;
    search_contract::search_symbols_options_from_input(&symbol_input(input))
        .map_err(|_| QueryRefusalKind::UnsupportedOption)?;
    Ok(())
}
pub(super) fn symbols(
    snapshot: &EmbeddedQuerySnapshot,
    input: &SymbolSearchRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    let live = &snapshot.generation.live;
    let executed = super::guidance::search::execute_symbol_search(live, &symbol_input(input))
        .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    budget.check()?;
    if input.estimate == Some(true) {
        budget.required(0)?;
        let approximate_tokens = executed
            .result
            .hits
            .iter()
            .map(|hit| ((hit.path.len() + hit.name.len() + hit.kind.len() + 128) / 4) as u64)
            .sum::<u64>()
            + 24;
        return Ok(QueryOutput::SearchEstimate(SearchEstimate {
            approximate_tokens,
        }));
    }
    cache_before_token_cap(budget, input.max_tokens, |budget| {
        project_symbols(snapshot, &executed, budget)
    })
}

fn cache_before_token_cap(
    budget: &mut Budget,
    max_tokens: Option<u64>,
    project: impl Fn(&mut Budget) -> Result<QueryOutput, QueryRefusalKind>,
) -> Result<QueryOutput, QueryRefusalKind> {
    if max_tokens.is_some() {
        let mut full_budget = budget.projection_budget();
        budget.cache_output = Some(project(&mut full_budget)?);
        budget.cache_truncated = full_budget.truncated;
        budget.check()?;
        budget.cap_tokens(max_tokens);
    }
    project(budget)
}

fn project_symbols(
    snapshot: &EmbeddedQuerySnapshot,
    executed: &super::guidance::search::SymbolSearchExecution,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    let live = &snapshot.generation.live;
    budget.truncated |= executed.result.overflow_count > 0;
    let total_matches = executed
        .result
        .hits
        .len()
        .saturating_add(executed.result.overflow_count) as u64;
    let mut symbols = Vec::new();
    let mut consumed = BTreeSet::new();
    for hit in &executed.result.hits {
        let file = live
            .get_file(&hit.path)
            .ok_or(QueryRefusalKind::InvalidSpan)?;
        let (index, symbol) = file
            .symbols
            .iter()
            .enumerate()
            .find(|(index, symbol)| {
                !consumed.contains(&(hit.path.clone(), *index))
                    && symbol.name == hit.name
                    && symbol.line_range.0 + 1 == hit.line
                    && symbol.kind.to_string() == hit.kind
            })
            .ok_or(QueryRefusalKind::InvalidSpan)?;
        consumed.insert((hit.path.clone(), index));
        let symbol = symbol_row(file, index, symbol)?;
        let tier = match hit.tier {
            engine::SymbolMatchTier::Exact => SymbolMatchTier::Exact,
            engine::SymbolMatchTier::Prefix => SymbolMatchTier::Prefix,
            engine::SymbolMatchTier::Substring => SymbolMatchTier::Substring,
        };
        let tier_bytes = match tier {
            SymbolMatchTier::Exact => 5,
            SymbolMatchTier::Prefix => 6,
            SymbolMatchTier::Substring => 9,
        };
        if !budget.row(symbol_size(&symbol) + tier_bytes) {
            break;
        }
        symbols.push(SymbolSearchHit { symbol, tier });
    }
    let mut text_fallback = Vec::new();
    for (path, line) in &executed.text_fallback {
        if !budget.row(path.len()) {
            break;
        }
        text_fallback.push(TextFallback {
            path: path.clone(),
            line: *line as u64,
        });
    }
    Ok(QueryOutput::SymbolSearch(SymbolSearchResult {
        symbols,
        total_matches,
        overflow_count: executed.result.overflow_count as u64,
        file_count: executed.result.file_count as u64,
        suppressed_by_noise: executed.suppressed_by_noise as u64,
        text_fallback,
    }))
}
pub(super) fn text(
    snapshot: &EmbeddedQuerySnapshot,
    input: &TextSearchRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    if input.estimate == Some(true) {
        budget.required(0)?;
        return Ok(QueryOutput::SearchEstimate(SearchEstimate {
            approximate_tokens: u64::from(input.limit.unwrap_or(50)) * 20 + 50,
        }));
    }
    let shared_input = text_input(input);
    let executed =
        super::guidance::search::execute_text_search(&snapshot.generation, &shared_input)
            .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    budget.check()?;
    let untracked_paths = untracked_sweep(snapshot, &shared_input, &executed);
    budget.check()?;
    cache_before_token_cap(budget, input.max_tokens, |budget| {
        project_text(snapshot, input, &executed, &untracked_paths, budget)
    })
}

/// MCP `render_search_text_output`'s zero-hit sweep over the same captured
/// publication, read through the shared gate exactly as
/// `admit_worktree_text_without_lines` reads for MCP.
fn untracked_sweep(
    snapshot: &EmbeddedQuerySnapshot,
    input: &search_contract::SearchTextInput,
    executed: &super::guidance::search::TextSearchExecution,
) -> Vec<String> {
    let zero_hit = matches!(
        &executed.result,
        Ok(found) if found.files.is_empty() && found.suppressed_by_noise == 0
    );
    if !zero_hit || executed.structural {
        return Vec::new();
    }
    let Some(repo) = super::embed_changes::open_repository(&snapshot.root) else {
        return Vec::new();
    };
    let live = &snapshot.generation.live;
    super::guidance::search::matching_untracked_paths_for_search_text(
        &repo,
        live,
        executed.effective_query.as_deref(),
        input.terms.as_deref(),
        executed.is_regex,
        &executed.options,
        &mut |path| {
            super::guidance::read_gate::worktree_text_with(&repo, path, &mut |full_path| {
                super::guidance::read_gate::disk_read_with(
                    live,
                    path,
                    full_path,
                    &mut || None,
                    &mut |_| {},
                )
            })
        },
    )
}

fn project_text(
    snapshot: &EmbeddedQuerySnapshot,
    input: &TextSearchRequest,
    executed: &super::guidance::search::TextSearchExecution,
    untracked_paths: &[String],
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    let found = executed.result.as_ref().map_err(|error| match error {
        engine::TextSearchError::UnsupportedWholeWordRegex
        | engine::TextSearchError::UnsupportedStructuralLanguage { .. } => {
            QueryRefusalKind::UnsupportedOption
        }
        _ => QueryRefusalKind::InvalidRequest,
    })?;
    budget.truncated |= found.overflow_count > 0;
    let grouping = input.group_by.unwrap_or_default();
    let context_mode = found.files.iter().any(|file| file.rendered_lines.is_some());
    let live = &snapshot.generation.live;
    let usage = grouping == TextGrouping::Usage && !context_mode;
    let excluded_by_usage = if usage {
        found
            .files
            .iter()
            .flat_map(|file| &file.matches)
            .filter(|hit| super::guidance::source::is_noise_line(&hit.line))
            .count() as u64
    } else {
        0
    };
    let rows = if grouping == TextGrouping::Names {
        let mut names = Vec::new();
        'files: for file in &found.files {
            for hit in &file.matches {
                if let Some(symbol) = &hit.enclosing_symbol
                    && !names.contains(&symbol.name)
                {
                    if !budget.row(symbol.name.len()) {
                        break 'files;
                    }
                    names.push(symbol.name.clone());
                }
            }
        }
        TextRows::Names(names)
    } else if grouping == TextGrouping::Symbol && !context_mode {
        let mut counts = HashMap::new();
        for hits in &found.files {
            budget.check()?;
            let file = live
                .get_file(&hits.path)
                .ok_or(QueryRefusalKind::InvalidSpan)?;
            for hit in &hits.matches {
                if let Some(index) = crate::domain::find_enclosing_symbol(
                    &file.symbols,
                    (hit.line_number as u32).saturating_sub(1),
                ) {
                    *counts.entry((hits.path.as_str(), index)).or_insert(0u64) += 1;
                }
            }
        }
        let mut groups = Vec::new();
        let mut seen = BTreeSet::new();
        let mut top_level = Vec::new();
        'files: for hits in &found.files {
            let file = live
                .get_file(&hits.path)
                .ok_or(QueryRefusalKind::InvalidSpan)?;
            for hit in &hits.matches {
                budget.check()?;
                if let Some(index) = crate::domain::find_enclosing_symbol(
                    &file.symbols,
                    (hit.line_number as u32).saturating_sub(1),
                ) {
                    if !seen.insert((hits.path.as_str(), index)) {
                        continue;
                    }
                    let symbol = symbol_row(file, index as usize, &file.symbols[index as usize])?;
                    if !budget.row(symbol_size(&symbol) + hit.line.len()) {
                        break 'files;
                    }
                    let match_count = counts[&(hits.path.as_str(), index)];
                    groups.push(TextSymbolGroup {
                        symbol,
                        match_count,
                        preview: hit.line.clone(),
                    });
                } else {
                    let Some(hit) = text_line(
                        live,
                        &hits.path,
                        hit.line_number as u32,
                        hit.line.clone(),
                        budget,
                    )?
                    else {
                        break 'files;
                    };
                    top_level.push(hit);
                }
            }
        }
        TextRows::Symbols { groups, top_level }
    } else {
        let mut files = Vec::new();
        for hits in &found.files {
            if !budget.row(hits.path.len()) {
                break;
            }
            let mut matches = Vec::new();
            for hit in &hits.matches {
                if usage && super::guidance::source::is_noise_line(&hit.line) {
                    continue;
                }
                let Some(hit) = text_line(
                    live,
                    &hits.path,
                    hit.line_number as u32,
                    hit.line.clone(),
                    budget,
                )?
                else {
                    break;
                };
                matches.push(hit);
            }
            let context = if let Some(lines) = &hits.rendered_lines {
                let mut context = Vec::new();
                for line in lines {
                    match line {
                        engine::TextDisplayLine::Separator => {
                            if !budget.row("Separator".len()) {
                                break;
                            }
                            context.push(ContextLine::Separator);
                        }
                        engine::TextDisplayLine::Line(line) => {
                            if !budget.row(line.line.len()) {
                                break;
                            }
                            context.push(ContextLine::Line {
                                line: line.line_number as u64,
                                text: line.line.clone(),
                                is_match: line.is_match,
                            });
                        }
                    }
                }
                Some(context)
            } else {
                None
            };
            let callers = if let Some(callers) = &hits.callers {
                let mut projected = Vec::new();
                for caller in callers {
                    if !budget.row(caller.file.len() + caller.symbol.len()) {
                        break;
                    }
                    projected.push(TextCaller {
                        path: caller.file.clone(),
                        symbol: caller.symbol.clone(),
                        line: caller.line,
                    });
                }
                Some(projected)
            } else {
                None
            };
            files.push(TextFile {
                path: hits.path.clone(),
                matches,
                context,
                callers,
            });
        }
        TextRows::Files(files)
    };
    let untracked_diagnostic = super::guidance::search::untracked_file_diagnostic(untracked_paths);
    if let Some(diagnostic) = &untracked_diagnostic {
        budget
            .required(untracked_paths.iter().map(String::len).sum::<usize>() + diagnostic.len())?;
    }
    Ok(QueryOutput::TextSearch(TextSearchResult {
        rows,
        total_matches: found.total_matches as u64,
        overflow_count: found.overflow_count as u64,
        suppressed_by_noise: found.suppressed_by_noise as u64,
        excluded_by_usage,
        excluded_knowledge_files: found.excluded_knowledge_files as u64,
        withheld_policy_files: found.withheld_policy_files as u64,
        withheld_size_files: found.withheld_size_files as u64,
        regex: executed.is_regex,
        structural: executed.structural,
        ranked: executed.options.ranked,
        auto_detected_regex: executed.auto_detected_regex,
        auto_corrected_regex: executed.auto_corrected_regex,
        untracked_paths: untracked_paths.to_vec(),
        untracked_diagnostic,
    }))
}
