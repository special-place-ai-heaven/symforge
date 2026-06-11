use super::{
    ConfigExtractor, EditCapability, ExtractionOutcome, ExtractionResult, MAX_ARRAY_ITEMS,
    MAX_DEPTH, build_line_starts, byte_to_line, join_array_index, join_key_path,
};
use crate::domain::{SymbolKind, SymbolRecord};

use super::{optional_u32, parse_diagnostic};

pub struct JsonExtractor;

/// Normalize JSONC (the tsconfig dialect) into strict JSON that `serde_json`
/// accepts, WITHOUT changing any byte offsets or line numbers: comments and
/// trailing commas are blanked to offset-preserving spaces (newlines kept).
///
/// `tsc --init` emits trailing commas by default and both `tsc` and VS Code
/// accept them, so every default-initialized `tsconfig.json` would otherwise
/// land in a Failed/0-symbol state. We blank a trailing comma — a `,` whose
/// next significant byte (skipping whitespace; comments are already blanked in
/// pass 1) is `}` or `]` — exactly the way comments are blanked. This token
/// sequence never occurs in strict JSON outside string literals, so it is safe
/// globally; string contents are respected in both passes.
fn normalize_jsonc(input: &[u8]) -> Vec<u8> {
    let stripped = strip_json_comments(input);
    blank_trailing_commas(&stripped)
}

/// Blank any trailing comma (a `,` followed only by whitespace before `}` or
/// `]`) with an offset-preserving space. Operates on comment-stripped bytes, so
/// the only non-whitespace bytes between a trailing comma and its closer are the
/// closer itself. String literals are respected: a `,`/`}`/`]` inside `"…"` is
/// never treated as structural.
fn blank_trailing_commas(input: &[u8]) -> Vec<u8> {
    let mut out = input.to_vec();
    let len = out.len();
    let mut i = 0;

    while i < len {
        let b = out[i];

        // Skip string literals verbatim (handle escapes) so a comma inside a
        // string is never mistaken for a structural trailing comma.
        if b == b'"' {
            i += 1;
            while i < len {
                let c = out[i];
                i += 1;
                if c == b'"' {
                    break;
                }
                if c == b'\\' && i < len {
                    i += 1; // skip the escaped byte
                }
            }
            continue;
        }

        if b == b',' {
            // Look ahead past whitespace to the next significant byte.
            let mut j = i + 1;
            while j < len && matches!(out[j], b' ' | b'\t' | b'\r' | b'\n') {
                j += 1;
            }
            if j < len && (out[j] == b'}' || out[j] == b']') {
                out[i] = b' ';
            }
        }

        i += 1;
    }

    out
}

/// Strip `//` line comments and `/* … */` block comments from JSON bytes,
/// producing valid JSON that `serde_json` can parse. String literals are
/// respected — comments inside `"…"` are left untouched. Newlines inside
/// block comments are preserved so that line numbers stay accurate.
fn strip_json_comments(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let len = input.len();
    let mut i = 0;

    while i < len {
        let b = input[i];

        // --- string literal: copy verbatim until closing quote ---
        if b == b'"' {
            out.push(b);
            i += 1;
            while i < len {
                let c = input[i];
                out.push(c);
                i += 1;
                if c == b'"' {
                    break;
                }
                if c == b'\\' && i < len {
                    // escaped char — copy next byte unconditionally
                    out.push(input[i]);
                    i += 1;
                }
            }
            continue;
        }

        // --- possible comment start ---
        if b == b'/' && i + 1 < len {
            let next = input[i + 1];

            // line comment: replace with spaces until newline
            if next == b'/' {
                i += 2; // skip "//"
                out.push(b' ');
                out.push(b' ');
                while i < len && input[i] != b'\n' {
                    out.push(b' ');
                    i += 1;
                }
                continue;
            }

            // block comment: replace with spaces, preserve newlines
            if next == b'*' {
                i += 2; // skip "/*"
                out.push(b' ');
                out.push(b' ');
                while i < len {
                    if input[i] == b'*' && i + 1 < len && input[i + 1] == b'/' {
                        out.push(b' ');
                        out.push(b' ');
                        i += 2;
                        break;
                    }
                    if input[i] == b'\n' {
                        out.push(b'\n');
                    } else {
                        out.push(b' ');
                    }
                    i += 1;
                }
                continue;
            }
        }

        // --- ordinary byte ---
        out.push(b);
        i += 1;
    }

    out
}

