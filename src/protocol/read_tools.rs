//! Read-oriented MCP tool input types and pure request parsing helpers.

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Deserializer, Serialize};

use crate::live_index::search;

/// Deserialize a `u32` from either a JSON number or a stringified number like `"5"`.
pub(crate) fn lenient_u32<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<u32>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Deserialize a `bool` from either a JSON boolean or a stringified boolean like `"true"`.
pub(crate) fn lenient_bool<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrStr {
        Bool(bool),
        Str(String),
        Null,
    }
    match BoolOrStr::deserialize(deserializer)? {
        BoolOrStr::Bool(b) => Ok(Some(b)),
        BoolOrStr::Str(s) => match s.as_str() {
            "true" | "1" => Ok(Some(true)),
            "false" | "0" => Ok(Some(false)),
            "" => Ok(None),
            _ => Err(serde::de::Error::custom(format!(
                "expected boolean or \"true\"/\"false\", got \"{s}\""
            ))),
        },
        BoolOrStr::Null => Ok(None),
    }
}

pub(crate) fn lenient_bool_required<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    lenient_bool(deserializer).map(|value| value.unwrap_or(false))
}

const INCLUDE_TESTS_SECTION_MARKER: &str = "__symforge_include_tests";

pub(crate) fn encode_include_tests_marker(
    mut sections: Option<Vec<String>>,
    include_tests: bool,
) -> Option<Vec<String>> {
    if include_tests {
        sections
            .get_or_insert_with(Vec::new)
            .push(INCLUDE_TESTS_SECTION_MARKER.to_string());
    }
    sections
}

pub(crate) fn include_tests_from_sections(sections: Option<&Vec<String>>) -> bool {
    sections
        .map(|items| {
            items
                .iter()
                .any(|section| section == INCLUDE_TESTS_SECTION_MARKER)
        })
        .unwrap_or(false)
}

pub(crate) fn visible_sections(sections: &Option<Vec<String>>) -> Option<Vec<String>> {
    sections.as_ref().and_then(|items| {
        let had_marker = items
            .iter()
            .any(|section| section.as_str() == INCLUDE_TESTS_SECTION_MARKER);
        let visible = items
            .iter()
            .filter(|section| section.as_str() != INCLUDE_TESTS_SECTION_MARKER)
            .cloned()
            .collect::<Vec<_>>();
        if visible.is_empty() && had_marker {
            None
        } else {
            Some(visible)
        }
    })
}

fn add_include_tests_schema(schema: &mut Schema) {
    if let Some(serde_json::Value::Object(properties)) = schema.get_mut("properties") {
        properties.insert(
            "include_tests".to_string(),
            serde_json::json!({
                "type": "boolean",
                "default": false,
                "description": "Include expanded test modules in context output."
            }),
        );
    }
}

pub(crate) fn lenient_u32_required<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => s.parse::<u32>().map_err(serde::de::Error::custom),
    }
}

/// Deserialize a `u64` from either a JSON number or a stringified number.
pub(crate) fn lenient_u64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u64),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<u64>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Deserialize an `i64` from either a JSON number or a stringified number.
pub(crate) fn lenient_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<i64>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Leniently deserialize an `Option<Vec<T>>` — accepts a native JSON array, a
/// stringified JSON array (as sent by some MCP clients like Kilo Code), or a
/// native array of stringified JSON objects (as sent by Codex).
pub(crate) fn lenient_option_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrStr {
        Vec(Vec<serde_json::Value>),
        Str(String),
        Null,
    }
    match VecOrStr::deserialize(deserializer)? {
        VecOrStr::Vec(values) => {
            let result: Result<Vec<T>, _> = values
                .into_iter()
                .enumerate()
                .map(|(i, v)| match serde_json::from_value::<T>(v.clone()) {
                    Ok(item) => Ok(item),
                    Err(direct_err) => {
                        if let serde_json::Value::String(ref s) = v {
                            serde_json::from_str::<T>(s).map_err(|_| {
                                serde::de::Error::custom(format!("element {i}: {direct_err}"))
                            })
                        } else {
                            Err(serde::de::Error::custom(format!(
                                "element {i}: {direct_err}"
                            )))
                        }
                    }
                })
                .collect();
            result.map(Some)
        }
        VecOrStr::Str(s) if s.is_empty() || s == "null" => Ok(None),
        VecOrStr::Str(s) => serde_json::from_str::<Vec<T>>(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
        VecOrStr::Null => Ok(None),
    }
}

