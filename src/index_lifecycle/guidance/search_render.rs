//! Shared search presentation for protocol and native routed queries.

pub(crate) type ExploreEnrichedSymbol = (String, String, String, Option<String>, Vec<String>);

/// Format the output of the `explore` tool.
pub struct ExploreResultViewInput<'a> {
    pub label: &'a str,
    pub symbol_hits: &'a [(String, String, String)],
    pub text_hits: &'a [(String, String, usize)],
    pub related_files: &'a [(String, usize)],
    pub enriched_symbols: &'a [ExploreEnrichedSymbol],
    pub symbol_impls: &'a [(String, Vec<String>)],
    pub symbol_deps: &'a [(String, Vec<String>)],
    pub derived_seed_terms: &'a [String],
    pub derived_symbols: &'a [String],
    pub derived_seed_files: &'a [String],
    pub enriched_imports: &'a [String],
    pub symbol_scores: &'a [f32],
    pub depth: u32,
}

const EXPLORE_STRONG_CONTEXT_SCORE: f32 = 0.80;

fn explore_symbol_score(symbol_scores: &[f32], index: usize) -> Option<f32> {
    let score = *symbol_scores.get(index)?;
    score.is_finite().then(|| score.clamp(0.0, 1.0))
}

fn should_show_explore_symbol(name: &str, score: Option<f32>) -> bool {
    !is_low_signal_explore_symbol_name(name)
        || score.unwrap_or_default() >= EXPLORE_STRONG_CONTEXT_SCORE
}

fn visible_explore_symbol_count(
    symbol_hits: &[(String, String, String)],
    symbol_scores: &[f32],
) -> usize {
    symbol_hits
        .iter()
        .enumerate()
        .filter(|(index, (name, _, _))| {
            should_show_explore_symbol(name, explore_symbol_score(symbol_scores, *index))
        })
        .count()
}

fn is_low_signal_explore_symbol_name(name: &str) -> bool {
    const LOW_SIGNAL_SYMBOL_NAMES: &[&str] = &[
        "as_mut",
        "as_ref",
        "bool",
        "build",
        "clone",
        "collect",
        "create",
        "default",
        "err",
        "expect",
        "from",
        "get",
        "handle",
        "i32",
        "i64",
        "init",
        "into",
        "into_iter",
        "iter",
        "iter_mut",
        "len",
        "main",
        "map",
        "new",
        "none",
        "ok",
        "option",
        "process",
        "result",
        "run",
        "set",
        "some",
        "str",
        "string",
        "to_owned",
        "to_string",
        "u32",
        "u64",
        "unwrap",
        "update",
        "usize",
        "vec",
    ];

    name.len() <= 2 || LOW_SIGNAL_SYMBOL_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

fn explore_symbol_score_suffix(score: Option<f32>) -> String {
    let reason = explore_symbol_reason(score);
    match score {
        Some(score) => format!("  [{score:.2}; {reason}]"),
        None => format!("  [{reason}]"),
    }
}

fn explore_symbol_reason(score: Option<f32>) -> &'static str {
    match score {
        Some(score) if score >= EXPLORE_STRONG_CONTEXT_SCORE => "reason: strong match",
        Some(score) if score >= 0.45 => "reason: contextual match",
        Some(_) => "reason: query match",
        None => "reason: matched query",
    }
}

fn is_low_signal_explore_text_hit(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with("# TODO")
        || trimmed.starts_with("# FIXME")
        || trimmed.starts_with("# NOTE")
        || trimmed.starts_with("# HACK")
    {
        return false;
    }
    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("/**")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("//")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("*/")
        || trimmed.starts_with("# ")
        || trimmed == "#"
        || trimmed.starts_with("--")
}

