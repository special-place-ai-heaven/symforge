use std::cell::OnceCell;
use std::ops::Range;

pub(super) struct Boundary {
    content: Range<usize>,
    after: usize,
    cell: bool,
}

fn complete_placeholder(mut value: &[u8]) -> bool {
    while value.len() >= 2
        && matches!(
            (value[0], value[value.len() - 1]),
            (b'<', b'>') | (b'[', b']') | (b'`', b'`')
        )
    {
        value = &value[1..value.len() - 1];
    }
    if super::is_placeholder_name(value) {
        return super::is_placeholder(value);
    }
    std::str::from_utf8(value).is_ok_and(super::is_placeholder_only_expression)
}

pub(super) fn placeholder_ends_at_boundary(
    path: &str,
    bytes: &[u8],
    match_start: usize,
    value_start: usize,
    value_end: usize,
    boundaries: &OnceCell<Vec<Boundary>>,
) -> bool {
    if crate::domain::LanguageId::from_path(path) != Some(crate::domain::LanguageId::Markdown) {
        return false;
    }
    let Some(value) = bytes.get(value_start..value_end) else {
        return false;
    };
    let marker = if let Some(marker) = value.strip_suffix(b":") {
        if !marker.ends_with(b"`") {
            return false;
        }
        marker
    } else {
        value
    };
    let marker_end = marker
        .iter()
        .rposition(|byte| *byte != b'`')
        .map_or(0, |at| at + 1);
    if !complete_placeholder(&marker[..marker_end]) {
        return false;
    }
    let boundaries = boundaries.get_or_init(|| parse_boundaries(bytes));
    let upper = boundaries.partition_point(|boundary| boundary.content.start <= match_start);
    boundaries[upper.saturating_sub(2)..upper]
        .iter()
        .any(|boundary| {
            let range = &boundary.content;
            if range.len() > super::CONTEXT_ASSIGNMENT_SCAN_BOUND
                || !(range.start..range.end).contains(&match_start)
                || value_start < match_start
                || value_start >= range.end
                || !bytes[range.start..match_start].trim_ascii().is_empty()
            {
                return false;
            }
            let end = value_end.min(range.end);
            if !complete_placeholder(&bytes[value_start..end]) {
                return false;
            }
            let tail = bytes[end..range.end].trim_ascii();
            let quote = value_start.checked_sub(1).and_then(|at| bytes.get(at));
            if matches!(quote, Some(b'\'' | b'"')) {
                if value_end > range.end || tail != &bytes[value_start - 1..value_start] {
                    return false;
                }
            } else if !tail.is_empty() {
                return false;
            }
            let prose_colon_capture = value_end == boundary.after.saturating_add(1)
                && bytes.get(boundary.after) == Some(&b':');
            if value_end > range.end && value_end != boundary.after && !prose_colon_capture {
                return false;
            }
            if boundary.cell {
                return true;
            }
            let line_end = bytes[boundary.after..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| boundary.after + offset);
            let rest = bytes[boundary.after..line_end].trim_ascii();
            let prose_colon = rest.strip_prefix(b":").is_none_or(|tail| {
                tail.trim_ascii_start()
                    .first()
                    .is_some_and(|byte| !b"+-*/%=?:&|^([{\\".contains(byte))
            });
            // A closed code span ends prose, but an explicit continuation remains ambiguous.
            prose_colon
                && !rest
                    .first()
                    .is_some_and(|byte| b"'\"`+-*/%=?&|^([{\\".contains(byte))
                && ![b"or".as_slice(), b"and", b"if", b"else"]
                    .iter()
                    .any(|word| {
                        rest.starts_with(word)
                            && rest.get(word.len()).is_some_and(u8::is_ascii_whitespace)
                    })
                && (!rest.is_empty()
                    || line_end == bytes.len()
                    || !super::line_break_continues_expression(bytes, line_end, false))
        })
}

