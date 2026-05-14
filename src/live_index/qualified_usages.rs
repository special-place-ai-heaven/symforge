/// A qualified path match with confidence classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QualifiedMatch {
    /// Byte offset of the match in the source.
    pub offset: usize,
    /// Line number (1-based).
    pub line: usize,
    /// Full source line containing the match.
    pub line_text: String,
    /// The full matched segment (e.g., "MyType::new()").
    pub context: String,
    /// Whether the match is confident (code context) or uncertain (string/comment).
    pub confident: bool,
}

/// A file content snapshot to scan for qualified path usages.
#[derive(Clone, Copy, Debug)]
pub(crate) struct QualifiedFileContent<'a> {
    pub file_path: &'a str,
    pub content: &'a [u8],
}

/// A project-wide qualified path usage with file identity and byte range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QualifiedUsage {
    pub file_path: String,
    pub byte_range: (u32, u32),
    pub line: u32,
    pub line_text: String,
    pub context: String,
    pub confident: bool,
}

/// Collect qualified path usages for `name` across file content snapshots.
///
/// This is a byte scan over every supplied file (`O(files * content_bytes)`),
/// comparable to grep over the workspace. Callers should invoke it only when
/// they need qualified-path coverage in addition to indexed references.
pub(crate) fn collect_qualified_usages<'a>(
    name: &str,
    file_contents: impl IntoIterator<Item = QualifiedFileContent<'a>>,
) -> Vec<QualifiedUsage> {
    let mut usages = Vec::new();
    for file_content in file_contents {
        if !has_qualified_identifier_bytes(file_content.content, name) {
            continue;
        }
        let source = match std::str::from_utf8(file_content.content) {
            Ok(source) => source,
            Err(_) => continue,
        };
        for qualified_match in find_qualified_usages(name, source) {
            let start = qualified_match.offset;
            let end = start + name.len();
            usages.push(QualifiedUsage {
                file_path: file_content.file_path.to_string(),
                byte_range: (start as u32, end as u32),
                line: qualified_match.line as u32,
                line_text: qualified_match.line_text,
                context: qualified_match.context,
                confident: qualified_match.confident,
            });
        }
    }
    usages
}

fn has_qualified_identifier_bytes(content: &[u8], identifier: &str) -> bool {
    let identifier = identifier.as_bytes();
    if identifier.is_empty() || identifier.len() > content.len() {
        return false;
    }

    let mut index = 0usize;
    let last_start = content.len() - identifier.len();
    while index <= last_start {
        let end = index + identifier.len();
        if content[index] == identifier[0] && &content[index..end] == identifier {
            let preceded = index >= 2 && content[index - 2] == b':' && content[index - 1] == b':';
            let followed =
                end + 2 <= content.len() && content[end] == b':' && content[end + 1] == b':';
            if preceded || followed {
                return true;
            }
            index = end;
        } else {
            index += 1;
        }
    }

    false
}