/// Leniently deserialize a `Vec<T>` — accepts a native JSON array, a stringified
/// JSON array (as sent by some MCP clients like Kilo Code), or a native array of
/// stringified JSON objects (as sent by Codex, e.g. `["{...}", "{...}"]`).
pub(crate) fn lenient_vec_required<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrStr {
        /// Native JSON array — try direct deserialization first via raw Value.
        Vec(Vec<serde_json::Value>),
        /// Entire array serialized as a single JSON string.
        Str(String),
    }
    match VecOrStr::deserialize(deserializer)? {
        VecOrStr::Vec(values) => {
            // Try deserializing each element. If an element is a JSON string
            // that looks like a JSON object, parse the inner string first.
            values
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    // First try direct deserialization (normal case: native objects)
                    match serde_json::from_value::<T>(v.clone()) {
                        Ok(item) => Ok(item),
                        Err(direct_err) => {
                            // If the value is a string, try parsing it as JSON
                            if let serde_json::Value::String(ref s) = v {
                                serde_json::from_str::<T>(s).map_err(|_| {
                                    serde::de::Error::custom(format!("element {i}: {direct_err}"))
                                })
                            } else {
                                Err(serde::de::Error::custom(format!(
                                    "element {i}: {direct_err}"
                                )))
                            }
                        }
                    }
                })
                .collect()
        }
        VecOrStr::Str(s) => serde_json::from_str::<Vec<T>>(&s).map_err(serde::de::Error::custom),
    }
}

/// Input for `get_symbol`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolInput {
    /// Relative path to the file (required for single lookup; ignored when `targets` is provided).
    #[serde(default)]
    pub path: String,
    /// Symbol name to look up (required for single lookup; ignored when `targets` is provided).
    #[serde(default)]
    pub name: String,
    /// Optional kind filter: "fn", "struct", "enum", "impl", etc.
    pub kind: Option<String>,
    /// Disambiguate when multiple symbols share the same name. Pass the 1-based start line of the
    /// desired symbol (shown in ambiguity errors and search_symbols output).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Optional batch mode: provide multiple targets to retrieve 2+ symbols or code slices in one call.
    /// Each target is a file path + symbol name or byte range. When provided, path/name/kind above are ignored.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[schemars(with = "Vec<SymbolTarget>")]
    pub targets: Option<Vec<SymbolTarget>>,
    /// When true, return an approximate token cost estimate instead of actual content.
    /// Useful for budget planning before fetching large symbols.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response (~1000 default when unset).
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

/// A single target in a `get_symbols` batch request.
///
/// Either provide `name` (symbol lookup) or `start_byte`/`end_byte` (code slice).
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SymbolTarget {
    /// Relative file path.
    pub path: String,
    /// Symbol name for symbol lookup (mutually exclusive with byte range).
    pub name: Option<String>,
    /// Kind filter for symbol lookup (e.g., "fn", "struct").
    pub kind: Option<String>,
    /// Disambiguate when multiple symbols share the same name. Pass the 1-based start line.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Start byte offset for code slice (mutually exclusive with name).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub start_byte: Option<u32>,
    /// End byte offset for code slice (inclusive).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub end_byte: Option<u32>,
}

/// Input for `get_file_content`.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetFileContentInput {
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
    /// When true, return an approximate token count for the file instead of content.
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
    /// Optional maximum token budget for the response.
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

/// Input for `validate_file_syntax`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct ValidateFileSyntaxInput {
    pub path: String,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
}

/// Input for `find_dependents`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindDependentsInput {
    /// Relative file path to find dependents for.
    pub path: String,
    /// Symbol name. NOT a valid parameter for file-level dependents — present
    /// only to detect a symbol-shaped misuse: if you pass `name` (symbol-level)
    /// alongside `path`, the handler returns an explicit redirect to
    /// `find_references`, which answers "who calls this symbol?". Leave unset for
    /// the file-level dependency graph ("what imports this file?").
    #[serde(default)]
    pub name: Option<String>,
    /// Maximum number of dependent files to show (default 20, capped at 100).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Maximum number of reference lines per file (default 5, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// Output format: "text" (default), "mermaid", or "dot".
    pub format: Option<String>,
    /// When true, show compact output: one line per dependent file as
    /// `path (N refs: M call, K type_usage, J import)` with no source text
    /// (60-75% smaller). Best for hub files with many dependents.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub compact: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

/// Input for `get_repo_map`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetRepoMapInput {
    /// Detail level: "compact" (default — ~500 token project overview), "full" (complete symbol outline of every file), "tree" (browsable file tree with per-file stats).
    pub detail: Option<String>,
    /// Subtree path to browse (only used when detail="tree", default: project root).
    pub path: Option<String>,
    /// Max depth levels to expand (only used when detail="tree", default: 2, max: 5).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub depth: Option<u32>,
    /// Maximum number of files to include in the output (only used when detail="full", default: 200).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_files: Option<u32>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

/// Input for `get_file_context`.
#[derive(Serialize, JsonSchema)]
#[schemars(transform = add_include_tests_schema)]
pub struct GetFileContextInput {
    /// Relative path to the file.
    pub path: String,
    /// Optional max token budget, matching hook behavior.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Optional list of sections to include. Allowed values: "outline", "imports", "consumers", "references", "git". Omit to include all sections.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[schemars(with = "Vec<String>")]
    pub sections: Option<Vec<String>>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
}