fn parse_boundaries(bytes: &[u8]) -> Vec<Boundary> {
    let mut result = Vec::new();
    let mut offset = 0;
    let mut fence = None;
    let mut previous_cells = None;
    let mut table_columns = None;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        let syntax_line = if offset == 0 {
            line.strip_prefix(super::UTF8_BOM).unwrap_or(line)
        } else {
            line
        };
        let trimmed = syntax_line.trim_ascii();
        // Unparsed HTML, front matter and nested fences retain the strict scanner.
        if fence.is_none()
            && (trimmed.starts_with(b"<")
                || trimmed.starts_with(b">")
                || (offset == 0 && trimmed == b"---")
                || (trimmed
                    .windows(3)
                    .any(|bytes| bytes == b"```" || bytes == b"~~~")
                    && !trimmed.starts_with(b"```")
                    && !trimmed.starts_with(b"~~~")))
        {
            break;
        }
        let leading = syntax_line.len() - syntax_line.trim_ascii_start().len();
        let indent = if syntax_line[..leading].contains(&b'\t') {
            4
        } else {
            leading
        };
        let marker = trimmed
            .first()
            .copied()
            .filter(|byte| matches!(byte, b'`' | b'~'));
        let run = marker.map_or(0, |marker| {
            trimmed.iter().take_while(|byte| **byte == marker).count()
        });
        if let Some((marker, length)) = fence {
            if indent <= 3
                && trimmed.first() == Some(&marker)
                && run >= length
                && trimmed[run..].is_empty()
            {
                fence = None;
            }
        } else if indent <= 3 && run >= 3 {
            fence = marker.map(|marker| (marker, run));
            previous_cells = None;
            table_columns = None;
        } else if indent < 4 && line.len() <= super::QUOTE_PARITY_LINE_BOUND {
            let cells = cells(line);
            let delimiter = cells.as_ref().is_some_and(|cells| {
                cells.len() >= 2
                    && cells.iter().all(|range| {
                        let value = line[range.clone()].trim_ascii();
                        let value = value.strip_prefix(b":").unwrap_or(value);
                        let value = value.strip_suffix(b":").unwrap_or(value);
                        value.len() >= 3 && value.iter().all(|byte| *byte == b'-')
                    })
            });
            if delimiter && previous_cells == cells.as_ref().map(Vec::len) {
                table_columns = previous_cells;
            } else if !cells
                .as_ref()
                .is_some_and(|cells| Some(cells.len()) == table_columns)
            {
                table_columns = None;
            }
            let table_cells = cells
                .as_deref()
                .filter(|cells| Some(cells.len()) == table_columns && !delimiter);
            inline_spans(line, offset, table_cells, &mut result);
            previous_cells = cells.as_ref().map(Vec::len);
        } else {
            previous_cells = None;
            table_columns = None;
        }
        offset += line.len();
    }
    result.sort_by_key(|boundary| boundary.content.start);
    result
}

fn cells(line: &[u8]) -> Option<Vec<Range<usize>>> {
    let mut cuts = Vec::new();
    let mut quote = None;
    let mut ticks = 0;
    let mut index = 0;
    while index < line.len() {
        match line[index] {
            b'\\' => index += 2,
            b'`' if quote.is_none() => {
                let run = line[index..]
                    .iter()
                    .take_while(|byte| **byte == b'`')
                    .count();
                if ticks == 0 {
                    ticks = run;
                } else if ticks == run {
                    ticks = 0;
                }
                index += run;
            }
            byte @ (b'\'' | b'"') if ticks == 0 => {
                if quote == Some(byte) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(byte);
                }
                index += 1;
            }
            b'|' if quote.is_none() && ticks == 0 => {
                cuts.push(index);
                index += 1;
            }
            _ => index += 1,
        }
    }
    if quote.is_some() || ticks != 0 || cuts.is_empty() {
        return None;
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    for end in cuts {
        if !line[start..end].trim_ascii().is_empty() {
            ranges.push(start..end);
        }
        start = end + 1;
    }
    if !line[start..].trim_ascii().is_empty() {
        ranges.push(start..line.len());
    }
    Some(ranges)
}

