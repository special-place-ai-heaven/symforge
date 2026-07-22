use super::{ConfigExtractor, EditCapability, ExtractionOutcome, ExtractionResult};
use crate::domain::{SymbolKind, SymbolRecord};
use std::collections::HashMap;

use super::parse_diagnostic;

/// Detects a CommonMark fenced-code-block delimiter line.
///
/// Returns `Some((fence_char, run_length, info_empty))` when `line`, after up to
/// three leading spaces, begins with a run of at least three identical backticks
/// (`` ` ``) or tildes (`~`). `info_empty` is true when only whitespace follows
/// the run — a closing fence carries no info string, so this distinguishes a
/// valid closer from an opener like ```` ```ruby ````. Returns `None` otherwise.
fn parse_code_fence(line: &str) -> Option<(u8, usize, bool)> {
    // Up to three leading spaces are allowed before a fence; four or more make
    // the line an indented code block, which is never a fence delimiter.
    let leading_spaces = line.bytes().take_while(|&b| b == b' ').count();
    if leading_spaces > 3 {
        return None;
    }
    let trimmed = &line[leading_spaces..];

    let fence_char = match trimmed.bytes().next()? {
        b'`' => b'`',
        b'~' => b'~',
        _ => return None,
    };
    let run_length = trimmed.bytes().take_while(|&b| b == fence_char).count();
    if run_length < 3 {
        return None;
    }
    let info = &trimmed[run_length..];
    let info_empty = info.trim().is_empty();
    Some((fence_char, run_length, info_empty))
}

pub struct MarkdownExtractor;