pub fn explore_result_view(input: ExploreResultViewInput<'_>) -> String {
    let ExploreResultViewInput {
        label,
        symbol_hits,
        text_hits,
        related_files,
        enriched_symbols,
        symbol_impls,
        symbol_deps,
        derived_seed_terms,
        derived_symbols,
        enriched_imports,
        derived_seed_files,
        symbol_scores,
        depth,
    } = input;

    let mut lines = vec![format!("── Exploring: {label} ──")];
    if !enriched_imports.is_empty() {
        lines.push(format!(
            "Enriched with project imports: {}",
            enriched_imports.join(", ")
        ));
    }
    lines.push(String::new());

    if !derived_symbols.is_empty() || !derived_seed_files.is_empty() {
        lines.push("Auto-derived cluster:".to_string());
        if !derived_seed_terms.is_empty() {
            lines.push(format!("  Seed terms: {}", derived_seed_terms.join(", ")));
        }
        if !derived_symbols.is_empty() {
            lines.push(format!(
                "  Promoted signals: {}",
                derived_symbols.join(", ")
            ));
        }
        if !derived_seed_files.is_empty() {
            lines.push(format!("  Seed files: {}", derived_seed_files.join(", ")));
        }
        lines.push(String::new());
    }

    let visible_symbol_count = visible_explore_symbol_count(symbol_hits, symbol_scores);
    if visible_symbol_count > 0 {
        lines.push(format!("Symbols ({visible_symbol_count} found):"));
        if depth >= 2 && !enriched_symbols.is_empty() {
            // Depth 2+: show enriched symbols with signatures.
            for (i, (name, kind, path, signature, dependents)) in
                enriched_symbols.iter().enumerate()
            {
                let score = explore_symbol_score(symbol_scores, i);
                if !should_show_explore_symbol(name, score) {
                    continue;
                }
                let score_suffix = explore_symbol_score_suffix(score);
                if let Some(sig) = signature {
                    // Show first line of signature only to keep it compact.
                    let first_line = sig.lines().next().unwrap_or(sig);
                    lines.push(format!("  {first_line}  [{kind}, {path}]{score_suffix}"));
                } else {
                    lines.push(format!("  {kind} {name}  {path}{score_suffix}"));
                }
                if !dependents.is_empty() {
                    lines.push(format!("    <- used by: {}", dependents.join(", ")));
                }
            }
            // Show remaining non-enriched symbols in compact form.
            if symbol_hits.len() > enriched_symbols.len() {
                for (i, (name, kind, path)) in
                    symbol_hits[enriched_symbols.len()..].iter().enumerate()
                {
                    let original_index = enriched_symbols.len() + i;
                    let score = explore_symbol_score(symbol_scores, original_index);
                    if !should_show_explore_symbol(name, score) {
                        continue;
                    }
                    let score_suffix = explore_symbol_score_suffix(score);
                    lines.push(format!("  {kind} {name}  {path}{score_suffix}"));
                }
            }
        } else {
            // Depth 1: compact symbol list.
            for (i, (name, kind, path)) in symbol_hits.iter().enumerate() {
                let score = explore_symbol_score(symbol_scores, i);
                if !should_show_explore_symbol(name, score) {
                    continue;
                }
                let score_suffix = explore_symbol_score_suffix(score);
                lines.push(format!("  {kind} {name}  {path}{score_suffix}"));
            }
        }
        lines.push(String::new());
    }

    // Depth 3: implementations + type dependencies
    if depth >= 3 && symbol_impls.is_empty() && symbol_deps.is_empty() {
        lines.push("No implementations or type dependencies found for top symbols.".to_string());
        lines.push(String::new());
    }
    if depth >= 3 && !symbol_impls.is_empty() {
        lines.push("Implementations:".to_string());
        for (name, impls) in symbol_impls {
            lines.push(format!("  {name}:"));
            for imp in impls {
                lines.push(format!("    -> {imp}"));
            }
        }
        lines.push(String::new());
    }

    if depth >= 3 && !symbol_deps.is_empty() {
        lines.push("Type dependencies:".to_string());
        for (name, deps) in symbol_deps {
            lines.push(format!("  {name}:"));
            for dep in deps {
                lines.push(format!("    -> {dep}"));
            }
        }
        lines.push(String::new());
    }

    let visible_text_hits: Vec<_> = text_hits
        .iter()
        .filter(|(_, line, _)| !is_low_signal_explore_text_hit(line))
        .collect();
    if !visible_text_hits.is_empty() {
        lines.push(format!(
            "Code patterns ({} found):",
            visible_text_hits.len()
        ));
        let mut last_path: Option<&str> = None;
        for (path, line, line_number) in &visible_text_hits {
            if last_path != Some(path.as_str()) {
                lines.push(format!("  {path}"));
                last_path = Some(path.as_str());
            }
            lines.push(format!("    > {line_number}: {line}"));
        }
        lines.push(String::new());
    }

    if !related_files.is_empty() {
        lines.push("Related files:".to_string());
        for (path, count) in related_files {
            lines.push(format!("  {path}  ({count} matches)"));
        }
    }

    if visible_symbol_count == 0 && visible_text_hits.is_empty() {
        lines.push("No matches found for this concept. Try rephrasing with different terms, use search_text(query=...) for a literal or regex match, broaden with include_noise=true to include vendor/generated files, or run health to confirm the index is populated.".to_string());
    }

    lines.join("\n")
}

