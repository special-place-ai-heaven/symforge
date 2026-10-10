use crate::domain::SymbolRecord;

/// The exact byte plan used by both MCP replacement and embedded replacement.
pub(crate) struct PreparedReplace {
    pub new_content: Vec<u8>,
    pub old_bytes: usize,
    pub inserted_bytes: usize,
}

pub(crate) fn prepare_replace(
    file_content: &[u8],
    symbol: &SymbolRecord,
    new_body: &str,
) -> Result<PreparedReplace, &'static str> {
    let (start, end) = symbol.byte_range;
    let (splice_start, _) = replacement_splice_range(file_content, symbol, new_body)?;
    let indent = detect_indentation(file_content, start);
    let line_ending = detect_line_ending(file_content);
    let normalized = normalize_line_endings(new_body.as_bytes(), line_ending);
    let normalized_str = std::str::from_utf8(&normalized).unwrap_or(new_body);
    let indented = apply_indentation(normalized_str, &indent, line_ending);
    let new_content = apply_splice(file_content, (splice_start, end), &indented);
    Ok(PreparedReplace {
        new_content,
        old_bytes: (end - start) as usize,
        inserted_bytes: indented.len(),
    })
}

/// The physical source bytes consumed by `prepare_replace`, including any
/// leading modifiers, indentation, or orphaned docs it actually removes.
pub(crate) fn replacement_splice_range(
    file_content: &[u8],
    symbol: &SymbolRecord,
    new_body: &str,
) -> Result<(u32, u32), &'static str> {
    let (start, end) = symbol.byte_range;
    if start > end || end as usize > file_content.len() || symbol.effective_start() > start {
        return Err("symbol span is outside the captured file");
    }
    let new_body_supplies_docs = body_starts_with_doc_comment(new_body);
    let effective = if new_body_supplies_docs {
        symbol.effective_start() as usize
    } else {
        start as usize
    };
    let raw_line_start = file_content[..effective]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map(|position| position + 1)
        .unwrap_or(0);
    let splice_start = if new_body_supplies_docs {
        extend_past_orphaned_docs(file_content, raw_line_start, symbol)
    } else {
        docless_replacement_splice_start(file_content, raw_line_start, start as usize)
    };
    Ok((splice_start as u32, end))
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

/// The longest leading-whitespace prefix common to every line of `lines` that
/// has non-whitespace content (blank / whitespace-only lines are ignored). This
/// is the body's uniform base indent — empty when any content line is already
/// flush-left (the normal case). Mirrors the prefix `textwrap.dedent` strips.
fn common_leading_whitespace<'a>(lines: &[&'a str]) -> &'a str {
    let mut common: Option<&'a str> = None;
    for raw in lines {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            continue;
        }
        let ws = &line[..line.len() - line.trim_start().len()];
        common = Some(match common {
            None => ws,
            Some(prev) => {
                let max = prev.len().min(ws.len());
                let (pb, wb) = (prev.as_bytes(), ws.as_bytes());
                let mut end = 0;
                while end < max && pb[end] == wb[end] {
                    end += 1;
                }
                &prev[..end]
            }
        });
        if common == Some("") {
            break;
        }
    }
    common.unwrap_or("")
}

