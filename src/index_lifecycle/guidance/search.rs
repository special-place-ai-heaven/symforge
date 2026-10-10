//! Shared indexed search orchestration above transport-independent search.

use std::path::Path;

use crate::domain::{FileClassification, LanguageId};
use crate::live_index::{LiveIndex, search};

fn enrich_with_callers(
    index: &crate::live_index::LiveIndex,
    result: &mut search::TextSearchResult,
    file_limit: usize,
) {
    use std::collections::HashSet;

    for file_matches in result.files.iter_mut().take(file_limit) {
        // Collect unique enclosing symbol names from this file's matches
        let mut symbol_names: HashSet<String> = HashSet::new();
        for m in &file_matches.matches {
            if let Some(ref enc) = m.enclosing_symbol {
                symbol_names.insert(enc.name.clone());
            }
        }

        if symbol_names.is_empty() {
            continue;
        }

        let mut callers: Vec<search::CallerEntry> = Vec::new();

        for sym_name in &symbol_names {
            let refs = index.find_references_for_name(sym_name, None, false);
            for (ref_file, ref_record) in refs {
                // Get enclosing symbol of the reference
                let enclosing_name = ref_record
                    .enclosing_symbol_index
                    .and_then(|idx| {
                        index
                            .get_file(ref_file)
                            .and_then(|f| f.symbols.get(idx as usize))
                            .map(|s| s.name.clone())
                    })
                    .unwrap_or_else(|| "(top-level)".to_string());

                // Skip self-references only when the caller IS one of the matched
                // symbols (same-file callers from different symbols are useful context)
                if ref_file == file_matches.path && symbol_names.contains(&enclosing_name) {
                    continue;
                }

                callers.push(search::CallerEntry {
                    file: ref_file.to_string(),
                    symbol: enclosing_name,
                    line: ref_record.line_range.0 + 1, // 0-based to 1-based
                });
            }
        }

        // Choose the earliest reference per caller before applying the cap.
        // Symbol-name and reverse-index iteration order must not select rows.
        callers.sort_unstable_by(|left, right| {
            left.file
                .cmp(&right.file)
                .then_with(|| left.symbol.cmp(&right.symbol))
                .then_with(|| left.line.cmp(&right.line))
        });
        callers.dedup_by(|left, right| left.file == right.file && left.symbol == right.symbol);
        callers.truncate(10);

        // Always set callers when follow_refs was requested — distinguishes
        // "not requested" (None) from "ran but found nothing" (Some([]))
        file_matches.callers = Some(callers);
    }
}

/// Strip vendor / personal-tooling paths from a `TextSearchResult` per the
/// caller's `include_vendor` / `include_personal_tooling` flags. Suppressed
/// match counts feed `suppressed_by_noise`, which renders into the existing
/// "N noise-filtered match(es) suppressed" envelope footer.
fn apply_path_predicate_filter(
    result: &mut Result<search::TextSearchResult, search::TextSearchError>,
    include_vendor: bool,
    include_personal_tooling: bool,
) {
    if include_vendor && include_personal_tooling {
        return;
    }
    if let Ok(r) = result {
        let mut filtered: usize = 0;
        r.files.retain(|file| {
            if !include_vendor && crate::live_index::query::is_vendor_path(&file.path) {
                filtered += file.matches.len();
                return false;
            }
            if !include_personal_tooling
                && crate::live_index::query::is_personal_tooling_path(&file.path)
            {
                filtered += file.matches.len();
                return false;
            }
            true
        });
        r.suppressed_by_noise = r.suppressed_by_noise.saturating_add(filtered);
    }
}