impl ConfigExtractor for MarkdownExtractor {
    fn extract(&self, content: &[u8]) -> ExtractionResult {
        let decoded = match crate::knowledge::decode_searchable_text(content) {
            Ok(decoded) => decoded,
            Err(_) => {
                return ExtractionResult {
                    symbols: vec![],
                    outcome: ExtractionOutcome::Failed(parse_diagnostic(
                        "utf-8",
                        "Invalid UTF-8",
                        None,
                        None,
                        None,
                        false,
                    )),
                };
            }
        };
        let text = decoded.text;

        if text.is_empty() {
            return ExtractionResult {
                symbols: vec![],
                outcome: ExtractionOutcome::Ok,
            };
        }

        // Collect (byte_offset, line_text) pairs, skipping YAML frontmatter.
        let mut lines: Vec<(usize, &str)> = Vec::new();
        {
            let raw: Vec<&str> = text.split('\n').collect();
            let mut i = 0usize;
            let mut byte_offset = decoded.leading_bytes as usize;
            fn clean_line(line: &str) -> &str {
                line.strip_suffix('\r').unwrap_or(line)
            }

            // Check for frontmatter
            if raw.first().copied().map(clean_line) == Some("---") {
                byte_offset += raw[0].len() + 1; // skip opening ---\n
                i = 1;
                let mut closed = false;
                while i < raw.len() {
                    let line_bytes = raw[i].len() + 1;
                    if clean_line(raw[i]) == "---" {
                        byte_offset += line_bytes;
                        i += 1;
                        closed = true;
                        break;
                    }
                    byte_offset += line_bytes;
                    i += 1;
                }
                if !closed {
                    return ExtractionResult {
                        symbols: vec![],
                        outcome: ExtractionOutcome::Ok,
                    };
                }
            }

            while i < raw.len() {
                lines.push((byte_offset, clean_line(raw[i])));
                byte_offset += raw[i].len();
                // Only add 1 for the newline delimiter if this isn't the last
                // segment or the original text actually ends with a newline.
                if i + 1 < raw.len() || text.ends_with('\n') {
                    byte_offset += 1;
                }
                i += 1;
            }
        }

        // Parse ATX and Setext headers from collected lines.
        struct HeaderInfo {
            level: u32,
            text: String,
            byte_start: usize,
            line_index: usize,
        }

        let mut headers: Vec<HeaderInfo> = Vec::new();
        // Track fenced code blocks so that '#' comment lines inside ``` or ~~~
        // fences (e.g. shell/ruby comments) are not mistaken for ATX headings,
        // per CommonMark. Holds (fence_char, opener_run_length) while open.
        let mut in_fence: Option<(u8, usize)> = None;
        for (li, &(byte_off, line)) in lines.iter().enumerate() {
            if let Some(fence) = parse_code_fence(line) {
                let (fence_char, fence_len, info_empty) = fence;
                match in_fence {
                    // Opening fence: remember its char and run length. An info
                    // string (e.g. ```ruby) is allowed on the opener.
                    None => in_fence = Some((fence_char, fence_len)),
                    // Closing fence: same char, run >= opener, and nothing but
                    // whitespace after the run (a closing fence has no info).
                    Some((open_char, open_len))
                        if fence_char == open_char && fence_len >= open_len && info_empty =>
                    {
                        in_fence = None;
                    }
                    // Otherwise the line stays inside the current fence as code.
                    Some(_) => {}
                }
                continue;
            }

            // Lines inside a fence are code, never headings.
            if in_fence.is_some() {
                continue;
            }

            let leading_spaces = line.bytes().take_while(|&byte| byte == b' ').count();
            let trimmed = &line[leading_spaces.min(line.len())..];

            if leading_spaces <= 3 && trimmed.starts_with('#') {
                let hashes = trimmed.bytes().take_while(|&byte| byte == b'#').count();
                if hashes <= 6 {
                    let rest = &trimmed[hashes..];
                    if rest.is_empty()
                        || rest
                            .bytes()
                            .next()
                            .is_some_and(|byte| byte == b' ' || byte == b'\t')
                    {
                        let mut title = rest.trim().to_string();
                        let closing_hashes = title.bytes().rev().take_while(|&b| b == b'#').count();
                        if closing_hashes > 0 {
                            let before = title.len() - closing_hashes;
                            if before == 0
                                || title.as_bytes()[before - 1].is_ascii_whitespace()
                            {
                                title.truncate(before);
                                title = title.trim_end().to_string();
                            }
                        }
                        headers.push(HeaderInfo {
                            level: hashes as u32,
                            text: title,
                            byte_start: byte_off,
                            line_index: li,
                        });
                        continue;
                    }
                }
            }

            let underline = trimmed.trim_end();
            let setext_level = if leading_spaces <= 3
                && !underline.is_empty()
                && underline.bytes().all(|byte| byte == b'=')
            {
                Some(1)
            } else if leading_spaces <= 3
                && !underline.is_empty()
                && underline.bytes().all(|byte| byte == b'-')
            {
                Some(2)
            } else {
                None
            };
            if let Some(level) = setext_level
                && li > 0
            {
                let (title_byte_start, title_line) = lines[li - 1];
                let title = title_line.trim();
                let prior_is_heading = headers.iter().any(|header| header.line_index == li - 1);
                if !title.is_empty()
                    && !prior_is_heading
                    && parse_code_fence(title_line).is_none()
                    && !title.starts_with('#')
                {
                    headers.push(HeaderInfo {
                        level,
                        text: title.to_string(),
                        byte_start: title_byte_start,
                        line_index: li - 1,
                    });
                }
            }
        }

        if headers.is_empty() {
            return ExtractionResult {
                symbols: vec![],
                outcome: ExtractionOutcome::Ok,
            };
        }

        let total_bytes = content.len() as u32;

        // Build symbols. Stack holds (level, segment) for path building.
        let mut stack: Vec<(u32, String)> = Vec::new();
        // Duplicate counter keyed by base path (before disambiguation suffix).
        let mut seen_paths: HashMap<String, u32> = HashMap::new();
        let mut symbols: Vec<SymbolRecord> = Vec::new();

        for (hi, header) in headers.iter().enumerate() {
            let level = header.level;

            // Pop entries at same or deeper level.
            while stack.last().is_some_and(|&(l, _)| l >= level) {
                stack.pop();
            }

            // Build dot-joined path from the raw heading text. Section names
            // are display-facing and looked up by exact full-name match only —
            // nothing splits them back into segments (unlike TOML key paths),
            // so `.`/`[`/`]` in a heading stay human-readable instead of being
            // escaped to `~1`/`~2`/`~3`. A heading like "A.B" colliding with a
            // nested A > B path falls into the existing #N duplicate handling.
            let segment = header.text.clone();
            let base_path = if stack.is_empty() {
                segment.clone()
            } else {
                let parent: String = stack
                    .iter()
                    .map(|(_, n)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                format!("{}.{}", parent, segment)
            };

            // Disambiguate duplicates.
            let count = seen_paths.entry(base_path.clone()).or_insert(0);
            *count += 1;
            let name = if *count == 1 {
                base_path.clone()
            } else {
                format!("{}#{}", base_path, count)
            };

            stack.push((level, segment));

            // Byte range: this header's start -> byte before next header at same or higher level (or EOF).
            let byte_end: u32 = headers[hi + 1..]
                .iter()
                .find(|h| h.level <= level)
                .map_or(total_bytes, |h| h.byte_start as u32);
            let byte_range = (header.byte_start as u32, byte_end);

            // Line range: same logic using line indices.
            let line_start = header.line_index as u32;
            let line_end: u32 = headers[hi + 1..]
                .iter()
                .find(|h| h.level <= level)
                .map_or(lines.len() as u32, |h| h.line_index as u32);

            symbols.push(SymbolRecord {
                name,
                kind: SymbolKind::Section,
                depth: level - 1,
                sort_order: hi as u32,
                byte_range,
                item_byte_range: Some(byte_range),
                line_range: (line_start, line_end),
                doc_byte_range: None,
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(content: &[u8]) -> Vec<SymbolRecord> {
        MarkdownExtractor.extract(content).symbols
    }

    #[test]
    fn test_single_header() {
        let syms = extract(b"# Title\nSome text\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Title");
        assert_eq!(syms[0].kind, SymbolKind::Section);
        assert_eq!(syms[0].depth, 0);
    }

    #[test]
    fn test_nested_headers() {
        let syms = extract(b"# Top\n## Sub\n### Deep\n");
        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "Top");
        assert_eq!(syms[1].name, "Top.Sub");
        assert_eq!(syms[2].name, "Top.Sub.Deep");
    }

    #[test]
    fn test_section_byte_range_spans_to_next_header() {
        // "# A\n" = 4, "line1\n" = 6, "line2\n" = 6  → "# B" starts at byte 16
        let content = b"# A\nline1\nline2\n# B\nline3\n";
        let syms = extract(content);
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].byte_range.0, 0);
        assert_eq!(syms[0].byte_range.1, 16);
        assert_eq!(syms[1].byte_range.0, 16);
        assert_eq!(syms[1].byte_range.1, content.len() as u32);
    }

    #[test]
    fn test_duplicate_headers_disambiguated() {
        let syms = extract(b"## Install\ntext\n## Install\ntext\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "Install");
        assert_eq!(syms[1].name, "Install#2");
    }

    // Dogfood B2: heading punctuation must stay human-readable — no `~1`/`~2`/`~3`
    // key-escaping leaking into display names.
    #[test]
    fn test_heading_with_dots_and_brackets_stays_readable() {
        let syms = extract(b"# SymForge 8.13.0 beta issues\n## [FIXED upstream] note\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "SymForge 8.13.0 beta issues");
        assert_eq!(
            syms[1].name,
            "SymForge 8.13.0 beta issues.[FIXED upstream] note"
        );
        assert!(
            !syms.iter().any(|s| s.name.contains("~1")
                || s.name.contains("~2")
                || s.name.contains("~3")),
            "escape encoding must not leak into section names"
        );
    }

    #[test]
    fn test_frontmatter_skipped() {
        let syms = extract(b"---\ntitle: Hello\n---\n# Real Header\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Real Header");
    }

    #[test]
    fn test_empty_file() {
        let syms = extract(b"");
        assert_eq!(syms.len(), 0);
    }

    #[test]
    fn test_edit_capability() {
        assert_eq!(
            MarkdownExtractor.edit_capability(),
            EditCapability::TextEditSafe
        );
    }

    // ---- SF-STRESS-019: fenced code blocks must not produce phantom headings ----

    #[test]
    fn test_hash_inside_backtick_fence_not_heading() {
        let syms = extract(b"# Real\n\n```ruby\n# config/ci.rb\n# not a heading\n```\n");
        assert_eq!(syms.len(), 1, "only the real heading is a section");
        assert_eq!(syms[0].name, "Real");
    }

    #[test]
    fn test_hash_inside_tilde_fence_not_heading() {
        let syms = extract(b"# Real\n\n~~~python\n# also a comment\n~~~\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Real");
    }

    #[test]
    fn test_fence_with_info_string_opens_and_closes() {
        // The rails ```ruby#6-7 case: info strings on the opener must be ignored
        // and must not be treated as a closer.
        let syms = extract(b"# Real\n\n```ruby#6-7\n# TODO: phantom\n```\n## Sub\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "Real");
        assert_eq!(syms[1].name, "Real.Sub", "real sub parents to the real heading");
    }

    #[test]
    fn test_unclosed_fence_at_eof_swallows_rest() {
        // An unclosed fence keeps the remainder of the file as code (CommonMark).
        let syms = extract(b"# Real\n\n```\n# inside unclosed fence\n## also inside\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Real");
    }

    #[test]
    fn test_closing_run_longer_than_opener_closes() {
        // Closer run length >= opener run length closes the fence.
        let syms = extract(b"# Real\n\n```\n# inside\n`````\n## After\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "Real");
        assert_eq!(syms[1].name, "Real.After");
    }

    #[test]
    fn test_shorter_closing_run_does_not_close() {
        // A run shorter than the opener does not close the fence, so the '#'
        // line after it stays code.
        let syms = extract(b"# Real\n\n````\n# inside\n```\n# still inside\n````\n## After\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "Real");
        assert_eq!(syms[1].name, "Real.After");
    }

    #[test]
    fn test_real_heading_after_closed_fence_keeps_parent_path() {
        // Hierarchy repair: a real '## Sub' after a phantom-producing fence must
        // parent under the preceding real heading, not a phantom level-1.
        let syms =
            extract(b"# Top\n\n```ruby\n# config/ci.rb\nclass Foo\nend\n```\n\n## Real Sub\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "Top");
        assert_eq!(syms[1].name, "Top.Real Sub");
        assert_eq!(syms[1].depth, 1);
    }

    #[test]
    fn test_indented_fence_within_three_spaces_tracked() {
        // Up to three leading spaces still opens a fence.
        let syms = extract(b"# Real\n\n   ```\n# inside indented fence\n   ```\n## After\n");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[1].name, "Real.After");
    }

    #[test]
    fn markdown_hit_preserves_section_and_line_pointer() {
        let content = b"\xEF\xBB\xBFpreface\r\n# Section\r\nline \xCE\xB1\r\n## Child\r\nlast";
        let syms = extract(content);

        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "Section");
        assert_eq!(syms[0].byte_range, (12, content.len() as u32));
        assert_eq!(syms[0].line_range, (1, 5));
        assert_eq!(syms[1].name, "Section.Child");
        assert_eq!(syms[1].byte_range, (32, content.len() as u32));
        assert_eq!(syms[1].line_range, (3, 5));
    }

    #[test]
    fn markdown_commonmark_corpus_handles_atx_setext_fence_frontmatter_table_and_link() {
        let content = br##"---
title: "# frontmatter is not a heading"
---
# ATX [Guide](guide.md) #
| Name | Value |
| --- | --- |

```md
Hidden setext
=============
# Hidden ATX
```

Setext [Top](top.md)
====================

Setext Child
-------------
body
"##;
        let syms = extract(content);

        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "ATX [Guide](guide.md)");
        assert_eq!(syms[0].depth, 0);
        assert_eq!(syms[1].name, "Setext [Top](top.md)");
        assert_eq!(syms[1].depth, 0);
        assert_eq!(syms[2].name, "Setext [Top](top.md).Setext Child");
        assert_eq!(syms[2].depth, 1);
        assert!(syms.iter().all(|symbol| !symbol.name.contains("Hidden")));
    }
}
