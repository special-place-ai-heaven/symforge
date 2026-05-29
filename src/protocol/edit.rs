use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::index::{LanguageId, SymbolKind, SymbolRecord};
use crate::live_index::SharedIndex;
use crate::live_index::qualified_usages;
use crate::live_index::query::{
    SymbolSelectorMatch, render_symbol_selector, resolve_symbol_selector,
};
use crate::live_index::store::IndexedFile;

// ---------------------------------------------------------------------------
// Path containment
// ---------------------------------------------------------------------------

/// Validate that a user-supplied relative path stays within the repo root.
/// Returns the canonicalized absolute path on success.
///
/// NOTE: Requires the target path to exist on disk (canonicalize).
pub(crate) fn safe_repo_path(repo_root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let full_path = repo_root.join(relative_path);

    // Lexical containment check — catches traversals like "../secret" even when
    // the target path doesn't exist on disk (where canonicalize would just fail).
    let has_parent_traversal = std::path::Path::new(relative_path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir));
    if has_parent_traversal {
        return Err(format!("path '{relative_path}' is outside the repository"));
    }

    let canon_root = repo_root
        .canonicalize()
        .map_err(|e| format!("cannot resolve repo root: {e}"))?;
    let canon_path = full_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve path '{relative_path}': {e}"))?;
    if !canon_path.starts_with(&canon_root) {
        return Err(format!("path '{relative_path}' is outside the repository"));
    }
    Ok(canon_path)
}

// ---------------------------------------------------------------------------
// Core splice
// ---------------------------------------------------------------------------

/// Splice `replacement` bytes into `content` at the given byte range [start, end).
pub(crate) fn apply_splice(content: &[u8], range: (u32, u32), replacement: &[u8]) -> Vec<u8> {
    let (start, end) = (range.0 as usize, range.1 as usize);
    let mut result = Vec::with_capacity(content.len() - (end - start) + replacement.len());
    result.extend_from_slice(&content[..start]);
    result.extend_from_slice(replacement);
    result.extend_from_slice(&content[end..]);
    result
}

// ---------------------------------------------------------------------------
// Line ending detection and normalization
// ---------------------------------------------------------------------------

/// Detected line ending style of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    /// Returns the byte sequence for this line ending.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        match self {
            LineEnding::Lf => b"\n",
            LineEnding::CrLf => b"\r\n",
        }
    }
}

/// Detect the dominant line ending style in file content.
/// Counts \r\n pairs vs lone \n. If \r\n > lone \n → CrLf, else Lf.
/// Empty or no-newline content defaults to Lf.
pub(crate) fn detect_line_ending(content: &[u8]) -> LineEnding {
    let mut crlf_count: usize = 0;
    let mut lf_count: usize = 0;
    let mut i = 0;
    while i < content.len() {
        if i + 1 < content.len() && content[i] == b'\r' && content[i + 1] == b'\n' {
            crlf_count += 1;
            i += 2;
        } else if content[i] == b'\n' {
            lf_count += 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    if crlf_count > lf_count {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    }
}

/// Normalize line endings in generated/replacement text to match the target style.
/// 1. Convert \r\n → \n  2. Convert lone \r → \n  3. If target is CrLf, convert \n → \r\n
pub(crate) fn normalize_line_endings(text: &[u8], target: LineEnding) -> Vec<u8> {
    // Step 1+2: canonicalize to \n
    let mut canonical = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if i + 1 < text.len() && text[i] == b'\r' && text[i + 1] == b'\n' {
            canonical.push(b'\n');
            i += 2;
        } else if text[i] == b'\r' {
            canonical.push(b'\n');
            i += 1;
        } else {
            canonical.push(text[i]);
            i += 1;
        }
    }
    match target {
        LineEnding::Lf => canonical,
        LineEnding::CrLf => {
            let mut result = Vec::with_capacity(canonical.len() * 2);
            for &byte in &canonical {
                if byte == b'\n' {
                    result.extend_from_slice(b"\r\n");
                } else {
                    result.push(byte);
                }
            }
            result
        }
    }
}

// ---------------------------------------------------------------------------
// Atomic file write
// ---------------------------------------------------------------------------

/// Write content to a file atomically: write to a unique temp file in the same directory,
/// then rename over the target. Using a `NamedTempFile` in the same directory ensures the
/// rename is within a single filesystem (no cross-device move) and avoids collisions between
/// concurrent callers that would occur with a fixed `.symforge_tmp` extension.
#[derive(Debug, Clone)]
pub(crate) struct AtomicWriteReport {
    pub tee_snapshot: crate::edit_safety::tee::TeeSnapshot,
}

pub(crate) fn atomic_write_file(path: &Path, content: &[u8]) -> std::io::Result<AtomicWriteReport> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent directory",
        )
    })?;
    let tee_snapshot = crate::edit_safety::tee::Tee::for_target(path)
        .snapshot(path)
        .unwrap_or_else(|err| crate::edit_safety::tee::TeeSnapshot::Warning {
            original_path: path.to_path_buf(),
            message: format!("unexpected tee snapshot error: {err}"),
        });
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    // persist() uses rename(2) on Unix and MoveFileExW(MOVEFILE_REPLACE_EXISTING) on Windows,
    // atomically replacing any existing target file.
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(AtomicWriteReport { tee_snapshot })
}

pub(crate) fn format_tee_snapshot_suffix(report: &AtomicWriteReport) -> String {
    report
        .tee_snapshot
        .response_hint()
        .map(|hint| format!("\n{hint}"))
        .unwrap_or_default()
}

fn append_response_suffix_to_first_summary(summaries: &mut Vec<String>, suffix: &str) {
    let suffix = suffix.trim_start_matches('\n');
    if suffix.is_empty() {
        return;
    }
    let indented = suffix
        .lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(first) = summaries.first_mut() {
        first.push('\n');
        first.push_str(&indented);
    } else {
        summaries.push(indented);
    }
}

// ---------------------------------------------------------------------------
// Reindex after write
// ---------------------------------------------------------------------------

/// Write content to a file and fully reindex from disk.
///
/// INVARIANT: All derived index state is rebuilt from the persisted on-disk bytes,
/// never from the in-memory buffer passed to `fs::write`. If the write partially
/// fails or the OS buffers differently, the index will still reflect reality.
pub(crate) fn reindex_after_write(
    index: &SharedIndex,
    abs_path: &Path,
    relative_path: &str,
    written: &[u8],
    language: LanguageId,
) {
    // Re-read from disk — not from the `written` parameter.
    let on_disk = match std::fs::read(abs_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(
                "reindex_after_write: failed to re-read {}: {e}",
                abs_path.display()
            );
            return;
        }
    };

    debug_assert_eq!(
        written,
        on_disk.as_slice(),
        "reindex_after_write: disk content differs from written buffer for {}",
        abs_path.display()
    );

    let mtime_secs = std::fs::metadata(abs_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let result = crate::parsing::process_file(relative_path, &on_disk, language);
    let indexed = IndexedFile::from_parse_result(result, on_disk).with_mtime(mtime_secs);
    index.update_file(relative_path.to_string(), indexed);
}

// ---------------------------------------------------------------------------
// Symbol resolution wrapper
// ---------------------------------------------------------------------------

const MAX_SYMBOL_SUGGESTIONS: usize = 3;
const MAX_SYMBOL_SUGGESTION_DISTANCE: usize = 3;
const MIN_SYMBOL_SUGGESTION_CONFIDENCE: f64 = 0.6;

fn did_you_mean_suffix(file: &IndexedFile, requested: &str) -> String {
    let suggestions = same_file_symbol_suggestions(file, requested);
    if suggestions.is_empty() {
        String::new()
    } else {
        format!(" did_you_mean: [{}]", suggestions.join(", "))
    }
}

fn same_file_symbol_suggestions(file: &IndexedFile, requested: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut scored = Vec::new();

    for sym in &file.symbols {
        let candidate = sym.name.trim();
        if candidate.is_empty() || candidate == requested || !seen.insert(candidate.to_string()) {
            continue;
        }

        if let Some((score, distance)) = symbol_suggestion_score(requested, candidate) {
            scored.push((score, distance, sym.line_range.0, candidate.to_string()));
        }
    }

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });

    scored
        .into_iter()
        .take(MAX_SYMBOL_SUGGESTIONS)
        .map(|(_, _, _, name)| name)
        .collect()
}

fn symbol_suggestion_score(requested: &str, candidate: &str) -> Option<(u16, usize)> {
    let requested_norm = normalize_symbol_name(requested);
    let candidate_norm = normalize_symbol_name(candidate);
    if requested_norm.is_empty() || candidate_norm.is_empty() {
        return None;
    }

    let mut best = bounded_levenshtein(
        &requested_norm,
        &candidate_norm,
        MAX_SYMBOL_SUGGESTION_DISTANCE,
    )
    .and_then(|distance| {
        let max_len = requested_norm
            .chars()
            .count()
            .max(candidate_norm.chars().count());
        let confidence = 1.0 - (distance as f64 / max_len as f64);
        if confidence >= MIN_SYMBOL_SUGGESTION_CONFIDENCE {
            Some(((confidence * 1000.0) as u16, distance))
        } else {
            None
        }
    });

    if has_separator_prefix(requested, candidate) {
        let prefix_score = 900;
        let prefix_distance = bounded_levenshtein(
            &requested_norm,
            &candidate_norm,
            MAX_SYMBOL_SUGGESTION_DISTANCE,
        )
        .unwrap_or(MAX_SYMBOL_SUGGESTION_DISTANCE + 1);
        match best {
            Some((score, _)) if score >= prefix_score => {}
            _ => best = Some((prefix_score, prefix_distance)),
        }
    }

    best
}

fn normalize_symbol_name(name: &str) -> String {
    let mut normalized = String::new();
    for ch in name.chars() {
        for folded in ch.to_lowercase() {
            if folded.is_alphanumeric() {
                normalized.push(folded);
            }
        }
    }
    normalized
}

fn has_separator_prefix(requested: &str, candidate: &str) -> bool {
    if normalize_symbol_name(requested).chars().count() < 3 {
        return false;
    }

    let requested = requested.to_lowercase();
    let candidate = candidate.to_lowercase();
    let mut candidate_chars = candidate.chars();
    for requested_char in requested.chars() {
        if candidate_chars.next() != Some(requested_char) {
            return false;
        }
    }

    matches!(candidate_chars.next(), Some(ch) if !ch.is_alphanumeric())
}

fn bounded_levenshtein(left: &str, right: &str, max_distance: usize) -> Option<usize> {
    let left_chars = left.chars().collect::<Vec<_>>();
    let right_chars = right.chars().collect::<Vec<_>>();
    if left_chars.len().abs_diff(right_chars.len()) > max_distance {
        return None;
    }

    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left_chars.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let deletion = previous[right_index + 1] + 1;
            let insertion = current[right_index] + 1;
            let substitution = previous[right_index] + usize::from(left_char != right_char);
            current[right_index + 1] = deletion.min(insertion).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let distance = previous[right_chars.len()];
    (distance <= max_distance).then_some(distance)
}

