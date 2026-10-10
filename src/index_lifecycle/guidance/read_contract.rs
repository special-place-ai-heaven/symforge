//! Shared content-read contract and selector validation.
use super::serde_input::{lenient_bool, lenient_u32};
use crate::live_index::search;
use serde::{Deserialize, Serialize};
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GetFileContentInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    /// Relative path to the file.
    pub path: String,
    /// Selection mode: `lines`, `symbol`, `match`, `chunk`.
    /// When set, only flags valid for that mode are accepted; cross-mode flags error.
    /// When omitted, mode is inferred from flags (backward compatible).
    #[serde(default)]
    pub mode: Option<String>,
    /// First line to include (1-indexed).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub start_line: Option<u32>,
    /// Last line to include (1-indexed, inclusive).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub end_line: Option<u32>,
    /// Select a 1-based chunk from the file using `max_lines` as the chunk size.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub chunk_index: Option<u32>,
    /// Maximum number of lines to include in a chunked read.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_lines: Option<u32>,
    /// Center the read around this 1-indexed line.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub around_line: Option<u32>,
    /// Center the read around the first case-insensitive literal match in the file.
    pub around_match: Option<String>,
    /// Select a specific 1-based occurrence of `around_match` instead of the first match.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub match_occurrence: Option<u32>,
    /// Center the read around a symbol in the target file.
    pub around_symbol: Option<String>,
    /// Optional exact-selector line for `around_symbol`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Number of lines of symmetric context to include around `around_line` or `around_match`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub context_lines: Option<u32>,
    /// Show 1-indexed line numbers for ordinary full-file or explicit-range reads.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub show_line_numbers: Option<bool>,
    /// Prepend a stable path or path-plus-range header for ordinary full-file or explicit-range reads.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub header: Option<bool>,
    /// When true, estimate file tokens instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Alias for `start_line` using the Read-tool idiom: 0-based line count to skip.
    /// Translated to `start_line = offset + 1` before processing.
    /// Cannot be combined with `start_line`, `end_line`, `around_line`, `around_match`,
    /// `around_symbol`, `chunk_index`, or `mode`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub offset: Option<u32>,
    /// Alias for `end_line` using the Read-tool idiom: number of lines to include.
    /// Translated to `end_line = offset + limit` before processing.
    /// Cannot be combined with the fields listed under `offset`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Response token budget.
    #[serde(default)]
    pub max_tokens: Option<u64>,
    /// When true, bypass session cache-hit and return a fresh payload.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub force_refresh: Option<bool>,
}

pub(crate) fn normalize_file_content_aliases(
    input: &mut GetFileContentInput,
) -> Result<(), String> {
    if input.offset.is_none() && input.limit.is_none() {
        return Ok(());
    }

    // Reject combination with any explicit native selector.
    let mut conflicts: Vec<&str> = Vec::new();
    if input.start_line.is_some() {
        conflicts.push("`start_line`");
    }
    if input.end_line.is_some() {
        conflicts.push("`end_line`");
    }
    if input.around_line.is_some() {
        conflicts.push("`around_line`");
    }
    if input.around_match.is_some() {
        conflicts.push("`around_match`");
    }
    if input.around_symbol.is_some() {
        conflicts.push("`around_symbol`");
    }
    if input.chunk_index.is_some() {
        conflicts.push("`chunk_index`");
    }
    if input.mode.is_some() {
        conflicts.push("`mode`");
    }
    if !conflicts.is_empty() {
        return Err(format!(
            "Invalid get_file_content request: `offset`/`limit` aliases cannot be combined with {}. Use native params instead.",
            conflicts.join(", ")
        ));
    }

    if input.limit == Some(0) {
        return Err("Invalid get_file_content request: `limit` must be 1 or greater.".to_string());
    }

    let offset = input.offset.unwrap_or(0);
    input.start_line = Some(offset.saturating_add(1));
    if let Some(limit) = input.limit {
        input.end_line = Some(offset.saturating_add(limit));
    }
    input.offset = None;
    input.limit = None;
    Ok(())
}

