//! Shared indexed search orchestration above transport-independent search.

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