fn search_text_compaction_query(query: Option<&str>, terms: Option<&[String]>) -> String {
    if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
        return q.to_string();
    }
    terms
        .map(|items| {
            items
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn maybe_compact_text_search_result(
    result: &mut Result<search::TextSearchResult, search::TextSearchError>,
    query: &str,
) {
    if query.trim().is_empty() {
        return;
    }
    if let Ok(text) = result {
        super::compression::compact_text_search_result(text, query);
    }
}

fn resolve_text_search_enclosing_symbols(
    index: &LiveIndex,
    result: &mut Result<search::TextSearchResult, search::TextSearchError>,
) {
    let Ok(result) = result else {
        return;
    };

    for file_matches in &mut result.files {
        let Some(file) = index.get_file(&file_matches.path) else {
            continue;
        };

        for line_match in &mut file_matches.matches {
            let zero_based_line = line_match.line_number.saturating_sub(1) as u32;
            line_match.enclosing_symbol =
                crate::domain::find_enclosing_symbol(&file.symbols, zero_based_line)
                    .and_then(|idx| file.symbols.get(idx as usize))
                    .map(|symbol| search::EnclosingMatchSymbol {
                        name: symbol.name.clone(),
                        kind: symbol.kind.to_string(),
                        line_range: symbol.line_range,
                    });
        }
    }
}

fn search_symbols_total_matches(result: &search::SymbolSearchResult) -> usize {
    result.hits.len().saturating_add(result.overflow_count)
}

fn hidden_search_symbols_noise_count(
    index: &LiveIndex,
    query: &str,
    kind: Option<&str>,
    options: &search::SymbolSearchOptions,
    visible: &search::SymbolSearchResult,
) -> usize {
    if options.noise_policy.include_vendor && options.include_personal_tooling {
        return 0;
    }

    let mut unfiltered_options = options.clone();
    unfiltered_options.noise_policy.include_vendor = true;
    unfiltered_options.include_personal_tooling = true;
    let unfiltered = search::search_symbols_with_options(index, query, kind, &unfiltered_options);
    search_symbols_total_matches(&unfiltered).saturating_sub(search_symbols_total_matches(visible))
}

pub(crate) struct TextSearchExecution {
    pub result: Result<search::TextSearchResult, search::TextSearchError>,
    pub options: search::TextSearchOptions,
    pub is_regex: bool,
    pub structural: bool,
    pub auto_detected_regex: bool,
    pub auto_corrected_regex: bool,
    pub effective_query: Option<String>,
}

/// Shared request semantics above the indexed text/AST search engine. Transport
/// presentation, admission and session accounting stay with the caller.
pub(crate) fn execute_text_search(
    generation: &crate::live_index::PublishedGeneration,
    input: &super::search_contract::SearchTextInput,
) -> Result<TextSearchExecution, String> {
    use super::search_contract::search_text_options_from_input;
    let index = &generation.live;
    let mut options = search_text_options_from_input(input)?;
    if input.structural.unwrap_or(false) {
        let pattern = input
            .query
            .as_deref()
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .ok_or_else(|| "Error: `query` is required for structural search.".to_owned())?;
        let mut result = search::search_structural(index, pattern, &options);
        resolve_text_search_enclosing_symbols(index, &mut result);
        if input.follow_refs.unwrap_or(false)
            && let Ok(result) = &mut result
        {
            enrich_with_callers(index, result, input.follow_refs_limit.unwrap_or(3) as usize);
        }
        apply_path_predicate_filter(
            &mut result,
            input.include_vendor.unwrap_or(false),
            input.include_personal_tooling.unwrap_or(false),
        );
        maybe_compact_text_search_result(&mut result, pattern);
        return Ok(TextSearchExecution {
            result,
            options,
            is_regex: false,
            structural: true,
            auto_detected_regex: false,
            auto_corrected_regex: false,
            effective_query: Some(pattern.to_owned()),
        });
    }
    let mut is_regex = input.regex.unwrap_or(false);
    let mut auto_detected_regex = false;
    if !is_regex
        && let Some(query) = &input.query
        && ["\\w", "\\d", "\\s", "\\b", "\\W", "\\D", "\\S"]
            .iter()
            .any(|escape| query.contains(escape))
    {
        is_regex = true;
        auto_detected_regex = true;
        if input.include_tests.is_none() {
            options.noise_policy.include_tests = true;
        }
    }
    if options.ranked {
        let temporal = &generation.code_signals.temporal;
        if matches!(
            temporal.state,
            crate::live_index::git_temporal::GitTemporalState::Ready
        ) {
            let scores: std::collections::HashMap<String, f32> = temporal
                .files
                .iter()
                .map(|(path, history)| (path.clone(), history.churn_score))
                .collect();
            if !scores.is_empty() {
                options.churn_scores = Some(scores);
            }
        }
    }
    let run = |query: Option<&str>| {
        let mut result = search::search_text_with_options(
            index,
            query,
            input.terms.as_deref(),
            is_regex,
            &options,
        );
        resolve_text_search_enclosing_symbols(index, &mut result);
        if input.follow_refs.unwrap_or(false)
            && let Ok(result) = &mut result
        {
            enrich_with_callers(index, result, input.follow_refs_limit.unwrap_or(3) as usize);
        }
        apply_path_predicate_filter(
            &mut result,
            input.include_vendor.unwrap_or(false),
            input.include_personal_tooling.unwrap_or(false),
        );
        result
    };
    let mut result = run(input.query.as_deref());
    let should_retry = matches!(&result, Err(search::TextSearchError::InvalidRegex { .. }))
        || matches!(&result, Ok(result) if result.files.is_empty());
    if is_regex
        && should_retry
        && let Some(query) = &input.query
        && let Some(fixed) = super::filters::fix_common_double_escapes(query)
    {
        let retry = run(Some(&fixed));
        if matches!(&retry, Ok(result) if !result.files.is_empty()) {
            return Ok(TextSearchExecution {
                result: retry,
                options,
                is_regex,
                structural: false,
                auto_detected_regex,
                auto_corrected_regex: true,
                effective_query: Some(fixed),
            });
        }
    }
    let compaction = search_text_compaction_query(input.query.as_deref(), input.terms.as_deref());
    maybe_compact_text_search_result(&mut result, &compaction);
    Ok(TextSearchExecution {
        result,
        options,
        is_regex,
        structural: false,
        auto_detected_regex,
        auto_corrected_regex: false,
        effective_query: input.query.clone(),
    })
}

pub(crate) struct SymbolSearchExecution {
    pub result: search::SymbolSearchResult,
    pub options: search::SymbolSearchOptions,
    pub suppressed_by_noise: usize,
    pub text_fallback: Vec<(String, usize)>,
}

pub(crate) fn execute_symbol_search(
    index: &LiveIndex,
    input: &super::search_contract::SearchSymbolsInput,
) -> Result<SymbolSearchExecution, String> {
    let query = input.query.as_deref().unwrap_or("").trim();
    let is_browse = query.is_empty();
    if is_browse && input.kind.is_none() && input.path_prefix.is_none() {
        return Err("search_symbols requires at least one of: query, kind, or path_prefix".into());
    }
    let options = super::search_contract::search_symbols_options_from_input(input)?;
    let result = search::search_symbols_with_options(index, query, input.kind.as_deref(), &options);
    let suppressed_by_noise =
        hidden_search_symbols_noise_count(index, query, input.kind.as_deref(), &options, &result);
    let mut text_fallback = Vec::new();
    if !is_browse && query.len() >= 2 && result.hits.len() < options.result_limit.get() / 2 {
        let mut text_options = search::TextSearchOptions::for_current_code_search();
        text_options.path_scope = options.path_scope.clone();
        text_options.noise_policy = options.noise_policy;
        text_options.include_personal_tooling = options.include_personal_tooling;
        text_options.language_filter = options.language_filter;
        text_options.total_limit = 15;
        text_options.max_per_file = 1;
        if let Ok(text) =
            search::search_text_with_options(index, Some(query), None, false, &text_options)
        {
            let paths: std::collections::HashSet<&str> =
                result.hits.iter().map(|hit| hit.path.as_str()).collect();
            text_fallback = text
                .files
                .into_iter()
                .filter(|file| !paths.contains(file.path.as_str()))
                .filter_map(|file| {
                    file.matches
                        .first()
                        .map(|hit| (file.path.clone(), hit.line_number))
                })
                .take(10)
                .collect();
        }
    }
    Ok(SymbolSearchExecution {
        result,
        options,
        suppressed_by_noise,
        text_fallback,
    })
}

fn language_for_path(path: &str) -> Option<LanguageId> {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .and_then(|extension| LanguageId::from_extension(&extension))
}

/// The untracked paths `live` does not know.
///
/// `live` is the caller's CAPTURED publication — the same bundle that produced
/// the response beside this verdict — so "not in the index" cannot disagree
/// with the rows the receipt names. Taking a `&LiveIndex` rather than the
/// server is what enforces that: this function has no route to
/// `SharedIndexHandle`, so a second, later read is a compile error rather than
/// something a reviewer has to catch.
pub(crate) fn untracked_paths_not_in_index(
    repo: &crate::git::GitRepo,
    live: &LiveIndex,
) -> Vec<String> {
    let Ok(mut paths) = repo.untracked_paths() else {
        return Vec::new();
    };

    paths.retain(|path| live.get_file(path).is_none());
    paths.sort();
    paths.dedup();
    paths
}

pub(crate) fn untracked_file_diagnostic(paths: &[String]) -> Option<String> {
    let first = paths.first()?;
    Some(format!(
        "untracked file may match: {} untracked path(s) are not indexed. To index the first match, call analyze_file_impact(\"{}\", new_file=true).",
        paths.len(),
        first
    ))
}

pub(crate) fn append_untracked_file_diagnostic(output: &mut String, paths: &[String]) {
    if let Some(diagnostic) = untracked_file_diagnostic(paths) {
        output.push_str("\n\n");
        output.push_str(&diagnostic);
    }
}

fn untracked_text_path_allowed(path: &str, options: &search::TextSearchOptions) -> bool {
    let classification = FileClassification::for_code_path(path);
    options.path_scope.matches(path)
        && options.search_scope.allows(&classification)
        && options.noise_policy.allows(&classification)
        && (options.noise_policy.include_vendor || !crate::live_index::query::is_vendor_path(path))
        && (options.include_personal_tooling
            || !crate::live_index::query::is_personal_tooling_path(path))
        && options
            .language_filter
            .as_ref()
            .is_none_or(|language| language_for_path(path).as_ref() == Some(language))
        && untracked_text_globs_allow(path, options)
}

fn untracked_text_globs_allow(path: &str, options: &search::TextSearchOptions) -> bool {
    let include_matches = match options.glob.as_deref() {
        Some(pattern) => globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
            .map(|glob| glob.compile_matcher().is_match(path))
            .unwrap_or(false),
        None => true,
    };
    let exclude_matches = match options.exclude_glob.as_deref() {
        Some(pattern) => globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
            .map(|glob| glob.compile_matcher().is_match(path))
            .unwrap_or(false),
        None => false,
    };
    include_matches && !exclude_matches
}

fn whole_word_contains(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(idx, matched)| {
        let before_is_word = haystack[..idx]
            .chars()
            .next_back()
            .is_some_and(|ch| ch == '_' || ch.is_alphanumeric());
        let after_idx = idx + matched.len();
        let after_is_word = haystack[after_idx..]
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_alphanumeric());
        !before_is_word && !after_is_word
    })
}

