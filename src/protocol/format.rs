//! Pure formatting functions for all 10 tool responses.
//!
//! All functions take `&LiveIndex` (or data derived from it) and return `String`.
//! No I/O, no async. Output matches the locked formats defined in CONTEXT.md.

/// Budget limits for reference/dependent output to prevent unbounded token usage.
pub struct OutputLimits {
    /// Maximum number of files to include in the output.
    pub max_files: usize,
    /// Maximum number of reference/hit lines per file.
    pub max_per_file: usize,
    /// Maximum total hits across all files (max_files * max_per_file).
    pub total_hits: usize,
}

impl OutputLimits {
    pub fn new(max_files: u32, max_per_file: u32) -> Self {
        Self {
            max_files: max_files.min(100) as usize,
            max_per_file: max_per_file.min(50) as usize,
            total_hits: (max_files.min(100) * max_per_file.min(50)) as usize,
        }
    }
}

impl Default for OutputLimits {
    fn default() -> Self {
        Self {
            max_files: 20,
            max_per_file: 10,
            total_hits: 200,
        }
    }
}

use crate::domain::index::{AdmissionTier, SkippedFile};
use crate::live_index::{
    ContextBundleFoundView, ContextBundleSectionView, ContextBundleView, FileContentView,
    FileOutlineView, FindDependentsView, FindReferencesView, HealthStats, ImplBlockSuggestionView,
    ImplementationsView, IndexedFile, InspectMatchView, LiveIndex, PublishedIndexState,
    RepoOutlineFileView, RepoOutlineView, SearchFilesResolveView, SearchFilesTier, SearchFilesView,
    SymbolDetailView, TypeDependencyView, WhatChangedTimestampView, search,
};
use crate::{cli::hook::HookAdoptionSnapshot, sidecar::StatsSnapshot};

/// Format the file outline for a given path.
///
/// Header: `{path}  ({N} symbols)`
/// Body: each symbol indented by `depth * 2` spaces, then `{kind:<12} {name:<30} {start}-{end}`
/// Not-found: "File not found: {path}"
pub fn file_outline(index: &LiveIndex, path: &str) -> String {
    match index.capture_shared_file(path) {
        Some(file) => file_outline_from_indexed_file(file.as_ref()),
        None => not_found_file(path),
    }
}

pub fn file_outline_from_indexed_file(file: &IndexedFile) -> String {
    render_file_outline(&file.relative_path, &file.symbols)
}

fn render_file_outline(relative_path: &str, symbols: &[crate::domain::SymbolRecord]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("{}  ({} symbols)", relative_path, symbols.len()));

    for sym in symbols {
        let indent = "  ".repeat(sym.depth as usize);
        let kind_str = sym.kind.to_string();
        lines.push(format!(
            "{}{:<12} {:<30} {}-{}",
            indent,
            kind_str,
            sym.name,
            sym.line_range.0 + 1,
            sym.line_range.1 + 1
        ));
    }

    lines.join("\n")
}

/// Compatibility renderer for `FileOutlineView`.
///
/// Main hot-path readers should prefer `file_outline_from_indexed_file()`.
pub fn file_outline_view(view: &FileOutlineView) -> String {
    render_file_outline(&view.relative_path, &view.symbols)
}

/// Return the full source body for a named symbol plus a footer.
///
/// Footer: `[{kind}, lines {start}-{end}, {byte_count} bytes]`
/// Not-found: see `not_found_symbol`
pub fn symbol_detail(
    index: &LiveIndex,
    path: &str,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    match index.capture_shared_file(path) {
        Some(file) => symbol_detail_from_indexed_file(file.as_ref(), name, kind_filter, None),
        None => not_found_file(path),
    }
}

pub fn symbol_detail_from_indexed_file(
    file: &IndexedFile,
    name: &str,
    kind_filter: Option<&str>,
    symbol_line: Option<u32>,
) -> String {
    use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};

    match resolve_symbol_selector(file, name, kind_filter, symbol_line) {
        SymbolSelectorMatch::Selected(_idx, sym) => {
            let start = sym.effective_start() as usize;
            let end = sym.byte_range.1 as usize;
            let clamped_end = end.min(file.content.len());
            let clamped_start = start.min(clamped_end);
            let body =
                String::from_utf8_lossy(&file.content[clamped_start..clamped_end]).into_owned();
            let byte_count = end.saturating_sub(start);
            format!(
                "{}\n[{}, lines {}-{}, {} bytes]",
                body,
                sym.kind,
                sym.line_range.0 + 1,
                sym.line_range.1 + 1,
                byte_count
            )
        }
        SymbolSelectorMatch::NotFound => {
            render_not_found_symbol(&file.relative_path, &file.symbols, name)
        }
        SymbolSelectorMatch::Ambiguous(lines) => {
            let line_strs: Vec<String> = lines.iter().map(|l| format!("{}", l + 1)).collect();
            format!(
                "Ambiguous: {} `{}` symbols in {} (lines {}). Pass symbol_line to disambiguate.",
                lines.len(),
                name,
                file.relative_path,
                line_strs.join(", ")
            )
        }
    }
}

/// Compatibility renderer for `SymbolDetailView`.
///
/// Main hot-path readers should prefer `symbol_detail_from_indexed_file()`.
pub fn symbol_detail_view(
    view: &SymbolDetailView,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    render_symbol_detail(
        &view.relative_path,
        &view.content,
        &view.symbols,
        name,
        kind_filter,
    )
}

fn render_symbol_detail(
    relative_path: &str,
    content: &[u8],
    symbols: &[crate::domain::SymbolRecord],
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    let matching: Vec<&crate::domain::SymbolRecord> = symbols
        .iter()
        .filter(|s| {
            s.name == name
                && kind_filter
                    .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                    .unwrap_or(true)
        })
        .collect();

    match matching.first() {
        None => render_not_found_symbol(relative_path, symbols, name),
        Some(s) => {
            let start = s.effective_start() as usize;
            let end = s.byte_range.1 as usize;
            let clamped_end = end.min(content.len());
            let clamped_start = start.min(clamped_end);
            let body = String::from_utf8_lossy(&content[clamped_start..clamped_end]).into_owned();
            let byte_count = end.saturating_sub(start);
            let mut result = format!(
                "{}\n[{}, lines {}-{}, {} bytes]",
                body,
                s.kind,
                s.line_range.0 + 1,
                s.line_range.1 + 1,
                byte_count
            );
            if matching.len() > 1 {
                let others = matching.len() - 1;
                let lines: Vec<String> = matching[1..]
                    .iter()
                    .map(|m| format!("{}", m.line_range.0 + 1))
                    .collect();
                result.push_str(&format!(
                    "\nNote: {} more `{}` in this file (line {}). Use symbol_line to disambiguate.",
                    others,
                    name,
                    lines.join(", ")
                ));
            }
            result
        }
    }
}

pub fn code_slice_view(path: &str, slice: &[u8]) -> String {
    let text = String::from_utf8_lossy(slice).into_owned();
    format!("{path}\n{text}")
}

pub fn code_slice_from_indexed_file(
    file: &IndexedFile,
    start_byte: usize,
    end_byte: Option<usize>,
) -> String {
    let end = end_byte
        .unwrap_or(file.content.len())
        .min(file.content.len());
    let start = start_byte.min(end);
    code_slice_view(&file.relative_path, &file.content[start..end])
}

/// Search for symbols matching a query (case-insensitive), with 3-tier scored ranking.
///
/// Output sections (only non-empty tiers shown):
/// ```text
/// ── Exact matches ──
///   {line}: {kind} {name}  ({file})
///
/// ── Prefix matches ──
///   ...
///
/// ── Substring matches ──
///   ...
/// ```
/// Header: `{N} matches in {M} files`
/// Empty: "No symbols matching '{query}'"
pub fn search_symbols_result(index: &LiveIndex, query: &str) -> String {
    search_symbols_result_with_kind(index, query, None)
}

pub fn search_symbols_result_with_kind(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
) -> String {
    let result = search::search_symbols(
        index,
        query,
        kind_filter,
        search::ResultLimit::symbol_search_default().get(),
    );
    search_symbols_result_view(&result, query)
}

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

/// Search for text content matches (case-insensitive substring).
///
/// For queries >= 3 chars, uses the TrigramIndex to select candidate files before scanning.
/// For queries < 3 chars, falls back to scanning all files (trigram search handles this internally).
///
/// Header: `{N} matches in {M} files`
/// Body: grouped by file, each match: `  {line_number}: {line_content}`
/// Empty: "No matches for '{query}'"
pub fn search_text_result(index: &LiveIndex, query: &str) -> String {
    search_text_result_with_options(index, Some(query), None, false)
}

pub fn search_text_result_with_options(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
) -> String {
    let result = search::search_text(index, query, terms, regex);
    search_text_result_view(result, None, None, None)
}

/// Returns true if the line looks like an import statement or a non-doc comment.
pub fn is_noise_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("/**") {
        return false;
    }
    trimmed.starts_with("use ")
        || trimmed.starts_with("import ")
        || (trimmed.starts_with("from ") && trimmed.contains(" import "))
        || trimmed.starts_with("require(")
        || trimmed.starts_with("#include")
        || trimmed.starts_with("//")
        || (trimmed.starts_with("# ")
            && !trimmed.starts_with("# TODO")
            && !trimmed.starts_with("# FIXME")
            && !trimmed.starts_with("# NOTE")
            && !trimmed.starts_with("# HACK")
            && !trimmed.starts_with("# type:"))
        || trimmed == "#"
        || trimmed.starts_with("#!")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("*/")
        || trimmed.starts_with("--")
        || (trimmed.starts_with("const ") && trimmed.contains("require("))
        || (trimmed.starts_with("let ") && trimmed.contains("require("))
        || (trimmed.starts_with("var ") && trimmed.contains("require("))
}

pub fn search_text_result_view(
    result: Result<search::TextSearchResult, search::TextSearchError>,
    group_by: Option<&str>,
    terms: Option<&[String]>,
    match_confidence: Option<f32>,
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
                 Hint: ast-grep patterns use $VAR for single-node metavariables \
                 and $$$ for multi-node wildcards (e.g., `fn $NAME($$$) {{ $$$ }}`). \
                 Narrow `language` to target a specific grammar if needed."
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
                 Hint: ast-grep only supports programming-language grammars. \
                 Config languages (TOML, JSON, YAML) are never searchable with \
                 `structural=true`. Widen `path_prefix` / `language` / `include_tests` \
                 so at least one source-code candidate is in scope."
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
        if result.suppressed_by_noise > 0 {
            return format!(
                "No matches for {} in source code. {} match(es) found in test modules — set include_tests=true to include them.",
                result.label, result.suppressed_by_noise
            );
        }
        // Structural searches can reach this branch three ways:
        //   1. pattern compiled for at least one candidate, matched nothing
        //   2. the index held no source-language candidates at all
        //   3. candidates existed but were all filtered out by globs / noise
        // The specific message can't distinguish them without an extra
        // counter, so avoid the earlier "Pattern parsed OK" overclaim and
        // just point at the levers that widen the search.
        if result.label.starts_with("structural ") {
            return format!(
                "No AST matches for {}. Consider widening the search \
                 (include_tests=true / include_generated=true / broader path_prefix) \
                 or simplifying the pattern.",
                result.label
            );
        }
        return format!(
            "No matches for {}. Suggestions: \
             try search_symbols(query=...) for symbol names, \
             or use regex=true for pattern matching, \
             or broaden with include_tests=true / include_generated=true.",
            result.label
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
                Some("usage") | Some("purpose") => {
                    let mut last_symbol: Option<String> = None;
                    let mut filtered_count = 0usize;
                    for line_match in &file.matches {
                        if is_noise_line(&line_match.line) {
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
                // None or Some("file") — default behavior
                _ => {
                    let mut last_symbol: Option<String> = None;
                    for line_match in &file.matches {
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
    lines.join("\n")
}

/// Generate a depth-limited source file tree with symbol counts per file and directory.
///
/// - `path`: subtree prefix filter (empty/blank = project root).
/// - `depth`: maximum depth levels to expand (default 2, max 5).
///
/// Output format:
/// ```text
/// {dir}/  ({N} files, {M} symbols)
///   {file} [{lang}]  ({K} symbols)
///   {subdir}/  ({N} files, {M} symbols)
/// ...
/// {D} directories, {F} files, {S} symbols
/// ```
pub fn file_tree(index: &LiveIndex, path: &str, depth: u32) -> String {
    let view = index.capture_repo_outline_view();
    file_tree_view(&view.files, path, depth)
}

pub fn file_tree_view(files: &[RepoOutlineFileView], path: &str, depth: u32) -> String {
    let depth = depth.min(5);
    let prefix = path.trim_matches('/');

    // Collect all files whose relative_path starts with the path prefix.
    let matching_files: Vec<&RepoOutlineFileView> = files
        .iter()
        .filter(|file| {
            let p = file.relative_path.as_str();
            if prefix.is_empty() {
                true
            } else {
                p.starts_with(prefix)
                    && (p.len() == prefix.len() || p.as_bytes().get(prefix.len()) == Some(&b'/'))
            }
        })
        .collect();

    if matching_files.is_empty() {
        return format!(
            "No source files found under '{}'",
            if prefix.is_empty() { "." } else { prefix }
        );
    }

    // Build a tree: BTreeMap from directory path -> Vec<(filename, lang, symbol_count)>
    // Node entries are keyed by their path component at each level.
    use std::collections::BTreeMap;

    // Strip the prefix from all paths before building the tree.
    let strip_len = if prefix.is_empty() {
        0
    } else {
        prefix.len() + 1
    };
    let stripped: Vec<(&str, &RepoOutlineFileView)> = matching_files
        .into_iter()
        .map(|file| {
            let p = file.relative_path.as_str();
            (
                if p.len() >= strip_len {
                    &p[strip_len..]
                } else {
                    p
                },
                file,
            )
        })
        .collect();

    // Recursively build tree lines.
    fn build_lines(
        entries: &[(&str, &RepoOutlineFileView)],
        current_depth: u32,
        max_depth: u32,
        indent: usize,
    ) -> Vec<String> {
        // Group by first path component.
        let mut dirs: BTreeMap<&str, Vec<(&str, &RepoOutlineFileView)>> = BTreeMap::new();
        let mut files_here: Vec<(&str, &RepoOutlineFileView)> = Vec::new();

        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                let dir_part = &rel[..slash];
                let rest = &rel[slash + 1..];
                dirs.entry(dir_part).or_default().push((rest, file));
            } else {
                files_here.push((rel, file));
            }
        }

        let pad = "  ".repeat(indent);
        let mut lines = Vec::new();

        // Files at this level
        files_here.sort_by_key(|(name, _)| *name);
        for (name, file) in &files_here {
            let sym_count = file.symbol_count;
            let sym_label = if sym_count == 1 { "symbol" } else { "symbols" };
            let tag = file.noise_class.tag();
            if tag.is_empty() {
                lines.push(format!(
                    "{}{} [{}]  ({} {})",
                    pad, name, file.language, sym_count, sym_label
                ));
            } else {
                lines.push(format!(
                    "{}{} [{}]  ({} {}) {}",
                    pad, name, file.language, sym_count, sym_label, tag
                ));
            }
        }

        // Directories at this level
        for (dir_name, children) in &dirs {
            let file_count = count_files(children);
            let sym_count: usize = children.iter().map(|(_, f)| f.symbol_count).sum();
            let sym_label = if sym_count == 1 { "symbol" } else { "symbols" };

            if current_depth >= max_depth {
                // Collapsed — just show summary line
                lines.push(format!(
                    "{}{}/  ({} files, {} {})",
                    pad, dir_name, file_count, sym_count, sym_label
                ));
            } else {
                lines.push(format!(
                    "{}{}/  ({} files, {} {})",
                    pad, dir_name, file_count, sym_count, sym_label
                ));
                let sub_lines = build_lines(children, current_depth + 1, max_depth, indent + 1);
                lines.extend(sub_lines);
            }
        }

        lines
    }

    fn count_files(entries: &[(&str, &RepoOutlineFileView)]) -> usize {
        let mut count = 0;
        for (rel, _) in entries {
            if rel.contains('/') {
                // nested
            } else {
                count += 1;
            }
        }
        // also count files in sub-directories
        let mut dirs: std::collections::HashMap<&str, Vec<(&str, &RepoOutlineFileView)>> =
            std::collections::HashMap::new();
        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                dirs.entry(&rel[..slash])
                    .or_default()
                    .push((&rel[slash + 1..], file));
            }
        }
        for children in dirs.values() {
            count += count_files(children);
        }
        count
    }

    fn count_dirs(entries: &[(&str, &RepoOutlineFileView)]) -> usize {
        let mut dirs: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut sub_entries: std::collections::HashMap<&str, Vec<(&str, &RepoOutlineFileView)>> =
            std::collections::HashMap::new();
        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                let dir_name = &rel[..slash];
                dirs.insert(dir_name);
                sub_entries
                    .entry(dir_name)
                    .or_default()
                    .push((&rel[slash + 1..], file));
            }
        }
        let mut total = dirs.len();
        for children in sub_entries.values() {
            total += count_dirs(children);
        }
        total
    }

    let body_lines = build_lines(&stripped, 1, depth, 0);

    let total_files = stripped.len();
    let total_dirs = count_dirs(&stripped);
    let total_symbols: usize = stripped.iter().map(|(_, f)| f.symbol_count).sum();
    let sym_label = if total_symbols == 1 {
        "symbol"
    } else {
        "symbols"
    };

    let mut output = body_lines;
    output.push(format!(
        "{} directories, {} files, {} {}",
        total_dirs, total_files, total_symbols, sym_label
    ));

    output.join("\n")
}