use super::source::is_noise_line;
use crate::live_index::{SearchFilesResolveView, SearchFilesTier, SearchFilesView, search};

pub fn search_symbols_result_view(result: &search::SymbolSearchResult, query: &str) -> String {
    if result.hits.is_empty() {
        return format!(
            "No symbols matching '{query}'. \
             Try: search_text(query=\"{query}\") for text matches, \
             or explore(query=\"{query}\") for concept-based discovery."
        );
    }

    let mut lines = vec![format!(
        "{} matches in {} files",
        result.hits.len(),
        result.file_count
    )];

    let mut last_tier: Option<search::SymbolMatchTier> = None;
    for hit in &result.hits {
        if last_tier != Some(hit.tier) {
            last_tier = Some(hit.tier);
            let header = match hit.tier {
                search::SymbolMatchTier::Exact => "\u{2500}\u{2500} Exact matches \u{2500}\u{2500}",
                search::SymbolMatchTier::Prefix => {
                    "\u{2500}\u{2500} Prefix matches \u{2500}\u{2500}"
                }
                search::SymbolMatchTier::Substring => {
                    "\u{2500}\u{2500} Substring matches \u{2500}\u{2500}"
                }
            };
            if lines.len() > 1 {
                lines.push(String::new());
            }
            lines.push(header.to_string());
        }
        // Strip redundant kind prefix from name (e.g., impl blocks named "impl Foo").
        let display_name = if hit.name.starts_with(&format!("{} ", hit.kind)) {
            &hit.name[hit.kind.len() + 1..]
        } else {
            &hit.name
        };
        let confidence = match hit.tier {
            search::SymbolMatchTier::Exact => 1.0f32,
            search::SymbolMatchTier::Prefix => 0.85,
            search::SymbolMatchTier::Substring => 0.70,
        };
        lines.push(format!(
            "  {}: {} {}  ({})  [{:.2}]",
            hit.line, hit.kind, display_name, hit.path, confidence
        ));
    }

    lines.join("\n")
}

/// Context for building context-aware suggestions on a zero-hit search.
///
/// Lets the empty-result message avoid self-referential advice — e.g. it must
/// not suggest `regex=true` when the search already set it, nor
/// `include_tests=true` when tests are already included.
#[derive(Clone, Copy, Default)]
pub struct SearchSuggestionContext {
    pub regex: bool,
    pub include_tests: bool,
    /// True when the zero-hit query was a multi-word literal. Declarations are
    /// often macro-generated or wrapped/formatted differently than the query
    /// (dogfood #5a: `pub type ProjectId` finds nothing when the type is minted
    /// by a macro), so the suggestions steer toward the bare identifier.
    pub multi_word_literal: bool,
}