fn untracked_text_matches(
    content: &str,
    query: Option<&str>,
    terms: Option<&[String]>,
    is_regex: bool,
    options: &search::TextSearchOptions,
) -> bool {
    let case_sensitive = options.case_sensitive.unwrap_or(is_regex);
    if is_regex {
        let Some(pattern) = query.map(str::trim).filter(|pattern| !pattern.is_empty()) else {
            return false;
        };
        return regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .map(|regex| regex.is_match(content))
            .unwrap_or(false);
    }

    let normalized_terms: Vec<&str> = match terms {
        Some(raw_terms) if !raw_terms.is_empty() => raw_terms
            .iter()
            .map(|term| term.trim())
            .filter(|term| !term.is_empty())
            .collect(),
        _ => query
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| vec![text])
            .unwrap_or_default(),
    };
    if normalized_terms.is_empty() {
        return false;
    }

    if case_sensitive {
        if options.whole_word {
            normalized_terms
                .iter()
                .any(|term| whole_word_contains(content, term))
        } else {
            normalized_terms.iter().any(|term| content.contains(term))
        }
    } else {
        let lowered = content.to_lowercase();
        normalized_terms.iter().any(|term| {
            let lowered_term = term.to_lowercase();
            if options.whole_word {
                whole_word_contains(&lowered, &lowered_term)
            } else {
                lowered.contains(&lowered_term)
            }
        })
    }
}