/// Re-column `text` to `indent`: strip the body's uniform base indent, then
/// prefix each non-empty line with `indent`, using the given line ending.
///
/// Stripping the common base indent first means a body the caller pasted at
/// some other column (e.g. an 8-space chat-context indent) is re-columned to
/// exactly the symbol's `indent` rather than COMPOUNDING to base+indent. When
/// the body is already flush-left (its first content line has no leading
/// whitespace — the normal case) the base is empty and this is a pure prefix,
/// so existing callers are unaffected.
pub(crate) fn apply_indentation(text: &str, indent: &[u8], line_ending: LineEnding) -> Vec<u8> {
    let mut result = Vec::new();
    // Use split('\n') instead of lines() so that trailing newlines produce a trailing
    // empty element, preserving them. str::lines() silently strips all trailing newlines.
    let parts: Vec<&str> = text.split('\n').collect();
    let base = common_leading_whitespace(&parts);
    for (i, line) in parts.iter().enumerate() {
        // Strip '\r' left behind by split('\n') on CRLF input; re-emit via line_ending.
        let line = line.strip_suffix('\r').unwrap_or(line);
        if i > 0 {
            result.extend_from_slice(line_ending.as_bytes());
        }
        if !line.is_empty() {
            // Dedent the uniform base, then apply the symbol's column.
            let dedented = line.strip_prefix(base).unwrap_or(line);
            result.extend_from_slice(indent);
            result.extend_from_slice(dedented.as_bytes());
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
    let line_start = insert_before_position(file_content, sym);
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

pub(crate) fn insert_before_position(file_content: &[u8], sym: &SymbolRecord) -> u32 {
    let sym_start = sym.effective_start() as usize;
    file_content[..sym_start]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map(|position| position + 1)
        .unwrap_or(0) as u32
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
    let insert_pos = insert_after_position(file_content, sym);
    apply_splice(file_content, (insert_pos, insert_pos), &insertion)
}

pub(crate) fn insert_after_position(file_content: &[u8], sym: &SymbolRecord) -> u32 {
    skip_trailing_semicolon(file_content, sym.byte_range.1 as usize) as u32
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

/// Walk upward from `line_start` (the first byte of the symbol's opening
/// line) and include contiguous Rust outer-attribute lines (`#[...]`) that sit
/// directly above the item with no blank line between. Attributes belong to
/// the item; leaving them behind on a delete orphans them onto the following
/// item — for example a stray `#[test]` that then fails to compile. Inner
/// attributes (`#![...]`) are not consumed (they belong to the enclosing
/// scope), and only the leading line of a multi-line attribute is recognized,
/// which is never worse than the previous behaviour of consuming none.
fn extend_past_leading_attributes(file_content: &[u8], line_start: usize) -> usize {
    let mut start = line_start;
    while start > 0 {
        // `start` sits just past a '\n'; find the bounds of the line above it.
        let prev_line_end = start - 1;
        let prev_line_start = file_content[..prev_line_end]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line = &file_content[prev_line_start..prev_line_end];
        let trimmed = std::str::from_utf8(line).unwrap_or("").trim();
        if trimmed.starts_with("#[") {
            start = prev_line_start;
        } else {
            break;
        }
    }
    start
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
    let (start, end) = delete_source_range(file_content, sym, line_ending);
    let spliced = apply_splice(file_content, (start, end), b"");
    collapse_blank_lines(&spliced, line_ending)
}

pub(crate) fn delete_source_range(
    file_content: &[u8],
    sym: &SymbolRecord,
    line_ending: LineEnding,
) -> (u32, u32) {
    // Extend to start of line (include leading whitespace, attached attributes,
    // and orphaned doc comments).
    let start = {
        let s = sym.effective_start() as usize;
        let line_start = file_content[..s]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        // Consume contiguous outer-attribute lines (`#[...]`) directly above the
        // item so they are removed with it instead of being orphaned onto the
        // next item (e.g. a stray `#[test]`, which then fails to compile).
        let after_attrs = extend_past_leading_attributes(file_content, line_start);
        extend_past_orphaned_docs(file_content, after_attrs, sym) as u32
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
    (start, end)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SpliceFootprint {
    Bytes(u32, u32),
    Insert(u32),
}

pub(crate) fn splice_footprints_overlap(left: SpliceFootprint, right: SpliceFootprint) -> bool {
    match (left, right) {
        (SpliceFootprint::Bytes(a, b), SpliceFootprint::Bytes(c, d)) => a < d && c < b,
        (SpliceFootprint::Insert(at), SpliceFootprint::Bytes(start, end))
        | (SpliceFootprint::Bytes(start, end), SpliceFootprint::Insert(at)) => {
            start <= at && at < end
        }
        (SpliceFootprint::Insert(left), SpliceFootprint::Insert(right)) => left == right,
    }
}

/// Extra source ranges removed by `build_delete`'s whole-file blank-line
/// cleanup, expressed in the same original byte coordinates as the main
/// deletion. A batch must account for these before applying cached spans.
pub(crate) fn delete_cleanup_ranges(
    file_content: &[u8],
    sym: &SymbolRecord,
    line_ending: LineEnding,
) -> Vec<(u32, u32)> {
    let (start, end) = delete_source_range(file_content, sym, line_ending);
    let spliced = apply_splice(file_content, (start, end), b"");
    let mut removed = Vec::new();
    match line_ending {
        LineEnding::Lf => {
            let mut consecutive = 0;
            for (position, byte) in spliced.iter().enumerate() {
                if *byte == b'\n' {
                    consecutive += 1;
                    if consecutive > 2 {
                        push_cleanup_range(&mut removed, position, position + 1, start, end);
                    }
                } else {
                    consecutive = 0;
                }
            }
        }
        LineEnding::CrLf => {
            let mut consecutive = 0;
            let mut position = 0;
            while position < spliced.len() {
                if spliced.get(position..position + 2) == Some(b"\r\n") {
                    consecutive += 1;
                    if consecutive > 2 {
                        push_cleanup_range(&mut removed, position, position + 2, start, end);
                    }
                    position += 2;
                } else {
                    consecutive = 0;
                    position += 1;
                }
            }
        }
    }
    removed
}

fn push_cleanup_range(
    removed: &mut Vec<(u32, u32)>,
    spliced_start: usize,
    spliced_end: usize,
    main_start: u32,
    main_end: u32,
) {
    let boundary = main_start as usize;
    for (start, end) in [
        (spliced_start, spliced_end.min(boundary)),
        (spliced_start.max(boundary), spliced_end),
    ] {
        if start >= end {
            continue;
        }
        let shift = if start >= boundary {
            (main_end - main_start) as usize
        } else {
            0
        };
        let source_start = (start + shift) as u32;
        let source_end = (end + shift) as u32;
        if let Some(last) = removed.last_mut()
            && last.1 == source_start
        {
            last.1 = source_end;
        } else {
            removed.push((source_start, source_end));
        }
    }
}

/// Any removed byte before an unprocessed splice shifts its captured offset;
/// removal within a splice changes its target bytes. Both require refusal.
pub(crate) fn cleanup_invalidates_splice(cleanup: &[(u32, u32)], other: SpliceFootprint) -> bool {
    let last_target_byte = match other {
        SpliceFootprint::Bytes(_, end) => end,
        SpliceFootprint::Insert(at) => at.saturating_add(1),
    };
    cleanup.iter().any(|(start, _)| *start < last_target_byte)
}

/// Collapse runs of 3+ consecutive newlines down to 2 (one blank line).
/// On CRLF files, counts `\r\n` pairs; on LF files, counts `\n` bytes.
pub(crate) fn collapse_blank_lines(content: &[u8], line_ending: LineEnding) -> Vec<u8> {
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

/// The same edit-within match selection used by the MCP writer and Embed.
/// Errors carry no source text or search needle so callers can apply their
/// own bounded, secret-safe presentation policy.
pub(crate) struct WithinSelection {
    pub new_body: String,
    pub count: usize,
    pub untargeted_extra: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WithinSelectionError {
    InvalidSpan,
    InvalidUtf8,
    ConflictingTargeting,
    OccurrenceOutOfRange { requested: u32, total: usize },
    NotFound,
}

pub(crate) fn select_edit_within_body(
    file_content: &[u8],
    sym: &SymbolRecord,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
    occurrence: Option<u32>,
    near_line: Option<u32>,
) -> Result<WithinSelection, WithinSelectionError> {
    let sym_start = sym.effective_start() as usize;
    let sym_end = sym.byte_range.1 as usize;
    let body = file_content
        .get(sym_start..sym_end)
        .ok_or(WithinSelectionError::InvalidSpan)?;
    let body_str = std::str::from_utf8(body).map_err(|_| WithinSelectionError::InvalidUtf8)?;
    let line_ending = detect_line_ending(file_content);
    let normalized_old = normalize_line_endings(old_text.as_bytes(), line_ending);
    let normalized_old_str =
        String::from_utf8(normalized_old).unwrap_or_else(|_| old_text.to_string());
    let normalized_new = normalize_line_endings(new_text.as_bytes(), line_ending);
    let normalized_new_str =
        String::from_utf8(normalized_new).unwrap_or_else(|_| new_text.to_string());
    let targeting_modes = usize::from(replace_all)
        + usize::from(occurrence.is_some())
        + usize::from(near_line.is_some());
    if targeting_modes > 1 {
        return Err(WithinSelectionError::ConflictingTargeting);
    }
    let (new_body, count, untargeted_extra) = if replace_all {
        let count = body_str.matches(&normalized_old_str).count();
        if count > 0 {
            (
                body_str.replace(&normalized_old_str, &normalized_new_str),
                count,
                0,
            )
        } else {
            match try_whitespace_flexible_replace(
                body_str,
                &normalized_old_str,
                &normalized_new_str,
                true,
            ) {
                Some((body, count)) => (body, count, 0),
                None => return Err(WithinSelectionError::NotFound),
            }
        }
    } else if occurrence.is_some() || near_line.is_some() {
        let positions: Vec<usize> = body_str
            .match_indices(normalized_old_str.as_str())
            .map(|(pos, _)| pos)
            .collect();
        if positions.is_empty() {
            return Err(WithinSelectionError::NotFound);
        }
        let index = if let Some(n) = occurrence {
            let n_usize = n as usize;
            if n_usize == 0 || n_usize > positions.len() {
                return Err(WithinSelectionError::OccurrenceOutOfRange {
                    requested: n,
                    total: positions.len(),
                });
            }
            n_usize - 1
        } else {
            let target = i64::from(near_line.unwrap_or(1));
            let line_of = |pos: usize| {
                1 + file_content[..sym_start + pos]
                    .iter()
                    .filter(|&&byte| byte == b'\n')
                    .count() as i64
            };
            (0..positions.len())
                .min_by_key(|&i| (line_of(positions[i]) - target).abs())
                .unwrap_or(0)
        };
        let pos = positions[index];
        let mut body = String::with_capacity(body_str.len() + normalized_new_str.len());
        body.push_str(&body_str[..pos]);
        body.push_str(&normalized_new_str);
        body.push_str(&body_str[pos + normalized_old_str.len()..]);
        (body, 1, 0)
    } else if body_str.contains(normalized_old_str.as_str()) {
        (
            body_str.replacen(&normalized_old_str, &normalized_new_str, 1),
            1,
            body_str
                .matches(&normalized_old_str)
                .count()
                .saturating_sub(1),
        )
    } else {
        match try_whitespace_flexible_replace(
            body_str,
            &normalized_old_str,
            &normalized_new_str,
            false,
        ) {
            Some((body, count)) => (body, count, 0),
            None => return Err(WithinSelectionError::NotFound),
        }
    };
    Ok(WithinSelection {
        new_body,
        count,
        untargeted_extra,
    })
}

// ---------------------------------------------------------------------------