/// Build the suggestion list for a zero-hit (non-structural) literal/regex
/// search, dropping any suggestion that the caller already enabled.
fn no_match_suggestions(ctx: SearchSuggestionContext) -> String {
    let mut suggestions: Vec<&str> = vec!["try search_symbols(query=...) for symbol names"];
    if ctx.regex {
        // regex already on — suggest relaxing the pattern instead of enabling it.
        suggestions.push("simplify the regex or use literal terms=[...] instead");
    } else {
        suggestions.push("use regex=true for pattern matching");
    }
    if !ctx.include_tests {
        suggestions.push("broaden with include_tests=true / include_generated=true");
    } else {
        suggestions.push("broaden with include_generated=true or a wider path_prefix");
    }
    if ctx.multi_word_literal {
        suggestions.push(
            "a multi-word literal misses declarations that are macro-generated or \
             formatted differently — retry with the bare identifier alone",
        );
    }
    suggestions.join(", ")
}

/// Zero-hit suffix when code-scoped search skipped in-scope knowledge-only files.
///
/// Returns `message` unchanged when `excluded` is 0 so a scope with no
/// knowledge-only files never grows a `search_knowledge` note.
pub(crate) fn append_excluded_knowledge_note(message: String, excluded: usize) -> String {
    if excluded == 0 {
        return message;
    }
    let noun = if excluded == 1 { "file" } else { "files" };
    format!(
        "{message}\nThis search did not scan {excluded} in-scope knowledge-only {noun}; use search_knowledge."
    )
}

pub(crate) fn append_withheld_admission_note(
    message: String,
    policy: usize,
    size: usize,
) -> String {
    if policy == 0 && size == 0 {
        return message;
    }
    let mut parts = Vec::new();
    if policy > 0 {
        let noun = if policy == 1 { "file" } else { "files" };
        parts.push(format!(
            "{policy} in-scope {noun} withheld by admission policy"
        ));
    }
    if size > 0 {
        let noun = if size == 1 { "file" } else { "files" };
        parts.push(format!("{size} in-scope {noun} over the size threshold"));
    }
    format!(
        "{message}\nThis search did not include {}.",
        parts.join(" and ")
    )
}