/// The untracked paths a zero-hit text search could have missed: unknown to
/// `live`, inside the search's own filters, and matching the query over the
/// bytes `admit` returns. `admit` is the caller's gated working-tree read; it
/// runs BEFORE any matching, because the caller's own regex is evaluated
/// against this content and an anchored pattern would otherwise recover a
/// refused file character by character from which paths come back. A refusal
/// drops the path, disclosing neither content nor existence-by-match.
pub(crate) fn matching_untracked_paths_for_search_text(
    repo: &crate::git::GitRepo,
    live: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    is_regex: bool,
    options: &search::TextSearchOptions,
    admit: &mut dyn FnMut(&str) -> Result<Option<String>, String>,
) -> Vec<String> {
    untracked_paths_not_in_index(repo, live)
        .into_iter()
        .filter(|path| untracked_text_path_allowed(path, options))
        .filter(|path| {
            admit(path).ok().flatten().is_some_and(|content| {
                untracked_text_matches(&content, query, terms, is_regex, options)
            })
        })
        .collect()
}

pub(crate) fn untracked_path_has_component(path: &str, component: &str) -> bool {
    path.split('/')
        .any(|part| part.eq_ignore_ascii_case(component))
}

pub(crate) fn untracked_common_path_filters_allow(
    path: &str,
    include_vendor: bool,
    include_personal_tooling: bool,
) -> bool {
    (include_vendor || !crate::live_index::query::is_vendor_path(path))
        && (include_personal_tooling || !crate::live_index::query::is_personal_tooling_path(path))
}