pub(crate) fn file_content_options_from_input(
    input: &GetFileContentInput,
) -> Result<search::FileContentOptions, String> {
    let show_line_numbers = input.show_line_numbers.unwrap_or(false);
    let header = input.header.unwrap_or(false);

    // ── Mode dispatch ───────────────────────────────────────────────────
    if let Some(mode) = &input.mode {
        let mode_str = mode.as_str();
        let result = match mode_str {
            "lines" => validate_lines_mode(input),
            "symbol" => validate_symbol_mode(input),
            "match" => validate_match_mode(input),
            "chunk" => validate_chunk_mode(input),
            "search" => Err("mode 'search' is not yet implemented".to_string()),
            other => Err(format!(
                "Unknown mode '{other}'. Valid modes: lines, symbol, match, chunk."
            )),
        };
        return result.map(|mut opts| {
            opts.content_context.mode_name = Some(mode_str.to_string());
            opts.content_context.mode_explicit = true;
            opts
        });
    }
    // No mode — infer from flags (existing backward-compatible behavior below)

    if input.symbol_line.is_some() && input.around_symbol.is_none() {
        return Err(
            "Invalid get_file_content request: `symbol_line` requires `around_symbol`.".to_string(),
        );
    }

    if input.match_occurrence.is_some() && input.around_match.is_none() {
        return Err(
            "Invalid get_file_content request: `match_occurrence` requires `around_match`."
                .to_string(),
        );
    }

    if matches!(input.match_occurrence, Some(0)) {
        return Err(
            "Invalid get_file_content request: `match_occurrence` must be 1 or greater."
                .to_string(),
        );
    }

    if let Some(raw_around_symbol) = input.around_symbol.as_deref() {
        let around_symbol = raw_around_symbol.trim();
        if around_symbol.is_empty() {
            return Err(
                "Invalid get_file_content request: `around_symbol` must not be empty.".to_string(),
            );
        }

        if input.start_line.is_some()
            || input.end_line.is_some()
            || input.around_line.is_some()
            || input.around_match.is_some()
            || input.chunk_index.is_some()
        {
            return Err(
                "Invalid get_file_content request: `around_symbol` cannot be combined with `start_line`, `end_line`, `around_line`, `around_match`, or `chunk_index`. Valid with `around_symbol`: `symbol_line`, `context_lines`, `max_lines`."
                    .to_string(),
            );
        }

        let mut opts = search::FileContentOptions::for_explicit_path_read_around_symbol(
            input.path.clone(),
            around_symbol,
            input.symbol_line,
            input.context_lines,
            input.max_lines,
            show_line_numbers,
            header,
        );
        opts.content_context.mode_name = Some("symbol".to_string());
        opts.content_context.mode_explicit = false;
        return Ok(opts);
    }

    if input.max_lines.is_some() && input.chunk_index.is_none() {
        return Err(
            "Invalid get_file_content request: `max_lines` requires `chunk_index`.".to_string(),
        );
    }

    if let Some(chunk_index) = input.chunk_index {
        let Some(max_lines) = input.max_lines else {
            return Err(
                "Invalid get_file_content request: `chunk_index` requires `max_lines`.".to_string(),
            );
        };

        if chunk_index == 0 {
            return Err(
                "Invalid get_file_content request: `chunk_index` must be 1 or greater.".to_string(),
            );
        }

        if max_lines == 0 {
            return Err(
                "Invalid get_file_content request: `max_lines` must be 1 or greater.".to_string(),
            );
        }

        if input.start_line.is_some()
            || input.end_line.is_some()
            || input.around_line.is_some()
            || input.around_match.is_some()
        {
            return Err(
                "Invalid get_file_content request: chunked reads (`chunk_index` + `max_lines`) cannot be combined with `start_line`, `end_line`, `around_line`, or `around_match`."
                    .to_string(),
            );
        }

        let mut opts = search::FileContentOptions::for_explicit_path_read_chunk(
            input.path.clone(),
            chunk_index,
            max_lines,
        );
        opts.content_context.mode_name = Some("chunk".to_string());
        opts.content_context.mode_explicit = false;
        return Ok(opts);
    }

    // show_line_numbers and header are now allowed with all read modes
    // including around_line and around_match for better usability.

    if let Some(raw_around_match) = input.around_match.as_deref() {
        let around_match = raw_around_match.trim();
        if around_match.is_empty() {
            return Err(
                "Invalid get_file_content request: `around_match` must not be empty.".to_string(),
            );
        }

        if input.start_line.is_some() || input.end_line.is_some() || input.around_line.is_some() {
            return Err(
                "Invalid get_file_content request: `around_match` cannot be combined with `start_line`, `end_line`, or `around_line`. Valid with `around_match`: `context_lines`."
                    .to_string(),
            );
        }

        let mut opts = search::FileContentOptions::for_explicit_path_read_around_match(
            input.path.clone(),
            around_match,
            input.match_occurrence,
            input.context_lines,
            show_line_numbers,
            header,
        );
        opts.content_context.mode_name = Some("match".to_string());
        opts.content_context.mode_explicit = false;
        return Ok(opts);
    }

    if input.around_line.is_some() && (input.start_line.is_some() || input.end_line.is_some()) {
        return Err(
            "Invalid get_file_content request: `around_line` cannot be combined with `start_line` or `end_line`. Valid with `around_line`: `context_lines`."
                .to_string(),
        );
    }

    let mut opts = match input.around_line {
        Some(around_line) => search::FileContentOptions::for_explicit_path_read_around_line(
            input.path.clone(),
            around_line,
            input.context_lines,
            show_line_numbers,
            header,
        ),
        None => search::FileContentOptions::for_explicit_path_read_with_format(
            input.path.clone(),
            input.start_line,
            input.end_line,
            show_line_numbers,
            header,
        ),
    };
    opts.content_context.mode_name = Some("lines".to_string());
    opts.content_context.mode_explicit = false;
    Ok(opts)
}