/// Find qualified path usages of `identifier` in `source`.
///
/// Looks for patterns where the identifier appears as a path segment:
/// - `identifier::method()` -- associated function call
/// - `module::identifier::method()` -- deeper nesting
/// - `use path::identifier` -- import path
/// - `identifier::<T>::method()` -- turbofish syntax
///
/// Classifies matches as confident (in code) vs uncertain (in strings/comments).
pub(crate) fn find_qualified_usages(identifier: &str, source: &str) -> Vec<QualifiedMatch> {
    let mut results = Vec::new();

    // Track block comment nesting depth across the whole source.
    // We scan line by line but need to carry block-comment state.
    let mut in_block_comment = false;
    // Track raw string state: None = not in raw string, Some(n) = in raw string with n #s.
    let mut in_raw_string: Option<usize> = None;

    let mut line_byte_offset = 0usize;

    for (line_num, line) in source.split('\n').enumerate() {
        let line_num = line_num + 1;
        // Scan this line for occurrences of `identifier`, updating parse state.
        let line_bytes = line.as_bytes();
        let id_len = identifier.len();

        // We walk through the line character by character to find all occurrences
        // of `identifier` and classify each.
        let mut col = 0usize; // byte index within line
        while col < line_bytes.len() {
            // Skip byte positions inside multi-byte UTF-8 sequences.
            // Identifiers and `::` are ASCII, so no match can start mid-character.
            if !line.is_char_boundary(col) {
                col += 1;
                continue;
            }

            // Check for raw string start: r" or r#..."#
            if !in_block_comment && in_raw_string.is_none() && line_bytes[col] == b'r' {
                let mut hashes = 0usize;
                let mut j = col + 1;
                while j < line_bytes.len() && line_bytes[j] == b'#' {
                    hashes += 1;
                    j += 1;
                }
                if j < line_bytes.len() && line_bytes[j] == b'"' {
                    in_raw_string = Some(hashes);
                    col = j + 1;
                    continue;
                }
            }

            if let Some(hashes) = in_raw_string {
                if line_bytes[col] == b'"' {
                    let mut j = col + 1;
                    let mut count = 0usize;
                    while j < line_bytes.len() && line_bytes[j] == b'#' && count < hashes {
                        count += 1;
                        j += 1;
                    }
                    if count == hashes {
                        in_raw_string = None;
                        col = j;
                        continue;
                    }
                }
                if col + id_len <= line_bytes.len()
                    && line.is_char_boundary(col + id_len)
                    && &line[col..col + id_len] == identifier
                {
                    let preceded = col >= 2 && &line[col - 2..col] == "::";
                    let followed = col + id_len + 2 <= line.len()
                        && line.is_char_boundary(col + id_len + 2)
                        && &line[col + id_len..col + id_len + 2] == "::";
                    if preceded || followed {
                        let ctx_start = line.floor_char_boundary(col.saturating_sub(20));
                        let ctx_end = line.ceil_char_boundary((col + id_len + 20).min(line.len()));
                        results.push(QualifiedMatch {
                            offset: line_byte_offset + col,
                            line: line_num,
                            line_text: line.to_string(),
                            context: line[ctx_start..ctx_end].to_string(),
                            confident: false,
                        });
                    }
                }
                col += 1;
                continue;
            }

            if !in_block_comment {
                if col + 1 < line_bytes.len()
                    && line_bytes[col] == b'/'
                    && line_bytes[col + 1] == b'/'
                {
                    let rest = &line[col..];
                    let mut search_start = 0usize;
                    while let Some(pos) = rest[search_start..].find(identifier) {
                        let abs_col = col + search_start + pos;
                        if !line.is_char_boundary(abs_col) {
                            search_start += pos + 1;
                            continue;
                        }
                        let preceded = abs_col >= 2
                            && line.is_char_boundary(abs_col - 2)
                            && &line[abs_col - 2..abs_col] == "::";
                        let followed = abs_col + id_len + 2 <= line.len()
                            && line.is_char_boundary(abs_col + id_len)
                            && line.is_char_boundary(abs_col + id_len + 2)
                            && &line[abs_col + id_len..abs_col + id_len + 2] == "::";
                        if preceded || followed {
                            let ctx_start = line.floor_char_boundary(abs_col.saturating_sub(20));
                            let ctx_end =
                                line.ceil_char_boundary((abs_col + id_len + 20).min(line.len()));
                            results.push(QualifiedMatch {
                                offset: line_byte_offset + abs_col,
                                line: line_num,
                                line_text: line.to_string(),
                                context: line[ctx_start..ctx_end].to_string(),
                                confident: false,
                            });
                        }
                        search_start += pos + 1;
                    }
                    break;
                }

                if col + 1 < line_bytes.len()
                    && line_bytes[col] == b'/'
                    && line_bytes[col + 1] == b'*'
                {
                    in_block_comment = true;
                    col += 2;
                    continue;
                }
            } else {
                if col + 1 < line_bytes.len()
                    && line_bytes[col] == b'*'
                    && line_bytes[col + 1] == b'/'
                {
                    in_block_comment = false;
                    col += 2;
                    continue;
                }
                if col + id_len <= line_bytes.len()
                    && line.is_char_boundary(col + id_len)
                    && &line[col..col + id_len] == identifier
                {
                    let prec2 = col >= 2 && &line[col - 2..col] == "::";
                    let fol2 = col + id_len + 2 <= line.len()
                        && line.is_char_boundary(col + id_len + 2)
                        && &line[col + id_len..col + id_len + 2] == "::";
                    if prec2 || fol2 {
                        let ctx_start = line.floor_char_boundary(col.saturating_sub(20));
                        let ctx_end = line.ceil_char_boundary((col + id_len + 20).min(line.len()));
                        results.push(QualifiedMatch {
                            offset: line_byte_offset + col,
                            line: line_num,
                            line_text: line.to_string(),
                            context: line[ctx_start..ctx_end].to_string(),
                            confident: false,
                        });
                    }
                }
                col += 1;
                continue;
            }

            if line_bytes[col] == b'"' {
                col += 1;
                while col < line_bytes.len() && line_bytes[col] != b'"' {
                    if line_bytes[col] == b'\\' {
                        col += 2;
                        continue;
                    }
                    if !line.is_char_boundary(col) {
                        col += 1;
                        continue;
                    }
                    if col + id_len <= line_bytes.len()
                        && line.is_char_boundary(col + id_len)
                        && &line[col..col + id_len] == identifier
                    {
                        let prec2 = col >= 2 && &line[col - 2..col] == "::";
                        let fol2 = col + id_len + 2 <= line.len()
                            && line.is_char_boundary(col + id_len + 2)
                            && &line[col + id_len..col + id_len + 2] == "::";
                        if prec2 || fol2 {
                            let ctx_start = line.floor_char_boundary(col.saturating_sub(20));
                            let ctx_end =
                                line.ceil_char_boundary((col + id_len + 20).min(line.len()));
                            results.push(QualifiedMatch {
                                offset: line_byte_offset + col,
                                line: line_num,
                                line_text: line.to_string(),
                                context: line[ctx_start..ctx_end].to_string(),
                                confident: false,
                            });
                        }
                    }
                    col += 1;
                }
                col += 1;
                continue;
            }

            if col + id_len <= line_bytes.len()
                && line.is_char_boundary(col + id_len)
                && &line[col..col + id_len] == identifier
            {
                let prec2 = col >= 2 && &line[col - 2..col] == "::";
                let fol2 = col + id_len + 2 <= line.len()
                    && line.is_char_boundary(col + id_len + 2)
                    && &line[col + id_len..col + id_len + 2] == "::";
                if prec2 || fol2 {
                    let ctx_start = line.floor_char_boundary(col.saturating_sub(20));
                    let ctx_end = line.ceil_char_boundary((col + id_len + 20).min(line.len()));
                    results.push(QualifiedMatch {
                        offset: line_byte_offset + col,
                        line: line_num,
                        line_text: line.to_string(),
                        context: line[ctx_start..ctx_end].to_string(),
                        confident: true,
                    });
                }
                col += id_len;
                continue;
            }

            col += 1;
        }

        line_byte_offset += line.len() + 1;
    }

    results
}
