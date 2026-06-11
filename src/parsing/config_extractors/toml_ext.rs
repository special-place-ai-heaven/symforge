use super::{
    ConfigExtractor, EditCapability, ExtractionOutcome, ExtractionResult, MAX_DEPTH,
    build_line_starts, byte_to_line, join_key_path,
};
use crate::domain::{SymbolKind, SymbolRecord};

use super::{parse_diagnostic, parse_diagnostic_from_span};

pub struct TomlExtractor;

impl ConfigExtractor for TomlExtractor {
    fn extract(&self, content: &[u8]) -> ExtractionResult {
        let content_str = match std::str::from_utf8(content) {
            Ok(s) => s,
            Err(e) => {
                return ExtractionResult {
                    symbols: vec![],
                    outcome: ExtractionOutcome::Failed(parse_diagnostic(
                        "utf-8",
                        e.to_string(),
                        None,
                        None,
                        None,
                        false,
                    )),
                };
            }
        };

        if content_str.trim().is_empty() {
            return ExtractionResult {
                symbols: vec![],
                outcome: ExtractionOutcome::Ok,
            };
        }

        // Try strict parse first; fall back to line-scanning on parse error.
        // Note: DocumentMut rejects some constructs (e.g. [a] with a.b="v" then [a.b])
        // that real-world TOML files use. Line scanning handles these gracefully.
        let line_starts = build_line_starts(content);

        match content_str.parse::<toml_edit::DocumentMut>() {
            Ok(doc) => {
                let mut symbols = Vec::new();
                let mut sort_order: u32 = 0;
                let mut walker = TomlWalker {
                    raw: content,
                    line_starts: &line_starts,
                    symbols: &mut symbols,
                    sort_order: &mut sort_order,
                };
                walker.walk_table(doc.as_table(), "", 0);
                ExtractionResult {
                    symbols,
                    outcome: ExtractionOutcome::Ok,
                }
            }
            Err(parse_err) => {
                // Fall back to line-based scanning so we still extract useful keys.
                let symbols = line_scan(content, &line_starts);
                let diagnostic = parse_diagnostic_from_span(
                    "toml_edit",
                    parse_err.message(),
                    content,
                    parse_err.span(),
                    !symbols.is_empty(),
                );
                let outcome = if symbols.is_empty() {
                    ExtractionOutcome::Failed(diagnostic)
                } else {
                    ExtractionOutcome::Partial(diagnostic)
                };
                ExtractionResult { symbols, outcome }
            }
        }
    }

    fn edit_capability(&self) -> EditCapability {
        EditCapability::StructuralEditSafe
    }
}

// ---------------------------------------------------------------------------
// toml_edit document walker (used when parse succeeds)
// ---------------------------------------------------------------------------

struct TomlWalker<'a> {
    raw: &'a [u8],
    line_starts: &'a [u32],
    symbols: &'a mut Vec<SymbolRecord>,
    sort_order: &'a mut u32,
}