// ── Mode validators for get_file_content ────────────────────────────────

/// Collect names of flags that are `Some` / `true` for error messages.
fn describe_received_flags(input: &GetFileContentInput) -> String {
    let mut flags = Vec::new();
    if input.start_line.is_some() {
        flags.push("start_line");
    }
    if input.end_line.is_some() {
        flags.push("end_line");
    }
    if input.around_line.is_some() {
        flags.push("around_line");
    }
    if input.around_match.is_some() {
        flags.push("around_match");
    }
    if input.match_occurrence.is_some() {
        flags.push("match_occurrence");
    }
    if input.around_symbol.is_some() {
        flags.push("around_symbol");
    }
    if input.symbol_line.is_some() {
        flags.push("symbol_line");
    }
    if input.chunk_index.is_some() {
        flags.push("chunk_index");
    }
    if input.max_lines.is_some() {
        flags.push("max_lines");
    }
    if input.context_lines.is_some() {
        flags.push("context_lines");
    }
    if input.show_line_numbers.is_some() {
        flags.push("show_line_numbers");
    }
    if input.header.is_some() {
        flags.push("header");
    }
    flags.join(", ")
}

fn validate_lines_mode(input: &GetFileContentInput) -> Result<search::FileContentOptions, String> {
    let cross: Vec<&str> = [
        input.around_symbol.as_ref().map(|_| "around_symbol"),
        input.around_match.as_ref().map(|_| "around_match"),
        input.match_occurrence.map(|_| "match_occurrence"),
        input.chunk_index.map(|_| "chunk_index"),
    ]
    .into_iter()
    .flatten()
    .collect();

    if !cross.is_empty() {
        return Err(format!(
            "mode=lines conflicts with {}. Use mode={}. Received: {}",
            cross.join(", "),
            if input.around_symbol.is_some() {
                "symbol"
            } else if input.around_match.is_some() || input.match_occurrence.is_some() {
                "match"
            } else {
                "chunk"
            },
            describe_received_flags(input),
        ));
    }

    let show_line_numbers = input.show_line_numbers.unwrap_or(false);
    let header = input.header.unwrap_or(false);

    if input.around_line.is_some() && (input.start_line.is_some() || input.end_line.is_some()) {
        return Err(
            "Invalid get_file_content request: `around_line` cannot be combined with `start_line` or `end_line`. Valid with `around_line`: `context_lines`."
                .to_string(),
        );
    }

    Ok(match input.around_line {
        Some(around_line) => search::FileContentOptions::for_explicit_path_read_around_line(
            input.path.clone(),
            around_line,
            input.context_lines,
            show_line_numbers,
            header,
        ),
        None => search::FileContentOptions::for_explicit_path_read_with_format(
            input.path.clone(),
            input.start_line,
            input.end_line,
            show_line_numbers,
            header,
        ),
    })
}