pub fn search_text_result_view(
    result: Result<search::TextSearchResult, search::TextSearchError>,
    group_by: Option<&str>,
    terms: Option<&[String]>,
    match_confidence: Option<f32>,
    suggestion_ctx: SearchSuggestionContext,
) -> String {
    let result = match result {
        Ok(result) => result,
        Err(search::TextSearchError::EmptyRegexQuery) => {
            return "Regex search requires a non-empty query.".to_string();
        }
        Err(search::TextSearchError::EmptyQueryOrTerms) => {
            return "Search requires a non-empty query or terms.".to_string();
        }
        Err(search::TextSearchError::InvalidRegex { pattern, error }) => {
            return format!("Invalid regex '{pattern}': {error}");
        }
        Err(search::TextSearchError::InvalidGlob {
            field,
            pattern,
            error,
        }) => {
            return format!("Invalid glob for `{field}` ('{pattern}'): {error}");
        }
        Err(search::TextSearchError::UnsupportedWholeWordRegex) => {
            return "whole_word is not supported when `regex=true`.".to_string();
        }
        Err(search::TextSearchError::InvalidStructuralPattern { pattern, error }) => {
            return format!(
                "Error: structural pattern failed to parse.\n\
                 Pattern: {pattern}\n\
                 Parse error: {error}\n\
                 Hint: $VAR = single-node metavariable, $$$ = multi-node wildcard \
                 (e.g. `fn $NAME($$$) {{ $$$ }}`). Narrow `language` if needed."
            );
        }
        Err(search::TextSearchError::UnsupportedStructuralLanguage {
            pattern,
            sample_error,
        }) => {
            return format!(
                "Error: structural search has no supported grammar for any indexed file.\n\
                 Pattern: {pattern}\n\
                 Sample: {sample_error}\n\
                 Hint: ast-grep covers programming languages only, not TOML/JSON/YAML. \
                 Widen `path_prefix` / `language` / `include_tests`."
            );
        }
    };

    let annotate_term = |line: &str| -> String {
        match &terms {
            Some(ts) if ts.len() > 1 => {
                let lower = line.to_lowercase();
                for term in *ts {
                    if lower.contains(&term.to_lowercase()) {
                        return format!("  [term: {term}]");
                    }
                }
                String::new()
            }
            _ => String::new(),
        }
    };

    if result.files.is_empty() {
        let message = if result.suppressed_by_noise > 0 {
            format!(
                "No matches for {} in source code. {} match(es) found in test modules — set include_tests=true to include them.",
                result.label, result.suppressed_by_noise
            )
        } else if result.label.starts_with("structural ") {
            // Structural searches can reach this branch three ways:
            //   1. pattern compiled for at least one candidate, matched nothing
            //   2. the index held no source-language candidates at all
            //   3. candidates existed but were all filtered out by globs / noise
            // The specific message can't distinguish them without an extra
            // counter, so avoid the earlier "Pattern parsed OK" overclaim and
            // just point at the levers that widen the search.
            let widen = if suggestion_ctx.include_tests {
                "include_generated=true / broader path_prefix"
            } else {
                "include_tests=true / include_generated=true / broader path_prefix"
            };
            format!(
                "No AST matches for {}. Consider widening the search \
                 ({widen}) or simplifying the pattern.",
                result.label
            )
        } else {
            format!(
                "No matches for {}. Suggestions: {}.",
                result.label,
                no_match_suggestions(suggestion_ctx)
            )
        };
        return append_withheld_admission_note(
            append_excluded_knowledge_note(message, result.excluded_knowledge_files),
            result.withheld_policy_files,
            result.withheld_size_files,
        );
    }

    let mut lines = vec![if let Some(confidence) = match_confidence {
        format!(
            "{} matches in {} files  [{:.2}]",
            result.total_matches,
            result.files.len(),
            confidence
        )
    } else {
        format!(
            "{} matches in {} files",
            result.total_matches,
            result.files.len()
        )
    }];
    if group_by == Some("names") {
        let mut names = Vec::new();
        for file in &result.files {
            for line_match in &file.matches {
                if let Some(enc) = &line_match.enclosing_symbol
                    && !names.iter().any(|seen| seen == &enc.name)
                {
                    names.push(enc.name.clone());
                }
            }
        }
        if names.is_empty() {
            lines.push("  (no enclosing symbol names)".to_string());
        } else {
            for name in names {
                lines.push(format!("  {name}"));
            }
        }
        return append_withheld_admission_note(
            lines.join("\n"),
            result.withheld_policy_files,
            result.withheld_size_files,
        );
    }
    for file in &result.files {
        lines.push(file.path.clone());
        if let Some(rendered_lines) = &file.rendered_lines {
            // Context mode: don't apply grouping — context windows don't compose well with it
            for rendered_line in rendered_lines {
                match rendered_line {
                    search::TextDisplayLine::Separator => lines.push("  ...".to_string()),
                    search::TextDisplayLine::Line(rendered_line) => lines.push(format!(
                        "{} {}: {}",
                        if rendered_line.is_match { ">" } else { " " },
                        rendered_line.line_number,
                        rendered_line.line
                    )),
                }
            }
        } else {
            match group_by {
                Some("symbol") => {
                    // One entry per unique enclosing symbol, showing match count
                    // Preserve insertion order by tracking fully-qualified buckets,
                    // not just names, so duplicate names in one file stay distinct.
                    let mut symbol_order: Vec<(String, String, u32, u32)> = Vec::new();
                    let mut symbol_counts: std::collections::HashMap<
                        (String, String, u32, u32),
                        usize,
                    > = std::collections::HashMap::new();
                    let mut no_symbol_count = 0usize;
                    for line_match in &file.matches {
                        if let Some(ref enc) = line_match.enclosing_symbol {
                            let key = (
                                enc.name.clone(),
                                enc.kind.clone(),
                                enc.line_range.0 + 1,
                                enc.line_range.1 + 1,
                            );
                            match symbol_counts.entry(key.clone()) {
                                std::collections::hash_map::Entry::Vacant(entry) => {
                                    symbol_order.push(key);
                                    entry.insert(1);
                                }
                                std::collections::hash_map::Entry::Occupied(mut entry) => {
                                    *entry.get_mut() += 1;
                                }
                            }
                        } else {
                            no_symbol_count += 1;
                        }
                    }
                    for (sym_name, kind, start, end) in &symbol_order {
                        if let Some(count) =
                            symbol_counts.get(&(sym_name.clone(), kind.clone(), *start, *end))
                        {
                            let match_word = if *count == 1 { "match" } else { "matches" };
                            lines.push(format!(
                                "  {} {} (lines {}-{}): {} {}",
                                kind, sym_name, start, end, count, match_word
                            ));
                        }
                    }
                    if no_symbol_count > 0 {
                        let match_word = if no_symbol_count == 1 {
                            "match"
                        } else {
                            "matches"
                        };
                        lines.push(format!("  (top-level): {} {}", no_symbol_count, match_word));
                    }
                }
                // "usage"/"purpose" filter noise lines and report how many were skipped;
                // None/"file" (default) keep every line. Otherwise identical rendering.
                mode => {
                    let filter_noise = matches!(mode, Some("usage") | Some("purpose"));
                    let mut last_symbol: Option<String> = None;
                    let mut filtered_count = 0usize;
                    for line_match in &file.matches {
                        if filter_noise && is_noise_line(&line_match.line) {
                            filtered_count += 1;
                            continue;
                        }
                        if let Some(ref enc) = line_match.enclosing_symbol {
                            if last_symbol.as_deref() != Some(enc.name.as_str()) {
                                lines.push(format!(
                                    "  in {} {} (lines {}-{}):",
                                    enc.kind,
                                    enc.name,
                                    enc.line_range.0 + 1,
                                    enc.line_range.1 + 1
                                ));
                                last_symbol = Some(enc.name.clone());
                            }
                            lines.push(format!(
                                "    > {}: {}{}",
                                line_match.line_number,
                                line_match.line,
                                annotate_term(&line_match.line)
                            ));
                        } else {
                            last_symbol = None;
                            lines.push(format!(
                                "  {}: {}{}",
                                line_match.line_number,
                                line_match.line,
                                annotate_term(&line_match.line)
                            ));
                        }
                    }
                    if filtered_count > 0 {
                        lines.push(format!(
                            "  ({filtered_count} import/comment match(es) excluded by usage filter)"
                        ));
                    }
                }
            }
        }
        if let Some(ref callers) = file.callers {
            if callers.is_empty() {
                lines.push("    (no cross-references found)".to_string());
            } else {
                let caller_strs: Vec<String> = callers
                    .iter()
                    .map(|c| format!("{} ({}:{})", c.symbol, c.file, c.line))
                    .collect();
                lines.push(format!("    Called by: {}", caller_strs.join(", ")));
            }
        }
    }
    // Add follow_refs clarification if any non-empty callers were included
    let has_nonempty_callers = result
        .files
        .iter()
        .any(|f| f.callers.as_ref().is_some_and(|c| !c.is_empty()));
    if has_nonempty_callers {
        lines.push(String::new());
        lines.push(
            "Note: Caller information is for the enclosing symbol, not for the search text itself."
                .to_string(),
        );
    }
    append_withheld_admission_note(
        lines.join("\n"),
        result.withheld_policy_files,
        result.withheld_size_files,
    )
}