impl TomlWalker<'_> {
    fn push_symbol(&mut self, key_path: &str, depth: u32, start: usize, end: usize) {
        self.symbols.push(make_symbol(
            key_path,
            depth,
            start,
            end,
            *self.sort_order,
            self.line_starts,
        ));
        *self.sort_order += 1;
    }

    fn walk_table(&mut self, table: &toml_edit::Table, parent_path: &str, depth: u32) {
        if depth >= MAX_DEPTH {
            return;
        }
        let mut search_from: usize = 0;
        for (key, item) in table.iter() {
            let key_path = join_key_path(parent_path, key);
            self.walk_item(item, key, &key_path, depth, &mut search_from);
        }
    }

    fn walk_item(
        &mut self,
        item: &toml_edit::Item,
        raw_key: &str,
        key_path: &str,
        depth: u32,
        search_from: &mut usize,
    ) {
        match item {
            toml_edit::Item::None => {}

            toml_edit::Item::Value(value) => {
                let (start, end) = find_key_value_bytes(self.raw, raw_key, *search_from);
                if end > *search_from {
                    *search_from = end;
                }
                self.push_symbol(key_path, depth, start, end);

                if depth + 1 < MAX_DEPTH
                    && let Some(inline_table) = value.as_inline_table()
                {
                    for (k, v) in inline_table.iter() {
                        let child_path = join_key_path(key_path, k);
                        self.walk_item(
                            &toml_edit::Item::Value(v.clone()),
                            k,
                            &child_path,
                            depth + 1,
                            search_from,
                        );
                    }
                }
            }

            toml_edit::Item::Table(table) => {
                let (start, end) = find_table_header_bytes(self.raw, key_path);
                self.push_symbol(key_path, depth, start, end);
                if depth + 1 < MAX_DEPTH {
                    self.walk_table(table, key_path, depth + 1);
                }
            }

            toml_edit::Item::ArrayOfTables(array) => {
                for (i, table) in array.iter().enumerate() {
                    let indexed_path = format!("{}[{}]", key_path, i);
                    let (start, end) = find_array_table_header_bytes(self.raw, key_path, i);
                    self.push_symbol(&indexed_path, depth, start, end);
                    if depth + 1 < MAX_DEPTH {
                        self.walk_table(table, &indexed_path, depth + 1);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Line-based fallback scanner (used when toml_edit rejects the file)
// ---------------------------------------------------------------------------

/// Scan TOML line by line, extracting section headers and key = value lines.
/// Does not recurse into inline tables. Suitable for files that toml_edit rejects.
fn line_scan(bytes: &[u8], line_starts: &[u32]) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order: u32 = 0;
    let mut current_section: String = String::new();
    let mut depth_offset: u32 = 0;
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let line_start = i;
        let line_end = bytes[i..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);

        let line_bytes = &bytes[line_start..line_end];
        let trimmed = trim_leading_whitespace(line_bytes);

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with(b"#") {
            i = line_end;
            continue;
        }

        if trimmed.starts_with(b"[[") {
            // Array of tables: [[section]]
            if let Some(section) = extract_bracket_content(trimmed, true) {
                current_section = section.clone();
                depth_offset = section.matches('.').count() as u32;
                symbols.push(make_symbol(
                    &section,
                    depth_offset,
                    line_start,
                    line_end,
                    sort_order,
                    line_starts,
                ));
                sort_order += 1;
            }
        } else if trimmed.starts_with(b"[") {
            // Table header: [section]
            if let Some(section) = extract_bracket_content(trimmed, false) {
                current_section = section.clone();
                depth_offset = section.matches('.').count() as u32;
                symbols.push(make_symbol(
                    &section,
                    depth_offset,
                    line_start,
                    line_end,
                    sort_order,
                    line_starts,
                ));
                sort_order += 1;
            }
        } else if let Some(key) = extract_key_from_line(trimmed) {
            // key = value
            let key_path = if current_section.is_empty() {
                key.clone()
            } else {
                join_key_path(&current_section, &key)
            };
            let d = depth_offset + 1;
            if d < MAX_DEPTH {
                symbols.push(make_symbol(
                    &key_path,
                    d,
                    line_start,
                    line_end,
                    sort_order,
                    line_starts,
                ));
                sort_order += 1;
            }
        }

        i = line_end;
    }

    symbols
}

/// Extract section name from `[section]` or `[[section]]` line.
fn extract_bracket_content(line: &[u8], double: bool) -> Option<String> {
    let open: &[u8] = if double { b"[[" } else { b"[" };
    let close: &[u8] = if double { b"]]" } else { b"]" };

    if !line.starts_with(open) {
        return None;
    }
    let inner_start = open.len();
    let close_pos = line[inner_start..]
        .windows(close.len())
        .position(|w| w == close)?;
    let inner = &line[inner_start..inner_start + close_pos];
    let s = std::str::from_utf8(inner).ok()?.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Extract key name from `key = value` line. Returns bare key name.
fn extract_key_from_line(line: &[u8]) -> Option<String> {
    // Find `=` that isn't inside quotes
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in line.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if b == b'\\' && in_string {
            escape = true;
            continue;
        }
        if b == b'"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && b == b'=' {
            let key_bytes = trim_trailing_whitespace(&line[..i]);
            let key = std::str::from_utf8(key_bytes).ok()?.trim();
            // Strip surrounding quotes if any
            let key = key.trim_matches('"').trim_matches('\'');
            if key.is_empty() {
                return None;
            }
            return Some(key.to_string());
        }
    }
    None
}

fn trim_trailing_whitespace(s: &[u8]) -> &[u8] {
    let end = s
        .iter()
        .rposition(|&b| b != b' ' && b != b'\t')
        .map(|p| p + 1)
        .unwrap_or(0);
    &s[..end]
}

// ---------------------------------------------------------------------------
// Raw byte span finders (used by toml_edit walker)
// ---------------------------------------------------------------------------

fn find_key_value_bytes(bytes: &[u8], key: &str, search_from: usize) -> (usize, usize) {
    let key_bytes = key.as_bytes();
    let len = bytes.len();
    let mut i = search_from;
    while i < len {
        let line_start = i;
        let line_end = bytes[i..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);
        let line = &bytes[line_start..line_end];
        let trimmed = trim_leading_whitespace(line);
        if !trimmed.starts_with(b"#")
            && !trimmed.starts_with(b"[")
            && line_starts_with_key(trimmed, key_bytes)
        {
            return (line_start, line_end.min(len));
        }
        i = line_end;
    }
    (0, 0)
}

fn find_table_header_bytes(bytes: &[u8], key_path: &str) -> (usize, usize) {
    // key_path has been through join_key_path which escapes dots as ~1 and ~ as ~0.
    // Unescape before building the bracket pattern for raw-text search.
    let unescaped = unescape_key_path(key_path);
    find_header_pattern(bytes, &format!("[{}]", unescaped))
}

/// Reverse the escaping applied by `escape_key_segment` / `join_key_path`:
/// `~0` -> `~`, `~1` -> `.`, `~2` -> `[`, `~3` -> `]`.
///
/// A single left-to-right scan handles all four escapes unambiguously; the
/// previous sequential-`replace` form silently left `~2`/`~3` (escaped brackets)
/// untouched, producing a wrong header pattern for keys with literal brackets.
fn unescape_key_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.peek() {
                Some('0') => {
                    out.push('~');
                    chars.next();
                    continue;
                }
                Some('1') => {
                    out.push('.');
                    chars.next();
                    continue;
                }
                Some('2') => {
                    out.push('[');
                    chars.next();
                    continue;
                }
                Some('3') => {
                    out.push(']');
                    chars.next();
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

fn find_array_table_header_bytes(bytes: &[u8], key_path: &str, index: usize) -> (usize, usize) {
    let unescaped = unescape_key_path(key_path);
    let pattern = format!("[[{}]]", unescaped);
    let pattern_bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut count = 0;
    while i < len {
        let line_start = i;
        let line_end = bytes[i..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);
        let line = trim_leading_whitespace(&bytes[line_start..line_end]);
        if line.starts_with(pattern_bytes) {
            if count == index {
                return (line_start, line_end.min(len));
            }
            count += 1;
        }
        i = line_end;
    }
    (0, 0)
}

fn find_header_pattern(bytes: &[u8], pattern: &str) -> (usize, usize) {
    let pattern_bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        let line_start = i;
        let line_end = bytes[i..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);
        let line = trim_leading_whitespace(&bytes[line_start..line_end]);
        if line.starts_with(pattern_bytes) {
            return (line_start, line_end.min(len));
        }
        i = line_end;
    }
    (0, 0)
}

fn trim_leading_whitespace(s: &[u8]) -> &[u8] {
    let pos = s
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(s.len());
    &s[pos..]
}

fn line_starts_with_key(line: &[u8], key: &[u8]) -> bool {
    // Check unquoted form: key =
    if line.len() >= key.len() && line.starts_with(key) {
        let after = &line[key.len()..];
        if after
            .first()
            .map(|&b| b == b' ' || b == b'\t' || b == b'=')
            .unwrap_or(false)
        {
            return true;
        }
    }
    // Check quoted form: "key" =
    if line.first() == Some(&b'"') {
        let expected_len = 1 + key.len() + 1; // opening quote + key + closing quote
        if line.len() >= expected_len && line[1..].starts_with(key) && line[1 + key.len()] == b'"' {
            let after = &line[expected_len..];
            return after
                .first()
                .map(|&b| b == b' ' || b == b'\t' || b == b'=')
                .unwrap_or(false);
        }
    }
    false
}

fn make_symbol(
    name: &str,
    depth: u32,
    byte_start: usize,
    byte_end: usize,
    sort_order: u32,
    line_starts: &[u32],
) -> SymbolRecord {
    let start_line = byte_to_line(line_starts, byte_start as u32);
    let end_line = byte_to_line(line_starts, byte_end.saturating_sub(1) as u32);
    let byte_range = (byte_start as u32, byte_end as u32);
    SymbolRecord {
        name: name.to_string(),
        kind: SymbolKind::Key,
        depth,
        sort_order,
        byte_range,
        line_range: (start_line, end_line),
        doc_byte_range: None,
        item_byte_range: Some(byte_range),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_ranges_are_not_zero() {
        let content = b"name = \"test\"\nversion = \"1.0\"\n[package]\nauthors = [\"me\"]\n";
        let result = TomlExtractor.extract(content);
        assert!(!result.symbols.is_empty(), "should have symbols");
        // "version" is on line 1 (0-indexed). Its line_range must NOT be (0,0).
        let version = result.symbols.iter().find(|s| s.name == "version").unwrap();
        assert_eq!(
            version.line_range,
            (1, 1),
            "version is on line 1 but got {:?}",
            version.line_range
        );
        // [package] header is on line 2
        let package = result.symbols.iter().find(|s| s.name == "package").unwrap();
        assert_eq!(
            package.line_range.0, 2,
            "package header starts on line 2 but got {:?}",
            package.line_range
        );
    }

    #[test]
    fn test_top_level_keys() {
        let content = b"name = \"test\"\nversion = \"1.0\"\n";
        let result = TomlExtractor.extract(content);
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.name == "name" && s.kind == SymbolKind::Key)
        );
        assert!(result.symbols.iter().any(|s| s.name == "version"));
    }

    #[test]
    fn test_table_keys() {
        let content = b"[package]\nname = \"test\"\nversion = \"1.0\"\n";
        let result = TomlExtractor.extract(content);
        assert!(
            result.symbols.iter().any(|s| s.name == "package"),
            "missing package"
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "package.name"),
            "missing package.name"
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "package.version"),
            "missing package.version"
        );
    }

    #[test]
    fn test_nested_tables() {
        let content =
            b"[dependencies]\nserde = \"1.0\"\n\n[dependencies.serde]\nfeatures = [\"derive\"]\n";
        let result = TomlExtractor.extract(content);
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.name == "dependencies.serde"),
            "missing dependencies.serde; symbols={:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_inline_table() {
        let content = b"[package]\nmetadata = { key = \"value\" }\n";
        let result = TomlExtractor.extract(content);
        assert!(
            result.symbols.iter().any(|s| s.name == "package.metadata"),
            "missing package.metadata"
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.name == "package.metadata.key"),
            "missing package.metadata.key"
        );
    }

    #[test]
    fn test_empty_file() {
        assert!(TomlExtractor.extract(b"").symbols.is_empty());
    }

    #[test]
    fn test_malformed_toml() {
        let result = TomlExtractor.extract(b"[invalid\nno closing");
        assert!(result.symbols.is_empty());

        let diagnostic = match &result.outcome {
            ExtractionOutcome::Failed(diagnostic) => diagnostic,
            _ => panic!("expected failed extraction"),
        };

        assert_eq!(diagnostic.parser, "toml_edit");
        assert!(!diagnostic.fallback_used);
        assert!(diagnostic.line.is_some());
        assert!(diagnostic.column.is_some());
    }

    #[test]
    fn test_malformed_toml_reports_partial_diagnostic_when_line_scan_recovers_symbols() {
        let result = TomlExtractor.extract(
            b"[package]\nname = \"symforge\"\nversion = \"0.1.0\"\ninvalid = \"unterminated\n",
        );

        assert!(
            result.symbols.iter().any(|symbol| symbol.name == "package"),
            "fallback scan should still recover the table header"
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|symbol| symbol.name == "package.name"),
            "fallback scan should still recover keys"
        );

        let diagnostic = match &result.outcome {
            ExtractionOutcome::Partial(diagnostic) => diagnostic,
            _ => panic!("expected partial extraction"),
        };

        assert_eq!(diagnostic.parser, "toml_edit");
        assert!(diagnostic.fallback_used);
        assert!(diagnostic.line.is_some());
        assert!(diagnostic.column.is_some());
        assert!(diagnostic.byte_span.is_some());
    }

    #[test]
    fn test_edit_capability() {
        assert_eq!(
            TomlExtractor.edit_capability(),
            EditCapability::StructuralEditSafe
        );
    }

    // ---- SF-STRESS-015: unescape must reverse all four escapes ----

    #[test]
    fn test_unescape_key_path_reverses_all_escapes() {
        // The previous sequential-replace form left ~2/~3 (brackets) untouched.
        assert_eq!(unescape_key_path("a~1b"), "a.b");
        assert_eq!(unescape_key_path("a~0b"), "a~b");
        assert_eq!(unescape_key_path("items~20~3"), "items[0]");
        assert_eq!(unescape_key_path("a~1b~2c~3"), "a.b[c]");
    }

    #[test]
    fn test_unescape_key_path_roundtrips_escape_key_segment() {
        for raw in ["plain", "with.dot", "tilde~here", "arr[0]", "mix.[a]~b"] {
            let escaped = super::super::escape_key_segment(raw);
            assert_eq!(
                unescape_key_path(&escaped),
                raw,
                "round-trip failed for {raw:?} (escaped {escaped:?})"
            );
        }
    }

    #[test]
    fn test_unescape_key_path_preserves_non_ascii() {
        // A byte-wise unescaper would corrupt multi-byte UTF-8; the char-wise
        // scan must leave non-ASCII keys intact.
        assert_eq!(unescape_key_path("café~1com"), "café.com");
        assert_eq!(unescape_key_path("日本~0語"), "日本~語");
    }
}