fn validate_symbol_mode(input: &GetFileContentInput) -> Result<search::FileContentOptions, String> {
    let Some(raw_around_symbol) = input.around_symbol.as_deref() else {
        return Err("mode=symbol requires around_symbol".to_string());
    };
    let around_symbol = raw_around_symbol.trim();
    if around_symbol.is_empty() {
        return Err("mode=symbol requires around_symbol".to_string());
    }

    let cross: Vec<&str> = [
        input.start_line.map(|_| "start_line"),
        input.end_line.map(|_| "end_line"),
        input.around_line.map(|_| "around_line"),
        input.around_match.as_ref().map(|_| "around_match"),
        input.match_occurrence.map(|_| "match_occurrence"),
        input.chunk_index.map(|_| "chunk_index"),
    ]
    .into_iter()
    .flatten()
    .collect();

    if !cross.is_empty() {
        return Err(format!(
            "mode=symbol conflicts with {}. Received: {}",
            cross.join(", "),
            describe_received_flags(input),
        ));
    }

    let show_line_numbers = input.show_line_numbers.unwrap_or(false);
    let header = input.header.unwrap_or(false);

    Ok(
        search::FileContentOptions::for_explicit_path_read_around_symbol(
            input.path.clone(),
            around_symbol,
            input.symbol_line,
            input.context_lines,
            input.max_lines,
            show_line_numbers,
            header,
        ),
    )
}

fn validate_match_mode(input: &GetFileContentInput) -> Result<search::FileContentOptions, String> {
    let Some(raw_around_match) = input.around_match.as_deref() else {
        return Err("mode=match requires around_match".to_string());
    };
    let around_match = raw_around_match.trim();
    if around_match.is_empty() {
        return Err("mode=match requires around_match".to_string());
    }
    if matches!(input.match_occurrence, Some(0)) {
        return Err(
            "Invalid get_file_content request: `match_occurrence` must be 1 or greater."
                .to_string(),
        );
    }

    let cross: Vec<&str> = [
        input.start_line.map(|_| "start_line"),
        input.end_line.map(|_| "end_line"),
        input.around_line.map(|_| "around_line"),
        input.around_symbol.as_ref().map(|_| "around_symbol"),
        input.chunk_index.map(|_| "chunk_index"),
    ]
    .into_iter()
    .flatten()
    .collect();

    if !cross.is_empty() {
        return Err(format!(
            "mode=match conflicts with {}. Use mode={}. Received: {}",
            cross.join(", "),
            if input.around_symbol.is_some() {
                "symbol"
            } else if input.start_line.is_some()
                || input.end_line.is_some()
                || input.around_line.is_some()
            {
                "lines"
            } else {
                "chunk"
            },
            describe_received_flags(input),
        ));
    }

    let show_line_numbers = input.show_line_numbers.unwrap_or(false);
    let header = input.header.unwrap_or(false);

    Ok(
        search::FileContentOptions::for_explicit_path_read_around_match(
            input.path.clone(),
            around_match,
            input.match_occurrence,
            input.context_lines,
            show_line_numbers,
            header,
        ),
    )
}

fn validate_chunk_mode(input: &GetFileContentInput) -> Result<search::FileContentOptions, String> {
    let chunk_index = input
        .chunk_index
        .ok_or_else(|| "mode=chunk requires chunk_index".to_string())?;
    let max_lines = input
        .max_lines
        .ok_or_else(|| "mode=chunk requires max_lines".to_string())?;

    if chunk_index == 0 {
        return Err(
            "Invalid get_file_content request: `chunk_index` must be 1 or greater.".to_string(),
        );
    }
    if max_lines == 0 {
        return Err(
            "Invalid get_file_content request: `max_lines` must be 1 or greater.".to_string(),
        );
    }

    let cross: Vec<&str> = [
        input.around_symbol.as_ref().map(|_| "around_symbol"),
        input.around_match.as_ref().map(|_| "around_match"),
        input.match_occurrence.map(|_| "match_occurrence"),
        input.around_line.map(|_| "around_line"),
        input.start_line.map(|_| "start_line"),
        input.end_line.map(|_| "end_line"),
    ]
    .into_iter()
    .flatten()
    .collect();

    if !cross.is_empty() {
        return Err(format!(
            "mode=chunk conflicts with {}. Use mode={}. Received: {}",
            cross.join(", "),
            if input.around_symbol.is_some() {
                "symbol"
            } else if input.around_match.is_some() || input.match_occurrence.is_some() {
                "match"
            } else {
                "lines"
            },
            describe_received_flags(input),
        ));
    }

    Ok(search::FileContentOptions::for_explicit_path_read_chunk(
        input.path.clone(),
        chunk_index,
        max_lines,
    ))
}
