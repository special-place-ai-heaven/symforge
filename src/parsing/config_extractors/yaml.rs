use super::{
    ConfigExtractor, EditCapability, ExtractionOutcome, ExtractionResult, MAX_ARRAY_ITEMS,
    MAX_DEPTH, build_line_starts, byte_to_line, join_array_index, join_key_path,
};
use crate::domain::{SymbolKind, SymbolRecord};

use super::{optional_u32, parse_diagnostic};

pub struct YamlExtractor;

impl ConfigExtractor for YamlExtractor {
    fn extract(&self, content: &[u8]) -> ExtractionResult {
        if content.is_empty() {
            return ExtractionResult {
                symbols: vec![],
                outcome: ExtractionOutcome::Ok,
            };
        }

        let value: serde_yml::Value = match serde_yml::from_slice(content) {
            Ok(v) => v,
            Err(e) => {
                let location = e.location();
                return ExtractionResult {
                    symbols: vec![],
                    outcome: ExtractionOutcome::Failed(parse_diagnostic(
                        "serde_yml",
                        e.to_string(),
                        location.as_ref().and_then(|loc| optional_u32(loc.line())),
                        location.as_ref().and_then(|loc| optional_u32(loc.column())),
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
        let mut walker = YamlWalker {
            content,
            line_starts: &line_starts,
            symbols: &mut symbols,
            sort_order: &mut sort_order,
        };

        match &value {
            serde_yml::Value::Mapping(map) => {
                walker.walk_mapping(map, "", (0, content.len() as u32), 0);
            }
            serde_yml::Value::Sequence(seq) => {
                walker.walk_sequence(seq, "", (0, content.len() as u32), 0);
            }
            _ => {}
        }

        super::ci_yaml::append_ci_yaml_facts(
            content,
            &line_starts,
            &value,
            &mut symbols,
            &mut sort_order,
        );

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

/// Find the byte range in `content` for the YAML mapping key `key`.
///
/// Searches for `key:` pattern inside `search_end` starting from `search_from`,
/// requiring the match to appear at the start of a mapping line. Sequence-item
/// inline keys such as `- uses:` are accepted.
/// The range extends from the key start to just before the next sibling key
/// at the same or lesser indentation, or to end of content.
fn find_yaml_key_range(
    content: &[u8],
    key: &str,
    search_from: &mut usize,
    search_end: usize,
) -> (usize, usize) {
    let needle = format!("{}:", key);
    let needle_bytes = needle.as_bytes();

    let mut pos = *search_from;
    let bounded_end = search_end.min(content.len());
    while pos + needle_bytes.len() <= bounded_end {
        if let Some(rel) = find_substring(&content[pos..bounded_end], needle_bytes) {
            let abs_start = pos + rel;

            let line_start = content[..abs_start]
                .iter()
                .rposition(|&b| b == b'\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let line_end = content[abs_start..bounded_end]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bounded_end, |offset| abs_start + offset + 1);

            let prefix = &content[line_start..abs_start];
            if let Some((indent, inline_sequence_key)) = yaml_key_prefix(prefix) {
                let key_end = abs_start + needle_bytes.len();
                let range_end = if inline_sequence_key {
                    line_end
                } else {
                    find_block_end(content, key_end, indent).min(bounded_end)
                };

                *search_from = key_end;
                return (abs_start, range_end);
            }

            pos = abs_start + 1;
        } else {
            break;
        }
    }

    (*search_from, bounded_end)
}

fn yaml_key_prefix(prefix: &[u8]) -> Option<(usize, bool)> {
    let indent = prefix
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    let rest = &prefix[indent..];
    if rest.is_empty() {
        return Some((indent, false));
    }
    if rest[0] == b'-' && rest[1..].iter().all(|&b| b == b' ' || b == b'\t') {
        return Some((indent, true));
    }
    None
}

/// Given that a key was found ending at `after_key`, scan forward line-by-line
/// to find where its block ends — i.e., the start of the next line at
/// indentation <= `key_indent` that contains non-whitespace, non-comment content.
fn find_block_end(content: &[u8], after_key: usize, key_indent: usize) -> usize {
    let mut i = after_key;

    // Skip to end of the current key line.
    while i < content.len() && content[i] != b'\n' {
        i += 1;
    }
    if i < content.len() {
        i += 1; // consume the newline
    }

    while i < content.len() {
        let line_start = i;
        let mut indent = 0usize;
        while i < content.len() && (content[i] == b' ' || content[i] == b'\t') {
            indent += 1;
            i += 1;
        }

        if i >= content.len() {
            return content.len();
        }

        let ch = content[i];

        // Blank lines — continue scanning.
        if ch == b'\n' || ch == b'\r' {
            i += 1;
            continue;
        }

        // Comment lines — continue scanning.
        if ch == b'#' {
            while i < content.len() && content[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // If this line's indentation is <= the key's indentation,
        // this is a sibling or parent — the block ends here.
        if indent <= key_indent {
            return line_start;
        }

        // Child line — skip it.
        while i < content.len() && content[i] != b'\n' {
            i += 1;
        }
        if i < content.len() {
            i += 1;
        }
    }

    content.len()
}

/// Find the first occurrence of `needle` in `haystack`.
fn find_substring(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

struct YamlWalker<'a> {
    content: &'a [u8],
    line_starts: &'a [u32],
    symbols: &'a mut Vec<SymbolRecord>,
    sort_order: &'a mut u32,
}

impl YamlWalker<'_> {
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
            item_byte_range: Some(byte_range),
            line_range: (start_line, end_line),
            doc_byte_range: None,
        });
        *self.sort_order += 1;
    }

    fn walk_mapping(
        &mut self,
        map: &serde_yml::Mapping,
        parent_path: &str,
        parent_byte_range: (u32, u32),
        depth: u32,
    ) {
        let mut search_from = parent_byte_range.0 as usize;
        let search_end = parent_byte_range.1 as usize;

        for (k, v) in map.iter() {
            // serde_yml 0.0.13's `Mapping` keys are `String` (noyalib backend),
            // so the key is already a plain string and needs no scalar coercion.
            let key_str = k.clone();

            let key_path = join_key_path(parent_path, &key_str);
            let (byte_start, byte_end) =
                find_yaml_key_range(self.content, &key_str, &mut search_from, search_end);
            let byte_range = (byte_start as u32, byte_end as u32);
            self.push_key_symbol(key_path.clone(), depth, byte_range);

            if depth + 1 < MAX_DEPTH {
                match v {
                    serde_yml::Value::Mapping(child_map) => {
                        self.walk_mapping(child_map, &key_path, byte_range, depth + 1);
                    }
                    serde_yml::Value::Sequence(child_seq) => {
                        self.walk_sequence(child_seq, &key_path, byte_range, depth + 1);
                    }
                    _ => {}
                }
            }
        }
    }

    fn walk_sequence(
        &mut self,
        seq: &[serde_yml::Value],
        parent_path: &str,
        parent_byte_range: (u32, u32),
        depth: u32,
    ) {
        let item_ranges = find_sequence_item_ranges(self.content, parent_byte_range);
        for (i, v) in seq.iter().enumerate() {
            if i >= MAX_ARRAY_ITEMS {
                break;
            }

            let elem_path = join_array_index(parent_path, i);
            let byte_range = item_ranges.get(i).copied().unwrap_or(parent_byte_range);
            self.push_key_symbol(elem_path.clone(), depth, byte_range);

            if depth + 1 < MAX_DEPTH {
                match v {
                    serde_yml::Value::Mapping(child_map) => {
                        self.walk_mapping(child_map, &elem_path, byte_range, depth + 1);
                    }
                    serde_yml::Value::Sequence(child_seq) => {
                        self.walk_sequence(child_seq, &elem_path, byte_range, depth + 1);
                    }
                    _ => {}
                }
            }
        }
    }
}

fn find_sequence_item_ranges(content: &[u8], parent_byte_range: (u32, u32)) -> Vec<(u32, u32)> {
    let start = parent_byte_range.0 as usize;
    let end = (parent_byte_range.1 as usize).min(content.len());
    if start >= end {
        return Vec::new();
    }

    let mut cursor = start;
    let mut item_indent: Option<usize> = None;
    let mut current_start: Option<usize> = None;
    let mut ranges = Vec::new();
    while cursor < end {
        let line_start = cursor;
        let line_end = content[cursor..end]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(end, |offset| cursor + offset + 1);
        cursor = line_end;

        let line = &content[line_start..line_end];
        let indent = line
            .iter()
            .take_while(|&&b| b == b' ' || b == b'\t')
            .count();
        let rest = &line[indent..];
        if rest.is_empty() || matches!(rest[0], b'#' | b'\r' | b'\n') {
            continue;
        }

        let is_item = rest[0] == b'-'
            && rest
                .get(1)
                .is_some_and(|next| matches!(next, b' ' | b'\t' | b'\r' | b'\n'));
        if !is_item {
            continue;
        }

        match item_indent {
            None => {
                item_indent = Some(indent);
                current_start = Some(line_start);
            }
            Some(existing_indent) if indent == existing_indent => {
                if let Some(previous_start) = current_start.replace(line_start) {
                    ranges.push((previous_start as u32, line_start as u32));
                }
            }
            _ => {}
        }
    }

    if let Some(previous_start) = current_start {
        ranges.push((previous_start as u32, end as u32));
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_mapping() {
        let content = b"name: test\nversion: 1.0\n";
        let result = YamlExtractor.extract(content);
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.name == "name" && s.kind == SymbolKind::Key)
        );
        assert!(result.symbols.iter().any(|s| s.name == "version"));
    }

    #[test]
    fn test_nested_mapping() {
        let content = b"server:\n  host: localhost\n  port: 8080\n";
        let result = YamlExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "server"));
        assert!(result.symbols.iter().any(|s| s.name == "server.host"));
        assert!(result.symbols.iter().any(|s| s.name == "server.port"));
    }

    #[test]
    fn test_sequence() {
        let content = b"items:\n  - a\n  - b\n";
        let result = YamlExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "items[0]"));
        assert!(result.symbols.iter().any(|s| s.name == "items[1]"));
    }

    #[test]
    fn test_empty_file() {
        assert!(YamlExtractor.extract(b"").symbols.is_empty());
    }

    #[test]
    fn test_malformed_yaml() {
        let result = YamlExtractor.extract(b":\n  :\n  - [invalid");
        assert!(result.symbols.is_empty());
        assert!(matches!(result.outcome, ExtractionOutcome::Failed(_)));
    }

    #[test]
    fn test_edit_capability() {
        assert_eq!(
            YamlExtractor.edit_capability(),
            EditCapability::TextEditSafe
        );
    }

    #[test]
    fn test_depth_limit() {
        let content =
            b"a:\n  b:\n    c:\n      d:\n        e:\n          f:\n            g: deep\n";
        let result = YamlExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "a.b.c.d.e.f"));
        assert!(!result.symbols.iter().any(|s| s.name == "a.b.c.d.e.f.g"));
    }

    #[test]
    fn test_array_cap() {
        let mut content = String::from("arr:\n");
        for i in 0..25 {
            content.push_str(&format!("  - {}\n", i));
        }
        let result = YamlExtractor.extract(content.as_bytes());
        let arr_items: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.name.starts_with("arr["))
            .collect();
        assert_eq!(arr_items.len(), 20);
    }

    #[test]
    fn test_byte_range_within_bounds() {
        let content = b"name: test\nversion: 1.0\n";
        let result = YamlExtractor.extract(content);
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
    fn test_sequence_items_get_precise_byte_ranges() {
        let content = b"items:\n  - alpha\n  - nested:\n      ok: true\n  - gamma\n";
        let result = YamlExtractor.extract(content);
        assert!(matches!(result.outcome, ExtractionOutcome::Ok));

        let first = result
            .symbols
            .iter()
            .find(|sym| sym.name == "items[0]")
            .expect("first sequence item");
        let second = result
            .symbols
            .iter()
            .find(|sym| sym.name == "items[1]")
            .expect("second sequence item");
        let third = result
            .symbols
            .iter()
            .find(|sym| sym.name == "items[2]")
            .expect("third sequence item");

        let first_text =
            std::str::from_utf8(&content[first.byte_range.0 as usize..first.byte_range.1 as usize])
                .unwrap();
        let second_text = std::str::from_utf8(
            &content[second.byte_range.0 as usize..second.byte_range.1 as usize],
        )
        .unwrap();
        let third_text =
            std::str::from_utf8(&content[third.byte_range.0 as usize..third.byte_range.1 as usize])
                .unwrap();

        assert!(first_text.trim_end().ends_with("- alpha"));
        assert!(second_text.contains("- nested:"));
        assert!(second_text.contains("ok: true"));
        assert!(third_text.trim_end().ends_with("- gamma"));
        assert_ne!(first.byte_range, second.byte_range);
        assert_ne!(second.byte_range, third.byte_range);
    }
}
