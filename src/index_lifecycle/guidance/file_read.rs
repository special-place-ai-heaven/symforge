//! Shared source-read rendering. Presentation may normalize displayed line endings;
//! the index and native exact-byte pages retain the original source bytes.
use super::source::{
    approx_tokens_from_bytes, canonical_truncation_notice, token_truncation_footer,
};
use crate::live_index::{FileContentView, IndexedFile, LiveIndex, search};
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
    if let Some(around_symbol) = context.around_symbol.as_deref() {
        return render_numbered_around_symbol_excerpt(
            file,
            around_symbol,
            context.symbol_line,
            context.context_lines.unwrap_or(0),
            context.max_lines,
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

const DEFAULT_AROUND_LINE_CONTEXT_LINES: u32 = 2;

pub(crate) fn render_file_content_bytes(
    path: &str,
    content: &[u8],
    context: search::ContentContext,
) -> String {
    let content = String::from_utf8_lossy(content);
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len() as u32;

    if let Some(chunk_index) = context.chunk_index {
        let Some(max_lines) = context.max_lines else {
            return format!("{path} [error: chunked read requires max_lines parameter]");
        };
        return render_numbered_chunk_excerpt(path, &lines, chunk_index, max_lines);
    }
    if let Some(around_match) = context.around_match.as_deref() {
        return render_numbered_around_match_excerpt(
            path,
            &lines,
            around_match,
            context.match_occurrence.unwrap_or(1),
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

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

fn render_numbered_chunk_excerpt(
    path: &str,
    lines: &[&str],
    chunk_index: u32,
    max_lines: u32,
) -> String {
    let chunk_size = max_lines as usize;

    if chunk_index == 0 || chunk_size == 0 {
        return out_of_range_file_chunk(path, chunk_index, 0);
    }

    let total_chunks = lines.len().div_ceil(chunk_size);
    if total_chunks == 0 {
        return out_of_range_file_chunk(path, chunk_index, 0);
    }

    let chunk_number = chunk_index as usize;
    if chunk_number > total_chunks {
        return out_of_range_file_chunk(path, chunk_index, total_chunks);
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
        path, chunk_index, total_chunks, start_line, end_line, body
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
            let detail = format!(
                "Range is {full_range_len} lines (symbol {} + {context_lines} ctx/side), showing first {ml}.",
                sym_end.saturating_sub(sym_start) + 1
            );
            result.push(canonical_truncation_notice(
                ml as u64,
                "lines",
                Some(detail.as_str()),
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
pub(crate) enum AroundSymbolResolutionError {
    NotFound,
    SelectorNotFound(u32),
    Ambiguous(Vec<u32>),
}

/// Resolve an `around_symbol` selector to the symbol's full 1-indexed line range
/// `(start_line, end_line)`.  Both bounds are inclusive.
pub(crate) fn resolve_around_symbol_range(
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
    path: &str,
    lines: &[&str],
    around_match: &str,
    match_occurrence: u32,
    context_lines: u32,
) -> String {
    let candidate_lines = find_case_insensitive_match_lines(lines, around_match);
    if candidate_lines.is_empty() {
        return not_found_file_match(path, around_match);
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
            path,
            candidate_lines.len()
        );
    };

    render_numbered_around_line_excerpt(lines, around_line, context_lines)
}

pub(crate) fn find_case_insensitive_match_lines(lines: &[&str], around_match: &str) -> Vec<u32> {
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
/// Append an honest NUL-byte warning to already-rendered `get_file_content`
/// output when `content` contains literal NUL (`0x00`) bytes.
///
/// Central rendering-boundary guard. NUL is valid UTF-8, so it passes
/// `String::from_utf8_lossy` untouched and then renders invisibly (terminals
/// show nothing or a space). An agent that copies the rendered text for an edit
/// gets bytes that do NOT match the file. The warning is appended AFTER the byte
/// cap so truncation can never drop it. `content` is the exact byte slice whose
/// rendering `output` represents (the full file for whole-file reads), so the
/// reported count/offsets describe what was actually scanned.
pub fn append_nul_byte_warning(output: String, content: &[u8]) -> String {
    let scan = crate::domain::index::scan_nul_bytes(content);
    match crate::domain::index::nul_byte_warning_line(&scan) {
        Some(warning) if output.is_empty() => warning,
        Some(warning) => format!("{output}\n{warning}"),
        None => output,
    }
}

pub fn cap_file_content_output(output: String) -> String {
    if output.len() <= GET_FILE_CONTENT_MAX_BYTES {
        return output;
    }
    let original_bytes = output.len();
    let original_tokens_est = approx_tokens_from_bytes(original_bytes);
    let cap_tokens_est = approx_tokens_from_bytes(GET_FILE_CONTENT_MAX_BYTES);
    let detail = format!(
        "Original output is ~{original_tokens_est} tokens ({original_bytes} bytes), exceeding {GET_FILE_CONTENT_MAX_BYTES}-byte cap. Use chunk_index + max_lines, around_line, around_match, or around_symbol to read a smaller window."
    );
    let footer = token_truncation_footer(cap_tokens_est, Some(detail.as_str()));

    // Reserve exactly enough bytes for the footer; align budget to a UTF-8 char boundary.
    let mut budget = GET_FILE_CONTENT_MAX_BYTES.saturating_sub(footer.len());
    while budget > 0 && !output.is_char_boundary(budget) {
        budget -= 1;
    }
    let truncate_at = match output[..budget].rfind('\n') {
        Some(pos) => pos + 1, // keep the newline
        None => budget,       // no newline — truncate at char boundary
    };
    let truncated = &output[..truncate_at];
    format!("{truncated}{footer}")
}

pub fn not_found_file(path: &str) -> String {
    format!("File not found: {path}")
}

pub fn not_found_file_match(path: &str, query: &str) -> String {
    format!("No matches for '{query}' in {path}")
}

fn out_of_range_file_chunk(path: &str, chunk_index: u32, total_chunks: usize) -> String {
    format!("Chunk {chunk_index} out of range for {path} ({total_chunks} chunks)")
}

pub fn not_found_symbol(index: &LiveIndex, path: &str, name: &str) -> String {
    match index.capture_shared_file(path) {
        None => not_found_file(path),
        Some(file) => render_not_found_symbol(&file.relative_path, &file.symbols, name),
    }
}

pub(crate) fn render_not_found_symbol(
    relative_path: &str,
    symbols: &[crate::domain::SymbolRecord],
    name: &str,
) -> String {
    let symbol_names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
    not_found_symbol_names(relative_path, &symbol_names, name)
}

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

pub(crate) fn not_found_symbol_names(
    relative_path: &str,
    symbol_names: &[String],
    name: &str,
) -> String {
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

pub const COMPETENT_READ_WINDOW_LINES: u32 = 50;

const COMPETENT_READ_AVG_LINE_BYTES: usize = 80;

pub const SMALL_FILE_CHAR_THRESHOLD: usize = 200;

pub fn competent_manual_baseline_chars(raw_chars: usize) -> usize {
    if raw_chars < SMALL_FILE_CHAR_THRESHOLD {
        return raw_chars;
    }
    let window =
        (COMPETENT_READ_WINDOW_LINES as usize).saturating_mul(COMPETENT_READ_AVG_LINE_BYTES);
    raw_chars.min(window)
}

pub fn default_read_max_tokens() -> u64 {
    estimate_tokens_from_chars(competent_manual_baseline_chars(4000))
}

pub fn resolve_read_max_tokens(explicit: Option<u64>, raw_chars: usize) -> u64 {
    match explicit {
        Some(t) if t > 0 => t,
        _ => {
            let baseline = competent_manual_baseline_chars(raw_chars);
            estimate_tokens_from_chars(baseline).max(64)
        }
    }
}

pub fn estimate_tokens_from_chars(chars: usize) -> u64 {
    chars.div_ceil(4) as u64
}

#[cfg(test)]
mod raw_selection_tests {
    use super::*;

    #[test]
    fn raw_content_applies_match_occurrence_and_chunk_selectors() {
        let bytes = b"first needle\r\nmiddle\r\nsecond needle\r\nlast\r\n";
        let mut context = search::ContentContext::line_range(None, None);
        context.around_match = Some("NEEDLE".into());
        context.match_occurrence = Some(2);
        context.context_lines = Some(0);
        assert_eq!(
            render_file_content_bytes("notes.txt", bytes, context),
            "3: second needle"
        );
        let mut context = search::ContentContext::line_range(None, None);
        context.chunk_index = Some(2);
        context.max_lines = Some(2);
        let rendered = render_file_content_bytes("notes.txt", bytes, context);
        assert!(rendered.contains("chunk 2/2, lines 3-4"));
        assert!(rendered.contains("3: second needle"));
        assert!(!rendered.contains("first needle"));
    }
}

pub fn append_dedup_hint_footer(
    output: String,
    kind: &str,
    age_secs: u64,
    approx_tokens: u32,
) -> String {
    format!(
        "{output}\n\n[session: same {kind} fetched {age_secs}s ago (~{approx_tokens} est tokens); reuse prior unless content changed]"
    )
}