impl<'de> Deserialize<'de> for GetFileContextInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            path: String,
            #[serde(default, deserialize_with = "lenient_u64")]
            max_tokens: Option<u64>,
            #[serde(default, deserialize_with = "lenient_option_vec")]
            sections: Option<Vec<String>>,
            #[serde(default, deserialize_with = "lenient_bool_required")]
            include_tests: bool,
            #[serde(default, deserialize_with = "lenient_bool")]
            estimate: Option<bool>,
        }

        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            path: raw.path,
            max_tokens: raw.max_tokens,
            sections: encode_include_tests_marker(raw.sections, raw.include_tests),
            estimate: raw.estimate,
        })
    }
}

/// Input for `get_symbol_context`.
#[derive(Serialize, JsonSchema)]
#[schemars(transform = add_include_tests_schema)]
pub struct GetSymbolContextInput {
    /// Symbol name to inspect.
    pub name: String,
    /// Optional file filter (ignored when bundle=true; use path instead).
    pub file: Option<String>,
    /// File path from `search_symbols`. Required when bundle=true or sections is provided.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Output verbosity: "summary" (one-line natural language summary ~90% smaller), "signature" (name+params+return only, ~80% smaller), "compact" (signature + first doc line), "full" (default — complete body). Applies to all three modes: default (controls the definition body), bundle (controls the main symbol body; dependency types always show full definitions), and sections/trace (controls the definition shown in the trace header).
    pub verbosity: Option<String>,
    /// When true, switch to bundle mode: returns symbol body + full definitions of all referenced custom types, resolved recursively. Best for edit preparation. Requires path.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub bundle: Option<bool>,
    /// Optional trace-analysis sections. When provided, switches to trace mode: definition,
    /// callers, callees, implementations, type dependencies, git activity.
    /// Valid values: "dependents", "siblings", "implementations", "git".
    /// Omit for default symbol-context mode. Pass empty array for all trace sections.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[schemars(with = "Vec<String>")]
    pub sections: Option<Vec<String>>,
    /// Optional max token budget for bundle mode. When set, preserves the main
    /// symbol body and sections, then includes type dependencies in priority
    /// order (direct first, then transitive) until the approximate budget
    /// (~4 chars per token) is exhausted.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// When true, return an approximate token cost estimate instead of actual content.
    /// Shows estimated tokens for body, callers, bundle, and raw file.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
}

impl<'de> Deserialize<'de> for GetSymbolContextInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            name: String,
            file: Option<String>,
            path: Option<String>,
            symbol_kind: Option<String>,
            #[serde(default, deserialize_with = "lenient_u32")]
            symbol_line: Option<u32>,
            verbosity: Option<String>,
            #[serde(default, deserialize_with = "lenient_bool")]
            bundle: Option<bool>,
            #[serde(default, deserialize_with = "lenient_option_vec")]
            sections: Option<Vec<String>>,
            #[serde(default, deserialize_with = "lenient_bool_required")]
            include_tests: bool,
            #[serde(default, deserialize_with = "lenient_u64")]
            max_tokens: Option<u64>,
            #[serde(default, deserialize_with = "lenient_bool")]
            estimate: Option<bool>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let sections = match raw.sections {
            Some(sections) => encode_include_tests_marker(Some(sections), raw.include_tests),
            None => None,
        };
        Ok(Self {
            name: raw.name,
            file: raw.file,
            path: raw.path,
            symbol_kind: raw.symbol_kind,
            symbol_line: raw.symbol_line,
            verbosity: raw.verbosity,
            bundle: raw.bundle,
            sections,
            max_tokens: raw.max_tokens,
            estimate: raw.estimate,
        })
    }
}

/// Input for `trace_symbol`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct TraceSymbolInput {
    /// File path containing the symbol.
    pub path: String,
    /// Symbol name to trace.
    pub name: String,
    /// Optional kind filter (e.g., "fn", "struct").
    pub kind: Option<String>,
    /// Optional line number to disambiguate overloaded symbols.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Optional list of output sections to include. When omitted, all sections are included.
    /// Valid values: "dependents", "siblings", "implementations", "git".
    #[serde(default, deserialize_with = "lenient_option_vec")]
    pub sections: Option<Vec<String>>,
    /// Output verbosity: "summary" (one-line natural language summary ~90% smaller), "signature" (name+params+return only, ~80% smaller), "compact" (signature + first doc line), "full" (default — complete body).
    pub verbosity: Option<String>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
}

/// Input for `inspect_match`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InspectMatchInput {
    /// Relative path to the file.
    pub path: String,
    /// 1-based line number to inspect.
    #[serde(deserialize_with = "lenient_u32_required")]
    pub line: u32,
    /// Number of context lines to show around the match (default 3).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub context: Option<u32>,
    /// Maximum number of siblings to show (default 10). Use 0 to hide siblings entirely.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub sibling_limit: Option<u32>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
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