impl ConfigExtractor for JsonExtractor {
    fn extract(&self, content: &[u8]) -> ExtractionResult {
        let stripped = normalize_jsonc(content);
        let value: serde_json::Value = match serde_json::from_slice(&stripped) {
            Ok(v) => v,
            Err(e) => {
                return ExtractionResult {
                    symbols: vec![],
                    outcome: ExtractionOutcome::Failed(parse_diagnostic(
                        "serde_json",
                        e.to_string(),
                        optional_u32(e.line()),
                        optional_u32(e.column()),
                        None,
                        false,
                    )),
                };
            }
        };

        // Build a line-start offset table for line_range computation.
        let line_starts = build_line_starts(content);

        let mut symbols = Vec::new();
        let mut sort_order: u32 = 0;
        let mut walker = JsonWalker {
            content,
            line_starts: &line_starts,
            symbols: &mut symbols,
            sort_order: &mut sort_order,
        };

        // Only walk into the root if it is an object or array.
        match &value {
            serde_json::Value::Object(map) => {
                walker.walk_object(map, "", 0);
            }
            serde_json::Value::Array(arr) => {
                walker.walk_array(arr, "", (0, content.len() as u32), 0);
            }
            _ => {}
        }

        ExtractionResult {
            symbols,
            outcome: ExtractionOutcome::Ok,
        }
    }

    fn edit_capability(&self) -> EditCapability {
        EditCapability::TextEditSafe
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct JsonWalker<'a> {
    content: &'a [u8],
    line_starts: &'a [u32],
    symbols: &'a mut Vec<SymbolRecord>,
    sort_order: &'a mut u32,
}

impl JsonWalker<'_> {
    fn push_key_symbol(&mut self, name: String, depth: u32, byte_range: (u32, u32)) {
        let start_line = byte_to_line(self.line_starts, byte_range.0);
        let end_line = byte_to_line(
            self.line_starts,
            byte_range.1.saturating_sub(1).max(byte_range.0),
        );

        self.symbols.push(SymbolRecord {
            name,
            kind: SymbolKind::Key,
            depth,
            sort_order: *self.sort_order,
            byte_range,
            line_range: (start_line, end_line),
            doc_byte_range: None,
            item_byte_range: Some(byte_range),
        });
        *self.sort_order += 1;
    }

    fn walk_object(
        &mut self,
        map: &serde_json::Map<String, serde_json::Value>,
        parent_path: &str,
        depth: u32,
    ) {
        let mut search_from: usize = 0;

        for (key, value) in map.iter() {
            let key_path = join_key_path(parent_path, key);
            let (byte_start, byte_end) = find_key_value_range(self.content, key, &mut search_from);
            let byte_range = (byte_start as u32, byte_end as u32);
            self.push_key_symbol(key_path.clone(), depth, byte_range);

            if depth + 1 < MAX_DEPTH {
                match value {
                    serde_json::Value::Object(child_map) => {
                        self.walk_object(child_map, &key_path, depth + 1);
                    }
                    serde_json::Value::Array(child_arr) => {
                        self.walk_array(child_arr, &key_path, byte_range, depth + 1);
                    }
                    _ => {}
                }
            }
        }
    }

    fn walk_array(
        &mut self,
        arr: &[serde_json::Value],
        parent_path: &str,
        parent_byte_range: (u32, u32),
        depth: u32,
    ) {
        let item_ranges = find_array_item_ranges(self.content, parent_byte_range);
        for (i, value) in arr.iter().enumerate() {
            if i >= MAX_ARRAY_ITEMS {
                break;
            }

            let elem_path = join_array_index(parent_path, i);
            let byte_range = item_ranges.get(i).copied().unwrap_or(parent_byte_range);
            self.push_key_symbol(elem_path.clone(), depth, byte_range);

            if depth + 1 < MAX_DEPTH {
                match value {
                    serde_json::Value::Object(child_map) => {
                        self.walk_object(child_map, &elem_path, depth + 1);
                    }
                    serde_json::Value::Array(child_arr) => {
                        self.walk_array(child_arr, &elem_path, byte_range, depth + 1);
                    }
                    _ => {}
                }
            }
        }
    }
}