/// Format a byte count as a human-readable size string.
fn human_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Like `file_tree_view` but also incorporates skipped files:
/// - Tier 2 (MetadataOnly) files appear in the tree with a `[skipped: {reason}, {size}]` tag.
/// - Tier 3 (HardSkip) files do NOT appear in the tree; instead a footer line is appended:
///   `{N} hard-skipped artifacts not shown (>100MB)`
pub fn file_tree_view_with_skipped(
    files: &[RepoOutlineFileView],
    skipped: &[SkippedFile],
    path: &str,
    depth: u32,
) -> String {
    // Separate Tier 2 and Tier 3 skipped files, filtered to the path prefix.
    let prefix = path.trim_matches('/');
    let tier2: Vec<&SkippedFile> = skipped
        .iter()
        .filter(|sf| {
            sf.decision.tier == AdmissionTier::MetadataOnly
                && (prefix.is_empty()
                    || sf.path.starts_with(prefix)
                        && (sf.path.len() == prefix.len()
                            || sf.path.as_bytes().get(prefix.len()) == Some(&b'/')))
        })
        .collect();
    let tier3_count = skipped
        .iter()
        .filter(|sf| {
            sf.decision.tier == AdmissionTier::HardSkip
                && (prefix.is_empty()
                    || sf.path.starts_with(prefix)
                        && (sf.path.len() == prefix.len()
                            || sf.path.as_bytes().get(prefix.len()) == Some(&b'/')))
        })
        .count();

    // Build the base tree from indexed files.
    let base = if tier2.is_empty() && files.is_empty() {
        file_tree_view(files, path, depth)
    } else {
        // Build augmented file list: convert Tier 2 skipped files into synthetic
        // RepoOutlineFileView entries so they appear in the tree with the skip tag appended.
        // We render the base tree first, then inject Tier 2 entries separately.
        file_tree_view(files, path, depth)
    };

    // If there are no indexed files and no Tier 2 skipped files, the base already handles it.
    // We need to inject Tier 2 entries into the output.
    // Strategy: build Tier 2 lines separately and splice into base before the footer.
    // Simpler approach: re-render with Tier 2 files appended as extra lines after the base tree body.

    // Split base output into body lines and footer (last line is always the summary).
    let mut lines: Vec<String> = base.lines().map(String::from).collect();
    let footer = if lines.len() > 1 { lines.pop() } else { None };

    // Build Tier 2 file lines. Each gets placed at the correct indentation by stripping the prefix.
    let strip_len = if prefix.is_empty() {
        0
    } else {
        prefix.len() + 1
    };
    let mut tier2_lines: Vec<(String, String)> = tier2
        .iter()
        .map(|sf| {
            let p = sf.path.as_str();
            let rel = if p.len() >= strip_len {
                &p[strip_len..]
            } else {
                p
            };
            let reason = sf
                .decision
                .reason
                .as_ref()
                .map(|r| r.to_string())
                .unwrap_or_else(|| "skipped".to_string());
            let tag = format!("[skipped: {}, {}]", reason, human_size(sf.size));
            (rel.to_string(), tag)
        })
        .collect();
    tier2_lines.sort_by(|a, b| a.0.cmp(&b.0));

    for (rel, tag) in &tier2_lines {
        // Compute indentation from path depth.
        let depth_level = rel.chars().filter(|&c| c == '/').count();
        let pad = "  ".repeat(depth_level);
        let filename = rel.rsplit('/').next().unwrap_or(rel.as_str());
        lines.push(format!("{}{}  {}", pad, filename, tag));
    }

    // Re-append footer, then add Tier 3 footer if needed.
    if let Some(f) = footer {
        lines.push(f);
    }
    if tier3_count > 0 {
        let artifact_label = if tier3_count == 1 {
            "artifact"
        } else {
            "artifacts"
        };
        lines.push(format!(
            "{} hard-skipped {} not shown (>100MB)",
            tier3_count, artifact_label
        ));
    }

    lines.join("\n")
}

/// Generate a directory-tree overview of the repo.
///
/// Header: `{project_name}  ({N} files, {M} symbols)`
/// Body: sorted paths, each: `  {filename:<20} {language:<12} {symbol_count} symbols`
pub fn repo_outline(index: &LiveIndex, project_name: &str) -> String {
    let view = index.capture_repo_outline_view();
    repo_outline_view(&view, project_name)
}

pub fn repo_outline_view(view: &RepoOutlineView, project_name: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{project_name}  ({} files, {} symbols)",
        view.total_files, view.total_symbols
    ));

    // Always show full relative paths for orientation (not just disambiguated basenames).
    let path_width = view
        .files
        .iter()
        .map(|f| f.relative_path.len())
        .max()
        .unwrap_or(20)
        .clamp(20, 50);

    for file in &view.files {
        lines.push(format!(
            "  {:<width$} {:<12} {} symbols",
            file.relative_path,
            file.language.to_string(),
            file.symbol_count,
            width = path_width
        ));
    }

    lines.join("\n")
}

/// Generate a health report for the index.
///
/// Watcher state is read from `health_stats()` (Off defaults when no watcher is active).
/// Use `health_report_with_watcher` when the live `WatcherInfo` should be reflected.
///
/// Format:
/// ```text
/// Status: {Ready|Empty|Degraded}
/// Files:  {N} indexed ({P} parsed, {PP} partial, {F} failed)
/// Symbols: {S}
/// Loaded in: {D}ms
/// Watcher: active ({E} events, last: {T}, debounce: {D}ms)
///     or: degraded ({E} events processed before failure)
///     or: off
/// ```
pub fn health_report(index: &LiveIndex) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats();
    health_report_from_stats(status, &stats)
}

/// Generate a health report for the index with live watcher state.
///
/// Uses `health_stats_with_watcher` to incorporate the live `WatcherInfo` into the report.
/// Called by the `health` tool handler in production (watcher is always available there).
pub fn health_report_with_watcher(
    index: &LiveIndex,
    watcher: &crate::watcher::WatcherInfo,
) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats_with_watcher(watcher);
    health_report_from_stats(status, &stats)
}

pub fn health_report_from_published_state(
    published: &PublishedIndexState,
    watcher: &crate::watcher::WatcherInfo,
) -> String {
    let mut stats = HealthStats {
        file_count: published.file_count,
        symbol_count: published.symbol_count,
        parsed_count: published.parsed_count,
        partial_parse_count: published.partial_parse_count,
        failed_count: published.failed_count,
        load_duration: published.load_duration,
        watcher_state: watcher.state.clone(),
        events_processed: watcher.events_processed,
        last_event_at: watcher.last_event_at,
        debounce_window_ms: watcher.debounce_window_ms,
        overflow_count: watcher.overflow_count,
        last_overflow_at: watcher.last_overflow_at,
        stale_files_found: watcher.stale_files_found,
        last_reconcile_at: watcher.last_reconcile_at,
        partial_parse_files: published.partial_parse_files.clone(),
        failed_files: published.failed_files.clone(),
        tier_counts: published.tier_counts,
    };
    // Preserve the existing formatter shape by reusing HealthStats.
    if matches!(stats.watcher_state, crate::watcher::WatcherState::Off) {
        stats.events_processed = 0;
        stats.last_event_at = None;
    }
    health_report_from_stats(published.status_label(), &stats)
}

pub fn health_report_from_stats(status: &str, stats: &HealthStats) -> String {
    use crate::watcher::WatcherState;

    let relative_age = |time: Option<std::time::SystemTime>| -> String {
        match time {
            None => "never".to_string(),
            Some(t) => {
                let secs = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
                format!("{secs}s ago")
            }
        }
    };

    let watcher_line = match &stats.watcher_state {
        WatcherState::Active
            if stats.events_processed == 0
                && stats.last_event_at.is_none()
                && stats.overflow_count == 0
                && stats.stale_files_found == 0 =>
        {
            format!(
                "Watcher: active (idle; event-driven, waiting for filesystem changes, debounce: {}ms)",
                stats.debounce_window_ms
            )
        }
        WatcherState::Active => format!(
            "Watcher: active (event-driven; {} events, last change: {}, debounce: {}ms, overflows: {}, reconcile repairs: {}, last overflow: {}, last reconcile: {})",
            stats.events_processed,
            relative_age(stats.last_event_at),
            stats.debounce_window_ms,
            stats.overflow_count,
            stats.stale_files_found,
            relative_age(stats.last_overflow_at),
            relative_age(stats.last_reconcile_at)
        ),
        WatcherState::Degraded => format!(
            "Watcher: degraded (event stream failed after {} processed events, overflows: {}, reconcile repairs: {}, last overflow: {}, last reconcile: {})",
            stats.events_processed,
            stats.overflow_count,
            stats.stale_files_found,
            relative_age(stats.last_overflow_at),
            relative_age(stats.last_reconcile_at)
        ),
        WatcherState::Off => "Watcher: off".to_string(),
    };

    let (tier1, tier2, tier3) = stats.tier_counts;
    let total_discovered = tier1 + tier2 + tier3;
    let admission_section = format!(
        "\nAdmission: {} files discovered\n  Tier 1 (indexed): {}\n  Tier 2 (metadata only): {}\n  Tier 3 (hard-skipped): {}",
        total_discovered, tier1, tier2, tier3
    );

    let mut output = format!(
        "Status: {}\nFiles:  {} indexed ({} parsed, {} partial, {} failed)\nSymbols: {}\nLoaded in: {}ms\n{}{}",
        status,
        stats.file_count,
        stats.parsed_count,
        stats.partial_parse_count,
        stats.failed_count,
        stats.symbol_count,
        stats.load_duration.as_millis(),
        watcher_line,
        admission_section
    );

    if stats.partial_parse_count > 0 && stats.failed_count == 0 {
        output.push_str(
            "\nParse resilience: partial files kept best-effort symbols; inspect the partial list below only if answers from those files look incomplete.",
        );
    } else if stats.failed_count > 0 {
        output.push_str(
            "\nParse resilience: failed files are excluded from symbol-level answers until they re-index cleanly. Inspect the failed-file list below, use raw reads or validate_file_syntax for those paths, then re-run index_folder after fixing the source file.",
        );
    }

    if !stats.partial_parse_files.is_empty() {
        output.push_str(&format!(
            "\nPartial parse files ({}):\n",
            stats.partial_parse_files.len()
        ));
        for (i, path) in stats.partial_parse_files.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {}\n", i + 1, path));
        }
        if stats.partial_parse_files.len() > 10 {
            output.push_str(&format!(
                "  ... and {} more partial files\n",
                stats.partial_parse_files.len() - 10
            ));
        }
    }

    if !stats.failed_files.is_empty() {
        output.push_str(&format!("\nFailed files ({}):\n", stats.failed_files.len()));
        for (i, (path, error)) in stats.failed_files.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {} — {}\n", i + 1, path, error));
        }
        if stats.failed_files.len() > 10 {
            output.push_str(&format!(
                "  ... and {} more failed files\n",
                stats.failed_files.len() - 10
            ));
        }
    }

    output
}