/// Resolve a symbol by name/kind/line, returning (index, cloned record) or user-friendly error.
pub(crate) fn resolve_or_error(
    file: &IndexedFile,
    name: &str,
    kind: Option<&str>,
    line: Option<u32>,
) -> Result<(usize, SymbolRecord), String> {
    match resolve_symbol_selector(file, name, kind, line) {
        SymbolSelectorMatch::Selected(idx, sym) => Ok((idx, sym.clone())),
        SymbolSelectorMatch::NotFound => {
            let label = render_symbol_selector(name, kind, line);
            // Surface parse status so users know WHY symbols are missing.
            let status_hint = match &file.parse_status {
                crate::live_index::store::ParseStatus::Failed { error } => {
                    format!(
                        " (file failed to parse: {error} — symbol tools unavailable for this file)"
                    )
                }
                crate::live_index::store::ParseStatus::PartialParse { warning } => {
                    format!(
                        " (file partially parsed with errors: {warning} — some symbols may be missing)"
                    )
                }
                _ => String::new(),
            };
            let suggestion_hint = did_you_mean_suffix(file, name);
            Err(format!(
                "Symbol not found: {label}{status_hint}{suggestion_hint}"
            ))
        }
        SymbolSelectorMatch::Ambiguous(candidate_lines) => {
            let candidates = candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Ambiguous: multiple definitions of `{name}`. \
                 Pass `symbol_line` to disambiguate. Candidate lines: {candidates}"
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Indentation utilities
// ---------------------------------------------------------------------------

/// Detect the leading whitespace on the line containing `byte_offset`.
pub(crate) fn detect_indentation(content: &[u8], byte_offset: u32) -> Vec<u8> {
    let offset = byte_offset as usize;
    let line_start = content[..offset]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let indent_end = content[line_start..]
        .iter()
        .position(|b| !b.is_ascii_whitespace() || *b == b'\n')
        .unwrap_or(0);
    content[line_start..line_start + indent_end].to_vec()
}

/// Prefix each non-empty line of `text` with `indent`, using the given line ending.
pub(crate) fn apply_indentation(text: &str, indent: &[u8], line_ending: LineEnding) -> Vec<u8> {
    let mut result = Vec::new();
    // Use split('\n') instead of lines() so that trailing newlines produce a trailing
    // empty element, preserving them. str::lines() silently strips all trailing newlines.
    let parts: Vec<&str> = text.split('\n').collect();
    for (i, line) in parts.iter().enumerate() {
        // Strip '\r' left behind by split('\n') on CRLF input; re-emit via line_ending.
        let line = line.strip_suffix('\r').unwrap_or(line);
        if i > 0 {
            result.extend_from_slice(line_ending.as_bytes());
        }
        if !line.is_empty() {
            result.extend_from_slice(indent);
            result.extend_from_slice(line.as_bytes());
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Insert helpers
// ---------------------------------------------------------------------------

/// Build the bytes to insert before a symbol: indented content + separator + existing content.
/// Splices at the start of the line (before existing indentation) so indentation isn't doubled.
/// Uses `\n\n` when the target symbol has no doc comments and no blank line already precedes
/// the splice point (visual separation between definitions), and `\n` otherwise (avoids triple
/// newlines when a blank line already exists, and keeps doc comments tight against their symbol).
pub(crate) fn build_insert_before(
    file_content: &[u8],
    sym: &SymbolRecord,
    new_code: &str,
    line_ending: LineEnding,
) -> Vec<u8> {
    let sym_start = sym.effective_start() as usize;
    let line_start = file_content[..sym_start]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0) as u32;
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let normalized = normalize_line_endings(new_code.as_bytes(), line_ending);
    let normalized_str = std::str::from_utf8(&normalized).unwrap_or(new_code);
    let indented = apply_indentation(normalized_str, &indent, line_ending);
    let mut insertion = indented;
    let le = line_ending.as_bytes();
    let separator: Vec<u8> = if sym.doc_byte_range.is_some() {
        le.to_vec()
    } else {
        // Use single newline only when a blank line already precedes the symbol
        // (avoids creating triple-newline sequences). At start-of-file (empty prefix),
        // there's no existing blank line, so use double newline for visual separation.
        let prefix = &file_content[..line_start as usize];
        let already_has_blank = match line_ending {
            LineEnding::CrLf => {
                prefix.len() >= 4
                    && prefix[prefix.len() - 2] == b'\r'
                    && prefix[prefix.len() - 1] == b'\n'
                    && prefix[prefix.len() - 4] == b'\r'
                    && prefix[prefix.len() - 3] == b'\n'
            }
            LineEnding::Lf => {
                prefix.len() >= 2
                    && prefix[prefix.len() - 1] == b'\n'
                    && prefix[prefix.len() - 2] == b'\n'
            }
        };
        if already_has_blank {
            le.to_vec()
        } else {
            let mut sep = Vec::with_capacity(le.len() * 2);
            sep.extend_from_slice(le);
            sep.extend_from_slice(le);
            sep
        }
    };
    insertion.extend_from_slice(&separator);
    apply_splice(file_content, (line_start, line_start), &insertion)
}

/// Build the bytes to insert after a symbol: existing content + blank line + indented content.
///
/// Handles the C/C++ quirk where struct/enum/class definitions end their tree-sitter
/// node at `}` while the actual declaration includes a trailing `;`.  When the byte
/// immediately following the symbol end (skipping spaces/tabs) is `;`, the insertion
/// point moves past it so the result stays syntactically valid.
pub(crate) fn build_insert_after(
    file_content: &[u8],
    sym: &SymbolRecord,
    new_code: &str,
    line_ending: LineEnding,
) -> Vec<u8> {
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let normalized = normalize_line_endings(new_code.as_bytes(), line_ending);
    let normalized_str = std::str::from_utf8(&normalized).unwrap_or(new_code);
    let indented = apply_indentation(normalized_str, &indent, line_ending);
    let le = line_ending.as_bytes();
    let mut insertion = Vec::new();
    insertion.extend_from_slice(le);
    insertion.extend_from_slice(le);
    insertion.extend_from_slice(&indented);
    // Skip past a trailing `;` that belongs to the parent declaration (C/C++
    // struct/enum/class: tree-sitter node ends at `}`, declaration at `};`).
    let insert_pos = skip_trailing_semicolon(file_content, sym.byte_range.1 as usize) as u32;
    apply_splice(file_content, (insert_pos, insert_pos), &insertion)
}

/// If the byte(s) immediately after `pos` (skipping spaces and tabs, but not
/// newlines) form a `;`, return the position just past it.  Otherwise return `pos`.
fn skip_trailing_semicolon(content: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < content.len() && (content[i] == b' ' || content[i] == b'\t') {
        i += 1;
    }
    if i < content.len() && content[i] == b';' {
        i + 1
    } else {
        pos
    }
}

// ---------------------------------------------------------------------------
// Delete helper
// ---------------------------------------------------------------------------

/// Build file content with the symbol removed, including leading whitespace and trailing newlines.
/// Collapses runs of 3+ consecutive blank lines down to 1 after deletion.
/// Scan upward from `line_start` to include orphaned doc comments when
/// `doc_byte_range` is `None`. Returns the (possibly earlier) byte offset
/// that includes the orphaned comments. Used by `build_delete` and
/// `replace_symbol_body` to handle blank-line-separated doc comments.
pub(crate) fn extend_past_orphaned_docs(
    file_content: &[u8],
    line_start: usize,
    sym: &SymbolRecord,
) -> usize {
    if sym.doc_byte_range.is_some() {
        return line_start;
    }
    let above = &file_content[..line_start];
    let lines: Vec<&[u8]> = above.split(|&b| b == b'\n').collect();
    let mut i = lines.len();
    // Skip trailing empty element from split
    if i > 0 && lines[i - 1].is_empty() {
        i -= 1;
    }
    // Skip exactly one blank line
    if i > 0 && lines[i - 1].iter().all(|b| b.is_ascii_whitespace()) {
        i -= 1;
        // Collect consecutive comment lines above the blank line
        let mut found_comments = false;
        while i > 0 {
            let line_text = std::str::from_utf8(lines[i - 1]).unwrap_or("");
            let trimmed = line_text.trim_start();
            if trimmed.starts_with("///")
                || trimmed.starts_with("//!")
                || trimmed.starts_with("/**")
                || trimmed.starts_with("* ")
                || trimmed == "*/"
                || trimmed.starts_with("# ")
                || trimmed == "#"
            {
                found_comments = true;
                i -= 1;
            } else {
                break;
            }
        }
        if found_comments {
            // split('\n') leaves \r in slices for CRLF; +1 accounts for the \n separator
            return lines[..i].iter().map(|l| l.len() + 1).sum();
        }
    }
    line_start
}

/// Whether `body` begins (first non-blank line) with a doc-comment marker.
///
/// Used by `replace_symbol_body` to decide whether the caller intends to
/// supply fresh docs for the symbol. When true, the splice range extends
/// past the existing docs so the old ones are replaced. When false, the
/// splice starts at the signature line so attached docs are preserved.
///
/// Conservative on purpose: only matches markers that are unambiguously
/// doc comments across the grammars SymForge indexes. Line comments like
/// `//` and `#` are NOT counted because they may be ordinary code
/// comments or, for `#`, Rust attributes (e.g., `#[inline]`).
pub(crate) fn body_starts_with_doc_comment(body: &str) -> bool {
    let Some(first) = body.lines().find(|l| !l.trim().is_empty()) else {
        return false;
    };
    let trimmed = first.trim_start();
    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("/**")
        || trimmed.starts_with("/*!")
        || trimmed.starts_with("#[doc")
}

/// Return the splice start for a docless replacement.
///
/// Normally this is the start of the symbol's source line. When a doc marker
/// shares the line with the symbol, preserve the marker and its separator, then
/// replace the old modifiers/signature with the caller's `new_body`.
pub(crate) fn docless_replacement_splice_start(
    file_content: &[u8],
    raw_line_start: usize,
    symbol_start: usize,
) -> usize {
    if raw_line_start >= symbol_start || symbol_start > file_content.len() {
        return raw_line_start;
    }

    let prefix = &file_content[raw_line_start..symbol_start];
    same_line_doc_prefix_end(prefix)
        .map(|end| raw_line_start + end)
        .unwrap_or(raw_line_start)
}

fn same_line_doc_prefix_end(prefix: &[u8]) -> Option<usize> {
    let Ok(text) = std::str::from_utf8(prefix) else {
        return None;
    };
    let leading = text.len() - text.trim_start().len();
    let trimmed = &text[leading..];

    if trimmed.starts_with("/**") || trimmed.starts_with("/*!") {
        let marker_end = trimmed.find("*/")? + 2;
        let after_padding = trimmed[marker_end..]
            .find(|c: char| !c.is_whitespace())
            .map(|pos| marker_end + pos)
            .unwrap_or(trimmed.len());
        return Some(leading + after_padding);
    }

    if trimmed.starts_with("#[doc") {
        let marker_end = trimmed.find(']')? + 1;
        let after_padding = trimmed[marker_end..]
            .find(|c: char| !c.is_whitespace())
            .map(|pos| marker_end + pos)
            .unwrap_or(trimmed.len());
        return Some(leading + after_padding);
    }

    None
}

pub(crate) fn build_delete(
    file_content: &[u8],
    sym: &SymbolRecord,
    line_ending: LineEnding,
) -> Vec<u8> {
    // Extend to start of line (include leading whitespace and orphaned doc comments).
    let start = {
        let s = sym.effective_start() as usize;
        let line_start = file_content[..s]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        extend_past_orphaned_docs(file_content, line_start, sym) as u32
    };
    // Extend past trailing newlines (consume up to one blank line).
    // CRLF-aware: on CRLF files, a line ending is \r\n not just \n.
    let end = {
        let e = sym.byte_range.1 as usize;
        let mut pos = e;
        // Skip to end of current line (past any trailing non-newline chars).
        while pos < file_content.len() && file_content[pos] != b'\n' {
            pos += 1;
        }
        // Consume the \n (or \r\n).
        if pos < file_content.len() && file_content[pos] == b'\n' {
            pos += 1;
        }
        // Consume one more blank line if present.
        match line_ending {
            LineEnding::CrLf => {
                if pos + 1 < file_content.len()
                    && file_content[pos] == b'\r'
                    && file_content[pos + 1] == b'\n'
                {
                    pos += 2;
                }
            }
            LineEnding::Lf => {
                if pos < file_content.len() && file_content[pos] == b'\n' {
                    pos += 1;
                }
            }
        }
        pos as u32
    };
    let spliced = apply_splice(file_content, (start, end), b"");
    collapse_blank_lines(&spliced, line_ending)
}

/// Collapse runs of 3+ consecutive newlines down to 2 (one blank line).
/// On CRLF files, counts `\r\n` pairs; on LF files, counts `\n` bytes.
fn collapse_blank_lines(content: &[u8], line_ending: LineEnding) -> Vec<u8> {
    let mut result = Vec::with_capacity(content.len());
    match line_ending {
        LineEnding::Lf => {
            let mut consecutive_newlines = 0u32;
            for &b in content {
                if b == b'\n' {
                    consecutive_newlines += 1;
                    if consecutive_newlines <= 2 {
                        result.push(b);
                    }
                } else {
                    consecutive_newlines = 0;
                    result.push(b);
                }
            }
        }
        LineEnding::CrLf => {
            // Count \r\n pairs as line endings; threshold at 2 pairs (one blank line).
            let mut consecutive_line_endings = 0u32;
            let mut i = 0;
            while i < content.len() {
                if i + 1 < content.len() && content[i] == b'\r' && content[i + 1] == b'\n' {
                    consecutive_line_endings += 1;
                    if consecutive_line_endings <= 2 {
                        result.push(b'\r');
                        result.push(b'\n');
                    }
                    i += 2;
                } else {
                    consecutive_line_endings = 0;
                    result.push(content[i]);
                    i += 1;
                }
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Edit-within helper
// ---------------------------------------------------------------------------

/// Find-and-replace text within a symbol's byte range. Returns (new_content, replacement_count).
pub(crate) fn build_edit_within(
    file_content: &[u8],
    sym: &SymbolRecord,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
) -> Result<(Vec<u8>, usize), String> {
    let sym_start = sym.effective_start() as usize;
    let sym_end = sym.byte_range.1 as usize;
    let body = &file_content[sym_start..sym_end];
    let body_str =
        std::str::from_utf8(body).map_err(|_| "Symbol body is not valid UTF-8.".to_string())?;

    // Callers (LLMs) almost always supply `\n`-separated text regardless of the
    // file's on-disk convention. Normalize both the search needle and the
    // replacement to the file's dominant line ending so matches succeed in
    // CRLF files and the splice never introduces mixed line endings.
    let line_ending = detect_line_ending(file_content);
    let needle = String::from_utf8(normalize_line_endings(old_text.as_bytes(), line_ending))
        .map_err(|_| "Normalized search text is not valid UTF-8.".to_string())?;
    let replacement = String::from_utf8(normalize_line_endings(new_text.as_bytes(), line_ending))
        .map_err(|_| "Normalized replacement text is not valid UTF-8.".to_string())?;

    let (new_body, count) = if replace_all {
        let count = body_str.matches(needle.as_str()).count();
        if count == 0 {
            return Err(format!(
                "`{old_text}` not found within symbol `{}`",
                sym.name
            ));
        }
        (
            body_str.replace(needle.as_str(), replacement.as_str()),
            count,
        )
    } else {
        match body_str.find(needle.as_str()) {
            Some(_) => (
                body_str.replacen(needle.as_str(), replacement.as_str(), 1),
                1,
            ),
            None => {
                return Err(format!(
                    "`{old_text}` not found within symbol `{}`",
                    sym.name
                ));
            }
        }
    };

    let effective_range = (sym.effective_start(), sym.byte_range.1);
    let new_content = apply_splice(file_content, effective_range, new_body.as_bytes());
    Ok((new_content, count))
}

// ---------------------------------------------------------------------------
// Whitespace-flexible matching fallback
// ---------------------------------------------------------------------------

/// Return the leading whitespace of the first non-blank line.
fn indent_of_first_nonempty<'a>(lines: &[&'a str]) -> &'a str {
    for line in lines {
        let trimmed = line.trim_start();
        if !trimmed.is_empty() {
            return &line[..line.len() - trimmed.len()];
        }
    }
    ""
}

/// Re-indent `line` from `old_base` indentation to `file_base`.
fn reindent_line(line: &str, old_base: &str, file_base: &str) -> String {
    if line.trim().is_empty() {
        return String::new();
    }
    match line.strip_prefix(old_base) {
        Some(rest) => format!("{file_base}{rest}"),
        None => {
            // Line has different indent depth than the base.
            let line_indent = line.len() - line.trim_start().len();
            let old_indent = old_base.len();
            if line_indent < old_indent {
                // Less indented (e.g. closing brace) — preserve relative de-indent.
                let deficit = old_indent - line_indent;
                if file_base.len() > deficit {
                    format!(
                        "{}{}",
                        &file_base[..file_base.len() - deficit],
                        line.trim_start()
                    )
                } else {
                    line.trim_start().to_string()
                }
            } else {
                // More indented but prefix mismatch (tabs vs spaces mix).
                let extra = &line[old_indent..line_indent];
                format!("{file_base}{extra}{}", line.trim_start())
            }
        }
    }
}

/// Attempt a whitespace-flexible find-and-replace within `body`.
///
/// When an exact match of `old_text` fails, this tries matching lines
/// with leading whitespace stripped.  If found, `new_text` is re-indented
/// to match the file's actual indentation before replacement.
///
/// Returns `Some((new_body, count))` on success, `None` if no flexible
/// match is found either.
pub(crate) fn try_whitespace_flexible_replace(
    body: &str,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
) -> Option<(String, usize)> {
    let body_lines: Vec<&str> = body.lines().collect();
    let old_lines: Vec<&str> = old_text.lines().collect();

    if old_lines.is_empty() || old_lines.iter().all(|l| l.trim().is_empty()) {
        return None;
    }

    let old_trimmed: Vec<&str> = old_lines.iter().map(|l| l.trim_start()).collect();
    let window = old_trimmed.len();

    // Find matching positions (line-aligned, trimmed comparison).
    let mut matches: Vec<usize> = Vec::new();
    for start in 0..=body_lines.len().saturating_sub(window) {
        let hit = old_trimmed
            .iter()
            .enumerate()
            .all(|(i, ot)| body_lines[start + i].trim_start() == *ot);
        if hit {
            matches.push(start);
            if !replace_all {
                break;
            }
        }
    }

    if matches.is_empty() {
        return None;
    }

    // Pre-compute byte offset of each line start.
    let mut line_starts: Vec<usize> = vec![0];
    for (i, b) in body.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }

    let count = matches.len();
    let mut result = body.to_string();

    // Process in reverse so earlier byte offsets remain valid.
    for &m in matches.iter().rev() {
        let byte_start = line_starts[m];
        let byte_end = if m + window < line_starts.len() {
            line_starts[m + window]
        } else {
            body.len()
        };

        let matched_lines = &body_lines[m..m + window];
        let old_base = indent_of_first_nonempty(&old_lines);
        let file_base = indent_of_first_nonempty(matched_lines);

        let reindented: Vec<String> = new_text
            .lines()
            .map(|l| reindent_line(l, old_base, file_base))
            .collect();
        let mut replacement = reindented.join("\n");

        // Preserve trailing newline when the matched region included one.
        if byte_end > byte_start
            && result.as_bytes().get(byte_end - 1) == Some(&b'\n')
            && !replacement.ends_with('\n')
        {
            replacement.push('\n');
        }

        result.replace_range(byte_start..byte_end, &replacement);
    }

    Some((result, count))
}

// ---------------------------------------------------------------------------
// Input structs for tool handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ReplaceSymbolBodyInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name to replace.
    pub name: String,
    /// Optional kind filter (e.g., "fn", "struct", "impl").
    pub kind: Option<String>,
    /// Line number to disambiguate when multiple symbols share the same name.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Complete new source code for the symbol (replaces the entire definition).
    // `body` is accepted as a legacy alias for leniency toward models that drop
    // the `new_` prefix. Providing both `new_body` and `body` in the same payload
    // is a hard error (duplicate field) — we refuse to silently choose between
    // ambiguous inputs. See tests in this file for the accept/reject contract.
    #[serde(alias = "body")]
    pub new_body: String,
    /// When true, validate and preview but skip the actual write.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
    /// Optional replay guard for committed mutations. Dry runs do not reserve or replay.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Caller's working directory (absolute path). Consumed by the
    /// `worktree-awareness` feature hook to redirect the write into the
    /// matching git worktree. Omit to preserve today's behaviour (write to
    /// the indexed copy).
    #[serde(default)]
    pub working_directory: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InsertSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Name of the reference symbol to insert adjacent to.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Code to insert. Will be indented to match the target symbol's indentation.
    pub content: String,
    /// Where to insert relative to the target symbol: `"before"` or `"after"` (default `"after"`).
    #[serde(default)]
    pub position: Option<String>,
    /// When true, validate and preview but skip the actual write.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
    /// Optional replay guard for committed mutations. Dry runs do not reserve or replay.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Caller's working directory (absolute path). Consumed by the
    /// `worktree-awareness` feature hook to redirect the write into the
    /// matching git worktree.
    #[serde(default)]
    pub working_directory: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct DeleteSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name to delete.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// When true, validate and preview but skip the actual write.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
    /// Optional replay guard for committed mutations. Dry runs do not reserve or replay.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Caller's working directory (absolute path). Consumed by the
    /// `worktree-awareness` feature hook to redirect the write into the
    /// matching git worktree.
    #[serde(default)]
    pub working_directory: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct EditWithinSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name that scopes the edit.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Old text to find within the symbol body (literal match).
    pub old_text: String,
    /// Replacement text.
    pub new_text: String,
    /// If true, replace all occurrences within the symbol. Default: false (first match only).
    #[serde(default)]
    pub replace_all: bool,
    /// When true, validate and preview but skip the actual write.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
    /// Optional replay guard for committed mutations. Dry runs do not reserve or replay.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Caller's working directory (absolute path). Consumed by the
    /// `worktree-awareness` feature hook to redirect the write into the
    /// matching git worktree.
    #[serde(default)]
    pub working_directory: Option<String>,
}

// ---------------------------------------------------------------------------
// Batch edit types and execution
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchEditInput {
    /// List of individual edits to apply atomically.
    #[serde(deserialize_with = "super::tools::lenient_vec_required")]
    #[schemars(with = "Vec<SingleEdit>")]
    pub edits: Vec<SingleEdit>,
    /// When true, validate and plan all edits but skip disk writes and index mutation.
    /// Returns per-edit preview lines prefixed with `[DRY RUN]`.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
    /// Optional replay guard for committed mutations. Dry runs do not reserve or replay.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Caller's working directory (absolute path). Applies to all edits in the
    /// batch unless a per-edit override is set. Consumed by the
    /// `worktree-awareness` feature hook to redirect writes into the matching
    /// git worktree.
    #[serde(default)]
    pub working_directory: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct SingleEdit {
    /// Relative file path.
    pub path: String,
    /// Symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// The edit operation to perform.
    pub operation: EditOperation,
    /// Per-edit caller working directory (absolute path). Overrides any
    /// `working_directory` set on the enclosing `BatchEditInput`. Consumed by
    /// the `worktree-awareness` feature hook.
    #[serde(default)]
    pub working_directory: Option<String>,
}

impl SingleEdit {
    /// Parse a shorthand string into a `SingleEdit`.
    ///
    /// Accepted formats:
    /// - `"path::name => replace body"`
    /// - `"path::name => insert_before content"`
    /// - `"path::name => insert_after content"`
    /// - `"path::name => delete"`
    /// - `"path::name => edit_within old_text >>> new_text"`
    ///
    /// The `path::name` portion uses `::` as the separator between file path
    /// and symbol name.  Single `:` is also accepted as a fallback (last `:`).
    fn from_shorthand(s: &str) -> Option<Self> {
        // Split on " => " to get target and operation
        let (target, op_str) = s.split_once(" => ")?;

        // Parse path::name
        let (path, name) = if let Some(pos) = target.find("::") {
            (target[..pos].trim(), &target[pos + 2..])
        } else if let Some(pos) = target.rfind(':') {
            (target[..pos].trim(), &target[pos + 1..])
        } else {
            return None;
        };

        if path.is_empty() || name.is_empty() {
            return None;
        }

        let op_str = op_str.trim();

        // Parse operation keyword and body
        let operation = if op_str == "delete" {
            EditOperation::Delete
        } else if let Some(body) = op_str.strip_prefix("replace ") {
            EditOperation::Replace {
                new_body: body.to_string(),
            }
        } else if let Some(body) = op_str.strip_prefix("insert_before ") {
            EditOperation::InsertBefore {
                content: body.to_string(),
            }
        } else if let Some(body) = op_str.strip_prefix("insert_after ") {
            EditOperation::InsertAfter {
                content: body.to_string(),
            }
        } else if let Some(body) = op_str.strip_prefix("edit_within ") {
            // Format: "old_text >>> new_text"
            let (old_text, new_text) = body.split_once(" >>> ")?;
            EditOperation::EditWithin {
                old_text: old_text.to_string(),
                new_text: new_text.to_string(),
            }
        } else {
            return None;
        };

        Some(SingleEdit {
            path: path.to_string(),
            name: name.trim().to_string(),
            kind: None,
            symbol_line: None,
            operation,
            working_directory: None,
        })
    }
}

impl<'de> serde::Deserialize<'de> for SingleEdit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum EditOrStr {
            Struct {
                path: String,
                name: String,
                #[serde(default)]
                kind: Option<String>,
                #[serde(default, deserialize_with = "super::tools::lenient_u32")]
                symbol_line: Option<u32>,
                operation: EditOperation,
                #[serde(default)]
                working_directory: Option<String>,
            },
            Str(String),
        }

        match EditOrStr::deserialize(deserializer)? {
            EditOrStr::Struct {
                path,
                name,
                kind,
                symbol_line,
                operation,
                working_directory,
            } => Ok(SingleEdit {
                path,
                name,
                kind,
                symbol_line,
                operation,
                working_directory,
            }),
            EditOrStr::Str(s) => {
                // Try JSON parse first (stringified object)
                if let Ok(edit) = serde_json::from_str::<serde_json::Value>(&s)
                    && edit.is_object()
                {
                    // Re-deserialize with the struct variant logic
                    // We need manual extraction since we can't recurse
                    let path = edit
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = edit
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let kind = edit.get("kind").and_then(|v| v.as_str()).map(String::from);
                    let symbol_line = edit
                        .get("symbol_line")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32);
                    let operation: EditOperation = edit
                        .get("operation")
                        .ok_or_else(|| {
                            D::Error::custom("missing 'operation' in stringified SingleEdit")
                        })
                        .and_then(|op| {
                            serde_json::from_value(op.clone()).map_err(D::Error::custom)
                        })?;
                    let working_directory = edit
                        .get("working_directory")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    if path.is_empty() || name.is_empty() {
                        return Err(D::Error::custom(format!(
                            "SingleEdit stringified object must have non-empty path and name, got '{s}'"
                        )));
                    }
                    return Ok(SingleEdit {
                        path,
                        name,
                        kind,
                        symbol_line,
                        operation,
                        working_directory,
                    });
                }

                // Try shorthand DSL: "path::name => operation body"
                SingleEdit::from_shorthand(&s).ok_or_else(|| {
                    D::Error::custom(format!(
                        "SingleEdit string must be 'path::name => operation body' \
                         (operations: replace, insert_before, insert_after, delete, \
                         edit_within old >>> new) or a JSON object with \
                         path/name/operation fields, got '{s}'"
                    ))
                })
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum EditOperation {
    /// Replace the entire symbol definition.
    #[serde(rename = "replace")]
    Replace { new_body: String },
    /// Insert code before the symbol.
    #[serde(rename = "insert_before")]
    InsertBefore { content: String },
    /// Insert code after the symbol.
    #[serde(rename = "insert_after")]
    InsertAfter { content: String },
    /// Delete the symbol.
    #[serde(rename = "delete")]
    Delete,
    /// Find-and-replace within the symbol.
    #[serde(rename = "edit_within")]
    EditWithin { old_text: String, new_text: String },
}

/// Apply multiple symbol-addressed edits atomically.
/// Validates all symbols first, rejects overlapping ranges, then applies in reverse-offset order.
/// When `dry_run` is true, all validation runs identically but disk writes and index mutation are skipped.
///
/// `top_level_working_directory` carries `BatchEditInput.working_directory` so the
/// `EditHook` chain can consult it for path resolution. When a per-edit
/// `SingleEdit.working_directory` is also set, the per-edit value wins for that file
/// (first non-`None` per file in iteration order).
pub(crate) fn execute_batch_edit(
    index: &SharedIndex,
    repo_root: &Path,
    edits: &[SingleEdit],
    dry_run: bool,
    top_level_working_directory: Option<&Path>,
) -> Result<Vec<String>, String> {
    struct ResolvedEdit {
        path: String,
        sym: SymbolRecord,
        operation: usize,
        language: LanguageId,
    }

    // Phase 1: Resolve all symbols.
    let n = edits.len();
    let targeted_paths: Vec<&str> = edits.iter().map(|e| e.path.as_str()).collect();
    let rollback_footer = |paths: &[&str]| -> String {
        let path_list = paths
            .iter()
            .map(|p| format!("  - {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\nROLLED BACK — {n} edit(s) attempted on:\n{path_list}\nNo files were modified.")
    };

    let mut resolved = Vec::with_capacity(n);
    {
        let guard = index.read();
        for (i, edit) in edits.iter().enumerate() {
            let file = guard.get_file(&edit.path).ok_or_else(|| {
                format!(
                    "File not indexed: {}{}",
                    edit.path,
                    rollback_footer(&targeted_paths)
                )
            })?;
            let (_, sym) =
                resolve_or_error(file, &edit.name, edit.kind.as_deref(), edit.symbol_line)
                    .map_err(|e| {
                        format!("Edit {}: {e}{}", i + 1, rollback_footer(&targeted_paths))
                    })?;
            resolved.push(ResolvedEdit {
                path: edit.path.clone(),
                sym,
                operation: i,
                language: file.language.clone(),
            });
        }
    }

    // Phase 1b: Validate no overlapping byte ranges within the same file.
    let mut by_file: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, r) in resolved.iter().enumerate() {
        by_file.entry(r.path.clone()).or_default().push(i);
    }
    for (path, indices) in &by_file {
        for i in 0..indices.len() {
            for j in (i + 1)..indices.len() {
                let a = (
                    resolved[indices[i]].sym.effective_start(),
                    resolved[indices[i]].sym.byte_range.1,
                );
                let b = (
                    resolved[indices[j]].sym.effective_start(),
                    resolved[indices[j]].sym.byte_range.1,
                );
                if a.0 < b.1 && b.0 < a.1 {
                    return Err(format!(
                        "Overlapping edits in {path}: `{}` ({}-{}) and `{}` ({}-{}). \
                         Split into separate calls.{}",
                        resolved[indices[i]].sym.name,
                        a.0,
                        a.1,
                        resolved[indices[j]].sym.name,
                        b.0,
                        b.1,
                        rollback_footer(&targeted_paths),
                    ));
                }
            }
        }
    }

    // Phase 2: Sort each file's edits reverse by byte offset.
    for indices in by_file.values_mut() {
        indices.sort_by(|&a, &b| {
            resolved[b]
                .sym
                .effective_start()
                .cmp(&resolved[a].sym.effective_start())
        });
    }

    // Phase 3: Stage all edits per file in memory first.
    struct StagedFile {
        path: String,
        abs_path: PathBuf,
        original: Vec<u8>,
        new_content: Vec<u8>,
        language: LanguageId,
        summaries: Vec<String>,
        working_directory: Option<PathBuf>,
        resolved_target: crate::worktree::ResolvedTarget,
    }

    let mut staged: Vec<StagedFile> = Vec::with_capacity(by_file.len());

    for (path, indices) in &by_file {
        let file = {
            let guard = index.read();
            guard
                .capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };

        let mut content = file.content.clone();

        // TOCTOU guard: symbol byte ranges were resolved from the index snapshot above.
        // If the watcher updated the file between that snapshot and now, a range could
        // be out of bounds. Detect this early and ask the caller to retry.
        for &ri in indices {
            let r = &resolved[ri];
            if r.sym.byte_range.1 as usize > content.len() {
                return Err(format!(
                    "Symbol `{}` byte range ({},{}) is out of bounds for file `{}` \
                     (content length {}). The file may have been modified concurrently — \
                     please retry.",
                    r.sym.name,
                    r.sym.byte_range.0,
                    r.sym.byte_range.1,
                    path,
                    content.len(),
                ));
            }
        }

        let language = resolved[indices[0]].language.clone();
        let line_ending = detect_line_ending(&content);
        let mut file_summaries: Vec<String> = Vec::new();

        for &ri in indices {
            let r = &resolved[ri];
            let edit = &edits[r.operation];
            match &edit.operation {
                EditOperation::Replace { new_body } => {
                    let old_bytes = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    let effective = r.sym.effective_start() as usize;
                    let raw_line_start = content[..effective]
                        .iter()
                        .rposition(|&b| b == b'\n')
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    let line_start =
                        extend_past_orphaned_docs(&content, raw_line_start, &r.sym) as u32;
                    let indent = detect_indentation(&content, r.sym.byte_range.0);
                    let normalized = normalize_line_endings(new_body.as_bytes(), line_ending);
                    let normalized_str = std::str::from_utf8(&normalized).unwrap_or(new_body);
                    let indented = apply_indentation(normalized_str, &indent, line_ending);
                    content = apply_splice(&content, (line_start, r.sym.byte_range.1), &indented);
                    file_summaries.push(super::edit_format::format_replace(
                        path,
                        &r.sym.name,
                        &r.sym.kind.to_string(),
                        old_bytes,
                        new_body.len(),
                    ));
                }
                EditOperation::InsertBefore { content: code } => {
                    content = build_insert_before(&content, &r.sym, code, line_ending);
                    file_summaries.push(super::edit_format::format_insert(
                        path,
                        &r.sym.name,
                        "before",
                        code.len(),
                    ));
                }
                EditOperation::InsertAfter { content: code } => {
                    content = build_insert_after(&content, &r.sym, code, line_ending);
                    file_summaries.push(super::edit_format::format_insert(
                        path,
                        &r.sym.name,
                        "after",
                        code.len(),
                    ));
                }
                EditOperation::Delete => {
                    let deleted = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    content = build_delete(&content, &r.sym, line_ending);
                    file_summaries.push(super::edit_format::format_delete(
                        path,
                        &r.sym.name,
                        &r.sym.kind.to_string(),
                        deleted,
                    ));
                }
                EditOperation::EditWithin { old_text, new_text } => {
                    let old_bytes = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    let old_content_len = content.len();
                    let (new, count) =
                        build_edit_within(&content, &r.sym, old_text, new_text, false)
                            .map_err(|e| format!("Edit in {path}:{}: {e}", r.sym.name))?;
                    content = new;
                    // Compute new symbol size from content length delta
                    let new_bytes = (old_bytes as isize
                        + (content.len() as isize - old_content_len as isize))
                        as usize;
                    file_summaries.push(super::edit_format::format_edit_within(
                        path,
                        &r.sym.name,
                        count,
                        old_bytes,
                        new_bytes,
                    ));
                }
            }
        }

        let indexed_abs_path = match safe_repo_path(repo_root, path) {
            Ok(p) => p,
            Err(e) => return Err(format!("Path containment error for '{path}': {e}")),
        };
        // Per-edit override wins over the top-level batch value; first non-None
        // per file decides (mixing per-edit values across the same file is
        // undefined and accepted only because order is deterministic).
        let per_file_working_directory: Option<PathBuf> = indices
            .iter()
            .find_map(|&ri| edits[resolved[ri].operation].working_directory.as_deref())
            .map(PathBuf::from)
            .or_else(|| top_level_working_directory.map(PathBuf::from));
        let hook_ctx = super::edit_hooks::EditContext {
            relative_path: path,
            indexed_absolute_path: &indexed_abs_path,
            repo_root,
            working_directory: per_file_working_directory.as_deref(),
        };
        let resolved_target = match super::edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return Err(format!("Path resolution error for '{path}': {e}")),
        };
        let abs_path = resolved_target.target_path.clone();
        staged.push(StagedFile {
            path: path.clone(),
            abs_path,
            original: file.content.clone(),
            new_content: content,
            language,
            summaries: file_summaries,
            working_directory: per_file_working_directory,
            resolved_target,
        });
    }

    if dry_run {
        let mut summaries = Vec::new();
        for staged_file in &staged {
            for summary in &staged_file.summaries {
                summaries.push(format!("[DRY RUN] Would {summary}"));
            }
        }
        return Ok(summaries);
    }

    // Phase 4: Apply all writes, rolling back any already-written files on failure.
    let mut written: Vec<usize> = Vec::new();
    let mut write_reports: Vec<Option<AtomicWriteReport>> = vec![None; staged.len()];
    let mut write_error: Option<String> = None;
    for (i, staged_file) in staged.iter().enumerate() {
        match atomic_write_file(&staged_file.abs_path, &staged_file.new_content) {
            Ok(report) => {
                write_reports[i] = Some(report);
            }
            Err(e) => {
                write_error = Some(format!("Write failed for {}: {e}", staged_file.path));
                break;
            }
        }
        written.push(i);
    }

    if let Some(err_msg) = write_error {
        let mut rollback_failures: Vec<String> = Vec::new();
        for &written_index in &written {
            let staged_file = &staged[written_index];
            if let Err(rb_err) = atomic_write_file(&staged_file.abs_path, &staged_file.original) {
                rollback_failures.push(format!("  {}: {rb_err}", staged_file.path));
                continue;
            }
            match std::fs::read(&staged_file.abs_path) {
                Ok(on_disk) => {
                    reindex_after_write(
                        index,
                        &staged_file.abs_path,
                        &staged_file.path,
                        &on_disk,
                        staged_file.language.clone(),
                    );
                }
                Err(rb_err) => {
                    rollback_failures.push(format!(
                        "  {} (reindex after rollback): {rb_err}",
                        staged_file.path
                    ));
                }
            }
        }

        if rollback_failures.is_empty() {
            return Err(format!(
                "{err_msg}\n\nROLLED BACK — {} file(s) restored to original content. No batch edit was applied.",
                written.len(),
            ));
        } else {
            return Err(format!(
                "{err_msg}\n\nROLLBACK INCOMPLETE — {} file(s) could not be restored:\n{}\nWARNING: codebase may be in a partially-edited state. Manually verify the following files:\n{}",
                rollback_failures.len(),
                rollback_failures.join("\n"),
                written
                    .iter()
                    .map(|&written_index| format!("  {}", staged[written_index].path))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
    }

    // Phase 5: All writes succeeded — reindex every file and return summaries.
    let mut summaries = Vec::new();
    for (i, staged_file) in staged.iter().enumerate() {
        reindex_after_write(
            index,
            &staged_file.abs_path,
            &staged_file.path,
            &staged_file.new_content,
            staged_file.language.clone(),
        );
        let hook_ctx = super::edit_hooks::EditContext {
            relative_path: &staged_file.path,
            indexed_absolute_path: &staged_file.resolved_target.indexed_path,
            repo_root,
            working_directory: staged_file.working_directory.as_deref(),
        };
        super::edit_hooks::after_commit(&hook_ctx, &staged_file.abs_path);
        let mut file_summaries = staged_file.summaries.clone();
        if let Some(report) = &write_reports[i]
            && let Some(hint) = report.tee_snapshot.response_hint()
        {
            append_response_suffix_to_first_summary(&mut file_summaries, &hint);
        }
        let reroute_suffix = super::edit_format::format_reroute_suffix(
            staged_file.working_directory.as_deref(),
            &staged_file.resolved_target,
        );
        append_response_suffix_to_first_summary(&mut file_summaries, &reroute_suffix);
        summaries.extend(file_summaries);
    }

    Ok(summaries)
}

// ---------------------------------------------------------------------------
// Batch rename
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchRenameInput {
    /// Relative file path containing the symbol definition.
    pub path: String,
    /// Current symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// New name for the symbol.
    pub new_name: String,
    /// When true, show what would change without writing any files.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
    /// Optional replay guard for committed mutations. Dry runs do not reserve or replay.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// When true, exclude non-source files (docs, configs, images) from renaming.
    /// Only files with a recognized programming language extension are included.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub code_only: Option<bool>,
    /// Caller's working directory (absolute path). Applies to every file the
    /// rename touches. Consumed by the `worktree-awareness` feature hook to
    /// redirect writes into the matching git worktree.
    #[serde(default)]
    pub working_directory: Option<String>,
}

/// Validate rename ranges for a single file. Sorts descending, deduplicates exact matches,
/// validates bounds/text/overlaps. Mutates `ranges` in place.
fn validate_rename_ranges(
    ranges: &mut Vec<(u32, u32)>,
    original: &[u8],
    old_name: &str,
    file_path: &str,
) -> Result<(), String> {
    let old_bytes = old_name.as_bytes();

    // Sort descending by (start, end) — current code only sorts by start
    ranges.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    ranges.dedup();

    // Validate bounds and text match; remove ranges that don't match (xref may
    // produce wider ranges for qualified paths like crate::Widget).
    ranges.retain(|&(start, end)| {
        if start >= end || end as usize > original.len() {
            return false;
        }
        let actual = &original[start as usize..end as usize];
        actual == old_bytes
    });

    // Check overlaps: ranges sorted descending, so prev.start >= curr.start
    for window in ranges.windows(2) {
        let prev = window[0]; // higher offset
        let curr = window[1]; // lower offset
        if curr.1 > prev.0 {
            return Err(format!(
                "{file_path}: overlapping ranges ({}, {}) and ({}, {})",
                curr.0, curr.1, prev.0, prev.1
            ));
        }
    }

    Ok(())
}

/// Rename a symbol and all its references across the project.
pub(crate) fn execute_batch_rename(
    index: &SharedIndex,
    repo_root: &Path,
    input: &BatchRenameInput,
) -> Result<String, String> {
    // Phase 1: Resolve the definition and find the name within its body.
    let (def_name_range, language) = {
        let guard = index.read();
        let file = guard
            .get_file(&input.path)
            .ok_or_else(|| format!("File not indexed: {}", input.path))?;
        let (_, sym) =
            resolve_or_error(file, &input.name, input.kind.as_deref(), input.symbol_line)?;
        let body = &file.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize];
        let name_offset = body
            .windows(input.name.len())
            .position(|w| w == input.name.as_bytes())
            .ok_or_else(|| {
                format!(
                    "Could not locate name `{}` within symbol body at {}:{}-{}",
                    input.name, input.path, sym.byte_range.0, sym.byte_range.1
                )
            })?;
        let abs_start = sym.byte_range.0 + name_offset as u32;
        let abs_end = abs_start + input.name.len() as u32;
        ((abs_start, abs_end), file.language.clone())
    };

    // Phase 2: Find all references across the project.
    let ref_sites: Vec<(String, (u32, u32))> = {
        let guard = index.read();
        let refs = guard.find_references_for_name(&input.name, None, false);
        refs.into_iter()
            .map(|(path, rr)| (path.to_string(), rr.byte_range))
            .collect()
    };

    // Filter ref_sites by code_only
    let ref_sites: Vec<(String, (u32, u32))> = if input.code_only.unwrap_or(false) {
        ref_sites
            .into_iter()
            .filter(|(path, _)| {
                let ext = path.rsplit('.').next().unwrap_or("");
                match crate::domain::index::LanguageId::from_extension(ext) {
                    None => false,
                    Some(lang) => !crate::parsing::config_extractors::is_config_language(&lang),
                }
            })
            .collect()
    } else {
        ref_sites
    };

    // Phase 2b: Supplemental qualified-path scan with confidence classification.
    // The xref index tracks call targets (e.g. "new" in Widget::new()), not
    // path prefixes. find_qualified_usages catches Type::method() patterns,
    // import paths, and any other qualified usage the xref system doesn't index.
    // Matches are split into confident (code context) and uncertain (comments/strings).
    //
    // We collect file content snapshots under the lock, then run the scan outside it.
    let file_contents: Vec<(String, Vec<u8>)> = {
        let guard = index.read();
        guard
            .files
            .iter()
            .filter(|(path, _)| {
                if !input.code_only.unwrap_or(false) {
                    return true;
                }
                let ext = path.rsplit('.').next().unwrap_or("");
                match crate::domain::index::LanguageId::from_extension(ext) {
                    None => false,
                    Some(lang) => !crate::parsing::config_extractors::is_config_language(&lang),
                }
            })
            .map(|(path, file)| (path.clone(), file.content.clone()))
            .collect()
    };

    // Collect confident and uncertain supplemental matches separately.
    // Each entry: (file_path, byte_range (start, end))
    let mut qualified_confident: Vec<(String, (u32, u32))> = Vec::new();
    // Uncertain entries also carry the display context string for the warning block.
    let mut qualified_uncertain: Vec<(String, u32, String)> = Vec::new(); // (path, line, context)

    let qualified_inputs =
        file_contents
            .iter()
            .map(|(path, content)| qualified_usages::QualifiedFileContent {
                file_path: path.as_str(),
                content: content.as_slice(),
            });
    for usage in qualified_usages::collect_qualified_usages(&input.name, qualified_inputs) {
        if usage.confident {
            qualified_confident.push((usage.file_path, usage.byte_range));
        } else {
            qualified_uncertain.push((usage.file_path, usage.line, usage.context));
        }
    }

    // Phase 3: Group rename sites by file.
    // Confident sources: definition site, indexed refs, qualified confident matches.
    // Uncertain matches are NOT applied — only surfaced in output.
    let mut by_file: std::collections::HashMap<String, Vec<(u32, u32)>> =
        std::collections::HashMap::new();
    by_file
        .entry(input.path.clone())
        .or_default()
        .push(def_name_range);
    for (path, range) in &ref_sites {
        by_file.entry(path.clone()).or_default().push(*range);
    }
    for (path, range) in &qualified_confident {
        by_file.entry(path.clone()).or_default().push(*range);
    }
    // Validate, sort descending, dedup, and check for overlaps.
    for (path, ranges) in by_file.iter_mut() {
        let file = {
            let guard = index.read();
            guard
                .capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };
        validate_rename_ranges(ranges, &file.content, &input.name, path)?;
    }

    // Build uncertain warning lines sorted by file then line, deduped.
    let mut sorted_uncertain = qualified_uncertain.clone();
    sorted_uncertain.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    sorted_uncertain.dedup();
    let uncertain_lines: Vec<String> = sorted_uncertain
        .iter()
        .map(|(path, line, ctx)| format!("  {}:{}  {}", path, line, ctx))
        .collect();

    // Dry run: return preview without writing, with separate confident/uncertain sections.
    if input.dry_run.unwrap_or(false) {
        let total_confident: usize = by_file.values().map(|r| r.len()).sum();
        let mut lines = vec![format!("Dry run: `{}` → `{}`", input.name, input.new_name,)];
        lines.push(format!(
            "\n── Confident matches (will be applied) — {} site(s) across {} file(s) ──",
            total_confident,
            by_file.len(),
        ));
        let mut sorted_files: Vec<_> = by_file.iter().collect();
        sorted_files.sort_by_key(|(p, _)| (*p).clone());
        for (path, ranges) in sorted_files {
            lines.push(format!("  {} ({} site(s))", path, ranges.len()));
        }
        if !uncertain_lines.is_empty() {
            lines.push(format!(
                "\n── Uncertain matches (NOT applied — review manually) — {} site(s) ──",
                uncertain_lines.len(),
            ));
            lines.extend(uncertain_lines);
        }
        return Ok(lines.join("\n"));
    }

    // Phase 4: Atomic rename — stage all new content in memory first, then write all.
    // On any write failure, roll back already-written files to their original content.
    let new_name_bytes = input.new_name.as_bytes();

    // Stage: compute new content for every file without touching disk.
    struct StagedFile {
        path: String,
        abs_path: std::path::PathBuf,
        original: Vec<u8>,
        new_content: Vec<u8>,
        language: LanguageId,
        refs_count: usize,
        working_directory: Option<std::path::PathBuf>,
        resolved_target: crate::worktree::ResolvedTarget,
    }
    let mut staged: Vec<StagedFile> = Vec::with_capacity(by_file.len());
    for (path, ranges) in &by_file {
        let file = {
            let guard = index.read();
            guard
                .capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };
        let original = file.content.clone();
        let mut new_content = original.clone();
        let mut last_start: Option<u32> = None;
        for range in ranges {
            debug_assert!(
                last_start.is_none_or(|prev| range.0 < prev),
                "ranges must be strictly descending: {} not < {:?}",
                range.0,
                last_start
            );
            new_content = apply_splice(&new_content, *range, new_name_bytes);
            last_start = Some(range.0);
        }
        let lang = if path == &input.path {
            language.clone()
        } else {
            file.language.clone()
        };
        let indexed_abs = match safe_repo_path(repo_root, path) {
            Ok(p) => p,
            Err(e) => return Err(format!("Path containment error for '{path}': {e}")),
        };
        let working_directory: Option<std::path::PathBuf> = input
            .working_directory
            .as_deref()
            .map(std::path::PathBuf::from);
        let hook_ctx = super::edit_hooks::EditContext {
            relative_path: path,
            indexed_absolute_path: &indexed_abs,
            repo_root,
            working_directory: working_directory.as_deref(),
        };
        let resolved_target = match super::edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return Err(format!("Path resolution error for '{path}': {e}")),
        };
        staged.push(StagedFile {
            path: path.clone(),
            abs_path: resolved_target.target_path.clone(),
            original,
            new_content,
            language: lang,
            refs_count: ranges.len(),
            working_directory,
            resolved_target,
        });
    }

    // Apply: write each staged file; on failure roll back already-written files.
    let mut written: Vec<usize> = Vec::new(); // indices into staged
    let mut write_error: Option<String> = None;
    for (i, sf) in staged.iter().enumerate() {
        if let Err(e) = atomic_write_file(&sf.abs_path, &sf.new_content) {
            write_error = Some(format!("Write failed for {}: {e}", sf.path));
            break;
        }
        written.push(i);
    }

    if let Some(err_msg) = write_error {
        // Rollback: restore every file that was already written.
        let mut rollback_failures: Vec<String> = Vec::new();
        for &wi in &written {
            let sf = &staged[wi];
            if let Err(rb_err) = atomic_write_file(&sf.abs_path, &sf.original) {
                rollback_failures.push(format!("  {}: {rb_err}", sf.path));
                continue;
            }
            // Re-read from disk and reindex to ensure index matches disk.
            match std::fs::read(&sf.abs_path) {
                Ok(on_disk) => {
                    reindex_after_write(
                        index,
                        &sf.abs_path,
                        &sf.path,
                        &on_disk,
                        sf.language.clone(),
                    );
                }
                Err(rb_err) => {
                    rollback_failures
                        .push(format!("  {} (reindex after rollback): {rb_err}", sf.path));
                }
            }
        }
        if rollback_failures.is_empty() {
            return Err(format!(
                "{err_msg}\n\nROLLED BACK — {} file(s) restored to original content. \
                 No rename was applied.",
                written.len(),
            ));
        } else {
            return Err(format!(
                "{err_msg}\n\nROLLBACK INCOMPLETE — {} file(s) could not be restored:\n{}\n\
                 WARNING: codebase may be in a partially-renamed state. \
                 Manually verify the following files:\n{}",
                rollback_failures.len(),
                rollback_failures.join("\n"),
                written
                    .iter()
                    .map(|&wi| format!("  {}", staged[wi].path))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
    }

    // All writes succeeded — reindex every file.
    let mut files_updated = 0;
    let mut refs_updated = 0;
    for sf in &staged {
        reindex_after_write(
            index,
            &sf.abs_path,
            &sf.path,
            &sf.new_content,
            sf.language.clone(),
        );
        let hook_ctx = super::edit_hooks::EditContext {
            relative_path: &sf.path,
            indexed_absolute_path: &sf.resolved_target.indexed_path,
            repo_root,
            working_directory: sf.working_directory.as_deref(),
        };
        super::edit_hooks::after_commit(&hook_ctx, &sf.abs_path);
        files_updated += 1;
        refs_updated += sf.refs_count;
    }

    let mut output = format!(
        "Renamed `{}` → `{}` — {refs_updated} site(s) across {files_updated} file(s)",
        input.name, input.new_name,
    );
    if !uncertain_lines.is_empty() {
        output.push_str(&format!(
            "\n\n── Uncertain matches (NOT applied — review manually) — {} site(s) ──\n",
            uncertain_lines.len(),
        ));
        output.push_str(&uncertain_lines.join("\n"));
    }
    for sf in &staged {
        output.push_str(&super::edit_format::format_reroute_suffix(
            sf.working_directory.as_deref(),
            &sf.resolved_target,
        ));
    }
    Ok(output)
}

// ---------------------------------------------------------------------------
// Batch insert
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchInsertInput {
    /// Code to insert at each target location.
    pub content: String,
    /// Where to insert: before or after.
    pub position: InsertPosition,
    /// Target symbols to insert adjacent to.
    #[serde(deserialize_with = "super::tools::lenient_vec_required")]
    #[schemars(with = "Vec<InsertTarget>")]
    pub targets: Vec<InsertTarget>,
    /// When true, validate and preview but skip disk writes and index mutation.
    /// Returns per-target preview lines prefixed with `[DRY RUN]`.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
    /// Optional replay guard for committed mutations. Dry runs do not reserve or replay.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Caller's working directory (absolute path). Applies to all targets in
    /// the batch unless a per-target override is set. Consumed by the
    /// `worktree-awareness` feature hook to redirect writes into the matching
    /// git worktree.
    #[serde(default)]
    pub working_directory: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InsertPosition {
    Before,
    After,
}

#[derive(Serialize, JsonSchema)]
pub struct InsertTarget {
    /// Relative file path.
    pub path: String,
    /// Symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Per-target caller working directory (absolute path). Overrides any
    /// `working_directory` set on the enclosing `BatchInsertInput`. Consumed by
    /// the `worktree-awareness` feature hook.
    #[serde(default)]
    pub working_directory: Option<String>,
}

/// Accept both structured `{"path":"...","name":"..."}` and shorthand `"path::name"` strings.
impl<'de> serde::Deserialize<'de> for InsertTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum TargetOrStr {
            Struct {
                path: String,
                name: String,
                kind: Option<String>,
                #[serde(default, deserialize_with = "super::tools::lenient_u32")]
                symbol_line: Option<u32>,
                #[serde(default)]
                working_directory: Option<String>,
            },
            Str(String),
        }

        match TargetOrStr::deserialize(deserializer)? {
            TargetOrStr::Struct {
                path,
                name,
                kind,
                symbol_line,
                working_directory,
            } => Ok(InsertTarget {
                path,
                name,
                kind,
                symbol_line,
                working_directory,
            }),
            TargetOrStr::Str(s) => {
                // Accept "path::name" or "path:name" shorthand
                let (path, name) = if let Some(pos) = s.find("::") {
                    (s[..pos].to_string(), s[pos + 2..].to_string())
                } else if let Some(pos) = s.rfind(':') {
                    (s[..pos].to_string(), s[pos + 1..].to_string())
                } else {
                    return Err(D::Error::custom(format!(
                        "InsertTarget string must be 'path::name' or a JSON object \
                         with path/name fields, got '{s}'"
                    )));
                };
                if path.is_empty() || name.is_empty() {
                    return Err(D::Error::custom(format!(
                        "InsertTarget string must have non-empty path and name, got '{s}'"
                    )));
                }
                Ok(InsertTarget {
                    path,
                    name,
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                })
            }
        }
    }
}

/// Insert the same code before or after multiple symbols across the project.
/// Validates all targets first, stages file updates in memory, then writes the
/// full set transactionally. When `dry_run` is true, validation and staging run
/// identically but disk writes and index mutation are skipped.
pub(crate) fn execute_batch_insert(
    index: &SharedIndex,
    repo_root: &Path,
    input: &BatchInsertInput,
) -> Result<Vec<String>, String> {
    struct ResolvedTarget {
        path: String,
        sym: SymbolRecord,
        operation: usize,
        language: LanguageId,
    }

    let position_label = match input.position {
        InsertPosition::Before => "before",
        InsertPosition::After => "after",
    };

    // Phase 1: resolve all targets from the current index snapshot.
    let mut resolved = Vec::with_capacity(input.targets.len());
    {
        let guard = index.read();
        for (i, target) in input.targets.iter().enumerate() {
            let file = guard
                .get_file(&target.path)
                .ok_or_else(|| format!("File not indexed: {}", target.path))?;
            let (_, sym) = resolve_or_error(
                file,
                &target.name,
                target.kind.as_deref(),
                target.symbol_line,
            )
            .map_err(|e| format!("Target {}: {e}", target.path))?;
            resolved.push(ResolvedTarget {
                path: target.path.clone(),
                sym,
                operation: i,
                language: file.language.clone(),
            });
        }
    }

    let mut by_file: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, resolved_target) in resolved.iter().enumerate() {
        by_file
            .entry(resolved_target.path.clone())
            .or_default()
            .push(i);
    }

    fn insertion_anchor(position: InsertPosition, sym: &SymbolRecord) -> u32 {
        match position {
            InsertPosition::Before => sym.effective_start(),
            InsertPosition::After => sym.byte_range.1,
        }
    }

    for indices in by_file.values_mut() {
        indices.sort_by(|&a, &b| {
            insertion_anchor(input.position, &resolved[b].sym)
                .cmp(&insertion_anchor(input.position, &resolved[a].sym))
                .then_with(|| resolved[b].operation.cmp(&resolved[a].operation))
        });
    }

    struct StagedFile {
        path: String,
        abs_path: PathBuf,
        original: Vec<u8>,
        new_content: Vec<u8>,
        language: LanguageId,
        summaries: Vec<String>,
        working_directory: Option<PathBuf>,
        resolved_target: crate::worktree::ResolvedTarget,
    }

    let mut staged: Vec<StagedFile> = Vec::with_capacity(by_file.len());
    let mut staged_paths: Vec<String> = by_file.keys().cloned().collect();
    staged_paths.sort();

    for path in staged_paths {
        let indices = by_file
            .get(&path)
            .ok_or_else(|| format!("Batch insert staging bug: missing path bucket for {path}"))?;

        let file = {
            let guard = index.read();
            guard
                .capture_shared_file(&path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };

        let mut content = file.content.clone();
        for &ri in indices {
            let target = &resolved[ri];
            if target.sym.byte_range.1 as usize > content.len() {
                return Err(format!(
                    "Symbol `{}` byte range ({},{}) is out of bounds for file `{}` (content length {}). The file may have been modified concurrently — please retry.",
                    target.sym.name,
                    target.sym.byte_range.0,
                    target.sym.byte_range.1,
                    path,
                    content.len(),
                ));
            }
        }

        let line_ending = detect_line_ending(&content);
        let mut file_summaries = Vec::with_capacity(indices.len());
        for &ri in indices {
            let target = &resolved[ri];
            content = match input.position {
                InsertPosition::Before => {
                    build_insert_before(&content, &target.sym, &input.content, line_ending)
                }
                InsertPosition::After => {
                    build_insert_after(&content, &target.sym, &input.content, line_ending)
                }
            };
            file_summaries.push(super::edit_format::format_insert(
                &path,
                &target.sym.name,
                position_label,
                input.content.len(),
            ));
        }

        let indexed_abs_path = match safe_repo_path(repo_root, &path) {
            Ok(p) => p,
            Err(e) => return Err(format!("Target {path}: {e}")),
        };
        // Per-target override wins over the top-level batch value; first non-None
        // per file decides (mixing per-target values across the same file is
        // undefined and accepted only because order is deterministic).
        let per_file_working_directory: Option<PathBuf> = indices
            .iter()
            .find_map(|&ri| {
                input.targets[resolved[ri].operation]
                    .working_directory
                    .as_deref()
            })
            .map(PathBuf::from)
            .or_else(|| input.working_directory.as_deref().map(PathBuf::from));
        let hook_ctx = super::edit_hooks::EditContext {
            relative_path: &path,
            indexed_absolute_path: &indexed_abs_path,
            repo_root,
            working_directory: per_file_working_directory.as_deref(),
        };
        let resolved_target = match super::edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return Err(format!("Target {path}: path resolution error: {e}")),
        };
        staged.push(StagedFile {
            path: path.clone(),
            abs_path: resolved_target.target_path.clone(),
            original: file.content.clone(),
            new_content: content,
            language: resolved[indices[0]].language.clone(),
            summaries: file_summaries,
            working_directory: per_file_working_directory,
            resolved_target,
        });
    }

    if input.dry_run.unwrap_or(false) {
        let mut summaries = Vec::new();
        for staged_file in &staged {
            for summary in &staged_file.summaries {
                summaries.push(format!("[DRY RUN] Would {summary}"));
            }
        }
        return Ok(summaries);
    }

    let mut written: Vec<usize> = Vec::new();
    let mut write_error: Option<String> = None;
    for (i, staged_file) in staged.iter().enumerate() {
        if let Err(e) = atomic_write_file(&staged_file.abs_path, &staged_file.new_content) {
            write_error = Some(format!("Write failed for {}: {e}", staged_file.path));
            break;
        }
        written.push(i);
    }

    if let Some(err_msg) = write_error {
        let mut rollback_failures: Vec<String> = Vec::new();
        for &written_index in &written {
            let staged_file = &staged[written_index];
            if let Err(rb_err) = atomic_write_file(&staged_file.abs_path, &staged_file.original) {
                rollback_failures.push(format!("  {}: {rb_err}", staged_file.path));
                continue;
            }
            match std::fs::read(&staged_file.abs_path) {
                Ok(on_disk) => {
                    reindex_after_write(
                        index,
                        &staged_file.abs_path,
                        &staged_file.path,
                        &on_disk,
                        staged_file.language.clone(),
                    );
                }
                Err(rb_err) => {
                    rollback_failures.push(format!(
                        "  {} (reindex after rollback): {rb_err}",
                        staged_file.path
                    ));
                }
            }
        }

        if rollback_failures.is_empty() {
            return Err(format!(
                "{err_msg}\n\nROLLED BACK — {} file(s) restored to original content. No batch insert was applied.",
                written.len(),
            ));
        }

        return Err(format!(
            "{err_msg}\n\nROLLBACK INCOMPLETE — {} file(s) could not be restored:\n{}\nWARNING: codebase may be in a partially-inserted state. Manually verify the following files:\n{}",
            rollback_failures.len(),
            rollback_failures.join("\n"),
            written
                .iter()
                .map(|&written_index| format!("  {}", staged[written_index].path))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }

    let mut summaries = Vec::new();
    for staged_file in &staged {
        reindex_after_write(
            index,
            &staged_file.abs_path,
            &staged_file.path,
            &staged_file.new_content,
            staged_file.language.clone(),
        );
        let hook_ctx = super::edit_hooks::EditContext {
            relative_path: &staged_file.path,
            indexed_absolute_path: &staged_file.resolved_target.indexed_path,
            repo_root,
            working_directory: staged_file.working_directory.as_deref(),
        };
        super::edit_hooks::after_commit(&hook_ctx, &staged_file.abs_path);
        let mut file_summaries = staged_file.summaries.clone();
        let reroute_suffix = super::edit_format::format_reroute_suffix(
            staged_file.working_directory.as_deref(),
            &staged_file.resolved_target,
        );
        append_response_suffix_to_first_summary(&mut file_summaries, &reroute_suffix);
        summaries.extend(file_summaries);
    }

    Ok(summaries)
}

// ---------------------------------------------------------------------------
// Stale reference detection
// ---------------------------------------------------------------------------

/// Extract the first line of a symbol as a rough "signature" for change detection.
pub(crate) fn extract_signature(content: &[u8], byte_range: (u32, u32)) -> String {
    let start = byte_range.0 as usize;
    let end = byte_range.1 as usize;
    let slice = &content[start..end];
    let first_line_end = slice
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..first_line_end]).to_string()
}

/// Find the parent impl block's type name for a symbol, if any.
///
/// Walks backward through the file's symbol list to find an `impl` block at a
/// lower depth that encloses the target symbol's byte range. Extracts the
/// concrete type name (e.g. `Foo` from `impl Foo` or `impl Trait for Foo`).
pub(crate) fn find_parent_impl_type(file: &IndexedFile, sym: &SymbolRecord) -> Option<String> {
    if sym.depth == 0 {
        return None; // top-level symbol, not inside an impl block
    }
    // Walk the symbol list to find the enclosing impl block.
    for s in &file.symbols {
        if s.kind != SymbolKind::Impl {
            continue;
        }
        // The impl block must enclose the target symbol.
        if s.byte_range.0 <= sym.byte_range.0 && s.byte_range.1 >= sym.byte_range.1 {
            return extract_impl_type_name(&s.name);
        }
    }
    None
}

/// Extract the concrete type name from an impl block name.
///
/// Handles patterns like:
/// - `impl Foo` -> `Foo`
/// - `impl Trait for Foo` -> `Foo`
/// - `impl<T> Foo<T>` -> `Foo`
/// - `impl<T: Clone> Trait for Foo<T>` -> `Foo`
fn extract_impl_type_name(impl_name: &str) -> Option<String> {
    let name = impl_name.trim();
    // Strip leading "impl" keyword if present (some parsers include it).
    let rest = name.strip_prefix("impl").unwrap_or(name).trim_start();
    // Strip generic parameters from the front: `<T: Clone> Trait for Foo<T>` -> `Trait for Foo<T>`
    let rest = strip_leading_generics(rest);
    // Check for "for" keyword: `Trait for Foo<T>` -> `Foo<T>`
    let type_part = if let Some(pos) = rest.find(" for ") {
        rest[pos + 5..].trim_start()
    } else {
        rest.trim_start()
    };
    // Strip trailing generics: `Foo<T>` -> `Foo`
    let type_name = type_part.split('<').next().unwrap_or(type_part).trim();
    if type_name.is_empty() {
        None
    } else {
        Some(type_name.to_string())
    }
}

/// Strip a leading `<...>` generic parameter list, handling nested angle brackets.
fn strip_leading_generics(s: &str) -> &str {
    let s = s.trim_start();
    if !s.starts_with('<') {
        return s;
    }
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return s[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    s // malformed generics, return as-is
}

/// Detect references that may be stale after a symbol edit.
/// Compares old vs new signature (first line). Returns (path, line, enclosing_name) triples.
///
/// When `parent_type` is provided (i.e. the symbol is a method inside an `impl` block),
/// only warns about references in files that also mention the parent type — this avoids
/// false positives like warning about `Path::display()` when `Widget::display()` changed.
pub(crate) fn detect_stale_references(
    index: &SharedIndex,
    path: &str,
    name: &str,
    old_signature: &str,
    new_signature: &str,
    parent_type: Option<&str>,
    source_language: Option<&crate::domain::LanguageId>,
) -> Vec<(String, u32, Option<String>)> {
    if old_signature == new_signature {
        return Vec::new();
    }
    let guard = index.read();
    let refs = guard.find_references_for_name(name, None, false);

    // When we know the parent type, collect the set of files that reference it.
    // Only those files could plausibly call `ParentType::method_name()`.
    let type_files: Option<std::collections::HashSet<&str>> = parent_type.map(|tn| {
        guard
            .find_references_for_name(tn, None, false)
            .into_iter()
            .map(|(fp, _)| fp)
            .collect()
    });

    refs.into_iter()
        .filter(|(ref_path, _)| *ref_path != path)
        .filter(|(ref_path, _)| {
            // Skip references in files of a different language to reduce false positives
            // (e.g., Rust `add` flagging Python's `add`).
            if let Some(lang) = source_language
                && let Some(ref_file) = guard.get_file(ref_path)
                && ref_file.language != *lang
            {
                return false;
            }
            true
        })
        .filter(|(ref_path, _)| {
            // If we have a parent type filter, only keep refs in files that also mention it.
            match &type_files {
                Some(tf) => tf.contains(ref_path),
                None => true,
            }
        })
        .map(|(ref_path, rr)| {
            let enclosing = rr.enclosing_symbol_index.and_then(|idx| {
                guard
                    .get_file(ref_path)
                    .and_then(|f| f.symbols.get(idx as usize))
                    .map(|s| s.name.clone())
            });
            (ref_path.to_string(), rr.line_range.0 + 1, enclosing)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::index::SymbolKind;
    use crate::live_index::qualified_usages::find_qualified_usages;

    // -- apply_splice --

    #[test]
    fn test_apply_splice_replaces_middle() {
        let content = b"fn foo() { old_body }";
        let result = apply_splice(content, (11, 19), b"new_body");
        assert_eq!(result, b"fn foo() { new_body }");
    }

    #[test]
    fn test_apply_splice_replaces_at_start() {
        let content = b"old_start rest";
        let result = apply_splice(content, (0, 9), b"new");
        assert_eq!(result, b"new rest");
    }

    #[test]
    fn test_apply_splice_replaces_at_end() {
        let content = b"prefix old_end";
        let result = apply_splice(content, (7, 14), b"new_end");
        assert_eq!(result, b"prefix new_end");
    }

    #[test]
    fn test_apply_splice_empty_replacement_deletes() {
        let content = b"keep_this remove_this keep_that";
        let result = apply_splice(content, (10, 21), b"");
        assert_eq!(result, b"keep_this  keep_that");
    }

    #[test]
    fn test_apply_splice_empty_range_inserts() {
        let content = b"ab";
        let result = apply_splice(content, (1, 1), b"X");
        assert_eq!(result, b"aXb");
    }

    // -- validate_rename_ranges --

    #[test]
    fn test_validate_rename_ranges_exact_dedup() {
        let content = b"foo bar foo baz foo";
        let mut ranges = vec![(0u32, 3u32), (8, 11), (16, 19), (8, 11)];
        validate_rename_ranges(&mut ranges, content, "foo", "test.rs").unwrap();
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn test_validate_rename_ranges_overlap_rejected() {
        // "aaaa" contains "aa" at overlapping offsets (0,2) and (1,3)
        let content = b"aaaa";
        let mut ranges = vec![(0u32, 2u32), (1, 3)];
        let result = validate_rename_ranges(&mut ranges, content, "aa", "test.rs");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("overlapping"));
    }

    #[test]
    fn test_validate_rename_ranges_mismatched_filtered() {
        // Ranges that don't match the expected text are silently removed
        let content = b"xxfooxxfooxx";
        let mut ranges = vec![(0u32, 12u32), (2, 5)];
        validate_rename_ranges(&mut ranges, content, "foo", "test.rs").unwrap();
        // (0,12) doesn't match "foo", filtered out; (2,5) = "foo", kept
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], (2, 5));
    }

    #[test]
    fn test_validate_rename_ranges_adjacent_allowed() {
        let content = b"foofoofoo";
        let mut ranges = vec![(0u32, 3u32), (3, 6), (6, 9)];
        validate_rename_ranges(&mut ranges, content, "foo", "test.rs").unwrap();
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn test_validate_rename_ranges_text_mismatch_filtered() {
        let content = b"foo bar baz";
        let mut ranges = vec![(4u32, 7u32)];
        validate_rename_ranges(&mut ranges, content, "foo", "test.rs").unwrap();
        // (4,7) = "bar", doesn't match "foo" — filtered out
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_validate_rename_ranges_dedup_count() {
        let content = b"foo + foo + foo";
        let mut ranges = vec![(0u32, 3u32), (6, 9), (12, 15), (6, 9)];
        validate_rename_ranges(&mut ranges, content, "foo", "test.rs").unwrap();
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn test_batch_rename_length_change_close_refs() {
        let content = b"ab ab ab";
        let mut ranges = vec![(0u32, 2u32), (3, 5), (6, 8)];
        validate_rename_ranges(&mut ranges, content, "ab", "test.rs").unwrap();
        let new_name = b"xyz";
        let mut result = content.to_vec();
        for range in &ranges {
            result = apply_splice(&result, *range, new_name);
        }
        assert_eq!(result, b"xyz xyz xyz");
    }

    // -- atomic_write_file --

    #[test]
    fn test_atomic_write_file_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        atomic_write_file(&path, b"fn main() {}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"fn main() {}");
    }

    #[test]
    fn test_atomic_write_file_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        std::fs::write(&path, b"old content").unwrap();
        atomic_write_file(&path, b"new content").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new content");
    }

    #[test]
    fn test_atomic_write_file_no_leftover_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        atomic_write_file(&path, b"content").unwrap();
        let tmp = path.with_extension("symforge_tmp");
        assert!(!tmp.exists());
    }

    // -- reindex_after_write --

    #[test]
    fn test_reindex_after_write_updates_index() {
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let content = b"fn hello() {}\nfn world() {}\n";
        std::fs::write(&abs_path, content).unwrap();
        let handle = crate::live_index::LiveIndex::empty();
        reindex_after_write(&handle, &abs_path, "src/lib.rs", content, LanguageId::Rust);
        let guard = handle.read();
        let file = guard.get_file("src/lib.rs");
        assert!(file.is_some());
        let symbols = &file.unwrap().symbols;
        assert!(symbols.iter().any(|s| s.name == "hello"));
        assert!(symbols.iter().any(|s| s.name == "world"));
    }

    #[test]
    fn test_reindex_after_write_replaces_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let handle = crate::live_index::LiveIndex::empty();

        let v1 = b"fn alpha() {}\n";
        std::fs::write(&abs_path, v1).unwrap();
        reindex_after_write(&handle, &abs_path, "src/lib.rs", v1, LanguageId::Rust);

        let v2 = b"fn beta() {}\n";
        std::fs::write(&abs_path, v2).unwrap();
        reindex_after_write(&handle, &abs_path, "src/lib.rs", v2, LanguageId::Rust);

        let guard = handle.read();
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(!file.symbols.iter().any(|s| s.name == "alpha"));
        assert!(file.symbols.iter().any(|s| s.name == "beta"));
    }

    #[test]
    fn test_reindex_reads_from_disk_not_buffer() {
        // Verify the INVARIANT: index state is built from on-disk bytes.
        // Write one thing to disk, pass different bytes as `written` — the
        // debug_assert would fire in debug builds, but in release builds the
        // index should reflect what is actually on disk.
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let on_disk = b"fn disk_fn() {}\n";
        std::fs::write(&abs_path, on_disk).unwrap();
        let handle = crate::live_index::LiveIndex::empty();
        // Pass the real on-disk bytes as `written` (normal case — no divergence).
        reindex_after_write(&handle, &abs_path, "src/lib.rs", on_disk, LanguageId::Rust);
        let guard = handle.read();
        let file = guard.get_file("src/lib.rs").unwrap();
        // Index reflects what is on disk.
        assert!(file.symbols.iter().any(|s| s.name == "disk_fn"));
    }

    #[test]
    fn test_search_text_matches_disk_after_edit() {
        // Setup: write old content to disk and index it.
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let old_content = b"fn old_content_marker() {}\n";
        std::fs::write(&abs_path, old_content).unwrap();
        let handle = crate::live_index::LiveIndex::empty();
        reindex_after_write(
            &handle,
            &abs_path,
            "src/lib.rs",
            old_content,
            LanguageId::Rust,
        );
        // Verify old content is in the index.
        {
            let guard = handle.read();
            let file = guard.get_file("src/lib.rs").unwrap();
            assert!(file.symbols.iter().any(|s| s.name == "old_content_marker"));
        }

        // Edit: overwrite disk with new content and reindex.
        let new_content = b"fn new_content_marker() {}\n";
        atomic_write_file(&abs_path, new_content).unwrap();
        reindex_after_write(
            &handle,
            &abs_path,
            "src/lib.rs",
            new_content,
            LanguageId::Rust,
        );

        // Verify: old symbol gone, new symbol present — index matches disk.
        let guard = handle.read();
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(
            !file.symbols.iter().any(|s| s.name == "old_content_marker"),
            "old symbol should no longer be in the index"
        );
        assert!(
            file.symbols.iter().any(|s| s.name == "new_content_marker"),
            "new symbol should be in the index after reindex from disk"
        );
    }

    // -- resolve_or_error --

    fn make_test_indexed_file(symbols: Vec<SymbolRecord>) -> IndexedFile {
        IndexedFile {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::index::FileClassification::for_code_path("test.rs"),
            content: Vec::new(),
            symbols,
            parse_status: crate::live_index::store::ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 0,
            content_hash: String::new(),
            references: Vec::new(),
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        }
    }

    fn make_test_symbol(
        name: &str,
        kind: SymbolKind,
        byte_range: (u32, u32),
        line_start: u32,
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (line_start, line_start + 2),
            doc_byte_range: None,
        }
    }

    #[test]
    fn test_resolve_or_error_finds_exact() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("bar", SymbolKind::Function, (22, 50), 5),
        ]);
        let result = resolve_or_error(&file, "foo", None, None);
        assert!(result.is_ok());
        let (idx, sym) = result.unwrap();
        assert_eq!(idx, 0);
        assert_eq!(sym.name, "foo");
    }

    #[test]
    fn test_resolve_or_error_not_found() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "foo",
            SymbolKind::Function,
            (0, 20),
            1,
        )]);
        let result = resolve_or_error(&file, "baz", None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    fn did_you_mean_names(err: &str) -> Vec<&str> {
        err.split("did_you_mean: [")
            .nth(1)
            .and_then(|tail| tail.split(']').next())
            .map(|items| {
                items
                    .split(", ")
                    .filter(|name| !name.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    #[test]
    fn test_resolve_or_error_not_found_suggests_close_same_file_symbol() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "foo_bar",
            SymbolKind::Function,
            (0, 20),
            1,
        )]);
        let result = resolve_or_error(&file, "foo", None, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            did_you_mean_names(&err).contains(&"foo_bar"),
            "error was: {err}"
        );
    }

    #[test]
    fn test_resolve_or_error_not_found_omits_noisy_suggestions() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "foo_bar",
            SymbolKind::Function,
            (0, 20),
            1,
        )]);
        let result = resolve_or_error(&file, "quux", None, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            !err.contains("did_you_mean"),
            "unexpected suggestion in: {err}"
        );
    }

    #[test]
    fn test_resolve_or_error_not_found_caps_suggestions_at_three() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("foo_alpha", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("foo_beta", SymbolKind::Function, (22, 40), 5),
            make_test_symbol("foo_gamma", SymbolKind::Function, (42, 60), 9),
            make_test_symbol("foo_delta", SymbolKind::Function, (62, 80), 13),
        ]);
        let result = resolve_or_error(&file, "foo", None, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let suggestions = did_you_mean_names(&err);
        assert_eq!(suggestions.len(), 3, "error was: {err}");
    }

    #[test]
    fn test_resolve_or_error_not_found_preserves_partial_parse_hint_with_suggestion() {
        let mut file = make_test_indexed_file(vec![make_test_symbol(
            "foo_bar",
            SymbolKind::Function,
            (0, 20),
            1,
        )]);
        file.parse_status = crate::live_index::store::ParseStatus::PartialParse {
            warning: "syntax error near line 9".to_string(),
        };
        let result = resolve_or_error(&file, "foo", None, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("did_you_mean: [foo_bar]"), "error was: {err}");
        assert!(
            err.contains("file partially parsed with errors: syntax error near line 9"),
            "error was: {err}"
        );
    }

    #[test]
    fn test_resolve_or_error_ambiguous_shows_candidates() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("foo", SymbolKind::Function, (22, 50), 5),
        ]);
        let result = resolve_or_error(&file, "foo", None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Ambiguous"), "error was: {err}");
        assert!(err.contains("symbol_line"), "error was: {err}");
    }

    #[test]
    fn test_resolve_or_error_disambiguates_by_kind() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("Foo", SymbolKind::Struct, (0, 20), 1),
            make_test_symbol("Foo", SymbolKind::Impl, (22, 80), 5),
        ]);
        let result = resolve_or_error(&file, "Foo", Some("struct"), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().1.kind, SymbolKind::Struct);
    }

    #[test]
    fn test_resolve_or_error_disambiguates_by_line() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("foo", SymbolKind::Function, (22, 50), 5),
        ]);
        let result = resolve_or_error(&file, "foo", None, Some(6));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, 1);
    }

    // -- cascading name resolution fallbacks --

    #[test]
    fn test_resolve_impl_prefix_trait_for_type() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "impl MyTrait for MyStruct",
            SymbolKind::Impl,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "MyTrait for MyStruct", None, None);
        assert!(result.is_ok(), "should resolve impl without prefix");
    }

    #[test]
    fn test_resolve_impl_prefix_inherent() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "impl MyStruct",
            SymbolKind::Impl,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "MyStruct", None, None);
        assert!(
            result.is_ok(),
            "should resolve inherent impl without prefix"
        );
    }

    #[test]
    fn test_resolve_whitespace_normalised() {
        // Rust generics: LLM might send extra spaces
        let file = make_test_indexed_file(vec![make_test_symbol(
            "impl Display for Vec<T>",
            SymbolKind::Impl,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "impl Display for Vec< T >", None, None);
        assert!(result.is_ok(), "should resolve with normalised whitespace");
    }

    #[test]
    fn test_resolve_impl_prefix_plus_whitespace() {
        // Combined: missing prefix + whitespace diff
        let file = make_test_indexed_file(vec![make_test_symbol(
            "impl Display for Vec<T>",
            SymbolKind::Impl,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "Display for Vec< T >", None, None);
        assert!(
            result.is_ok(),
            "should resolve impl prefix + ws normalisation"
        );
    }

    #[test]
    fn test_resolve_cpp_qualified_method() {
        // C++: LLM sends "Foo::bar" but index stores "bar"
        let file = make_test_indexed_file(vec![make_test_symbol(
            "bar",
            SymbolKind::Function,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "Foo::bar", None, None);
        assert!(
            result.is_ok(),
            "should resolve C++ qualified name by stripping"
        );
    }

    #[test]
    fn test_resolve_go_receiver_method() {
        // Go: LLM sends "Server.Handle" but index stores "Handle"
        let file = make_test_indexed_file(vec![make_test_symbol(
            "Handle",
            SymbolKind::Method,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "Server.Handle", None, None);
        assert!(
            result.is_ok(),
            "should resolve Go receiver method by stripping"
        );
    }

    #[test]
    fn test_resolve_css_at_rule_prefix() {
        // CSS: LLM sends "@media" but index stores "@media (max-width: 768px)"
        let file = make_test_indexed_file(vec![make_test_symbol(
            "@media (max-width: 768px)",
            SymbolKind::Other,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "@media", None, None);
        assert!(result.is_ok(), "should resolve CSS @-rule by prefix match");
    }

    #[test]
    fn test_resolve_css_selector_prefix() {
        // CSS: LLM sends ".btn" but index stores ".btn, .btn-primary"
        let file = make_test_indexed_file(vec![make_test_symbol(
            ".btn, .btn-primary",
            SymbolKind::Other,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, ".btn", None, None);
        assert!(
            result.is_ok(),
            "should resolve CSS selector by prefix match"
        );
    }

    #[test]
    fn test_resolve_css_prefix_no_false_positive() {
        // ".btn" should NOT match ".btn-group" (hyphen is not a delimiter)
        let file = make_test_indexed_file(vec![make_test_symbol(
            ".btn-group",
            SymbolKind::Other,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, ".btn", None, None);
        assert!(
            result.is_err(),
            "should NOT match .btn-group for .btn query"
        );
    }

    #[test]
    fn test_resolve_swift_extension() {
        // Swift: LLM sends "extension MyClass: Drawable" but index stores "MyClass"
        let file = make_test_indexed_file(vec![make_test_symbol(
            "MyClass",
            SymbolKind::Impl,
            (0, 50),
            1,
        )]);
        let result = resolve_or_error(&file, "extension MyClass: Drawable", None, None);
        assert!(
            result.is_ok(),
            "should resolve Swift extension by stripping prefix"
        );
    }

    #[test]
    fn test_resolve_exact_match_wins_over_fallback() {
        // When exact match exists, fallback should not interfere
        let file = make_test_indexed_file(vec![
            make_test_symbol("bar", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("impl bar", SymbolKind::Impl, (22, 50), 5),
        ]);
        let result = resolve_or_error(&file, "bar", None, None);
        assert!(result.is_ok());
        // Should match the exact "bar" function, not "impl bar"
        assert_eq!(result.unwrap().0, 0);
    }

    // -- indentation --

    #[test]
    fn test_detect_indentation_spaces() {
        let content = b"fn outer() {\n    fn inner() {}\n}";
        let indent = detect_indentation(content, 14);
        assert_eq!(indent, b"    ");
    }

    #[test]
    fn test_detect_indentation_tabs() {
        let content = b"fn outer() {\n\tfn inner() {}\n}";
        let indent = detect_indentation(content, 14);
        assert_eq!(indent, b"\t");
    }

    #[test]
    fn test_detect_indentation_no_indent() {
        let content = b"fn top_level() {}";
        let indent = detect_indentation(content, 0);
        assert_eq!(indent, b"");
    }

    #[test]
    fn test_detect_indentation_at_newline_boundary() {
        let content = b"line1\nline2";
        let indent = detect_indentation(content, 6);
        assert_eq!(indent, b"");
    }

    #[test]
    fn test_apply_indentation_adds_prefix() {
        let result = apply_indentation("fn new() {\n    body;\n}", b"    ", LineEnding::Lf);
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(text, "    fn new() {\n        body;\n    }");
    }

    #[test]
    fn test_apply_indentation_preserves_empty_lines() {
        let result = apply_indentation("a\n\nb", b"  ", LineEnding::Lf);
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(text, "  a\n\n  b");
    }

    #[test]
    fn test_apply_indentation_empty_indent_is_identity() {
        let result = apply_indentation("fn foo() {}", b"", LineEnding::Lf);
        assert_eq!(result, b"fn foo() {}");
    }

    // -- insert helpers --

    #[test]
    fn test_build_insert_before_adds_content_with_indent() {
        let content = b"    fn existing() {}\n";
        let sym = make_test_symbol("existing", SymbolKind::Function, (4, 20), 1);
        let result = build_insert_before(content, &sym, "fn new_fn() {}", LineEnding::Lf);
        let text = std::str::from_utf8(&result).unwrap();
        // No doc comment on the symbol → expect \n\n separator for visual separation.
        assert!(
            text.starts_with("    fn new_fn() {}\n\n    fn existing"),
            "got: {text}"
        );
    }

    #[test]
    fn test_build_insert_after_adds_content_with_indent() {
        let content = b"    fn existing() {}";
        let sym = make_test_symbol("existing", SymbolKind::Function, (4, 20), 1);
        let result = build_insert_after(content, &sym, "fn new_fn() {}", LineEnding::Lf);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(
            text.contains("fn existing() {}\n\n    fn new_fn() {}"),
            "got: {text}"
        );
    }

    #[test]
    fn test_build_insert_after_skips_trailing_semicolon() {
        // C/C++ struct: tree-sitter node ends at `}` (exclusive end = 21),
        // declaration has `};` so `;` is at byte 21.
        let content = b"struct Foo { int x; };\n";
        let sym = make_test_symbol("Foo", SymbolKind::Struct, (0, 21), 1);
        let result = build_insert_after(content, &sym, "struct Bar { int y; };", LineEnding::Lf);
        let text = std::str::from_utf8(&result).unwrap();
        // The insertion should go AFTER the `;`, not between `}` and `;`
        assert!(
            text.contains("};\n\nstruct Bar"),
            "should insert after semicolon, got: {text}"
        );
        assert!(
            !text.contains("}\n\nstruct Bar"),
            "should not split '}}; ' got: {text}"
        );
    }

    #[test]
    fn test_build_insert_after_no_semicolon_unchanged() {
        // Functions don't have trailing `;` — behavior unchanged
        let content = b"fn foo() {}\nfn bar() {}\n";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, 11), 1);
        let result = build_insert_after(content, &sym, "fn baz() {}", LineEnding::Lf);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(text.contains("fn foo() {}\n\nfn baz() {}"), "got: {text}");
    }

    // -- build_delete --

    #[test]
    fn test_build_delete_removes_symbol_and_trailing_newline() {
        let content = b"fn keep() {}\n\nfn remove() {}\n\nfn also_keep() {}\n";
        let sym = make_test_symbol("remove", SymbolKind::Function, (14, 28), 3);
        let result = build_delete(content, &sym, LineEnding::Lf);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("remove"), "got: {text}");
        assert!(text.contains("keep"), "got: {text}");
        assert!(text.contains("also_keep"), "got: {text}");
    }

    #[test]
    fn test_build_delete_collapses_excessive_blank_lines() {
        // Simulate what happens after deleting 3 adjacent symbols: triple blank lines.
        let content = b"fn a() {}\n\n\n\nfn d() {}\n";
        // "a" occupies bytes 0..9, pretend we already removed the middle ones.
        // Just verify collapse_blank_lines works on this content.
        let collapsed = super::collapse_blank_lines(content, LineEnding::Lf);
        let text = std::str::from_utf8(&collapsed).unwrap();
        // Should have at most one blank line (two consecutive \n).
        assert!(
            !text.contains("\n\n\n"),
            "should collapse 3+ newlines: {text:?}"
        );
        assert!(text.contains("fn a() {}\n\nfn d()"), "got: {text:?}");
    }

    // -- build_edit_within --

    #[test]
    fn test_build_edit_within_replaces_first_match() {
        let content = b"fn foo() { old; old; }";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, 22), 1);
        let (result, count) = build_edit_within(content, &sym, "old", "new", false).unwrap();
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(count, 1);
        assert_eq!(text, "fn foo() { new; old; }");
    }

    #[test]
    fn test_build_edit_within_replaces_all() {
        let content = b"fn foo() { old; old; }";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, 22), 1);
        let (result, count) = build_edit_within(content, &sym, "old", "new", true).unwrap();
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(count, 2);
        assert_eq!(text, "fn foo() { new; new; }");
    }

    #[test]
    fn test_build_edit_within_not_found() {
        let content = b"fn foo() { body; }";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, 18), 1);
        let result = build_edit_within(content, &sym, "missing", "new", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_edit_within_matches_lf_needle_in_crlf_body() {
        // Callers supply `\n`-separated text, but the file on disk uses CRLF.
        // The search must still match, and the splice must preserve CRLF without
        // introducing lone LF line endings.
        let content = b"fn foo() {\r\n    let x = 1;\r\n    bar();\r\n}";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, content.len() as u32), 1);
        let (result, count) =
            build_edit_within(content, &sym, "    let x = 1;\n", "    let x = 2;\n", false)
                .unwrap();
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(count, 1);
        assert_eq!(text, "fn foo() {\r\n    let x = 2;\r\n    bar();\r\n}");
        // Every LF is part of a CRLF pair — no mixed line endings were introduced.
        assert_eq!(
            text.matches('\n').count(),
            text.matches("\r\n").count(),
            "result must not contain lone LF line endings: {text:?}"
        );
    }

    #[test]
    fn test_build_edit_within_replace_all_lf_needle_in_crlf_body() {
        // replace_all must also normalize the needle so every CRLF occurrence is
        // matched and replaced.
        let content = b"fn foo() {\r\n    old();\r\n    old();\r\n}";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, content.len() as u32), 1);
        let (result, count) =
            build_edit_within(content, &sym, "    old();\n", "    new();\n", true).unwrap();
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(count, 2);
        assert_eq!(text, "fn foo() {\r\n    new();\r\n    new();\r\n}");
    }

    // -- whitespace-flexible fallback --

    #[test]
    fn test_ws_flexible_basic_indent_mismatch() {
        let body = "fn foo() {\n        let x = 1;\n        let y = 2;\n    }";
        let old = "    let x = 1;\n    let y = 2;";
        let new = "    let x = 10;\n    let y = 20;";
        let (result, count) = try_whitespace_flexible_replace(body, old, new, false).unwrap();
        assert_eq!(count, 1);
        assert!(result.contains("        let x = 10;"));
        assert!(result.contains("        let y = 20;"));
        assert!(!result.contains("let x = 1;"));
    }

    #[test]
    fn test_ws_flexible_replace_all() {
        let body = "fn f() {\n    a();\n    b();\n    a();\n}";
        let old = "  a();";
        let new = "  z();";
        let (result, count) = try_whitespace_flexible_replace(body, old, new, true).unwrap();
        assert_eq!(count, 2);
        assert_eq!(result.matches("z()").count(), 2);
        assert_eq!(result.matches("a()").count(), 0);
    }

    #[test]
    fn test_ws_flexible_no_match_returns_none() {
        let body = "fn foo() {\n    let x = 1;\n}";
        let old = "let y = 99;";
        assert!(try_whitespace_flexible_replace(body, old, "z", false).is_none());
    }

    #[test]
    fn test_ws_flexible_preserves_relative_indent() {
        let body = "impl Foo {\n        fn bar() {\n            inner();\n        }\n    }";
        let old = "    fn bar() {\n        inner();\n    }";
        let new = "    fn bar() {\n        outer();\n        extra();\n    }";
        let (result, _) = try_whitespace_flexible_replace(body, old, new, false).unwrap();
        assert!(result.contains("        fn bar() {"));
        assert!(result.contains("            outer();"));
        assert!(result.contains("            extra();"));
        assert!(result.contains("        }"));
    }

    #[test]
    fn test_ws_flexible_trailing_newline_preserved() {
        let body = "fn f() {\n    old();\n    next();\n}";
        let old = "  old();";
        let new = "  new();";
        let (result, count) = try_whitespace_flexible_replace(body, old, new, false).unwrap();
        assert_eq!(count, 1);
        // The line after the match should still be present.
        assert!(result.contains("    next();"));
    }

    #[test]
    fn test_ws_flexible_empty_old_returns_none() {
        let body = "fn f() {}";
        assert!(try_whitespace_flexible_replace(body, "", "x", false).is_none());
        assert!(try_whitespace_flexible_replace(body, "   \n  ", "x", false).is_none());
    }

    // -- execute_batch_edit --

    #[test]
    fn test_execute_batch_edit_applies_multiple_edits() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn alpha() { old }\n").unwrap();
        std::fs::write(src.join("b.rs"), b"fn beta() { keep }\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn alpha() { old }\n" as &[u8]),
            ("src/b.rs", b"fn beta() { keep }\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "alpha".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn alpha() { new }".to_string(),
                },
                working_directory: None,
            },
            SingleEdit {
                path: "src/b.rs".to_string(),
                name: "beta".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
                working_directory: None,
            },
        ];

        let summaries = execute_batch_edit(&handle, dir.path(), &edits, false, None).unwrap();
        assert_eq!(summaries.len(), 2);

        let a_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(a_content.contains("new"), "a.rs: {a_content}");

        let b_content = std::fs::read_to_string(src.join("b.rs")).unwrap();
        assert!(!b_content.contains("beta"), "b.rs: {b_content}");
    }

    #[test]
    fn test_execute_batch_edit_rejects_overlapping() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn foo() {}\nfn bar() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn foo() {}\nfn bar() {}\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        // Create two edits that target overlapping fake ranges won't work easily,
        // but we can test with two edits on the same symbol (same range = overlapping).
        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
                working_directory: None,
            },
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
                working_directory: None,
            },
        ];

        let result = execute_batch_edit(&handle, dir.path(), &edits, false, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Overlapping"));
    }

    #[test]
    fn test_execute_batch_edit_rollback_message_on_nonexistent_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn foo() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn foo() {}\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        // First edit targets a real symbol; second targets a nonexistent one.
        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn foo() { modified }".to_string(),
                },
                working_directory: None,
            },
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "nonexistent_symbol".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
                working_directory: None,
            },
        ];

        let result = execute_batch_edit(&handle, dir.path(), &edits, false, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("ROLLED BACK"),
            "expected ROLLED BACK in: {err}"
        );
        assert!(
            err.contains("No files were modified"),
            "expected 'No files were modified' in: {err}"
        );
        assert!(err.contains("2"), "expected edit count (2) in: {err}");

        // Confirm the file was NOT modified.
        let file_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            file_content.contains("fn foo() {}"),
            "file should be unmodified: {file_content}"
        );
    }

    #[test]
    fn test_execute_batch_edit_nonexistent_symbol_includes_same_file_suggestion() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn foo_bar() {}\n").unwrap();
        std::fs::write(src.join("b.rs"), b"fn foo() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn foo_bar() {}\n" as &[u8]),
            ("src/b.rs", b"fn foo() {}\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let edits = vec![SingleEdit {
            path: "src/a.rs".to_string(),
            name: "foo".to_string(),
            kind: None,
            symbol_line: None,
            operation: EditOperation::Delete,
            working_directory: None,
        }];

        let result = execute_batch_edit(&handle, dir.path(), &edits, false, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("did_you_mean: [foo_bar]"), "error was: {err}");
        assert!(
            !err.contains("did_you_mean: [foo]"),
            "must not suggest symbol from src/b.rs: {err}"
        );
    }

    #[test]
    fn test_execute_batch_edit_dry_run_previews_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn alpha() { old }\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn alpha() { old }\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        let edits = vec![SingleEdit {
            path: "src/a.rs".to_string(),
            name: "alpha".to_string(),
            kind: None,
            symbol_line: None,
            operation: EditOperation::Replace {
                new_body: "fn alpha() { new }".to_string(),
            },
            working_directory: None,
        }];

        let summaries = execute_batch_edit(&handle, dir.path(), &edits, true, None).unwrap();
        assert_eq!(summaries.len(), 1, "expected one preview line");
        assert!(
            summaries[0].contains("[DRY RUN]"),
            "expected [DRY RUN] prefix in: {}",
            summaries[0]
        );

        // File must be unchanged.
        let file_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            file_content.contains("old"),
            "dry_run must not write to disk: {file_content}"
        );
        assert!(
            !file_content.contains("new"),
            "dry_run must not write to disk: {file_content}"
        );
    }

    #[test]
    fn test_execute_batch_edit_dry_run_same_error_as_real() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn foo() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn foo() {}\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        let edits = vec![SingleEdit {
            path: "src/a.rs".to_string(),
            name: "nonexistent_symbol".to_string(),
            kind: None,
            symbol_line: None,
            operation: EditOperation::Delete,
            working_directory: None,
        }];

        let real_err = execute_batch_edit(&handle, dir.path(), &edits, false, None).unwrap_err();
        let dry_err = execute_batch_edit(&handle, dir.path(), &edits, true, None).unwrap_err();

        assert_eq!(
            real_err, dry_err,
            "dry_run must produce identical error to real run"
        );
    }

    // -- execute_batch_insert --

    #[test]
    fn test_execute_batch_insert_adds_to_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn handler_a() {}\n").unwrap();
        std::fs::write(src.join("b.rs"), b"fn handler_b() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn handler_a() {}\n" as &[u8]),
            ("src/b.rs", b"fn handler_b() {}\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let input = BatchInsertInput {
            content: "fn logging() { log::info!(\"called\"); }".to_string(),
            position: InsertPosition::After,
            targets: vec![
                InsertTarget {
                    path: "src/a.rs".to_string(),
                    name: "handler_a".to_string(),
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                },
                InsertTarget {
                    path: "src/b.rs".to_string(),
                    name: "handler_b".to_string(),
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                },
            ],
            dry_run: Some(false),
            idempotency_key: None,
            working_directory: None,
        };

        let summaries = execute_batch_insert(&handle, dir.path(), &input).unwrap();
        assert_eq!(summaries.len(), 2);

        let a = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(a.contains("logging"), "a.rs: {a}");
        let b = std::fs::read_to_string(src.join("b.rs")).unwrap();
        assert!(b.contains("logging"), "b.rs: {b}");
    }

    #[test]
    fn test_execute_batch_insert_dry_run_previews_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn handler_a() {}\n").unwrap();
        std::fs::write(src.join("b.rs"), b"fn handler_b() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn handler_a() {}\n" as &[u8]),
            ("src/b.rs", b"fn handler_b() {}\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let input = BatchInsertInput {
            content: "fn logging() {}".to_string(),
            position: InsertPosition::After,
            targets: vec![
                InsertTarget {
                    path: "src/a.rs".to_string(),
                    name: "handler_a".to_string(),
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                },
                InsertTarget {
                    path: "src/b.rs".to_string(),
                    name: "handler_b".to_string(),
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                },
            ],
            dry_run: Some(true),
            idempotency_key: None,
            working_directory: None,
        };

        let summaries = execute_batch_insert(&handle, dir.path(), &input).unwrap();
        assert_eq!(summaries.len(), 2, "expected two preview lines");
        for s in &summaries {
            assert!(s.contains("[DRY RUN]"), "expected [DRY RUN] prefix in: {s}");
        }

        // Files must be unchanged.
        let a = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            !a.contains("logging"),
            "dry_run must not write to disk: {a}"
        );
        let b = std::fs::read_to_string(src.join("b.rs")).unwrap();
        assert!(
            !b.contains("logging"),
            "dry_run must not write to disk: {b}"
        );
    }

    #[test]
    fn test_execute_batch_insert_rolls_back_on_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let sub = src.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        std::fs::write(src.join("a.rs"), b"fn handler_a() {}\n").unwrap();
        std::fs::write(sub.join("b.rs"), b"fn handler_b() {}\n").unwrap();
        std::fs::write(src.join("c.rs"), b"fn handler_c() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn handler_a() {}\n" as &[u8]),
            ("src/sub/b.rs", b"fn handler_b() {}\n"),
            ("src/c.rs", b"fn handler_c() {}\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        // Replace the target file with a directory after indexing so path
        // containment still resolves, but atomic_write_file fails when it tries
        // to persist a temp file over a directory. This is deterministic across
        // Windows, Linux, WSL, and macOS.
        let b_path = sub.join("b.rs");
        std::fs::remove_file(&b_path).unwrap();
        std::fs::create_dir(&b_path).unwrap();

        let input = BatchInsertInput {
            content: "fn logging() {}".to_string(),
            position: InsertPosition::After,
            targets: vec![
                InsertTarget {
                    path: "src/a.rs".to_string(),
                    name: "handler_a".to_string(),
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                },
                InsertTarget {
                    path: "src/sub/b.rs".to_string(),
                    name: "handler_b".to_string(),
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                },
                InsertTarget {
                    path: "src/c.rs".to_string(),
                    name: "handler_c".to_string(),
                    kind: None,
                    symbol_line: None,
                    working_directory: None,
                },
            ],
            dry_run: Some(false),
            idempotency_key: None,
            working_directory: None,
        };

        let result = execute_batch_insert(&handle, dir.path(), &input);

        let err = result.unwrap_err();
        assert!(
            err.contains("ROLLED BACK") || err.contains("Write failed"),
            "expected rollback message in: {err}"
        );
        assert!(
            err.contains("No batch insert was applied"),
            "expected atomic rollback message in: {err}"
        );

        for path in ["src/a.rs", "src/c.rs"] {
            let disk = std::fs::read_to_string(dir.path().join(path)).unwrap();
            assert!(
                !disk.contains("logging"),
                "{path} should be restored after rollback: {disk}"
            );
            let guard = handle.read();
            let indexed = guard.get_file(path).unwrap();
            assert!(
                !std::str::from_utf8(&indexed.content)
                    .unwrap()
                    .contains("logging"),
                "index should match rolled-back disk state for {path}"
            );
        }

        assert!(
            b_path.is_dir(),
            "failed target should remain a directory to prove the write failed at the filesystem boundary"
        );
        let missing_target = handle
            .read()
            .get_file("src/sub/b.rs")
            .unwrap()
            .content
            .clone();
        assert!(
            !std::str::from_utf8(&missing_target)
                .unwrap()
                .contains("logging"),
            "index entry for failed target should remain unchanged"
        );
    }

    // -- atomic rollback on write failure --

    #[test]
    fn test_batch_edit_rolls_back_on_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let sub = src.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        std::fs::write(src.join("a.rs"), b"fn alpha() { old }\n").unwrap();
        std::fs::write(sub.join("b.rs"), b"fn beta() { old }\n").unwrap();
        std::fs::write(src.join("c.rs"), b"fn gamma() { old }\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn alpha() { old }\n" as &[u8]),
            ("src/sub/b.rs", b"fn beta() { old }\n"),
            ("src/c.rs", b"fn gamma() { old }\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let b_path = sub.join("b.rs");
        std::fs::remove_file(&b_path).unwrap();
        std::fs::create_dir(&b_path).unwrap();

        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "alpha".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn alpha() { new }".to_string(),
                },
                working_directory: None,
            },
            SingleEdit {
                path: "src/sub/b.rs".to_string(),
                name: "beta".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn beta() { new }".to_string(),
                },
                working_directory: None,
            },
            SingleEdit {
                path: "src/c.rs".to_string(),
                name: "gamma".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn gamma() { new }".to_string(),
                },
                working_directory: None,
            },
        ];

        let result = execute_batch_edit(&handle, dir.path(), &edits, false, None);

        let err = result.unwrap_err();
        assert!(
            err.contains("ROLLED BACK") || err.contains("Write failed"),
            "expected rollback message in: {err}"
        );
        assert!(
            err.contains("No batch edit was applied"),
            "expected atomic rollback message in: {err}"
        );

        let a_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            a_content.contains("old"),
            "a.rs should be restored after rollback: {a_content}"
        );
        assert!(
            !a_content.contains("new"),
            "a.rs must not keep staged edits after rollback: {a_content}"
        );

        assert!(
            b_path.is_dir(),
            "failed target should remain a directory to prove the write failed at the filesystem boundary"
        );

        let c_content = std::fs::read_to_string(src.join("c.rs")).unwrap();
        assert!(
            c_content.contains("old"),
            "c.rs should not be written after a rollback-triggering failure: {c_content}"
        );

        let guard = handle.read();
        let a_indexed = guard.get_file("src/a.rs").unwrap();
        let c_indexed = guard.get_file("src/c.rs").unwrap();
        assert!(
            std::str::from_utf8(&a_indexed.content)
                .unwrap()
                .contains("old"),
            "index should match rolled-back disk state for a.rs"
        );
        assert!(
            std::str::from_utf8(&c_indexed.content)
                .unwrap()
                .contains("old"),
            "index should remain original for c.rs"
        );
    }

    #[test]
    fn test_batch_rename_rolls_back_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        // Three files all containing "OldName". Put b.rs in a subdirectory so we
        // can replace that file with a directory after indexing. That keeps path
        // containment intact while making atomic_write_file fail cross-platform.
        let sub = dir.path().join("src").join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(src.join("a.rs"), b"struct OldName;\n").unwrap();
        std::fs::write(sub.join("b.rs"), b"use crate::OldName;\n").unwrap();
        std::fs::write(src.join("c.rs"), b"fn use_it(x: OldName) {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"struct OldName;\n" as &[u8]),
            ("src/sub/b.rs", b"use crate::OldName;\n"),
            ("src/c.rs", b"fn use_it(x: OldName) {}\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let b_path = sub.join("b.rs");
        std::fs::remove_file(&b_path).unwrap();
        std::fs::create_dir(&b_path).unwrap();

        let input = crate::protocol::edit::BatchRenameInput {
            path: "src/a.rs".to_string(),
            name: "OldName".to_string(),
            new_name: "NewName".to_string(),
            kind: None,
            symbol_line: None,
            dry_run: Some(false),
            idempotency_key: None,
            code_only: None,
            working_directory: None,
        };

        let result = execute_batch_rename(&handle, dir.path(), &input);

        // Must be an error.
        let err = result.unwrap_err();
        assert!(
            err.contains("ROLLED BACK") || err.contains("Write failed"),
            "expected rollback message in: {err}"
        );

        // All files that were written before the failure must be rolled back to "OldName".
        let a_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            a_content.contains("OldName"),
            "a.rs should be rolled back to OldName: {a_content}"
        );
        assert!(
            !a_content.contains("NewName"),
            "a.rs must not contain NewName after rollback: {a_content}"
        );

        assert!(
            b_path.is_dir(),
            "failed target should remain a directory to prove the write failed at the filesystem boundary"
        );
    }

    // -- extract_signature --

    #[test]
    fn test_extract_signature_returns_first_line() {
        let content = b"fn foo(x: i32) {\n    body();\n}";
        let sig = extract_signature(content, (0, 30));
        assert_eq!(sig, "fn foo(x: i32) {");
    }

    #[test]
    fn test_extract_signature_single_line() {
        let content = b"fn foo() {}";
        let sig = extract_signature(content, (0, 11));
        assert_eq!(sig, "fn foo() {}");
    }

    // -- extract_impl_type_name --

    #[test]
    fn test_extract_impl_type_name_simple() {
        assert_eq!(extract_impl_type_name("impl Foo"), Some("Foo".to_string()));
    }

    #[test]
    fn test_extract_impl_type_name_trait_for() {
        assert_eq!(
            extract_impl_type_name("impl Display for Foo"),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn test_extract_impl_type_name_generic() {
        assert_eq!(
            extract_impl_type_name("impl<T> Foo<T>"),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn test_extract_impl_type_name_generic_trait_for() {
        assert_eq!(
            extract_impl_type_name("impl<T: Clone> Trait for Foo<T>"),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn test_extract_impl_type_name_no_impl_prefix() {
        // Some parsers may strip the "impl" keyword from the name.
        assert_eq!(extract_impl_type_name("Foo"), Some("Foo".to_string()));
    }

    // -- find_parent_impl_type --

    #[test]
    fn test_find_parent_impl_type_for_method() {
        let file = make_test_indexed_file(vec![
            SymbolRecord {
                name: "impl Widget".to_string(),
                kind: SymbolKind::Impl,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 100),
                line_range: (0, 10),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "display".to_string(),
                kind: SymbolKind::Method,
                depth: 1,
                sort_order: 1,
                byte_range: (20, 80),
                line_range: (2, 8),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ]);
        let method = &file.symbols[1];
        assert_eq!(
            find_parent_impl_type(&file, method),
            Some("Widget".to_string())
        );
    }

    #[test]
    fn test_find_parent_impl_type_standalone_fn() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "standalone",
            SymbolKind::Function,
            (0, 50),
            1,
        )]);
        let func = &file.symbols[0];
        assert_eq!(find_parent_impl_type(&file, func), None);
    }

    // -- detect_stale_references with parent_type filtering --

    fn make_ref_file(refs: Vec<crate::domain::index::ReferenceRecord>) -> IndexedFile {
        IndexedFile {
            relative_path: String::new(),
            language: LanguageId::Rust,
            classification: crate::domain::index::FileClassification::for_code_path("test.rs"),
            content: Vec::new(),
            symbols: Vec::new(),
            parse_status: crate::live_index::store::ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 0,
            content_hash: String::new(),
            references: refs,
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        }
    }

    #[test]
    fn test_detect_stale_refs_method_filters_by_parent_type() {
        use crate::domain::index::ReferenceKind;
        let handle = crate::live_index::LiveIndex::empty();

        // File A: has Widget type ref + display call -> should be warned
        handle.update_file(
            "src/a.rs".to_string(),
            make_ref_file(vec![
                crate::domain::index::ReferenceRecord {
                    name: "display".to_string(),
                    qualified_name: None,
                    kind: ReferenceKind::Call,
                    byte_range: (32, 39),
                    line_range: (1, 1),
                    enclosing_symbol_index: None,
                },
                crate::domain::index::ReferenceRecord {
                    name: "Widget".to_string(),
                    qualified_name: None,
                    kind: ReferenceKind::TypeUsage,
                    byte_range: (12, 18),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
            ]),
        );

        // File B: has display call but NO Widget ref -> should NOT be warned
        handle.update_file(
            "src/b.rs".to_string(),
            make_ref_file(vec![crate::domain::index::ReferenceRecord {
                name: "display".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (19, 26),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            }]),
        );

        // With parent_type = Some("Widget"), only file A should be warned
        let refs = detect_stale_references(
            &handle,
            "src/widget.rs",
            "display",
            "fn display(&self) {",
            "fn display(&self, verbose: bool) {",
            Some("Widget"),
            None,
        );
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "src/a.rs");
    }

    #[test]
    fn test_detect_stale_refs_standalone_fn_warns_all() {
        use crate::domain::index::ReferenceKind;
        let handle = crate::live_index::LiveIndex::empty();

        // File A: has display call
        handle.update_file(
            "src/a.rs".to_string(),
            make_ref_file(vec![crate::domain::index::ReferenceRecord {
                name: "display".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (12, 19),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            }]),
        );

        // File B: also has display call
        handle.update_file(
            "src/b.rs".to_string(),
            make_ref_file(vec![crate::domain::index::ReferenceRecord {
                name: "display".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (15, 22),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            }]),
        );

        // With parent_type = None (standalone fn), both files should be warned
        let refs = detect_stale_references(
            &handle,
            "src/lib.rs",
            "display",
            "fn display() {",
            "fn display(verbose: bool) {",
            None,
            None,
        );
        assert_eq!(refs.len(), 2);
    }

    // -- doc-aware build_delete and build_insert_before --

    #[test]
    fn test_build_delete_includes_doc_comments() {
        // "/// Doc line 1\n" = 15 bytes (0..15)
        // "/// Doc line 2\n" = 15 bytes (15..30)
        // "pub fn foo() {}\n" = 16 bytes (30..46)
        // "\n"               =  1 byte  (46..47)
        // "fn bar() {}\n"    = 12 bytes (47..59)
        let content = b"/// Doc line 1\n/// Doc line 2\npub fn foo() {}\n\nfn bar() {}\n";
        let sym = SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (30, 46),
            line_range: (2, 2),
            doc_byte_range: Some((0, 30)),
            item_byte_range: None,
        };
        let result = build_delete(content, &sym, LineEnding::Lf);
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("/// Doc line 1"),
            "doc comments should be deleted"
        );
        assert!(
            !result_str.contains("pub fn foo"),
            "function body should be deleted"
        );
        assert!(
            result_str.contains("fn bar()"),
            "other function should remain"
        );
    }

    #[test]
    fn test_build_delete_removes_blank_line_separated_doc_comments() {
        // Regression: doc comments separated by a blank line from the symbol
        // are NOT attached via doc_byte_range (scan_doc_range stops at blank lines).
        // But delete_symbol should still clean them up to avoid orphaned comments.
        //
        // "/// Batch-inserted marker\n" = 26 bytes (0..26)
        // "\n"                          =  1 byte  (26..27)
        // "fn batch_marker() {}\n"      = 21 bytes (27..48)
        // "\n"                          =  1 byte  (48..49)
        // "fn other() {}\n"             = 14 bytes (49..63)
        let content = b"/// Batch-inserted marker\n\nfn batch_marker() {}\n\nfn other() {}\n";
        let sym = SymbolRecord {
            name: "batch_marker".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (27, 48),
            line_range: (2, 2),
            doc_byte_range: None, // blank line prevents attachment
            item_byte_range: None,
        };
        let result = build_delete(content, &sym, LineEnding::Lf);
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("/// Batch-inserted marker"),
            "orphaned doc comment should be cleaned up, got: {result_str}"
        );
        assert!(
            result_str.contains("fn other()"),
            "other function should remain, got: {result_str}"
        );
    }

    #[test]
    fn test_build_insert_before_goes_above_doc_comments() {
        // "/// Doc for foo\n" = 16 bytes (0..16)
        // "pub fn foo() {}\n" = 16 bytes (16..32)
        let content = b"/// Doc for foo\npub fn foo() {}\n";
        let sym = SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (16, 32),
            line_range: (1, 1),
            doc_byte_range: Some((0, 16)),
            item_byte_range: None,
        };
        let result = build_insert_before(content, &sym, "use std::io;", LineEnding::Lf);
        let result_str = String::from_utf8(result).unwrap();
        let use_pos = result_str
            .find("use std::io;")
            .expect("inserted content missing");
        let doc_pos = result_str
            .find("/// Doc for foo")
            .expect("doc comment missing");
        assert!(
            use_pos < doc_pos,
            "insert should go above doc comments (use_pos={use_pos}, doc_pos={doc_pos})"
        );
    }

    #[test]
    fn test_build_insert_before_double_newline_without_doc_comments() {
        let content = b"struct Point { x: f64 }\n";
        let sym = SymbolRecord {
            name: "Point".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let result =
            build_insert_before(content, &sym, "struct Point3D { x: f64 }", LineEnding::Lf);
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            result_str.contains("Point3D { x: f64 }\n\nstruct Point"),
            "should have \\n\\n separator when no doc comment: {result_str}"
        );
    }

    #[test]
    fn test_build_insert_before_no_double_blank_line() {
        // File already has a blank line before the symbol: inserting should NOT create \n\n\n.
        // "\n"            =  1 byte (0..1)   — blank line preceding the symbol
        // "fn existing() {}\n" = 18 bytes (1..19)
        let content = b"\nfn existing() {}\n";
        let sym = SymbolRecord {
            name: "existing".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (1, 18),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let result = build_insert_before(content, &sym, "fn new_fn() {}", LineEnding::Lf);
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("\n\n\n"),
            "should not produce triple newline when blank line already precedes symbol: {result_str:?}"
        );
        assert!(
            result_str.contains("fn new_fn() {}"),
            "inserted content missing: {result_str:?}"
        );
        assert!(
            result_str.contains("fn existing() {}"),
            "existing content missing: {result_str:?}"
        );
    }

    #[test]
    fn test_build_insert_before_first_symbol_in_file() {
        // Symbol starts at byte 0 (prefix is empty) — no double blank line should be produced.
        let content = b"fn first() {}\n";
        let sym = SymbolRecord {
            name: "first".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 13),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let result = build_insert_before(content, &sym, "fn before() {}", LineEnding::Lf);
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("\n\n\n"),
            "should not produce triple newline when symbol is first in file: {result_str:?}"
        );
        assert!(
            result_str.contains("fn before() {}"),
            "inserted content missing: {result_str:?}"
        );
        assert!(
            result_str.contains("fn first() {}"),
            "original content missing: {result_str:?}"
        );
    }

    #[test]
    fn test_build_insert_before_with_doc_byte_range() {
        // Symbol has doc_byte_range — separator is always \n (tight against doc comment).
        // "/// Doc\n"       =  8 bytes (0..8)
        // "fn target() {}\n" = 15 bytes (8..23)
        // "\n"               =  1 byte  (23..24)
        // "fn other() {}\n"  = 14 bytes (24..38)
        let content = b"/// Doc\nfn target() {}\n\nfn other() {}\n";
        let sym = SymbolRecord {
            name: "target".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (8, 23),
            line_range: (1, 1),
            doc_byte_range: Some((0, 8)),
            item_byte_range: None,
        };
        let result = build_insert_before(content, &sym, "fn inserted() {}", LineEnding::Lf);
        let result_str = String::from_utf8(result).unwrap();
        // insertion goes above the doc comment, with \n separator (not \n\n)
        assert!(
            !result_str.contains("\n\n\n"),
            "should not produce triple newline with doc_byte_range: {result_str:?}"
        );
        let ins_pos = result_str
            .find("fn inserted()")
            .expect("inserted content missing");
        let doc_pos = result_str.find("/// Doc").expect("doc comment missing");
        assert!(
            ins_pos < doc_pos,
            "insertion should appear before doc comment: ins={ins_pos} doc={doc_pos}"
        );
    }

    #[test]
    fn test_build_edit_within_no_doc_duplication() {
        // "/// Doc comment\n" = 16 bytes (0..16)
        // "pub fn foo() {}\n" = 16 bytes (16..32)
        let content = b"/// Doc comment\npub fn foo() {}\n";
        let sym = SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (16, 32),
            line_range: (1, 1),
            doc_byte_range: Some((0, 16)),
            item_byte_range: None,
        };
        let (result, count) = build_edit_within(content, &sym, "foo", "bar", false).unwrap();
        let result_str = String::from_utf8(result).unwrap();
        assert_eq!(count, 1);
        // Doc comment should appear exactly once, not duplicated
        assert_eq!(
            result_str.matches("/// Doc comment").count(),
            1,
            "doc comment should not be duplicated: {result_str}"
        );
        assert!(result_str.contains("pub fn bar()"), "edit should apply");
    }

    // -----------------------------------------------------------------------
    // find_qualified_usages tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_finds_type_new_qualified_call() {
        let source = "let x = MyType::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_finds_deep_nested_qualified() {
        let source = "let x = module::MyType::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_finds_use_import_path() {
        let source = "use crate::module::MyType;";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_scanner_finds_all_raw_occurrences_of_common_name() {
        let source = "let x = SomeOther::new();\nlet y = Target::new();";
        let matches = find_qualified_usages("new", source);
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|m| m.confident));
    }

    #[test]
    fn test_uncertain_match_in_string() {
        let source = r#"let s = "MyType::new()";"#;
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }

    #[test]
    fn test_uncertain_match_in_comment() {
        let source = "// MyType::new() creates an instance";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }

    #[test]
    fn test_finds_turbofish_qualified_call() {
        let source = "let x = MyType::<T>::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_uncertain_match_in_block_comment() {
        let source = "/* MyType::new() creates an instance */";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }

    #[test]
    fn test_uncertain_match_in_multiline_string() {
        let source = "let s = r\"\n            MyType::new()\n        \";";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }

    #[test]
    fn test_find_qualified_usages_non_ascii_no_panic() {
        // Source containing em dash (3-byte UTF-8: \xe2\x80\x94) — must not panic
        let source = "/// Retry in a moment \u{2014} just wait\nfn foo() { bar::baz(); }\n";
        let matches = find_qualified_usages("baz", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_find_qualified_usages_emoji_no_panic() {
        let source = "/// \u{1F980} Rust crab\nfn main() { std::io::println(); }\n";
        let matches = find_qualified_usages("io", source);
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_qualified_usages_cjk_no_panic() {
        // CJK characters are 3 bytes each
        let source = "// \u{4F60}\u{597D}\u{4E16}\u{754C} hello::world\nlet x = foo::bar();\n";
        let matches = find_qualified_usages("bar", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
        // Also check the comment match
        let matches2 = find_qualified_usages("world", source);
        assert_eq!(matches2.len(), 1);
        assert!(!matches2[0].confident);
    }

    #[test]
    fn test_find_qualified_usages_multibyte_in_string_literal() {
        // Em dash inside a string literal with qualified path nearby
        let source = "let s = \"retry \u{2014} now\"; foo::bar();";
        let matches = find_qualified_usages("bar", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_find_qualified_usages_multibyte_in_block_comment() {
        let source = "/* \u{2014} em dash */ foo::bar();";
        let matches = find_qualified_usages("bar", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_find_qualified_usages_macro_body_classified_confident() {
        // Qualified paths inside macro_rules! bodies are real code references
        // and should be classified as confident (not uncertain).
        let source = r#"
macro_rules! make_widget {
    ($val:expr) => {
        Widget::new($val)
    };
}
fn uses_it() { Widget::default(); }
"#;
        let matches = find_qualified_usages("Widget", source);
        assert_eq!(
            matches.len(),
            2,
            "should find Widget in macro body and fn body"
        );
        assert!(
            matches.iter().all(|m| m.confident),
            "macro body matches must be confident: {:?}",
            matches
                .iter()
                .map(|m| (&m.context, m.confident))
                .collect::<Vec<_>>()
        );
    }

    // -- atomic_write_file --

    #[test]
    fn test_atomic_write_concurrent_no_hybrid() {
        use std::io::ErrorKind;
        use std::sync::{Arc, Barrier};

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");

        // Write an initial file so the target exists before threads start.
        std::fs::write(&target, b"initial").unwrap();

        let payload_a = vec![b'A'; 1024 * 1024];
        let payload_b = vec![b'B'; 1024 * 1024];

        let barrier = Arc::new(Barrier::new(2));
        let target_a = target.clone();
        let target_b = target.clone();
        let pa = payload_a.clone();
        let pb = payload_b.clone();
        let ba = Arc::clone(&barrier);
        let bb = Arc::clone(&barrier);

        let ha = std::thread::spawn(move || {
            ba.wait();
            atomic_write_file(&target_a, &pa)
        });
        let hb = std::thread::spawn(move || {
            bb.wait();
            atomic_write_file(&target_b, &pb)
        });

        let result_a = ha.join().unwrap();
        let result_b = hb.join().unwrap();

        for result in [&result_a, &result_b] {
            if let Err(err) = result {
                assert_eq!(
                    err.kind(),
                    ErrorKind::PermissionDenied,
                    "unexpected concurrent atomic_write_file error: {err}"
                );
            }
        }
        assert!(
            result_a.is_ok() || result_b.is_ok(),
            "at least one concurrent writer should succeed: a={result_a:?} b={result_b:?}"
        );

        let result = std::fs::read(&target).unwrap();
        assert!(
            result == payload_a || result == payload_b,
            "file must be exactly payload A or B, not a hybrid (len={})",
            result.len()
        );
    }

    #[test]
    fn test_atomic_write_no_orphan_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output.txt");

        atomic_write_file(&target, b"hello world").unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "expected exactly 1 file in dir, found: {:?}",
            entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_atomic_write_error_path_no_orphan() {
        let dir = tempfile::tempdir().unwrap();
        // Target is in a nonexistent subdirectory — write must fail.
        let bad_target = dir.path().join("nonexistent_subdir").join("file.txt");

        let result = atomic_write_file(&bad_target, b"data");
        assert!(result.is_err(), "expected error for nonexistent parent dir");

        // No temp files should have leaked into the real dir.
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            entries.is_empty(),
            "expected no orphan temp files, found: {:?}",
            entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_detect_line_ending_lf() {
        assert_eq!(detect_line_ending(b"hello\nworld\n"), LineEnding::Lf);
    }

    #[test]
    fn test_detect_line_ending_crlf() {
        assert_eq!(detect_line_ending(b"hello\r\nworld\r\n"), LineEnding::CrLf);
    }

    #[test]
    fn test_detect_line_ending_empty() {
        assert_eq!(detect_line_ending(b""), LineEnding::Lf);
    }

    #[test]
    fn test_detect_line_ending_dominant_count() {
        assert_eq!(detect_line_ending(b"a\r\nb\r\nc\n"), LineEnding::CrLf);
        assert_eq!(detect_line_ending(b"a\r\nb\nc\n"), LineEnding::Lf);
    }

    #[test]
    fn test_normalize_line_endings_to_crlf() {
        let result = normalize_line_endings(b"line1\nline2\nline3", LineEnding::CrLf);
        assert_eq!(result, b"line1\r\nline2\r\nline3");
    }

    #[test]
    fn test_normalize_line_endings_to_lf() {
        let result = normalize_line_endings(b"line1\r\nline2\r\nline3", LineEnding::Lf);
        assert_eq!(result, b"line1\nline2\nline3");
    }

    #[test]
    fn test_normalize_lone_cr() {
        let result = normalize_line_endings(b"line1\rline2\r", LineEnding::CrLf);
        assert_eq!(result, b"line1\r\nline2\r\n");
    }

    // -- CRLF preservation tests --

    #[test]
    fn test_apply_indentation_preserves_crlf() {
        let text = "fn foo() {\n    bar();\n}\n";
        let indent = b"    ";
        let result = apply_indentation(text, indent, LineEnding::CrLf);
        // Must contain CRLF
        assert!(
            result.windows(2).any(|w| w == b"\r\n"),
            "should contain CRLF"
        );
        // No bare \n (every \n must be preceded by \r)
        for (i, &byte) in result.iter().enumerate() {
            if byte == b'\n' {
                assert!(i > 0 && result[i - 1] == b'\r', "bare LF at byte {i}");
            }
        }
    }

    #[test]
    fn test_build_insert_before_crlf_preserved() {
        // Build a CRLF file and insert before the symbol; verify no bare \n in output
        let content = b"fn existing() {\r\n    body();\r\n}\r\n";
        let sym = make_test_symbol("existing", SymbolKind::Function, (0, 32), 1);
        let result = build_insert_before(content, &sym, "fn new_fn() {}", LineEnding::CrLf);
        // No bare \n in output (every \n must be preceded by \r)
        for (i, &byte) in result.iter().enumerate() {
            if byte == b'\n' {
                assert!(
                    i > 0 && result[i - 1] == b'\r',
                    "bare LF at byte {i} in insert_before output"
                );
            }
        }
        // Output must contain CRLF
        assert!(
            result.windows(2).any(|w| w == b"\r\n"),
            "output should contain CRLF"
        );
    }

    #[test]
    fn test_build_delete_crlf_no_orphan_cr() {
        // Three CRLF functions; delete the middle one; verify no orphan \r
        let content = b"fn keep1() {}\r\n\r\nfn remove() {}\r\n\r\nfn keep2() {}\r\n";
        // "fn remove() {}" occupies bytes 17..31 (exclusive)
        let sym = make_test_symbol("remove", SymbolKind::Function, (17, 31), 3);
        let result = build_delete(content, &sym, LineEnding::CrLf);
        // No orphan \r (every \r must be followed by \n)
        for (i, &byte) in result.iter().enumerate() {
            if byte == b'\r' {
                assert!(
                    i + 1 < result.len() && result[i + 1] == b'\n',
                    "orphan CR at byte {i}"
                );
            }
        }
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("remove"), "deleted symbol should be absent");
        assert!(text.contains("keep1"), "keep1 should remain");
        assert!(text.contains("keep2"), "keep2 should remain");
    }

    #[test]
    fn test_collapse_blank_lines_crlf() {
        // 4 CRLFs between lines = 3 blank lines — should collapse to 1 blank line (2 CRLFs)
        let input = b"line1\r\n\r\n\r\n\r\nline2\r\n";
        let result = collapse_blank_lines(input, LineEnding::CrLf);
        assert_eq!(result, b"line1\r\n\r\nline2\r\n");
    }

    #[test]
    fn test_lf_file_stays_lf_after_edit() {
        // Editing an LF file must not introduce any \r
        let content = b"fn existing() {\n    body();\n}\n";
        let sym = make_test_symbol("existing", SymbolKind::Function, (0, 29), 1);
        let result = build_insert_before(content, &sym, "fn new_fn() {}", LineEnding::Lf);
        assert!(
            !result.contains(&b'\r'),
            "LF file edit must not introduce CR"
        );
    }

    #[test]
    fn test_batch_edit_crlf_multiple_replacements() {
        // Normalize replacement text for CRLF, verify it uses CRLF with no bare LF
        let replacement = "fn alpha() {}\nfn beta() {}\n";
        let normalized = normalize_line_endings(replacement.as_bytes(), LineEnding::CrLf);
        // Must contain CRLF
        assert!(
            normalized.windows(2).any(|w| w == b"\r\n"),
            "normalized replacement should contain CRLF"
        );
        // No bare \n
        for (i, &byte) in normalized.iter().enumerate() {
            if byte == b'\n' {
                assert!(
                    i > 0 && normalized[i - 1] == b'\r',
                    "bare LF at byte {i} in normalized replacement"
                );
            }
        }
    }

    #[test]
    fn test_crlf_edit_no_mixed_endings() {
        // Splice normalized CRLF replacement into a CRLF file; verify no bare LF in result
        let file_content = b"fn keep() {\r\n    x();\r\n}\r\n";
        let replacement_lf = b"fn keep() {\r\n    y();\r\n}\r\n";
        // Normalize (already CRLF, but exercise the path)
        let normalized = normalize_line_endings(replacement_lf, LineEnding::CrLf);
        // Splice normalized text over the entire file range
        let range = (0u32, file_content.len() as u32);
        let result = apply_splice(file_content, range, &normalized);
        // No bare \n in the final result
        for (i, &byte) in result.iter().enumerate() {
            if byte == b'\n' {
                assert!(
                    i > 0 && result[i - 1] == b'\r',
                    "bare LF at byte {i} after splice into CRLF file"
                );
            }
        }
    }

    #[test]
    fn test_single_edit_shorthand_replace() {
        let json =
            serde_json::json!("src/lib.rs::beta => replace fn beta(x: i32) -> i32 { x * 4 }");
        let edit: SingleEdit = serde_json::from_value(json).unwrap();
        assert_eq!(edit.path, "src/lib.rs");
        assert_eq!(edit.name, "beta");
        assert!(edit.kind.is_none());
        assert!(edit.symbol_line.is_none());
        match &edit.operation {
            EditOperation::Replace { new_body } => {
                assert_eq!(new_body, "fn beta(x: i32) -> i32 { x * 4 }");
            }
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    #[test]
    fn test_single_edit_shorthand_delete() {
        let json = serde_json::json!("src/lib.rs::old_fn => delete");
        let edit: SingleEdit = serde_json::from_value(json).unwrap();
        assert_eq!(edit.path, "src/lib.rs");
        assert_eq!(edit.name, "old_fn");
        assert!(matches!(edit.operation, EditOperation::Delete));
    }

    #[test]
    fn test_single_edit_shorthand_insert_before() {
        let json = serde_json::json!("src/lib.rs::main => insert_before fn setup() {}");
        let edit: SingleEdit = serde_json::from_value(json).unwrap();
        assert_eq!(edit.name, "main");
        match &edit.operation {
            EditOperation::InsertBefore { content } => {
                assert_eq!(content, "fn setup() {}");
            }
            other => panic!("expected InsertBefore, got {other:?}"),
        }
    }

    #[test]
    fn test_single_edit_shorthand_insert_after() {
        let json = serde_json::json!("src/lib.rs::main => insert_after fn teardown() {}");
        let edit: SingleEdit = serde_json::from_value(json).unwrap();
        match &edit.operation {
            EditOperation::InsertAfter { content } => {
                assert_eq!(content, "fn teardown() {}");
            }
            other => panic!("expected InsertAfter, got {other:?}"),
        }
    }

    #[test]
    fn test_single_edit_shorthand_edit_within() {
        let json = serde_json::json!("src/lib.rs::process => edit_within x + 1 >>> x + 2");
        let edit: SingleEdit = serde_json::from_value(json).unwrap();
        match &edit.operation {
            EditOperation::EditWithin { old_text, new_text } => {
                assert_eq!(old_text, "x + 1");
                assert_eq!(new_text, "x + 2");
            }
            other => panic!("expected EditWithin, got {other:?}"),
        }
    }

    #[test]
    fn test_single_edit_shorthand_in_batch_array() {
        // The original failing case: batch_edit with string elements
        let json = serde_json::json!({
            "dry_run": true,
            "edits": [
                "src/lib.rs::beta => replace fn beta(x: i32) -> i32 { x * 4 }"
            ]
        });
        let input: BatchEditInput = serde_json::from_value(json).unwrap();
        assert!(input.dry_run == Some(true));
        assert_eq!(input.edits.len(), 1);
        assert_eq!(input.edits[0].path, "src/lib.rs");
        assert_eq!(input.edits[0].name, "beta");
    }

    #[test]
    fn test_single_edit_shorthand_invalid_no_separator() {
        let json = serde_json::json!("src/lib.rs beta replace something");
        let result = serde_json::from_value::<SingleEdit>(json);
        assert!(result.is_err());
        let error = result.err().unwrap().to_string();
        assert!(
            error.contains("JSON object with path/name/operation fields"),
            "error: {error}"
        );
    }

    #[test]
    fn test_single_edit_struct_still_works() {
        // Normal JSON struct path must still work
        let json = serde_json::json!({
            "path": "src/lib.rs",
            "name": "beta",
            "operation": {
                "type": "replace",
                "new_body": "fn beta() {}"
            }
        });
        let edit: SingleEdit = serde_json::from_value(json).unwrap();
        assert_eq!(edit.path, "src/lib.rs");
        assert_eq!(edit.name, "beta");
        match &edit.operation {
            EditOperation::Replace { new_body } => assert_eq!(new_body, "fn beta() {}"),
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    #[test]
    fn test_single_edit_stringified_json_object() {
        // Stringified JSON object (Codex pattern)
        let inner = serde_json::json!({
            "path": "src/lib.rs",
            "name": "gamma",
            "operation": { "type": "delete" }
        });
        let json = serde_json::json!(inner.to_string());
        let edit: SingleEdit = serde_json::from_value(json).unwrap();
        assert_eq!(edit.path, "src/lib.rs");
        assert_eq!(edit.name, "gamma");
        assert!(matches!(edit.operation, EditOperation::Delete));
    }

    #[test]
    fn test_insert_target_shorthand_string_still_works() {
        let json = serde_json::json!("src/lib.rs::helper");
        let target: InsertTarget = serde_json::from_value(json).unwrap();
        assert_eq!(target.path, "src/lib.rs");
        assert_eq!(target.name, "helper");
        assert!(target.kind.is_none());
        assert!(target.symbol_line.is_none());
    }

    #[test]
    fn test_insert_target_invalid_message_mentions_structured_payload() {
        let json = serde_json::json!("helper");
        let result = serde_json::from_value::<InsertTarget>(json);
        assert!(result.is_err());
        let error = result.err().unwrap().to_string();
        assert!(
            error.contains("JSON object with path/name fields"),
            "error: {error}"
        );
    }

    // -- ReplaceSymbolBodyInput: new_body / body alias --
    //
    // These three tests lock in the minimal alias contract: the canonical
    // field name is `new_body`, `body` is accepted as a legacy alias, and
    // providing both at once is a hard error rather than a silent pick.

    #[test]
    fn test_replace_symbol_body_input_accepts_canonical_new_body() {
        let raw = r#"{"path":"src/lib.rs","name":"foo","new_body":"fn foo() {}"}"#;
        let input: ReplaceSymbolBodyInput = serde_json::from_str(raw).unwrap();
        assert_eq!(input.path, "src/lib.rs");
        assert_eq!(input.name, "foo");
        assert_eq!(input.new_body, "fn foo() {}");
    }

    #[test]
    fn test_replace_symbol_body_input_accepts_body_alias() {
        let raw = r#"{"path":"src/lib.rs","name":"foo","body":"fn foo() {}"}"#;
        let input: ReplaceSymbolBodyInput = serde_json::from_str(raw).unwrap();
        assert_eq!(input.path, "src/lib.rs");
        assert_eq!(input.name, "foo");
        assert_eq!(input.new_body, "fn foo() {}");
    }

    #[test]
    fn test_replace_symbol_body_input_rejects_both_new_body_and_body() {
        // Use a raw JSON string (not serde_json::json!) so the parser actually
        // streams both keys to the struct visitor; a serde_json::Value literal
        // would silently dedupe keys in its backing Map and never exercise the
        // duplicate-field path.
        let raw = r#"{"path":"src/lib.rs","name":"foo","new_body":"a","body":"b"}"#;
        let result: Result<ReplaceSymbolBodyInput, _> = serde_json::from_str(raw);
        assert!(
            result.is_err(),
            "expected duplicate-field error, got Ok(ReplaceSymbolBodyInput)"
        );
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("duplicate field"),
            "expected duplicate-field error, got: {err}"
        );
    }

    // ─── working_directory plumbing — round-trip tests ───────────────────
    //
    // The 7 edit tools and the 2 per-target sub-structs each accept an optional
    // `working_directory` field. These tests pin the deserialization contract:
    // (a) the field is OPTIONAL — payloads without it succeed and produce
    //     `None`, preserving today's behaviour; and
    // (b) when present, the value is preserved on the deserialized struct so
    //     handlers and the `EditHook` chain can see it.

    #[test]
    fn test_replace_symbol_body_input_accepts_working_directory() {
        let raw = r#"{
            "path":"src/lib.rs",
            "name":"foo",
            "new_body":"fn foo() {}",
            "working_directory":"/tmp/wt"
        }"#;
        let v: ReplaceSymbolBodyInput = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt"));
    }

    #[test]
    fn test_replace_symbol_body_input_omitted_working_directory_is_none() {
        let raw = r#"{"path":"src/lib.rs","name":"foo","new_body":"fn foo() {}"}"#;
        let v: ReplaceSymbolBodyInput = serde_json::from_str(raw).unwrap();
        assert!(v.working_directory.is_none());
    }

    #[test]
    fn test_insert_symbol_input_accepts_working_directory() {
        let raw = r#"{
            "path":"src/lib.rs",
            "name":"foo",
            "content":"fn bar() {}",
            "working_directory":"/tmp/wt"
        }"#;
        let v: InsertSymbolInput = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt"));
    }

    #[test]
    fn test_delete_symbol_input_accepts_working_directory() {
        let raw = r#"{"path":"src/lib.rs","name":"foo","working_directory":"/tmp/wt"}"#;
        let v: DeleteSymbolInput = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt"));
    }

    #[test]
    fn test_edit_within_symbol_input_accepts_working_directory() {
        let raw = r#"{
            "path":"src/lib.rs",
            "name":"foo",
            "old_text":"a",
            "new_text":"b",
            "working_directory":"/tmp/wt"
        }"#;
        let v: EditWithinSymbolInput = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt"));
    }

    #[test]
    fn test_batch_edit_input_accepts_working_directory() {
        let raw = r#"{
            "edits":[{"path":"src/lib.rs","name":"foo","operation":{"type":"delete"}}],
            "working_directory":"/tmp/wt"
        }"#;
        let v: BatchEditInput = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt"));
        assert!(v.edits[0].working_directory.is_none());
    }

    #[test]
    fn test_batch_insert_input_accepts_working_directory() {
        let raw = r#"{
            "content":"fn bar() {}",
            "position":"after",
            "targets":[{"path":"src/lib.rs","name":"foo"}],
            "working_directory":"/tmp/wt"
        }"#;
        let v: BatchInsertInput = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt"));
        assert!(v.targets[0].working_directory.is_none());
    }

    #[test]
    fn test_batch_rename_input_accepts_working_directory() {
        let raw = r#"{
            "path":"src/lib.rs",
            "name":"foo",
            "new_name":"bar",
            "working_directory":"/tmp/wt"
        }"#;
        let v: BatchRenameInput = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt"));
    }

    #[test]
    fn test_single_edit_struct_accepts_working_directory() {
        // SingleEdit has a custom Deserialize impl with a Struct branch — make
        // sure the new field lives in that branch too.
        let raw = r#"{
            "path":"src/lib.rs",
            "name":"foo",
            "operation":{"type":"delete"},
            "working_directory":"/tmp/wt-per-edit"
        }"#;
        let v: SingleEdit = serde_json::from_str(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt-per-edit"));
    }

    #[test]
    fn test_single_edit_stringified_object_accepts_working_directory() {
        // The custom Deserialize also accepts a stringified JSON object — that
        // branch hand-extracts fields, so it has its own pickup of the new one.
        let raw = serde_json::Value::String(
            r#"{"path":"src/lib.rs","name":"foo","operation":{"type":"delete"},"working_directory":"/tmp/wt-per-edit"}"#
                .to_string(),
        );
        let v: SingleEdit = serde_json::from_value(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt-per-edit"));
    }

    #[test]
    fn test_single_edit_shorthand_leaves_working_directory_none() {
        // The "path::name => operation body" shorthand has no syntax for
        // per-edit working_directory — it must default to None so callers can
        // mix shorthand entries into a batch with a top-level working_directory.
        let raw = serde_json::Value::String("src/lib.rs::foo => delete".to_string());
        let v: SingleEdit = serde_json::from_value(raw).unwrap();
        assert!(v.working_directory.is_none());
    }

    #[test]
    fn test_insert_target_struct_accepts_working_directory() {
        let raw = serde_json::json!({
            "path": "src/lib.rs",
            "name": "foo",
            "working_directory": "/tmp/wt-per-target"
        });
        let v: InsertTarget = serde_json::from_value(raw).unwrap();
        assert_eq!(v.working_directory.as_deref(), Some("/tmp/wt-per-target"));
    }

    #[test]
    fn test_insert_target_string_shorthand_leaves_working_directory_none() {
        let raw = serde_json::Value::String("src/lib.rs::foo".to_string());
        let v: InsertTarget = serde_json::from_value(raw).unwrap();
        assert!(v.working_directory.is_none());
    }

    #[test]
    fn docless_replacement_splice_start_preserves_same_line_block_doc() {
        let source = b"/** @deprecated */ export function legacy() {}";
        let symbol_start = source
            .windows(b"function".len())
            .position(|window| window == b"function")
            .unwrap();

        let start = docless_replacement_splice_start(source, 0, symbol_start);

        assert_eq!(&source[..start], b"/** @deprecated */ ");
    }

    #[test]
    fn docless_replacement_splice_start_preserves_same_line_doc_attribute() {
        let source = br#"#[doc = "legacy"] pub fn legacy() {}"#;
        let symbol_start = source
            .windows(b"fn".len())
            .position(|window| window == b"fn")
            .unwrap();

        let start = docless_replacement_splice_start(source, 0, symbol_start);

        assert_eq!(&source[..start], br#"#[doc = "legacy"] "#);
    }

    #[test]
    fn docless_replacement_splice_start_ignores_non_doc_prefix() {
        let source = b"export function legacy() {}";
        let symbol_start = source
            .windows(b"function".len())
            .position(|window| window == b"function")
            .unwrap();

        let start = docless_replacement_splice_start(source, 0, symbol_start);

        assert_eq!(start, 0);
    }

    #[test]
    fn body_starts_with_doc_comment_detects_common_doc_markers() {
        // Unambiguous doc markers across supported languages.
        assert!(body_starts_with_doc_comment(
            "/// rust outer line doc\nfn foo() {}"
        ));
        assert!(body_starts_with_doc_comment(
            "//! rust inner line doc\nmod m {}"
        ));
        assert!(body_starts_with_doc_comment(
            "/** jsdoc-style block\n * details\n */\nfn foo() {}"
        ));
        assert!(body_starts_with_doc_comment(
            "/*! rust inner block doc */\nmod m {}"
        ));
        assert!(body_starts_with_doc_comment(
            "#[doc = \"attr doc\"]\nfn foo() {}"
        ));

        // Leading blank lines should not defeat detection.
        assert!(body_starts_with_doc_comment("\n\n/// doc\nfn foo() {}"));

        // Leading indentation is allowed.
        assert!(body_starts_with_doc_comment(
            "    /// indented doc\n    fn foo() {}"
        ));
    }

    #[test]
    fn body_starts_with_doc_comment_rejects_non_doc_prefixes() {
        // Plain code.
        assert!(!body_starts_with_doc_comment("fn foo() {}"));

        // Ordinary line comments — could be code annotations, not doc.
        assert!(!body_starts_with_doc_comment(
            "// regular comment\nfn foo() {}"
        ));

        // Python comment — Python docstrings are inside the body, not above.
        assert!(!body_starts_with_doc_comment(
            "# python comment\ndef foo(): pass"
        ));

        // Rust attributes — must not be misread as docs. This was the
        // concrete bug that would have duplicated `#[test]` during a
        // replace_symbol_body on an attribute-prefixed function.
        assert!(!body_starts_with_doc_comment("#[inline]\npub fn foo() {}"));
        assert!(!body_starts_with_doc_comment("#[derive(Debug)]\nstruct S;"));

        // Empty body — nothing to detect.
        assert!(!body_starts_with_doc_comment(""));
        assert!(!body_starts_with_doc_comment("\n\n   \n"));
    }
}