fn find_array_item_ranges(content: &[u8], parent_byte_range: (u32, u32)) -> Vec<(u32, u32)> {
    let start = parent_byte_range.0 as usize;
    let end = (parent_byte_range.1 as usize).min(content.len());
    if start >= end {
        return Vec::new();
    }

    let Some(value_start) = find_value_start_in_range(content, start, end) else {
        return Vec::new();
    };
    if value_start >= end || content[value_start] != b'[' {
        return Vec::new();
    }

    let array_end = scan_container_end(content, value_start).min(end);
    let mut cursor = value_start + 1;
    let mut ranges = Vec::new();
    while cursor < array_end {
        cursor = skip_whitespace(content, cursor);
        if cursor >= array_end || content[cursor] == b']' {
            break;
        }

        let item_start = cursor;
        let item_end = scan_value_end(content, item_start).min(array_end);
        ranges.push((item_start as u32, item_end as u32));

        cursor = skip_whitespace(content, item_end);
        if cursor < array_end && content[cursor] == b',' {
            cursor += 1;
        }
    }

    ranges
}

fn find_value_start_in_range(content: &[u8], start: usize, end: usize) -> Option<usize> {
    if start >= end || start >= content.len() {
        return None;
    }

    let mut i = start;
    let mut in_string = false;
    let mut escaped = false;
    while i < end {
        let byte = content[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b':' {
            return Some(skip_whitespace(content, i + 1).min(end));
        }
        i += 1;
    }

    Some(skip_whitespace(content, start).min(end))
}

/// Search the raw bytes for `"key":` starting from `*search_from`, returning
/// the byte range covering the key and its associated value.
///
/// The start is the opening `"` of the key. The end is determined by scanning
/// past the value (tracking braces, brackets, and strings).
fn find_key_value_range(content: &[u8], key: &str, search_from: &mut usize) -> (usize, usize) {
    // Build the needle: `"key"` (we search for the quoted key).
    // Escape backslashes and double-quotes within the key so that keys like
    // `a"b` or `a\b` match their JSON-encoded form (`"a\"b"`, `"a\\b"`).
    let escaped_key = key.replace('\\', "\\\\").replace('"', "\\\"");
    let needle = format!("\"{}\"", escaped_key);
    let needle_bytes = needle.as_bytes();

    // Search forward from the current cursor.
    let hay = &content[*search_from..];
    if let Some(rel_pos) = find_substring(hay, needle_bytes) {
        let abs_key_start = *search_from + rel_pos;

        // Find the colon after the key.
        let after_key = abs_key_start + needle_bytes.len();
        let colon_pos = match content[after_key..].iter().position(|&b| b == b':') {
            Some(p) => after_key + p,
            None => {
                // Fallback: return just the key span.
                let end = abs_key_start + needle_bytes.len();
                *search_from = end;
                return (abs_key_start, end);
            }
        };

        // Skip whitespace after the colon to find the value start.
        let value_start = skip_whitespace(content, colon_pos + 1);

        // Determine the value end.
        let value_end = scan_value_end(content, value_start);

        *search_from = value_end;
        (abs_key_start, value_end)
    } else {
        // Key not found (shouldn't happen for valid JSON). Return file bounds.
        (0, content.len())
    }
}

/// Find the first occurrence of `needle` in `haystack` (simple byte search).
fn find_substring(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Skip ASCII whitespace bytes, returning the index of the first non-WS byte.
fn skip_whitespace(content: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < content.len() && matches!(content[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

/// Scan past a single JSON value (string, number, bool, null, object, array),
/// returning the byte position just past the value.
fn scan_value_end(content: &[u8], start: usize) -> usize {
    if start >= content.len() {
        return content.len();
    }

    match content[start] {
        b'"' => scan_string_end(content, start),
        b'{' | b'[' => scan_container_end(content, start),
        _ => {
            // Primitive: number, bool, null — ends at comma, `}`, `]`, or whitespace.
            let mut i = start;
            while i < content.len()
                && !matches!(
                    content[i],
                    b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n'
                )
            {
                i += 1;
            }
            i
        }
    }
}

/// Scan past a JSON string (handling escape sequences).
fn scan_string_end(content: &[u8], start: usize) -> usize {
    // start points at the opening `"`.
    let mut i = start + 1;
    while i < content.len() {
        if content[i] == b'\\' {
            i += 2; // skip escaped char
        } else if content[i] == b'"' {
            return i + 1; // past the closing quote
        } else {
            i += 1;
        }
    }
    content.len()
}

/// Scan past a JSON object `{…}` or array `[…]`, tracking nesting and strings.
fn scan_container_end(content: &[u8], start: usize) -> usize {
    let open = content[start];
    let close = if open == b'{' { b'}' } else { b']' };

    let mut depth: u32 = 0;
    let mut i = start;
    while i < content.len() {
        match content[i] {
            b'"' => {
                // Skip string contents.
                i = scan_string_end(content, i);
                continue;
            }
            b if b == open => depth += 1,
            b if b == close => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    content.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_level_keys() {
        let content = br#"{"name": "test", "version": "1.0"}"#;
        let result = JsonExtractor.extract(content);
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.name == "name" && s.kind == SymbolKind::Key)
        );
        assert!(result.symbols.iter().any(|s| s.name == "version"));
    }

    #[test]
    fn test_nested_keys() {
        let content = br#"{"scripts": {"test": "jest", "build": "tsc"}}"#;
        let result = JsonExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "scripts"));
        assert!(result.symbols.iter().any(|s| s.name == "scripts.test"));
        assert!(result.symbols.iter().any(|s| s.name == "scripts.build"));
    }

    #[test]
    fn test_array_indexing() {
        let content = br#"{"items": ["a", "b", "c"]}"#;
        let result = JsonExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "items[0]"));
        assert!(result.symbols.iter().any(|s| s.name == "items[2]"));
    }

    #[test]
    fn test_depth_limit() {
        let content = br#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":"deep"}}}}}}}"#;
        let result = JsonExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "a.b.c.d.e.f"));
        assert!(!result.symbols.iter().any(|s| s.name == "a.b.c.d.e.f.g"));
    }

    #[test]
    fn test_array_cap() {
        let items: Vec<String> = (0..25).map(|i| format!("{i}")).collect();
        let content = format!(r#"{{"arr": [{}]}}"#, items.join(","));
        let result = JsonExtractor.extract(content.as_bytes());
        let arr_items: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.name.starts_with("arr["))
            .collect();
        assert_eq!(arr_items.len(), 20);
    }

    #[test]
    fn test_literal_dot_key_escaped() {
        let content = br#"{"a.b": "value"}"#;
        let result = JsonExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "a~1b"));
    }

    #[test]
    fn test_literal_bracket_key_escaped() {
        let content = br#"{"items[0]": "literal"}"#;
        let result = JsonExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "items~20~3"));
    }

    #[test]
    fn test_empty_object() {
        let result = JsonExtractor.extract(b"{}");
        assert!(result.symbols.is_empty());
    }

    #[test]
    fn test_malformed_json() {
        let result = JsonExtractor.extract(b"{invalid json");
        assert!(result.symbols.is_empty());
        assert!(matches!(result.outcome, ExtractionOutcome::Failed(_)));
    }

    #[test]
    fn test_byte_range_within_bounds() {
        let content = b"{\n  \"name\": \"test\",\n  \"version\": \"1.0\"\n}";
        let result = JsonExtractor.extract(content);
        for sym in &result.symbols {
            assert!(
                sym.byte_range.1 <= content.len() as u32,
                "symbol {} byte_range end {} exceeds file length {}",
                sym.name,
                sym.byte_range.1,
                content.len()
            );
        }
    }

    #[test]
    fn test_array_items_get_precise_byte_ranges() {
        let content = br#"{"items": [1, {"nested": true}, 3]}"#;
        let result = JsonExtractor.extract(content);
        assert!(matches!(result.outcome, ExtractionOutcome::Ok));

        let first = result
            .symbols
            .iter()
            .find(|sym| sym.name == "items[0]")
            .expect("first array item");
        let second = result
            .symbols
            .iter()
            .find(|sym| sym.name == "items[1]")
            .expect("second array item");
        let third = result
            .symbols
            .iter()
            .find(|sym| sym.name == "items[2]")
            .expect("third array item");

        assert_eq!(
            &content[first.byte_range.0 as usize..first.byte_range.1 as usize],
            b"1"
        );
        assert_eq!(
            &content[second.byte_range.0 as usize..second.byte_range.1 as usize],
            br#"{"nested": true}"#
        );
        assert_eq!(
            &content[third.byte_range.0 as usize..third.byte_range.1 as usize],
            b"3"
        );
        assert_ne!(first.byte_range, second.byte_range);
        assert_ne!(second.byte_range, third.byte_range);
    }

    #[test]
    fn test_edit_capability() {
        assert_eq!(
            JsonExtractor.edit_capability(),
            EditCapability::TextEditSafe
        );
    }

    #[test]
    fn test_jsonc_line_comments() {
        let content = b"{\n  // This is a comment\n  \"name\": \"test\"\n}";
        let result = JsonExtractor.extract(content);
        assert!(
            matches!(result.outcome, ExtractionOutcome::Ok),
            "JSONC with line comments should parse OK"
        );
        assert!(result.symbols.iter().any(|s| s.name == "name"));
    }

    #[test]
    fn test_jsonc_block_comments() {
        let content = b"{\n  /* block comment */\n  \"name\": \"test\"\n}";
        let result = JsonExtractor.extract(content);
        assert!(
            matches!(result.outcome, ExtractionOutcome::Ok),
            "JSONC with block comments should parse OK"
        );
        assert!(result.symbols.iter().any(|s| s.name == "name"));
    }

    #[test]
    fn test_jsonc_trailing_commas_now_parse() {
        // SF-STRESS-016: `tsc --init` emits trailing commas by default, so the
        // JSONC normalizer must blank them and parse the keys out.
        let content = br#"{"a": 1,}"#;
        let result = JsonExtractor.extract(content);
        assert!(
            matches!(result.outcome, ExtractionOutcome::Ok),
            "Trailing commas should now parse (JSONC tolerance)"
        );
        assert!(result.symbols.iter().any(|s| s.name == "a"));
    }

    #[test]
    fn test_jsonc_trailing_comma_in_array() {
        let content = br#"{"items": [1, 2, 3,]}"#;
        let result = JsonExtractor.extract(content);
        assert!(
            matches!(result.outcome, ExtractionOutcome::Ok),
            "Trailing comma in an array should parse"
        );
        assert!(result.symbols.iter().any(|s| s.name == "items[2]"));
    }

    #[test]
    fn test_jsonc_trailing_comma_inside_string_preserved() {
        // A comma before a `}`/`]` that lives INSIDE a string is NOT a trailing
        // comma and must be left untouched (the value stays intact).
        let content = br#"{"pattern": "a,}", "next": 1}"#;
        let result = JsonExtractor.extract(content);
        assert!(
            matches!(result.outcome, ExtractionOutcome::Ok),
            "string content with a comma-before-brace must parse"
        );
        assert!(result.symbols.iter().any(|s| s.name == "pattern"));
        assert!(result.symbols.iter().any(|s| s.name == "next"));
    }

    #[test]
    fn test_tsconfig_jsonc_with_comments_and_trailing_commas() {
        // A representative `tsc --init`-style tsconfig.json: line comments,
        // block comment, AND trailing commas in both an object and an array.
        let content = br#"{
  // Visit https://aka.ms/tsconfig to read more about this file
  "compilerOptions": {
    "target": "es2016", /* the output target */
    "module": "commonjs",
    "strict": true,
  },
  "include": [
    "src/**/*",
    "tests/**/*",
  ],
}"#;
        let result = JsonExtractor.extract(content);
        assert!(
            matches!(result.outcome, ExtractionOutcome::Ok),
            "a default-initialized tsconfig.json must parse"
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.name == "compilerOptions.target"),
            "nested keys are extracted from JSONC tsconfig"
        );
        assert!(result.symbols.iter().any(|s| s.name == "include[1]"));
    }

    #[test]
    fn test_malformed_json_still_fails_after_jsonc_normalize() {
        // The JSONC tolerance must NOT mask genuinely malformed JSON — an honest
        // degraded state is still required for non-dialect breakage.
        let result = JsonExtractor.extract(b"{\"a\": }");
        assert!(
            matches!(result.outcome, ExtractionOutcome::Failed(_)),
            "a missing value is genuine malformation and must still Fail"
        );
    }

    #[test]
    fn test_jsonc_comments_inside_strings_preserved() {
        let content = br#"{"url": "https://example.com", "pattern": "// not a comment"}"#;
        let result = JsonExtractor.extract(content);
        assert!(
            matches!(result.outcome, ExtractionOutcome::Ok),
            "Comments inside strings should not be stripped"
        );
        assert!(result.symbols.iter().any(|s| s.name == "url"));
        assert!(result.symbols.iter().any(|s| s.name == "pattern"));
    }
}