/// List files changed since the given Unix timestamp.
///
/// If since_ts < loaded_at: return list of all files (entire index is "newer")
/// If since_ts >= loaded_at: return "No changes detected since last index load."
pub fn what_changed_result(index: &LiveIndex, since_ts: i64) -> String {
    let view = index.capture_what_changed_timestamp_view();
    what_changed_timestamp_view(&view, since_ts)
}

pub fn what_changed_timestamp_view(view: &WhatChangedTimestampView, since_ts: i64) -> String {
    if since_ts < view.loaded_secs {
        // Entire index is newer — list all files
        if view.paths.is_empty() {
            return "Index is empty — no files tracked.".to_string();
        }
        view.paths.join("\n")
    } else {
        "No changes detected since last index load.".to_string()
    }
}

pub fn what_changed_paths_result(paths: &[String], empty_message: &str) -> String {
    let mut normalized_paths: Vec<String> =
        paths.iter().map(|path| path.replace('\\', "/")).collect();
    normalized_paths.sort();
    normalized_paths.dedup();

    if normalized_paths.is_empty() {
        return empty_message.to_string();
    }

    normalized_paths.join("\n")
}

pub fn search_files_resolve_result_view(view: &SearchFilesResolveView) -> String {
    match view {
        SearchFilesResolveView::EmptyHint => "Path hint must not be empty.".to_string(),
        SearchFilesResolveView::Resolved { path } => path.clone(),
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

pub fn search_files(index: &LiveIndex, query: &str, limit: usize) -> String {
    let view = index.capture_search_files_view(query, limit, None);
    search_files_result_view(&view)
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
                        SearchFilesTier::CoChange => {
                            "── Co-changed files (git temporal coupling) ──"
                        }
                        SearchFilesTier::StrongPath => "── Strong path matches ──",
                        SearchFilesTier::Basename => "── Basename matches ──",
                        SearchFilesTier::LoosePath => "── Loose path matches ──",
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
                };
                if let (Some(score), Some(shared)) = (hit.coupling_score, hit.shared_commits) {
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

/// Return raw file content, optionally sliced by 1-indexed line range.
///
/// Not-found: "File not found: {path}"
pub fn file_content(
    index: &LiveIndex,
    path: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    let options = search::FileContentOptions::for_explicit_path_read(path, start_line, end_line);
    match index.capture_shared_file_for_scope(&options.path_scope) {
        Some(file) => {
            file_content_from_indexed_file_with_context(file.as_ref(), options.content_context)
        }
        None => not_found_file(path),
    }
}

pub fn file_content_from_indexed_file(
    file: &IndexedFile,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    file_content_from_indexed_file_with_context(
        file,
        search::ContentContext::line_range(start_line, end_line),
    )
}

pub fn file_content_from_indexed_file_with_context(
    file: &IndexedFile,
    context: search::ContentContext,
) -> String {
    if let Some(chunk_index) = context.chunk_index {
        let max_lines = match context.max_lines {
            Some(ml) => ml,
            None => {
                return format!(
                    "{} [error: chunked read requires max_lines parameter]",
                    file.relative_path
                );
            }
        };
        return render_numbered_chunk_excerpt(file, chunk_index, max_lines);
    }

    if let Some(around_symbol) = context.around_symbol.as_deref() {
        return render_numbered_around_symbol_excerpt(
            file,
            around_symbol,
            context.symbol_line,
            context.context_lines.unwrap_or(0),
            context.max_lines,
        );
    }

    if let Some(around_match) = context.around_match.as_deref() {
        return render_numbered_around_match_excerpt(
            file,
            around_match,
            context.match_occurrence.unwrap_or(1),
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

    render_file_content_bytes(&file.relative_path, &file.content, context)
}

/// Compatibility renderer for `FileContentView`.
///
/// Main hot-path readers should prefer `file_content_from_indexed_file()`.
pub fn file_content_view(
    view: &FileContentView,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    render_file_content_bytes(
        &view.relative_path,
        &view.content,
        search::ContentContext::line_range(start_line, end_line),
    )
}

pub fn validate_file_syntax_result(path: &str, file: &IndexedFile) -> String {
    let mut lines = vec![
        format!("Syntax validation: {path}"),
        format!("Language: {}", file.language),
    ];

    match &file.parse_status {
        crate::live_index::ParseStatus::Parsed => {
            lines.push("Status: ok".to_string());
        }
        crate::live_index::ParseStatus::PartialParse { warning } => {
            lines.push("Status: partial".to_string());
            if let Some(diagnostic) = &file.parse_diagnostic {
                lines.push(format!("Diagnostic: {}", diagnostic.summary()));
                if let Some((start, end)) = diagnostic.byte_span {
                    lines.push(format!("Byte span: {start}..{end}"));
                }
            } else {
                lines.push(format!("Diagnostic: {warning}"));
            }
        }
        crate::live_index::ParseStatus::Failed { error } => {
            lines.push("Status: failed".to_string());
            if let Some(diagnostic) = &file.parse_diagnostic {
                lines.push(format!("Diagnostic: {}", diagnostic.summary()));
                if let Some((start, end)) = diagnostic.byte_span {
                    lines.push(format!("Byte span: {start}..{end}"));
                }
            } else {
                lines.push(format!("Diagnostic: {error}"));
            }
        }
    }

    lines.push(format!("Symbols extracted: {}", file.symbols.len()));
    lines.join("\n")
}

const DEFAULT_AROUND_LINE_CONTEXT_LINES: u32 = 2;

pub(crate) fn render_file_content_bytes(
    path: &str,
    content: &[u8],
    context: search::ContentContext,
) -> String {
    let content = String::from_utf8_lossy(content);
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len() as u32;

    // Validate explicit line range against file length.
    if let Some(start) = context.start_line
        && start > line_count
    {
        return format!(
            "{path} [error: requested range (lines {start}-{}) exceeds file length ({line_count} lines)]",
            context.end_line.unwrap_or(start),
        );
    }

    if let Some(around_line) = context.around_line {
        if around_line > line_count {
            return format!(
                "{path} [error: around_line={around_line} exceeds file length ({line_count} lines)]",
            );
        }
        return render_numbered_around_line_excerpt(
            &lines,
            around_line,
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

    if !context.show_line_numbers && !context.header {
        return match (context.start_line, context.end_line) {
            (None, None) => content.into_owned(),
            (start, end) => render_raw_line_slice(&lines, start, end),
        };
    }

    render_ordinary_read(
        path,
        &lines,
        context.start_line,
        context.end_line,
        context.show_line_numbers,
        context.header,
    )
}

fn render_raw_line_slice(lines: &[&str], start_line: Option<u32>, end_line: Option<u32>) -> String {
    slice_lines(lines, start_line, end_line)
        .into_iter()
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_ordinary_read(
    path: &str,
    lines: &[&str],
    start_line: Option<u32>,
    end_line: Option<u32>,
    show_line_numbers: bool,
    header: bool,
) -> String {
    let selected = slice_lines(lines, start_line, end_line);
    let body = if show_line_numbers {
        selected
            .iter()
            .map(|(line_number, line)| format!("{line_number}: {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        selected
            .iter()
            .map(|(_, line)| *line)
            .collect::<Vec<_>>()
            .join("\n")
    };

    if !header {
        return body;
    }

    let header_line = if start_line.is_some() || end_line.is_some() {
        render_ordinary_read_header(path, &selected)
    } else {
        path.to_string()
    };

    if body.is_empty() {
        header_line
    } else {
        format!("{header_line}\n{body}")
    }
}

fn slice_lines<'a>(
    lines: &'a [&'a str],
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> Vec<(u32, &'a str)> {
    let start_idx = start_line
        .map(|start| start.saturating_sub(1) as usize)
        .unwrap_or(0);
    let end_idx = end_line.map(|end| end as usize).unwrap_or(usize::MAX);

    lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            if idx >= start_idx && idx < end_idx {
                Some((idx as u32 + 1, *line))
            } else {
                None
            }
        })
        .collect()
}

fn render_ordinary_read_header(path: &str, selected: &[(u32, &str)]) -> String {
    match (selected.first(), selected.last()) {
        (Some((first, _)), Some((last, _))) => format!("{path} [lines {first}-{last}]"),
        _ => format!("{path} [lines empty]"),
    }
}

fn render_numbered_chunk_excerpt(file: &IndexedFile, chunk_index: u32, max_lines: u32) -> String {
    let content = String::from_utf8_lossy(&file.content);
    let lines: Vec<&str> = content.lines().collect();
    let chunk_size = max_lines as usize;

    if chunk_index == 0 || chunk_size == 0 {
        return out_of_range_file_chunk(&file.relative_path, chunk_index, 0);
    }

    let total_chunks = lines.len().div_ceil(chunk_size);
    if total_chunks == 0 {
        return out_of_range_file_chunk(&file.relative_path, chunk_index, 0);
    }

    let chunk_number = chunk_index as usize;
    if chunk_number > total_chunks {
        return out_of_range_file_chunk(&file.relative_path, chunk_index, total_chunks);
    }

    let start_idx = (chunk_number - 1) * chunk_size;
    let end_idx = (start_idx + chunk_size).min(lines.len());
    let start_line = start_idx + 1;
    let end_line = end_idx;

    let body = lines[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(offset, line)| format!("{}: {line}", start_line + offset))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "{} [chunk {}/{}, lines {}-{}]\n{}",
        file.relative_path, chunk_index, total_chunks, start_line, end_line, body
    )
}

fn render_numbered_around_symbol_excerpt(
    file: &IndexedFile,
    around_symbol: &str,
    symbol_line: Option<u32>,
    context_lines: u32,
    max_lines: Option<u32>,
) -> String {
    let content = String::from_utf8_lossy(&file.content);
    let lines: Vec<&str> = content.lines().collect();

    match resolve_around_symbol_range(file, around_symbol, symbol_line) {
        Ok((sym_start, sym_end)) => render_numbered_symbol_range_excerpt(
            &lines,
            sym_start,
            sym_end,
            context_lines,
            max_lines,
        ),
        Err(AroundSymbolResolutionError::NotFound) => {
            render_not_found_symbol(&file.relative_path, &file.symbols, around_symbol)
        }
        Err(AroundSymbolResolutionError::SelectorNotFound(symbol_line)) => {
            format!(
                "Symbol not found in {}: {} at line {}",
                file.relative_path, around_symbol, symbol_line
            )
        }
        Err(AroundSymbolResolutionError::Ambiguous(candidate_lines)) => {
            let candidate_lines = candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Ambiguous symbol selector for {around_symbol} in {}; pass `symbol_line` to disambiguate. Candidates: {candidate_lines}",
                file.relative_path
            )
        }
    }
}

/// Render a numbered excerpt covering the full symbol range `sym_start..=sym_end`
/// (1-indexed inclusive), extended by `context_lines` on each side.
/// When `max_lines` is set and the total exceeds it, truncate with a hint.
fn render_numbered_symbol_range_excerpt(
    lines: &[&str],
    sym_start: u32,
    sym_end: u32,
    context_lines: u32,
    max_lines: Option<u32>,
) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let total = lines.len();
    let start = (sym_start as usize)
        .saturating_sub(context_lines as usize)
        .max(1);
    let end = ((sym_end as usize).saturating_add(context_lines as usize)).min(total);

    if start > end || start > total {
        return String::new();
    }

    let full_range_len = end - start + 1;

    if let Some(ml) = max_lines {
        let ml = ml as usize;
        if ml > 0 && full_range_len > ml {
            let truncated_end = start + ml - 1;
            let mut result: Vec<String> = (start..=truncated_end)
                .map(|n| format!("{n}: {}", lines[n - 1]))
                .collect();
            result.push(format!(
                "... truncated (symbol is {} lines, showing first {})",
                sym_end.saturating_sub(sym_start) + 1,
                ml
            ));
            return result.join("\n");
        }
    }

    (start..=end)
        .map(|n| format!("{n}: {}", lines[n - 1]))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, PartialEq, Eq)]
enum AroundSymbolResolutionError {
    NotFound,
    SelectorNotFound(u32),
    Ambiguous(Vec<u32>),
}

/// Resolve an `around_symbol` selector to the symbol's full 1-indexed line range
/// `(start_line, end_line)`.  Both bounds are inclusive.
fn resolve_around_symbol_range(
    file: &IndexedFile,
    around_symbol: &str,
    symbol_line: Option<u32>,
) -> Result<(u32, u32), AroundSymbolResolutionError> {
    let matching_symbols: Vec<&crate::domain::SymbolRecord> = file
        .symbols
        .iter()
        .filter(|symbol| symbol.name == around_symbol)
        .collect();

    if matching_symbols.is_empty() {
        return Err(AroundSymbolResolutionError::NotFound);
    }

    if let Some(symbol_line) = symbol_line {
        // symbol_line is 1-based (from search_symbols output); line_range is 0-based.
        let exact_matches: Vec<&crate::domain::SymbolRecord> = matching_symbols
            .iter()
            .copied()
            .filter(|symbol| symbol.line_range.0 + 1 == symbol_line)
            .collect();

        return match exact_matches.as_slice() {
            [symbol] => Ok((
                symbol.line_range.0.saturating_add(1),
                symbol.line_range.1.saturating_add(1),
            )),
            [] => Err(AroundSymbolResolutionError::SelectorNotFound(symbol_line)),
            _ => Err(AroundSymbolResolutionError::Ambiguous(
                dedup_symbol_candidate_lines(&exact_matches),
            )),
        };
    }

    match matching_symbols.as_slice() {
        [symbol] => Ok((
            symbol.line_range.0.saturating_add(1),
            symbol.line_range.1.saturating_add(1),
        )),
        _ => Err(AroundSymbolResolutionError::Ambiguous(
            dedup_symbol_candidate_lines(&matching_symbols),
        )),
    }
}

fn dedup_symbol_candidate_lines(symbols: &[&crate::domain::SymbolRecord]) -> Vec<u32> {
    let mut candidate_lines: Vec<u32> = symbols.iter().map(|symbol| symbol.line_range.0).collect();
    candidate_lines.sort_unstable();
    candidate_lines.dedup();
    candidate_lines
}

fn render_numbered_around_match_excerpt(
    file: &IndexedFile,
    around_match: &str,
    match_occurrence: u32,
    context_lines: u32,
) -> String {
    let content = String::from_utf8_lossy(&file.content);
    let lines: Vec<&str> = content.lines().collect();

    let candidate_lines = find_case_insensitive_match_lines(&lines, around_match);
    if candidate_lines.is_empty() {
        return not_found_file_match(&file.relative_path, around_match);
    }

    let occurrence_index = match_occurrence.saturating_sub(1) as usize;
    let Some(&around_line) = candidate_lines.get(occurrence_index) else {
        let available_lines = candidate_lines
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        return format!(
            "Match occurrence {match_occurrence} for '{around_match}' not found in {}; {} match(es) available at lines {available_lines}",
            file.relative_path,
            candidate_lines.len()
        );
    };

    render_numbered_around_line_excerpt(&lines, around_line, context_lines)
}

fn find_case_insensitive_match_lines(lines: &[&str], around_match: &str) -> Vec<u32> {
    let needle = around_match.to_lowercase();

    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            line.to_lowercase()
                .contains(&needle)
                .then_some((index + 1) as u32)
        })
        .collect()
}

fn render_numbered_around_line_excerpt(
    lines: &[&str],
    around_line: u32,
    context_lines: u32,
) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let anchor = around_line.max(1) as usize;
    let context = context_lines as usize;
    let start = anchor.saturating_sub(context).max(1);
    let end = anchor.saturating_add(context).min(lines.len());

    if start > end || start > lines.len() {
        return String::new();
    }

    (start..=end)
        .map(|line_number| format!("{line_number}: {}", lines[line_number - 1]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Hard byte cap for `get_file_content` output. Anchored to the Claude Code
/// token-optimizer hook trip point (~25K tokens). ASCII source ≈ 3–4 chars/token
/// → 60 KB ≈ 15–20K tokens (well under trip). Exposed as a const for easy tuning.
pub const GET_FILE_CONTENT_MAX_BYTES: usize = 60_000;

/// Apply the hard byte cap to `get_file_content` output.
/// Returns `output` unchanged when `output.len() <= GET_FILE_CONTENT_MAX_BYTES`.
/// Otherwise truncates at the last `\n` boundary under the cap and appends a
/// footer suggesting narrower read modes. Idempotent.
pub fn cap_file_content_output(output: String) -> String {
    if output.len() <= GET_FILE_CONTENT_MAX_BYTES {
        return output;
    }
    // Reserve ~300 bytes for footer; align budget to a UTF-8 char boundary.
    let mut budget = GET_FILE_CONTENT_MAX_BYTES.saturating_sub(300);
    while budget > 0 && !output.is_char_boundary(budget) {
        budget -= 1;
    }
    let truncate_at = match output[..budget].rfind('\n') {
        Some(pos) => pos + 1, // keep the newline
        None => budget,       // no newline — truncate at char boundary
    };
    let truncated = &output[..truncate_at];
    let original_bytes = output.len();
    format!(
        "{truncated}\n[Output truncated: {original_bytes} bytes exceeds {GET_FILE_CONTENT_MAX_BYTES}-byte cap. \
Use chunk_index + max_lines, around_line, around_match, or around_symbol to read a smaller window.]"
    )
}

/// "File not found: {path}"
pub fn not_found_file(path: &str) -> String {
    format!("File not found: {path}")
}

/// Richer "file not found" with suggested similar paths.
/// Call this from tool handlers where the index is available.
pub fn not_found_file_with_suggestions(path: &str, suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        format!("File not found: {path}. Use search_files to find the correct path.")
    } else {
        let top: Vec<&str> = suggestions.iter().take(5).map(|s| s.as_str()).collect();
        format!("File not found: {path}. Did you mean: {}?", top.join(", "))
    }
}

/// "No matches for '{query}' in {path}"
pub fn not_found_file_match(path: &str, query: &str) -> String {
    format!("No matches for '{query}' in {path}")
}

fn out_of_range_file_chunk(path: &str, chunk_index: u32, total_chunks: usize) -> String {
    format!("Chunk {chunk_index} out of range for {path} ({total_chunks} chunks)")
}

/// "No symbol {name} in {path}. Close matches: {top 5 fuzzy matches}. Use get_file_context with sections=['outline'] for the full list."
pub fn not_found_symbol(index: &LiveIndex, path: &str, name: &str) -> String {
    match index.capture_shared_file(path) {
        None => not_found_file(path),
        Some(file) => render_not_found_symbol(&file.relative_path, &file.symbols, name),
    }
}

fn render_not_found_symbol(
    relative_path: &str,
    symbols: &[crate::domain::SymbolRecord],
    name: &str,
) -> String {
    let symbol_names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
    not_found_symbol_names(relative_path, &symbol_names, name)
}

/// Simple edit-distance score for fuzzy matching (lower is closer).
fn fuzzy_distance(a: &str, b: &str) -> usize {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    // Substring match gets highest priority (distance 0).
    if b_lower.contains(&a_lower) || a_lower.contains(&b_lower) {
        return 0;
    }

    // Prefix match gets second priority.
    let prefix_len = a_lower
        .chars()
        .zip(b_lower.chars())
        .take_while(|(x, y)| x == y)
        .count();
    if prefix_len > 0 {
        return a.len().max(b.len()) - prefix_len;
    }

    // Fall back to simple character overlap distance.
    let a_chars: std::collections::HashSet<char> = a_lower.chars().collect();
    let b_chars: std::collections::HashSet<char> = b_lower.chars().collect();
    let intersection = a_chars.intersection(&b_chars).count();
    if intersection == 0 {
        return usize::MAX;
    }
    a.len().max(b.len()) - intersection
}

fn not_found_symbol_names(relative_path: &str, symbol_names: &[String], name: &str) -> String {
    if symbol_names.is_empty() {
        return format!(
            "No symbol {name} in {relative_path}. \
             This file has no indexed symbols — it may use top-level statements, \
             expression-bodied code, or a syntax not extracted by the parser. \
             Use get_file_content without around_symbol to read the raw file."
        );
    }

    // Rank by fuzzy distance and take top 5.
    // Filter out very short names (1-2 chars like "i", "d") that are usually
    // loop variables and produce unhelpful suggestions.
    let min_name_len = 2.min(name.len());
    let mut scored: Vec<(&String, usize)> = symbol_names
        .iter()
        .filter(|s| s.len() >= min_name_len)
        .map(|s| (s, fuzzy_distance(name, s)))
        .collect();
    scored.sort_by_key(|(_, d)| *d);

    let close_matches: Vec<&str> = scored
        .iter()
        .take(5)
        .filter(|(_, d)| *d < usize::MAX)
        .map(|(s, _)| s.as_str())
        .collect();

    if close_matches.is_empty() {
        format!(
            "No symbol {name} in {relative_path}. No close matches found. \
             Use get_file_context with sections=['outline'] to see all {} symbols in this file.",
            symbol_names.len()
        )
    } else {
        format!(
            "No symbol {name} in {relative_path}. Close matches: {}. \
             Use get_file_context with sections=['outline'] for the full list ({} symbols).",
            close_matches.join(", "),
            symbol_names.len()
        )
    }
}

/// Find all references for a name across the repo, grouped by file with 3-line context.
///
/// kind_filter: "call" | "import" | "type_usage" | "all" | None (all)
/// Output format matches CONTEXT.md decision AD-6 (compact human-readable).
pub fn find_references_result(index: &LiveIndex, name: &str, kind_filter: Option<&str>) -> String {
    let limits = OutputLimits::default();
    let view = index.capture_find_references_view(name, kind_filter, limits.total_hits);
    find_references_result_view(&view, name, &limits)
}

pub fn find_references_result_view(
    view: &FindReferencesView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.total_refs == 0 {
        return format!("No references found for \"{name}\"");
    }

    let total = view.total_refs;
    let total_files = view.total_files;
    let shown_files = view.files.len().min(limits.max_files);
    let mut lines = if shown_files < total_files {
        vec![format!(
            "{total} references across {total_files} files (showing {shown_files})  [1.00]"
        )]
    } else {
        vec![format!("{total} references in {total_files} files  [1.00]")]
    };
    if view.total_refs > 50 {
        lines.push(format!(
            "Note: '{}' is a very common identifier — results may include unrelated symbols. \
             Add path or symbol_kind to scope the search.",
            name
        ));
    }
    lines.push(String::new()); // blank line

    let mut total_emitted = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if total_emitted >= limits.total_hits {
            break;
        }
        lines.push(file.file_path.clone());
        let mut hit_count = 0usize;
        let mut truncated_hits = 0usize;
        for hit in &file.hits {
            if hit_count >= limits.max_per_file || total_emitted >= limits.total_hits {
                truncated_hits += 1;
                continue;
            }
            for line in &hit.context_lines {
                if line.is_reference_line {
                    if let Some(annotation) = &line.enclosing_annotation {
                        lines.push(format!(
                            "  {}: {:<40}{}",
                            line.line_number, line.text, annotation
                        ));
                    } else {
                        lines.push(format!("  {}: {}", line.line_number, line.text));
                    }
                } else {
                    lines.push(format!("  {}: {}", line.line_number, line.text));
                }
            }
            hit_count += 1;
            total_emitted += 1;
        }
        if truncated_hits > 0 {
            lines.push(format!("  ... and {truncated_hits} more references"));
        }
        lines.push(String::new()); // blank line between files
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

/// Render a compact find_references result: file:line [kind] in symbol — no source text.
pub fn find_references_compact_view(
    view: &FindReferencesView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.total_refs == 0 {
        return format!("No references found for \"{name}\"");
    }

    let total_files = view.total_files;
    let shown_files = view.files.len().min(limits.max_files);
    let mut lines = if shown_files < total_files {
        vec![format!(
            "{} references to \"{}\" across {} files (showing {})",
            view.total_refs, name, total_files, shown_files
        )]
    } else {
        vec![format!(
            "{} references to \"{}\" in {} files",
            view.total_refs, name, total_files
        )]
    };
    if view.total_refs > 50 {
        lines.push(format!(
            "Note: '{}' is a very common identifier — results may include unrelated symbols. \
             Add path or symbol_kind to scope the search.",
            name
        ));
    }

    let mut total_emitted = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if total_emitted >= limits.total_hits {
            break;
        }
        lines.push(file.file_path.clone());
        let mut hit_count = 0usize;
        let mut truncated_hits = 0usize;
        for hit in &file.hits {
            if hit_count >= limits.max_per_file || total_emitted >= limits.total_hits {
                truncated_hits += 1;
                continue;
            }
            for line in &hit.context_lines {
                if line.is_reference_line {
                    let annotation = line.enclosing_annotation.as_deref().unwrap_or("");
                    lines.push(format!("  :{} {}", line.line_number, annotation));
                }
            }
            hit_count += 1;
            total_emitted += 1;
        }
        if truncated_hits > 0 {
            lines.push(format!("  ... and {truncated_hits} more"));
        }
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    lines.join("\n")
}

/// Format results of `find_references` implementations mode.
pub fn implementations_result_view(
    view: &ImplementationsView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.entries.is_empty() {
        return format!("No implementations found for \"{name}\"");
    }

    let total = view.entries.len();
    let shown = total.min(limits.max_files * limits.max_per_file);
    let mut lines = vec![format!("{total} implementation(s) found for \"{name}\"")];
    lines.push(String::new());

    // Group by trait name for readable output
    let mut current_trait: Option<&str> = None;
    for (i, entry) in view.entries.iter().enumerate() {
        if i >= shown {
            break;
        }
        if current_trait != Some(&entry.trait_name) {
            if current_trait.is_some() {
                lines.push(String::new());
            }
            lines.push(format!("trait/interface {}:", entry.trait_name));
            current_trait = Some(&entry.trait_name);
        }
        lines.push(format!(
            "  {} ({}:{})",
            entry.implementor,
            entry.file_path,
            entry.line + 1
        ));
    }

    let remaining = total.saturating_sub(shown);
    if remaining > 0 {
        lines.push(String::new());
        lines.push(format!("... and {remaining} more"));
    }

    lines.join("\n")
}

/// Find all files that import (depend on) the given path.
///
/// Output format: compact list grouped by importing file, each with import line.
pub fn find_dependents_result(index: &LiveIndex, path: &str) -> String {
    let view = index.capture_find_dependents_view(path);
    find_dependents_result_view(&view, path, &OutputLimits::default())
}

pub fn find_dependents_result_view(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!(
            "No file-level dependents found for \"{path}\"\nTip: use find_references(name=\"<symbol>\", path=\"{path}\") for symbol-level callers/usages."
        );
    }

    let total_files = view.files.len();
    let shown_files = total_files.min(limits.max_files);
    let mut lines = vec![
        format!("File-level dependency graph: {total_files} files depend on {path}"),
        "Need symbol-level callers/usages instead? Use find_references(name=\"<symbol>\", path=\"<file>\")."
            .to_string(),
    ];
    lines.push(String::new()); // blank line

    for file in view.files.iter().take(limits.max_files) {
        lines.push(file.file_path.clone());
        let total_refs = file.lines.len();
        let shown_refs = total_refs.min(limits.max_per_file);
        for line in file.lines.iter().take(limits.max_per_file) {
            lines.push(format!(
                "  {}: {}   [{}]",
                line.line_number, line.line_content, line.kind
            ));
        }
        let remaining_refs = total_refs.saturating_sub(shown_refs);
        if remaining_refs > 0 {
            lines.push(format!("  ... and {remaining_refs} more references"));
        }
        lines.push(String::new()); // blank line between files
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    // Remove trailing blank line
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

/// Render a compact find_dependents result: file:line [kind] without source text.
pub fn find_dependents_compact_view(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!(
            "No file-level dependents found for \"{path}\"\nTip: use find_references(name=\"<symbol>\", path=\"{path}\") for symbol-level callers/usages."
        );
    }

    let total_files = view.files.len();
    let shown_files = total_files.min(limits.max_files);
    let mut lines = vec![
        format!("File-level dependency graph: {total_files} files depend on {path}"),
        "Need symbol-level callers/usages instead? Use find_references(name=\"<symbol>\", path=\"<file>\")."
            .to_string(),
    ];

    for file in view.files.iter().take(limits.max_files) {
        let total_refs = file.lines.len();
        let shown_refs = total_refs.min(limits.max_per_file);
        let kinds: Vec<&str> = file
            .lines
            .iter()
            .take(limits.max_per_file)
            .map(|l| l.kind.as_str())
            .collect();
        let summary = if kinds.is_empty() {
            file.file_path.clone()
        } else {
            let unique_kinds: Vec<&str> = {
                let mut k = kinds.clone();
                k.sort_unstable();
                k.dedup();
                k
            };
            format!(
                "  {}  ({} refs: {})",
                file.file_path,
                total_refs,
                unique_kinds.join(", ")
            )
        };
        lines.push(summary);
        let remaining = total_refs.saturating_sub(shown_refs);
        if remaining > 0 {
            // still count but don't show individual lines
        }
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    lines.join("\n")
}

/// Render a find_dependents result as a Mermaid flowchart.
pub fn find_dependents_mermaid(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let mut lines = vec!["flowchart LR".to_string()];
    let target_id = mermaid_node_id(path);
    lines.push(format!("    {target_id}[\"{path}\"]"));

    for file in view.files.iter().take(limits.max_files) {
        let dep_id = mermaid_node_id(&file.file_path);
        let ref_count = file.lines.len();

        let mut names: Vec<&str> = Vec::new();
        for line in &file.lines {
            if !names.contains(&line.name.as_str()) {
                names.push(&line.name);
                if names.len() >= 3 {
                    break;
                }
            }
        }
        let remaining = ref_count.saturating_sub(names.len());
        let label = if names.is_empty() {
            format!("{ref_count} refs")
        } else if remaining > 0 {
            format!("{} +{remaining}", names.join(", "))
        } else {
            names.join(", ")
        };
        lines.push(format!(
            "    {dep_id}[\"{}\"] -->|\"{label}\"| {target_id}",
            file.file_path
        ));
    }

    let remaining = view.files.len().saturating_sub(limits.max_files);
    if remaining > 0 {
        lines.push(format!(
            "    more[\"... and {remaining} more files\"] --> {target_id}"
        ));
    }

    lines.join("\n")
}

/// Render a find_dependents result as a Graphviz DOT digraph.
pub fn find_dependents_dot(view: &FindDependentsView, path: &str, limits: &OutputLimits) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let mut lines = vec!["digraph dependents {".to_string()];
    lines.push("    rankdir=LR;".to_string());
    lines.push(format!(
        "    \"{}\" [shape=box, style=bold];",
        dot_escape(path)
    ));

    for file in view.files.iter().take(limits.max_files) {
        let mut names: Vec<&str> = Vec::new();
        for line in &file.lines {
            if !names.contains(&line.name.as_str()) {
                names.push(&line.name);
                if names.len() >= 3 {
                    break;
                }
            }
        }
        let remaining = file.lines.len().saturating_sub(names.len());
        let label = if names.is_empty() {
            format!("{} refs", file.lines.len())
        } else if remaining > 0 {
            format!("{} +{remaining}", names.join(", "))
        } else {
            names.join(", ")
        };
        lines.push(format!(
            "    \"{}\" -> \"{}\" [label=\"{}\"];",
            dot_escape(&file.file_path),
            dot_escape(path),
            label
        ));
    }

    let remaining = view.files.len().saturating_sub(limits.max_files);
    if remaining > 0 {
        lines.push(format!(
            "    \"... and {} more\" -> \"{}\" [style=dashed];",
            remaining,
            dot_escape(path)
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Sanitize a file path into a valid Mermaid node ID (alphanumeric + underscores).
fn mermaid_node_id(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Escape a string for DOT label/node usage.
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Get full context bundle for a symbol: definition body + callers + callees + type usages.
///
/// Each section is capped at 20 entries with "...and N more" overflow.
pub fn context_bundle_result(
    index: &LiveIndex,
    path: &str,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    let view = index.capture_context_bundle_view(path, name, kind_filter, None);
    context_bundle_result_view(&view, "full")
}

pub fn context_bundle_result_view(view: &ContextBundleView, verbosity: &str) -> String {
    context_bundle_result_view_with_max_tokens(view, verbosity, None)
}

pub fn context_bundle_impl_suggestion_tip(view: &ContextBundleView) -> String {
    match view {
        ContextBundleView::Found(view) => format_impl_block_suggestions(view.as_ref()),
        _ => String::new(),
    }
}

/// Extract a compact callees section from a context bundle view for use in
/// default `get_symbol_context` mode (which otherwise only shows callers).
pub fn context_bundle_callees_text(view: &ContextBundleView) -> String {
    match view {
        ContextBundleView::Found(found) if found.callees.total_count > 0 => {
            format_context_bundle_section("Callees", &found.callees)
        }
        _ => String::new(),
    }
}

pub fn context_bundle_result_view_with_max_tokens(
    view: &ContextBundleView,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> String {
    match view {
        ContextBundleView::FileNotFound { path } => not_found_file(path),
        ContextBundleView::AmbiguousSymbol {
            path,
            name,
            candidate_lines,
        } => format!(
            "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {}",
            candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ContextBundleView::SymbolNotFound {
            relative_path,
            symbol_names,
            name,
        } => not_found_symbol_names(relative_path, symbol_names, name),
        ContextBundleView::Found(view) => {
            render_context_bundle_found_with_max_tokens(view.as_ref(), verbosity, max_tokens)
        }
    }
}

fn render_context_bundle_found_with_max_tokens(
    view: &ContextBundleFoundView,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> String {
    let (body, actual_level) = resolve_verbosity(
        &view.body,
        Some(verbosity),
        max_tokens,
        0.4, // allocate 40% of budget to body, leave room for deps
    );
    let mut output = format!(
        "{}\n[{}, {}:{}-{}, {} bytes]\n",
        body,
        view.kind_label,
        view.file_path,
        view.line_range.0 + 1,
        view.line_range.1 + 1,
        view.byte_count
    );
    if actual_level != "full" && actual_level != verbosity {
        output.push_str(&format!(
            "[adaptive verbosity: {} — body reduced to fit {} token budget]\n",
            actual_level,
            max_tokens.unwrap_or(0)
        ));
    }
    output.push_str(&format_context_bundle_section("Callers", &view.callers));
    output.push_str(&format_context_bundle_section("Callees", &view.callees));
    output.push_str(&format_context_bundle_section(
        "Type usages",
        &view.type_usages,
    ));
    match max_tokens {
        Some(max_tokens) => {
            let max_bytes = (max_tokens as usize).saturating_mul(4);
            if max_bytes > 0 && output.len() > max_bytes {
                let mut truncated = truncate_text_at_line_boundary(&output, max_bytes);
                truncated.push_str(&format_bundle_truncation_notice(max_tokens, None));
                if !view.implementation_suggestions.is_empty() {
                    truncated.push_str(&format_impl_block_suggestions(view));
                }
                return truncated;
            }

            let (dep_text, omitted) =
                format_type_dependencies_with_budget(&view.dependencies, max_bytes, output.len());
            output.push_str(&dep_text);
            if omitted > 0 {
                output.push_str(&format_bundle_truncation_notice(max_tokens, Some(omitted)));
            }
        }
        None => {
            if !view.dependencies.is_empty() {
                output.push_str(&format_type_dependencies(&view.dependencies));
            }
        }
    }
    if !view.implementation_suggestions.is_empty() {
        output.push_str(&format_impl_block_suggestions(view));
    }
    output
}

/// Format results of `trace_symbol`.
pub fn trace_symbol_result_view(
    view: &crate::live_index::TraceSymbolView,
    name: &str,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> String {
    match view {
        crate::live_index::TraceSymbolView::FileNotFound { path } => not_found_file(path),
        crate::live_index::TraceSymbolView::AmbiguousSymbol {
            path,
            name,
            candidate_lines,
        } => format!(
            "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {}",
            candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        crate::live_index::TraceSymbolView::SymbolNotFound {
            relative_path,
            symbol_names,
            name,
        } => not_found_symbol_names(relative_path, symbol_names, name),
        crate::live_index::TraceSymbolView::Found(found) => {
            let mut output =
                render_context_bundle_found_with_max_tokens(&found.context_bundle, verbosity, max_tokens);

            if !found.siblings.is_empty() {
                output.push_str(&format_siblings(&found.siblings, 0));
            }

            if !found.dependents.files.is_empty() {
                output.push_str("\n\n");
                let dependents_fn = if verbosity == "full" {
                    find_dependents_result_view
                } else {
                    find_dependents_compact_view
                };
                output.push_str(&dependents_fn(
                    &found.dependents,
                    &found.context_bundle.file_path,
                    &OutputLimits::default(),
                ));
            }

            if !found.implementations.entries.is_empty() {
                output.push_str("\n\n");
                output.push_str(&implementations_result_view(
                    &found.implementations,
                    name,
                    &OutputLimits::default(),
                ));
            }

            if let Some(git) = &found.git_activity {
                output.push_str(&format_trace_git_activity(git));
            }

            output
        }
    }
}

fn format_siblings(siblings: &[crate::live_index::SiblingSymbolView], overflow: usize) -> String {
    let mut lines = vec!["\nNearby siblings:".to_string()];
    for sib in siblings {
        lines.push(format!(
            "  {:<12} {:<30} {}-{}",
            sib.kind_label, sib.name, sib.line_range.0, sib.line_range.1
        ));
    }
    if overflow > 0 {
        lines.push(format!("  ... and {overflow} more siblings"));
    }
    lines.join("\n")
}

fn format_trace_git_activity(git: &crate::live_index::GitActivityView) -> String {
    let mut lines = vec![String::new()];
    lines.push(format!(
        "Git activity:  {} {:.2} ({})    {} commits, last {}",
        git.churn_bar, git.churn_score, git.churn_label, git.commit_count, git.last_relative,
    ));
    lines.push(format!(
        "  Last:  {} \"{}\" ({}, {})",
        git.last_hash, git.last_message, git.last_author, git.last_timestamp,
    ));
    if !git.owners.is_empty() {
        lines.push(format!("  Owners: {}", git.owners.join(", ")));
    }
    if !git.co_changes.is_empty() {
        lines.push("  Co-changes:".to_string());
        for (path, coupling, shared) in &git.co_changes {
            lines.push(format!(
                "    {}  ({:.2} coupling, {} shared commits)",
                path, coupling, shared,
            ));
        }
    }
    lines.join("\n")
}

/// Format results of `inspect_match`.
pub fn inspect_match_result_view(view: &InspectMatchView) -> String {
    match view {
        InspectMatchView::FileNotFound { path } => not_found_file(path),
        InspectMatchView::LineOutOfBounds {
            path,
            line,
            total_lines,
        } => {
            format!("Line {line} is out of bounds for {path} (file has {total_lines} lines).")
        }
        InspectMatchView::Found(found) => {
            let mut output = String::new();

            // 1. Excerpt
            output.push_str(&found.excerpt);
            output.push('\n');

            // 2. Parent chain (shows full nesting context when deeper than 1 level)
            if found.parent_chain.len() > 1 {
                output.push_str("\nScope: ");
                let chain: Vec<String> = found
                    .parent_chain
                    .iter()
                    .map(|p| format!("{} {}", p.kind_label, p.name))
                    .collect();
                output.push_str(&chain.join(" → "));
            }

            // 3. Enclosing symbol (deepest)
            if let Some(enclosing) = &found.enclosing {
                output.push_str(&format_enclosing(enclosing));
            } else {
                output.push_str("\n(No enclosing symbol)");
            }

            // 4. Siblings
            if !found.siblings.is_empty() || found.siblings_overflow > 0 {
                output.push_str(&format_siblings(&found.siblings, found.siblings_overflow));
            }

            output
        }
    }
}

fn format_enclosing(enclosing: &crate::live_index::EnclosingSymbolView) -> String {
    format!(
        "\nEnclosing symbol: {} {} (lines {}-{})",
        enclosing.kind_label, enclosing.name, enclosing.line_range.0, enclosing.line_range.1
    )
}

fn format_context_bundle_section(title: &str, section: &ContextBundleSectionView) -> String {
    // Detect if this section has deduplicated entries (any occurrence_count > 1).
    let has_dedup = section.entries.iter().any(|e| e.occurrence_count > 1);

    let header =
        if has_dedup && section.unique_count > 0 && section.unique_count < section.total_count {
            format!(
                "\n{title} ({} total, {} unique):",
                section.total_count, section.unique_count
            )
        } else {
            format!("\n{title} ({}):", section.total_count)
        };

    let mut lines = vec![header];

    let mut external_count = 0usize;

    for entry in &section.entries {
        if is_external_symbol(&entry.display_name, &entry.file_path) {
            external_count += 1;
        }

        // Build the name part, appending ×N for deduplicated entries.
        let name_part = if entry.occurrence_count > 1 {
            format!("{} (×{})", entry.display_name, entry.occurrence_count)
        } else {
            entry.display_name.clone()
        };

        if let Some(enclosing) = &entry.enclosing {
            lines.push(format!(
                "  {:<30} {}:{}  {}",
                name_part, entry.file_path, entry.line_number, enclosing
            ));
        } else {
            lines.push(format!(
                "  {:<30} {}:{}",
                name_part, entry.file_path, entry.line_number
            ));
        }
    }

    if section.overflow_count > 0 {
        // Estimate external ratio from shown entries and extrapolate
        let shown = section.entries.len();
        let est_external = if shown > 0 {
            (external_count as f64 / shown as f64 * section.overflow_count as f64).round() as usize
        } else {
            0
        };
        let est_project = section.overflow_count.saturating_sub(est_external);
        if has_dedup {
            // For deduplicated sections, overflow is in unique callee names
            lines.push(format!(
                "  ...and {} more unique {}",
                section.overflow_count,
                title.to_lowercase()
            ));
        } else if est_external > 0 {
            lines.push(format!(
                "  ...and {} more {} ({} project, ~{} stdlib/framework)",
                section.overflow_count,
                title.to_lowercase(),
                est_project,
                est_external
            ));
        } else {
            lines.push(format!(
                "  ...and {} more {}",
                section.overflow_count,
                title.to_lowercase()
            ));
        }
    }

    lines.join("\n")
}

/// Heuristic: classify a symbol reference as external (stdlib/framework) vs project-defined.
fn is_external_symbol(name: &str, file_path: &str) -> bool {
    // No file path means it's a builtin/external
    if file_path.is_empty() {
        return true;
    }
    // Common stdlib/framework patterns across languages
    let external_prefixes = [
        "std::",
        "core::",
        "alloc::",
        "System.",
        "Microsoft.",
        "java.",
        "javax.",
        "kotlin.",
        "android.",
        "console.",
        "JSON.",
        "Math.",
        "Object.",
        "Array.",
        "String.",
        "Promise.",
        "Map.",
        "Set.",
        "Error.",
    ];
    for prefix in &external_prefixes {
        if name.starts_with(prefix) {
            return true;
        }
    }
    // Single-word lowercase names that are very common builtins
    let common_builtins = [
        "println",
        "print",
        "eprintln",
        "format",
        "vec",
        "to_string",
        "clone",
        "unwrap",
        "expect",
        "push",
        "pop",
        "len",
        "is_empty",
        "iter",
        "map",
        "filter",
        "collect",
        "into",
        "from",
        "default",
        "new",
        "Add",
        "Sub",
        "Display",
        "Debug",
        "ToString",
        "log",
        "warn",
        "error",
        "info",
        "LogWarning",
        "LogError",
        "LogInformation",
        "Console",
    ];
    common_builtins.contains(&name)
}

/// Extract the full signature from a symbol body.
///
/// Handles common patterns: `fn foo(...)`, `pub struct Foo`, `class Bar`, etc.
/// Skips leading doc comments, then collects lines until the declaration is
/// complete (opening brace `{`, `where` clause, or terminating `;`).
/// Multi-line signatures are joined on one line with spaces, preserving
/// visibility, generic parameters, and return type.
fn extract_signature(body: &str) -> String {
    let mut sig_lines: Vec<&str> = Vec::new();
    let mut in_sig = false;

    for line in body.lines() {
        let trimmed = line.trim();

        if !in_sig {
            // Skip leading empty lines and doc/attribute comments
            if trimmed.is_empty()
                || trimmed.starts_with("///")
                || trimmed.starts_with("//!")
                || trimmed.starts_with("//")
                || trimmed.starts_with("/**")
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
                || trimmed.starts_with('#')
            {
                continue;
            }
            in_sig = true;
        }

        sig_lines.push(trimmed);

        // Stop once the signature is terminated:
        // - opens a body block: `{`
        // - `where` clause (generics constraints) — include the where line then stop at `{`
        // - declaration terminator: `;` (abstract methods, type aliases, extern fns)
        // - `=>` (match arm / single-expr lambda — stop collecting)
        if trimmed.ends_with('{')
            || trimmed.ends_with("where")
            || trimmed.ends_with(';')
            || trimmed == "{"
        {
            break;
        }
        // A line that IS just a where clause body — keep collecting until `{`
        // A plain `)` or `->` line means multi-line sig still continuing — keep going
        // But cap at 10 lines to avoid pulling in the entire body for edge cases
        if sig_lines.len() >= 10 {
            break;
        }
    }

    if sig_lines.is_empty() {
        return body.lines().next().unwrap_or("").to_string();
    }

    // Join multi-line signatures onto a single line, collapsing extra whitespace
    let joined = sig_lines.join(" ");
    // Strip trailing ` {` or ` ;` from the end — the signature line should not
    // include the opening brace or semicolon
    let result = joined
        .trim_end_matches(" {")
        .trim_end_matches('{')
        .trim_end_matches(';')
        .trim();
    result.to_string()
}

/// Extract the first doc-comment line from a symbol body.
///
/// Looks for `///`, `//!`, `/** ... */`, `# ...` (Python docstring-adjacent),
/// or `/* ... */` style comments immediately before/after the signature.
fn extract_first_doc_line(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Rust doc comments
        if let Some(rest) = trimmed.strip_prefix("///") {
            let doc = rest.trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // Rust inner doc comments
        if let Some(rest) = trimmed.strip_prefix("//!") {
            let doc = rest.trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // C-style block doc comments
        if let Some(rest) = trimmed.strip_prefix("/**") {
            let doc = rest.trim_end_matches("*/").trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // XML doc comments (C#)
        if trimmed.starts_with("/// <summary>") || trimmed.starts_with("/// <remarks>") {
            continue; // skip XML tags, look for actual text
        }
        // Python/JS docstrings
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let doc = trimmed
                .trim_start_matches("\"\"\"")
                .trim_start_matches("'''")
                .trim_end_matches("\"\"\"")
                .trim_end_matches("'''")
                .trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // If we hit a non-comment line, stop looking
        if !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
            && !trimmed.starts_with('#')
        {
            break;
        }
    }
    None
}

/// Apply verbosity filter to a symbol body.
///
/// - `"summary"`: one-line natural language summary (doc comment or heuristic from name/signature).
/// - `"signature"`: full declaration line — visibility, name, generics, params, return type (~80% smaller).
/// - `"compact"`: signature + first doc-comment line.
/// - `"full"` or anything else: complete body (default).
pub(crate) fn apply_verbosity(body: &str, verbosity: &str) -> String {
    match verbosity {
        "summary" => auto_summarize(body),
        "signature" => extract_signature(body),
        "compact" => {
            let sig = extract_signature(body);
            if let Some(doc) = extract_first_doc_line(body) {
                format!("{sig}\n  // {doc}")
            } else {
                sig
            }
        }
        _ => body.to_string(),
    }
}

/// Auto-select the richest verbosity level whose output fits within a token budget.
///
/// Cascades through `full → compact → signature → summary`, returning the first
/// level whose rendered output is ≤ `body_budget_tokens * 4` bytes.  Always returns
/// at least the summary level even if it exceeds the budget.
///
/// Returns `(rendered_body, level_name)`.
pub(crate) fn adaptive_verbosity(body: &str, body_budget_tokens: u64) -> (String, &'static str) {
    let max_bytes = (body_budget_tokens as usize).saturating_mul(4);

    // Try full first
    if body.len() <= max_bytes {
        return (body.to_string(), "full");
    }

    // Try compact
    let compact = apply_verbosity(body, "compact");
    if compact.len() <= max_bytes {
        return (compact, "compact");
    }

    // Try signature
    let signature = apply_verbosity(body, "signature");
    if signature.len() <= max_bytes {
        return (signature, "signature");
    }

    // Summary as last resort
    (apply_verbosity(body, "summary"), "summary")
}

/// Resolve verbosity for a symbol body, with adaptive fallback.
///
/// - If `explicit_verbosity` is `Some` (user chose a level), applies it directly.
/// - If `max_tokens` is `Some` and no explicit verbosity, auto-selects the richest
///   level fitting within `max_tokens * body_fraction`.
/// - Otherwise returns the full body.
///
/// `body_fraction` controls how much of the total token budget is allocated to the
/// body (e.g. 0.4 for bundle mode where dependencies need space, 0.7 for standalone).
///
/// Returns `(rendered_body, level_name)`.
pub(crate) fn resolve_verbosity(
    body: &str,
    explicit_verbosity: Option<&str>,
    max_tokens: Option<u64>,
    body_fraction: f64,
) -> (String, &'static str) {
    // Explicit user choice always wins
    if let Some(v) = explicit_verbosity {
        if v != "full" {
            let level: &'static str = match v {
                "summary" => "summary",
                "signature" => "signature",
                "compact" => "compact",
                _ => "full",
            };
            return (apply_verbosity(body, v), level);
        }
    }

    // Adaptive: auto-select when budget is set and no explicit verbosity (or explicit "full")
    if let Some(tokens) = max_tokens {
        if tokens > 0 {
            let body_budget = (tokens as f64 * body_fraction) as u64;
            let (rendered, level) = adaptive_verbosity(body, body_budget);
            if level != "full" {
                return (rendered, level);
            }
        }
    }

    // Default: full
    (body.to_string(), "full")
}

/// Generate a one-line natural language summary for a symbol body.
///
/// Priority:
/// 1. First doc-comment line (if present and meaningful)
/// 2. Heuristic from function name patterns (get_, set_, is_, new, from_, etc.)
/// 3. Signature-based fallback with parameter/return type info
fn auto_summarize(body: &str) -> String {
    // Try doc comment first
    if let Some(doc) = extract_first_doc_line(body) {
        // Ensure it's meaningful (not just a tag or very short)
        if doc.len() > 5 && !doc.starts_with('@') && !doc.starts_with('<') {
            return doc;
        }
    }

    let sig = extract_signature(body);

    // Extract the function/type name from the signature
    let name = extract_declaration_name(&sig).unwrap_or_default();
    if name.is_empty() {
        return sig;
    }

    // Try heuristic summary based on name patterns
    if let Some(heuristic) = heuristic_from_name(&name, &sig) {
        return heuristic;
    }

    // Fallback: signature-based summary
    sig
}

/// Generate a heuristic summary from common naming patterns.
fn heuristic_from_name(name: &str, sig: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();

    // Common prefixes with semantic meaning
    let patterns: &[(&str, &str)] = &[
        ("test_", "Test: "),
        ("get_", "Returns the "),
        ("set_", "Sets the "),
        ("is_", "Checks whether "),
        ("has_", "Checks whether it has "),
        ("should_", "Checks whether it should "),
        ("can_", "Checks whether it can "),
        ("with_", "Creates a copy with "),
        ("from_", "Constructs from "),
        ("into_", "Converts into "),
        ("try_", "Attempts to "),
        ("parse_", "Parses "),
        ("render_", "Renders "),
        ("format_", "Formats "),
        ("validate_", "Validates "),
        ("build_", "Builds "),
        ("create_", "Creates "),
        ("make_", "Creates "),
        ("load_", "Loads "),
        ("save_", "Saves "),
        ("read_", "Reads "),
        ("write_", "Writes "),
        ("find_", "Finds "),
        ("search_", "Searches for "),
        ("collect_", "Collects "),
        ("compute_", "Computes "),
        ("calculate_", "Calculates "),
        ("update_", "Updates "),
        ("delete_", "Deletes "),
        ("remove_", "Removes "),
        ("add_", "Adds "),
        ("insert_", "Inserts "),
        ("handle_", "Handles "),
        ("process_", "Processes "),
        ("run_", "Runs "),
        ("execute_", "Executes "),
        ("start_", "Starts "),
        ("stop_", "Stops "),
        ("init_", "Initializes "),
        ("setup_", "Sets up "),
        ("cleanup_", "Cleans up "),
        ("resolve_", "Resolves "),
        ("normalize_", "Normalizes "),
        ("convert_", "Converts "),
        ("transform_", "Transforms "),
        ("apply_", "Applies "),
        ("check_", "Checks "),
        ("ensure_", "Ensures "),
        ("spawn_", "Spawns "),
        ("emit_", "Emits "),
        ("dispatch_", "Dispatches "),
        ("register_", "Registers "),
        ("detect_", "Detects "),
        ("extract_", "Extracts "),
        ("capture_", "Captures "),
        ("record_", "Records "),
    ];

    for (prefix, verb) in patterns {
        if lower.starts_with(prefix) {
            let rest = &name[prefix.len()..];
            let readable = rest.replace('_', " ");
            let mut chars = readable.chars();
            let capitalized = match chars.next() {
                Some(first) => {
                    let mut text = first.to_uppercase().collect::<String>();
                    text.push_str(chars.as_str());
                    text
                }
                None => readable,
            };
            return Some(format!("{verb}{capitalized}"));
        }
    }

    // Special cases
    if lower == "new" || lower == "default" {
        // Check if it's inside an impl block
        if sig.contains("impl") || sig.contains("Self") || sig.contains("->") {
            return Some("Constructor".to_string());
        }
    }

    if lower == "drop" {
        return Some("Destructor / cleanup on drop".to_string());
    }

    if lower == "fmt" && sig.contains("Formatter") {
        return Some("Display/Debug formatting implementation".to_string());
    }

    // Struct/enum/type with field count
    if sig.contains("struct ") || sig.contains("class ") {
        return Some(format!("Data type: {name}"));
    }
    if sig.contains("enum ") {
        return Some(format!("Enumeration: {name}"));
    }
    if sig.contains("trait ") || sig.contains("interface ") {
        return Some(format!("Interface/trait: {name}"));
    }
    if sig.contains("impl ") {
        return Some(format!("Implementation block for {name}"));
    }

    None
}

fn format_type_dependencies(deps: &[TypeDependencyView]) -> String {
    let mut output = format!("\nDependencies ({}):", deps.len());
    for dep in deps {
        output.push_str(&format_type_dependency(dep));
    }
    output
}

fn format_type_dependencies_with_budget(
    deps: &[TypeDependencyView],
    max_bytes: usize,
    base_len: usize,
) -> (String, usize) {
    if deps.is_empty() || max_bytes == 0 {
        return (String::new(), 0);
    }

    let mut rendered = String::new();
    let header = format!("\nDependencies ({}):", deps.len());
    let mut header_added = false;
    let mut included = 0usize;

    for dep in deps {
        let dep_block = format_type_dependency(dep);
        let header_cost = if header_added { 0 } else { header.len() };
        if base_len + rendered.len() + header_cost + dep_block.len() > max_bytes {
            break;
        }
        if !header_added {
            rendered.push_str(&header);
            header_added = true;
        }
        rendered.push_str(&dep_block);
        included += 1;
    }

    if included == deps.len() {
        return (rendered, 0);
    }
    if included == 0 {
        return (String::new(), deps.len());
    }
    (rendered, deps.len().saturating_sub(included))
}

fn format_type_dependency(dep: &TypeDependencyView) -> String {
    let depth_marker = if dep.depth > 0 {
        format!(" (depth {})", dep.depth)
    } else {
        String::new()
    };
    format!(
        "\n── {} [{}, {}:{}-{}{}] ──\n{}",
        dep.name,
        dep.kind_label,
        dep.file_path,
        dep.line_range.0 + 1,
        dep.line_range.1 + 1,
        depth_marker,
        dep.body
    )
}

fn format_impl_block_suggestions(view: &ContextBundleFoundView) -> String {
    let is_type_definition = matches!(view.kind_label.as_str(), "struct" | "enum");
    if !is_type_definition
        || view.callers.total_count != 0
        || view.implementation_suggestions.is_empty()
    {
        return String::new();
    }

    let mut output = format!(
        "\nTip: This {} has 0 direct callers. Try `get_symbol_context` on one of its impl blocks:",
        view.kind_label
    );
    for suggestion in &view.implementation_suggestions {
        output.push_str(&format_impl_block_suggestion(suggestion));
    }
    output.push('\n');
    output
}

fn format_impl_block_suggestion(suggestion: &ImplBlockSuggestionView) -> String {
    format!(
        "\n- {} ({}:{})",
        suggestion.display_name, suggestion.file_path, suggestion.line_number
    )
}

fn format_bundle_truncation_notice(max_tokens: u64, omitted_dependencies: Option<usize>) -> String {
    match omitted_dependencies {
        Some(count) => format!(
            "\nTruncated at ~{max_tokens} tokens. {count} additional type dependencies not shown.\n"
        ),
        None => format!("\nTruncated at ~{max_tokens} tokens.\n"),
    }
}

/// Enforce a max-token budget on an already-assembled output string.
///
/// If the output exceeds `max_tokens * 4` bytes it is truncated at a line
/// boundary and a clear notice is appended.  Returns the original string
/// unchanged when no budget is set or the output fits within the budget.
pub fn enforce_token_budget(output: String, max_tokens: Option<u64>) -> String {
    let max_tokens = match max_tokens {
        Some(t) if t > 0 => t,
        _ => return output,
    };
    let max_bytes = (max_tokens as usize).saturating_mul(4);
    if output.len() <= max_bytes {
        return output;
    }
    let actual_tokens_est = output.len() / 4;
    let mut truncated = truncate_text_at_line_boundary(&output, max_bytes);
    truncated.push_str(&format!(
        "\n\n[truncated — output is ~{} tokens, budget is {} tokens]\n",
        actual_tokens_est, max_tokens
    ));
    truncated
}

fn truncate_text_at_line_boundary(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }

    let mut last_char_end = 0usize;
    let mut last_newline_end = None;
    for (idx, ch) in text.char_indices() {
        let char_end = idx + ch.len_utf8();
        if char_end > max_bytes {
            break;
        }
        last_char_end = char_end;
        if ch == '\n' {
            last_newline_end = Some(char_end);
        }
    }

    let end = last_newline_end.unwrap_or(last_char_end);
    text[..end].to_string()
}

/// "Index is loading... try again shortly."
pub fn loading_guard_message() -> String {
    "Index is loading... try again shortly.".to_string()
}

/// "Index not loaded. Call index_folder to index a directory."
pub fn empty_guard_message() -> String {
    "Index not loaded. Call index_folder to index a directory.".to_string()
}

/// Format a "Token Savings (this session)" section from a `StatsSnapshot`.
///
/// Input: `snap` — the `StatsSnapshot` from `TokenStats::summary()`.
/// Output: a multi-line string listing per-hook-type fire counts and token savings.
///
/// If all counters are zero, returns an empty string (no savings section shown).
/// This is a fail-open function — callers can append the result without checking emptiness.
///
/// ```text
/// ── Token Savings (this session) ──
/// Read:  N fires, ~M tokens saved
/// Edit:  N fires, ~M tokens saved
/// Write: N fires
/// Grep:  N fires, ~M tokens saved
/// Total: ~T tokens saved
/// ```
pub fn format_token_savings(snap: &StatsSnapshot) -> String {
    let total_saved = snap.read_saved_tokens + snap.edit_saved_tokens + snap.grep_saved_tokens;

    // Show section only when at least one hook has fired.
    let any_fires =
        snap.read_fires > 0 || snap.edit_fires > 0 || snap.write_fires > 0 || snap.grep_fires > 0;

    if !any_fires {
        return String::new();
    }

    let mut lines = vec!["── Token Savings (this session) ──".to_string()];

    if snap.read_fires > 0 {
        lines.push(format!(
            "Read:  {} fires, ~{} tokens saved",
            snap.read_fires, snap.read_saved_tokens
        ));
    }
    if snap.edit_fires > 0 {
        lines.push(format!(
            "Edit:  {} fires, ~{} tokens saved",
            snap.edit_fires, snap.edit_saved_tokens
        ));
    }
    if snap.write_fires > 0 {
        lines.push(format!("Write: {} fires", snap.write_fires));
    }
    if snap.grep_fires > 0 {
        lines.push(format!(
            "Grep:  {} fires, ~{} tokens saved",
            snap.grep_fires, snap.grep_saved_tokens
        ));
    }

    lines.push(format!("Total: ~{} tokens saved", total_saved));

    lines.join("\n")
}

/// Format a per-tool token breakdown section showing tokens served, saved, and efficiency ratio.
///
/// Input: `details` — sorted Vec of `(tool_name, tokens_served, tokens_saved)`.
/// Returns empty string when details is empty.
pub fn format_tool_token_breakdown(details: &[(String, u64, u64)]) -> String {
    if details.is_empty() {
        return String::new();
    }

    let total_served: u64 = details.iter().map(|(_, s, _)| s).sum();
    let total_saved: u64 = details.iter().map(|(_, _, s)| s).sum();
    let total_naive = total_served + total_saved;
    let efficiency = if total_served > 0 {
        total_naive as f64 / total_served as f64
    } else {
        1.0
    };
    let reduction_pct = if total_naive > 0 {
        (total_saved as f64 / total_naive as f64 * 100.0) as u64
    } else {
        0
    };

    let mut lines = vec![format!(
        "\u{2500}\u{2500} Session Efficiency \u{2500}\u{2500}\nTokens served: {}\nNaive equivalent: {}\nEfficiency: {:.1}x ({reduction_pct}% reduction)",
        total_served, total_naive, efficiency
    )];

    lines.push(String::new());
    lines.push("\u{2500}\u{2500} Per-Tool Breakdown \u{2500}\u{2500}".to_string());
    let max_name = details.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);
    for (name, served, saved) in details.iter().take(10) {
        let tool_naive = served + saved;
        let tool_eff = if *served > 0 {
            format!("{:.1}x", tool_naive as f64 / *served as f64)
        } else {
            "-".to_string()
        };
        lines.push(format!(
            "  {:<width$}  {} served, {} saved ({})",
            name,
            served,
            saved,
            tool_eff,
            width = max_name
        ));
    }

    lines.join("\n")
}

/// Format a "Tool Call Counts (this session)" section from per-tool invocation counts.
///
/// Input: `counts` — sorted slice of `(tool_name, count)` from `TokenStats::tool_call_counts()`.
/// Output: a multi-line string. Returns empty string when `counts` is empty.
///
/// ```text
/// ── Tool Call Counts (this session) ──
/// search_text:        12
/// get_file_context:    7
/// get_symbol:          3
/// ```
pub fn format_tool_call_counts(counts: &[(String, usize)]) -> String {
    if counts.is_empty() {
        return String::new();
    }

    let mut lines = vec!["── Tool Call Counts (this session) ──".to_string()];
    // Align counts by padding tool names to the width of the longest name.
    let max_name_len = counts.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    for (name, count) in counts {
        lines.push(format!("{:<width$}  {}", name, count, width = max_name_len));
    }

    lines.join("\n")
}

/// Estimate tokens saved by a structured response vs raw file content.
/// Returns a one-line footer string, or empty string if no meaningful savings.
pub fn compact_savings_footer(response_chars: usize, raw_chars: usize) -> String {
    if raw_chars <= response_chars || raw_chars < 200 {
        return String::new();
    }
    // Rough token estimate: ~4 chars per token for code
    let response_tokens = response_chars / 4;
    let raw_tokens = raw_chars / 4;
    let saved = raw_tokens.saturating_sub(response_tokens);
    if saved < 50 {
        return String::new();
    }
    format!("\n\n~{saved} tokens saved vs raw file read")
}

/// Format a "Hook Adoption (current session)" section from hook-time workflow counters.
pub(crate) fn format_hook_adoption(snap: &HookAdoptionSnapshot) -> String {
    if snap.is_empty() {
        return String::new();
    }

    let total = snap.total_attempts();
    let routed = snap.total_routed();
    let percent = if total == 0 {
        0
    } else {
        ((routed as f64 / total as f64) * 100.0).round() as usize
    };

    let mut lines = vec![
        "── Hook Adoption (current session) ──".to_string(),
        format!("Owned workflows routed: {routed}/{total} ({percent}%)"),
    ];

    let total_no_sidecar = snap.source_read.no_sidecar
        + snap.source_search.no_sidecar
        + snap.repo_start.no_sidecar
        + snap.prompt_context.no_sidecar
        + snap.post_edit_impact.no_sidecar;
    let total_sidecar_error = snap.source_read.sidecar_error
        + snap.source_search.sidecar_error
        + snap.repo_start.sidecar_error
        + snap.prompt_context.sidecar_error
        + snap.post_edit_impact.sidecar_error;
    let fail_open_total = snap.total_fail_open();
    if fail_open_total > 0 {
        lines.push(format!(
            "Fail-open outcomes: {fail_open_total} (no sidecar {total_no_sidecar}, sidecar errors {total_sidecar_error})"
        ));
    } else {
        lines.push("Fail-open outcomes: 0".to_string());
    }

    // Show daemon fallback total if any occurred.
    let total_daemon = snap.source_read.daemon_fallback
        + snap.source_search.daemon_fallback
        + snap.repo_start.daemon_fallback
        + snap.prompt_context.daemon_fallback
        + snap.post_edit_impact.daemon_fallback;
    if total_daemon > 0 {
        lines.push(format!("Daemon fallback routed: {total_daemon}"));
        lines.push(
            "Daemon fallback counts as routed work: the hook reached the daemon even though the sidecar was unavailable."
                .to_string(),
        );
    }

    let mut push_workflow_line =
        |label: &str, counts: &crate::cli::hook::WorkflowAdoptionCounts| {
            if counts.total() == 0 {
                return;
            }
            let mut parts = vec![format!("routed {}", counts.routed)];
            if counts.daemon_fallback > 0 {
                parts.push(format!("daemon fallback {}", counts.daemon_fallback));
            }
            if counts.fail_open() > 0 && counts.no_sidecar > 0 {
                parts.push(format!("no sidecar {}", counts.no_sidecar));
            }
            if counts.fail_open() > 0 && counts.sidecar_error > 0 {
                parts.push(format!("sidecar errors {}", counts.sidecar_error));
            }
            lines.push(format!("{label}: {}", parts.join(", ")));
        };

    push_workflow_line("Source read", &snap.source_read);
    push_workflow_line("Source search", &snap.source_search);
    push_workflow_line("Repo start", &snap.repo_start);
    push_workflow_line("Prompt context", &snap.prompt_context);
    push_workflow_line("Post-edit impact", &snap.post_edit_impact);

    if let Some(first) = snap.first_repo_start {
        lines.push(format!("First repo start: {}", first.label()));
    }

    // Show a hint when all fail-open outcomes are due to no-sidecar.
    if snap.total_fail_open() > 0 && snap.total_routed() == 0 && total_daemon == 0 {
        lines.push(String::new());
        lines.push("⚠ All hook attempts failed open (no sidecar found).".to_string());
        lines.push("  Start SymForge as an MCP server or run 'symforge daemon start'.".to_string());
    } else if fail_open_total > 0 && total_sidecar_error == 0 {
        lines.push(String::new());
        lines.push(
            "Fail-open here is mostly benign: hooks fired before a sidecar was reachable or on workflows intentionally left pass-through."
                .to_string(),
        );
    } else if total_sidecar_error > 0 {
        lines.push(String::new());
        lines.push(
            "Actionable note: sidecar errors are real routing failures and worth investigating separately from no-sidecar outcomes."
                .to_string(),
        );
    }

    lines.join("\n")
}

/// Format a compact "what next" hint line for tool outputs.
pub fn compact_next_step_hint(items: &[&str]) -> String {
    let items: Vec<&str> = items
        .iter()
        .copied()
        .filter(|item| !item.trim().is_empty())
        .collect();
    if items.is_empty() {
        return String::new();
    }
    format!("\nTip: {}", items.join(" | "))
}

/// Format a one-line git temporal summary for the health report.
pub fn git_temporal_health_line(
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
) -> String {
    use crate::live_index::git_temporal::GitTemporalState;

    match &temporal.state {
        GitTemporalState::Pending => "Git temporal: pending".to_string(),
        GitTemporalState::Computing => "Git temporal: computing...".to_string(),
        GitTemporalState::Unavailable(reason) => {
            format!("Git temporal: unavailable ({reason})")
        }
        GitTemporalState::Ready => {
            let stats = &temporal.stats;
            let mut lines = vec![format!(
                "Git temporal: ready ({} commits over {}d, computed in {}ms)",
                stats.total_commits_analyzed,
                stats.analysis_window_days,
                stats.compute_duration.as_millis(),
            )];

            if !stats.hotspots.is_empty() {
                let top: Vec<String> = stats
                    .hotspots
                    .iter()
                    .take(5)
                    .map(|(path, score)| format!("{path} ({score:.2})"))
                    .collect();
                lines.push(format!("  Hotspots: {}", top.join(", ")));
            }

            if !stats.most_coupled.is_empty() {
                let (a, b, score) = &stats.most_coupled[0];
                lines.push(format!(
                    "  Strongest coupling: {a} \u{2194} {b} ({score:.2})"
                ));
            }

            lines.join("\n")
        }
    }
}

/// Render the "Top frecent files" section for the health report.
///
/// `entries` is a pre-sorted list of `(path, decayed_score)` from
/// `FrecencyStore::top_frecent`. When empty, returns a short "no data yet"
/// line so operators can tell the section is live but unpopulated.
pub fn format_frecency_top(entries: &[(std::path::PathBuf, f64)]) -> String {
    let mut lines = vec!["── Top frecent files ──".to_string()];
    if entries.is_empty() {
        lines.push("  (no frecency rows recorded yet)".to_string());
    } else {
        for (path, score) in entries {
            lines.push(format!("  {:.2}  {}", score, path.display()));
        }
    }
    lines.join("\n")
}

/// Render the "Last 10 frecency bumps" debug section for the health report.
///
/// Gated at the call-site on `SYMFORGE_DEBUG_RANKING=1`. `entries` comes from
/// `FrecencyStore::last_10_bumps` (already ordered newest-first). Empty input
/// produces a short "no data yet" line.
pub fn format_frecency_last_bumps(
    entries: &[crate::live_index::frecency::BumpEntry],
) -> String {
    let mut lines = vec!["── Last 10 frecency bumps ──".to_string()];
    if entries.is_empty() {
        lines.push("  (no frecency rows recorded yet)".to_string());
    } else {
        for e in entries {
            lines.push(format!(
                "  ts={} hits={}  {}",
                e.last_access_ts,
                e.hit_count,
                e.path.display(),
            ));
        }
    }
    lines.join("\n")
}

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

    if depth >= 2 && !enriched_symbols.is_empty() {
        // Depth 2+: show enriched symbols with signatures
        lines.push(format!("Symbols ({} found):", symbol_hits.len()));
        for (i, (name, kind, path, signature, dependents)) in enriched_symbols.iter().enumerate() {
            let score_suffix = symbol_scores.get(i).map(|s| format!("  [{:.2}]", s)).unwrap_or_default();
            if let Some(sig) = signature {
                // Show first line of signature only to keep it compact
                let first_line = sig.lines().next().unwrap_or(sig);
                lines.push(format!("  {first_line}  [{kind}, {path}]{score_suffix}"));
            } else {
                lines.push(format!("  {kind} {name}  {path}{score_suffix}"));
            }
            if !dependents.is_empty() {
                lines.push(format!("    <- used by: {}", dependents.join(", ")));
            }
        }
        // Show remaining non-enriched symbols in compact form
        if symbol_hits.len() > enriched_symbols.len() {
            for (i, (name, kind, path)) in symbol_hits[enriched_symbols.len()..].iter().enumerate() {
                let score_suffix = symbol_scores.get(enriched_symbols.len() + i).map(|s| format!("  [{:.2}]", s)).unwrap_or_default();
                lines.push(format!("  {kind} {name}  {path}{score_suffix}"));
            }
        }
        lines.push(String::new());
    } else if !symbol_hits.is_empty() {
        // Depth 1: original compact format
        lines.push(format!("Symbols ({} found):", symbol_hits.len()));
        for (i, (name, kind, path)) in symbol_hits.iter().enumerate() {
            let score_suffix = symbol_scores.get(i).map(|s| format!("  [{:.2}]", s)).unwrap_or_default();
            lines.push(format!("  {kind} {name}  {path}{score_suffix}"));
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

    if !text_hits.is_empty() {
        lines.push(format!("Code patterns ({} found):", text_hits.len()));
        let mut last_path: Option<&str> = None;
        for (path, line, line_number) in text_hits {
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

    if symbol_hits.is_empty() && text_hits.is_empty() {
        lines.push("No matches found.".to_string());
    }

    lines.join("\n")
}

/// Format git temporal data for a single file: churn, ownership, co-changes, last commit.
pub fn co_changes_result_view(
    path: &str,
    history: &crate::live_index::git_temporal::GitFileHistory,
    limit: usize,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Git temporal data for {path}"));
    lines.push(String::new());

    // Churn
    lines.push(format!(
        "Churn score: {:.2} ({} commits)",
        history.churn_score, history.commit_count
    ));

    // Last commit
    let c = &history.last_commit;
    lines.push(format!(
        "Last commit: {} {} — {} ({})",
        c.hash, c.timestamp, c.message_head, c.author
    ));
    lines.push(String::new());

    // Ownership
    if !history.contributors.is_empty() {
        lines.push("Ownership:".to_string());
        for contrib in &history.contributors {
            lines.push(format!(
                "  {}: {} commits ({:.0}%)",
                contrib.author, contrib.commit_count, contrib.percentage
            ));
        }
        lines.push(String::new());
    }

    // Co-changes
    if history.co_changes.is_empty() {
        lines.push(
            "No high-confidence co-changing files detected (needs at least 2 shared commits and Jaccard >= 0.15)."
                .to_string(),
        );
        if !history.weak_co_changes.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Low-confidence candidates (top {}):",
                limit.min(history.weak_co_changes.len())
            ));
            for entry in history.weak_co_changes.iter().take(limit) {
                lines.push(format!(
                    "  {:<50} coupling: {:.3}  ({} shared commits)",
                    entry.path, entry.coupling_score, entry.shared_commits
                ));
            }
            lines.push(
                "These missed the strong co-change threshold and are advisory only.".to_string(),
            );
        }
    } else {
        lines.push(format!(
            "Co-changing files (top {}):",
            limit.min(history.co_changes.len())
        ));
        for entry in history.co_changes.iter().take(limit) {
            lines.push(format!(
                "  {:<50} coupling: {:.3}  ({} shared commits)",
                entry.path, entry.coupling_score, entry.shared_commits
            ));
        }
    }

    lines.join("\n")
}

/// Format symbol-level diff between two git refs.
pub fn diff_symbols_result_view(
    base: &str,
    target: &str,
    changed_files: &[&str],
    repo: &crate::git::GitRepo,
    compact: bool,
    summary_only: bool,
) -> String {
    use std::collections::HashMap;

    let mut lines = Vec::new();
    let target_label = if target.is_empty() {
        "working tree"
    } else {
        target
    };
    lines.push(format!("Symbol diff: {base}...{target_label}"));
    lines.push(format!("{} files changed", changed_files.len()));
    lines.push(String::new());

    let mut total_added = 0usize;
    let mut total_removed = 0usize;
    let mut total_modified = 0usize;
    let mut files_with_changes = 0usize;

    for file_path in changed_files {
        // Get content at base and target refs
        let base_content = repo
            .file_at_ref(base, file_path)
            .unwrap_or_default()
            .unwrap_or_default();

        // When target is empty, we're in uncommitted mode — read from the
        // working tree instead of a git ref (file_at_ref("") returns None,
        // which would make every symbol appear "removed").
        let target_content = if target.is_empty() {
            repo.file_from_workdir(file_path)
                .unwrap_or_default()
                .unwrap_or_default()
        } else {
            repo.file_at_ref(target, file_path)
                .unwrap_or_default()
                .unwrap_or_default()
        };

        // Extract symbol names from both versions — prefer tree-sitter AST,
        // fall back to regex for unsupported languages.
        let base_symbols = crate::parsing::extract_symbols_for_diff(&base_content, file_path)
            .unwrap_or_else(|| extract_symbol_signatures(&base_content));
        let target_symbols = crate::parsing::extract_symbols_for_diff(&target_content, file_path)
            .unwrap_or_else(|| extract_symbol_signatures(&target_content));

        let base_names: HashMap<&str, &str> = base_symbols
            .iter()
            .map(|(n, s)| (n.as_str(), s.as_str()))
            .collect();
        let target_names: HashMap<&str, &str> = target_symbols
            .iter()
            .map(|(n, s)| (n.as_str(), s.as_str()))
            .collect();

        let mut file_added = Vec::new();
        let mut file_removed = Vec::new();
        let mut file_modified = Vec::new();

        // Find added and modified
        for (name, sig) in &target_names {
            match base_names.get(name) {
                None => file_added.push(*name),
                Some(base_sig) if base_sig != sig => file_modified.push(*name),
                _ => {}
            }
        }

        // Find removed
        for name in base_names.keys() {
            if !target_names.contains_key(name) {
                file_removed.push(*name);
            }
        }

        if file_added.is_empty() && file_removed.is_empty() && file_modified.is_empty() {
            continue; // No symbol-level changes
        }

        total_added += file_added.len();
        total_removed += file_removed.len();
        total_modified += file_modified.len();
        files_with_changes += 1;

        if !summary_only {
            if compact {
                // Compact mode: one line per file with counts and symbol names
                let mut parts = Vec::new();
                if !file_added.is_empty() {
                    let names = compact_symbol_list(&file_added);
                    parts.push(format!("+{}: {}", file_added.len(), names));
                }
                if !file_removed.is_empty() {
                    let names = compact_symbol_list(&file_removed);
                    parts.push(format!("-{}: {}", file_removed.len(), names));
                }
                if !file_modified.is_empty() {
                    let names = compact_symbol_list(&file_modified);
                    parts.push(format!("~{}: {}", file_modified.len(), names));
                }
                lines.push(format!("  {} ({})", file_path, parts.join(", ")));
            } else {
                lines.push(format!("── {} ──", file_path));
                if !file_added.is_empty() {
                    let mut sorted = file_added.clone();
                    sorted.sort_unstable();
                    for name in &sorted {
                        lines.push(format!("  + {name}"));
                    }
                }
                if !file_removed.is_empty() {
                    let mut sorted = file_removed.clone();
                    sorted.sort_unstable();
                    for name in &sorted {
                        lines.push(format!("  - {name}"));
                    }
                }
                if !file_modified.is_empty() {
                    let mut sorted = file_modified.clone();
                    sorted.sort_unstable();
                    for name in &sorted {
                        lines.push(format!("  ~ {name}"));
                    }
                }
                lines.push(String::new());
            }
        }
    }

    // Summary
    lines.push(format!(
        "Summary: +{total_added} added, -{total_removed} removed, ~{total_modified} modified"
    ));
    let files_with_symbol_changes = total_added + total_removed + total_modified;
    if files_with_symbol_changes == 0 && !changed_files.is_empty() {
        lines.push(format!(
            "Note: {} file(s) changed but no symbol boundaries were affected (changes in comments, whitespace, or non-symbol code).",
            changed_files.len()
        ));
    }

    if compact && files_with_changes > 0 && changed_files.len() > files_with_changes {
        let omitted = changed_files.len() - files_with_changes;
        lines.push(format!(
            "({omitted} file(s) with only non-symbol changes omitted)"
        ));
    }

    lines.join("\n")
}

/// Format a list of symbol names for compact display: up to 3 names, then "..."
fn compact_symbol_list(names: &[&str]) -> String {
    let mut sorted: Vec<&str> = names.to_vec();
    sorted.sort_unstable();
    if sorted.len() <= 3 {
        sorted.join(", ")
    } else {
        format!("{}, ... +{} more", sorted[..3].join(", "), sorted.len() - 3)
    }
}

/// Extract symbol name → signature pairs from source code using simple pattern matching.
/// Returns Vec<(name, signature_line)> for functions, classes, structs, enums, traits, interfaces.
fn extract_symbol_signatures(content: &str) -> Vec<(String, String)> {
    let mut symbols = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip empty, comments, imports
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("use ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("from ")
        {
            continue;
        }

        // Match common symbol declaration patterns
        let name = extract_declaration_name(trimmed);
        if let Some(name) = name {
            symbols.push((name, trimmed.to_string()));
        }
    }
    symbols
}

/// Check if a word is a well-known type keyword that would appear between
/// `const` and the actual variable name in C#, Java, or TypeScript.
fn is_likely_type_keyword(word: &str) -> bool {
    matches!(
        word,
        "string"
            | "String"
            | "int"
            | "Int32"
            | "Int64"
            | "bool"
            | "Boolean"
            | "float"
            | "double"
            | "decimal"
            | "char"
            | "byte"
            | "long"
            | "short"
            | "uint"
            | "object"
            | "var"
            | "number"
            | "bigint"
            | "any"
    )
}

/// Try to extract a declaration name from a line of code.
pub(crate) fn extract_declaration_name(line: &str) -> Option<String> {
    // Strip leading visibility modifier generically: pub, pub(crate), pub(super), pub(in path).
    let stripped = if let Some(rest) = line.strip_prefix("pub") {
        if let Some(after_paren) = rest.strip_prefix('(') {
            // Skip balanced parens: pub(crate), pub(super), pub(in crate::foo)
            if let Some(close) = after_paren.find(')') {
                after_paren[close + 1..].trim_start()
            } else {
                rest.trim_start()
            }
        } else {
            rest.trim_start()
        }
    } else if let Some(rest) = line.strip_prefix("export default ") {
        rest
    } else if let Some(rest) = line.strip_prefix("export ") {
        rest
    } else {
        line
    };

    let keywords = [
        "async fn ",
        "fn ",
        "struct ",
        "enum ",
        "trait ",
        "type ",
        "const ",
        "static ",
        "class ",
        "interface ",
        "function ",
        "async function ",
        "async def ",
        "def ",
    ];

    for kw in &keywords {
        if let Some(rest) = stripped.strip_prefix(kw) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            // For `const`, the first word might be a type name (C#: `const string Foo`).
            // If it looks like a well-known type, skip it and take the next identifier.
            if *kw == "const " && is_likely_type_keyword(&name) {
                let after_type = &rest[name.len()..].trim_start();
                let real_name: String = after_type
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !real_name.is_empty() {
                    return Some(real_name);
                }
            }
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests;