pub(crate) fn untracked_path_matches_search_files_query(path: &str, query: &str) -> bool {
    let normalized_query = normalize_untracked_search_path(query);
    if normalized_query.is_empty() {
        return false;
    }

    let is_glob = normalized_query.contains('*')
        || normalized_query.contains('?')
        || normalized_query.contains('[');
    if is_glob
        && let Ok(glob) = globset::GlobBuilder::new(&normalized_query)
            .literal_separator(false)
            .build()
    {
        return glob.compile_matcher().is_match(path);
    }

    let path_lower = path.to_ascii_lowercase();
    let normalized_query_lower = normalized_query.to_ascii_lowercase();
    let has_path_context = normalized_query.contains('/');
    if path_lower == normalized_query_lower
        || (has_path_context && path_lower.ends_with(&normalized_query_lower))
    {
        return true;
    }

    let tokens: Vec<String> = normalized_query
        .split(|ch: char| ch == '/' || ch.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let Some(basename_token) = tokens.last() else {
        return false;
    };
    let component_tokens = if tokens.len() > 1 {
        &tokens[..tokens.len() - 1]
    } else {
        &[][..]
    };
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if file_name.eq_ignore_ascii_case(basename_token)
        && component_tokens
            .iter()
            .all(|component| untracked_path_has_component(path, component))
    {
        return true;
    }

    if basename_token.len() >= 3 {
        let file_stem = Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        if file_stem.to_ascii_lowercase().starts_with(basename_token) {
            return true;
        }
    }

    tokens.iter().all(|token| path_lower.contains(token))
}

/// MCP `search_text`'s rendered answer over one publication: the result
/// envelope, the grouped view and the zero-hit untracked diagnostic for
/// `matching_untracked_paths`, which the caller swept from its own git root.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_search_text_output(
    // T046: the caller's entry capture — banner and parse-state come off the
    // SAME publication that produced the rows, not fresh loads taken here.
    generation: &crate::live_index::store::PublishedGeneration,
    result: Result<search::TextSearchResult, search::TextSearchError>,
    query: Option<&str>,
    structural: bool,
    group_by: Option<&str>,
    terms: Option<&[String]>,
    options: &search::TextSearchOptions,
    is_regex: bool,
    auto_detected_regex: bool,
    auto_corrected_regex: bool,
    matching_untracked_paths: &[String],
) -> String {
    use super::search_envelope::{
        SourceAuthority, format_search_envelope, search_completeness_label, search_scope_summary,
        search_text_evidence, search_text_match_type_label,
    };
    let envelope = match &result {
        Ok(result) if !result.files.is_empty() => Some(format_search_envelope(
            &search_text_match_type_label(
                structural,
                is_regex,
                terms,
                auto_detected_regex,
                auto_corrected_regex,
                options.ranked,
            ),
            SourceAuthority::from_freshness(&generation.freshness),
            super::reference_read::search_parse_state_for_paths(
                &generation.live,
                result.files.iter().map(|file| file.path.as_str()),
            ),
            &search_completeness_label(result.overflow_count, result.suppressed_by_noise),
            &search_scope_summary(
                &options.path_scope,
                options.language_filter.as_ref(),
                &options.noise_policy,
                options.include_personal_tooling,
                options.glob.as_deref(),
                options.exclude_glob.as_deref(),
                options.ranked,
            ),
            &search_text_evidence(result),
        )),
        _ => None,
    };
    let confidence = if auto_corrected_regex {
        0.75f32
    } else if is_regex && auto_detected_regex {
        0.80
    } else if is_regex {
        0.85
    } else if options.ranked {
        0.80
    } else {
        0.95
    };
    let output = super::search_render::search_text_result_view(
        result,
        group_by,
        terms,
        Some(confidence),
        super::search_render::SearchSuggestionContext {
            regex: is_regex,
            include_tests: options.noise_policy.include_tests,
            multi_word_literal: !is_regex
                && query.is_some_and(|q| q.trim().contains(char::is_whitespace)),
        },
    );
    let mut rendered = match envelope {
        Some(envelope) => format!("{envelope}\n\n{output}"),
        None => output,
    };
    append_untracked_file_diagnostic(&mut rendered, matching_untracked_paths);
    rendered
}