fn inline_spans(
    line: &[u8],
    offset: usize,
    cells: Option<&[Range<usize>]>,
    result: &mut Vec<Boundary>,
) {
    let mut index = 0;
    while index < line.len() {
        if line[index] == b'\\' {
            index += 2;
            continue;
        }
        if line[index] != b'`' {
            index += 1;
            continue;
        }
        let run = line[index..]
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        let opener = index;
        let start = index + run;
        index = start;
        while index < line.len() {
            if line[index] != b'`' {
                index += 1;
                continue;
            }
            let close = line[index..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            if close == run {
                let after = index + close;
                let cell = cells.is_some_and(|cells| {
                    cells.iter().any(|cell| {
                        cell.start <= opener
                            && cell.end >= after
                            && line[cell.start..opener].trim_ascii().is_empty()
                            && line[after..cell.end].trim_ascii().is_empty()
                    })
                });
                result.push(Boundary {
                    content: offset + start..offset + index,
                    after: offset + after,
                    cell,
                });
                index += close;
                break;
            }
            index += close;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{SecretScan, SecretSpansScan, scan_secret_bytes, scan_secret_spans};

    fn clean(source: &str) {
        assert_eq!(
            scan_secret_bytes("guide.md", source.as_bytes()),
            SecretScan::Clean
        );
        assert!(
            matches!(scan_secret_spans("guide.md", source.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty())
        );
    }

    fn sensitive(source: &str) {
        assert!(matches!(
            scan_secret_bytes("guide.md", source.as_bytes()),
            SecretScan::Sensitive { .. }
        ));
        assert!(
            matches!(scan_secret_spans("guide.md", source.as_bytes()), SecretSpansScan::Spans(spans) if !spans.is_empty())
        );
    }

    #[test]
    fn placeholders_stop_at_complete_markdown_boundaries() {
        let key = "token";
        for value in [
            "REDACTED",
            "\"REDACTED\"",
            "'REDACTED'",
            "your_marker",
            "${MARKER}",
        ] {
            let assignment = format!("{key}={value}");
            clean(&format!(
                "| Example | Label |\n| --- | --- |\n| `{assignment}` | \"OrdinaryLabel\" |\n"
            ));
            clean(&format!("Use `{assignment}` with \"OrdinaryLabel\".\n"));
            clean(&format!("Use `{assignment}`: \"OrdinaryLabel\".\n"));
        }
        clean(&format!("Use ``{key}=REDACTED`` with \"OrdinaryLabel\".\n"));
    }

    #[test]
    fn rejected_prefix_labels_still_check_the_expression() {
        let key = "token";
        let label = "your-password here";
        let canary = ["Opaque", "Fixture", "Canary"].concat();
        sensitive(&format!("Use `{key}=\"{label}\" + \"{canary}\"`\n"));
        clean(&format!("{key}=\"{label}\"\n"));
    }

    #[test]
    fn rust_marker_terminal_newline_requires_a_source_escape() {
        let key = "token";
        let other = "password";
        let newline = ["\\", "n"].concat();
        let clean = format!("fn demo() {{ let text = \"{key}=your_marker{newline}\"; }}");
        assert_eq!(
            scan_secret_bytes("src/demo.rs", clean.as_bytes()),
            SecretScan::Clean
        );
        assert!(
            matches!(scan_secret_spans("src/demo.rs", clean.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty())
        );
        let canary = ["Opaque", "Fixture", "Canary"].concat();
        for source in [
            format!("fn demo() {{ let text = r#\"{key}=your_marker{newline}\"#; }}"),
            format!("fn demo() {{ let text = \"{key}=your_marker;{other}={canary}{newline}\"; }}"),
        ] {
            assert!(matches!(
                scan_secret_bytes("src/demo.rs", source.as_bytes()),
                SecretScan::Sensitive { .. }
            ));
            assert!(
                matches!(scan_secret_spans("src/demo.rs", source.as_bytes()), SecretSpansScan::Spans(spans) if !spans.is_empty())
            );
        }
    }

    #[test]
    fn standalone_prefix_markers_keep_independent_findings() {
        let key = "token";
        let other = "password";
        let canary = ["Opaque", "Fixture", "Canary"].concat();
        sensitive(&format!("Use `{key}=your_value+{canary}`\n"));
        sensitive(&format!("Use `{key}=your_value;{other}={canary}`\n"));
        clean(&format!("Use `{key}=your_marker`\n"));
        clean(&format!("Use `{key}=your-marker`\n"));
    }

    #[test]
    fn quoted_pipes_and_incomplete_delimiters_do_not_prove_a_boundary() {
        let key = "token";
        let canary = ["Markdown", "Canary", "9328"].concat();
        sensitive(&format!(
            "| Example | Label |\n| --- | --- |\n| \"quoted | {key}=REDACTED\" + \"{canary}\" | ok |\n"
        ));
        sensitive(&format!("Use ``{key}=REDACTED` with \"{canary}\"\n"));
        sensitive(&format!("```\n`{key}=REDACTED` \"{canary}\"\n"));
    }

    #[test]
    fn ambiguous_boundaries_and_independent_credentials_stay_withheld() {
        let key = "token";
        let other = "password";
        let canary = ["Markdown", "Canary", "9328"].concat();
        for source in [
            format!("Use `{key}=REDACTED` + \"{canary}\"\n"),
            format!("Use `{key}=REDACTED`: + \"{canary}\"\n"),
            format!("Use `{key}=REDACTED`:\n + \"{canary}\"\n"),
            format!("Use `{key}=REDACTED` ? \"{canary}\" : other\n"),
            format!("Use `{key}=\"REDACTED\": \"{canary}\"`\n"),
            format!("Use `{key}=\"REDACTED`:\" + \"{canary}\"\n"),
            format!("Use `{key}=REDACTED`; \"{canary}\"\n"),
            format!("Use `{key}=REDACTED`\n + \"{canary}\"\n"),
            format!("Use `{key}=REDACTED + \"{canary}\"`\n"),
            format!("Use `{key}=your_value + \"{canary}\"`\n"),
            format!("Use `{key}=your_value+{canary}` with \"OrdinaryLabel\"\n"),
            format!("Use `{key}=your_value;{other}={canary}` with \"OrdinaryLabel\"\n"),
            format!("Use `{key}=[REDACTED]]` with \"OrdinaryLabel\"\n"),
            format!("Use `{key}=your_value`+\"{canary}\"\n"),
            format!("Use `{key}=[REDACTED] + \"{canary}\"`\n"),
            format!("Use `{key}=REDACTED with \"{canary}\"\n"),
            format!("Use \\`{key}=REDACTED` with \"{canary}\"\n"),
            format!("```python\n`{key}=REDACTED` \"{canary}\"\n```\n"),
            format!("    `{key}=REDACTED` \"{canary}\"\n"),
            format!("\t`{key}=REDACTED` \"{canary}\"\n"),
            format!("<script>\n`{key}=REDACTED` \"{canary}\"\n</script>\n"),
            format!("\u{feff}---\nexample: `{key}=\"REDACTED\"` with \"{canary}\"\n---\n"),
            format!("> ```python\n> `{key}=REDACTED` \"{canary}\"\n> ```\n"),
            format!(
                "| Example | Label |\n| --- | --- |\n| `{key}=\"REDACTED\"` \\| \"{canary}\" | ok |\n"
            ),
            format!(
                "| Example | Label |\n| --- | --- |\n| `{key}=REDACTED` | {other}=\"{canary}\" |\n"
            ),
        ] {
            sensitive(&source);
        }
        assert!(matches!(
            scan_secret_bytes(
                "guide.toml",
                format!("Use `{key}=REDACTED` with \"{canary}\"\n").as_bytes()
            ),
            SecretScan::Sensitive { .. }
        ));
    }
}