pub fn search_files_resolve_result_view(view: &SearchFilesResolveView) -> String {
    match view {
        SearchFilesResolveView::EmptyHint => "Path hint must not be empty.".to_string(),
        SearchFilesResolveView::Resolved { path } => path.clone(),
        SearchFilesResolveView::ResolvedMetadataOnly { path, reason } => {
            format!("{path}  [metadata-only: {reason}]")
        }
        SearchFilesResolveView::NotFound { hint } => {
            format!(
                "No indexed source path matched '{hint}'. \
                 Try search_files(query=\"{hint}\") without resolve=true for fuzzy matches, \
                 or check the path with get_repo_map(detail=\"tree\")."
            )
        }
        SearchFilesResolveView::Ambiguous {
            hint,
            matches,
            overflow_count,
        } => {
            let mut lines = vec![format!(
                "Ambiguous path hint '{hint}' ({} matches)",
                matches.len() + overflow_count
            )];
            lines.extend(matches.iter().map(|path| format!("  {path}")));
            if *overflow_count > 0 {
                lines.push(format!("  ... and {} more", overflow_count));
            }
            lines.join("\n")
        }
    }
}

pub fn search_files_result_view(view: &SearchFilesView) -> String {
    match view {
        SearchFilesView::EmptyQuery => "Path search requires a non-empty query.".to_string(),
        SearchFilesView::NotFound { query } => {
            format!("No indexed source files matching '{query}'")
        }
        SearchFilesView::Found {
            total_matches,
            overflow_count,
            hits,
            ..
        } => {
            let mut lines = vec![if *total_matches == 1 {
                "1 matching file".to_string()
            } else {
                format!("{total_matches} matching files")
            }];

            let mut last_tier: Option<SearchFilesTier> = None;
            for hit in hits {
                if last_tier != Some(hit.tier) {
                    last_tier = Some(hit.tier);
                    let header = match hit.tier {
                        SearchFilesTier::CoChange => "── Co-changed files (coupling store) ──",
                        SearchFilesTier::StrongPath => "── Strong path matches ──",
                        SearchFilesTier::Basename => "── Basename matches ──",
                        SearchFilesTier::LoosePath => "── Loose path matches ──",
                        SearchFilesTier::MetadataOnly => {
                            "── Metadata-only paths (Tier-2: not parsed) ──"
                        }
                    };
                    if lines.len() > 1 {
                        lines.push(String::new());
                    }
                    lines.push(header.to_string());
                }
                let confidence = match hit.tier {
                    SearchFilesTier::CoChange => 0.60f32,
                    SearchFilesTier::StrongPath => 0.80,
                    SearchFilesTier::Basename => 0.90,
                    SearchFilesTier::LoosePath => 0.40,
                    SearchFilesTier::MetadataOnly => 0.30,
                };
                if hit.tier == SearchFilesTier::MetadataOnly {
                    let reason = hit.metadata_reason.as_deref().unwrap_or("metadata only");
                    lines.push(format!(
                        "  {}  [metadata-only: {}]  [{:.2}]",
                        hit.path, reason, confidence
                    ));
                } else if let (Some(score), Some(shared)) = (hit.coupling_score, hit.shared_commits)
                {
                    lines.push(format!(
                        "  {}  ({:.0}% coupled, {} shared commits)  [{:.2}]",
                        hit.path,
                        score * 100.0,
                        shared,
                        confidence
                    ));
                } else {
                    lines.push(format!("  {}  [{:.2}]", hit.path, confidence));
                }
            }

            if *overflow_count > 0 {
                lines.push(format!("... and {} more", overflow_count));
            }

            lines.join("\n")
        }
    }
}