/// MCP `search_symbols`' rendered answer over one publication: the result
/// envelope, the tiered view and the sparse-hit text fallback.
pub(crate) fn render_symbol_search(
    generation: &crate::live_index::store::PublishedGeneration,
    execution: &SymbolSearchExecution,
    query: &str,
) -> String {
    use super::search_envelope::{
        SourceAuthority, format_search_envelope, search_completeness_label, search_scope_summary,
        search_symbols_evidence, search_symbols_match_type_label,
    };
    let result = &execution.result;
    let options = &execution.options;
    let is_browse = query.is_empty();
    // Browse ordering is owned by the engine (search::search_symbols_with_options),
    // which ranks browse results by importance (reference count -> kind -> path ->
    // line). Do NOT re-sort here: a tool-level re-sort would override that order and
    // reintroduce the symbol_kind_priority display-kind mismatch ("fn" -> 0.1). (018 US2)
    let envelope = if result.hits.is_empty() {
        None
    } else {
        Some(format_search_envelope(
            search_symbols_match_type_label(result, is_browse),
            SourceAuthority::from_freshness(&generation.freshness),
            super::reference_read::search_parse_state_for_paths(
                &generation.live,
                result.hits.iter().map(|hit| hit.path.as_str()),
            ),
            &search_completeness_label(result.overflow_count, execution.suppressed_by_noise),
            &search_scope_summary(
                &options.path_scope,
                options.language_filter.as_ref(),
                &options.noise_policy,
                options.include_personal_tooling,
                None,
                None,
                false,
            ),
            &search_symbols_evidence(result),
        ))
    };
    let output = super::search_render::search_symbols_result_view(result, query);
    let output = match envelope {
        Some(envelope) => format!("{envelope}\n\n{output}"),
        None => output,
    };
    if execution.text_fallback.is_empty() {
        output
    } else {
        let paths: Vec<_> = execution
            .text_fallback
            .iter()
            .map(|(path, line)| format!("{path}:{line}"))
            .collect();
        format!(
            "{output}\n\nText path fallback (sparse symbol hits):\n{}",
            paths.join("\n")
        )
    }
}

/// MCP `search_files`' zero-hit diagnostic: untracked worktree paths the
/// publication does not know whose names match `query`.
pub(crate) fn matching_untracked_paths_for_search_files(
    repo: &crate::git::GitRepo,
    live: &LiveIndex,
    query: &str,
    include_vendor: bool,
    include_personal_tooling: bool,
) -> Vec<String> {
    untracked_paths_not_in_index(repo, live)
        .into_iter()
        .filter(|path| {
            untracked_common_path_filters_allow(path, include_vendor, include_personal_tooling)
                && untracked_path_matches_search_files_query(path, query)
        })
        .collect()
}

pub(crate) fn normalize_untracked_search_path(raw: &str) -> String {
    let mut normalized = raw.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized.trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caller_enrichment_selects_stable_rows_and_earliest_reference_before_cap() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("targets.rs"),
            "pub fn zebra() {}\npub fn alpha() {}\n",
        )
        .unwrap();
        for name in ["z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            std::fs::write(
                root.path().join(format!("{name}.rs")),
                format!("fn caller_{name}() {{\n zebra();\n alpha();\n}}\n"),
            )
            .unwrap();
        }
        let shared = crate::live_index::LiveIndex::load(root.path()).unwrap();
        let index = shared.read();
        let result = search::search_text_with_options(
            &index,
            Some("pub fn"),
            None,
            false,
            &search::TextSearchOptions {
                path_scope: search::PathScope::exact("targets.rs"),
                max_per_file: 20,
                ..search::TextSearchOptions::default()
            },
        )
        .unwrap();
        for _ in 0..64 {
            let mut result = Ok(result.clone());
            resolve_text_search_enclosing_symbols(&index, &mut result);
            let mut result = result.unwrap();
            enrich_with_callers(&index, &mut result, 1);
            let callers = result.files[0].callers.as_ref().unwrap();
            let paths: Vec<_> = callers.iter().map(|caller| caller.file.as_str()).collect();
            assert_eq!(
                paths,
                vec![
                    "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs", "i.rs", "j.rs"
                ]
            );
            assert!(callers.iter().all(|caller| caller.line == 2));
        }
    }
}
