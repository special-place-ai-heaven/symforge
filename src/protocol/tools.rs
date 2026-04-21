use parking_lot::RwLock;
/// MCP tool handler methods and their input parameter structs.
///
/// Each handler follows the pattern:
/// 1. Acquire read lock (or write lock for `index_folder`)
/// 2. Check loading guard (except `health` which always responds)
/// 3. Extract needed data into owned values
/// 4. Drop lock
/// 5. Call `format::` function
/// 6. Return `String`
///
/// Anti-patterns avoided (per RESEARCH.md):
/// - Never return JSON — always plain text String (AD-6)
/// - Never use MCP error codes for not-found — return helpful text via format functions
/// - Never hold RwLockReadGuard across await points — extract into owned values first
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

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

/// Deserialize a required `u32` from either a JSON number or a stringified number.
fn lenient_u32_required<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
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
fn lenient_u64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
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
fn lenient_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
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

use crate::domain::LanguageId;
use crate::live_index::{
    IndexedFile, SearchFilesHit, SearchFilesResolveView, SearchFilesTier, SearchFilesView, search,
    store::{IndexState, LiveIndex},
};
use crate::protocol::edit;
use crate::protocol::edit_format;
use crate::protocol::edit_hooks;
use crate::protocol::format;
use crate::protocol::search_format;
use crate::sidecar::handlers::{
    ImpactParams, OutlineParams, SymbolContextParams, impact_tool_text, outline_tool_text,
    repo_map_text, symbol_context_tool_text,
};
use crate::sidecar::{SidecarState, TokenStats};
use crate::watcher;

use super::SymForgeServer;

// ─── Input parameter structs ────────────────────────────────────────────────

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

/// Input for `search_symbols`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SearchSymbolsInput {
    /// Search query (case-insensitive substring match). Optional when `kind` or `path_prefix` is
    /// provided — omitting `query` enables browse mode.
    #[serde(default)]
    pub query: Option<String>,
    /// Optional kind filter using display names such as `fn`, `class`, or `interface`.
    pub kind: Option<String>,
    /// Optional relative path prefix scope, for example `src/` or `src/protocol`.
    pub path_prefix: Option<String>,
    /// Optional canonical language name such as `Rust`, `TypeScript`, `C#`, or `C++`.
    pub language: Option<String>,
    /// Optional maximum number of matches to return (default 50, capped at 100).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// When true, include generated files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_generated: Option<bool>,
    /// When true, include test files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_tests: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

/// Input for `search_text`.
#[derive(Deserialize, Serialize, JsonSchema)]
#[derive(Default)]
pub struct SearchTextInput {
    /// Search query (case-insensitive substring match unless `regex` is true).
    pub query: Option<String>,
    /// Optional list of terms to match with OR semantics.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[schemars(with = "Vec<String>")]
    pub terms: Option<Vec<String>>,
    /// Interpret `query` as a regex pattern instead of a literal substring.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub regex: Option<bool>,
    /// Optional relative path prefix scope, for example `src/` or `src/protocol`.
    pub path_prefix: Option<String>,
    /// Optional canonical language name such as `Rust`, `TypeScript`, `C#`, or `C++`.
    pub language: Option<String>,
    /// Optional maximum number of matches to return across all files (default 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Optional maximum number of matches to return per file (default 5).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// When true, include generated files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_generated: Option<bool>,
    /// When true, include test files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_tests: Option<bool>,
    /// Optional repo-relative include glob, for example `src/**/*.ts`.
    pub glob: Option<String>,
    /// Optional repo-relative exclude glob, for example `**/*.spec.ts`.
    pub exclude_glob: Option<String>,
    /// Optional symmetric number of surrounding lines to render around each match.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub context: Option<u32>,
    /// Optional case-sensitivity override. Literal mode defaults to false; regex mode defaults to true.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub case_sensitive: Option<bool>,
    /// When true, require whole-word matches for literal searches. Not supported with `regex=true`.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub whole_word: Option<bool>,
    /// Group matches: "file" (default), "symbol" (one entry per enclosing symbol),
    /// "usage" (exclude imports and comments), or "names" (flat deduplicated list of
    /// symbol names containing matches — useful as input to batch operations).
    pub group_by: Option<String>,
    /// When true, for each match include a compact list of callers of the enclosing symbol.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub follow_refs: Option<bool>,
    /// Max number of file matches to enrich with callers when follow_refs=true (default 3).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub follow_refs_limit: Option<u32>,
    /// When true, re-rank results by semantic importance (caller count, churn, symbol kind)
    /// rather than simple match count. Default: false.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub ranked: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response. Output is truncated at a line boundary if exceeded.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// When true, interpret `query` as an ast-grep structural pattern instead of a text search.
    /// Matches AST patterns using tree-sitter. Use `$VAR` for single-node metavariables
    /// and `$$$` for multi-node wildcards. Example: `fn $NAME($$$) { $$$ }`.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub structural: Option<bool>,
}

/// Input for `search_files`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SearchFilesInput {
    /// Filename, folder name, or partial path. Required for search and resolve modes. Optional when `changed_with` is provided.
    #[serde(default)]
    pub query: String,
    /// Optional maximum number of matches to return (default 20, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Optional current file path to boost local results.
    pub current_file: Option<String>,
    /// Find files that frequently co-change with this file (uses git temporal coupling data).
    pub changed_with: Option<String>,
    /// Set to true for exact path resolution mode: resolves an ambiguous filename or partial path to one exact project path.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub resolve: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Optional ranking mode. `"frecency"` fuses the tier-based path
    /// match with per-workspace frecency; requires `SYMFORGE_FRECENCY=1`.
    /// Any other value (including `None`) preserves the default
    /// tier-based ordering exactly.
    ///
    /// Note: co-change rerank is NOT yet wired into this parameter. It
    /// will land in Tentacle 3 (see ADR 0013) at which point this
    /// docstring will be updated to describe the fused mode. Until
    /// then, coupling-store signals are only available via the
    /// separate `changed_with=` branch.
    #[serde(default)]
    pub rank_by: Option<String>,
}

/// Input for `index_folder`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct IndexFolderInput {
    /// Absolute or relative path to the directory to index.
    pub path: String,
}

/// Input for `what_changed`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct WhatChangedInput {
    /// Optional Unix timestamp (seconds since epoch). Files newer than this are returned.
    #[serde(default, deserialize_with = "lenient_i64")]
    pub since: Option<i64>,
    /// Optional git ref to diff against, for example `HEAD~5` or `branch:main`.
    pub git_ref: Option<String>,
    /// When true, report uncommitted git changes. Defaults to true when no other mode is specified and a repo root exists.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub uncommitted: Option<bool>,
    /// Optional relative path prefix scope, for example `src/` or `src/protocol`.
    pub path_prefix: Option<String>,
    /// Optional canonical language name such as `Rust`, `TypeScript`, `C#`, or `C++`.
    pub language: Option<String>,
    /// When true, exclude non-source files (docs, configs, images, lock files).
    /// Only files with a recognized programming language extension are included.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub code_only: Option<bool>,
    /// When true, also include a symbol-level diff alongside the file list.
    /// Only applies in git_ref and uncommitted modes (not timestamp mode).
    /// In git_ref mode, diffs the given ref against HEAD.
    /// In uncommitted mode, diffs HEAD against the working tree.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_symbol_diff: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
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
}

/// Input for `validate_file_syntax`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct ValidateFileSyntaxInput {
    pub path: String,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
}

/// Input for `find_references`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindReferencesInput {
    /// Symbol name to find references for (or trait/type name when mode='implementations').
    pub name: String,
    /// Filter by reference kind: "call", "import", "type_usage", or "all" (default: "all"). Ignored when mode='implementations'.
    pub kind: Option<String>,
    /// Optional exact-selector path from `search_symbols`, for example `src/db.rs`. Ignored when mode='implementations'.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`. Ignored when mode='implementations'.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`. Ignored when mode='implementations'.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Maximum number of files/entries to show (default 20 for references, 200 for implementations; capped at 100/500).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Maximum number of reference hits per file (default 10, capped at 50). Ignored when mode='implementations'.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// When true, show compact output: file:line [kind] in symbol — no source text (60-75% smaller). Ignored when mode='implementations'.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub compact: Option<bool>,
    /// Mode: "references" (default — call sites, imports, type usages) or "implementations" (trait/interface implementors and implemented traits). When mode='implementations', only name, direction, and limit are used.
    #[serde(default)]
    pub mode: Option<String>,
    /// Search direction for implementations mode: "trait" (find implementors), "type" (find traits a type implements), or "auto" (default: search both).
    #[serde(default)]
    pub direction: Option<String>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

/// Input for `find_dependents`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindDependentsInput {
    /// Relative file path to find dependents for.
    pub path: String,
    /// Maximum number of dependent files to show (default 20, capped at 100).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Maximum number of reference lines per file (default 10, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// Output format: "text" (default), "mermaid", or "dot".
    pub format: Option<String>,
    /// When true, show compact output: file:line [kind] without source text (60-75% smaller).
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
#[derive(Deserialize, Serialize, JsonSchema)]
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

/// Input for `get_symbol_context`.
#[derive(Deserialize, Serialize, JsonSchema)]
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

/// Input for `analyze_file_impact`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AnalyzeFileImpactInput {
    /// Relative path to the file to re-read from disk.
    pub path: String,
    /// When true, treat the file as newly created and index it.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub new_file: Option<bool>,
    /// When true, also include git temporal co-change data (files that historically change together with this file).
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_co_changes: Option<bool>,
    /// Maximum co-changing files to return (default 10). Only used when include_co_changes=true.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub co_changes_limit: Option<u32>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
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

/// Input for `explore`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ExploreInput {
    /// Natural-language concept or topic to explore (e.g., "error handling", "concurrency", "database").
    pub query: String,
    /// Maximum number of results per category (default 10).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Exploration depth: 1 (default) = symbol names + text patterns + files.
    /// 2 = adds signatures and one hop of dependents for top symbols.
    /// 3 = adds call chains and type dependency edges for top symbols.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub depth: Option<u32>,
    /// When true, include results from vendor, generated, and gitignored files.
    /// By default these are hidden to reduce noise (default false).
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_noise: Option<bool>,
    /// Optional canonical language name filter (e.g., "Rust", "TypeScript", "C#").
    pub language: Option<String>,
    /// Optional relative path prefix scope (e.g., "src/", "backend/").
    pub path_prefix: Option<String>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SmartQueryInput {
    /// Natural language question about the codebase. Examples:
    /// "who calls optimize_deterministic", "where is LiveIndex defined",
    /// "how does the parser work", "what changed", "find file tools.rs"
    pub query: String,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct EditPlanInput {
    /// The symbol name or file path you want to edit.
    pub target: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct InvestigationInput {
    /// Optional focus area to filter suggestions.
    pub focus: Option<String>,
}

/// Input for `diff_symbols`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct DiffSymbolsInput {
    /// Base git ref to compare from (default: "main").
    pub base: Option<String>,
    /// Target git ref to compare to (default: "HEAD").
    pub target: Option<String>,
    /// Optional path filter — only show diffs for files matching this prefix.
    pub path_prefix: Option<String>,
    /// Optional canonical language name such as `Rust`, `TypeScript`, `C#`, or `C++`.
    pub language: Option<String>,
    /// When true, exclude non-source files (docs, configs, images, lock files).
    /// Only files with a recognized programming language extension are included.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub code_only: Option<bool>,
    /// When true, show only per-file summary counts (+N added, -N removed, ~N modified)
    /// without listing individual symbols. Like `git diff --stat` vs `git diff`.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub compact: Option<bool>,
    /// When true, emit only the aggregate summary line (total added/removed/modified
    /// counts) without any per-file detail. Useful for quick change-scope assessment.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub summary_only: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

enum WhatChangedMode {
    Timestamp(i64),
    GitRef(String),
    Uncommitted,
}

#[derive(Default)]
struct ExploreMatchScore {
    raw_count: usize,
    matched_terms: HashSet<String>,
}

#[derive(Default)]
struct ExploreFileSignal {
    raw_score: u64,
    matched_terms: HashSet<String>,
}

struct DerivedExploreCluster {
    seed_terms: Vec<String>,
    promoted_symbols: Vec<String>,
    seed_files: Vec<String>,
}

fn determine_what_changed_mode(
    input: &WhatChangedInput,
    has_repo_root: bool,
) -> Result<WhatChangedMode, String> {
    if let Some(git_ref) = input
        .git_ref
        .as_deref()
        .map(str::trim)
        .filter(|git_ref| !git_ref.is_empty())
    {
        return if has_repo_root {
            Ok(WhatChangedMode::GitRef(
                git_ref
                    .strip_prefix("branch:")
                    .unwrap_or(git_ref)
                    .to_string(),
            ))
        } else {
            Err("Git change detection unavailable; pass `since` for timestamp mode.".to_string())
        };
    }

    if input.uncommitted.unwrap_or(false) || (input.since.is_none() && has_repo_root) {
        return if has_repo_root {
            Ok(WhatChangedMode::Uncommitted)
        } else {
            Err("Git change detection unavailable; pass `since` for timestamp mode.".to_string())
        };
    }

    if let Some(since) = input.since {
        Ok(WhatChangedMode::Timestamp(since))
    } else {
        Err(
            "what_changed requires either `since`, `git_ref`, or an available repo root."
                .to_string(),
        )
    }
}

fn parse_language_filter(input: Option<&str>) -> Result<Option<LanguageId>, String> {
    let Some(language) = input.map(str::trim).filter(|language| !language.is_empty()) else {
        return Ok(None);
    };

    let normalized = language.to_ascii_lowercase();
    let parsed = match normalized.as_str() {
        "rust" => Some(LanguageId::Rust),
        "python" => Some(LanguageId::Python),
        "javascript" => Some(LanguageId::JavaScript),
        "typescript" => Some(LanguageId::TypeScript),
        "go" => Some(LanguageId::Go),
        "java" => Some(LanguageId::Java),
        "c" => Some(LanguageId::C),
        "c++" => Some(LanguageId::Cpp),
        "c#" => Some(LanguageId::CSharp),
        "ruby" => Some(LanguageId::Ruby),
        "php" => Some(LanguageId::Php),
        "swift" => Some(LanguageId::Swift),
        "kotlin" => Some(LanguageId::Kotlin),
        "dart" => Some(LanguageId::Dart),
        "perl" => Some(LanguageId::Perl),
        "elixir" => Some(LanguageId::Elixir),
        "json" => Some(LanguageId::Json),
        "toml" => Some(LanguageId::Toml),
        "yaml" => Some(LanguageId::Yaml),
        "markdown" | "md" => Some(LanguageId::Markdown),
        "env" => Some(LanguageId::Env),
        "html" => Some(LanguageId::Html),
        "css" => Some(LanguageId::Css),
        "scss" => Some(LanguageId::Scss),
        _ => None,
    };

    parsed.map(Some).ok_or_else(|| {
        "Unsupported language filter. Use one of: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir, JSON, TOML, YAML, Markdown, Env, HTML, CSS, SCSS.".to_string()
    })
}

fn filter_paths_by_prefix_and_language(
    paths: Vec<String>,
    path_prefix: Option<&str>,
    language: Option<&str>,
    code_only: bool,
) -> Result<Vec<String>, String> {
    let lang_filter = parse_language_filter(language)?;
    let prefix = path_prefix
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.replace('\\', "/")
                .trim_start_matches("./")
                .trim_start_matches('/')
                .trim_end_matches('/')
                .to_string()
        });

    Ok(paths
        .into_iter()
        .filter(|path| {
            if let Some(ref pfx) = prefix
                && !path.starts_with(pfx.as_str())
            {
                return false;
            }
            if let Some(ref lang) = lang_filter {
                let ext = path.rsplit('.').next().unwrap_or("");
                if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang) {
                    return false;
                }
            }
            if code_only && lang_filter.is_none() {
                let ext = path.rsplit('.').next().unwrap_or("");
                match crate::domain::index::LanguageId::from_extension(ext) {
                    None => return false,
                    Some(lang) => {
                        if crate::parsing::config_extractors::is_config_language(&lang) {
                            return false;
                        }
                    }
                }
            }
            true
        })
        .collect())
}

fn normalize_path_prefix(input: Option<&str>) -> search::PathScope {
    let Some(prefix) = input.map(str::trim).filter(|prefix| !prefix.is_empty()) else {
        return search::PathScope::any();
    };

    let normalized = prefix
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();

    if normalized.is_empty() {
        search::PathScope::any()
    } else {
        search::PathScope::prefix(normalized)
    }
}

fn normalize_exact_path(input: &str) -> String {
    let normalized = input
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();

    if normalized.is_empty() {
        input.trim().to_string()
    } else {
        normalized
    }
}

fn normalize_search_text_glob(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| {
            let normalized = pattern
                .replace('\\', "/")
                .trim_start_matches("./")
                .trim_start_matches('/')
                .to_string();
            // Auto-prefix bare filenames (no glob chars, no path separators)
            // so "foo.rs" matches "**/foo.rs" instead of failing silently.
            if !normalized.contains('*')
                && !normalized.contains('/')
                && !normalized.contains('?')
                && !normalized.contains('[')
            {
                format!("**/{normalized}")
            } else {
                normalized
            }
        })
        .filter(|pattern| !pattern.is_empty())
}

fn search_symbols_options_from_input(
    input: &SearchSymbolsInput,
) -> Result<search::SymbolSearchOptions, String> {
    let is_browse = input
        .query
        .as_ref()
        .map(|q| q.trim().is_empty())
        .unwrap_or(true);
    let default_limit = if is_browse { 20u32 } else { 50u32 };
    Ok(search::SymbolSearchOptions {
        path_scope: normalize_path_prefix(input.path_prefix.as_deref()),
        search_scope: search::SearchScope::Code,
        result_limit: search::ResultLimit::new(
            input.limit.unwrap_or(default_limit).min(100) as usize
        ),
        noise_policy: search::NoisePolicy {
            include_generated: input.include_generated.unwrap_or(false),
            include_tests: input.include_tests.unwrap_or(false),
            include_vendor: true,
            include_ignored: false,
        },
        language_filter: parse_language_filter(input.language.as_deref())?,
    })
}

fn search_text_options_from_input(
    input: &SearchTextInput,
) -> Result<search::TextSearchOptions, String> {
    let is_regex = input.regex.unwrap_or(false);
    let is_ranked = input.ranked.unwrap_or(false);

    // When regex mode is active the user is doing a targeted, precise search
    // and expects completeness over noise reduction.  Default to including
    // tests and vendor files so we don't silently omit results that grep
    // would find.  The user can still explicitly opt out via the normal
    // include_tests / include_generated flags.
    let include_tests = input.include_tests.unwrap_or(is_regex);
    let include_generated = input.include_generated.unwrap_or(false);

    // When ranked=true the user wants results ordered by importance, which
    // requires scanning broadly first.  A low total_limit starves the
    // ranker — boost it so common terms still surface enough files.
    let total_limit = input.limit.unwrap_or(if is_ranked { 200 } else { 50 }) as usize;

    Ok(search::TextSearchOptions {
        path_scope: normalize_path_prefix(input.path_prefix.as_deref()),
        search_scope: search::SearchScope::Code,
        noise_policy: search::NoisePolicy {
            include_generated,
            include_tests,
            include_vendor: true,
            include_ignored: false,
        },
        language_filter: parse_language_filter(input.language.as_deref())?,
        total_limit,
        max_per_file: input.max_per_file.unwrap_or(5) as usize,
        glob: normalize_search_text_glob(input.glob.as_deref()),
        exclude_glob: normalize_search_text_glob(input.exclude_glob.as_deref()),
        context: input.context.map(|context| context as usize),
        case_sensitive: input.case_sensitive,
        whole_word: input.whole_word.unwrap_or(false),
        ranked: is_ranked,
        churn_scores: None,
    })
}

/// Attempt to fix double-escaped regex character classes (e.g., `\\s` -> `\s`).
/// Returns `Some(fixed)` if the pattern contains likely double-escaped sequences,
/// `None` otherwise.
fn fix_common_double_escapes(pattern: &str) -> Option<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\\\\([sdwbntSDWB])").expect("static regex")
    });
    if !RE.is_match(pattern) {
        return None;
    }
    Some(RE.replace_all(pattern, r"\$1").to_string())
}

/// Find up to 5 similar file paths for "file not found" suggestions.
/// Extracts the basename from the failed path and searches the index.
fn suggest_similar_files(index: &crate::live_index::LiveIndex, path: &str) -> Vec<String> {
    let basename = std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(path);
    // Try basename match first
    let mut suggestions: Vec<String> = index
        .find_files_by_basename(basename)
        .into_iter()
        .take(5)
        .map(|s| s.to_string())
        .collect();
    // If no basename match, try the stem (without extension)
    if suggestions.is_empty() {
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|f| f.to_str())
            .unwrap_or(basename);
        if stem.len() >= 3 {
            let stem_lower = stem.to_ascii_lowercase();
            suggestions = index
                .all_files()
                .map(|(p, _)| p.as_str())
                .filter(|p| {
                    let file_stem = std::path::Path::new(p)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    file_stem.to_ascii_lowercase().contains(&stem_lower)
                })
                .take(5)
                .map(|s| s.to_string())
                .collect();
        }
    }
    suggestions
}

fn enrich_with_callers(
    index: &crate::live_index::LiveIndex,
    result: &mut search::TextSearchResult,
    file_limit: usize,
) {
    use std::collections::HashSet;

    for file_matches in result.files.iter_mut().take(file_limit) {
        // Collect unique enclosing symbol names from this file's matches
        let mut symbol_names: HashSet<String> = HashSet::new();
        for m in &file_matches.matches {
            if let Some(ref enc) = m.enclosing_symbol {
                symbol_names.insert(enc.name.clone());
            }
        }

        if symbol_names.is_empty() {
            continue;
        }

        let mut callers: Vec<search::CallerEntry> = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new(); // (file, symbol) dedup

        for sym_name in &symbol_names {
            let refs = index.find_references_for_name(sym_name, None, false);
            for (ref_file, ref_record) in refs {
                // Get enclosing symbol of the reference
                let enclosing_name = ref_record
                    .enclosing_symbol_index
                    .and_then(|idx| {
                        index
                            .get_file(ref_file)
                            .and_then(|f| f.symbols.get(idx as usize))
                            .map(|s| s.name.clone())
                    })
                    .unwrap_or_else(|| "(top-level)".to_string());

                // Skip self-references only when the caller IS one of the matched
                // symbols (same-file callers from different symbols are useful context)
                if ref_file == file_matches.path && symbol_names.contains(&enclosing_name) {
                    continue;
                }

                let key = (ref_file.to_string(), enclosing_name.clone());
                if seen.insert(key) {
                    callers.push(search::CallerEntry {
                        file: ref_file.to_string(),
                        symbol: enclosing_name,
                        line: ref_record.line_range.0 + 1, // 0-based to 1-based
                    });
                }
            }
        }

        // Cap at 10 callers to avoid noise
        callers.truncate(10);

        // Always set callers when follow_refs was requested — distinguishes
        // "not requested" (None) from "ran but found nothing" (Some([]))
        file_matches.callers = Some(callers);
    }
}

/// Translate `offset`/`limit` (Read-tool idiom) into `start_line`/`end_line` in-place.
/// Called at the outermost handler boundary before `proxy_tool_call` so both the
/// local index path and the sidecar-proxied path observe identical normalized input.
fn normalize_file_content_aliases(input: &mut GetFileContentInput) -> Result<(), String> {
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
        return Err(
            "Invalid get_file_content request: `limit` must be 1 or greater.".to_string(),
        );
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

fn file_content_options_from_input(
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

fn freshen_exact_path_for_targeted_retrieval(
    server: &SymForgeServer,
    path_scope: &search::PathScope,
) -> bool {
    let search::PathScope::Exact(relative_path) = path_scope else {
        return false;
    };
    let Some(repo_root) = server.capture_repo_root() else {
        return false;
    };
    let Ok(abs_path) = edit::safe_repo_path(&repo_root, relative_path) else {
        return false;
    };
    watcher::freshen_file_if_stale(relative_path, &abs_path, &server.index)
}

fn edit_capability_label(
    capability: crate::parsing::config_extractors::EditCapability,
) -> &'static str {
    use crate::parsing::config_extractors::EditCapability;

    match capability {
        EditCapability::IndexOnly => "index-only",
        EditCapability::TextEditSafe => "text-edit-safe",
        EditCapability::StructuralEditSafe => "structural-edit-safe",
    }
}

fn prepare_exact_path_for_edit(
    server: &SymForgeServer,
    relative_path: &str,
) -> Result<(PathBuf, edit_format::EditSourceAuthority), String> {
    let repo_root = server
        .capture_repo_root()
        .ok_or_else(|| "Error: no repository root configured.".to_string())?;
    let abs_path =
        edit::safe_repo_path(&repo_root, relative_path).map_err(|e| format!("Error: {e}"))?;
    let source_authority =
        if watcher::freshen_file_if_stale(relative_path, &abs_path, &server.index) {
            edit_format::EditSourceAuthority::DiskRefreshed
        } else {
            edit_format::EditSourceAuthority::CurrentIndex
        };
    Ok((abs_path, source_authority))
}

fn prepare_batch_paths_for_edit(
    server: &SymForgeServer,
    repo_root: &std::path::Path,
    relative_paths: &[String],
) -> Result<edit_format::EditSourceAuthority, String> {
    let mut unique_paths = relative_paths.to_vec();
    unique_paths.sort();
    unique_paths.dedup();

    let mut refreshed = false;
    for relative_path in unique_paths {
        let abs_path =
            edit::safe_repo_path(repo_root, &relative_path).map_err(|e| format!("Error: {e}"))?;
        if watcher::freshen_file_if_stale(&relative_path, &abs_path, &server.index) {
            refreshed = true;
        }
    }

    Ok(if refreshed {
        edit_format::EditSourceAuthority::DiskRefreshed
    } else {
        edit_format::EditSourceAuthority::CurrentIndex
    })
}

fn prepare_project_wide_rename(
    server: &SymForgeServer,
    repo_root: &std::path::Path,
) -> edit_format::EditSourceAuthority {
    if watcher::reconcile_stale_files(repo_root, &server.index) > 0 {
        edit_format::EditSourceAuthority::DiskRefreshed
    } else {
        edit_format::EditSourceAuthority::CurrentIndex
    }
}

fn symbol_anchor(path: &str, symbol: &crate::domain::SymbolRecord) -> String {
    format!("{path}:{}", symbol.line_range.0.saturating_add(1))
}

fn search_scope_summary(
    path_scope: &search::PathScope,
    language_filter: Option<&LanguageId>,
    noise_policy: &search::NoisePolicy,
    glob: Option<&str>,
    exclude_glob: Option<&str>,
    ranked: bool,
) -> String {
    let mut parts = Vec::new();
    match path_scope {
        search::PathScope::Any => parts.push("repo-wide".to_string()),
        search::PathScope::Exact(path) => parts.push(format!("path `{path}`")),
        search::PathScope::Prefix(prefix) => parts.push(format!("path prefix `{prefix}`")),
    }
    if let Some(language) = language_filter {
        parts.push(format!("language `{language}`"));
    }
    parts.push(if noise_policy.include_tests {
        "tests included".to_string()
    } else {
        "tests filtered".to_string()
    });
    parts.push(if noise_policy.include_generated {
        "generated included".to_string()
    } else {
        "generated filtered".to_string()
    });
    if let Some(glob) = glob {
        parts.push(format!("glob `{glob}`"));
    }
    if let Some(exclude_glob) = exclude_glob {
        parts.push(format!("exclude `{exclude_glob}`"));
    }
    if ranked {
        parts.push("ranked ordering enabled".to_string());
    }
    parts.join("; ")
}

fn search_parse_state_for_paths<'a, I>(index: &LiveIndex, paths: I) -> &'static str
where
    I: IntoIterator<Item = &'a str>,
{
    if paths
        .into_iter()
        .filter_map(|path| index.get_file(path))
        .any(|file| file.parse_diagnostic.is_some())
    {
        "partial"
    } else {
        "parsed"
    }
}

fn parse_state_for_file(file: &IndexedFile) -> &'static str {
    match &file.parse_status {
        crate::live_index::store::ParseStatus::Parsed => "parsed",
        crate::live_index::store::ParseStatus::PartialParse { .. } => "partial",
        crate::live_index::store::ParseStatus::Failed { .. } => "degraded",
    }
}

fn context_source_authority_label(refreshed: bool) -> &'static str {
    if refreshed {
        "disk-refreshed"
    } else {
        "current index"
    }
}

fn context_bundle_completeness_label(
    view: &crate::live_index::ContextBundleFoundView,
    rendered: &str,
) -> String {
    let section_overflow =
        view.callers.overflow_count + view.callees.overflow_count + view.type_usages.overflow_count;
    let mut parts = Vec::new();
    if rendered.contains("Truncated at ~") {
        parts.push("budget-limited".to_string());
    }
    if section_overflow > 0 {
        parts.push(format!(
            "section-capped ({} additional reference entries not shown)",
            section_overflow
        ));
    }
    if parts.is_empty() {
        "full".to_string()
    } else {
        parts.join("; ")
    }
}

fn search_completeness_label(overflow_count: usize, suppressed_by_noise: usize) -> String {
    let mut parts = vec![if overflow_count > 0 {
        format!("truncated by result cap ({overflow_count} more omitted)")
    } else {
        "full for current scope".to_string()
    }];
    if suppressed_by_noise > 0 {
        parts.push(format!(
            "{suppressed_by_noise} noise-filtered match(es) suppressed"
        ));
    }
    parts.join("; ")
}

fn search_text_match_type_label(
    is_regex: bool,
    terms: Option<&[String]>,
    auto_detected_regex: bool,
    auto_corrected_regex: bool,
    ranked: bool,
) -> String {
    if auto_corrected_regex {
        "heuristic (auto-corrected regex)".to_string()
    } else if is_regex && auto_detected_regex {
        "heuristic (auto-detected regex)".to_string()
    } else if is_regex {
        "heuristic (regex)".to_string()
    } else if ranked {
        match terms {
            Some(terms) if !terms.is_empty() => "heuristic (ranked OR-literal terms)".to_string(),
            _ => "heuristic (ranked literal)".to_string(),
        }
    } else if matches!(terms, Some(terms) if !terms.is_empty()) {
        "constrained (OR-literal terms)".to_string()
    } else {
        "constrained (literal)".to_string()
    }
}

fn search_symbols_match_type_label(
    result: &search::SymbolSearchResult,
    is_browse: bool,
) -> &'static str {
    if is_browse {
        "constrained (scoped browse)"
    } else {
        match result.hits.first().map(|hit| hit.tier) {
            Some(search::SymbolMatchTier::Exact) => "exact",
            Some(search::SymbolMatchTier::Prefix) => "constrained (prefix tier)",
            Some(search::SymbolMatchTier::Substring) => "heuristic (substring tier)",
            None => "constrained",
        }
    }
}

fn search_files_match_type_label(view: &SearchFilesView) -> &'static str {
    match view {
        SearchFilesView::Found { hits, .. } => match hits.first().map(|hit| hit.tier) {
            Some(SearchFilesTier::CoChange) => "heuristic (git-temporal coupling)",
            Some(SearchFilesTier::StrongPath) | Some(SearchFilesTier::Basename) => {
                "constrained (tiered path relevance)"
            }
            Some(SearchFilesTier::LoosePath) => "heuristic (loose path relevance)",
            None => "constrained",
        },
        _ => "constrained",
    }
}

fn search_files_resolve_match_type_label(view: &SearchFilesResolveView) -> &'static str {
    match view {
        SearchFilesResolveView::Resolved { .. } => "exact (resolve)",
        SearchFilesResolveView::Ambiguous { .. } => "constrained (resolve candidates)",
        _ => "constrained",
    }
}

fn anchored_search_evidence(anchors: Vec<String>, noun: &str) -> String {
    if anchors.is_empty() {
        format!("no {noun} available")
    } else {
        let rendered = anchors
            .into_iter()
            .map(|anchor| format!("`{anchor}`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{noun} {rendered}")
    }
}

fn search_text_evidence(result: &search::TextSearchResult) -> String {
    let anchors = result
        .files
        .iter()
        .flat_map(|file| {
            file.matches
                .iter()
                .take(2)
                .map(move |line_match| format!("{}:{}", file.path, line_match.line_number))
        })
        .take(3)
        .collect();
    anchored_search_evidence(anchors, "line anchors")
}

fn search_symbols_evidence(result: &search::SymbolSearchResult) -> String {
    let anchors = result
        .hits
        .iter()
        .take(3)
        .map(|hit| format!("{}:{}", hit.path, hit.line))
        .collect();
    anchored_search_evidence(anchors, "symbol anchors")
}

fn search_paths_evidence<'a, I>(paths: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let anchors = paths
        .into_iter()
        .take(3)
        .map(std::borrow::ToOwned::to_owned)
        .collect();
    anchored_search_evidence(anchors, "paths")
}

fn find_references_match_type_label(input: &FindReferencesInput, mode: &str) -> &'static str {
    if mode == "implementations" {
        return "constrained (implementations mode)";
    }
    if input.path.is_some() && input.symbol_line.is_some() {
        "exact"
    } else if input.path.is_some() {
        "constrained (path-scoped symbol)"
    } else if input.symbol_kind.is_some() || input.kind.as_deref().is_some_and(|kind| kind != "all")
    {
        "constrained (repo-wide filtered symbol)"
    } else {
        "constrained (repo-wide name match)"
    }
}

fn find_references_scope_summary(input: &FindReferencesInput, mode: &str) -> String {
    let mut parts = Vec::new();
    if mode == "implementations" {
        parts.push(format!(
            "repo-wide implementations for symbol token `{}`",
            input.name
        ));
        parts.push(format!(
            "direction `{}`",
            input.direction.as_deref().unwrap_or("auto")
        ));
        return parts.join("; ");
    }

    match input.path.as_deref() {
        Some(path) => parts.push(format!("path `{path}`")),
        None => parts.push(format!("repo-wide symbol token `{}`", input.name)),
    }
    if let Some(line) = input.symbol_line {
        parts.push(format!("exact selector line {line}"));
    }
    if let Some(symbol_kind) = input.symbol_kind.as_deref() {
        parts.push(format!("symbol kind `{symbol_kind}`"));
    }
    if let Some(reference_kind) = input.kind.as_deref().filter(|kind| *kind != "all") {
        parts.push(format!("reference kind `{reference_kind}`"));
    }
    parts.join("; ")
}

fn find_references_completeness_label(
    view: &crate::live_index::FindReferencesView,
    limits: &format::OutputLimits,
) -> String {
    let shown_files = view.files.len().min(limits.max_files);
    let mut shown_refs = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if shown_refs >= limits.total_hits {
            break;
        }
        let remaining_budget = limits.total_hits.saturating_sub(shown_refs);
        shown_refs += file
            .hits
            .len()
            .min(limits.max_per_file)
            .min(remaining_budget);
    }
    let omitted_refs = view.total_refs.saturating_sub(shown_refs);
    let omitted_files = view.total_files.saturating_sub(shown_files);
    if omitted_refs == 0 && omitted_files == 0 {
        return "full for current scope".to_string();
    }
    let mut parts = vec!["truncated by result cap".to_string()];
    if omitted_refs > 0 {
        parts.push(format!("{omitted_refs} reference(s) omitted"));
    }
    if omitted_files > 0 {
        parts.push(format!("{omitted_files} file(s) omitted"));
    }
    parts.join("; ")
}

fn find_references_evidence(view: &crate::live_index::FindReferencesView) -> String {
    let anchors = view
        .files
        .iter()
        .flat_map(|file| {
            let file_path = file.file_path.clone();
            file.hits.iter().flat_map(move |hit| {
                hit.context_lines
                    .iter()
                    .filter(|line| line.is_reference_line)
                    .map({
                        let file_path = file_path.clone();
                        move |line| format!("{file_path}:{}", line.line_number)
                    })
            })
        })
        .take(3)
        .collect();
    anchored_search_evidence(anchors, "reference anchors")
}

fn implementations_parse_state_for_paths(
    index: &LiveIndex,
    view: &crate::live_index::ImplementationsView,
) -> &'static str {
    search_parse_state_for_paths(
        index,
        view.entries.iter().map(|entry| entry.file_path.as_str()),
    )
}

fn implementations_completeness_label(
    view: &crate::live_index::ImplementationsView,
    limits: &format::OutputLimits,
) -> String {
    let shown = view
        .entries
        .len()
        .min(limits.max_files * limits.max_per_file);
    let omitted = view.entries.len().saturating_sub(shown);
    if omitted == 0 {
        "full for current scope".to_string()
    } else {
        format!("truncated by result cap ({omitted} implementation entry(s) omitted)")
    }
}

fn implementations_evidence(view: &crate::live_index::ImplementationsView) -> String {
    let anchors = view
        .entries
        .iter()
        .take(3)
        .map(|entry| format!("{}:{}", entry.file_path, entry.line + 1))
        .collect();
    anchored_search_evidence(anchors, "implementation anchors")
}

fn explore_is_test_like_path(
    path: &str,
    classification: Option<&crate::domain::index::FileClassification>,
) -> bool {
    let path_lower = path.replace('\\', "/").to_ascii_lowercase();
    classification.is_some_and(|c| c.is_test)
        || path_lower.contains("/tests/")
        || path_lower.contains("/test/")
        || path_lower.contains("/__tests__/")
        || path_lower.ends_with("/tests.rs")
        || path_lower.ends_with("/test.rs")
        || path_lower.ends_with("_test.rs")
        || path_lower.ends_with("_spec.rs")
}

fn explore_should_skip_path_boost(
    path: &str,
    classification: &crate::domain::index::FileClassification,
    include_noise: bool,
) -> bool {
    if include_noise {
        return false;
    }
    explore_is_test_like_path(path, Some(classification))
        || classification.is_vendor
        || classification.is_generated
        || classification.is_config
}

fn explore_path_penalty(
    path: &str,
    classification: Option<&crate::domain::index::FileClassification>,
) -> u64 {
    let path_lower = path.replace('\\', "/").to_ascii_lowercase();
    if explore_is_test_like_path(path, classification) {
        return 2;
    }
    if classification.is_some_and(|c| c.is_config)
        || path_lower.ends_with(".md")
        || path_lower.contains("/docs/")
        || path_lower.contains("/plans/")
        || path_lower.contains("/manual/")
        || path_lower.contains("changelog")
        || path_lower.contains(".planning/")
        || path_lower.contains(".auto-claude")
    {
        return 2;
    }
    if path_lower.contains("/examples/")
        || path_lower.contains("/fixtures/")
        || path_lower.contains("/bench/")
        || path_lower.contains("/benches/")
        || path_lower.contains("/sample/")
        || path_lower.contains("/samples/")
    {
        return 3;
    }
    8
}

fn explore_fallback_alignment_multiplier(
    query_term_count: usize,
    matched_term_count: usize,
) -> u64 {
    if query_term_count <= 1 {
        return 8;
    }
    match (query_term_count, matched_term_count) {
        (_, 0) => 1,
        (2, 1) => 3,
        (2, _) => 8,
        (3, 1) => 2,
        (3, 2) => 6,
        (3, _) => 8,
        (_, 1) => 1,
        (_, 2) => 4,
        _ => 8,
    }
}

fn changed_paths_completeness_label(before_filter: usize, after_filter: usize) -> String {
    if before_filter == after_filter {
        "full for current scope".to_string()
    } else {
        format!("full for filtered scope ({after_filter} of {before_filter} path(s) shown)")
    }
}

fn what_changed_scope_summary(input: &WhatChangedInput, mode: &WhatChangedMode) -> String {
    let mut parts = Vec::new();
    match mode {
        WhatChangedMode::Timestamp(since_ts) => parts.push(format!("timestamp since `{since_ts}`")),
        WhatChangedMode::Uncommitted => parts.push("uncommitted working tree".to_string()),
        WhatChangedMode::GitRef(git_ref) => {
            parts.push(format!("git diff from `{git_ref}` to `HEAD`"))
        }
    }
    if let Some(path_prefix) = input
        .path_prefix
        .as_deref()
        .filter(|prefix| !prefix.trim().is_empty())
    {
        parts.push(format!(
            "path prefix `{}`",
            normalize_exact_path(path_prefix)
        ));
    }
    if let Some(language) = input.language.as_deref() {
        parts.push(format!("language `{language}`"));
    }
    if input.code_only.unwrap_or(false) {
        parts.push("code-only filter".to_string());
    }
    if input.include_symbol_diff.unwrap_or(false) {
        parts.push("symbol diff appended".to_string());
    }
    parts.join("; ")
}

fn what_changed_source_authority_label(mode: &WhatChangedMode) -> &'static str {
    match mode {
        WhatChangedMode::Timestamp(_) => "current index",
        WhatChangedMode::Uncommitted => "git working tree",
        WhatChangedMode::GitRef(_) => "git ref diff",
    }
}

fn what_changed_parse_state_label(
    mode: &WhatChangedMode,
    include_symbol_diff: bool,
) -> &'static str {
    match mode {
        WhatChangedMode::Timestamp(_) => "parsed",
        _ if include_symbol_diff => {
            "degraded (git path diff + lexical symbol diff — regex extraction may miss nested symbols)"
        }
        _ => "not-applicable (git path diff)",
    }
}

#[allow(clippy::too_many_arguments)]
fn render_diff_symbols_output(
    base: &str,
    target: &str,
    all_changed_files: usize,
    changed_files: &[&str],
    repo: &crate::git::GitRepo,
    compact: bool,
    summary_only: bool,
    path_prefix: Option<&str>,
    language: Option<&str>,
    code_only: bool,
) -> String {
    let mut scope_parts = vec![format!("git diff `{base}`...`{target}`")];
    if let Some(path_prefix) = path_prefix.filter(|prefix| !prefix.trim().is_empty()) {
        scope_parts.push(format!(
            "path prefix `{}`",
            normalize_exact_path(path_prefix)
        ));
    }
    if let Some(language) = language {
        scope_parts.push(format!("language `{language}`"));
    }
    if code_only {
        scope_parts.push("code-only filter".to_string());
    }
    if compact {
        scope_parts.push("compact output".to_string());
    }
    if summary_only {
        scope_parts.push("summary-only output".to_string());
    }
    let completeness = if changed_files.len() == all_changed_files {
        "full for filtered git delta".to_string()
    } else {
        format!(
            "full for filtered git delta ({} of {} changed file(s) shown)",
            changed_files.len(),
            all_changed_files
        )
    };
    let envelope = search_format::format_search_envelope(
        "exact (git ref diff)",
        "git ref diff",
        "high (tree-sitter AST extraction for supported languages, regex fallback for others)",
        &completeness,
        &scope_parts.join("; "),
        &search_paths_evidence(changed_files.iter().copied()),
    );
    let output =
        format::diff_symbols_result_view(base, target, changed_files, repo, compact, summary_only);
    format!("{envelope}\n\n{output}")
}

#[allow(clippy::too_many_arguments)]
fn render_search_text_output(
    server: &SymForgeServer,
    result: Result<search::TextSearchResult, search::TextSearchError>,
    group_by: Option<&str>,
    terms: Option<&[String]>,
    options: &search::TextSearchOptions,
    is_regex: bool,
    auto_detected_regex: bool,
    auto_corrected_regex: bool,
) -> String {
    let envelope = match &result {
        Ok(result) if !result.files.is_empty() => {
            let guard = server.index.read();
            Some(search_format::format_search_envelope(
                &search_text_match_type_label(
                    is_regex,
                    terms,
                    auto_detected_regex,
                    auto_corrected_regex,
                    options.ranked,
                ),
                "current index",
                search_parse_state_for_paths(
                    &guard,
                    result.files.iter().map(|file| file.path.as_str()),
                ),
                &search_completeness_label(result.overflow_count, result.suppressed_by_noise),
                &search_scope_summary(
                    &options.path_scope,
                    options.language_filter.as_ref(),
                    &options.noise_policy,
                    options.glob.as_deref(),
                    options.exclude_glob.as_deref(),
                    options.ranked,
                ),
                &search_text_evidence(result),
            ))
        }
        _ => None,
    };
    let confidence = if auto_corrected_regex {
        0.75f32
    } else if is_regex && auto_detected_regex {
        0.80
    } else if is_regex {
        0.85
    } else if options.ranked {
        0.80
    } else {
        0.95
    };
    let output = format::search_text_result_view(result, group_by, terms, Some(confidence));
    match envelope {
        Some(envelope) => format!("{envelope}\n\n{output}"),
        None => output,
    }
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

fn sidecar_state_for_server(server: &SymForgeServer) -> SidecarState {
    SidecarState {
        index: Arc::clone(&server.index),
        token_stats: server.token_stats.clone().unwrap_or_else(TokenStats::new),
        repo_root: server.capture_repo_root(),
        symbol_cache: Arc::new(RwLock::new(HashMap::new())),
    }
}

fn symbol_candidate_paths(index: &crate::live_index::store::LiveIndex, name: &str) -> Vec<String> {
    let mut candidates: Vec<String> = index
        .all_files()
        .filter_map(|(path, file)| {
            if file.symbols.iter().any(|s| s.name == name) {
                Some(path.to_string())
            } else {
                None
            }
        })
        .collect();
    candidates.sort();
    candidates.dedup();
    candidates
}

fn render_symbol_context_header(
    file: &IndexedFile,
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> Option<String> {
    use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};

    match resolve_symbol_selector(file, name, symbol_kind, symbol_line) {
        SymbolSelectorMatch::Selected(_, sym) => {
            let body = std::str::from_utf8(
                &file.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize],
            )
            .ok()?;
            let (rendered, actual_level) = format::resolve_verbosity(
                body,
                Some(verbosity),
                max_tokens,
                0.7, // standalone symbol — allocate 70% of budget to body
            );
            let mut output = format!(
                "{}\n[{}, {}:{}-{}]",
                rendered,
                sym.kind,
                file.relative_path,
                sym.line_range.0 + 1,
                sym.line_range.1 + 1
            );
            if actual_level != "full" && actual_level != verbosity {
                output.push_str(&format!(
                    "\n[adaptive verbosity: {} — fits within {} token budget]",
                    actual_level,
                    max_tokens.unwrap_or(0)
                ));
            }
            Some(output)
        }
        SymbolSelectorMatch::NotFound | SymbolSelectorMatch::Ambiguous(_) => None,
    }
}

enum CapturedGetSymbolsEntry {
    SymbolLookup {
        file: Arc<IndexedFile>,
        name: String,
        kind: Option<String>,
        symbol_line: Option<u32>,
    },
    CodeSlice {
        file: Arc<IndexedFile>,
        start_byte: usize,
        end_byte: Option<usize>,
    },
    FileNotFound {
        path: String,
    },
}

// ─── Tool handlers ───────────────────────────────────────────────────────────

/// Loading guard helper — returns `Some(message)` when index is NOT ready.
///
/// Call at the top of every handler except `health`. If `Some` is returned,
/// return that string immediately. Otherwise continue with the handler body.
macro_rules! loading_guard {
    ($guard:expr) => {
        match $guard.index_state() {
            IndexState::Ready => {}
            IndexState::Empty => return format::empty_guard_message(),
            IndexState::Loading => return format::loading_guard_message(),
            IndexState::CircuitBreakerTripped { summary } => {
                return format!("Index degraded: {summary}");
            }
        }
    };
}

/// Returns true if the path is a sensitive system directory that should not be indexed.
/// Guards against accidental or malicious indexing of /, /etc, /proc, Windows system dirs, etc.
fn is_sensitive_path(canonical: &std::path::Path) -> bool {
    let s = canonical.to_string_lossy();

    // Unix sensitive roots
    #[cfg(unix)]
    {
        const BLOCKED: &[&str] = &[
            "/", "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/run", "/sbin",
            "/sys", "/usr", "/var",
        ];
        if BLOCKED.iter().any(|b| s == *b) {
            return true;
        }
    }

    // Windows sensitive roots (case-insensitive)
    #[cfg(windows)]
    {
        let lower = s.to_ascii_lowercase();
        if lower == r"c:\" || lower == r"c:/" || lower.starts_with(r"c:\windows") {
            return true;
        }
    }

    false
}

fn loading_guard_message_from_published(
    published: &crate::live_index::PublishedIndexState,
) -> Option<String> {
    match published.status {
        crate::live_index::PublishedIndexStatus::Ready => None,
        crate::live_index::PublishedIndexStatus::Empty => Some(format::empty_guard_message()),
        crate::live_index::PublishedIndexStatus::Loading => Some(format::loading_guard_message()),
        crate::live_index::PublishedIndexStatus::Degraded => Some(format!(
            "Index degraded: {}",
            published
                .degraded_summary
                .as_deref()
                .unwrap_or("circuit breaker tripped")
        )),
    }
}

#[tool_router(vis = "pub(crate)")]
impl SymForgeServer {
    /// Look up symbol(s) by file path and name. Single mode: provide path + name for one symbol.
    /// Batch mode: provide targets[] array for multiple symbols or code slices in one call.
    /// When multiple symbols share the same name (e.g. `handle` in different impl blocks),
    /// pass symbol_line (1-based) to disambiguate; omitting it auto-selects by kind tier or
    /// returns an Ambiguous error listing candidate lines.
    /// NOT for finding symbols by name (use search_symbols first).
    /// NOT for understanding who calls it (use find_references or get_symbol_context).
    /// NOT for edit preparation (use get_symbol_context with bundle=true).
    #[tool(
        description = "Prefer this over reading an entire file when you already know the symbol or have narrowed to one file. Retrieves the complete source code of a specific symbol with doc comments. Single mode: provide path + name. Batch mode: provide targets[] array for 2+ symbols or code slices in one call (each target is file path + symbol name or byte range). When multiple symbols share the same name, pass symbol_line (1-based) to disambiguate; auto-selects by kind tier when possible. Use search_symbols first if you only know part of the name. NOT for understanding callers (use find_references or get_symbol_context). NOT for edit preparation (use get_symbol_context with bundle=true).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn get_symbol(&self, params: Parameters<GetSymbolInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbol", &params.0).await {
            return result;
        }

        // Batch mode: targets[] provided
        if let Some(ref targets) = params.0.targets {
            if targets.is_empty() {
                return "Error: targets array is empty.".to_string();
            }
            let captured = {
                let guard = self.index.read();
                loading_guard!(guard);

                targets
                    .iter()
                    .map(|target| match target.name.as_deref() {
                        Some(name) => match guard.capture_shared_file(&target.path) {
                            Some(file) => CapturedGetSymbolsEntry::SymbolLookup {
                                file,
                                name: name.to_string(),
                                kind: target.kind.clone(),
                                symbol_line: target.symbol_line,
                            },
                            None => CapturedGetSymbolsEntry::FileNotFound {
                                path: target.path.clone(),
                            },
                        },
                        None => match guard.capture_shared_file(&target.path) {
                            None => CapturedGetSymbolsEntry::FileNotFound {
                                path: target.path.clone(),
                            },
                            Some(file) => CapturedGetSymbolsEntry::CodeSlice {
                                file,
                                start_byte: target.start_byte.unwrap_or(0) as usize,
                                end_byte: target.end_byte.map(|e| e as usize),
                            },
                        },
                    })
                    .collect::<Vec<_>>()
            };

            // Frecency bump — batch commitment path. Collect the file path
            // for every resolved entry (skip FileNotFound since we did not
            // actually touch them), dedup, and emit one bump call at the end
            // per Implementation Notes §"Bump dedup per tool invocation".
            // No-op unless SYMFORGE_FRECENCY=1.
            let bump_paths: Vec<PathBuf> = captured
                .iter()
                .filter_map(|entry| match entry {
                    CapturedGetSymbolsEntry::SymbolLookup { file, .. }
                    | CapturedGetSymbolsEntry::CodeSlice { file, .. } => {
                        Some(PathBuf::from(&file.relative_path))
                    }
                    CapturedGetSymbolsEntry::FileNotFound { .. } => None,
                })
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            let output = captured
                .into_iter()
                .map(|entry| match entry {
                    CapturedGetSymbolsEntry::SymbolLookup {
                        file,
                        name,
                        kind,
                        symbol_line,
                    } => {
                        let body = format::symbol_detail_from_indexed_file(
                            file.as_ref(),
                            &name,
                            kind.as_deref(),
                            symbol_line,
                        );
                        format!(
                            "{body}{}",
                            format::compact_next_step_hint(&[
                                "get_symbol_context (callers/callees/types)",
                                "find_references (usages)",
                                "edit_within_symbol / replace_symbol_body (edits)",
                            ])
                        )
                    }
                    CapturedGetSymbolsEntry::CodeSlice {
                        file,
                        start_byte,
                        end_byte,
                    } => format::code_slice_from_indexed_file(file.as_ref(), start_byte, end_byte),
                    CapturedGetSymbolsEntry::FileNotFound { path } => format::not_found_file(&path),
                })
                .collect::<Vec<_>>()
                .join("\n---\n");
            self.record_tool_savings_named(
                "get_symbol",
                (output.len() * 5 / 4) as u64,
                (output.len() / 4) as u64,
            );
            self.bump_frecency(&bump_paths);
            return output;
        }

        // Single mode: path + name
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };

        // Estimate mode: return token cost without full content
        if params.0.estimate == Some(true) {
            if let Some(ref file) = file {
                let file_tokens = file.content.len() / 4;
                let sym_tokens = file
                    .symbols
                    .iter()
                    .find(|s| s.name == params.0.name)
                    .map(|s| (s.byte_range.1 - s.byte_range.0) as usize / 4)
                    .unwrap_or(0);
                return format!(
                    "Estimate for get_symbol(path=\"{}\", name=\"{}\"):\n  Symbol body: ~{} tokens\n  Raw file: ~{} tokens",
                    params.0.path, params.0.name, sym_tokens, file_tokens
                );
            } else {
                return format::not_found_file(&params.0.path);
            }
        }

        match file {
            Some(file) => {
                let body = format::symbol_detail_from_indexed_file(
                    file.as_ref(),
                    &params.0.name,
                    params.0.kind.as_deref(),
                    params.0.symbol_line,
                );
                let output = format!(
                    "{body}{}",
                    format::compact_next_step_hint(&[
                        "get_symbol_context (callers/callees/types)",
                        "find_references (usages)",
                        "edit_within_symbol / replace_symbol_body (edits)",
                    ])
                );
                self.record_tool_savings_named(
                    "get_symbol",
                    (output.len() * 5 / 4) as u64,
                    (output.len() / 4) as u64,
                );
                self.session_context.record_symbol(
                    &params.0.path,
                    &params.0.name,
                    (output.len() / 4) as u32,
                );
                // Frecency bump — commitment tool, single-symbol happy path.
                // No-op unless SYMFORGE_FRECENCY=1. See wiki
                // `[[SymForge Frecency-Weighted File Ranking]]` §"Bump hooks".
                self.bump_frecency(&[PathBuf::from(&params.0.path)]);
                output
            }
            None => {
                let suggestions = {
                    let guard = self.index.read();
                    suggest_similar_files(&guard, &params.0.path)
                };
                format::not_found_file_with_suggestions(&params.0.path, &suggestions)
            }
        }
    }

    /// Start here. Project overview with adjustable detail level. Modes: (1) default/compact: ~500 token
    /// overview with file count, languages, and directory tree. (2) detail='full': complete symbol outline
    /// of every file — warning: large output. (3) detail='tree': browsable file tree with per-file symbol
    /// counts and language tags — supports path and depth params for subtree browsing.
    /// NOT for file details (use get_file_context) or finding symbols (use search_symbols).
    #[tool(
        description = "Start here for project orientation and the first code-reading pass before any broad raw file read. Returns a structural overview of the repository. Modes: (1) default/compact: ~500 token overview with file count, languages, and directory tree. (2) detail='full': complete symbol outline of every file — warning: large output. (3) detail='tree': browsable file tree with per-file symbol counts and language tags — supports path and depth params for subtree browsing. NOT for file details (use get_file_context) or finding symbols (use search_symbols).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn get_repo_map(&self, params: Parameters<GetRepoMapInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_repo_map", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let guard = self.index.read();
            loading_guard!(guard);
            let file_count = guard.file_count();
            drop(guard);
            let detail = params.0.detail.as_deref().unwrap_or("compact");
            let est = match detail {
                "full" => {
                    let max = params.0.max_files.unwrap_or(200) as usize;
                    max.min(file_count) * 30
                }
                "tree" => file_count * 10,
                _ => 500,
            };
            return format!(
                "Estimate for get_repo_map: ~{} tokens (detail={}, files={})",
                est, detail, file_count
            );
        }
        let detail = if let Some(ref d) = params.0.detail {
            d.as_str()
        } else if let Some(max_tokens) = params.0.max_tokens {
            // Adaptive detail: auto-select "full" when the estimated output fits
            // within the token budget, otherwise fall back to "compact".
            let guard = self.index.read();
            let file_count = guard.file_count();
            drop(guard);
            let max_files = params.0.max_files.unwrap_or(200) as usize;
            let full_estimate = max_files.min(file_count) * 30;
            if (full_estimate as u64) <= max_tokens {
                "full"
            } else {
                "compact"
            }
        } else {
            "compact"
        };
        let output = match detail {
            "full" => {
                let published = self.index.published_state();
                if let Some(message) = loading_guard_message_from_published(&published) {
                    return message;
                }
                let view = self.index.published_repo_outline();
                if let Some(ref path) = params.0.path {
                    // Filter outline to files under the given path prefix
                    let filtered_files: Vec<_> = view
                        .files
                        .iter()
                        .filter(|fo| fo.relative_path.starts_with(path.as_str()))
                        .cloned()
                        .collect();
                    let filtered_symbols: usize =
                        filtered_files.iter().map(|f| f.symbol_count).sum();
                    let filtered_view = crate::live_index::query::RepoOutlineView {
                        total_files: filtered_files.len(),
                        total_symbols: filtered_symbols,
                        files: filtered_files,
                    };
                    let max_files = params.0.max_files.unwrap_or(200) as usize;
                    if filtered_view.files.len() > max_files {
                        let remaining = filtered_view.files.len() - max_files;
                        let truncated_files: Vec<_> = filtered_view
                            .files
                            .iter()
                            .take(max_files)
                            .cloned()
                            .collect();
                        let truncated_view = crate::live_index::query::RepoOutlineView {
                            total_files: filtered_view.total_files,
                            total_symbols: filtered_view.total_symbols,
                            files: truncated_files,
                        };
                        let mut output =
                            format::repo_outline_view(&truncated_view, &self.project_name);
                        output.push_str(&format!(
                            "\n\n... and {} more files (increase max_files= to see more)",
                            remaining
                        ));
                        output
                    } else {
                        format::repo_outline_view(&filtered_view, &self.project_name)
                    }
                } else {
                    let max_files = params.0.max_files.unwrap_or(200) as usize;
                    if view.files.len() > max_files {
                        let truncated_files: Vec<_> =
                            view.files.iter().take(max_files).cloned().collect();
                        let remaining = view.files.len() - max_files;
                        let truncated_view = crate::live_index::query::RepoOutlineView {
                            total_files: view.total_files,
                            total_symbols: view.total_symbols,
                            files: truncated_files,
                        };
                        let mut output =
                            format::repo_outline_view(&truncated_view, &self.project_name);
                        output.push_str(&format!(
                            "\n\n... and {} more files (use path= to scope or increase max_files=)",
                            remaining
                        ));
                        output
                    } else {
                        format::repo_outline_view(&view, &self.project_name)
                    }
                }
            }
            "tree" => {
                let published = self.index.published_state();
                if let Some(message) = loading_guard_message_from_published(&published) {
                    return message;
                }
                let path = params.0.path.as_deref().unwrap_or("");
                let depth = params.0.depth.unwrap_or(2).min(5);
                let view = self.index.published_repo_outline();
                let guard = self.index.read();
                let skipped = guard.skipped_files().to_vec();
                drop(guard);
                format::file_tree_view_with_skipped(&view.files, &skipped, path, depth)
            }
            _ => {
                let guard = self.index.read();
                loading_guard!(guard);
                drop(guard);

                let state = sidecar_state_for_server(self);
                match repo_map_text(&state) {
                    Ok(result) => {
                        let hint = format::compact_next_step_hint(&[
                            "get_file_context (open one file)",
                            "search_symbols (find by name)",
                            "search_text (find text/patterns)",
                            "diff_symbols (review changes)",
                        ]);
                        format!("{result}{hint}")
                    }
                    Err(StatusCode::NOT_FOUND) => "Repository map unavailable.".to_string(),
                    Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                        "Repository map failed: internal error.".to_string()
                    }
                    Err(other) => format!("Repository map failed: HTTP {}", other.as_u16()),
                }
            }
        };
        self.session_context.record_summary_output(
            "get_repo_map",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        format::enforce_token_budget(output, params.0.max_tokens)
    }

    /// Rich file summary: symbol outline, imports, consumers, references, and git activity.
    /// Use sections=['outline'] for a compact symbol outline only.
    /// Use sections=['outline','imports'] to limit output. Best tool for understanding a file before editing.
    /// Much smaller than reading the raw file.
    /// NOT for reading actual source code (use get_file_content or get_symbol).
    #[tool(
        description = "Prefer this over raw file reads for code understanding — it usually saves 70-95% of tokens by returning the file's symbol outline and structure first. Rich file summary: symbol outline, imports, consumers, references, and git activity. Use sections=['outline'] for a fast first pass, or sections=['outline','imports'] to stay compact. Best tool for understanding a file before editing or before deciding whether you need exact raw text. NOT for reading actual source code (use get_file_content or get_symbol).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn get_file_context(&self, params: Parameters<GetFileContextInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_context", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let guard = self.index.read();
            loading_guard!(guard);
            if let Some(file) = guard.capture_shared_file(&params.0.path) {
                let file_tokens = file.content.len() / 4;
                let outline_tokens = file.content.len() / 50; // ~5-10% of raw
                return format!(
                    "Estimate for get_file_context(path=\"{}\"):\n  Outline: ~{} tokens\n  Raw file: ~{} tokens\n  Savings: ~{}%",
                    params.0.path,
                    outline_tokens,
                    file_tokens,
                    if file_tokens > 0 {
                        100 - (outline_tokens * 100 / file_tokens)
                    } else {
                        0
                    }
                );
            } else {
                return format::not_found_file(&params.0.path);
            }
        }
        let raw_chars = {
            let guard = self.index.read();
            loading_guard!(guard);
            let raw = guard
                .capture_shared_file(&params.0.path)
                .map(|f| f.content.len())
                .unwrap_or(0);
            drop(guard);
            raw
        };

        let state = sidecar_state_for_server(self);
        let outline = OutlineParams {
            path: params.0.path.clone(),
            max_tokens: params.0.max_tokens,
            sections: params.0.sections.clone(),
        };
        match outline_tool_text(&state, &outline) {
            Ok(result) => {
                let hint = format::compact_next_step_hint(&[
                    "get_symbol (body)",
                    "find_references (callers/imports)",
                    "search_text (string/pattern usage)",
                    "get_file_content (exact raw text)",
                ]);
                let body = format!("{result}{hint}");
                let saved = raw_chars.saturating_sub(body.len());
                let footer = format::compact_savings_footer(body.len(), raw_chars);
                self.record_read_savings((saved / 4) as u64);
                let output = format!("{body}{footer}");
                self.session_context
                    .record_listed_file(&params.0.path, (output.len() / 4) as u32);
                // Frecency bump — commitment tool. Reached only on the happy
                // path after a successful outline fetch; no-op unless
                // SYMFORGE_FRECENCY=1. See wiki `[[SymForge Frecency-Weighted
                // File Ranking]]` §"Bump hooks".
                self.bump_frecency(&[PathBuf::from(&params.0.path)]);
                output
            }
            Err(StatusCode::NOT_FOUND) => format::not_found_file(&params.0.path),
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                "File context failed: internal error.".to_string()
            }
            Err(other) => format!("File context failed: HTTP {}", other.as_u16()),
        }
    }

    /// Symbol usage analysis with three modes. (1) Default: definition + callers grouped by file + callees + type usages.
    /// (2) bundle=true: symbol body + full definitions of all referenced custom types, resolved recursively — best
    /// for edit preparation (requires path). (3) sections=[...]: comprehensive trace analysis — definition, callers,
    /// callees, implementations, type dependencies, git activity. Valid sections: 'dependents', 'siblings',
    /// 'implementations', 'git' (empty array = all). Set verbosity='signature' for ~80% smaller output.
    /// NOT for just the symbol body (use get_symbol).
    #[tool(
        description = "Symbol usage analysis with three modes. (1) Default: definition + callers grouped by file + callees + type usages. (2) bundle=true: symbol body + full definitions of all referenced custom types, resolved recursively — best for edit preparation (requires path). (3) sections=[...]: comprehensive trace analysis — definition, callers, callees, implementations, type dependencies, git activity. Valid sections: 'dependents', 'siblings', 'implementations', 'git' (empty array = all). Set verbosity='signature' for ~80% smaller output. NOT for just the symbol body (use get_symbol).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn get_symbol_context(
        &self,
        params: Parameters<GetSymbolContextInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbol_context", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let path = params.0.path.as_deref().unwrap_or("");
            let guard = self.index.read();
            loading_guard!(guard);
            if let Some(file) = guard.capture_shared_file(path) {
                let sym_tokens = file
                    .symbols
                    .iter()
                    .find(|s| s.name == params.0.name)
                    .map(|s| (s.byte_range.1 - s.byte_range.0) as usize / 4)
                    .unwrap_or(0);
                let ref_count = file
                    .references
                    .iter()
                    .filter(|r| r.name == params.0.name)
                    .count();
                let bundle_est = sym_tokens * 3; // rough: body + deps
                return format!(
                    "Estimate for get_symbol_context(name=\"{}\"):\n  Symbol body: ~{} tokens\n  Callers: ~{} tokens\n  Bundle: ~{} tokens",
                    params.0.name,
                    sym_tokens,
                    ref_count * 15 + 50,
                    bundle_est
                );
            } else {
                return format!("Symbol '{}' not found in '{}'", params.0.name, path);
            }
        }
        if params.0.bundle.unwrap_or(false) {
            let path = match params.0.path.as_deref() {
                Some(p) => p.to_string(),
                None => return "Error: bundle=true requires the 'path' parameter.".to_string(),
            };
            let refreshed =
                freshen_exact_path_for_targeted_retrieval(self, &search::PathScope::exact(&path));
            let (view, raw_chars, parse_state) = {
                let guard = self.index.read();
                loading_guard!(guard);
                let raw = guard
                    .capture_shared_file(&path)
                    .map(|f| f.content.len())
                    .unwrap_or(0);
                let parse_state = guard
                    .capture_shared_file(&path)
                    .map(|f| parse_state_for_file(f.as_ref()))
                    .unwrap_or("parsed");
                let v = guard.capture_context_bundle_view(
                    &path,
                    &params.0.name,
                    params.0.symbol_kind.as_deref(),
                    params.0.symbol_line,
                );
                (v, raw, parse_state)
            };
            let verbosity = params.0.verbosity.as_deref().unwrap_or("full");
            let result = format::context_bundle_result_view_with_max_tokens(
                &view,
                verbosity,
                params.0.max_tokens,
            );
            let result = if let crate::live_index::ContextBundleView::Found(found) = &view {
                let scope = match params.0.max_tokens {
                    Some(max_tokens) => format!(
                        "path `{}`; bundle mode; max_tokens={max_tokens}",
                        found.file_path
                    ),
                    None => format!("path `{}`; bundle mode", found.file_path),
                };
                let envelope = search_format::format_search_envelope(
                    if params.0.symbol_line.is_some() {
                        "exact"
                    } else {
                        "constrained"
                    },
                    context_source_authority_label(refreshed),
                    parse_state,
                    &context_bundle_completeness_label(found.as_ref(), &result),
                    &scope,
                    &format!(
                        "symbol anchor `{}:{}`",
                        found.file_path,
                        found.line_range.0.saturating_add(1)
                    ),
                );
                format!("{envelope}\n\n{result}")
            } else {
                result
            };
            let saved = raw_chars.saturating_sub(result.len());
            let footer = format::compact_savings_footer(result.len(), raw_chars);
            self.record_read_savings((saved / 4) as u64);
            let output = format!("{result}{footer}");
            self.session_context
                .record_symbol(&path, &params.0.name, (output.len() / 4) as u32);
            // Frecency bump — commitment tool, bundle-mode happy path.
            // No-op unless SYMFORGE_FRECENCY=1.
            self.bump_frecency(&[PathBuf::from(&path)]);
            return output;
        }

        // Trace mode: comprehensive symbol analysis (absorbed from trace_symbol)
        if params.0.sections.is_some() {
            let path = match params.0.path.as_deref() {
                Some(p) => p.to_string(),
                None => return "Error: sections requires the 'path' parameter.".to_string(),
            };

            // Convert sections: Some(empty vec) = all sections (like trace_symbol's None)
            let sections_param = params
                .0
                .sections
                .as_ref()
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

            let trace_input = TraceSymbolInput {
                path: path.clone(),
                name: params.0.name.clone(),
                kind: params.0.symbol_kind.clone(),
                symbol_line: params.0.symbol_line,
                sections: sections_param,
                verbosity: params.0.verbosity.clone(),
                max_tokens: params.0.max_tokens,
                estimate: None,
            };
            let trace_result = self.trace_symbol(Parameters(trace_input)).await;
            self.session_context.record_symbol(
                &path,
                &params.0.name,
                (trace_result.len() / 4) as u32,
            );
            // Frecency bump — commitment tool, trace-mode happy path.
            // No-op unless SYMFORGE_FRECENCY=1.
            self.bump_frecency(&[PathBuf::from(&path)]);
            return format::enforce_token_budget(trace_result, params.0.max_tokens);
        }

        // Default: symbol context mode
        if let Some(result) = self.proxy_tool_call("get_symbol_context", &params.0).await {
            return result;
        }
        let file_path_hint = params.0.path.as_deref().or(params.0.file.as_deref());
        // Auto-resolve path from index when not provided
        let resolved_path: Option<String>;
        let auto_resolved_candidate_count: usize;
        let file_path_hint = if file_path_hint.is_some() {
            resolved_path = None;
            auto_resolved_candidate_count = 0;
            file_path_hint
        } else {
            let guard = self.index.read();
            let candidates = symbol_candidate_paths(&guard, &params.0.name);
            drop(guard);
            auto_resolved_candidate_count = candidates.len();
            if candidates.len() == 1 {
                resolved_path = Some(candidates.into_iter().next().unwrap());
                resolved_path.as_deref()
            } else if candidates.len() > 1 {
                resolved_path = Some(candidates[0].clone());
                resolved_path.as_deref()
            } else {
                resolved_path = None;
                None
            }
        };
        let verbosity = params.0.verbosity.as_deref().unwrap_or("full");
        let max_tokens = params.0.max_tokens;

        // Capture the symbol definition from the index so we can prepend it
        // (the sidecar only returns reference locations, not the definition itself).
        let (symbol_header, impl_block_tip, callees_text, raw_chars) = {
            let guard = self.index.read();
            loading_guard!(guard);

            let file = file_path_hint.and_then(|p| guard.capture_shared_file(p));
            let raw = file.as_ref().map(|f| f.content.len()).unwrap_or(0);

            let header = file.and_then(|f| {
                render_symbol_context_header(
                    f.as_ref(),
                    &params.0.name,
                    params.0.symbol_kind.as_deref(),
                    params.0.symbol_line,
                    verbosity,
                    max_tokens,
                )
            });

            let (impl_block_tip, callees_text) = file_path_hint
                .map(|path| {
                    let view = guard.capture_context_bundle_view(
                        path,
                        &params.0.name,
                        params.0.symbol_kind.as_deref(),
                        params.0.symbol_line,
                    );
                    let tip = format::context_bundle_impl_suggestion_tip(&view);
                    let callees = format::context_bundle_callees_text(&view);
                    (tip, callees)
                })
                .unwrap_or_default();

            (header, impl_block_tip, callees_text, raw)
        };

        let state = sidecar_state_for_server(self);
        let symbol_context = SymbolContextParams {
            name: params.0.name.clone(),
            file: params.0.file.clone(),
            path: params.0.path.clone(),
            symbol_kind: params.0.symbol_kind.clone(),
            symbol_line: params.0.symbol_line,
        };
        match symbol_context_tool_text(&state, &symbol_context) {
            Ok(refs_text) => {
                let mut output = String::new();
                if let Some(header) = &symbol_header {
                    output.push_str(header);
                    output.push_str("\n\n");
                }
                output.push_str(&refs_text);
                if !callees_text.is_empty() {
                    output.push('\n');
                    output.push_str(&callees_text);
                }
                if !impl_block_tip.is_empty() {
                    output.push_str(impl_block_tip.trim_start_matches('\n'));
                }
                output.push_str(&format::compact_next_step_hint(&[
                    "get_symbol_context (callers/callees/types)",
                    "find_references (usages)",
                    "edit_within_symbol / replace_symbol_body (edits)",
                ]));
                // Add disambiguation note when path was auto-resolved from multiple candidates
                if params.0.path.is_none()
                    && params.0.file.is_none()
                    && auto_resolved_candidate_count > 1
                    && let Some(ref resolved) = resolved_path
                {
                    output.push_str(&format!(
                        "\n\nNote: {} symbols named \"{}\" found — showing from {}. Specify path for precision.",
                        auto_resolved_candidate_count, params.0.name, resolved
                    ));
                }
                let saved = raw_chars.saturating_sub(output.len());
                let footer = format::compact_savings_footer(output.len(), raw_chars);
                self.record_read_savings((saved / 4) as u64);
                // Frecency bump — commitment tool, default-mode happy path.
                // Resolve the bump path the same way the output did:
                // explicit params first, auto-resolved path as fallback.
                // No-op unless SYMFORGE_FRECENCY=1.
                if let Some(bump_path) = params
                    .0
                    .path
                    .as_deref()
                    .or(params.0.file.as_deref())
                    .map(PathBuf::from)
                    .or_else(|| resolved_path.as_deref().map(PathBuf::from))
                {
                    self.bump_frecency(&[bump_path]);
                }
                format::enforce_token_budget(format!("{output}{footer}"), max_tokens)
            }
            Err(_) => {
                // Sidecar unavailable — fall back to the index definition so callers
                // always get at least the symbol body instead of an opaque error.
                if let Some(header) = symbol_header {
                    let mut body = format!(
                        "{header}\n\n(Reference data unavailable — showing definition only)"
                    );
                    if !impl_block_tip.is_empty() {
                        body.push('\n');
                        body.push_str(impl_block_tip.trim_start_matches('\n'));
                    }
                    body.push_str(&format::compact_next_step_hint(&[
                        "get_symbol_context (callers/callees/types)",
                        "find_references (usages)",
                    ]));
                    let footer = format::compact_savings_footer(body.len(), raw_chars);
                    let saved = raw_chars.saturating_sub(body.len());
                    self.record_read_savings((saved / 4) as u64);
                    let output = format!("{body}{footer}");
                    if let Some(ref p) = resolved_path {
                        self.session_context.record_symbol(
                            p,
                            &params.0.name,
                            (output.len() / 4) as u32,
                        );
                    } else if let Some(ref p) = params.0.path {
                        self.session_context.record_symbol(
                            p,
                            &params.0.name,
                            (output.len() / 4) as u32,
                        );
                    }
                    // Frecency bump — commitment tool, fallback-definition
                    // branch (sidecar unavailable but we still returned a
                    // useful symbol body to the caller). No-op unless
                    // SYMFORGE_FRECENCY=1.
                    if let Some(bump_path) = params
                        .0
                        .path
                        .as_deref()
                        .or(params.0.file.as_deref())
                        .map(PathBuf::from)
                        .or_else(|| resolved_path.as_deref().map(PathBuf::from))
                    {
                        self.bump_frecency(&[bump_path]);
                    }
                    format::enforce_token_budget(output, max_tokens)
                } else {
                    format!("Symbol \"{}\" not found in index.", params.0.name)
                }
            }
        }
    }

    /// Call AFTER editing a file. Re-reads from disk, updates the index, reports added/removed/modified
    /// symbols and affected dependents. Set include_co_changes=true to also see git temporal coupling data
    /// (files that historically change together with this file). Always call this after making edits.
    #[tool(
        description = "Call AFTER editing a file. Re-reads from disk, updates the index, reports added/removed/modified symbols and affected dependents. Set include_co_changes=true to also see git temporal coupling data (files that historically change together). Always call this after making edits to keep the index current.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn analyze_file_impact(
        &self,
        params: Parameters<AnalyzeFileImpactInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("analyze_file_impact", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let with_co = params.0.include_co_changes.unwrap_or(false);
            let co_limit = params.0.co_changes_limit.unwrap_or(10) as usize;
            let est = 200 + if with_co { co_limit * 13 } else { 0 };
            return format!(
                "Estimate for analyze_file_impact: ~{} tokens (include_co_changes={})",
                est, with_co
            );
        }
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }

        let state = sidecar_state_for_server(self);
        let impact = ImpactParams {
            path: params.0.path.clone(),
            new_file: params.0.new_file,
        };
        let mut result = match impact_tool_text(state, &impact).await {
            Ok(result) => result,
            Err(StatusCode::NOT_FOUND) => {
                return format!("File not found on disk: {}", params.0.path);
            }
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                return "Impact analysis failed: internal error.".to_string();
            }
            Err(other) => return format!("Impact analysis failed: HTTP {}", other.as_u16()),
        };

        // Append co-changes if requested
        if params.0.include_co_changes.unwrap_or(false) {
            let temporal = self.index.git_temporal();
            match temporal.state {
                crate::live_index::git_temporal::GitTemporalState::Ready => {
                    let limit = params.0.co_changes_limit.unwrap_or(10) as usize;
                    let path = params.0.path.as_str();
                    match temporal.files.get(path) {
                        Some(history) => {
                            result.push_str("\n\n");
                            result.push_str(&format::co_changes_result_view(path, history, limit));
                        }
                        None => {
                            result.push_str("\n\nNo git co-change data found for this file.");
                        }
                    }
                }
                crate::live_index::git_temporal::GitTemporalState::Pending
                | crate::live_index::git_temporal::GitTemporalState::Computing => {
                    result.push_str(
                        "\n\nGit temporal data is still loading. Co-changes unavailable.",
                    );
                }
                crate::live_index::git_temporal::GitTemporalState::Unavailable(ref reason) => {
                    result.push_str(&format!("\n\nGit temporal data unavailable: {reason}"));
                }
            }
        }

        self.session_context.record_summary_output(
            "analyze_file_impact",
            (result.len() / 4).min(u32::MAX as usize) as u32,
        );
        result
    }

    /// Find symbols by name substring across the project — returns name, kind, file, line range.
    /// Use when you know part of a symbol name but not the file. Supports kind filter, language filter,
    /// and path prefix scope. Query is optional: omit it to browse by kind or path_prefix (at least
    /// one of query, kind, or path_prefix is required).
    /// NOT for text content search (use search_text). NOT for file path search (use search_files).
    #[tool(
        description = "Prefer this before grep when you are looking for a function, class, type, or other symbol by name. Finds symbols across the repository in milliseconds and returns name, kind, file, and line range. Use when you know part of a symbol name but not the file. Supports kind filter, language filter, and path prefix scope. Query is optional — omit it to browse all symbols matching kind/path_prefix (browse mode defaults to limit=20, sorted by path+line). At least one of query, kind, or path_prefix is required. NOT for text content search (use search_text). NOT for file path search (use search_files).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn search_symbols(&self, params: Parameters<SearchSymbolsInput>) -> String {
        let query_str = params.0.query.as_deref().unwrap_or("").trim();
        let is_browse = query_str.is_empty();
        if is_browse && params.0.kind.is_none() && params.0.path_prefix.is_none() {
            return "search_symbols requires at least one of: query, kind, or path_prefix"
                .to_string();
        }
        if let Some(result) = self.proxy_tool_call("search_symbols", &params.0).await {
            return result;
        }
        let options = match search_symbols_options_from_input(&params.0) {
            Ok(options) => options,
            Err(message) => return message,
        };
        let mut result = {
            let guard = self.index.read();
            loading_guard!(guard);
            search::search_symbols_with_options(
                &guard,
                query_str,
                params.0.kind.as_deref(),
                &options,
            )
        };
        // In browse mode, sort by relevance: public symbols first, then kind priority,
        // then penalize very short/common names, then alphabetical tiebreaker.
        if is_browse {
            let scores: Vec<(bool, f32, bool)> = {
                let guard = self.index.read();
                result
                    .hits
                    .iter()
                    .map(|hit| {
                        let is_pub = guard
                            .get_file(&hit.path)
                            .map(|f| LiveIndex::has_pub_symbol(f, &hit.name))
                            .unwrap_or(false);
                        let kind_score = search::symbol_kind_priority(&hit.kind);
                        let is_short_common = hit.name.len() <= 3;
                        (is_pub, kind_score, is_short_common)
                    })
                    .collect()
            };
            let mut indices: Vec<usize> = (0..result.hits.len()).collect();
            indices.sort_by(|&i, &j| {
                scores[j]
                    .0
                    .cmp(&scores[i].0) // public first
                    .then(
                        scores[j]
                            .1
                            .partial_cmp(&scores[i].1) // higher kind priority first
                            .unwrap_or(std::cmp::Ordering::Equal),
                    )
                    .then(scores[i].2.cmp(&scores[j].2)) // non-short names first
                    .then(result.hits[i].name.cmp(&result.hits[j].name)) // alphabetical
            });
            result.hits = indices
                .into_iter()
                .map(|i| result.hits[i].clone())
                .collect();
        }
        let envelope = if result.hits.is_empty() {
            None
        } else {
            let guard = self.index.read();
            Some(search_format::format_search_envelope(
                search_symbols_match_type_label(&result, is_browse),
                "current index",
                search_parse_state_for_paths(
                    &guard,
                    result.hits.iter().map(|hit| hit.path.as_str()),
                ),
                &search_completeness_label(result.overflow_count, 0),
                &search_scope_summary(
                    &options.path_scope,
                    options.language_filter.as_ref(),
                    &options.noise_policy,
                    None,
                    None,
                    false,
                ),
                &search_symbols_evidence(&result),
            ))
        };
        let output = format::search_symbols_result_view(&result, query_str);
        let hint = format::compact_next_step_hint(&[
            "get_symbol (body)",
            "get_symbol_context (callers/callees)",
            "get_file_context (file overview)",
        ]);
        let output = match envelope {
            Some(envelope) => format!("{envelope}\n\n{output}{hint}"),
            None => format!("{output}{hint}"),
        };
        self.record_tool_savings_named(
            "search_symbols",
            (output.len() * 10 / 4) as u64,
            (output.len() / 4) as u64,
        );
        self.session_context.record_summary_output(
            "search_symbols",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        // Session tracking: record each symbol hit as list-only until its body is fetched.
        for hit in &result.hits {
            self.session_context
                .record_listed_symbol(&hit.path, &hit.name);
        }
        // Intentionally NO frecency bump here — search_symbols is a discovery
        // tool. See wiki `[[SymForge Frecency-Weighted File Ranking]]`
        // §"Search tools deliberately do NOT bump" for the positive-feedback-
        // loop rationale.
        format::enforce_token_budget(output, params.0.max_tokens)
    }

    /// Full-text search across file contents — literal, OR-terms, regex, or structural AST patterns.
    /// Shows matches with enclosing symbol context. Use group_by='symbol' to deduplicate,
    /// follow_refs=true to inline callers. Set structural=true to match AST patterns using
    /// ast-grep syntax ($VAR for metavariables, $$$ for multi-node wildcards).
    /// NOT for symbol name search (use search_symbols). NOT for file path search (use search_files).
    #[tool(
        description = "Prefer this over grep/ripgrep for code search — it returns matches with enclosing symbol context instead of raw lines alone. Full-text search across file contents: literal, OR-terms, regex, or structural AST patterns. Use group_by='symbol' to deduplicate and follow_refs=true to inline callers. Set structural=true with query as an ast-grep pattern to match code by AST structure (e.g., 'fn $NAME($$$) { $$$ }'). NOT for symbol name search (use search_symbols). NOT for file path search (use search_files).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn search_text(&self, params: Parameters<SearchTextInput>) -> String {
        if let Some(result) = self.proxy_tool_call("search_text", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let limit = params.0.limit.unwrap_or(50) as usize;
            let per_file = params.0.max_per_file.unwrap_or(5) as usize;
            let est = limit * 20 + 50;
            return format!(
                "Estimate for search_text: ~{} tokens (limit={}, max_per_file={})",
                est, limit, per_file
            );
        }
        // Structural (AST-pattern) search mode.
        if params.0.structural.unwrap_or(false) {
            let pattern = match params.0.query.as_deref() {
                Some(p) if !p.trim().is_empty() => p.trim(),
                _ => return "Error: `query` is required for structural search.".to_string(),
            };
            let options = match search_text_options_from_input(&params.0) {
                Ok(o) => o,
                Err(message) => return message,
            };
            let result = {
                let guard = self.index.read();
                loading_guard!(guard);
                search::search_structural(&guard, pattern, &options)
            };
            let output = render_search_text_output(
                self,
                result,
                params.0.group_by.as_deref(),
                params.0.terms.as_deref(),
                &options,
                false,
                false,
                false,
            );
            let hint = format::compact_next_step_hint(&[
                "inspect_match (deep-dive one hit)",
                "get_file_context (file overview)",
                "search_symbols (name-based lookup)",
            ]);
            let result = format!("{output}{hint}");
            self.session_context.record_summary_output(
                "search_text",
                (result.len() / 4).min(u32::MAX as usize) as u32,
            );
            return format::enforce_token_budget(result, params.0.max_tokens);
        }

        let mut options = match search_text_options_from_input(&params.0) {
            Ok(options) => options,
            Err(message) => return message,
        };
        let mut is_regex = params.0.regex.unwrap_or(false);
        let mut auto_detected_regex = false;
        let original_query = params.0.query.clone();

        // Auto-detect regex patterns: if regex is not explicitly set and the
        // query contains unambiguous regex sequences (\w, \d, \s, \b, .+, .*),
        // enable regex mode automatically. These sequences never appear
        // literally in code, so treating them as literals always gives 0 results.
        if !is_regex && let Some(ref q) = params.0.query {
            let has_regex_escape = q.contains("\\w")
                || q.contains("\\d")
                || q.contains("\\s")
                || q.contains("\\b")
                || q.contains("\\W")
                || q.contains("\\D")
                || q.contains("\\S");
            if has_regex_escape {
                is_regex = true;
                auto_detected_regex = true;
                // Relax noise policy for auto-detected regex — the user
                // is doing a targeted pattern search and expects grep-like
                // completeness, so include test files by default.
                if params.0.include_tests.is_none() {
                    options.noise_policy.include_tests = true;
                }
            }
        }

        // Extract churn scores from GitTemporalIndex BEFORE acquiring the
        // LiveIndex read lock to avoid lock ordering issues.
        if options.ranked {
            let git_temporal = self.index.git_temporal();
            if matches!(
                git_temporal.state,
                crate::live_index::git_temporal::GitTemporalState::Ready
            ) {
                let churn_map: std::collections::HashMap<String, f32> = git_temporal
                    .files
                    .iter()
                    .map(|(path, history)| (path.clone(), history.churn_score))
                    .collect();
                if !churn_map.is_empty() {
                    options.churn_scores = Some(churn_map);
                }
            }
        }

        let result = {
            let guard = self.index.read();
            loading_guard!(guard);
            let mut r = search::search_text_with_options(
                &guard,
                params.0.query.as_deref(),
                params.0.terms.as_deref(),
                is_regex,
                &options,
            );
            // Enrich with callers if follow_refs is set
            if params.0.follow_refs.unwrap_or(false)
                && let Ok(ref mut text_result) = r
            {
                let limit = params.0.follow_refs_limit.unwrap_or(3) as usize;
                enrich_with_callers(&guard, text_result, limit);
            }
            r
        };

        // Auto-correct double-escaped regex patterns: if regex=true and the
        // result is InvalidRegex or Ok-but-empty, try fixing common
        // double-escaped character classes (\\s → \s, \\d → \d, etc.).
        if is_regex {
            let should_retry = match &result {
                Err(search::TextSearchError::InvalidRegex { .. }) => true,
                Ok(r) if r.files.is_empty() => true,
                _ => false,
            };
            if should_retry
                && let Some(ref query) = original_query
                && let Some(fixed) = fix_common_double_escapes(query)
            {
                let retry_result = {
                    let guard = self.index.read();
                    loading_guard!(guard);
                    let mut r = search::search_text_with_options(
                        &guard,
                        Some(fixed.as_str()),
                        params.0.terms.as_deref(),
                        true,
                        &options,
                    );
                    if params.0.follow_refs.unwrap_or(false)
                        && let Ok(ref mut text_result) = r
                    {
                        let limit = params.0.follow_refs_limit.unwrap_or(3) as usize;
                        enrich_with_callers(&guard, text_result, limit);
                    }
                    r
                };
                // Use the retry result if it actually produced matches
                if let Ok(ref retry_ok) = retry_result
                    && !retry_ok.files.is_empty()
                {
                    let mut output = render_search_text_output(
                        self,
                        retry_result,
                        params.0.group_by.as_deref(),
                        params.0.terms.as_deref(),
                        &options,
                        true,
                        auto_detected_regex,
                        true,
                    );
                    output.push_str(&format!(
                        "\n(auto-corrected double-escaped regex: `{}` → `{}`)",
                        query, fixed
                    ));
                    output.push_str(&format::compact_next_step_hint(&[
                        "inspect_match (deep-dive one hit)",
                        "get_file_context (file overview)",
                        "search_symbols (name-based lookup)",
                    ]));
                    self.session_context.record_summary_output(
                        "search_text",
                        (output.len() / 4).min(u32::MAX as usize) as u32,
                    );
                    return output;
                }
            }
        }

        let output = render_search_text_output(
            self,
            result,
            params.0.group_by.as_deref(),
            params.0.terms.as_deref(),
            &options,
            is_regex,
            auto_detected_regex,
            false,
        );
        let hint = format::compact_next_step_hint(&[
            "inspect_match (deep-dive one hit)",
            "get_file_context (file overview)",
            "search_symbols (name-based lookup)",
        ]);
        let result = format!("{output}{hint}");
        self.session_context.record_summary_output(
            "search_text",
            (result.len() / 4).min(u32::MAX as usize) as u32,
        );
        // Intentionally NO frecency bump here — search_text is a discovery
        // tool. Bumping on discovery would create a positive feedback loop
        // (hot files get searched more, which would bump them more, which
        // would rank them higher still). See wiki `[[SymForge Frecency-
        // Weighted File Ranking]]` §"Search tools deliberately do NOT bump".
        format::enforce_token_budget(result, params.0.max_tokens)
    }

    /// Internal: trace_symbol logic, called by get_symbol_context when sections are provided.
    /// Also called directly by daemon backward-compat alias.
    pub(crate) async fn trace_symbol(&self, params: Parameters<TraceSymbolInput>) -> String {
        if let Some(result) = self.proxy_tool_call("trace_symbol", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let section_count = params
                .0
                .sections
                .as_ref()
                .map(|s| if s.is_empty() { 4 } else { s.len() })
                .unwrap_or(4);
            let est = 200 + section_count * 200;
            return format!(
                "Estimate for trace_symbol: ~{} tokens ({} sections)",
                est, section_count
            );
        }

        let mut trace_view = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_trace_symbol_view(
                &params.0.path,
                &params.0.name,
                params.0.kind.as_deref(),
                params.0.symbol_line,
                params.0.sections.as_deref(),
            )
        };

        // Fill in git activity if it was requested (or if all sections requested)
        if let crate::live_index::TraceSymbolView::Found(ref mut found) = trace_view {
            let wants_git = params
                .0
                .sections
                .as_ref()
                .map(|s| s.iter().any(|v| v.eq_ignore_ascii_case("git")))
                .unwrap_or(true);

            if wants_git {
                let temporal = self.index.git_temporal();
                if temporal.state == crate::live_index::git_temporal::GitTemporalState::Ready
                    && let Some(history) = temporal.files.get(&params.0.path)
                {
                    use crate::live_index::git_temporal::{churn_bar, churn_label, relative_time};

                    found.git_activity = Some(crate::live_index::GitActivityView {
                        churn_score: history.churn_score,
                        churn_bar: churn_bar(history.churn_score),
                        churn_label: churn_label(history.churn_score).to_string(),
                        commit_count: history.commit_count,
                        last_relative: relative_time(history.last_commit.days_ago),
                        last_hash: history.last_commit.hash.clone(),
                        last_message: history.last_commit.message_head.clone(),
                        last_author: history.last_commit.author.clone(),
                        last_timestamp: history.last_commit.timestamp.clone(),
                        owners: history
                            .contributors
                            .iter()
                            .map(|c| format!("{} {:.0}%", c.author, c.percentage))
                            .collect(),
                        co_changes: history
                            .co_changes
                            .iter()
                            .map(|e| (e.path.clone(), e.coupling_score, e.shared_commits))
                            .collect(),
                    });
                }
            }
        }

        let verbosity = params.0.verbosity.as_deref().unwrap_or("full");
        let output = format::trace_symbol_result_view(&trace_view, &params.0.name, verbosity, params.0.max_tokens);
        self.session_context.record_symbol(
            &params.0.path,
            &params.0.name,
            (output.len() / 4) as u32,
        );
        output
    }

    /// Inspect a specific line in full symbol context: shows the enclosing symbol, parent chain,
    /// and siblings. Works standalone with just path + line, or after search_text to deep-dive a match.
    #[tool(
        description = "Inspect a specific line in full symbol context: enclosing symbol, parent chain (e.g. module → class → method), and sibling symbols. Works standalone with just path + line number, or after search_text to deep-dive a specific hit. NOT for finding all occurrences of a pattern (use search_text). NOT for understanding a symbol's callers and callees (use get_symbol_context).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn inspect_match(&self, params: Parameters<InspectMatchInput>) -> String {
        if let Some(result) = self.proxy_tool_call("inspect_match", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let context = params.0.context.unwrap_or(3) as usize;
            let siblings = params.0.sibling_limit.unwrap_or(10) as usize;
            let est = (context * 2 + 1) * 20 + siblings * 8 + 80;
            return format!(
                "Estimate for inspect_match: ~{} tokens (context={}, siblings={})",
                est, context, siblings
            );
        }

        let view = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_inspect_match_view(
                &params.0.path,
                params.0.line,
                params.0.context,
                params.0.sibling_limit,
            )
        };

        let output = format::inspect_match_result_view(&view);
        self.session_context.record_summary_output(
            "inspect_match",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        format::enforce_token_budget(output, params.0.max_tokens)
    }

    /// Find files by path, filename, or folder — ranked by relevance. With changed_with=path,
    /// finds co-changing files via git temporal coupling. Set resolve=true for exact path resolution.
    /// NOT for file content search (use search_text). NOT for symbol names (use search_symbols).
    #[tool(
        description = "Find files by path, filename, or folder — ranked by relevance. Modes: (1) default: fuzzy search ranked by relevance, (2) changed_with=path: co-changing files via git temporal coupling, (3) resolve=true: resolve an ambiguous filename or partial path to one exact project path. NOT for file content search (use search_text). NOT for symbol names (use search_symbols).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn search_files(&self, params: Parameters<SearchFilesInput>) -> String {
        if let Some(result) = self.proxy_tool_call("search_files", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let limit = params.0.limit.unwrap_or(20) as usize;
            let est = limit * 10 + 30;
            return format!(
                "Estimate for search_files: ~{} tokens (limit={})",
                est, limit
            );
        }

        // Resolve mode: exact path resolution
        if params.0.resolve.unwrap_or(false) {
            if params.0.query.is_empty() {
                return "search_files with resolve=true requires a non-empty `query`.".to_string();
            }
            let view = {
                let guard = self.index.read();
                loading_guard!(guard);
                guard.capture_search_files_resolve_view(&params.0.query)
            };
            let envelope = match &view {
                SearchFilesResolveView::Resolved { path } => {
                    let guard = self.index.read();
                    Some(search_format::format_search_envelope(
                        search_files_resolve_match_type_label(&view),
                        "current index",
                        search_parse_state_for_paths(&guard, std::iter::once(path.as_str())),
                        "full for current scope",
                        "resolve=true against indexed file paths",
                        &search_paths_evidence(std::iter::once(path.as_str())),
                    ))
                }
                SearchFilesResolveView::Ambiguous {
                    matches,
                    overflow_count,
                    ..
                } => {
                    let guard = self.index.read();
                    Some(search_format::format_search_envelope(
                        search_files_resolve_match_type_label(&view),
                        "current index",
                        search_parse_state_for_paths(
                            &guard,
                            matches.iter().map(std::string::String::as_str),
                        ),
                        &search_completeness_label(*overflow_count, 0),
                        "resolve=true against indexed file paths",
                        &search_paths_evidence(matches.iter().map(std::string::String::as_str)),
                    ))
                }
                _ => None,
            };
            let output = format::search_files_resolve_result_view(&view);
            let result = match envelope {
                Some(envelope) => format!("{envelope}\n\n{output}"),
                None => output,
            };
            let result = format::enforce_token_budget(result, params.0.max_tokens);
        self.session_context.record_summary_output(
                "search_files",
                (result.len() / 4).min(u32::MAX as usize) as u32,
            );
            return result;
        }

        // Handle changed_with (git temporal coupling)
        if let Some(ref target_path) = params.0.changed_with {
            let temporal = self.index.git_temporal();
            match temporal.state {
                crate::live_index::git_temporal::GitTemporalState::Ready => {}
                crate::live_index::git_temporal::GitTemporalState::Unavailable(ref reason) => {
                    return format!("Git temporal data unavailable: {reason}");
                }
                _ => {
                    return "Git temporal data is still loading. Try again in a few seconds."
                        .to_string();
                }
            }
            let commit_count = temporal.stats.total_commits_analyzed;
            if let Some(history) = temporal.files.get(target_path.as_str()) {
                let hits: Vec<SearchFilesHit> = history
                    .co_changes
                    .iter()
                    .map(|entry| SearchFilesHit {
                        tier: SearchFilesTier::CoChange,
                        path: entry.path.clone(),
                        coupling_score: Some(entry.coupling_score),
                        shared_commits: Some(entry.shared_commits),
                    })
                    .collect();
                let weak_hits: Vec<SearchFilesHit> = history
                    .weak_co_changes
                    .iter()
                    .map(|entry| SearchFilesHit {
                        tier: SearchFilesTier::CoChange,
                        path: entry.path.clone(),
                        coupling_score: Some(entry.coupling_score),
                        shared_commits: Some(entry.shared_commits),
                    })
                    .collect();
                if hits.is_empty() {
                    if !weak_hits.is_empty() {
                        let total = weak_hits.len();
                        let mut result =
                            format::search_files_result_view(&SearchFilesView::Found {
                                query: format!("weak co-changes with {target_path}"),
                                total_matches: total,
                                overflow_count: 0,
                                hits: weak_hits,
                            });
                        let envelope = {
                            let guard = self.index.read();
                            search_format::format_search_envelope(
                                "weak heuristic (git-temporal coupling)",
                                "current index + git temporal",
                                search_parse_state_for_paths(
                                    &guard,
                                    history
                                        .weak_co_changes
                                        .iter()
                                        .map(|entry| entry.path.as_str()),
                                ),
                                "full for current scope",
                                &format!(
                                    "weak git-temporal co-change candidates for `{target_path}`"
                                ),
                                &search_paths_evidence(
                                    history
                                        .weak_co_changes
                                        .iter()
                                        .map(|entry| entry.path.as_str()),
                                ),
                            )
                        };
                        result = format!("{envelope}\n\n{result}");
                        result.push_str(
                            "\n\nLow confidence: these candidates missed the strong threshold (2 shared commits and Jaccard >= 0.15). Use them as hints, not proof.",
                        );
                        if commit_count < 50 {
                            result.push_str(&format!(
                                "\nOnly {commit_count} commit(s) were analyzed, so coupling may strengthen as history grows."
                            ));
                        }
                        return result;
                    }
                    return if commit_count < 50 {
                        format!(
                            "No high-confidence co-change data for '{target_path}'. Only {commit_count} commit(s) analyzed — strong co-change needs at least 2 shared commits and Jaccard >= 0.15. Results improve with more history."
                        )
                    } else {
                        format!(
                            "No high-confidence co-change data for '{target_path}' (analyzed {commit_count} commits). This file may change independently of others or only have weak coupling signals."
                        )
                    };
                }
                let total = hits.len();
                let mut result = format::search_files_result_view(&SearchFilesView::Found {
                    query: format!("co-changes with {target_path}"),
                    total_matches: total,
                    overflow_count: 0,
                    hits,
                });
                let envelope = {
                    let guard = self.index.read();
                    search_format::format_search_envelope(
                        "heuristic (git-temporal coupling)",
                        "current index + git temporal",
                        search_parse_state_for_paths(
                            &guard,
                            history.co_changes.iter().map(|entry| entry.path.as_str()),
                        ),
                        "full for current scope",
                        &format!("git-temporal co-changes for `{target_path}`"),
                        &search_paths_evidence(
                            history.co_changes.iter().map(|entry| entry.path.as_str()),
                        ),
                    )
                };
                result = format!("{envelope}\n\n{result}");
                if commit_count < 50 {
                    result.push_str(&format!(
                        "\n\n⚠ Low confidence: only {commit_count} commit(s) analyzed. Coupling scores may shift as history grows."
                    ));
                }
                return result;
            }
            if commit_count < 20 {
                return format!(
                    "'{target_path}' not found in git history ({commit_count} commit(s) analyzed). Co-change analysis needs more history to produce useful results."
                );
            }
            return format!(
                "No git history found for '{target_path}'. Check the file path is correct and that it has been committed."
            );
        }

        if params.0.query.is_empty() {
            return "search_files requires a non-empty `query` (or use `changed_with` to find co-changing files).".to_string();
        }

        let mut view = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_search_files_view(
                &params.0.query,
                params.0.limit.unwrap_or(20) as usize,
                params.0.current_file.as_deref(),
            )
        };
        // Optional frecency-fusion rerank. Activated only when the caller asked
        // for `rank_by="frecency"` AND the `SYMFORGE_FRECENCY=1` flag is on.
        // Any failure to open the on-disk store silently falls back to the
        // tier-based ordering — the feature must never break `search_files`.
        if params.0.rank_by.as_deref() == Some("frecency")
            && std::env::var("SYMFORGE_FRECENCY").as_deref() == Ok("1")
            && let SearchFilesView::Found { hits, .. } = &mut view
        {
            if let Some(repo_root) = self.capture_repo_root() {
                let db_path = repo_root.join(crate::paths::SYMFORGE_FRECENCY_DB_PATH);
                if let Ok(store) = crate::live_index::frecency::FrecencyStore::open(&db_path) {
                    let hit_paths: Vec<std::path::PathBuf> =
                        hits.iter().map(|h| std::path::PathBuf::from(&h.path)).collect();
                    let path_refs: Vec<&std::path::Path> =
                        hit_paths.iter().map(|p| p.as_path()).collect();
                    let now_ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    if let Ok(scores) = store.bulk_scores(&path_refs, now_ts) {
                        let breakdowns =
                            crate::live_index::search::score_hits_by_frecency_fusion(
                                hits, &scores,
                            );
                        let taken = std::mem::take(hits);
                        *hits = crate::live_index::search::reorder_hits_by_frecency_fusion(
                            taken,
                            &breakdowns,
                        );
                    }
                }
            }
        }
        let envelope = match &view {
            SearchFilesView::Found {
                hits,
                overflow_count,
                ..
            } => {
                let guard = self.index.read();
                let scope = match params.0.current_file.as_deref() {
                    Some(current_file) => {
                        format!("ranked indexed file paths; current file boost `{current_file}`")
                    }
                    None => "ranked indexed file paths".to_string(),
                };
                Some(search_format::format_search_envelope(
                    search_files_match_type_label(&view),
                    "current index",
                    search_parse_state_for_paths(&guard, hits.iter().map(|hit| hit.path.as_str())),
                    &search_completeness_label(*overflow_count, 0),
                    &scope,
                    &search_paths_evidence(hits.iter().map(|hit| hit.path.as_str())),
                ))
            }
            _ => None,
        };
        let output = format::search_files_result_view(&view);
        let result = match envelope {
            Some(envelope) => format!("{envelope}\n\n{output}"),
            None => output,
        };
        self.session_context.record_summary_output(
            "search_files",
            (result.len() / 4).min(u32::MAX as usize) as u32,
        );
        // Intentionally NO frecency bump here — search_files is a discovery
        // tool, and bumping on discovery creates a positive feedback loop
        // where files that rank high are searched more, which makes them
        // rank even higher. Bumps fire only on commitment (get_file_context,
        // get_file_content, get_symbol, get_symbol_context, and the 7 edit
        // tools via EditHook). See wiki `[[SymForge Frecency-Weighted File
        // Ranking]]` §"Search tools deliberately do NOT bump".
        result
    }

    /// Diagnostic: index status, file/symbol counts, load time, watcher state, token savings,
    /// hook adoption metrics, git temporal status. Always responds even during loading. Use to
    /// verify SymForge is working.
    #[tool(
        description = "Diagnostic: index status, file/symbol counts, load time, watcher state, token savings, hook adoption metrics, git temporal status. Always responds even during loading. Use to verify SymForge is working. NOT for diagnosing a specific file or symbol (use get_file_context or get_symbol).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn health(&self) -> String {
        if let Some(result) = self.proxy_tool_call_without_params("health").await {
            return result;
        }
        let published = self.index.published_state();
        let watcher_guard = self.watcher_info.lock();
        let mut result = format::health_report_from_published_state(&published, &watcher_guard);

        // Append token savings section if the sidecar's TokenStats are available.
        if let Some(ref stats) = self.token_stats {
            let snap = stats.summary();
            let savings = format::format_token_savings(&snap);
            if !savings.is_empty() {
                result.push('\n');
                result.push_str(&savings);
            }

            // Append per-tool call counts.
            let counts = stats.tool_call_counts();
            let counts_section = format::format_tool_call_counts(&counts);
            if !counts_section.is_empty() {
                result.push('\n');
                result.push_str(&counts_section);
            }

            // Append per-tool token efficiency breakdown.
            let token_details = stats.tool_token_details();
            let breakdown = format::format_tool_token_breakdown(&token_details);
            if !breakdown.is_empty() {
                result.push('\n');
                result.push_str(&breakdown);
            }
        }

        let adoption =
            crate::cli::hook::load_hook_adoption_snapshot(self.capture_repo_root().as_deref());
        let adoption_section = format::format_hook_adoption(&adoption);
        if !adoption_section.is_empty() {
            result.push('\n');
            result.push_str(&adoption_section);
        }

        // Append git temporal summary.
        result.push('\n');
        result.push_str(&format::git_temporal_health_line(
            &self.index.git_temporal(),
        ));

        // Append worktree-awareness misuse counter (rolling last-hour window).
        result.push('\n');
        result.push_str(&format!(
            "── Worktree-awareness misuse ──\nedit tool calls without working_directory (last hour): {}",
            self.worktree_misuse.current_window_count(),
        ));

        // Append frecency diagnostics when SYMFORGE_FRECENCY=1. The feature-flag
        // guard mirrors the one in `frecency::bump`; when the flag is unset,
        // the health output is byte-identical to pre-frecency releases.
        // See `docs/decisions/0011-frecency-bump-policy.md` for the visibility
        // rationale (Implementation Notes §Visibility: last-10 bumps in `health`).
        if std::env::var(crate::live_index::frecency::FRECENCY_FLAG_ENV).as_deref() == Ok("1")
            && let Some(repo_root) = self.capture_repo_root()
            && let Ok(store) = crate::live_index::frecency::FrecencyStore::open(
                &repo_root.join(crate::paths::SYMFORGE_FRECENCY_DB_PATH),
            )
        {
            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if let Ok(top) = store.top_frecent(10, now_ts) {
                result.push('\n');
                result.push_str(&format::format_frecency_top(&top));
            }
            // "Last 10 frecency bumps" is additionally gated on
            // SYMFORGE_DEBUG_RANKING=1 per CONTEXT.md §Scope — debug-only
            // surface for ranker tuning, not the default health view.
            if std::env::var("SYMFORGE_DEBUG_RANKING").as_deref() == Ok("1")
                && let Ok(last) = store.last_10_bumps()
            {
                result.push('\n');
                result.push_str(&format::format_frecency_last_bumps(&last));
            }
        }

        self.session_context
            .record_summary_output("health", (result.len() / 4).min(u32::MAX as usize) as u32);
        result
    }

    /// Reindex a directory from scratch — replaces the current index, restarts watcher, triggers
    /// git temporal analysis. Use when switching projects. Destructive to current index.
    #[tool(
        description = "Reindex a directory from scratch — replaces the current index, restarts watcher, triggers git temporal analysis. Use when switching projects. Destructive to current index. NOT for re-reading a single changed file (use analyze_file_impact). NOT for reading content from an existing index (use get_file_content).",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn index_folder(&self, params: Parameters<IndexFolderInput>) -> String {
        if let Some(result) = self.proxy_tool_call("index_folder", &params.0).await {
            // The daemon has rebound the session to the new project. Update our
            // local repo_root so that local-fallback tools (what_changed,
            // analyze_file_impact) and ensure_local_index use the correct root
            // if the daemon connection degrades later.
            if result.starts_with("Indexed ") {
                let new_root = PathBuf::from(&params.0.path);
                self.set_repo_root(Some(new_root));
            }
            return result;
        }
        let root = PathBuf::from(&params.0.path);
        if !root.exists() {
            return format!("Path does not exist: {}", params.0.path);
        }
        if !root.is_dir() {
            return format!("Path is not a directory: {}", params.0.path);
        }
        // Trust boundary: canonicalize and reject sensitive system paths.
        let root = match root.canonicalize() {
            Ok(p) => p,
            Err(e) => return format!("Cannot resolve path: {e}"),
        };
        if is_sensitive_path(&root) {
            return format!(
                "Refused to index sensitive system path: {}.                  Use a project directory instead.",
                root.display()
            );
        }
        let index = Arc::clone(&self.index);
        let reload_root = root.clone();
        match tokio::task::spawn_blocking(move || index.reload(&reload_root)).await {
            Ok(Ok(())) => {
                let published = self.index.published_state();
                let file_count = published.file_count;
                let symbol_count = published.symbol_count;

                self.set_repo_root(Some(root.clone()));

                // Restart the file watcher at the new root so freshness continues.
                crate::watcher::restart_watcher(
                    root.clone(),
                    Arc::clone(&self.index),
                    Arc::clone(&self.watcher_info),
                );
                tracing::info!(root = %root.display(), "file watcher restarted after index_folder");

                // Refresh git temporal data for the new root.
                crate::live_index::git_temporal::spawn_git_temporal_computation(
                    Arc::clone(&self.index),
                    root,
                );

                let output = format!("Indexed {} files, {} symbols.", file_count, symbol_count);
                self.session_context.record_summary_output(
                    "index_folder",
                    (output.len() / 4).min(u32::MAX as usize) as u32,
                );
                output
            }
            Ok(Err(e)) => format!("Index failed: {e}"),
            Err(join_err) => format!("Index failed: reload task panicked: {join_err}"),
        }
    }

    /// List changed files: uncommitted=true for working tree, git_ref for ref comparison, since for
    /// timestamp filter. Use to see what files changed.
    /// Set code_only=true to exclude non-source files (docs, configs, lock files).
    /// Set include_symbol_diff=true to also get a symbol-level diff in the same response (git modes only).
    #[tool(
        description = "List changed files: uncommitted=true for working tree, git_ref for ref comparison, since for timestamp filter. Filter with path_prefix and/or language. Set code_only=true to exclude non-source files (docs, configs, lock files). Set include_symbol_diff=true to also include a symbol-level diff in the same response (git_ref and uncommitted modes only), saving a round-trip vs calling diff_symbols separately. NOT for symbol-level change detail alone (use diff_symbols). NOT for finding files by name or content (use search_files or search_text).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn what_changed(&self, params: Parameters<WhatChangedInput>) -> String {
        if let Some(result) = self.proxy_tool_call("what_changed", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let with_diff = params.0.include_symbol_diff.unwrap_or(false);
            let est = if with_diff { 500 } else { 200 };
            return format!(
                "Estimate for what_changed: ~{} tokens (include_symbol_diff={})",
                est, with_diff
            );
        }
        let effective_repo_root = self.effective_repo_root_for_git_tools();
        let requested_git_mode = params.0.uncommitted.unwrap_or(false)
            || params
                .0
                .git_ref
                .as_deref()
                .map(str::trim)
                .is_some_and(|git_ref| !git_ref.is_empty());
        if params.0.since.is_none() && !requested_git_mode && effective_repo_root.is_none() {
            return "No repo root attached; call index_folder(path=...) or pass since=..."
                .to_string();
        }
        let mode = match determine_what_changed_mode(&params.0, effective_repo_root.is_some()) {
            Ok(mode) => mode,
            Err(message) => return message,
        };

        match &mode {
            WhatChangedMode::Timestamp(since_ts) => {
                let view = {
                    let guard = self.index.read();
                    loading_guard!(guard);
                    guard.capture_what_changed_timestamp_view()
                };
                if *since_ts < view.loaded_secs {
                    match filter_paths_by_prefix_and_language(
                        view.paths.clone(),
                        params.0.path_prefix.as_deref(),
                        params.0.language.as_deref(),
                        params.0.code_only.unwrap_or(false),
                    ) {
                        Ok(filtered) => {
                            if filtered.is_empty() {
                                return if view.paths.is_empty() {
                                    "Index is empty — no files tracked.".to_string()
                                } else {
                                    "No indexed files matched the requested filters since the last index load.".to_string()
                                };
                            }
                            let envelope = {
                                let guard = self.index.read();
                                search_format::format_search_envelope(
                                    "exact (timestamp compare)",
                                    what_changed_source_authority_label(&mode),
                                    search_parse_state_for_paths(
                                        &guard,
                                        filtered.iter().map(|path| path.as_str()),
                                    ),
                                    &changed_paths_completeness_label(
                                        view.paths.len(),
                                        filtered.len(),
                                    ),
                                    &what_changed_scope_summary(&params.0, &mode),
                                    &search_paths_evidence(
                                        filtered.iter().map(|path| path.as_str()),
                                    ),
                                )
                            };
                            let output = format::what_changed_paths_result(
                                &filtered,
                                "No indexed files matched the requested filters since the last index load.",
                            );
                            let result = format!("{envelope}\n\n{output}");
                            self.session_context.record_summary_output(
                                "what_changed",
                                (result.len() / 4).min(u32::MAX as usize) as u32,
                            );
                            result
                        }
                        Err(error) => {
                            self.session_context.record_summary_output(
                                "what_changed",
                                (error.len() / 4).min(u32::MAX as usize) as u32,
                            );
                            error
                        }
                    }
                } else {
                    let result = format::what_changed_timestamp_view(&view, *since_ts);
                    self.session_context.record_summary_output(
                        "what_changed",
                        (result.len() / 4).min(u32::MAX as usize) as u32,
                    );
                    result
                }
            }
            WhatChangedMode::Uncommitted => {
                let guard = self.index.read();
                loading_guard!(guard);
                drop(guard);

                let Some(repo_root) = effective_repo_root.as_deref() else {
                    return "Git change detection unavailable; pass `since` for timestamp mode."
                        .to_string();
                };
                let repo = match crate::git::GitRepo::open(repo_root) {
                    Ok(r) => r,
                    Err(e) => return format!("Git change detection failed: {e}"),
                };
                match repo.uncommitted_paths() {
                    Ok(paths) => {
                        let total_paths = paths.len();
                        match filter_paths_by_prefix_and_language(
                            paths,
                            params.0.path_prefix.as_deref(),
                            params.0.language.as_deref(),
                            params.0.code_only.unwrap_or(false),
                        ) {
                            Ok(filtered) => {
                                if filtered.is_empty() {
                                    return if total_paths == 0 {
                                        "No uncommitted changes detected.".to_string()
                                    } else {
                                        "No uncommitted changes matched the requested filters."
                                            .to_string()
                                    };
                                }
                                let include_symbol_diff =
                                    params.0.include_symbol_diff.unwrap_or(false);
                                let envelope = search_format::format_search_envelope(
                                    "exact (uncommitted working tree)",
                                    what_changed_source_authority_label(&mode),
                                    what_changed_parse_state_label(&mode, include_symbol_diff),
                                    &changed_paths_completeness_label(total_paths, filtered.len()),
                                    &what_changed_scope_summary(&params.0, &mode),
                                    &search_paths_evidence(
                                        filtered.iter().map(|path| path.as_str()),
                                    ),
                                );
                                let mut output = format::what_changed_paths_result(
                                    &filtered,
                                    "No uncommitted changes detected.",
                                );
                                if include_symbol_diff {
                                    let changed_refs: Vec<&str> =
                                        filtered.iter().map(|s| s.as_str()).collect();
                                    let sym_diff = format::diff_symbols_result_view(
                                        "HEAD",
                                        "",
                                        &changed_refs,
                                        &repo,
                                        true,
                                        false,
                                    );
                                    output.push_str("\n\n");
                                    output.push_str(&sym_diff);
                                }
                                let result = format!("{envelope}\n\n{output}");
                                self.session_context.record_summary_output(
                                    "what_changed",
                                    (result.len() / 4).min(u32::MAX as usize) as u32,
                                );
                                format::enforce_token_budget(result, params.0.max_tokens)
                            }
                            Err(e) => e,
                        }
                    }
                    Err(e) => format!("Git change detection failed: {e}"),
                }
            }
            WhatChangedMode::GitRef(git_ref) => {
                let guard = self.index.read();
                loading_guard!(guard);
                drop(guard);

                let Some(repo_root) = effective_repo_root.as_deref() else {
                    return "Git change detection unavailable; pass `since` for timestamp mode."
                        .to_string();
                };
                let repo = match crate::git::GitRepo::open(repo_root) {
                    Ok(r) => r,
                    Err(e) => return format!("Git change detection failed: {e}"),
                };
                match repo.changed_paths_from_ref(git_ref) {
                    Ok(paths) => {
                        let total_paths = paths.len();
                        match filter_paths_by_prefix_and_language(
                            paths,
                            params.0.path_prefix.as_deref(),
                            params.0.language.as_deref(),
                            params.0.code_only.unwrap_or(false),
                        ) {
                            Ok(filtered) => {
                                if filtered.is_empty() {
                                    return if total_paths == 0 {
                                        format!(
                                            "No changes detected relative to git ref '{git_ref}'."
                                        )
                                    } else {
                                        format!(
                                            "No changes relative to git ref '{git_ref}' matched the requested filters."
                                        )
                                    };
                                }
                                let include_symbol_diff =
                                    params.0.include_symbol_diff.unwrap_or(false);
                                let envelope = search_format::format_search_envelope(
                                    "exact (git ref diff)",
                                    what_changed_source_authority_label(&mode),
                                    what_changed_parse_state_label(&mode, include_symbol_diff),
                                    &changed_paths_completeness_label(total_paths, filtered.len()),
                                    &what_changed_scope_summary(&params.0, &mode),
                                    &search_paths_evidence(
                                        filtered.iter().map(|path| path.as_str()),
                                    ),
                                );
                                let mut output = format::what_changed_paths_result(
                                    &filtered,
                                    &format!(
                                        "No changes detected relative to git ref '{git_ref}'."
                                    ),
                                );
                                if include_symbol_diff {
                                    let base =
                                        git_ref.strip_prefix("branch:").unwrap_or(git_ref.as_str());
                                    let changed_refs: Vec<&str> =
                                        filtered.iter().map(|s| s.as_str()).collect();
                                    let sym_diff = format::diff_symbols_result_view(
                                        base,
                                        "HEAD",
                                        &changed_refs,
                                        &repo,
                                        true,
                                        false,
                                    );
                                    output.push_str("\n\n");
                                    output.push_str(&sym_diff);
                                }
                                let result = format!("{envelope}\n\n{output}");
                                self.session_context.record_summary_output(
                                    "what_changed",
                                    (result.len() / 4).min(u32::MAX as usize) as u32,
                                );
                                result
                            }
                            Err(e) => e,
                        }
                    }
                    Err(e) => format!("Git change detection failed: {e}"),
                }
            }
        }
    }

    /// Read raw file content. Modes: full file, line range, around_line/around_match/around_symbol,
    /// or chunked paging. Use this for exact docs/config reads, whitespace-sensitive inspection,
    /// or when you need actual source text that other tools don't provide.
    /// For structured understanding use get_file_context. For a single function
    /// body use get_symbol.
    #[tool(
        description = "Read exact raw file content. Modes: full file, line range, around_line/around_match/around_symbol, or chunked paging. Use this for exact docs/config reads, whitespace-sensitive inspection, or exact source excerpts after narrowing with get_file_context, search_text, or get_symbol. For structured code understanding use get_file_context first. For a single function body use get_symbol. Accepts offset/limit (Read-tool idiom) as aliases for start_line/end_line. Unknown fields are rejected with an error naming the invalid param. Responses are capped at ~60 KB; if truncated, a footer suggests chunk_index+max_lines, around_line, or around_symbol.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn get_file_content(&self, params: Parameters<GetFileContentInput>) -> String {
        let mut input = params.0;
        input.path = normalize_exact_path(&input.path);

        if let Err(e) = normalize_file_content_aliases(&mut input) {
            return e;
        }

        if let Some(result) = self.proxy_tool_call("get_file_content", &input).await {
            return format::cap_file_content_output(result);
        }
        // Estimate mode: return token cost without reading content
        if input.estimate == Some(true) {
            let guard = self.index.read();
            loading_guard!(guard);
            if let Some(file) = guard.capture_shared_file(&input.path) {
                let file_tokens = file.content.len() / 4;
                let line_count = file.content.iter().filter(|&&b| b == b'\n').count();
                return format!(
                    "Estimate for get_file_content(path=\"{}\"):\n  ~{} tokens (~{} lines)",
                    input.path, file_tokens, line_count
                );
            } else {
                return format::not_found_file(&input.path);
            }
        }

        let options = match file_content_options_from_input(&input) {
            Ok(options) => options,
            Err(message) => return message,
        };
        freshen_exact_path_for_targeted_retrieval(self, &options.path_scope);
        let mode_annotation = match (
            &options.content_context.mode_name,
            options.content_context.mode_explicit,
        ) {
            (Some(mode), true) => format!("── mode: {} (explicit) ──\n", mode),
            _ => String::new(),
        };
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file_for_scope(&options.path_scope)
        };
        match file {
            Some(file) => {
                let raw_chars = file.as_ref().content.len();
                let output = format::file_content_from_indexed_file_with_context(
                    file.as_ref(),
                    options.content_context,
                );
                self.record_tool_savings_named(
                    "get_file_content",
                    (raw_chars / 4) as u64,
                    (output.len() / 4) as u64,
                );
                self.session_context
                    .record_file(&input.path, (output.len() / 4) as u32);
                // Frecency bump — commitment tool. Indexed-file branch;
                // no-op unless SYMFORGE_FRECENCY=1. See wiki
                // `[[SymForge Frecency-Weighted File Ranking]]` §"Bump hooks".
                self.bump_frecency(&[PathBuf::from(&input.path)]);
                format::cap_file_content_output(format!("{}{}", mode_annotation, output))
            }
            None => {
                // Not in index — try raw disk read for non-source files
                // (Cargo.toml, package.json, workflow YAMLs, etc.)
                if let Some(root) = self.capture_repo_root() {
                    let canon_path = match edit::safe_repo_path(&root, &input.path) {
                        Ok(p) => p,
                        Err(_) => return format::not_found_file(&input.path),
                    };
                    if canon_path.is_file() {
                        match std::fs::read(&canon_path) {
                            Ok(content) => {
                                let body = format::render_file_content_bytes(
                                    &input.path,
                                    &content,
                                    options.content_context,
                                );
                                // Frecency bump — commitment tool. Raw-disk
                                // fallback branch (non-indexed source files);
                                // no-op unless SYMFORGE_FRECENCY=1.
                                self.bump_frecency(&[PathBuf::from(&input.path)]);
                                return format::cap_file_content_output(format!("{}{}", mode_annotation, body));
                            }
                            Err(e) => {
                                return format!("{} [error: could not read file: {e}]", input.path);
                            }
                        }
                    }
                }
                // Suggest similar files from the index
                let suggestions = {
                    let guard = self.index.read();
                    suggest_similar_files(&guard, &input.path)
                };
                format::not_found_file_with_suggestions(&input.path, &suggestions)
            }
        }
    }

    /// Find all references or implementations for a symbol. Modes: (1) default/references: call sites,
    /// Validate a file's syntax and surface parser diagnostics with exact locations when available.
    /// Best for malformed TOML/JSON/YAML and other config files where you need authoritative parse errors.
    #[tool(
        description = "Validate a file's syntax and surface parser diagnostics with exact locations when available. Best for malformed TOML/JSON/YAML and other config files where you need authoritative parse errors. Uses the indexed file when available and falls back to direct parsing from disk when needed. NOT for understanding file structure (use get_file_context). NOT for searching file content (use search_text).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn validate_file_syntax(
        &self,
        params: Parameters<ValidateFileSyntaxInput>,
    ) -> String {
        let mut input = params.0;
        input.path = normalize_exact_path(&input.path);

        if let Some(result) = self.proxy_tool_call("validate_file_syntax", &input).await {
            return result;
        }

        let path_scope = search::PathScope::exact(&input.path);
        freshen_exact_path_for_targeted_retrieval(self, &path_scope);

        let indexed_file = {
            let guard = self.index.read();
            // Skip loading_guard here — the disk-read fallback below does not
            // need the index, so we should not block when the index is still loading.
            guard.capture_shared_file(&input.path)
        };
        if let Some(file) = indexed_file {
            let output = format::validate_file_syntax_result(&input.path, file.as_ref());
            self.session_context.record_summary_output(
                "validate_file_syntax",
                (output.len() / 4).min(u32::MAX as usize) as u32,
            );
            return output;
        }

        let extension = std::path::Path::new(&input.path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let Some(language) = LanguageId::from_extension(extension) else {
            return format!(
                "Syntax validation unavailable for {}: unsupported or unknown file extension.",
                input.path
            );
        };

        let Some(root) = self.capture_repo_root() else {
            return format::not_found_file(&input.path);
        };
        let canon_path = match edit::safe_repo_path(&root, &input.path) {
            Ok(path) => path,
            Err(_) => return format::not_found_file(&input.path),
        };
        if !canon_path.is_file() {
            return format::not_found_file(&input.path);
        }

        let bytes = match std::fs::read(&canon_path) {
            Ok(bytes) => bytes,
            Err(e) => return format!("{} [error: could not read file: {e}]", input.path),
        };
        let classification = crate::domain::FileClassification::for_code_path(&input.path);
        let result = crate::parsing::process_file_with_classification(
            &input.path,
            &bytes,
            language,
            classification,
        );
        let file = IndexedFile::from_parse_result(result, bytes);
        let output = format::validate_file_syntax_result(&input.path, &file);
        self.session_context.record_summary_output(
            "validate_file_syntax",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        output
    }

    /// Find all references or implementations for a symbol. Modes: (1) default/references: call sites,
    /// imports, type usages grouped by file - set compact=true for ~60-75% smaller output. (2) mode='implementations':
    /// find trait/interface implementors bidirectionally - set direction='trait'/'type'/'auto'.
    /// Use when you need 'who calls this?' or 'who implements this?'
    /// NOT for file-level dependencies (use find_dependents).
    /// NOT for full refactoring context (use get_symbol_context with sections=[...]).
    #[tool(
        description = "Find all references or implementations for a symbol. Modes: (1) default/references: call sites, imports, type usages grouped by file - set compact=true for ~60-75% smaller output. (2) mode='implementations': find trait/interface implementors bidirectionally - set direction='trait'/'type'/'auto'. Use when you need 'who calls this?' or 'who implements this?' NOT for file-level dependencies (use find_dependents). NOT for full refactoring context (use get_symbol_context with sections=[...]).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn find_references(&self, params: Parameters<FindReferencesInput>) -> String {
        if let Some(result) = self.proxy_tool_call("find_references", &params.0).await {
            return result;
        }
        let input = &params.0;
        let mode = input.mode.as_deref().unwrap_or("references");

        if mode == "implementations" {
            let view = {
                let guard = self.index.read();
                loading_guard!(guard);
                guard.capture_implementations_view(&input.name, input.direction.as_deref())
            };
            let cap = input.limit.unwrap_or(200).min(500);
            let limits = format::OutputLimits::new(cap, cap);
            let envelope = if !view.entries.is_empty() {
                let guard = self.index.read();
                Some(search_format::format_search_envelope(
                    find_references_match_type_label(input, mode),
                    "current index",
                    implementations_parse_state_for_paths(&guard, &view),
                    &implementations_completeness_label(&view, &limits),
                    &find_references_scope_summary(input, mode),
                    &implementations_evidence(&view),
                ))
            } else {
                None
            };
            let result = format::implementations_result_view(&view, &input.name, &limits);
            if view.entries.is_empty() {
                let guard = self.index.read();
                let is_concrete = guard.all_files().any(|(_, file)| {
                    file.symbols.iter().any(|s| {
                        s.name == input.name
                            && matches!(
                                s.kind,
                                crate::domain::SymbolKind::Class
                                    | crate::domain::SymbolKind::Struct
                                    | crate::domain::SymbolKind::Enum
                            )
                    })
                });
                if is_concrete {
                    drop(guard);
                    return format!(
                        "No implementations found for \"{}\" — it is a class/struct, not an \
                         interface/trait.\nUse find_references with mode=\"references\" to find \
                         callers and usages instead.",
                        input.name
                    );
                }
                // Check if the symbol exists at all in the indexed project
                let exists_in_project = guard
                    .all_files()
                    .any(|(_, file)| file.symbols.iter().any(|s| s.name == input.name));
                drop(guard);
                if !exists_in_project {
                    return format!(
                        "No implementations found for \"{}\" — this symbol is not defined \
                         in the indexed project (likely from an external dependency).\n\
                         Use search_text to find usages of this symbol in your code instead.",
                        input.name
                    );
                }
            }
            return match envelope {
                Some(envelope) => format!("{envelope}\n\n{result}"),
                None => result,
            };
        }

        let limits =
            format::OutputLimits::new(input.limit.unwrap_or(20), input.max_per_file.unwrap_or(10));
        let result = {
            let guard = self.index.read();
            loading_guard!(guard);
            if let Some(path) = input.path.as_deref() {
                guard.capture_find_references_view_for_symbol(
                    path,
                    &input.name,
                    input.symbol_kind.as_deref(),
                    input.symbol_line,
                    input.kind.as_deref(),
                    limits.total_hits,
                )
            } else {
                Ok(guard.capture_find_references_view(
                    &input.name,
                    input.kind.as_deref(),
                    limits.total_hits,
                ))
            }
        };
        match result {
            Ok(view) => {
                let envelope = if !view.files.is_empty() {
                    let guard = self.index.read();
                    Some(search_format::format_search_envelope(
                        find_references_match_type_label(input, mode),
                        "current index",
                        search_parse_state_for_paths(
                            &guard,
                            view.files.iter().map(|file| file.file_path.as_str()),
                        ),
                        &find_references_completeness_label(&view, &limits),
                        &find_references_scope_summary(input, mode),
                        &find_references_evidence(&view),
                    ))
                } else {
                    None
                };
                let mut output = if input.compact.unwrap_or(false) {
                    format::find_references_compact_view(&view, &input.name, &limits)
                } else {
                    format::find_references_result_view(&view, &input.name, &limits)
                };

                // Supplemental: if index-based refs are empty, try text search to catch
                // qualified-path calls (e.g., module::func()) that the xref extractor misses.
                // This aligns find_references results with what search_text(follow_refs=true) finds.
                if view.files.is_empty() {
                    let text_options = search::TextSearchOptions::for_current_code_search();
                    let text_result = {
                        let guard = self.index.read();
                        search::search_text_with_options(
                            &guard,
                            Some(&input.name),
                            None,
                            false,
                            &text_options,
                        )
                    };
                    if let Ok(tr) = text_result
                        && !tr.files.is_empty()
                    {
                        output.push_str(&format!(
                            "\n\nNote: no indexed references found, but search_text found {} file(s) \
                             containing \"{}\". The index may miss qualified-path calls (e.g., \
                             module::{}()). Use search_text(query=\"{}\") for full coverage.",
                            tr.files.len(), input.name, input.name, input.name
                        ));
                    }
                }

                self.record_tool_savings_named(
                    "find_references",
                    (output.len() * 8 / 4) as u64,
                    (output.len() / 4) as u64,
                );
                self.session_context.record_summary_output(
                    "find_references",
                    (output.len() / 4).min(u32::MAX as usize) as u32,
                );
                self.session_context.record_listed_symbol("", &input.name);
                let result = match envelope {
                    Some(envelope) => format!("{envelope}\n\n{output}"),
                    None => output,
                };
                format::enforce_token_budget(result, params.0.max_tokens)
            }
            Err(error) => error,
        }
    }

    /// File-level dependency graph: which files import the given file. Set compact=true for ~60-75%
    /// smaller output. Supports Mermaid/Graphviz output. Use for "what breaks if I change this file?"
    /// NOT for symbol-level references (use find_references).
    /// NOT for git co-change patterns (use analyze_file_impact with include_co_changes=true).
    #[tool(
        description = "File-level dependency graph: which files import the given file. Set compact=true for ~60-75% smaller output. Supports Mermaid/Graphviz output. Use for 'what breaks if I change this file?' NOT for symbol-level references (use find_references). NOT for git co-change patterns (use analyze_file_impact with include_co_changes=true).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn find_dependents(&self, params: Parameters<FindDependentsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("find_dependents", &params.0).await {
            return result;
        }
        let input = &params.0;
        let view = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_find_dependents_view(&input.path)
        };
        let limits =
            format::OutputLimits::new(input.limit.unwrap_or(20), input.max_per_file.unwrap_or(10));
        let fmt = input.format.as_deref().unwrap_or("text");
        let output = match fmt {
            "mermaid" => format::find_dependents_mermaid(&view, &input.path, &limits),
            "dot" => format::find_dependents_dot(&view, &input.path, &limits),
            _ if input.compact.unwrap_or(false) => {
                format::find_dependents_compact_view(&view, &input.path, &limits)
            }
            _ => format::find_dependents_result_view(&view, &input.path, &limits),
        };
        self.session_context.record_summary_output(
            "find_dependents",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        format::enforce_token_budget(output, params.0.max_tokens)
    }

    /// Extract query terms that aren't part of the matched concept key,
    /// filtering out stopwords and short words.
    fn compute_remainder_terms(query: &str, concept_key: &str) -> Vec<String> {
        const STOPWORDS: &[&str] = &[
            "a", "an", "the", "in", "on", "of", "for", "to", "and", "or", "is", "it", "my", "at",
            "by", "do", "no", "so", "up", "if", "with", "from", "this", "that",
        ];
        let key_words: Vec<&str> = concept_key.split_whitespace().collect();
        query
            .split_whitespace()
            .filter(|w| {
                let lower = w.to_ascii_lowercase();
                !key_words.iter().any(|kw| kw.eq_ignore_ascii_case(w))
                    && !STOPWORDS.contains(&lower.as_str())
                    && lower.len() >= 3
            })
            .map(|w| w.to_ascii_lowercase())
            .collect()
    }

    fn explore_symbol_segments(name: &str) -> Vec<String> {
        let mut segments = Vec::new();
        let mut current = String::new();
        for ch in name.chars() {
            let is_separator =
                !ch.is_alphanumeric() || matches!(ch, '_' | ':' | '-' | '/' | '\\' | '.');
            if is_separator {
                if !current.is_empty() {
                    segments.push(current.to_ascii_lowercase());
                    current.clear();
                }
                continue;
            }

            let split_before = ch.is_uppercase()
                && !current.is_empty()
                && current
                    .chars()
                    .last()
                    .is_some_and(|prev| prev.is_lowercase() || prev.is_ascii_digit());
            if split_before {
                segments.push(current.to_ascii_lowercase());
                current.clear();
            }
            current.push(ch);
        }
        if !current.is_empty() {
            segments.push(current.to_ascii_lowercase());
        }

        segments
    }

    fn explore_fallback_symbol_match(name: &str, term: &str) -> bool {
        if name.eq_ignore_ascii_case(term) {
            return true;
        }

        let segments = Self::explore_symbol_segments(name);
        segments.iter().any(|segment| segment == term)
    }

    fn explore_terms_related(lhs: &str, rhs: &str) -> bool {
        if lhs.eq_ignore_ascii_case(rhs) {
            return true;
        }

        let lhs = lhs.to_ascii_lowercase();
        let rhs = rhs.to_ascii_lowercase();
        let shared_prefix = lhs
            .chars()
            .zip(rhs.chars())
            .take_while(|(a, b)| a == b)
            .count();
        shared_prefix >= 5
    }

    fn record_explore_file_signal(
        file_signals: &mut HashMap<String, ExploreFileSignal>,
        path: &str,
        term: &str,
        weight: u64,
    ) {
        let signal = file_signals.entry(path.to_string()).or_default();
        signal.raw_score += weight;
        signal.matched_terms.insert(term.to_string());
    }

    fn derive_explore_cluster(
        index: &crate::live_index::LiveIndex,
        query_terms: &[String],
        file_signals: &HashMap<String, ExploreFileSignal>,
        limit: usize,
    ) -> Option<DerivedExploreCluster> {
        const GENERIC_SYMBOLS: &[&str] = &[
            "build", "create", "error", "get", "handle", "init", "main", "new", "parse", "process",
            "result", "run", "set", "test", "update",
        ];

        if query_terms.len() < 2 {
            return None;
        }

        let mut ranked_files: Vec<(String, u64, usize)> = file_signals
            .iter()
            .filter_map(|(path, signal)| {
                let file = index.get_file(path)?;
                let coverage = signal.matched_terms.len();
                if coverage < 2 {
                    return None;
                }
                let path_penalty = explore_path_penalty(path, Some(&file.classification));
                let score = (signal.raw_score + ((coverage as u64) * (coverage as u64) * 10))
                    * path_penalty;
                Some((path.clone(), score, coverage))
            })
            .collect();
        ranked_files.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked_files.truncate(limit.clamp(1, 3));
        if ranked_files.is_empty() {
            return None;
        }

        let mut candidate_scores: HashMap<String, u64> = HashMap::new();
        for (path, file_score, coverage) in &ranked_files {
            let Some(file) = index.get_file(path) else {
                continue;
            };

            for symbol in &file.symbols {
                let lower = symbol.name.to_ascii_lowercase();
                if lower.len() < 4 || GENERIC_SYMBOLS.contains(&lower.as_str()) {
                    continue;
                }

                let segments = Self::explore_symbol_segments(&symbol.name);
                let overlap = query_terms
                    .iter()
                    .filter(|term| {
                        segments
                            .iter()
                            .any(|segment| Self::explore_terms_related(segment, term))
                    })
                    .count();
                if overlap == 0 {
                    continue;
                }

                let kind_bonus = match symbol.kind.to_string().as_str() {
                    "struct" | "class" | "trait" | "interface" | "enum" => 5,
                    "fn" | "method" => 4,
                    "impl" | "mod" | "module" => 3,
                    _ => 1,
                } as u64;
                let reverse_hits = index
                    .reverse_index
                    .get(&symbol.name)
                    .map(|hits| hits.len())
                    .unwrap_or(0);
                let rarity_bonus = match reverse_hits {
                    0 => 8,
                    1..=2 => 6,
                    3..=5 => 4,
                    6..=10 => 2,
                    _ => 1,
                } as u64;
                let length_bonus = (symbol.name.len().min(24) / 6) as u64;
                let score = *file_score
                    + ((*coverage as u64) * 5)
                    + ((overlap as u64) * 12)
                    + kind_bonus
                    + rarity_bonus
                    + length_bonus;

                let entry = candidate_scores.entry(symbol.name.clone()).or_insert(0);
                if score > *entry {
                    *entry = score;
                }
            }
        }

        let mut ranked_symbols: Vec<(String, u64)> = candidate_scores.into_iter().collect();
        ranked_symbols.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let promoted_symbols: Vec<String> = ranked_symbols
            .into_iter()
            .take(limit.clamp(1, 4))
            .map(|(name, _)| name)
            .collect();
        if promoted_symbols.is_empty() {
            return None;
        }

        Some(DerivedExploreCluster {
            seed_terms: query_terms.to_vec(),
            promoted_symbols,
            seed_files: ranked_files.into_iter().map(|(path, _, _)| path).collect(),
        })
    }

    fn ask_query_tokens(query: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        for raw in query.split_whitespace() {
            let token = raw
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
                .to_string();
            if token.is_empty() {
                continue;
            }
            if token.len() < 2 {
                continue;
            }
            if tokens
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&token))
            {
                continue;
            }
            tokens.push(token);
        }
        tokens
    }

    fn ask_symbol_candidate_tokens(query: &str) -> Vec<String> {
        Self::ask_query_tokens(query)
            .into_iter()
            .filter(|token| {
                let is_symbol_like = token.contains('_')
                    || token.contains("::")
                    || token.chars().skip(1).any(|c| c.is_uppercase());
                is_symbol_like && token.len() >= 4
            })
            .collect()
    }

    fn extract_exact_symbol_understanding_candidate(
        index: &crate::live_index::LiveIndex,
        query: &str,
    ) -> Option<String> {
        const GENERIC_SYMBOLS: &[&str] = &[
            "build", "create", "get", "handle", "init", "main", "new", "parse", "process", "run",
            "set", "test", "update",
        ];

        /// Score a single (path, symbol) match for prominence. Higher = more canonical.
        fn score_match(path: &str, line_start: u32, line_end: u32) -> i32 {
            let mut score = 0i32;
            if path.starts_with("src/") || path.contains("/src/") {
                score += 10;
            }
            let lower = path.to_ascii_lowercase();
            if !lower.contains("test")
                && !lower.contains("vendor")
                && !lower.contains("example")
                && !lower.contains("bench")
            {
                score += 5;
            }
            let span = line_end.saturating_sub(line_start);
            score += ((span / 10) as i32).min(10);
            score
        }

        /// Search `index` for an exact (case-insensitive) match for `token`.
        /// Returns `Some((canonical_name, best_score))` when 1-5 matches are found.
        fn find_token(index: &crate::live_index::LiveIndex, token: &str) -> Option<(String, i32)> {
            let mut matches: Vec<(String, i32)> = Vec::new(); // (canonical_name, score)
            for (path, file) in index.all_files() {
                for symbol in &file.symbols {
                    if symbol.name.eq_ignore_ascii_case(token) {
                        let s =
                            score_match(path.as_str(), symbol.line_range.0, symbol.line_range.1);
                        matches.push((symbol.name.clone(), s));
                    }
                }
            }
            if matches.is_empty() || matches.len() > 5 {
                return None;
            }
            // Pick the match with the highest score; stable (first wins on tie).
            let best = matches
                .into_iter()
                .max_by_key(|(_, score)| *score)
                .expect("non-empty; guarded by is_empty check above");
            Some(best)
        }

        // Collect (canonical_name, score) for each qualifying token.
        let mut candidates: Vec<(String, i32)> = Vec::new();
        let tokens = Self::ask_symbol_candidate_tokens(query);
        for token in &tokens {
            if GENERIC_SYMBOLS.contains(&token.to_ascii_lowercase().as_str()) {
                continue;
            }
            if let Some(hit) = find_token(index, token) {
                candidates.push(hit);
            }
        }

        // Compound token joining: try "token_a_token_b" for adjacent pairs when
        // no single-token candidate was found yet.
        if candidates.is_empty() && tokens.len() >= 2 {
            for window in tokens.windows(2) {
                let joined = format!("{}_{}", window[0], window[1]);
                if GENERIC_SYMBOLS.contains(&joined.to_ascii_lowercase().as_str()) {
                    continue;
                }
                if let Some(hit) = find_token(index, &joined) {
                    candidates.push(hit);
                }
            }
        }

        if candidates.is_empty() {
            return None;
        }

        // Deduplicate by canonical name, keeping the entry with the highest score.
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        candidates.dedup_by(|later, first| {
            // `dedup_by` drops `later` when true is returned.  The sort above orders ascending
            // by name, then descending by score within each name group, so `first` always holds
            // the highest score for a given name — exactly the entry we want to keep.
            later.0.eq_ignore_ascii_case(&first.0)
        });

        // Pick the single candidate with the highest prominence score.
        candidates
            .into_iter()
            .max_by_key(|(_, score)| *score)
            .map(|(name, _)| name)
    }

    fn extract_exact_implementation_understanding_candidate(
        index: &crate::live_index::LiveIndex,
        query: &str,
    ) -> Option<String> {
        const IMPLEMENTATION_CUES: &[&str] = &[
            "type",
            "types",
            "implementation",
            "implementations",
            "implementor",
            "implementors",
            "implementer",
            "implementers",
            "implements",
        ];
        const GENERIC_QUERY_WORDS: &[&str] = &[
            "a",
            "all",
            "an",
            "and",
            "are",
            "describe",
            "does",
            "explain",
            "help",
            "how",
            "main",
            "me",
            "of",
            "the",
            "through",
            "tell",
            "type",
            "types",
            "understand",
            "walk",
            "what",
            "work",
        ];
        let tokens = Self::ask_query_tokens(query);
        if !tokens.iter().any(|token| {
            IMPLEMENTATION_CUES
                .iter()
                .any(|cue| token.eq_ignore_ascii_case(cue))
        }) {
            return None;
        }

        let mut candidates = Vec::new();
        for token in tokens {
            let lower = token.to_ascii_lowercase();
            if GENERIC_QUERY_WORDS.contains(&lower.as_str()) {
                continue;
            }

            if let Some(candidate) = Self::exact_trait_like_symbol_candidate(index, &token) {
                candidates.push(candidate);
                continue;
            }

            if lower.ends_with('s') && lower.len() > 4 {
                let singular = &token[..token.len() - 1];
                if let Some(candidate) = Self::exact_trait_like_symbol_candidate(index, singular) {
                    candidates.push(candidate);
                }
            }
        }

        candidates.sort();
        candidates.dedup();
        if candidates.len() == 1 {
            candidates.into_iter().next()
        } else {
            None
        }
    }

    fn exact_trait_like_symbol_candidate(
        index: &crate::live_index::LiveIndex,
        token: &str,
    ) -> Option<String> {
        let mut exact_match_count = 0usize;
        let mut canonical_name: Option<String> = None;
        for (_path, file) in index.all_files() {
            for symbol in &file.symbols {
                if !symbol.name.eq_ignore_ascii_case(token) {
                    continue;
                }
                if !matches!(
                    symbol.kind,
                    crate::domain::index::SymbolKind::Trait
                        | crate::domain::index::SymbolKind::Interface
                        | crate::domain::index::SymbolKind::Type
                ) {
                    continue;
                }
                exact_match_count += 1;
                if canonical_name.is_none() {
                    canonical_name = Some(symbol.name.clone());
                }
            }
        }

        if exact_match_count == 1 {
            canonical_name
        } else {
            None
        }
    }

    /// Start here when you don't know where to look. Accepts a natural-language concept
    /// and returns related symbols, patterns, and files. Set depth=2 for signatures and
    /// callers of top symbols (~1500 tokens). Set depth=3 for implementations and type
    /// dependency chains (~3000 tokens). NOT for finding a specific symbol by name
    /// (use search_symbols). NOT for text content search (use search_text).
    #[tool(
        description = "Start here when you don't know where to look. Accepts a natural-language concept and returns related symbols, patterns, and files. Set depth=2 for signatures and callers of top symbols (~1500 tokens). Set depth=3 for implementations and type dependency chains (~3000 tokens). NOT for finding a specific symbol by name (use search_symbols). NOT for text content search (use search_text).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn explore(&self, params: Parameters<ExploreInput>) -> String {
        if let Some(result) = self.proxy_tool_call("explore", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let depth = params.0.depth.unwrap_or(1);
            let est = match depth {
                1 => 500,
                2 => 1500,
                _ => 3000,
            };
            return format!("Estimate for explore: ~{} tokens (depth={})", est, depth);
        }
        let lang_filter = match parse_language_filter(params.0.language.as_deref()) {
            Ok(f) => f,
            Err(e) => return e,
        };
        let limit = params.0.limit.unwrap_or(10) as usize;
        let include_noise = params.0.include_noise.unwrap_or(false);
        let guard = self.index.read();
        loading_guard!(guard);

        let concept = super::explore::match_concept(&params.0.query);

        let mut enriched_imports: Vec<String> = Vec::new();
        let (label, symbol_queries, text_queries, remainder_terms) = if let Some((key, c)) = concept
        {
            let remainder = Self::compute_remainder_terms(&params.0.query, key);
            let mut sym_q: Vec<String> = c.symbol_queries.iter().map(|s| s.to_string()).collect();
            // Convention-aware enrichment: add project-specific imports related to the concept.
            let project_imports =
                crate::protocol::conventions::extract_top_import_roots(&guard, 100);
            let enrichment = super::explore::enrich_concept_with_imports(c, &project_imports);
            enriched_imports = enrichment;
            sym_q.extend(enriched_imports.iter().cloned());
            (
                c.label.to_string(),
                sym_q,
                c.text_queries
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                remainder,
            )
        } else {
            let terms = super::explore::fallback_terms(&params.0.query);
            if terms.is_empty() {
                return "Explore requires a non-empty query.".to_string();
            }
            (
                format!("'{}'", params.0.query),
                terms.clone(),
                terms,
                vec![],
            )
        };

        // Phase 1: Symbol search — over-fetch and track both match counts and
        // query-term coverage per symbol so multi-term hits outrank one-term noise.
        let mut match_scores: HashMap<(String, String, String), ExploreMatchScore> = HashMap::new();
        let mut file_signals: HashMap<String, ExploreFileSignal> = HashMap::new();

        // Phase 0: Module-path boosting — symbols from files whose path segment
        // matches a query term get a weight boost. +2 for exact segment match,
        // +1 for substring segment match. Per-directory cap of `limit` symbols.
        // For concept+remainder queries, boost on remainder terms only so path-scoping
        // is driven by the narrowing terms (e.g., "watcher" in "error handling in the watcher").
        let boost_terms = if remainder_terms.is_empty() {
            symbol_queries.clone()
        } else {
            remainder_terms.clone()
        };
        for term in &boost_terms {
            let term_lower = term.to_ascii_lowercase();
            for (file_path, file) in guard.all_files() {
                if explore_should_skip_path_boost(file_path, &file.classification, include_noise) {
                    continue;
                }
                let segments: Vec<&str> = file_path.split(&['/', '\\'][..]).collect();
                let best_match = segments
                    .iter()
                    .filter_map(|seg| {
                        let seg_lower = seg.to_ascii_lowercase();
                        let seg_stem = seg_lower
                            .strip_suffix(".rs")
                            .or_else(|| seg_lower.strip_suffix(".py"))
                            .or_else(|| seg_lower.strip_suffix(".ts"))
                            .or_else(|| seg_lower.strip_suffix(".js"))
                            .or_else(|| seg_lower.strip_suffix(".go"))
                            .unwrap_or(&seg_lower);
                        if seg_stem == term_lower {
                            Some(2usize)
                        } else if seg_stem.contains(&*term_lower) {
                            Some(1usize)
                        } else {
                            None
                        }
                    })
                    .max();
                if let Some(weight) = best_match {
                    Self::record_explore_file_signal(
                        &mut file_signals,
                        file_path,
                        &term_lower,
                        weight as u64,
                    );
                    for (injected, sym) in file.symbols.iter().enumerate() {
                        if injected >= limit {
                            break;
                        }
                        let entry = (sym.name.clone(), sym.kind.to_string(), file_path.clone());
                        let score = match_scores.entry(entry).or_default();
                        if score.raw_count == 0 {
                            // Cap path-boost: only seed symbols that have no prior
                            // matches. This prevents path-matching files from
                            // dominating over content-matching files.
                            score.raw_count = weight.min(1);
                        }
                        score.matched_terms.insert(term_lower.clone());
                    }
                }
            }
        }

        // Merge remainder terms into symbol/text queries for Phases 1-2 so that compound
        // queries like "error handling in the watcher" search concept queries AND "watcher".
        let mut all_symbol_queries = symbol_queries.clone();
        all_symbol_queries.extend(remainder_terms.iter().cloned());
        let mut all_text_queries = text_queries.clone();
        all_text_queries.extend(remainder_terms.iter().cloned());

        let fallback_mode = concept.is_none();
        for sq in &all_symbol_queries {
            let term_key = sq.to_ascii_lowercase();
            let result = search::search_symbols(&guard, sq, None, limit * 3);
            let mut seen_paths = HashSet::new();
            for hit in &result.hits {
                if fallback_mode && !Self::explore_fallback_symbol_match(&hit.name, &term_key) {
                    continue;
                }
                let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
                let score = match_scores.entry(entry).or_default();
                score.raw_count += 1;
                score.matched_terms.insert(term_key.clone());
                if seen_paths.insert(hit.path.clone()) {
                    Self::record_explore_file_signal(&mut file_signals, &hit.path, &term_key, 2);
                }
            }
        }

        // Filter Phase 1 results by language and path_prefix
        if lang_filter.is_some() || params.0.path_prefix.is_some() {
            match_scores.retain(|(_, _, path), _| {
                if let Some(ref prefix) = params.0.path_prefix
                    && !path.starts_with(prefix.as_str())
                {
                    return false;
                }
                if let Some(ref lang) = lang_filter {
                    let ext = path.rsplit('.').next().unwrap_or("");
                    if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang)
                    {
                        return false;
                    }
                }
                true
            });
            file_signals.retain(|path, _| {
                if let Some(ref prefix) = params.0.path_prefix
                    && !path.starts_with(prefix.as_str())
                {
                    return false;
                }
                if let Some(ref lang) = lang_filter {
                    let ext = path.rsplit('.').next().unwrap_or("");
                    if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang)
                    {
                        return false;
                    }
                }
                true
            });
        }

        if !include_noise {
            let noise_policy = search::NoisePolicy::hide_classified_noise();
            file_signals.retain(|path, _| {
                let Some(file) = guard.get_file(path) else {
                    return false;
                };
                if explore_is_test_like_path(path, Some(&file.classification)) {
                    return false;
                }
                let class = search::NoisePolicy::classify_path(path, None);
                !noise_policy.should_hide(class)
            });
        }

        // Phase 2: Text search — collect text hits and inject enclosing symbols into match_counts
        let mut text_hits: Vec<(String, String, usize)> = Vec::new(); // (path, line, line_number)
        for tq in &all_text_queries {
            let mut options = search::TextSearchOptions {
                total_limit: limit.min(50),
                max_per_file: limit, // need enough matches per file for enclosing symbol extraction
                ..search::TextSearchOptions::for_current_code_search()
            };
            if let Some(ref prefix) = params.0.path_prefix {
                options.path_scope = search::PathScope::Prefix(prefix.clone());
            }
            if let Some(ref lang) = lang_filter {
                options.language_filter = Some(lang.clone());
            }
            let result = search::search_text_with_options(&guard, Some(tq), None, false, &options);
            if let Ok(r) = result {
                let term_key = tq.to_ascii_lowercase();
                for file in &r.files {
                    if !file.matches.is_empty() {
                        Self::record_explore_file_signal(
                            &mut file_signals,
                            &file.path,
                            &term_key,
                            3,
                        );
                    }
                    for m in &file.matches {
                        if text_hits.len() < limit && !format::is_noise_line(&m.line) {
                            text_hits.push((file.path.clone(), m.line.clone(), m.line_number));
                        }
                        // Inject enclosing symbol into match_counts.
                        // Weight 2 so content matches outweigh path-only boosts.
                        if let Some(ref enc) = m.enclosing_symbol {
                            let entry = (enc.name.clone(), enc.kind.clone(), file.path.clone());
                            let score = match_scores.entry(entry).or_default();
                            score.raw_count += 2;
                            score.matched_terms.insert(term_key.clone());
                        }
                    }
                }
            }
        }

        // Filter text hits by path_prefix (language already handled via TextSearchOptions)
        if let Some(ref prefix) = params.0.path_prefix {
            text_hits.retain(|(path, _, _)| path.starts_with(prefix.as_str()));
        }

        let derived_cluster = if fallback_mode {
            Self::derive_explore_cluster(&guard, &symbol_queries, &file_signals, limit)
        } else {
            None
        };

        if let Some(cluster) = &derived_cluster {
            for derived_query in &cluster.promoted_symbols {
                let term_key = derived_query.to_ascii_lowercase();
                if !all_symbol_queries
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(derived_query))
                {
                    let result = search::search_symbols(&guard, derived_query, None, limit * 2);
                    let mut seen_paths = HashSet::new();
                    for hit in &result.hits {
                        let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
                        let score = match_scores.entry(entry).or_default();
                        score.raw_count += 1;
                        score.matched_terms.insert(term_key.clone());
                        if seen_paths.insert(hit.path.clone()) {
                            Self::record_explore_file_signal(
                                &mut file_signals,
                                &hit.path,
                                &term_key,
                                2,
                            );
                        }
                    }
                }

                let mut options = search::TextSearchOptions {
                    total_limit: limit.min(50),
                    max_per_file: limit,
                    ..search::TextSearchOptions::for_current_code_search()
                };
                if let Some(ref prefix) = params.0.path_prefix {
                    options.path_scope = search::PathScope::Prefix(prefix.clone());
                }
                if let Some(ref lang) = lang_filter {
                    options.language_filter = Some(lang.clone());
                }

                if let Ok(result) = search::search_text_with_options(
                    &guard,
                    Some(derived_query),
                    None,
                    false,
                    &options,
                ) {
                    for file in &result.files {
                        if !file.matches.is_empty() {
                            Self::record_explore_file_signal(
                                &mut file_signals,
                                &file.path,
                                &term_key,
                                2,
                            );
                        }
                        for m in &file.matches {
                            if text_hits.len() < limit && !format::is_noise_line(&m.line) {
                                text_hits.push((file.path.clone(), m.line.clone(), m.line_number));
                            }
                            if let Some(ref enc) = m.enclosing_symbol {
                                let entry = (enc.name.clone(), enc.kind.clone(), file.path.clone());
                                let score = match_scores.entry(entry).or_default();
                                score.raw_count += 1;
                                score.matched_terms.insert(term_key.clone());
                            }
                        }
                    }
                }
            }
        }

        // Phase 3: Filter noise, weight by kind and path, sort, truncate to limit.
        // Exclude explore.rs itself (CONCEPT_MAP contains concept keywords in its body).
        match_scores.retain(|(_, _, path), _| !path.ends_with("protocol/explore.rs"));

        // Score each symbol: match_count * kind_weight, penalized for doc/generated files.
        let scored: Vec<((String, String, String), u64)> = match_scores
            .into_iter()
            .map(|((name, kind, path), score_data)| {
                // Kind weight: definition-like symbols rank higher than incidental matches.
                let kind_weight: u64 = match kind.as_str() {
                    "fn" | "method" => 4,
                    "struct" | "class" | "trait" | "interface" | "enum" => 4,
                    "impl" | "mod" | "module" => 3,
                    "const" | "type" => 2,
                    "variable" | "let" => 1,
                    "key" | "section" => 1,
                    _ => 2, // "other" (selectors, etc.)
                };
                let classification = guard.get_file(&path).map(|file| &file.classification);
                let path_penalty = explore_path_penalty(&path, classification);
                let coverage_bonus: u64 = match score_data.matched_terms.len() as u64 {
                    0 | 1 => 1,
                    n => n * n,
                };
                let alignment_multiplier = if fallback_mode {
                    explore_fallback_alignment_multiplier(
                        symbol_queries.len(),
                        score_data.matched_terms.len(),
                    )
                } else {
                    8
                };
                let score = (score_data.raw_count as u64)
                    * kind_weight
                    * path_penalty
                    * coverage_bonus
                    * alignment_multiplier;
                ((name, kind, path), score)
            })
            .collect();

        let mut ranked = scored;
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.0.cmp(&b.0.0)));
        // Filter out weak matches (score < 8 means single text-only hit in a doc file).
        ranked.retain(|(_, score)| *score >= 8);
        ranked.truncate(limit);
        let max_score = ranked.first().map(|(_, s)| *s as f32).unwrap_or(1.0);
        let symbol_scores: Vec<f32> = ranked
            .iter()
            .map(|(_, s)| if max_score > 0.0 { (*s as f32 / max_score).min(1.0) } else { 0.0 })
            .collect();
        let symbol_hits: Vec<(String, String, String)> =
            ranked.into_iter().map(|(k, _)| k).collect();

        // Noise filtering: hide vendor/generated/gitignored files by default.
        let mut noise_hidden: usize = 0;
        let (symbol_hits, text_hits) = if !include_noise {
            let noise_policy = search::NoisePolicy::hide_classified_noise();
            let filtered_symbols: Vec<(String, String, String)> = symbol_hits
                .into_iter()
                .filter(|(_, _, path)| {
                    let Some(file) = guard.get_file(path) else {
                        return false;
                    };
                    if explore_is_test_like_path(path, Some(&file.classification)) {
                        noise_hidden += 1;
                        return false;
                    }
                    let class = search::NoisePolicy::classify_path(path, None);
                    let hide = noise_policy.should_hide(class);
                    if hide {
                        noise_hidden += 1;
                    }
                    !hide
                })
                .collect();
            let filtered_text: Vec<(String, String, usize)> = text_hits
                .into_iter()
                .filter(|(path, _, _)| {
                    let Some(file) = guard.get_file(path) else {
                        return false;
                    };
                    if explore_is_test_like_path(path, Some(&file.classification)) {
                        noise_hidden += 1;
                        return false;
                    }
                    let class = search::NoisePolicy::classify_path(path, None);
                    let hide = noise_policy.should_hide(class);
                    if hide {
                        noise_hidden += 1;
                    }
                    !hide
                })
                .collect();
            (filtered_symbols, filtered_text)
        } else {
            (symbol_hits, text_hits)
        };

        // Count files by symbol/text presence
        let mut file_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (_, _, path) in &symbol_hits {
            *file_counts.entry(path.clone()).or_default() += 1;
        }
        for (path, _, _) in &text_hits {
            *file_counts.entry(path.clone()).or_default() += 1;
        }
        let mut related_files: Vec<(String, usize)> = file_counts.into_iter().collect();
        related_files.sort_by(|a, b| b.1.cmp(&a.1));
        related_files.truncate(limit);

        // Depth 2+: enrich top symbol hits with signatures and dependents
        let depth = params.0.depth.unwrap_or(1).clamp(1, 3);
        let mut enriched_symbols: Vec<format::ExploreEnrichedSymbol> = Vec::new();
        // (name, kind, path, signature, dependent_files)

        if depth >= 2 {
            let enrich_limit = 5.min(symbol_hits.len());
            for (name, kind, path) in &symbol_hits[..enrich_limit] {
                let signature = guard.get_file(path).and_then(|file| {
                    let sym = file.symbols.iter().find(|s| {
                        s.name == *name && s.kind.to_string().eq_ignore_ascii_case(kind)
                    })?;
                    let body = std::str::from_utf8(
                        &file.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize],
                    )
                    .ok()?;
                    Some(format::apply_verbosity(body, "signature"))
                });

                let dependents = {
                    let ref_view = guard.capture_find_references_view(name, None, 3);
                    ref_view
                        .files
                        .iter()
                        .take(3)
                        .map(|f| f.file_path.clone())
                        .collect()
                };

                enriched_symbols.push((
                    name.clone(),
                    kind.clone(),
                    path.clone(),
                    signature,
                    dependents,
                ));
            }
        }

        // Depth 3: gather implementations AND type dependencies for top symbols
        let mut symbol_impls: Vec<(String, Vec<String>)> = Vec::new();
        let mut symbol_deps: Vec<(String, Vec<String>)> = Vec::new();
        if depth >= 3 {
            let impl_limit = 3.min(enriched_symbols.len());
            for (name, _kind, path, _, _) in &enriched_symbols[..impl_limit] {
                // Implementations (trait → implementors)
                let impl_view = guard.capture_implementations_view(name, None);
                let impl_names: Vec<String> = impl_view
                    .entries
                    .iter()
                    .take(5)
                    .map(|e| {
                        format!(
                            "{} impl {} ({}:{})",
                            e.implementor, e.trait_name, e.file_path, e.line
                        )
                    })
                    .collect();
                if !impl_names.is_empty() {
                    symbol_impls.push((name.clone(), impl_names));
                }

                // Type dependencies (what types does this symbol reference?)
                let bundle = guard.capture_context_bundle_view(path, name, None, None);
                if let crate::live_index::query::ContextBundleView::Found(found) = bundle {
                    let dep_names: Vec<String> = found
                        .dependencies
                        .iter()
                        .take(8)
                        .map(|d| format!("{} {} ({})", d.kind_label, d.name, d.file_path))
                        .collect();
                    if !dep_names.is_empty() {
                        symbol_deps.push((name.clone(), dep_names));
                    }
                }
            }
        }

        let mut output = format::explore_result_view(format::ExploreResultViewInput {
            label: &label,
            symbol_hits: &symbol_hits,
            text_hits: &text_hits,
            related_files: &related_files,
            enriched_symbols: &enriched_symbols,
            symbol_impls: &symbol_impls,
            symbol_deps: &symbol_deps,
            derived_seed_terms: derived_cluster
                .as_ref()
                .map(|cluster| cluster.seed_terms.as_slice())
                .unwrap_or(&[]),
            derived_symbols: derived_cluster
                .as_ref()
                .map(|cluster| cluster.promoted_symbols.as_slice())
                .unwrap_or(&[]),
            enriched_imports: &enriched_imports,
            symbol_scores: &symbol_scores,
            derived_seed_files: derived_cluster
                .as_ref()
                .map(|cluster| cluster.seed_files.as_slice())
                .unwrap_or(&[]),
            depth,
        });

        if noise_hidden > 0 {
            output.push_str(&format!(
                "\n\nNote: {noise_hidden} result(s) from vendor/generated files hidden. Use include_noise=true to include."
            ));
        }

        self.record_tool_savings_named(
            "explore",
            (output.len() * 10 / 4) as u64,
            (output.len() / 4) as u64,
        );
        self.session_context
            .record_summary_output("explore", (output.len() / 4).min(u32::MAX as usize) as u32);
        format::enforce_token_budget(output, params.0.max_tokens)
    }

    #[tool(
        description = "Detect project coding conventions from the indexed codebase. Returns error handling style, naming patterns, test organization, common imports, and file structure. Use when you need to write code that fits the project's existing patterns.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn conventions(&self) -> String {
        if let Some(result) = self.proxy_tool_call_without_params("conventions").await {
            return result;
        }
        let conv = {
            let guard = self.index.read();
            loading_guard!(guard);
            crate::protocol::conventions::detect_conventions(&guard)
        };
        let mut output = crate::protocol::conventions::format_conventions(&conv);
        output.push_str("\n\n── Worktree awareness ──\n");
        output.push_str(
            "When editing from inside a git worktree, pass `working_directory` \
             (absolute path of the worktree root) to every edit tool \
             (edit_within_symbol, replace_symbol_body, insert_symbol, \
             delete_symbol, batch_edit, batch_insert, batch_rename) so writes \
             land in your worktree instead of the indexed copy.\n\
             Feature-gated on `SYMFORGE_WORKTREE_AWARE=1`; when the flag is \
             unset or the parameter is omitted, today's indexed-path \
             behaviour is preserved. See README §Worktree awareness.",
        );
        output.push_str("\n\n── Frecency ranking ──\n");
        output.push_str(
            "Per-workspace file-ranking signal that decays on a 7-day half-life \
             and fuses with existing path-match and co-change signals. Call \
             `search_files` with `rank_by=\"frecency\"` to surface files you \
             recently touched. Feature-gated on `SYMFORGE_FRECENCY=1`; when the \
             flag is unset the ranker and every bump hook are no-ops.\n\
             Bump-on-commitment policy: SymForge bumps a path's frecency score \
             on every edit tool and on the read tools that imply commitment to \
             a known file (`get_file_context`, `get_file_content`, `get_symbol`, \
             `get_symbol_context`). Discovery tools (`search_files`, \
             `search_text`, `search_symbols`) deliberately never bump — \
             searching for a file is not the same as working on it, and \
             self-bumping searches corrupt rankings via a positive feedback \
             loop. Batch tools dedup bumps per invocation. Set \
             `SYMFORGE_DEBUG_RANKING=1` for per-signal score breakdowns in \
             responses and the last-10 bumps list in `health`. See README \
             §Frecency ranking and ADR 0011 for the full policy.",
        );
        self.session_context.record_summary_output(
            "conventions",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        output
    }

    #[tool(
        description = "Plan an edit: analyzes a target symbol or file, counts references, and suggests the right sequence of SymForge edit tools. Accepts a bare symbol name, a file path, or an exact selector like `src/lib.rs::helper`. Use before making changes to understand impact.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn edit_plan(&self, params: Parameters<EditPlanInput>) -> String {
        if let Some(result) = self.proxy_tool_call("edit_plan", &params.0).await {
            return result;
        }
        let guard = self.index.read();
        loading_guard!(guard);
        let output = crate::protocol::edit_plan::plan_edit(&guard, &params.0.target);
        self.session_context.record_summary_output(
            "edit_plan",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        output
    }

    #[tool(
        description = "Suggest what to investigate next based on what you've already loaded. Analyzes session context to find referenced-but-not-loaded symbols. Use during deep investigations to find gaps.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn investigation_suggest(
        &self,
        params: Parameters<InvestigationInput>,
    ) -> String {
        if let Some(result) = self
            .proxy_tool_call("investigation_suggest", &params.0)
            .await
        {
            return result;
        }
        let guard = self.index.read();
        loading_guard!(guard);
        let output = crate::protocol::investigation::suggest_next_steps(
            &guard,
            &self.session_context,
            params.0.focus.as_deref(),
        );
        self.session_context.record_summary_output(
            "investigation_suggest",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        output
    }

    #[tool(
        description = "Show what symbols and files have been fetched this session. Returns a context inventory with token counts. Use to track your context budget and avoid re-fetching content you already have.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn context_inventory(&self) -> String {
        if let Some(result) = self
            .proxy_tool_call_without_params("context_inventory")
            .await
        {
            return result;
        }
        let snap = self.session_context.snapshot();
        crate::protocol::session::format_context_inventory(&snap)
    }

    #[tool(
        description = "Natural language entry point — ask any question about the codebase and SymForge routes to the right tool internally. Use when unsure which specific tool to call. Examples: 'who calls X', 'where is X defined', 'how does X work', 'what changed', 'find file X'. Returns the result plus which tool was used, so you can call it directly next time.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn ask(&self, params: Parameters<SmartQueryInput>) -> String {
        use crate::protocol::smart_query;

        let original_q = params.0.query.trim();
        let q = smart_query::strip_leading_articles(original_q);
        if q.is_empty() {
            return "query requires a non-empty question.".to_string();
        }

        let (mut intent, mut matched_prefix) = smart_query::classify_intent_with_match(q);
        if matches!(
            intent,
            smart_query::QueryIntent::Understand { .. } | smart_query::QueryIntent::Explore { .. }
        ) {
            let guard = self.index.read();
            if let Some(name) =
                Self::extract_exact_implementation_understanding_candidate(&guard, q)
            {
                intent = smart_query::QueryIntent::UnderstandImplementations { name };
                matched_prefix = false;
            } else if let Some(symbol) =
                Self::extract_exact_symbol_understanding_candidate(&guard, q)
            {
                intent = smart_query::QueryIntent::UnderstandSymbol { symbol };
                matched_prefix = false;
            }
        }
        let assessment = smart_query::assess_route(&intent, matched_prefix);
        let route_desc = smart_query::route_description(&intent);

        let result = match &intent {
            smart_query::QueryIntent::FindCallers { symbol, path } => {
                let input = FindReferencesInput {
                    name: symbol.clone(),
                    kind: None,
                    path: path.clone(),
                    symbol_kind: None,
                    symbol_line: None,
                    limit: None,
                    max_per_file: None,
                    compact: Some(true),
                    mode: None,
                    direction: None,
                    estimate: None,
                    max_tokens: None,
                };
                self.find_references(Parameters(input)).await
            }
            smart_query::QueryIntent::FindSymbol { name, kind } => {
                let input = SearchSymbolsInput {
                    query: Some(name.clone()),
                    kind: kind.clone(),
                    path_prefix: None,
                    language: None,
                    limit: None,
                    include_generated: None,
                    include_tests: None,
                    estimate: None,
                    max_tokens: None,
                };
                self.search_symbols(Parameters(input)).await
            }
            smart_query::QueryIntent::FindFile { hint } => {
                let input = SearchFilesInput {
                    query: hint.clone(),
                    limit: None,
                    current_file: None,
                    changed_with: None,
                    resolve: None,
                    estimate: None,
                    max_tokens: None,
                    rank_by: None,
                };
                self.search_files(Parameters(input)).await
            }
            smart_query::QueryIntent::FindChanges => {
                let input = WhatChangedInput {
                    since: None,
                    git_ref: None,
                    uncommitted: Some(true),
                    path_prefix: None,
                    language: None,
                    code_only: Some(true),
                    include_symbol_diff: Some(true),
                    estimate: None,
                    max_tokens: None,
                };
                self.what_changed(Parameters(input)).await
            }
            smart_query::QueryIntent::Understand { concept } => {
                let input = ExploreInput {
                    query: concept.clone(),
                    limit: None,
                    depth: Some(2),
                    include_noise: None,
                    language: None,
                    path_prefix: None,
                    estimate: None,
                    max_tokens: None,
                };
                self.explore(Parameters(input)).await
            }
            smart_query::QueryIntent::UnderstandSymbol { symbol } => {
                let input = GetSymbolContextInput {
                    name: symbol.clone(),
                    file: None,
                    path: None,
                    symbol_kind: None,
                    symbol_line: None,
                    verbosity: Some("compact".to_string()),
                    bundle: None,
                    sections: None,
                    max_tokens: None,
                    estimate: None,
                };
                self.get_symbol_context(Parameters(input)).await
            }
            smart_query::QueryIntent::UnderstandImplementations { name } => {
                let input = FindReferencesInput {
                    name: name.clone(),
                    kind: None,
                    path: None,
                    symbol_kind: None,
                    symbol_line: None,
                    limit: None,
                    max_per_file: None,
                    compact: None,
                    mode: Some("implementations".to_string()),
                    direction: None,
                    estimate: None,
                    max_tokens: None,
                };
                self.find_references(Parameters(input)).await
            }
            smart_query::QueryIntent::SearchCode { pattern } => {
                let input = SearchTextInput {
                    query: Some(pattern.clone()),
                    terms: None,
                    regex: None,
                    path_prefix: None,
                    language: None,
                    limit: None,
                    max_per_file: None,
                    glob: None,
                    exclude_glob: None,
                    context: None,
                    case_sensitive: None,
                    whole_word: None,
                    group_by: None,
                    follow_refs: None,
                    follow_refs_limit: None,
                    ranked: None,
                    include_generated: None,
                    include_tests: None,
                    estimate: None,
                    max_tokens: None,
                    structural: None,
                };
                self.search_text(Parameters(input)).await
            }
            smart_query::QueryIntent::FindDependents { target } => {
                let input = FindDependentsInput {
                    path: target.clone(),
                    limit: None,
                    max_per_file: None,
                    format: None,
                    compact: Some(true),
                    estimate: None,
                    max_tokens: None,
                };
                self.find_dependents(Parameters(input)).await
            }
            smart_query::QueryIntent::FindImplementations { name } => {
                let input = FindReferencesInput {
                    name: name.clone(),
                    kind: None,
                    path: None,
                    symbol_kind: None,
                    symbol_line: None,
                    limit: None,
                    max_per_file: None,
                    compact: None,
                    mode: Some("implementations".to_string()),
                    direction: None,
                    estimate: None,
                    max_tokens: None,
                };
                self.find_references(Parameters(input)).await
            }
            smart_query::QueryIntent::Explore { query } => {
                let input = ExploreInput {
                    query: query.clone(),
                    limit: None,
                    depth: Some(2),
                    include_noise: None,
                    language: None,
                    path_prefix: None,
                    estimate: None,
                    max_tokens: None,
                };
                self.explore(Parameters(input)).await
            }
        };

        let mut envelope = format!(
            "Route confidence: {}\nChosen tool: {}\nInvocation: {}\nRationale: {}",
            smart_query::route_confidence_label(assessment.confidence),
            smart_query::route_tool_name(&intent),
            smart_query::route_invocation(&intent),
            assessment.rationale,
        );
        if original_q != q {
            envelope.push_str(&format!("\nOriginal query: {original_q}"));
        }
        if let Some(next_step) = assessment.suggested_next_step {
            envelope.push_str(&format!("\nSuggested next step: {next_step}"));
        }

        let output = format!("{envelope}\n{route_desc}\n\n{result}");
        self.session_context
            .record_summary_output("ask", (output.len() / 4).min(u32::MAX as usize) as u32);
        format::enforce_token_budget(output, params.0.max_tokens)
    }

    /// Symbol-level diff between two git refs. Shows +added, -removed, ~modified symbols per changed
    /// file. Filter with path_prefix and/or language. Set code_only=true to exclude non-source files.
    /// Use for code review to see which functions/classes changed.
    /// NOT for file-level change lists (use what_changed).
    #[tool(
        description = "Symbol-level diff between two git refs. Shows +added, -removed, ~modified symbols per changed file. Filter with path_prefix and/or language. Set code_only=true to exclude non-source files. Use for code review to see which functions/classes changed. NOT for file-level change lists (use what_changed).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn diff_symbols(&self, params: Parameters<DiffSymbolsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("diff_symbols", &params.0).await {
            return result;
        }
        if params.0.estimate == Some(true) {
            let compact = params.0.compact.unwrap_or(false);
            let summary = params.0.summary_only.unwrap_or(false);
            let est = if summary {
                50
            } else if compact {
                200
            } else {
                500
            };
            return format!(
                "Estimate for diff_symbols: ~{} tokens (compact={}, summary_only={})",
                est, compact, summary
            );
        }
        let base = params.0.base.as_deref().unwrap_or("main");
        let target = params.0.target.as_deref().unwrap_or("HEAD");

        let repo_root = self.capture_repo_root();

        let Some(repo_root) = repo_root else {
            return "No repository root found.".to_string();
        };

        // Check index is not loading/empty
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }

        // Get changed files
        let repo = match crate::git::GitRepo::open(&repo_root) {
            Ok(r) => r,
            Err(e) => return format!("Failed to open repository: {e}"),
        };
        let changed_files_owned = match repo.changed_paths_between_refs(base, target) {
            Ok(paths) => paths,
            Err(e) => return format!("Failed to run git diff: {e}"),
        };

        // Apply path_prefix + language filter
        let lang_filter = match parse_language_filter(params.0.language.as_deref()) {
            Ok(f) => f,
            Err(e) => return e,
        };
        let code_only = params.0.code_only.unwrap_or(false);
        let changed_files: Vec<&str> = changed_files_owned
            .iter()
            .map(|s| s.as_str())
            .filter(|p| {
                if let Some(ref prefix) = params.0.path_prefix
                    && !p.starts_with(prefix.as_str())
                {
                    return false;
                }
                if let Some(ref lang) = lang_filter {
                    let ext = p.rsplit('.').next().unwrap_or("");
                    if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang)
                    {
                        return false;
                    }
                }
                if code_only && lang_filter.is_none() {
                    let ext = p.rsplit('.').next().unwrap_or("");
                    match crate::domain::index::LanguageId::from_extension(ext) {
                        None => return false,
                        Some(lang) => {
                            if crate::parsing::config_extractors::is_config_language(&lang) {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .collect();

        if changed_files.is_empty() {
            return format!("No file changes found between {base} and {target}.");
        }

        let output = render_diff_symbols_output(
            base,
            target,
            changed_files_owned.len(),
            &changed_files,
            &repo,
            params.0.compact.unwrap_or(false),
            params.0.summary_only.unwrap_or(false),
            params.0.path_prefix.as_deref(),
            params.0.language.as_deref(),
            code_only,
        );
        self.record_tool_savings((output.len() * 5 / 4) as u64, (output.len() / 4) as u64);
        self.session_context.record_summary_output(
            "diff_symbols",
            (output.len() / 4).min(u32::MAX as usize) as u32,
        );
        format::enforce_token_budget(output, params.0.max_tokens)
    }

    // ─── Edit tools (Tier 1) ─────────────────────────────────────────────────

    fn check_edit_capability(
        language: &crate::domain::LanguageId,
        required: crate::parsing::config_extractors::EditCapability,
        tool_name: &str,
    ) -> Option<String> {
        use crate::parsing::config_extractors::{EditCapability, edit_capability_for_language};
        if let Some(cap) = edit_capability_for_language(language) {
            let allowed = match required {
                EditCapability::IndexOnly => false,
                EditCapability::TextEditSafe => {
                    matches!(
                        cap,
                        EditCapability::TextEditSafe | EditCapability::StructuralEditSafe
                    )
                }
                EditCapability::StructuralEditSafe => {
                    matches!(cap, EditCapability::StructuralEditSafe)
                }
            };
            if !allowed {
                let suggestion = match required {
                    EditCapability::StructuralEditSafe => {
                        "use edit_within_symbol for scoped text replacements, or the built-in Edit tool for raw text edits."
                    }
                    EditCapability::TextEditSafe => {
                        "use the built-in Edit tool for raw text edits in this file type."
                    }
                    EditCapability::IndexOnly => {
                        "inspect the file with read-only tools or use the built-in Edit tool for raw text edits."
                    }
                };
                return Some(edit_format::format_capability_warning(
                    tool_name,
                    &language.to_string(),
                    edit_capability_label(required),
                    edit_capability_label(cap),
                    suggestion,
                ));
            }
        }
        None // No capability restriction
    }

    /// Replace a symbol's entire definition with new source code. The index resolves the symbol's
    /// byte range server-side — no need to read the file first. Content is auto-indented to match
    /// the original symbol's indentation level.
    /// NOT for small edits within a symbol (use edit_within_symbol).
    /// NOT for removing a symbol entirely (use delete_symbol).
    #[tool(
        description = "Replace a symbol's entire definition with new source code. The index resolves the symbol's byte range server-side — no need to read the file first. Content is auto-indented to match the original symbol's indentation level. Use symbol_line to disambiguate overloaded names. NOT for small edits within a symbol (use edit_within_symbol). NOT for removing a symbol entirely (use delete_symbol).",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    pub(crate) async fn replace_symbol_body(
        &self,
        params: Parameters<edit::ReplaceSymbolBodyInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("replace_symbol_body", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_flag_on(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory: params
                .0
                .working_directory
                .as_deref()
                .map(std::path::Path::new),
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return format!("Error: {e}"),
        };
        let resolved_path = resolved_target.target_path.clone();
        let working_directory_supplied = params.0.working_directory.is_some();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "replace_symbol_body",
        ) {
            return warning;
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        if params.0.dry_run == Some(true) {
            let old_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
            let summary = format!(
                "[DRY RUN] Would replace `{}` in {} (old: {} bytes -> new: {} bytes)",
                params.0.name,
                params.0.path,
                old_bytes,
                params.0.new_body.len()
            );
            return format!(
                "{}\n{}",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::StructuralEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                summary
            );
        }
        let old_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
        // Decide where the splice starts based on whether the caller
        // supplied fresh docs in `new_body`:
        //   * new_body starts with a doc marker → extend past the old
        //     attached/orphaned docs so the new ones replace them
        //     (prevents duplicate JSDoc/XML doc blocks).
        //   * new_body has no doc marker → start at the signature line
        //     so existing attached docs and attributes stay put.
        // Preserving docs by default was the behavior users expected;
        // swallowing them silently was the bug surfaced in the v7.5 review.
        let new_body_supplies_docs = edit::body_starts_with_doc_comment(&params.0.new_body);
        let effective = if new_body_supplies_docs {
            sym.effective_start() as usize
        } else {
            sym.byte_range.0 as usize
        };
        let raw_line_start = file.content[..effective]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line_start = if new_body_supplies_docs {
            edit::extend_past_orphaned_docs(&file.content, raw_line_start, &sym) as u32
        } else {
            raw_line_start as u32
        };
        let indent = edit::detect_indentation(&file.content, sym.byte_range.0);
        let line_ending = edit::detect_line_ending(&file.content);
        let normalized = edit::normalize_line_endings(params.0.new_body.as_bytes(), line_ending);
        let normalized_str = std::str::from_utf8(&normalized).unwrap_or(&params.0.new_body);
        let indented = edit::apply_indentation(normalized_str, &indent, line_ending);
        let new_content =
            edit::apply_splice(&file.content, (line_start, sym.byte_range.1), &indented);
        if let Err(e) = edit::atomic_write_file(&resolved_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        let old_sig = edit::extract_signature(&file.content, sym.byte_range);
        let new_sig = params.0.new_body.lines().next().unwrap_or("").to_string();
        // Detect parent impl type for type-aware reference filtering.
        // Methods inside `impl Foo` only warn about refs in files that also mention `Foo`.
        let parent_type = edit::find_parent_impl_type(&file, &sym);
        edit::reindex_after_write(
            &self.index,
            &resolved_path,
            &params.0.path,
            &new_content,
            file.language.clone(),
        );
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let warnings = edit::detect_stale_references(
            &self.index,
            &params.0.path,
            &params.0.name,
            &old_sig,
            &new_sig,
            parent_type.as_deref(),
            Some(&file.language),
        );
        let mut result = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::StructuralEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_replace(
                &params.0.path,
                &params.0.name,
                &sym.kind.to_string(),
                old_bytes,
                indented.len(),
            )
        );
        result.push_str(&edit_format::format_stale_warnings(
            &params.0.path,
            &params.0.name,
            &warnings,
        ));
        result.push_str(&edit_format::format_reroute_suffix(
            working_directory_supplied,
            &resolved_target,
        ));
        result
    }

    /// Insert code before or after a named symbol. Content is auto-indented to match the target
    /// symbol's indentation level — provide unindented code.
    /// NOT for replacing existing code (use replace_symbol_body or edit_within_symbol).
    #[tool(
        description = "Insert code before or after a named symbol. Set position='before' or 'after' (default 'after'). Content is auto-indented to match the target symbol's indentation level — provide unindented code. Use symbol_line to disambiguate overloaded names. NOT for replacing existing code (use replace_symbol_body or edit_within_symbol).",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    pub(crate) async fn insert_symbol(
        &self,
        params: Parameters<edit::InsertSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("insert_symbol", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_flag_on(params.0.working_directory.as_deref());
        let position = params.0.position.as_deref().unwrap_or("after");
        if position != "before" && position != "after" {
            return format!("Error: position must be 'before' or 'after', got '{position}'");
        }
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory: params
                .0
                .working_directory
                .as_deref()
                .map(std::path::Path::new),
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return format!("Error: {e}"),
        };
        let resolved_path = resolved_target.target_path.clone();
        let working_directory_supplied = params.0.working_directory.is_some();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "insert_symbol",
        ) {
            return warning;
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        if params.0.dry_run == Some(true) {
            let summary = format!(
                "[DRY RUN] Would insert {} `{}` in {} ({} bytes of content)",
                position,
                params.0.name,
                params.0.path,
                params.0.content.len()
            );
            return format!(
                "{}\n{}",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::StructuralEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                summary
            );
        }
        let line_ending = edit::detect_line_ending(&file.content);
        let new_content = if position == "before" {
            edit::build_insert_before(&file.content, &sym, &params.0.content, line_ending)
        } else {
            edit::build_insert_after(&file.content, &sym, &params.0.content, line_ending)
        };
        if let Err(e) = edit::atomic_write_file(&resolved_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        edit::reindex_after_write(
            &self.index,
            &resolved_path,
            &params.0.path,
            &new_content,
            file.language.clone(),
        );
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let mut out = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::StructuralEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_insert(
                &params.0.path,
                &params.0.name,
                position,
                params.0.content.len(),
            )
        );
        out.push_str(&edit_format::format_reroute_suffix(
            working_directory_supplied,
            &resolved_target,
        ));
        out
    }

    /// Remove a symbol's entire definition and clean up surrounding blank lines.
    /// NOT for replacing a symbol (use replace_symbol_body).
    #[tool(
        description = "Remove a symbol's entire definition and clean up surrounding blank lines. Use symbol_line to disambiguate overloaded names. NOT for replacing a symbol (use replace_symbol_body).",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    pub(crate) async fn delete_symbol(
        &self,
        params: Parameters<edit::DeleteSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("delete_symbol", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_flag_on(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory: params
                .0
                .working_directory
                .as_deref()
                .map(std::path::Path::new),
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return format!("Error: {e}"),
        };
        let resolved_path = resolved_target.target_path.clone();
        let working_directory_supplied = params.0.working_directory.is_some();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "delete_symbol",
        ) {
            return warning;
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        if params.0.dry_run == Some(true) {
            let deleted_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
            let summary = format!(
                "[DRY RUN] Would delete `{}` in {} ({} bytes)",
                params.0.name, params.0.path, deleted_bytes
            );
            return format!(
                "{}\n{}",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::StructuralEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                summary
            );
        }
        let deleted_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
        let line_ending = edit::detect_line_ending(&file.content);
        let new_content = edit::build_delete(&file.content, &sym, line_ending);
        if let Err(e) = edit::atomic_write_file(&resolved_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        edit::reindex_after_write(
            &self.index,
            &resolved_path,
            &params.0.path,
            &new_content,
            file.language.clone(),
        );
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let mut out = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::StructuralEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_delete(
                &params.0.path,
                &params.0.name,
                &sym.kind.to_string(),
                deleted_bytes,
            )
        );
        out.push_str(&edit_format::format_reroute_suffix(
            working_directory_supplied,
            &resolved_target,
        ));
        out
    }

    /// Find-and-replace scoped to a symbol's byte range — won't affect code outside it. The LLM
    /// never needs to read the symbol body — just provide the old and new text.
    /// NOT for replacing the entire symbol (use replace_symbol_body).
    /// NOT for adding new symbols (use insert_before/after_symbol).
    #[tool(
        description = "Find-and-replace scoped to a symbol's byte range — won't affect code outside it. The LLM never needs to read the symbol body — just provide the old and new text. Set replace_all=true for every occurrence within the symbol. NOT for replacing the entire symbol (use replace_symbol_body). NOT for adding new symbols (use insert_before/after_symbol).",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    pub(crate) async fn edit_within_symbol(
        &self,
        params: Parameters<edit::EditWithinSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("edit_within_symbol", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_flag_on(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory: params
                .0
                .working_directory
                .as_deref()
                .map(std::path::Path::new),
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return format!("Error: {e}"),
        };
        let resolved_path = resolved_target.target_path.clone();
        let working_directory_supplied = params.0.working_directory.is_some();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::TextEditSafe,
            "edit_within_symbol",
        ) {
            return warning;
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        let sym_start = sym.effective_start() as usize;
        let sym_end = sym.byte_range.1 as usize;
        let body = &file.content[sym_start..sym_end];
        let body_str = match std::str::from_utf8(body) {
            Ok(s) => s,
            Err(_) => return "Error: symbol body is not valid UTF-8.".to_string(),
        };
        // Normalize both old_text and new_text to match file line endings.
        let line_ending = edit::detect_line_ending(&file.content);
        let normalized_old =
            edit::normalize_line_endings(params.0.old_text.as_bytes(), line_ending);
        let normalized_old_str =
            String::from_utf8(normalized_old).unwrap_or_else(|_| params.0.old_text.clone());
        let normalized_new =
            edit::normalize_line_endings(params.0.new_text.as_bytes(), line_ending);
        let normalized_new_str =
            String::from_utf8(normalized_new).unwrap_or_else(|_| params.0.new_text.clone());
        let (new_body, count) = if params.0.replace_all {
            let count = body_str.matches(&normalized_old_str).count();
            if count > 0 {
                (body_str.replace(&normalized_old_str, &normalized_new_str), count)
            } else {
                // Fallback: try whitespace-flexible matching.
                match edit::try_whitespace_flexible_replace(
                    body_str,
                    &normalized_old_str,
                    &normalized_new_str,
                    true,
                ) {
                    Some(result) => result,
                    None => (body_str.to_string(), 0), // hits count==0 error below
                }
            }
        } else {
            match body_str.find(&normalized_old_str) {
                Some(_) => (
                    body_str.replacen(&normalized_old_str, &normalized_new_str, 1),
                    1,
                ),
                None => {
                    // Fallback: try whitespace-flexible matching.
                    match edit::try_whitespace_flexible_replace(
                        body_str,
                        &normalized_old_str,
                        &normalized_new_str,
                        false,
                    ) {
                        Some(result) => result,
                        None => {
                            // Show a preview of the symbol body so the LLM can see what's actually there
                            let preview_len = 800.min(body_str.len());
                            let preview = &body_str[..preview_len];
                            let truncated = if preview_len < body_str.len() {
                                format!("\n... ({} more bytes)", body_str.len() - preview_len)
                            } else {
                                String::new()
                            };
                            return format!(
                                "Error: `{}` not found within symbol `{}`. \
                                 The symbol body is ({} bytes):\n```\n{}{}\n```",
                                params.0.old_text,
                                params.0.name,
                                body_str.len(),
                                preview,
                                truncated
                            );
                        }
                    }
                }
            }
        };
        if params.0.dry_run == Some(true) {
            if count == 0 {
                let preview_len = 800.min(body_str.len());
                let preview = &body_str[..preview_len];
                let truncated = if preview_len < body_str.len() {
                    format!("\n... ({} more bytes)", body_str.len() - preview_len)
                } else {
                    String::new()
                };
                return format!(
                    "Error: `{}` not found within symbol `{}`. \
                     The symbol body is ({} bytes):\n```\n{}{}\n```",
                    params.0.old_text,
                    params.0.name,
                    body_str.len(),
                    preview,
                    truncated
                );
            }
            return format!(
                "{}\n[DRY RUN] Would edit within `{}` in {} ({} replacement(s))",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::TextEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                params.0.name,
                params.0.path,
                count
            );
        }
        if count == 0 {
            let preview_len = 800.min(body_str.len());
            let preview = &body_str[..preview_len];
            let truncated = if preview_len < body_str.len() {
                format!("\n... ({} more bytes)", body_str.len() - preview_len)
            } else {
                String::new()
            };
            return format!(
                "Error: `{}` not found within symbol `{}`. \
                 The symbol body is ({} bytes):\n```\n{}{}\n```",
                params.0.old_text,
                params.0.name,
                body_str.len(),
                preview,
                truncated
            );
        }
        let old_sym_bytes = sym_end - sym_start;
        let effective_range = (sym.effective_start(), sym.byte_range.1);
        let new_content = edit::apply_splice(&file.content, effective_range, new_body.as_bytes());
        if let Err(e) = edit::atomic_write_file(&resolved_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        edit::reindex_after_write(
            &self.index,
            &resolved_path,
            &params.0.path,
            &new_content,
            file.language.clone(),
        );
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let mut out = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::TextEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_edit_within(
                &params.0.path,
                &params.0.name,
                count,
                old_sym_bytes,
                new_body.len(),
            )
        );
        out.push_str(&edit_format::format_reroute_suffix(
            working_directory_supplied,
            &resolved_target,
        ));
        out
    }

    // ── Tier 2: Batch edit tools ──────────────────────────────────────────

    /// Apply multiple symbol-addressed edits atomically.
    /// Set dry_run=true for a read-only preview that makes no file changes.
    #[tool(
        description = "Apply multiple symbol-addressed edits atomically across files. Each edit specifies a file, symbol, and operation (replace/insert_before/insert_after/delete/edit_within). Accepts either structured edits or shorthand strings like `src/lib.rs::helper => edit_within old >>> new`. All symbols are validated before any writes — if any resolution fails, no files are modified. Set dry_run=true for a READ-ONLY preview that shows what would change without writing (safe, no confirmation needed). Edits within the same file must target non-overlapping symbols. NOT for single-symbol edits (use replace_symbol_body, insert_symbol, etc.).",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    pub(crate) async fn batch_edit(&self, params: Parameters<edit::BatchEditInput>) -> String {
        if let Some(result) = self.proxy_tool_call("batch_edit", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_flag_on(params.0.working_directory.as_deref());
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }
        let batch_paths: Vec<String> = params.0.edits.iter().map(|e| e.path.clone()).collect();
        let source_authority = match prepare_batch_paths_for_edit(self, &repo_root, &batch_paths) {
            Ok(authority) => authority,
            Err(e) => return e,
        };
        match edit::execute_batch_edit(
            &self.index,
            &repo_root,
            &params.0.edits,
            params.0.dry_run.unwrap_or(false),
            params
                .0
                .working_directory
                .as_deref()
                .map(std::path::Path::new),
        ) {
            Ok(summaries) => {
                let file_count = params
                    .0
                    .edits
                    .iter()
                    .map(|e| e.path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                let write_semantics = if params.0.dry_run.unwrap_or(false) {
                    edit_format::EditWriteSemantics::DryRunNoWrites
                } else {
                    edit_format::EditWriteSemantics::TransactionalWriteRollbackAndReindex
                };
                let evidence = format!(
                    "{} edit target(s) across {} file(s)",
                    params.0.edits.len(),
                    file_count
                );
                format!(
                    "{}\n{}",
                    edit_format::format_batch_envelope(
                        edit_format::EditSafetyMode::StructuralEditSafe,
                        edit_format::MatchType::Exact,
                        source_authority,
                        write_semantics,
                        &evidence,
                    ),
                    edit_format::format_batch_summary(&summaries, file_count),
                )
            }
            Err(e) => e,
        }
    }

    /// Rename a symbol and update all references project-wide.
    /// Set dry_run=true for a read-only preview that makes no file changes.
    #[tool(
        description = "Rename a symbol and update all references across the project. Finds the definition and all usage sites via the index's reverse reference map. Set dry_run=true for a READ-ONLY preview that lists affected files without writing any changes (safe, no confirmation needed). Applies confident matches transactionally across files; uncertain matches are surfaced for manual review instead of being modified. Common names (e.g. `new`, `get`) can still produce false positives — verify with what_changed afterward. NOT for replacing a symbol's body (use replace_symbol_body).",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    pub(crate) async fn batch_rename(&self, params: Parameters<edit::BatchRenameInput>) -> String {
        if let Some(result) = self.proxy_tool_call("batch_rename", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_flag_on(params.0.working_directory.as_deref());
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }
        let source_authority = prepare_project_wide_rename(self, &repo_root);
        match edit::execute_batch_rename(&self.index, &repo_root, &params.0) {
            Ok(summary) => {
                let write_semantics = if params.0.dry_run.unwrap_or(false) {
                    edit_format::EditWriteSemantics::DryRunNoWrites
                } else {
                    edit_format::EditWriteSemantics::TransactionalWriteRollbackAndReindex
                };
                let evidence = format!(
                    "definition `{}` + project-wide constrained references",
                    params.0.path
                );
                format!(
                    "{}\n{}",
                    edit_format::format_batch_envelope(
                        edit_format::EditSafetyMode::StructuralEditSafe,
                        edit_format::MatchType::Constrained,
                        source_authority,
                        write_semantics,
                        &evidence,
                    ),
                    summary,
                )
            }
            Err(e) => e,
        }
    }

    /// Insert the same code at multiple symbol locations across files.
    #[tool(
        description = "Insert the same code before or after multiple symbols across the project. Useful for adding logging, instrumentation, or boilerplate to many locations at once. Accepts either structured targets or shorthand strings like `src/lib.rs::helper`. Code is auto-indented to match each target symbol. All targets are validated before any writes, and live execution applies transactionally across files with rollback on failure. Set dry_run=true for a READ-ONLY preview. NOT for inserting at a single location (use insert_symbol).",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    pub(crate) async fn batch_insert(&self, params: Parameters<edit::BatchInsertInput>) -> String {
        if let Some(result) = self.proxy_tool_call("batch_insert", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_flag_on(params.0.working_directory.as_deref());
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }
        let batch_paths: Vec<String> = params.0.targets.iter().map(|t| t.path.clone()).collect();
        let source_authority = match prepare_batch_paths_for_edit(self, &repo_root, &batch_paths) {
            Ok(authority) => authority,
            Err(e) => return e,
        };
        match edit::execute_batch_insert(&self.index, &repo_root, &params.0) {
            Ok(summaries) => {
                let file_count = params
                    .0
                    .targets
                    .iter()
                    .map(|t| t.path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                let write_semantics = if params.0.dry_run.unwrap_or(false) {
                    edit_format::EditWriteSemantics::DryRunNoWrites
                } else {
                    edit_format::EditWriteSemantics::TransactionalWriteRollbackAndReindex
                };
                let evidence = format!(
                    "{} target(s) across {} file(s)",
                    params.0.targets.len(),
                    file_count
                );
                format!(
                    "{}\n{}",
                    edit_format::format_batch_envelope(
                        edit_format::EditSafetyMode::StructuralEditSafe,
                        edit_format::MatchType::Exact,
                        source_authority,
                        write_semantics,
                        &evidence,
                    ),
                    edit_format::format_batch_summary(&summaries, file_count),
                )
            }
            Err(e) => e,
        }
    }
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use crate::protocol::SymForgeServer;
    use rmcp::handler::server::wrapper::Parameters;
    use tempfile::TempDir;

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn make_symbol(name: &str, kind: SymbolKind, line_start: u32, line_end: u32) -> SymbolRecord {
        let byte_range = (0, 10);
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (line_start, line_end),
            doc_byte_range: None,
        }
    }

    fn make_symbol_with_bytes(
        name: &str,
        kind: SymbolKind,
        line_start: u32,
        line_end: u32,
        byte_range: (u32, u32),
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (line_start, line_end),
            doc_byte_range: None,
        }
    }

    fn make_file(path: &str, content: &[u8], symbols: Vec<SymbolRecord>) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path(path),
                content: content.to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references: vec![],
                alias_map: std::collections::HashMap::new(),
                mtime_secs: 0,
            },
        )
    }

    fn make_file_with_refs(
        path: &str,
        content: &[u8],
        symbols: Vec<SymbolRecord>,
        references: Vec<ReferenceRecord>,
    ) -> (String, IndexedFile) {
        let (key, mut file) = make_file(path, content, symbols);
        file.references = references;
        (key, file)
    }

    fn make_ref(
        name: &str,
        qualified_name: Option<&str>,
        kind: ReferenceKind,
        line: u32,
        enclosing_symbol_index: Option<u32>,
    ) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: qualified_name.map(str::to_string),
            kind,
            byte_range: (line * 10, line * 10 + 6),
            line_range: (line, line),
            enclosing_symbol_index,
        }
    }

    fn make_live_index_ready(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let files_map = files
            .into_iter()
            .map(|(path, file)| (path, std::sync::Arc::new(file)))
            .collect::<HashMap<_, _>>();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
    }

    fn make_live_index_empty() -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: true,
            load_source: crate::live_index::store::IndexLoadSource::EmptyBootstrap,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
        }
    }

    fn make_live_index_tripped() -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let cb = CircuitBreakerState::new(0.10);
        for _ in 0..8 {
            cb.record_success();
        }
        for i in 0..2 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort(); // trips at 20% > 10%
        LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
        }
    }

    fn make_server_with_root(index: LiveIndex, repo_root: Option<PathBuf>) -> SymForgeServer {
        use crate::watcher::WatcherInfo;
        use parking_lot::Mutex;
        let shared = crate::live_index::SharedIndexHandle::shared(index);
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        SymForgeServer::new(
            shared,
            "test_project".to_string(),
            watcher_info,
            repo_root,
            None,
        )
    }

    fn make_server(index: LiveIndex) -> SymForgeServer {
        make_server_with_root(index, None)
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set_path(key: &'static str, value: &Path) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: called only in single-threaded test context; no concurrent env readers.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                // SAFETY: called only in single-threaded test context; no concurrent env readers.
                Some(previous) => unsafe {
                    std::env::set_var(self.key, previous);
                },
                // SAFETY: called only in single-threaded test context; no concurrent env readers.
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    struct CwdGuard {
        previous: PathBuf,
    }

    impl CwdGuard {
        fn set(path: &Path) -> Self {
            let previous = std::env::current_dir().expect("current dir");
            std::env::set_current_dir(path).expect("set current dir");
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            if std::env::set_current_dir(&self.previous).is_err() {
                std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("restore current dir");
            }
        }
    }

    fn run_git(repo_root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .expect("git command should start");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_git_repo() -> TempDir {
        let dir = TempDir::new().expect("temp git repo");
        run_git(dir.path(), &["init", "-q"]);
        run_git(
            dir.path(),
            &["config", "user.email", "symforge-tests@example.com"],
        );
        run_git(dir.path(), &["config", "user.name", "SymForge Tests"]);
        dir
    }

    // ── Loading guard tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_loading_guard_empty_returns_empty_message() {
        let server = make_server(make_live_index_empty());
        // Any non-health tool should return the empty guard message
        let result = server
            .get_symbol(Parameters(super::GetSymbolInput {
                path: "anything.rs".to_string(),
                name: "anything".to_string(),
                kind: None,
                symbol_line: None,
                targets: None,
                estimate: None,
            }))
            .await;
        assert_eq!(
            result,
            crate::protocol::format::empty_guard_message(),
            "empty index should return empty guard message"
        );
    }

    #[tokio::test]
    async fn test_loading_guard_circuit_breaker_returns_degraded_message() {
        let server = make_server(make_live_index_tripped());
        let result = server
            .get_symbol(Parameters(super::GetSymbolInput {
                path: "anything.rs".to_string(),
                name: "anything".to_string(),
                kind: None,
                symbol_line: None,
                targets: None,
                estimate: None,
            }))
            .await;
        assert!(
            result.starts_with("Index degraded:"),
            "tripped CB should return degraded message, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_health_always_responds_on_empty_index() {
        let server = make_server(make_live_index_empty());
        let result = server.health().await;
        // Health should NOT return the guard message; it should return actual health info
        assert!(
            !result.starts_with("Index not loaded"),
            "health should always respond, got: {result}"
        );
        assert!(
            result.contains("Status: Empty"),
            "health of empty index should show Empty, got: {result}"
        );
    }

    // ── Tool handler tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_file_context_outline_only_contains_path_and_symbol() {
        let sym = make_symbol("main", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/main.rs".to_string(),
                max_tokens: None,
                sections: Some(vec!["outline".to_string()]),
                estimate: None,
            }))
            .await;
        assert!(result.contains("src/main.rs"), "should contain path");
        assert!(result.contains("main"), "should contain symbol name");
        assert!(result.contains("Tip:"), "should include next-step hint");
    }

    #[tokio::test]
    async fn test_get_symbol_delegates_to_formatter() {
        let sym = make_symbol("foo", SymbolKind::Function, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_symbol(Parameters(super::GetSymbolInput {
                path: "src/lib.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                targets: None,
                estimate: None,
            }))
            .await;
        // Should return source body or not-found message — not a guard message
        assert!(
            !result.starts_with("Index"),
            "should not return guard message, got: {result}"
        );
        assert!(
            result.contains("Tip:"),
            "should include next-step hint: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_repo_map_full_uses_project_name() {
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_repo_map(Parameters(super::GetRepoMapInput {
                detail: Some("full".to_string()),
                path: None,
                depth: None,
                max_files: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("test_project"),
            "repo outline should use project_name, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_repo_map_full_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .get_repo_map(Parameters(super::GetRepoMapInput {
                detail: Some("full".to_string()),
                path: None,
                depth: None,
                max_files: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_get_repo_map_full_proxies_to_daemon_session() {
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set_path("SYMFORGE_HOME", daemon_home.path());
        let project = TempDir::new().expect("project dir");
        fs::create_dir_all(project.path().join("src")).expect("src dir");
        fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = crate::daemon::spawn_daemon("127.0.0.1")
            .await
            .expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let opened = reqwest::Client::new()
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&crate::daemon::OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(1234),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<crate::daemon::OpenProjectResponse>()
            .await
            .expect("open body");

        let daemon_client = crate::daemon::DaemonSessionClient::new_for_test(
            base_url,
            opened.project_id,
            opened.session_id,
            opened.project_name,
        );
        let server = SymForgeServer::new_daemon_proxy(daemon_client);

        let result = server
            .get_repo_map(Parameters(super::GetRepoMapInput {
                detail: Some("full".to_string()),
                path: None,
                depth: None,
                max_files: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("main.rs"),
            "remote repo outline should come from daemon project instance, got: {result}"
        );

        let _ = handle.shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_get_repo_map_returns_directory_breakdown() {
        let sym = make_symbol("main", SymbolKind::Function, 1, 3);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .get_repo_map(Parameters(super::GetRepoMapInput {
                detail: None,
                path: None,
                depth: None,
                max_files: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("Index: 1 files, 1 symbols"),
            "repo map should include totals header; got: {result}"
        );
        assert!(
            result.contains("src"),
            "repo map should include directory breakdown; got: {result}"
        );
        assert!(
            result.contains("Tip:"),
            "repo map should include next-step hint"
        );
    }

    #[tokio::test]
    async fn test_get_file_context_returns_outline_and_key_references() {
        let callee = make_symbol("target", SymbolKind::Function, 1, 3);
        let caller = make_symbol("caller", SymbolKind::Function, 1, 3);
        let target_file = make_file("src/target.rs", b"pub fn target() {}", vec![callee]);
        let caller_file = make_file_with_refs(
            "src/caller.rs",
            b"use crate::target;\nfn caller() { target(); }",
            vec![caller],
            vec![
                ReferenceRecord {
                    name: "target".to_string(),
                    qualified_name: Some("crate::target".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (4, 10),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "target".to_string(),
                    qualified_name: None,
                    kind: ReferenceKind::Call,
                    byte_range: (30, 36),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
        );
        let server = make_server(make_live_index_ready(vec![target_file, caller_file]));

        let result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/target.rs".to_string(),
                max_tokens: None,
                sections: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("src/target.rs"),
            "file context should include file header; got: {result}"
        );
        assert!(
            result.contains("Key references"),
            "file context should include reference section; got: {result}"
        );
        assert!(
            result.contains("Scope: path `src/target.rs`; all sections"),
            "file context should surface scope; got: {result}"
        );
        assert!(
            result.contains("src/caller.rs"),
            "file context should include caller file; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_context_shows_parse_diagnostic_for_partial_file() {
        let (key, mut file) = make_file(
            "Cargo.toml",
            b"[package]\nname = \"symforge\"\ninvalid = \"unterminated\n",
            vec![make_symbol("package", SymbolKind::Key, 1, 1)],
        );
        let diagnostic = crate::domain::ParseDiagnostic {
            parser: "toml_edit".to_string(),
            message: "missing closing quote".to_string(),
            line: Some(3),
            column: Some(11),
            byte_span: Some((28, 41)),
            fallback_used: true,
        };
        file.language = LanguageId::Toml;
        file.parse_status = ParseStatus::PartialParse {
            warning: diagnostic.summary(),
        };
        file.parse_diagnostic = Some(diagnostic);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "Cargo.toml".to_string(),
                max_tokens: None,
                sections: Some(vec!["outline".to_string()]),
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("Parse status: partial"),
            "file context should surface parse status; got: {result}"
        );
        assert!(
            result.contains("Diagnostic: toml_edit: missing closing quote (line 3, column 11) [fallback symbol extraction used]"),
            "file context should include the structured diagnostic; got: {result}"
        );
        assert!(
            result.contains("Byte span: 28..41"),
            "file context should include the diagnostic byte span; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_validate_file_syntax_reports_partial_toml_diagnostics() {
        let repo = TempDir::new().expect("temp repo");
        fs::write(
            repo.path().join("Cargo.toml"),
            "[package]\nname = \"symforge\"\nversion = \"0.1.0\"\ninvalid = \"unterminated\n",
        )
        .expect("write malformed toml");
        let server =
            make_server_with_root(make_live_index_empty(), Some(repo.path().to_path_buf()));

        let result = server
            .validate_file_syntax(Parameters(super::ValidateFileSyntaxInput {
                path: "Cargo.toml".to_string(),
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("Status: partial"),
            "validator should report a partial parse when fallback extraction succeeds; got: {result}"
        );
        assert!(
            result.contains("Diagnostic: toml_edit:"),
            "validator should show the parser source; got: {result}"
        );
        assert!(
            result.contains("fallback symbol extraction used"),
            "validator should surface fallback usage; got: {result}"
        );
        assert!(
            !result.contains("Symbols extracted: 0"),
            "validator should report recovered symbols for malformed TOML; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_context_shows_imports_and_used_by_sections() {
        let callee = make_symbol("target", SymbolKind::Function, 1, 3);
        let caller = make_symbol("caller", SymbolKind::Function, 1, 3);
        // caller.rs imports from crate::target and calls target().
        let caller_file = make_file_with_refs(
            "src/caller.rs",
            b"use crate::target;\nfn caller() { target(); }",
            vec![caller],
            vec![
                ReferenceRecord {
                    name: "target".to_string(),
                    qualified_name: Some("crate::target".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (4, 10),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "target".to_string(),
                    qualified_name: None,
                    kind: ReferenceKind::Call,
                    byte_range: (30, 36),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
        );
        let target_file = make_file("src/target.rs", b"pub fn target() {}", vec![callee]);
        let server = make_server(make_live_index_ready(vec![target_file, caller_file]));

        // Check caller.rs — should have "Imports from" section.
        let caller_result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/caller.rs".to_string(),
                max_tokens: Some(2000),
                sections: None,
                estimate: None,
            }))
            .await;
        assert!(
            caller_result.contains("Imports from"),
            "caller should show imports section; got: {caller_result}"
        );
        assert!(
            caller_result.contains("crate::target"),
            "caller should list crate::target as import source; got: {caller_result}"
        );

        // Check target.rs — should have "Used by" section.
        let target_result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/target.rs".to_string(),
                max_tokens: Some(2000),
                sections: None,
                estimate: None,
            }))
            .await;
        assert!(
            target_result.contains("Used by"),
            "target should show used-by section; got: {target_result}"
        );
        assert!(
            target_result.contains("src/caller.rs"),
            "target should list caller.rs as consumer; got: {target_result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_context_ignores_generic_name_noise_without_real_dependency() {
        let target = make_symbol("main", SymbolKind::Function, 1, 3);
        let helper = make_symbol("helper", SymbolKind::Function, 1, 4);
        let helper_main = make_symbol("main", SymbolKind::Function, 5, 7);
        let target_file = make_file("src/target.py", b"def main():\n    pass\n", vec![target]);
        let helper_file = make_file_with_refs(
            "scripts/helper.py",
            b"def helper():\n    main()\n\ndef main():\n    pass\n",
            vec![helper, helper_main],
            vec![ReferenceRecord {
                name: "main".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (18, 22),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            }],
        );
        let server = make_server(make_live_index_ready(vec![target_file, helper_file]));

        let result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/target.py".to_string(),
                max_tokens: None,
                sections: None,
                estimate: None,
            }))
            .await;

        assert!(
            !result.contains("scripts/helper.py"),
            "generic-name local calls should not be attributed as key references: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_returns_grouped_references() {
        let caller = make_symbol("caller", SymbolKind::Function, 1, 3);
        let caller_file = make_file_with_refs(
            "src/caller.rs",
            b"fn caller() { target(); }",
            vec![caller],
            vec![ReferenceRecord {
                name: "target".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (12, 18),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            }],
        );
        let server = make_server(make_live_index_ready(vec![caller_file]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "target".to_string(),
                file: None,
                path: None,
                symbol_kind: None,
                symbol_line: None,
                verbosity: None,
                bundle: None,
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("src/caller.rs"),
            "symbol context should group matches by file; got: {result}"
        );
        assert!(
            result.contains("in fn caller"),
            "symbol context should include enclosing symbol names; got: {result}"
        );
        assert!(
            result.contains("Scope: repo-wide symbol token `target`"),
            "default symbol context should surface repo-wide scope; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_surfaces_impl_guidance_for_zero_caller_struct() {
        let content = b"pub struct ProjectOrchestratorActor;\n\nimpl ProjectOrchestratorActor {\n    pub fn new() -> Self {\n        Self\n    }\n}\n\nimpl Actor for ProjectOrchestratorActor {\n    fn handle(&self) {}\n}\n";
        let symbols = vec![
            SymbolRecord {
                name: "ProjectOrchestratorActor".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 35),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "impl ProjectOrchestratorActor".to_string(),
                kind: SymbolKind::Impl,
                depth: 0,
                sort_order: 1,
                byte_range: (37, 117),
                line_range: (2, 6),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "impl Actor for ProjectOrchestratorActor".to_string(),
                kind: SymbolKind::Impl,
                depth: 0,
                sort_order: 2,
                byte_range: (119, content.len() as u32),
                line_range: (8, 10),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ];
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/actors.rs",
            content,
            symbols,
        )]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "ProjectOrchestratorActor".to_string(),
                file: None,
                path: Some("src/actors.rs".to_string()),
                symbol_kind: Some("struct".to_string()),
                symbol_line: None,
                verbosity: None,
                bundle: None,
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("0 direct callers"),
            "default symbol context should surface impl guidance for zero-caller structs: {result}"
        );
        assert!(
            result.contains("impl ProjectOrchestratorActor (src/actors.rs:3)"),
            "inherent impl suggestion should be surfaced: {result}"
        );
        assert!(
            result.contains("impl Actor for ProjectOrchestratorActor (src/actors.rs:9)"),
            "trait impl suggestion should be surfaced: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_exact_selector_excludes_unrelated_same_name_hits() {
        let target = make_file(
            "src/db.rs",
            b"pub fn connect() {}\n",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
        );
        let dependent = make_file_with_refs(
            "src/service.rs",
            b"use crate::db::connect;\nfn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, 0, None),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    1,
                    Some(0),
                ),
            ],
        );
        let unrelated = make_file_with_refs(
            "src/other.rs",
            b"fn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_ref("connect", None, ReferenceKind::Call, 0, Some(0))],
        );
        let server = make_server(make_live_index_ready(vec![target, dependent, unrelated]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: None,
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: Some(2),
                verbosity: None,
                bundle: None,
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("src/service.rs"),
            "expected dependent hit: {result}"
        );
        assert!(
            result.contains("Scope: path `src/db.rs`; exact selector line 2"),
            "exact selector scope should be explicit; got: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "unrelated same-name file should be excluded: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_exact_selector_requires_line_for_ambiguous_symbol() {
        let target = make_file(
            "src/db.rs",
            b"fn connect() { first(); }\nfn connect() { second(); }\n",
            vec![
                make_symbol_with_bytes("connect", SymbolKind::Function, 1, 1, (0, 25)),
                make_symbol_with_bytes("connect", SymbolKind::Function, 2, 2, (26, 52)),
            ],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: None,
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: None,
                verbosity: None,
                bundle: None,
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
        assert!(
            !result.contains("first()"),
            "ambiguous selector should not prepend an arbitrary definition: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    #[test]
    fn test_symbol_candidate_paths_sorts_all_matches() {
        let index = make_live_index_ready(vec![
            make_file(
                "src/zeta.rs",
                b"fn connect() {}\n",
                vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            ),
            make_file(
                "src/echo.rs",
                b"fn connect() {}\n",
                vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            ),
            make_file(
                "src/bravo.rs",
                b"fn connect() {}\n",
                vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            ),
            make_file(
                "src/alpha.rs",
                b"fn connect() {}\n",
                vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            ),
            make_file(
                "src/delta.rs",
                b"fn connect() {}\n",
                vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            ),
            make_file(
                "src/charlie.rs",
                b"fn connect() {}\n",
                vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            ),
        ]);

        let candidates = super::symbol_candidate_paths(&index, "connect");
        assert_eq!(candidates.len(), 6);
        assert_eq!(candidates.first().map(String::as_str), Some("src/alpha.rs"));
        assert_eq!(candidates.last().map(String::as_str), Some("src/zeta.rs"));
    }

    #[tokio::test]
    async fn test_get_symbol_context_exact_selector_respects_file_filter() {
        let target = make_file(
            "src/db.rs",
            b"pub fn connect() {}\n",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
        );
        let service = make_file_with_refs(
            "src/service.rs",
            b"use crate::db::connect;\nfn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, 0, None),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    1,
                    Some(0),
                ),
            ],
        );
        let api = make_file_with_refs(
            "src/api.rs",
            b"use crate::db::connect;\nfn expose() { connect(); }\n",
            vec![make_symbol("expose", SymbolKind::Function, 2, 2)],
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, 0, None),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    1,
                    Some(0),
                ),
            ],
        );
        let server = make_server(make_live_index_ready(vec![target, service, api]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: Some("src/service.rs".to_string()),
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: Some(2),
                verbosity: None,
                bundle: None,
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(result.contains("src/service.rs"), "got: {result}");
        assert!(!result.contains("src/api.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn test_analyze_file_impact_reports_symbol_change() {
        let repo = TempDir::new().expect("temp repo");
        fs::create_dir_all(repo.path().join("src")).expect("src dir");
        let source_path = repo.path().join("src").join("lib.rs");
        fs::write(&source_path, "pub fn new_name() {}\n").expect("write updated source");

        let old_symbol = make_symbol("old_name", SymbolKind::Function, 1, 1);
        let (key, file) = make_file("src/lib.rs", b"pub fn old_name() {}\n", vec![old_symbol]);
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );

        let result = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "src/lib.rs".to_string(),
                new_file: None,
                include_co_changes: None,
                co_changes_limit: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("new_name"),
            "impact tool should re-read the file from repo_root and report new symbols; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_analyze_file_impact_unchanged_shows_status() {
        // Use setup_edit_test so the index holds real parsed symbols (correct byte_ranges).
        // Then call analyze_file_impact WITHOUT modifying the file — should report unchanged.
        let source = b"pub fn alpha() {}\npub fn beta() {}\n";
        let (dir, server, _file_path) = setup_edit_test(source);
        let _ = dir; // keep temp dir alive

        let result = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "src/lib.rs".to_string(),
                new_file: None,
                include_co_changes: None,
                co_changes_limit: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("indexed and unchanged"),
            "unchanged file should report 'indexed and unchanged'; got: {result}"
        );
        assert!(
            result.contains("Symbols: 2"),
            "should show symbol count; got: {result}"
        );
        assert!(
            result.contains("what_changed"),
            "should suggest what_changed; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_analyze_file_impact_deleted_file() {
        let source = b"pub fn gone() {}\n";
        let (dir, server, file_path) = setup_edit_test(source);
        let _ = dir; // keep temp dir alive

        // Delete the file from disk to simulate external deletion.
        fs::remove_file(&file_path).expect("remove source");

        let result = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "src/lib.rs".to_string(),
                new_file: None,
                include_co_changes: None,
                co_changes_limit: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("not found on disk"),
            "deleted file should report 'not found on disk'; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_analyze_file_impact_auto_indexes_new_file_when_missing_from_index() {
        let repo = TempDir::new().expect("temp repo");
        fs::create_dir_all(repo.path().join("src")).expect("src dir");
        let source_path = repo.path().join("src").join("fresh.rs");
        fs::write(&source_path, "pub fn fresh_symbol() {}\n").expect("write new source");

        let server = make_server_with_root(
            make_live_index_ready(vec![]),
            Some(repo.path().to_path_buf()),
        );

        let result = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "src/fresh.rs".to_string(),
                new_file: None,
                include_co_changes: None,
                co_changes_limit: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("[Indexed, 0 callers yet]"),
            "new on-disk file should auto-index even without new_file=true; got: {result}"
        );
        assert!(
            result.contains("fresh_symbol") || result.contains("1 fn"),
            "auto-indexed new file should report symbol summary; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_index_folder_rebinds_repo_root_for_local_impact_analysis() {
        let repo = TempDir::new().expect("temp repo");
        fs::create_dir_all(repo.path().join("scratch")).expect("scratch dir");
        let source_path = repo.path().join("scratch").join("impact_case.rs");
        fs::write(&source_path, "pub fn old_name() {}\n").expect("write initial source");

        let server = make_server(make_live_index_empty());
        let index_result = server
            .index_folder(Parameters(super::IndexFolderInput {
                path: repo.path().display().to_string(),
            }))
            .await;

        assert!(
            index_result.contains("Indexed 1 files"),
            "index_folder should load the temp repo, got: {index_result}"
        );

        fs::write(&source_path, "pub fn new_name() {}\n").expect("write updated source");
        let outside = TempDir::new().expect("outside cwd");
        let _cwd_guard = CwdGuard::set(outside.path());

        let impact = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "scratch/impact_case.rs".to_string(),
                new_file: None,
                include_co_changes: None,
                co_changes_limit: None,
                estimate: None,
            }))
            .await;

        assert!(
            impact.contains("new_name"),
            "impact analysis should keep using the indexed repo root after index_folder, got: {impact}"
        );

        let outline = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "scratch/impact_case.rs".to_string(),
                max_tokens: None,
                sections: Some(vec!["outline".to_string()]),
                estimate: None,
            }))
            .await;

        assert!(
            outline.contains("new_name"),
            "impact analysis must not replace the indexed file with an empty parse, got: {outline}"
        );
    }

    #[tokio::test]
    async fn test_index_folder_rebinds_repo_root_for_local_what_changed_git_mode() {
        let repo = init_git_repo();
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "fn foo() {}\n").expect("write initial file");
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "init", "-q"]);

        let server = make_server(make_live_index_empty());
        let index_result = server
            .index_folder(Parameters(super::IndexFolderInput {
                path: repo.path().display().to_string(),
            }))
            .await;
        assert!(
            index_result.contains("Indexed 1 files"),
            "index_folder should load the temp repo, got: {index_result}"
        );

        fs::write(
            repo.path().join("src/lib.rs"),
            "fn foo() { println!(\"changed\"); }\n",
        )
        .expect("modify tracked file");
        let outside = TempDir::new().expect("outside cwd");
        let _cwd_guard = CwdGuard::set(outside.path());

        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: None,
                uncommitted: None,
                path_prefix: None,
                language: None,
                code_only: None,
                include_symbol_diff: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("src/lib.rs"),
            "what_changed should keep using the indexed repo root after index_folder, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_index_folder_can_reindex_same_root_twice_locally() {
        let repo = TempDir::new().expect("temp repo");
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "pub fn first() {}\n").expect("write initial file");

        let server = make_server(make_live_index_empty());
        let first = server
            .index_folder(Parameters(super::IndexFolderInput {
                path: repo.path().display().to_string(),
            }))
            .await;
        assert!(
            first.contains("Indexed 1 files"),
            "first local index_folder should succeed, got: {first}"
        );

        fs::write(repo.path().join("src/lib.rs"), "pub fn second() {}\n").expect("update file");
        let second = server
            .index_folder(Parameters(super::IndexFolderInput {
                path: repo.path().display().to_string(),
            }))
            .await;
        assert!(
            second.contains("Indexed 1 files"),
            "second local index_folder should also succeed, got: {second}"
        );

        let outline = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/lib.rs".to_string(),
                max_tokens: None,
                sections: Some(vec!["outline".to_string()]),
                estimate: None,
            }))
            .await;
        assert!(
            outline.contains("second"),
            "second local index_folder should refresh the same-root index, got: {outline}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_returns_results() {
        let sym = make_symbol("find_user", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn find_user() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: Some("find".to_string()),
                kind: None,
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("find_user"),
            "should find matching symbol, got: {result}"
        );
        assert!(
            result.contains("Match type: constrained (prefix tier)"),
            "search_symbols should expose trust envelope, got: {result}"
        );
        assert!(
            result.contains("Source authority: current index"),
            "search_symbols should expose source authority, got: {result}"
        );
        assert!(
            result.contains("Tip:"),
            "search_symbols should include next-step hint"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_kind_filter_returns_only_requested_kind() {
        let function = make_symbol("JobRunner", SymbolKind::Function, 1, 5);
        let class = make_symbol("Job", SymbolKind::Class, 6, 10);
        let (key, file) = make_file(
            "src/lib.rs",
            b"fn JobRunner() {}\nstruct Job {}",
            vec![function, class],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: Some("job".to_string()),
                kind: Some("class".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("class Job"),
            "class should remain visible: {result}"
        );
        assert!(
            !result.contains("fn JobRunner"),
            "function should be filtered out: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_hides_generated_and_test_noise_by_default() {
        let server = make_server(make_live_index_ready(vec![
            make_file(
                "src/job.rs",
                b"struct Job {}\n",
                vec![make_symbol("Job", SymbolKind::Class, 1, 1)],
            ),
            make_file(
                "src/generated/job_generated.rs",
                b"struct JobGenerated {}\n",
                vec![make_symbol("JobGenerated", SymbolKind::Class, 2, 2)],
            ),
            make_file(
                "tests/job_test.rs",
                b"struct JobTest {}\n",
                vec![make_symbol("JobTest", SymbolKind::Class, 3, 3)],
            ),
        ]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: Some("job".to_string()),
                kind: Some("class".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("class Job"),
            "expected primary hit: {result}"
        );
        assert!(
            !result.contains("JobGenerated"),
            "generated symbol noise should be hidden by default: {result}"
        );
        assert!(
            !result.contains("JobTest"),
            "test symbol noise should be hidden by default: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_hides_inline_test_module_symbols() {
        // src/lib.rs is NOT a test file, but contains `mod tests { struct TestHelper; }`.
        // With include_tests=false (default), symbols inside the inline test module
        // should be hidden even though the file itself is classified as source code.
        let content = b"struct Foo {}\nmod tests {\n  struct TestHelper;\n}\n";
        let symbols = vec![
            SymbolRecord {
                name: "Foo".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 13),
                item_byte_range: Some((0, 13)),
                line_range: (0, 0),
                doc_byte_range: None,
            },
            SymbolRecord {
                name: "tests".to_string(),
                kind: SymbolKind::Module,
                depth: 0,
                sort_order: 1,
                byte_range: (14, 48),
                item_byte_range: Some((14, 48)),
                line_range: (1, 3),
                doc_byte_range: None,
            },
            SymbolRecord {
                name: "TestHelper".to_string(),
                kind: SymbolKind::Struct,
                depth: 1,
                sort_order: 2,
                byte_range: (28, 46),
                item_byte_range: Some((28, 46)),
                line_range: (2, 2),
                doc_byte_range: None,
            },
        ];
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/lib.rs",
            content,
            symbols,
        )]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: None,
                kind: Some("struct".to_string()),
                path_prefix: Some("src/".to_string()),
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None, // default: false
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("Foo"),
            "source struct should appear: {result}"
        );
        assert!(
            !result.contains("TestHelper"),
            "struct inside inline mod tests should be hidden: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_tool_can_include_generated_without_tests() {
        let server = make_server(make_live_index_ready(vec![
            make_file(
                "src/job.rs",
                b"struct Job {}\n",
                vec![make_symbol("Job", SymbolKind::Class, 1, 1)],
            ),
            make_file(
                "src/generated/job_generated.rs",
                b"struct JobGenerated {}\n",
                vec![make_symbol("JobGenerated", SymbolKind::Class, 2, 2)],
            ),
            make_file(
                "tests/job_test.rs",
                b"struct JobTest {}\n",
                vec![make_symbol("JobTest", SymbolKind::Class, 3, 3)],
            ),
        ]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: Some("job".to_string()),
                kind: Some("class".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: Some(true),
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("class Job"),
            "expected primary hit: {result}"
        );
        assert!(
            result.contains("JobGenerated"),
            "generated symbol should be visible when opted in: {result}"
        );
        assert!(
            !result.contains("JobTest"),
            "test noise should stay hidden without explicit opt-in: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_tool_can_include_tests_without_generated() {
        let server = make_server(make_live_index_ready(vec![
            make_file(
                "src/job.rs",
                b"struct Job {}\n",
                vec![make_symbol("Job", SymbolKind::Class, 1, 1)],
            ),
            make_file(
                "src/generated/job_generated.rs",
                b"struct JobGenerated {}\n",
                vec![make_symbol("JobGenerated", SymbolKind::Class, 2, 2)],
            ),
            make_file(
                "tests/job_test.rs",
                b"struct JobTest {}\n",
                vec![make_symbol("JobTest", SymbolKind::Class, 3, 3)],
            ),
        ]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: Some("job".to_string()),
                kind: Some("class".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: Some(true),
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("class Job"),
            "expected primary hit: {result}"
        );
        assert!(
            !result.contains("JobGenerated"),
            "generated noise should stay hidden without explicit opt-in: {result}"
        );
        assert!(
            result.contains("JobTest"),
            "test symbol should be visible when opted in: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_tool_respects_scope_language_limit_and_kind() {
        let rust_model = make_file(
            "src/models/job.rs",
            b"struct Job {}\nfn JobRunner() {}\n",
            vec![
                make_symbol("Job", SymbolKind::Class, 1, 1),
                make_symbol("JobRunner", SymbolKind::Function, 2, 2),
            ],
        );
        let mut ts_ui = make_file(
            "src/ui/job.ts",
            b"class JobCard {}\nclass JobList {}\n",
            vec![
                make_symbol("JobCard", SymbolKind::Class, 1, 1),
                make_symbol("JobList", SymbolKind::Class, 2, 2),
            ],
        );
        ts_ui.1.language = LanguageId::TypeScript;
        let server = make_server(make_live_index_ready(vec![rust_model, ts_ui]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: Some("job".to_string()),
                kind: Some("class".to_string()),
                path_prefix: Some("src/ui".to_string()),
                language: Some("TypeScript".to_string()),
                limit: Some(1),
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("1 matches in 1 files"),
            "expected bounded output: {result}"
        );
        assert!(
            result.contains("class JobCard"),
            "expected scoped class hit: {result}"
        );
        assert!(
            !result.contains("JobList"),
            "limit should truncate later hits: {result}"
        );
        assert!(
            !result.contains("src/models/job.rs"),
            "path scope should exclude rust model: {result}"
        );
        assert!(
            !result.contains("fn JobRunner"),
            "kind filter should exclude function: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_browse_by_kind_without_query() {
        let sym_fn = make_symbol("do_work", SymbolKind::Function, 1, 5);
        let sym_struct = make_symbol("Worker", SymbolKind::Class, 6, 10);
        let (key, file) = make_file(
            "src/protocol/worker.rs",
            b"fn do_work() {}\nstruct Worker {}",
            vec![sym_fn, sym_struct],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: None,
                kind: Some("fn".to_string()),
                path_prefix: Some("src/protocol/".to_string()),
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("do_work"),
            "browse mode should return fn symbols, got: {result}"
        );
        assert!(
            !result.contains("Worker"),
            "browse mode kind filter should exclude structs, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_rejects_fully_unbounded() {
        let sym = make_symbol("anything", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn anything() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: None,
                kind: None,
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("requires at least one of"),
            "fully unbounded browse should be rejected, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_browse_default_limit_20() {
        // Create 25 distinct symbols — browse mode should return only 20
        let syms: Vec<_> = (1..=25)
            .map(|i| make_symbol(&format!("sym_{i:02}"), SymbolKind::Function, i, i))
            .collect();
        let content = b"fn placeholder() {}";
        let (key, file) = make_file("src/lib.rs", content, syms);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: None,
                kind: Some("fn".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("20 matches in"),
            "browse mode default limit should be 20, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_query_mode_still_defaults_to_50() {
        // Create 55 distinct symbols — query mode should return 50
        let syms: Vec<_> = (1..=55)
            .map(|i| make_symbol(&format!("item_{i:02}"), SymbolKind::Function, i, i))
            .collect();
        let content = b"fn placeholder() {}";
        let (key, file) = make_file("src/lib.rs", content, syms);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: Some("item".to_string()),
                kind: None,
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("50 matches in"),
            "query mode default limit should remain 50, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_returns_results() {
        let (key, file) = make_file("src/lib.rs", b"fn find_user() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("find".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;
        assert!(
            result.contains("find_user"),
            "should find matching text, got: {result}"
        );
        assert!(
            result.contains("Match type: constrained (literal)"),
            "search_text should expose trust envelope, got: {result}"
        );
        assert!(
            result.contains("Scope: repo-wide; tests filtered; generated filtered"),
            "search_text should expose applied scope, got: {result}"
        );
        assert!(
            result.contains("Tip:"),
            "search_text should include next-step hint"
        );
    }

    #[tokio::test]
    async fn test_search_text_trust_envelope_reports_partial_parse_and_truncation() {
        let (key, mut file) = make_file(
            "Cargo.toml",
            b"needle one\nneedle two\nneedle three\n",
            vec![make_symbol("package", SymbolKind::Key, 1, 1)],
        );
        let diagnostic = crate::domain::ParseDiagnostic {
            parser: "toml_edit".to_string(),
            message: "missing closing quote".to_string(),
            line: Some(3),
            column: Some(11),
            byte_span: Some((28, 41)),
            fallback_used: true,
        };
        file.language = LanguageId::Toml;
        file.parse_status = ParseStatus::PartialParse {
            warning: diagnostic.summary(),
        };
        file.parse_diagnostic = Some(diagnostic);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: Some(2),
                max_per_file: Some(2),
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(result.contains("Parse state: partial"), "got: {result}");
        assert!(
            result.contains("Completeness: truncated by result cap (1 more omitted)"),
            "got: {result}"
        );
        assert!(
            result.contains("Evidence: line anchors `Cargo.toml:1`"),
            "got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_terms_or_returns_results() {
        let (key, file) = make_file(
            "src/lib.rs",
            b"// TODO: first\n// FIXME: second\n// NOTE: ignored",
            vec![],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: None,
                terms: Some(vec!["TODO".to_string(), "FIXME".to_string()]),
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;
        assert!(
            result.contains("TODO: first"),
            "TODO term should match: {result}"
        );
        assert!(
            result.contains("FIXME: second"),
            "FIXME term should match: {result}"
        );
        assert!(
            !result.contains("NOTE: ignored"),
            "unmatched line should be absent: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_terms_annotates_matched_term() {
        let sym_a = make_symbol("fn_alpha", SymbolKind::Function, 0, 0);
        let sym_b = make_symbol("fn_beta", SymbolKind::Function, 1, 1);
        let file = make_file(
            "src/lib.rs",
            b"fn fn_alpha() { alpha_value }\nfn fn_beta() { beta_value }\n",
            vec![sym_a, sym_b],
        );
        let server = make_server(make_live_index_ready(vec![file]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: None,
                terms: Some(vec!["alpha_value".to_string(), "beta_value".to_string()]),
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(
            result.contains("[term: alpha_value]"),
            "should annotate alpha term: {result}"
        );
        assert!(
            result.contains("[term: beta_value]"),
            "should annotate beta term: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_hides_generated_and_test_noise_by_default() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/real.rs", b"needle visible", vec![]),
            make_file("tests/generated/noise.rs", b"needle hidden", vec![]),
        ]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(
            result.contains("src/real.rs"),
            "expected visible file: {result}"
        );
        assert!(
            !result.contains("tests/generated/noise.rs"),
            "generated/test noise should be hidden by default: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_respects_scope_language_and_caps() {
        let mut ts_app = make_file(
            "src/app.ts",
            b"needle one\nneedle two\nneedle three\n",
            vec![],
        );
        ts_app.1.language = LanguageId::TypeScript;
        let mut ts_lib = make_file("src/lib.ts", b"needle four\nneedle five\n", vec![]);
        ts_lib.1.language = LanguageId::TypeScript;
        let noise = make_file(
            "tests/generated/noise.ts",
            b"needle hidden\nneedle hidden two\n",
            vec![],
        );
        let rust = make_file("src/lib.rs", b"needle rust\nneedle rust two\n", vec![]);
        let server = make_server(make_live_index_ready(vec![ts_app, ts_lib, noise, rust]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: Some("src".to_string()),
                language: Some("TypeScript".to_string()),
                limit: Some(3),
                max_per_file: Some(2),
                include_generated: Some(false),
                include_tests: Some(false),
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(result.contains("src/app.ts"), "expected app.ts: {result}");
        assert!(result.contains("src/lib.ts"), "expected lib.ts: {result}");
        assert!(
            !result.contains("needle three"),
            "per-file cap should truncate app.ts: {result}"
        );
        assert!(
            !result.contains("needle five"),
            "total cap should truncate final result set: {result}"
        );
        assert!(
            !result.contains("tests/generated/noise.ts"),
            "noise file should be excluded: {result}"
        );
        assert!(
            !result.contains("src/lib.rs"),
            "language filter should exclude Rust: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_context_renders_windows() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/lib.rs",
            b"line 1\nline 2\nneedle 3\nline 4\nneedle 5\nline 6\nline 7\nline 8\nneedle 9\nline 10\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: Some(1),
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(
            result.contains("  2: line 2"),
            "context line missing: {result}"
        );
        assert!(
            result.contains("> 3: needle 3"),
            "match marker missing: {result}"
        );
        assert!(result.contains("  ..."), "separator missing: {result}");
    }

    #[tokio::test]
    async fn test_search_text_tool_respects_glob_and_exclude_glob() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/app.ts", b"needle app\n", vec![]),
            make_file("src/app.spec.ts", b"needle spec\n", vec![]),
            make_file("src/nested/feature.ts", b"needle nested\n", vec![]),
            make_file("src/lib.rs", b"needle rust\n", vec![]),
        ]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: Some("src".to_string()),
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: Some("src/**/*.ts".to_string()),
                exclude_glob: Some("**/*.spec.ts".to_string()),
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(result.contains("src/app.ts"), "expected app.ts: {result}");
        assert!(
            result.contains("src/nested/feature.ts"),
            "expected nested ts file: {result}"
        );
        assert!(
            !result.contains("src/app.spec.ts"),
            "exclude_glob should suppress spec file: {result}"
        );
        assert!(
            !result.contains("src/lib.rs"),
            "include glob should suppress rust file: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_reports_invalid_glob() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/app.ts",
            b"needle app\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: Some("[".to_string()),
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(
            result.contains("Invalid glob for `glob`"),
            "expected invalid glob error, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_respects_case_sensitive_and_whole_word() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/lib.rs",
            b"Needle\nneedle\nNeedleCase\nNeedle suffix\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("Needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: Some(true),
                whole_word: Some(true),
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(
            result.contains("  1: Needle"),
            "exact whole-word match missing: {result}"
        );
        assert!(
            result.contains("  4: Needle suffix"),
            "whole-word prefix match on a line should remain visible: {result}"
        );
        assert!(
            !result.contains("  2: needle"),
            "case-sensitive search should exclude lowercase line: {result}"
        );
        assert!(
            !result.contains("  3: NeedleCase"),
            "whole-word search should exclude embedded identifier match: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_reports_regex_whole_word_rejection() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/lib.rs",
            b"needle\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: Some(true),
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: Some(true),
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(
            result.contains("whole_word is not supported when `regex=true`"),
            "expected regex/whole_word rejection, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_auto_corrects_double_escaped_regex() {
        let (key, file) = make_file("src/handler.rs", b"fn handle_request() {}\n", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        // Use a double-escaped \s (arrives as \\s in the JSON string)
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some(r"fn\\s+handle_".to_string()),
                terms: None,
                regex: Some(true),
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;

        assert!(
            result.contains("handle_request"),
            "should find match after auto-correcting double-escaped regex, got: {result}"
        );
        assert!(
            result.contains("auto-corrected"),
            "should include auto-correction note, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_files_returns_ranked_paths() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/protocol/tools.rs", b"fn a() {}", vec![]),
            make_file("src/sidecar/tools.rs", b"fn b() {}", vec![]),
            make_file("src/protocol/tools_helper.rs", b"fn c() {}", vec![]),
        ]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: "protocol/tools.rs".to_string(),
                limit: Some(20),
                current_file: None,
                changed_with: None,
                resolve: None,
                estimate: None,
                max_tokens: None,
                rank_by: None,
            }))
            .await;
        assert!(result.contains("2 matching files"), "got: {result}");
        assert!(
            result.contains("── Strong path matches ──"),
            "got: {result}"
        );
        assert!(
            result.contains("Match type: constrained (tiered path relevance)"),
            "search_files should expose trust envelope, got: {result}"
        );
        assert!(
            result.contains("Scope: ranked indexed file paths"),
            "search_files should expose search scope, got: {result}"
        );
        assert!(result.contains("── Basename matches ──"), "got: {result}");
        assert!(result.contains("src/protocol/tools.rs"), "got: {result}");
        assert!(result.contains("src/sidecar/tools.rs"), "got: {result}");
        assert!(!result.contains("tools_helper.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn test_search_files_not_found() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: "src/service.rs".to_string(),
                limit: None,
                current_file: None,
                changed_with: None,
                resolve: None,
                estimate: None,
                max_tokens: None,
                rank_by: None,
            }))
            .await;
        assert_eq!(result, "No indexed source files matching 'src/service.rs'");
    }

    #[tokio::test]
    async fn test_search_files_changed_with_returns_graceful_message() {
        // Without git temporal data loaded, should return informative message
        let (key, file) = make_file("src/daemon.rs", b"fn foo() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: String::new(),
                limit: None,
                current_file: None,
                changed_with: Some("src/daemon.rs".to_string()),
                resolve: None,
                estimate: None,
                max_tokens: None,
                rank_by: None,
            }))
            .await;
        // Without git temporal data, should return informative message (not an error/panic)
        assert!(!result.contains("panic"), "should not panic, got: {result}");
        assert!(
            result.contains("temporal") || result.contains("git"),
            "should mention temporal data status, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_files_changed_with_surfaces_weak_candidates() {
        let (key_a, file_a) = make_file("src/auth.rs", b"fn auth() {}", vec![]);
        let (key_b, file_b) = make_file("src/routes.rs", b"fn routes() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![
            (key_a, file_a),
            (key_b, file_b),
        ]));

        server
            .index
            .update_git_temporal(crate::live_index::git_temporal::GitTemporalIndex {
                files: HashMap::from([(
                    "src/auth.rs".to_string(),
                    crate::live_index::git_temporal::GitFileHistory {
                        commit_count: 4,
                        churn_score: 0.7,
                        last_commit: crate::live_index::git_temporal::CommitSummary {
                            hash: "abc1234".to_string(),
                            timestamp: "2026-04-02T12:00:00Z".to_string(),
                            author: "Tester".to_string(),
                            message_head: "touch auth".to_string(),
                            days_ago: 1.0,
                        },
                        contributors: vec![],
                        co_changes: vec![],
                        weak_co_changes: vec![crate::live_index::git_temporal::CoChangeEntry {
                            path: "src/routes.rs".to_string(),
                            coupling_score: 0.12,
                            shared_commits: 1,
                        }],
                    },
                )]),
                stats: crate::live_index::git_temporal::GitTemporalStats {
                    total_commits_analyzed: 12,
                    analysis_window_days: 90,
                    hotspots: vec![],
                    most_coupled: vec![],
                    computed_at: std::time::SystemTime::now(),
                    compute_duration: Duration::ZERO,
                },
                state: crate::live_index::git_temporal::GitTemporalState::Ready,
            });

        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: String::new(),
                limit: None,
                current_file: None,
                changed_with: Some("src/auth.rs".to_string()),
                resolve: None,
                estimate: None,
                max_tokens: None,
                rank_by: None,
            }))
            .await;

        assert!(
            result.contains("weak heuristic (git-temporal coupling)"),
            "{result}"
        );
        assert!(result.contains("Low confidence"), "{result}");
        assert!(result.contains("src/routes.rs"), "{result}");
    }

    #[tokio::test]
    async fn test_search_files_resolve_returns_exact_match() {
        let (key, file) = make_file("src/protocol/tools.rs", b"fn tool() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: "src/protocol/tools.rs".to_string(),
                limit: None,
                current_file: None,
                changed_with: None,
                resolve: Some(true),
                estimate: None,
                max_tokens: None,
                rank_by: None,
            }))
            .await;
        assert!(
            result.contains("Match type: exact (resolve)"),
            "got: {result}"
        );
        assert!(
            result.contains("Source authority: current index"),
            "got: {result}"
        );
        assert!(result.contains("src/protocol/tools.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn test_search_files_resolve_returns_ambiguous_matches() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/lib.rs", b"fn src_lib() {}", vec![]),
            make_file("tests/lib.rs", b"fn test_lib() {}", vec![]),
        ]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: "lib.rs".to_string(),
                limit: None,
                current_file: None,
                changed_with: None,
                resolve: Some(true),
                estimate: None,
                max_tokens: None,
                rank_by: None,
            }))
            .await;
        assert!(
            result.contains("Ambiguous path hint 'lib.rs'"),
            "got: {result}"
        );
        assert!(
            result.contains("Match type: constrained (resolve candidates)"),
            "got: {result}"
        );
        assert!(result.contains("src/lib.rs"), "got: {result}");
        assert!(result.contains("tests/lib.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn test_health_returns_status_fields() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server.health().await;
        assert!(result.contains("Status:"), "should have Status field");
        assert!(result.contains("Files:"), "should have Files field");
        assert!(result.contains("Symbols:"), "should have Symbols field");
    }

    #[tokio::test]
    async fn test_get_symbol_batch_symbol_lookup() {
        let sym = make_symbol("bar", SymbolKind::Function, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn bar() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_symbol(Parameters(super::GetSymbolInput {
                path: String::new(),
                name: String::new(),
                kind: None,
                symbol_line: None,
                targets: Some(vec![super::SymbolTarget {
                    path: "src/lib.rs".to_string(),
                    name: Some("bar".to_string()),
                    kind: None,
                    symbol_line: None,
                    start_byte: None,
                    end_byte: None,
                }]),
                estimate: None,
            }))
            .await;
        assert!(
            !result.starts_with("Index"),
            "should not return guard message, got: {result}"
        );
        assert!(
            result.contains("fn bar() {"),
            "symbol lookup branch should return symbol body, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_batch_code_slice() {
        let content = b"fn foo() { let x = 1; }";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_symbol(Parameters(super::GetSymbolInput {
                path: String::new(),
                name: String::new(),
                kind: None,
                symbol_line: None,
                targets: Some(vec![super::SymbolTarget {
                    path: "src/lib.rs".to_string(),
                    name: None,
                    kind: None,
                    symbol_line: None,
                    start_byte: Some(0),
                    end_byte: Some(8),
                }]),
                estimate: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "code slice should include path header, got: {result}"
        );
        assert!(
            result.contains("fn foo()"),
            "code slice should include content, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_returns_result() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        // since=0 (far past) → all files are "newer"
        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: Some(0),
                git_ref: None,
                uncommitted: None,
                path_prefix: None,
                language: None,
                code_only: None,
                include_symbol_diff: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "what_changed since epoch should list all files, got: {result}"
        );
        assert!(
            result.contains("Match type: exact (timestamp compare)"),
            "timestamp mode should report authority envelope: {result}"
        );
        assert!(
            result.contains("Source authority: current index"),
            "timestamp mode should report index authority: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_timestamp_respects_filters() {
        let (rust_key, rust_file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let (doc_key, doc_file) = make_file("docs/readme.md", b"# hi", vec![]);
        let server = make_server(make_live_index_ready(vec![
            (rust_key, rust_file),
            (doc_key, doc_file),
        ]));
        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: Some(0),
                git_ref: None,
                uncommitted: None,
                path_prefix: Some("src".to_string()),
                language: Some("rust".to_string()),
                code_only: None,
                include_symbol_diff: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "filtered timestamp mode should keep Rust path: {result}"
        );
        assert!(
            !result.contains("docs/readme.md"),
            "filtered timestamp mode should exclude non-matching path: {result}"
        );
        assert!(
            result.contains("Scope: timestamp since `0`; path prefix `src`; language `rust`"),
            "filtered timestamp mode should expose scope: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_defaults_to_uncommitted_git_changes() {
        let repo = init_git_repo();
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "fn foo() {}\n").expect("write initial file");
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "init", "-q"]);
        fs::write(
            repo.path().join("src/lib.rs"),
            "fn foo() { println!(\"changed\"); }\n",
        )
        .expect("modify tracked file");

        let (key, file) = make_file(
            "src/lib.rs",
            b"fn foo() { println!(\"changed\"); }\n",
            vec![],
        );
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );
        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: None,
                uncommitted: None,
                path_prefix: None,
                language: None,
                code_only: None,
                include_symbol_diff: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "default mode should surface uncommitted git changes: {result}"
        );
        assert!(
            result.contains("Source authority: git working tree"),
            "uncommitted mode should expose git authority: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_recovers_repo_root_from_cwd_when_missing() {
        let repo = init_git_repo();
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "fn foo() {}\n").expect("write initial file");
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "init", "-q"]);
        fs::write(
            repo.path().join("src/lib.rs"),
            "fn foo() { println!(\"changed\"); }\n",
        )
        .expect("modify tracked file");

        let (key, file) = make_file(
            "src/lib.rs",
            b"fn foo() { println!(\"changed\"); }\n",
            vec![],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let _cwd_guard = CwdGuard::set(repo.path());

        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: None,
                uncommitted: None,
                path_prefix: None,
                language: None,
                code_only: None,
                include_symbol_diff: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "what_changed should lazily recover the git root from cwd: {result}"
        );
        assert!(
            result.contains("Source authority: git working tree"),
            "recovered git mode should expose git authority: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_reports_repo_root_guidance_when_missing() {
        let home = dirs::home_dir().expect("home dir");
        let _cwd_guard = CwdGuard::set(&home);
        let server = make_server(make_live_index_empty());

        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: None,
                uncommitted: None,
                path_prefix: None,
                language: None,
                code_only: None,
                include_symbol_diff: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("No repo root attached; call index_folder(path=...) or pass since=..."),
            "missing repo root should return actionable guidance: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_git_ref_reports_diffed_files() {
        let repo = init_git_repo();
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "fn foo() {}\n").expect("write initial file");
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "init", "-q"]);
        fs::write(
            repo.path().join("src/lib.rs"),
            "fn foo() { println!(\"changed\"); }\n",
        )
        .expect("modify tracked file");

        let (key, file) = make_file(
            "src/lib.rs",
            b"fn foo() { println!(\"changed\"); }\n",
            vec![],
        );
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );
        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: Some("HEAD".to_string()),
                uncommitted: None,
                path_prefix: None,
                language: None,
                code_only: None,
                include_symbol_diff: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "git_ref mode should show changed files: {result}"
        );
        assert!(
            result.contains("Source authority: git ref diff"),
            "git_ref mode should expose git diff authority: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_returns_content() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("line 1"),
            "should return file content, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_not_found() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "nonexistent.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "File not found: nonexistent.rs. Use search_files to find the correct path."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_line_range_preserves_public_contract() {
        let content = b"line 1\nline 2\nline 3\nline 4";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: Some(2),
                end_line: Some(3),
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "line 2\nline 3");
    }

    #[tokio::test]
    async fn test_get_file_content_show_line_numbers_renders_numbered_full_read() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: Some(true),
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "1: line 1\n2: line 2\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_header_and_line_numbers_render_range_shell() {
        let content = b"line 1\nline 2\nline 3\nline 4";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: Some(2),
                end_line: Some(3),
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: Some(true),
                header: Some(true),
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "src/lib.rs [lines 2-3]\n2: line 2\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_around_line_renders_numbered_excerpt() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: Some(3),
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "2: line 2\n3: line 3\n4: line 4");
    }

    #[tokio::test]
    async fn test_get_file_content_allows_header_with_around_line() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: Some(2),
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: Some(true),
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        // Should succeed (not reject) — header is now allowed with around_line.
        assert!(
            !result.starts_with("Invalid"),
            "header + around_line should be allowed, got: {result}"
        );
        assert!(
            result.contains("line 2"),
            "should contain the around_line content, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_around_line_with_explicit_range() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: Some(2),
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: Some(2),
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `around_line` cannot be combined with `start_line` or `end_line`. Valid with `around_line`: `context_lines`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_around_match_renders_first_numbered_excerpt() {
        let content = b"line 1\nTODO first\nline 3\nTODO second\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: Some("todo".to_string()),
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "1: line 1\n2: TODO first\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_chunked_read_renders_header_and_numbered_lines() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: Some(2),
                max_lines: Some(2),
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "src/lib.rs [chunk 2/3, lines 3-4]\n3: line 3\n4: line 4"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_reports_out_of_range_chunk() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: Some(3),
                max_lines: Some(2),
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "Chunk 3 out of range for src/lib.rs (2 chunks)");
    }

    #[tokio::test]
    async fn test_get_file_content_reports_missing_around_match() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: Some("needle".to_string()),
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "No matches for 'needle' in src/lib.rs");
    }

    #[tokio::test]
    async fn test_get_file_content_selects_requested_match_occurrence() {
        let content = b"line 1\nTODO first\nline 3\nTODO second\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: Some("todo".to_string()),
                match_occurrence: Some(2),
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "3: line 3\n4: TODO second\n5: line 5");
    }

    #[tokio::test]
    async fn test_get_file_content_reports_missing_match_occurrence() {
        let content = b"line 1\nTODO first\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: Some("todo".to_string()),
                match_occurrence: Some(2),
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Match occurrence 2 for 'todo' not found in src/lib.rs; 1 match(es) available at lines 2"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_freshens_stale_indexed_file_before_rendering() {
        let repo = tempfile::tempdir().unwrap();
        let src_dir = repo.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let disk_path = src_dir.join("lib.rs");
        fs::write(&disk_path, "fn fresh() {}\n").unwrap();

        let (key, file) = make_file("src/lib.rs", b"fn stale() {}\n", vec![]);
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );

        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;

        assert!(
            result.contains("fn fresh() {}"),
            "expected fresh disk content, got: {result}"
        );
        assert!(
            !result.contains("fn stale() {}"),
            "stale indexed content should not be served after refresh: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_normalizes_exact_path_before_freshness_and_lookup() {
        let repo = tempfile::tempdir().unwrap();
        let src_dir = repo.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let disk_path = src_dir.join("lib.rs");
        fs::write(&disk_path, "fn fresh() {}\n").unwrap();

        let (key, file) = make_file("src/lib.rs", b"fn stale() {}\n", vec![]);
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );

        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "./src\\lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;

        assert!(
            result.contains("fn fresh() {}"),
            "expected fresh disk content, got: {result}"
        );

        let guard = server.index.read();
        assert!(guard.get_file("src/lib.rs").is_some());
        assert!(guard.get_file("./src\\lib.rs").is_none());
    }

    #[tokio::test]
    async fn test_get_file_content_around_symbol_renders_numbered_excerpt() {
        let content = b"line 1\nfn connect() {}\nline 3";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "1: line 1\n2: fn connect() {}\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_reports_ambiguous_around_symbol_without_symbol_line() {
        let content = b"fn connect() {}\nline 2\nfn connect() {}";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![
                make_symbol("connect", SymbolKind::Function, 0, 0),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Ambiguous symbol selector for connect in src/lib.rs; pass `symbol_line` to disambiguate. Candidates: 0, 2"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_around_symbol_symbol_line_disambiguates() {
        let content = b"fn connect() {}\nline 2\nfn connect() {}";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![
                make_symbol("connect", SymbolKind::Function, 0, 0),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: Some(3),
                context_lines: Some(0),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "3: fn connect() {}");
    }

    #[tokio::test]
    async fn test_get_file_content_reports_missing_around_symbol() {
        let content = b"fn helper() {}\nline 2";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![make_symbol("helper", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "No symbol connect in src/lib.rs. Close matches: helper. Use get_file_context with sections=['outline'] for the full list (1 symbols)."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_chunked_read_with_other_selectors() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: Some(2),
                end_line: None,
                chunk_index: Some(1),
                max_lines: Some(2),
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: chunked reads (`chunk_index` + `max_lines`) cannot be combined with `start_line`, `end_line`, `around_line`, or `around_match`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_chunk_index_without_max_lines() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: Some(1),
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `chunk_index` requires `max_lines`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_around_symbol_with_other_selectors() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: Some(2),
                end_line: None,
                chunk_index: Some(1),
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `around_symbol` cannot be combined with `start_line`, `end_line`, `around_line`, `around_match`, or `chunk_index`. Valid with `around_symbol`: `symbol_line`, `context_lines`, `max_lines`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_symbol_line_without_around_symbol() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: Some(2),
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `symbol_line` requires `around_symbol`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_around_match_with_other_selectors() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: Some(3),
                chunk_index: None,
                max_lines: None,
                around_line: Some(2),
                around_match: Some("line".to_string()),
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `around_match` cannot be combined with `start_line`, `end_line`, or `around_line`. Valid with `around_match`: `context_lines`."
        );
    }

    // ── Mode enum tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_mode_symbol_with_around_symbol_works() {
        let content = b"line 1\nfn connect() {}\nline 3";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("symbol".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("fn connect()"),
            "expected symbol content, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_mode_symbol_without_around_symbol_errors() {
        let content = b"line 1\nline 2";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("symbol".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "mode=symbol requires around_symbol");
    }

    #[tokio::test]
    async fn test_mode_lines_with_cross_mode_flag_errors() {
        let content = b"line 1\nline 2";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("lines".to_string()),
                start_line: Some(1),
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("foo".to_string()),
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("mode=lines conflicts with around_symbol"),
            "expected cross-mode error, got: {result}"
        );
        assert!(
            result.contains("Use mode=symbol"),
            "expected guidance, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_mode_lines_rejects_match_occurrence_cross_mode_flag() {
        let content = b"line 1\nline 2";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("lines".to_string()),
                start_line: Some(1),
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: Some(2),
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("mode=lines conflicts with match_occurrence"),
            "expected cross-mode error, got: {result}"
        );
        assert!(
            result.contains("Use mode=match"),
            "expected guidance, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_no_mode_legacy_flags_backward_compatible() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: Some(1),
                end_line: Some(2),
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("line 1"),
            "expected file content, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_mode_search_not_implemented() {
        let content = b"line 1";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("search".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "mode 'search' is not yet implemented");
    }

    #[tokio::test]
    async fn test_mode_symbol_with_context_lines_works() {
        let content = b"line 1\nfn connect() {}\nline 3";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("symbol".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(10),
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("fn connect()"),
            "expected symbol content with context, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_mode_symbol_rejects_match_occurrence_cross_mode_flag() {
        let content = b"line 1\nfn connect() {}\nline 3";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("symbol".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: Some(2),
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("mode=symbol conflicts with match_occurrence"),
            "expected cross-mode error, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_mode_unknown_errors() {
        let content = b"line 1";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("fancy".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Unknown mode 'fancy'. Valid modes: lines, symbol, match, chunk."
        );
    }

    #[tokio::test]
    async fn test_mode_match_without_around_match_errors() {
        let content = b"line 1";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("match".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "mode=match requires around_match");
    }

    #[tokio::test]
    async fn test_match_occurrence_requires_around_match() {
        let content = b"line 1";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: Some(1),
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `match_occurrence` requires `around_match`."
        );
    }

    #[tokio::test]
    async fn test_match_occurrence_must_be_positive() {
        let content = b"line 1\nTODO";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("match".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: Some("todo".to_string()),
                match_occurrence: Some(0),
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `match_occurrence` must be 1 or greater."
        );
    }

    #[tokio::test]
    async fn test_mode_chunk_without_required_flags_errors() {
        let content = b"line 1";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("chunk".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert_eq!(result, "mode=chunk requires chunk_index");
    }

    #[tokio::test]
    async fn test_mode_chunk_cross_mode_errors() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("chunk".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: Some(1),
                max_lines: Some(10),
                around_line: None,
                around_match: Some("line".to_string()),
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("mode=chunk conflicts with around_match"),
            "expected cross-mode error, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_mode_chunk_rejects_match_occurrence_cross_mode_flag() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: Some("chunk".to_string()),
                start_line: None,
                end_line: None,
                chunk_index: Some(1),
                max_lines: Some(10),
                around_line: None,
                around_match: None,
                match_occurrence: Some(2),
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: None,
                limit: None,
            }))
            .await;
        assert!(
            result.contains("mode=chunk conflicts with match_occurrence"),
            "expected cross-mode error, got: {result}"
        );
        assert!(
            result.contains("Use mode=match"),
            "expected guidance, got: {result}"
        );
    }

    // ── get_file_content serde / alias tests ───────────────────────────────

    #[test]
    fn test_get_file_content_input_deserializes_offset_and_limit() {
        let json = r#"{"path":"x","offset":10,"limit":5}"#;
        let input: super::GetFileContentInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.offset, Some(10));
        assert_eq!(input.limit, Some(5));
        assert!(input.start_line.is_none());
        assert!(input.end_line.is_none());
    }

    #[test]
    fn test_get_file_content_input_defaults_all_none() {
        let json = r#"{"path":"x"}"#;
        let input: super::GetFileContentInput = serde_json::from_str(json).unwrap();
        assert!(input.offset.is_none());
        assert!(input.limit.is_none());
        assert!(input.start_line.is_none());
    }

    #[test]
    fn test_get_file_content_input_rejects_unknown_field() {
        let json = r#"{"path":"x","typo_field":1}"#;
        let result = serde_json::from_str::<super::GetFileContentInput>(json);
        assert!(result.is_err(), "unknown field should be rejected");
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("typo_field"),
            "error should name the unknown field, got: {err}"
        );
    }

    #[test]
    fn test_get_file_content_input_rejects_non_numeric_offset() {
        let json = r#"{"path":"x","offset":"not-a-number"}"#;
        assert!(
            serde_json::from_str::<super::GetFileContentInput>(json).is_err(),
            "non-numeric offset should fail to deserialize"
        );
    }

    // ── normalize_file_content_aliases unit tests ───────────────────────────

    fn make_alias_input(offset: Option<u32>, limit: Option<u32>) -> super::GetFileContentInput {
        super::GetFileContentInput {
            path: "x".to_string(),
            mode: None,
            start_line: None,
            end_line: None,
            chunk_index: None,
            max_lines: None,
            around_line: None,
            around_match: None,
            match_occurrence: None,
            around_symbol: None,
            symbol_line: None,
            context_lines: None,
            show_line_numbers: None,
            header: None,
            estimate: None,
            offset,
            limit,
        }
    }

    #[test]
    fn test_normalize_aliases_offset_and_limit() {
        let mut input = make_alias_input(Some(10), Some(5));
        super::normalize_file_content_aliases(&mut input).unwrap();
        assert_eq!(input.start_line, Some(11));
        assert_eq!(input.end_line, Some(15));
        assert!(input.offset.is_none());
        assert!(input.limit.is_none());
    }

    #[test]
    fn test_normalize_aliases_zero_offset() {
        let mut input = make_alias_input(Some(0), Some(100));
        super::normalize_file_content_aliases(&mut input).unwrap();
        assert_eq!(input.start_line, Some(1));
        assert_eq!(input.end_line, Some(100));
    }

    #[test]
    fn test_normalize_aliases_no_limit_reads_to_end() {
        let mut input = make_alias_input(Some(50), None);
        super::normalize_file_content_aliases(&mut input).unwrap();
        assert_eq!(input.start_line, Some(51));
        assert!(input.end_line.is_none());
    }

    #[test]
    fn test_normalize_aliases_no_offset_defaults_to_start() {
        let mut input = make_alias_input(None, Some(20));
        super::normalize_file_content_aliases(&mut input).unwrap();
        assert_eq!(input.start_line, Some(1));
        assert_eq!(input.end_line, Some(20));
    }

    #[test]
    fn test_normalize_aliases_single_line() {
        let mut input = make_alias_input(Some(0), Some(1));
        super::normalize_file_content_aliases(&mut input).unwrap();
        assert_eq!(input.start_line, Some(1));
        assert_eq!(input.end_line, Some(1));
    }

    #[test]
    fn test_normalize_aliases_neither_set_is_noop() {
        let mut input = make_alias_input(None, None);
        super::normalize_file_content_aliases(&mut input).unwrap();
        assert!(input.start_line.is_none());
        assert!(input.end_line.is_none());
    }

    #[test]
    fn test_normalize_aliases_rejects_conflict_with_start_line() {
        let mut input = make_alias_input(Some(10), None);
        input.start_line = Some(5);
        let err = super::normalize_file_content_aliases(&mut input)
            .err()
            .unwrap();
        assert!(err.contains("`start_line`"), "got: {err}");
    }

    #[test]
    fn test_normalize_aliases_rejects_conflict_with_end_line() {
        let mut input = make_alias_input(Some(10), Some(5));
        input.end_line = Some(20);
        let err = super::normalize_file_content_aliases(&mut input)
            .err()
            .unwrap();
        assert!(err.contains("`end_line`"), "got: {err}");
    }

    #[test]
    fn test_normalize_aliases_rejects_conflict_with_around_symbol() {
        let mut input = make_alias_input(Some(10), None);
        input.around_symbol = Some("foo".to_string());
        let err = super::normalize_file_content_aliases(&mut input)
            .err()
            .unwrap();
        assert!(err.contains("`around_symbol`"), "got: {err}");
    }

    #[test]
    fn test_normalize_aliases_rejects_limit_zero() {
        let mut input = make_alias_input(None, Some(0));
        let err = super::normalize_file_content_aliases(&mut input)
            .err()
            .unwrap();
        assert!(err.contains("must be 1 or greater"), "got: {err}");
    }

    #[test]
    fn test_normalize_aliases_rejects_conflict_with_mode() {
        let mut input = make_alias_input(Some(10), None);
        input.mode = Some("lines".to_string());
        let err = super::normalize_file_content_aliases(&mut input)
            .err()
            .unwrap();
        assert!(err.contains("`mode`"), "got: {err}");
    }

    #[tokio::test]
    async fn test_get_file_content_offset_limit_returns_sliced_window() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: Some(1),
                limit: Some(2),
            }))
            .await;
        // offset=1 → start_line=2, limit=2 → end_line=3
        assert_eq!(result, "line 2\nline 3", "got: {result}");
    }

    #[tokio::test]
    async fn test_get_file_content_offset_limit_conflict_rejected() {
        let content = b"line 1\nline 2";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                mode: None,
                start_line: Some(1),
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                match_occurrence: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
                estimate: None,
                offset: Some(1),
                limit: None,
            }))
            .await;
        assert!(
            result.contains("cannot be combined"),
            "conflict should be rejected, got: {result}"
        );
    }

    // ── Explore tool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_explore_concept_returns_results() {
        let sym = make_symbol("Error", SymbolKind::Enum, 0, 5);
        let content = b"pub enum Error {\n    NotFound,\n    Io(std::io::Error),\n}\nimpl Error {\n    fn is_retryable(&self) -> bool { false }\n}\n";
        let (key, file) = make_file("src/error.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(5),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("Exploring: Error Handling"),
            "should have concept label, got: {result}"
        );
        assert!(
            result.contains("Error"),
            "should find Error symbol, got: {result}"
        );
        assert!(
            !result.contains("Auto-derived cluster:"),
            "static concept matches should not emit fallback-derived clustering: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_concept_enrichment_shows_annotation() {
        let sym = make_symbol("Error", SymbolKind::Enum, 0, 5);
        let content = b"use thiserror::Error;\npub enum Error {}\n";
        let refs = vec![make_ref(
            "thiserror",
            Some("thiserror::Error"),
            ReferenceKind::Import,
            0,
            None,
        )];
        let (key, file) = make_file_with_refs("src/error.rs", content, vec![sym], refs);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(5),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("Enriched with project imports: thiserror"),
            "should annotate convention enrichment, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_concept_enrichment_from_cargo_manifest() {
        // thiserror has NO import references — only a Cargo.toml dependency symbol.
        // The enrichment should still surface it via the manifest path.
        let sym = make_symbol("Error", SymbolKind::Enum, 0, 5);
        let content = b"#[derive(thiserror::Error)]\npub enum Error {}\n";
        let (key, file) = make_file("src/error.rs", content, vec![sym]);

        // Cargo.toml with a dependencies.thiserror symbol (no import refs needed).
        let cargo_sym = SymbolRecord {
            name: "dependencies.thiserror".to_string(),
            kind: SymbolKind::Key,
            depth: 1,
            sort_order: 0,
            byte_range: (0, 20),
            item_byte_range: Some((0, 20)),
            line_range: (0, 0),
            doc_byte_range: None,
        };
        let cargo_content = b"[dependencies]\nthiserror = \"2.0\"\n";
        let cargo_file = IndexedFile {
            relative_path: "Cargo.toml".to_string(),
            language: LanguageId::Toml,
            classification: crate::domain::FileClassification::for_code_path("Cargo.toml"),
            content: cargo_content.to_vec(),
            symbols: vec![cargo_sym],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: cargo_content.len() as u64,
            content_hash: "test".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        };

        let server = make_server(make_live_index_ready(vec![
            (key, file),
            ("Cargo.toml".to_string(), cargo_file),
        ]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(5),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("Enriched with project imports: thiserror"),
            "should enrich from Cargo.toml manifest deps, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_fallback_returns_results() {
        let content = b"fn process_data() { let x = 42; }\n";
        let sym = make_symbol("process_data", SymbolKind::Function, 0, 0);
        let (key, file) = make_file("src/main.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "process data".to_string(),
                limit: Some(5),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("Exploring:"),
            "should have explore header, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_empty_query() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "".to_string(),
                limit: None,
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("Explore requires a non-empty query"),
            "should reject empty query, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_depth_2_shows_signatures() {
        let content = b"pub enum Error {\n    NotFound,\n    Io(std::io::Error),\n}\nimpl Error {\n    fn is_retryable(&self) -> bool { false }\n}\n";
        let sym = SymbolRecord {
            name: "Error".to_string(),
            kind: SymbolKind::Enum,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (0, 5),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (key, file) = make_file("src/error.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(5),
                depth: Some(2),
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("pub enum Error"),
            "depth 2 should show signature, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_multi_term_scoring_injects_enclosing_symbol() {
        // BurstTracker contains "debounce" in its body; it won't match the symbol search for
        // "debounce" (name mismatch) but the text search will find the word and inject the
        // enclosing symbol into match_counts, so it should appear in results.
        // Query uses "burst debounce" — no concept match — to exercise fallback multi-term scoring.
        let content = b"pub struct BurstTracker {\n    debounce: u32,\n    burst_count: u32,\n}\n";
        let burst_tracker = SymbolRecord {
            name: "BurstTracker".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (0, 3),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (key, file) = make_file("src/watcher.rs", content, vec![burst_tracker]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "burst debounce".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("BurstTracker"),
            "BurstTracker should appear via enclosing-symbol injection from debounce text hit, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_cooccurrence_ranks_multi_term_symbol_above_single_term_noise() {
        let actor_content =
            b"pub fn handle_supervisor_evt() -> Result<(), ActorProcessingErr> { Ok(()) }\n";
        let actor_sym = SymbolRecord {
            name: "handle_supervisor_evt".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, actor_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let noise_content = b"pub fn recovery_words() {}\n";
        let noise_sym = SymbolRecord {
            name: "recovery_words".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, noise_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let actor_file = make_file("src/actors/supervisor.rs", actor_content, vec![actor_sym]);
        let noise_file = make_file("src/auth/recovery.rs", noise_content, vec![noise_sym]);
        let server = make_server(make_live_index_ready(vec![actor_file, noise_file]));

        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "actor supervision and error recovery".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        let actor_pos = result.find("handle_supervisor_evt");
        let noise_pos = result.find("recovery_words");
        assert!(
            actor_pos.is_some(),
            "actor-supervision symbol should appear in results: {result}"
        );
        assert!(
            noise_pos.is_none() || actor_pos < noise_pos,
            "multi-term actor match should outrank single-term recovery noise: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_fallback_surfaces_auto_derived_cluster() {
        let content = b"pub struct EventFanoutBus;\npub fn dispatch_delivery_event() {}\n";
        let bus_symbol = SymbolRecord {
            name: "EventFanoutBus".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let delivery_symbol = SymbolRecord {
            name: "dispatch_delivery_event".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 1,
            byte_range: (24, content.len() as u32),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (key, file) = make_file(
            "src/bus/fanout.rs",
            content,
            vec![bus_symbol, delivery_symbol],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "event fanout delivery".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("Auto-derived cluster:"),
            "fallback explore should surface a derived cluster: {result}"
        );
        assert!(
            result.contains("Promoted signals:"),
            "derived cluster should expose promoted symbols: {result}"
        );
        assert!(
            result.contains("EventFanoutBus") || result.contains("dispatch_delivery_event"),
            "derived cluster should promote repo-specific symbol names: {result}"
        );
        assert!(
            result.contains("src/bus/fanout.rs"),
            "derived cluster should surface the seed file it learned from: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_hides_test_scaffolding_for_broad_fallback_query() {
        let code_content =
            b"pub struct AuthenticationMiddleware;\npub fn enforce_authentication_middleware() {}\n";
        let code_symbols = vec![
            SymbolRecord {
                name: "AuthenticationMiddleware".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 36),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "enforce_authentication_middleware".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (37, code_content.len() as u32),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ];
        let test_content = b"pub fn test_authentication_middleware_format() {}\n";
        let test_symbol = SymbolRecord {
            name: "test_authentication_middleware_format".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, test_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let reason_content = b"pub enum ReasonCode { Authentication, RateLimit }\n";
        let reason_symbol = SymbolRecord {
            name: "ReasonCode".to_string(),
            kind: SymbolKind::Enum,
            depth: 0,
            sort_order: 0,
            byte_range: (0, reason_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let server = make_server(make_live_index_ready(vec![
            make_file("src/auth/middleware.rs", code_content, code_symbols),
            make_file(
                "src/protocol/format/tests.rs",
                test_content,
                vec![test_symbol],
            ),
            make_file(
                "src/frontend/scanner.rs",
                reason_content,
                vec![reason_symbol],
            ),
        ]));

        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "authentication middleware".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("AuthenticationMiddleware"),
            "real auth middleware symbol should appear in results: {result}"
        );
        assert!(
            !result.contains("test_authentication_middleware_format"),
            "test scaffolding should be hidden by default for broad fallback explore: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_multi_term_fallback_demotes_single_term_code_noise() {
        let auth_content = b"pub fn attach_authentication_middleware() { enforce_guard(); }\n";
        let auth_symbol = SymbolRecord {
            name: "attach_authentication_middleware".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, auth_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let noise_content = b"pub struct RateLimitKey;\n";
        let noise_symbol = SymbolRecord {
            name: "RateLimitKey".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, noise_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let server = make_server(make_live_index_ready(vec![
            make_file("src/auth/middleware.rs", auth_content, vec![auth_symbol]),
            make_file("src/rate_limit/key.rs", noise_content, vec![noise_symbol]),
        ]));

        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "authentication middleware guard".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        let auth_pos = result.find("attach_authentication_middleware");
        let noise_pos = result.find("RateLimitKey");
        assert!(
            auth_pos.is_some(),
            "multi-term auth middleware symbol should appear in results: {result}"
        );
        assert!(
            noise_pos.is_none() || auth_pos < noise_pos,
            "multi-term domain hit should outrank single-term code noise: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_module_path_boosting() {
        let content = b"pub struct WatcherInfo {\n    debounce_ms: u64,\n}\n";
        let watcher_sym = SymbolRecord {
            name: "WatcherInfo".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (0, 2),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (key, file) = make_file("src/watcher/mod.rs", content, vec![watcher_sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "watcher".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("WatcherInfo"),
            "WatcherInfo should appear via module-path boosting: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_concept_plus_remainder() {
        let content = b"pub fn handle_watcher_error() {}\n";
        let sym = SymbolRecord {
            name: "handle_watcher_error".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (key, file) = make_file("src/watcher/errors.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling in the watcher".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("handle_watcher_error") || result.contains("watcher"),
            "concept+remainder should surface watcher results: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_exact_concept_no_remainder() {
        let content = b"pub enum SymForgeError {}\n";
        let sym = SymbolRecord {
            name: "SymForgeError".to_string(),
            kind: SymbolKind::Enum,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (key, file) = make_file("src/error.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("SymForgeError"),
            "exact concept should find error symbols: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_hides_vendor_noise_by_default() {
        // Vendor file should be filtered out when include_noise is false (default).
        let vendor_content = b"pub fn vendor_func() {}\n";
        let vendor_sym = SymbolRecord {
            name: "vendor_func".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, vendor_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let src_content = b"pub fn process_error() {}\n";
        let src_sym = SymbolRecord {
            name: "process_error".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, src_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (vkey, vfile) = make_file("vendor/somelib/lib.rs", vendor_content, vec![vendor_sym]);
        let (skey, sfile) = make_file("src/error.rs", src_content, vec![src_sym]);
        let server = make_server(make_live_index_ready(vec![(vkey, vfile), (skey, sfile)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            !result.contains("vendor_func"),
            "vendor symbol should be hidden by default: {result}"
        );
        assert!(
            result.contains("process_error"),
            "src symbol should still appear: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_includes_vendor_when_include_noise_true() {
        let vendor_content = b"pub struct VendorError {}\n";
        let vendor_sym = SymbolRecord {
            name: "VendorError".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, vendor_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (vkey, vfile) = make_file("vendor/somelib/lib.rs", vendor_content, vec![vendor_sym]);
        let server = make_server(make_live_index_ready(vec![(vkey, vfile)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: Some(true),
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("VendorError"),
            "vendor symbol should appear with include_noise=true: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_noise_hint_shown_when_results_hidden() {
        let vendor_content = b"pub struct VendorError {}\n";
        let vendor_sym = SymbolRecord {
            name: "VendorError".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, vendor_content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let (vkey, vfile) = make_file("vendor/somelib/lib.rs", vendor_content, vec![vendor_sym]);
        let server = make_server(make_live_index_ready(vec![(vkey, vfile)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(10),
                depth: None,
                include_noise: None,
                language: None,
                path_prefix: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("vendor/generated files hidden")
                && result.contains("include_noise=true"),
            "should show noise hint when results are hidden: {result}"
        );
    }

    // ── INFR-05: No v1 tools in server ──────────────────────────────────────

    #[test]
    fn test_no_v1_tools_in_server() {
        // Build the tool list by inspecting what tool_router() generates
        let server = make_server(make_live_index_ready(vec![]));
        let router = server.tool_router.clone();
        let tool_names: Vec<String> = router
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();

        let v1_tools = [
            "cancel_index_run",
            "checkpoint_now",
            "resume_index_run",
            "get_provenance",
            "get_trust",
            "verify_chunk",
        ];

        for v1_tool in &v1_tools {
            assert!(
                !tool_names.iter().any(|n| n == v1_tool),
                "v1 tool '{v1_tool}' must not appear in server tool list (INFR-05); found: {tool_names:?}"
            );
        }
    }

    #[test]
    fn test_tools_registered_count_is_stable() {
        let server = make_server(make_live_index_ready(vec![]));
        let tool_count = server.tool_router.list_all().len();
        // Sanity check: we should have a reasonable number of tools.
        // Update this lower bound when removing tools; it prevents accidental regressions.
        assert!(
            tool_count >= 24,
            "server should expose at least 24 tools; found {tool_count}"
        );
    }

    #[tokio::test]
    async fn test_trace_symbol_delegates_to_formatter() {
        let target = make_file(
            "src/lib.rs",
            b"fn process() {}\n",
            vec![make_symbol("process", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .trace_symbol(Parameters(super::TraceSymbolInput {
                path: "src/lib.rs".to_string(),
                name: "process".to_string(),
                kind: None,
                symbol_line: None,
                sections: None,
                verbosity: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(result.contains("fn process"), "got: {result}");
        assert!(result.contains("Callers (0)"), "got: {result}");
    }

    #[tokio::test]
    async fn test_get_symbol_context_trace_mode() {
        let target = make_file(
            "src/lib.rs",
            b"fn process() {}\n",
            vec![make_symbol("process", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "process".to_string(),
                file: None,
                path: Some("src/lib.rs".to_string()),
                symbol_kind: None,
                symbol_line: None,
                verbosity: None,
                bundle: None,
                sections: Some(vec!["dependents".to_string()]),
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("fn process"),
            "trace mode should show definition, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_trace_mode_requires_path() {
        let target = make_file(
            "src/lib.rs",
            b"fn process() {}\n",
            vec![make_symbol("process", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "process".to_string(),
                file: None,
                path: None,
                symbol_kind: None,
                symbol_line: None,
                verbosity: None,
                bundle: None,
                sections: Some(vec![]),
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("Error: sections requires"),
            "should require path, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_inspect_match_delegates_to_formatter() {
        let target = make_file(
            "src/lib.rs",
            b"fn process() {\n    let x = 1;\n}\n",
            vec![make_symbol("process", SymbolKind::Function, 0, 2)],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 2,
                context: None,
                sibling_limit: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        // Verify excerpt
        assert!(result.contains("2:     let x = 1;"), "got: {result}");
        // Verify enclosing symbol (line_range is 0-based internally, displayed as 1-based)
        assert!(
            result.contains("Enclosing symbol: fn process (lines 1-3)"),
            "got: {result}"
        );
    }

    #[tokio::test]
    async fn test_inspect_match_default_sibling_limit_caps_at_10() {
        // Build a file with 15 top-level functions (depth 0).
        let mut syms = Vec::new();
        for i in 0u32..15 {
            syms.push(make_symbol(&format!("fn_{i}"), SymbolKind::Function, i, i));
        }
        let content = (0u32..15)
            .map(|i| format!("fn fn_{i}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");
        let target = make_file("src/lib.rs", content.as_bytes(), syms);
        let server = make_server(make_live_index_ready(vec![target]));

        // No sibling_limit — defaults to 10.
        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 1,
                context: Some(0),
                sibling_limit: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        // Count sibling rows (each rendered with two-space indent).
        let sibling_rows = result
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("  ..."))
            .count();
        assert!(
            sibling_rows <= 10,
            "expected ≤10 siblings, got {sibling_rows}; output:\n{result}"
        );
        assert!(
            result.contains("... and 5 more siblings"),
            "expected overflow hint; got:\n{result}"
        );
    }

    #[tokio::test]
    async fn test_inspect_match_sibling_limit_5_shows_exactly_5() {
        let mut syms = Vec::new();
        for i in 0u32..12 {
            syms.push(make_symbol(&format!("fn_{i}"), SymbolKind::Function, i, i));
        }
        let content = (0u32..12)
            .map(|i| format!("fn fn_{i}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");
        let target = make_file("src/lib.rs", content.as_bytes(), syms);
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 1,
                context: Some(0),
                sibling_limit: Some(5),
                estimate: None,
                max_tokens: None,
            }))
            .await;

        let sibling_rows = result
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("  ..."))
            .count();
        assert_eq!(
            sibling_rows, 5,
            "expected exactly 5 siblings; output:\n{result}"
        );
        assert!(
            result.contains("... and 7 more siblings"),
            "expected overflow hint; got:\n{result}"
        );
    }

    #[tokio::test]
    async fn test_inspect_match_sibling_limit_0_hides_siblings() {
        let mut syms = Vec::new();
        for i in 0u32..5 {
            syms.push(make_symbol(&format!("fn_{i}"), SymbolKind::Function, i, i));
        }
        let content = (0u32..5)
            .map(|i| format!("fn fn_{i}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");
        let target = make_file("src/lib.rs", content.as_bytes(), syms);
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 1,
                context: Some(0),
                sibling_limit: Some(0),
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            !result.contains("Nearby siblings:"),
            "siblings should be hidden; got:\n{result}"
        );
    }

    #[tokio::test]
    async fn test_get_repo_map_tree_returns_tree() {
        let sym = make_symbol("main", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_repo_map(Parameters(super::GetRepoMapInput {
                detail: Some("tree".to_string()),
                path: None,
                depth: None,
                max_files: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("main.rs"),
            "get_repo_map(tree) should include file name; got: {result}"
        );
        assert!(
            result.contains("symbol"),
            "get_repo_map(tree) should show symbol count; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_repo_map_tree_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .get_repo_map(Parameters(super::GetRepoMapInput {
                detail: Some("tree".to_string()),
                path: None,
                depth: None,
                max_files: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_find_references_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "process".to_string(),
                kind: None,
                path: None,
                symbol_kind: None,
                symbol_line: None,
                limit: None,
                max_per_file: None,
                compact: None,
                mode: None,
                direction: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_find_dependents_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/lib.rs".to_string(),
                limit: None,
                max_per_file: None,
                format: None,
                compact: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_get_symbol_context_bundle_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "process".to_string(),
                file: None,
                path: Some("src/lib.rs".to_string()),
                symbol_kind: None,
                symbol_line: None,
                verbosity: None,
                bundle: Some(true),
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_get_symbol_context_bundle_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "process".to_string(),
                file: None,
                path: Some("src/nonexistent.rs".to_string()),
                symbol_kind: None,
                symbol_line: None,
                verbosity: None,
                bundle: Some(true),
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;
        assert!(result.contains("File not found"), "got: {result}");
    }

    #[tokio::test]
    async fn test_get_symbol_context_bundle_exact_selector_uses_line_and_exact_callers() {
        let target = make_file(
            "src/db.rs",
            b"fn connect() { first(); }\nfn connect() { second(); }\n",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let dependent = make_file_with_refs(
            "src/service.rs",
            b"use crate::db::connect;\nfn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, 0, None),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    1,
                    Some(0),
                ),
            ],
        );
        let unrelated = make_file_with_refs(
            "src/other.rs",
            b"fn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_ref("connect", None, ReferenceKind::Call, 0, Some(0))],
        );
        let server = make_server(make_live_index_ready(vec![target, dependent, unrelated]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: None,
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: Some(2),
                verbosity: None,
                bundle: Some(true),
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("src/service.rs"),
            "expected dependent hit: {result}"
        );
        assert!(
            result.contains("Match type: exact"),
            "bundle mode should surface match type; got: {result}"
        );
        assert!(
            result.contains("Source authority: current index"),
            "bundle mode should surface source authority; got: {result}"
        );
        assert!(
            result.contains("Scope: path `src/db.rs`; bundle mode"),
            "bundle mode should surface scope; got: {result}"
        );
        assert!(
            result.contains("Evidence: symbol anchor `src/db.rs:"),
            "bundle mode should surface evidence anchor; got: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "unrelated same-name file should be excluded: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_bundle_exact_selector_requires_line_for_ambiguous_symbol() {
        let target = make_file(
            "src/db.rs",
            b"fn connect() {}\nfn connect() {}\n",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: None,
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: None,
                verbosity: None,
                bundle: Some(true),
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_references_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "nonexistent_xyz".to_string(),
                kind: None,
                path: None,
                symbol_kind: None,
                symbol_line: None,
                limit: None,
                max_per_file: None,
                compact: None,
                mode: None,
                direction: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        // Should get "No references found" not a guard message
        assert!(result.contains("No references found"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_references_exact_selector_excludes_unrelated_same_name_hits() {
        let target = make_file(
            "src/db.rs",
            b"pub fn connect() {}\n",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
        );
        let dependent = make_file_with_refs(
            "src/service.rs",
            b"use crate::db::connect;\nfn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, 0, None),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    1,
                    Some(0),
                ),
            ],
        );
        let unrelated = make_file_with_refs(
            "src/other.rs",
            b"fn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_ref("connect", None, ReferenceKind::Call, 0, Some(0))],
        );
        let server = make_server(make_live_index_ready(vec![target, dependent, unrelated]));

        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "connect".to_string(),
                kind: Some("call".to_string()),
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: Some(2),
                limit: None,
                max_per_file: None,
                compact: None,
                mode: None,
                direction: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("src/service.rs"),
            "expected dependent hit: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "unrelated same-name file should be excluded: {result}"
        );
        assert!(result.contains("Match type: exact"), "got: {result}");
        assert!(
            result.contains("Scope: path `src/db.rs`; exact selector line 2; symbol kind `fn`; reference kind `call`"),
            "exact selector scope should be explicit: {result}"
        );
        assert!(
            result.contains("Evidence: reference anchors `src/service.rs:2`"),
            "reference output should include evidence anchors: {result}"
        );
    }

    #[tokio::test]
    async fn test_find_references_exact_selector_requires_line_for_ambiguous_symbol() {
        let target = make_file(
            "src/db.rs",
            b"fn connect() {}\nfn connect() {}\n",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 10, 10),
            ],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "connect".to_string(),
                kind: Some("call".to_string()),
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: None,
                limit: None,
                max_per_file: None,
                compact: None,
                mode: None,
                direction: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("10"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_dependents_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/nonexistent.rs".to_string(),
                limit: None,
                max_per_file: None,
                format: None,
                compact: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("No file-level dependents found"),
            "got: {result}"
        );
        assert!(result.contains("find_references"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_dependents_mermaid_includes_symbol_names_in_edges() {
        // dep_file imports from src/target.rs (stem "target") and uses TargetType.
        // find_dependents_for_file should prefer the symbol-level TypeUsage ref,
        // so the mermaid edge label should include the symbol name "TargetType".
        let target_sym = make_symbol("TargetType", SymbolKind::Struct, 0, 2);
        let import_ref = make_ref("target", None, ReferenceKind::Import, 0, None);
        let usage_ref = make_ref("TargetType", None, ReferenceKind::TypeUsage, 1, Some(0));
        let dep_sym = make_symbol("consumer", SymbolKind::Function, 0, 1);
        let target_file = make_file(
            "src/target.rs",
            b"pub struct TargetType {}\n",
            vec![target_sym],
        );
        let dep_file = make_file_with_refs(
            "src/dep.rs",
            b"use target::TargetType;\nfn consumer() { TargetType }\n",
            vec![dep_sym],
            vec![import_ref, usage_ref],
        );
        let server = make_server(make_live_index_ready(vec![target_file, dep_file]));

        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/target.rs".to_string(),
                compact: None,
                format: Some("mermaid".to_string()),
                limit: None,
                max_per_file: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("TargetType"),
            "mermaid edge should include symbol name: {result}"
        );
    }

    #[tokio::test]
    async fn test_find_dependents_excludes_non_pub_name_collision() {
        // target.rs defines a non-pub `run` function.
        // other.rs imports from target (triggering the symbol_refs path) AND calls
        // its own local `run` — same name, different function.
        // Without the pub filter, the "run" Call ref would be falsely attributed to
        // target.rs's symbol. With the pub filter, symbol_refs is empty and the
        // result falls back to the import ref — "run" must not appear as attributed.
        let target_sym = make_symbol("run", SymbolKind::Function, 0, 1);
        let target_file = make_file(
            "src/target.rs",
            b"fn run() { internal }\n",
            vec![target_sym],
        );

        let other_sym = make_symbol("main", SymbolKind::Function, 2, 4);
        let other_import = make_ref(
            "target",
            Some("crate::target"),
            ReferenceKind::Import,
            0,
            None,
        );
        let other_call = make_ref("run", None, ReferenceKind::Call, 3, Some(0));
        let other_file = make_file_with_refs(
            "src/other.rs",
            b"use crate::target;\nfn run() {}\nfn main() {\n    run();\n}\n",
            vec![other_sym],
            vec![other_import, other_call],
        );

        let server = make_server(make_live_index_ready(vec![target_file, other_file]));

        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/target.rs".to_string(),
                compact: None,
                format: None,
                limit: None,
                max_per_file: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        // other.rs still appears as a dependent (via the import), but the
        // non-pub "run" symbol must NOT be surfaced as an attributed reference.
        assert!(
            result.contains("src/other.rs"),
            "other.rs should appear via the import ref: {result}"
        );
        assert!(
            !result.contains("run"),
            "non-pub 'run' collision must not be attributed as a symbol reference: {result}"
        );
    }

    #[tokio::test]
    async fn test_find_dependents_includes_pub_symbol_references() {
        let target_sym = make_symbol("PublicApi", SymbolKind::Struct, 0, 1);
        let target_file = make_file(
            "src/target.rs",
            b"pub struct PublicApi {}\n",
            vec![target_sym],
        );

        let consumer_sym = make_symbol("use_it", SymbolKind::Function, 1, 3);
        let consumer_import = ReferenceRecord {
            name: "target".to_string(),
            qualified_name: Some("crate::target".to_string()),
            kind: ReferenceKind::Import,
            byte_range: (0, 20),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        };
        let consumer_ref = make_ref("PublicApi", None, ReferenceKind::TypeUsage, 2, Some(0));
        let consumer_file = make_file_with_refs(
            "src/consumer.rs",
            b"use crate::target;\nfn use_it() {\n    PublicApi {}\n}\n",
            vec![consumer_sym],
            vec![consumer_import, consumer_ref],
        );

        let server = make_server(make_live_index_ready(vec![target_file, consumer_file]));

        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/target.rs".to_string(),
                compact: None,
                format: None,
                limit: None,
                max_per_file: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("src/consumer.rs"),
            "pub symbol with import should be a real dependent: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_rejects_empty_query() {
        let sym = make_symbol("Foo", SymbolKind::Class, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"struct Foo {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        // Empty/whitespace query with no kind or path_prefix is fully unbounded — must be rejected
        for query in ["", "   ", "\t"] {
            let result = server
                .search_symbols(Parameters(super::SearchSymbolsInput {
                    query: Some(query.to_string()),
                    kind: None,
                    path_prefix: None,
                    language: None,
                    limit: None,
                    include_generated: None,
                    include_tests: None,
                    estimate: None,
                    max_tokens: None,
                }))
                .await;
            assert!(
                result.contains("requires at least one of"),
                "empty query '{query}' with no kind/path_prefix should be rejected, got: {result}"
            );
        }
    }

    #[tokio::test]
    async fn test_inspect_match_out_of_bounds_line() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 0);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}\n", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 999999,
                context: None,
                sibling_limit: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("out of bounds"),
            "should report out of bounds, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_shows_enclosing_symbol() {
        let sym = make_symbol("handle_request", SymbolKind::Function, 0, 2);
        let content = b"fn handle_request() {\n    let db = connect();\n}\n";
        let (key, file) = make_file("src/handler.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("connect".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;
        assert!(
            result.contains("handle_request"),
            "should show enclosing symbol name, got: {result}"
        );
        assert!(
            result.contains("in fn handle_request"),
            "should show kind and name, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_group_by_symbol_deduplicates() {
        let sym = make_symbol("connect", SymbolKind::Function, 0, 4);
        let content = b"fn connect() {\n    let url = db_url();\n    let pool = Pool::new(url);\n    pool.connect()\n}\n";
        let (key, file) = make_file("src/db.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("pool".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: Some("symbol".to_string()),
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;
        // With group_by: "symbol", should show symbol name and match count
        assert!(
            result.contains("connect"),
            "should show symbol name: {result}"
        );
        assert!(
            result.contains("2 matches") || result.contains("match"),
            "should show match count: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_group_by_usage_filters_imports() {
        let content = b"use crate::db::connect;\nfn handler() { connect() }\n";
        let sym = make_symbol("handler", SymbolKind::Function, 1, 1);
        let (key, file) = make_file("src/api.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("connect".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: Some("usage".to_string()),
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;
        // Should exclude the "use" import line
        assert!(
            !result.contains("use crate"),
            "should filter out imports: {result}"
        );
        assert!(
            result.contains("handler"),
            "should keep usage matches: {result}"
        );
    }

    #[tokio::test]
    async fn test_inspect_match_line_zero_is_out_of_bounds() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 0);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}\n", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 0,
                context: None,
                sibling_limit: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;
        assert!(
            result.contains("out of bounds"),
            "line 0 should be out of bounds (1-based), got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_follow_refs_includes_callers() {
        // Build an index with cross-references
        let sym_a = make_symbol("connect", SymbolKind::Function, 0, 1);
        let file_a_content = b"fn connect() {\n    db_open()\n}\n";
        let (key_a, file_a) = make_file("src/db.rs", file_a_content, vec![sym_a]);

        let sym_b = make_symbol("handler", SymbolKind::Function, 0, 1);
        let file_b_content = b"fn handler() {\n    connect()\n}\n";
        let (key_b, file_b) = make_file_with_refs(
            "src/api.rs",
            file_b_content,
            vec![sym_b],
            vec![make_ref("connect", None, ReferenceKind::Call, 1, Some(0))],
        );

        let server = make_server(make_live_index_ready(vec![
            (key_a, file_a),
            (key_b, file_b),
        ]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("db_open".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: Some(true),
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;
        // Should show that connect() is called by handler() in src/api.rs
        assert!(
            result.contains("handler") || result.contains("api.rs"),
            "should show callers of enclosing symbol, got: {result}"
        );
        assert!(
            result.contains("Called by"),
            "should have Called by section, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_follow_refs_empty_callers_signal() {
        // A file with a function that has no callers anywhere
        let sym_a = make_symbol("orphan_fn", SymbolKind::Function, 0, 2);
        let file_a_content = b"fn orphan_fn() {\n    do_work()\n}\n";
        let (key_a, file_a) = make_file("src/orphan.rs", file_a_content, vec![sym_a]);

        let server = make_server(make_live_index_ready(vec![(key_a, file_a)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("do_work".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: Some(true),
                follow_refs_limit: None,
                ranked: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            }))
            .await;
        // follow_refs ran but found no cross-references — should signal this explicitly
        assert!(
            result.contains("no cross-references found"),
            "should show empty-callers signal when follow_refs found nothing, got: {result}"
        );
        assert!(
            !result.contains("Called by"),
            "should not show 'Called by' when callers list is empty, got: {result}"
        );
    }

    // ── Lenient deserialization tests ────────────────────────────────────

    #[test]
    fn test_lenient_u32_accepts_string() {
        let json = r#"{"query":"test","limit":"10"}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, Some(10));
    }

    #[test]
    fn test_lenient_u32_accepts_number() {
        let json = r#"{"query":"test","limit":10}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, Some(10));
    }

    #[test]
    fn test_lenient_u32_accepts_null() {
        let json = r#"{"query":"test","limit":null}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, None);
    }

    #[test]
    fn test_lenient_u32_accepts_absent() {
        let json = r#"{"query":"test"}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, None);
    }

    #[test]
    fn test_lenient_bool_accepts_string_true() {
        let json = r#"{"uncommitted":"true"}"#;
        let input: super::WhatChangedInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.uncommitted, Some(true));
    }

    #[test]
    fn test_lenient_bool_accepts_string_false() {
        let json = r#"{"uncommitted":"false"}"#;
        let input: super::WhatChangedInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.uncommitted, Some(false));
    }

    #[test]
    fn test_lenient_bool_accepts_native_bool() {
        let json = r#"{"uncommitted":true}"#;
        let input: super::WhatChangedInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.uncommitted, Some(true));
    }

    #[test]
    fn test_lenient_u32_required_accepts_string() {
        let json = r#"{"path":"src/lib.rs","line":"42"}"#;
        let input: super::InspectMatchInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.line, 42);
    }

    #[test]
    fn test_lenient_u32_required_accepts_number() {
        let json = r#"{"path":"src/lib.rs","line":42}"#;
        let input: super::InspectMatchInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.line, 42);
    }

    #[test]
    fn test_lenient_depth_accepts_string() {
        let json = r#"{"detail":"tree","depth":"1"}"#;
        let input: super::GetRepoMapInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.depth, Some(1));
    }

    #[tokio::test]
    async fn test_analyze_file_impact_co_changes_returns_loading_message_when_no_git_data() {
        // With an empty index (no git temporal data computed), the tool with include_co_changes
        // should return the "still loading" or "unavailable" message in the co-changes section.
        let server = make_server(make_live_index_empty());
        let result = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "src/lib.rs".to_string(),
                new_file: None,
                include_co_changes: Some(true),
                co_changes_limit: None,
                estimate: None,
            }))
            .await;
        // Git temporal starts as Pending in tests (no tokio runtime spawns it) — but the main
        // impact analysis uses the loading guard and returns early, so the co-changes append
        // won't be reached. The loading guard message is returned instead.
        assert!(
            result.contains("still loading")
                || result.contains("unavailable")
                || result == crate::protocol::format::empty_guard_message(),
            "expected loading/unavailable/guard message, got: {result}"
        );
    }

    // ─── Edit tool integration tests ─────────────────────────────────────────

    /// Helper: write a file to disk and build a server with it indexed.
    fn setup_edit_test(original: &[u8]) -> (TempDir, SymForgeServer, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("lib.rs");
        std::fs::write(&file_path, original).unwrap();

        let result = crate::parsing::process_file("src/lib.rs", original, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, original.to_vec());
        let index = make_live_index_ready(vec![("src/lib.rs".to_string(), indexed)]);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));
        (dir, server, file_path)
    }

    #[tokio::test]
    async fn test_replace_symbol_body_replaces_and_reindexes() {
        let original = b"fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/lib.rs".to_string(),
            name: "hello".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "fn hello() {\n    println!(\"HELLO\");\n}".to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("replaced"), "result was: {result}");
        assert!(result.contains("hello"), "result was: {result}");
        assert!(result.contains("Edit safety: structural-edit-safe"));
        assert!(result.contains("Path authority: repository-bound"));
        assert!(result.contains("Source authority: disk-refreshed"));
        assert!(result.contains("Write semantics: atomic write + reindex"));
        assert!(result.contains("Evidence: symbol anchor `src/lib.rs:1`"));

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(on_disk.contains("HELLO"), "disk: {on_disk}");
        assert!(on_disk.contains("world"), "other symbol intact: {on_disk}");

        let guard = server.index.read();
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(file.symbols.iter().any(|s| s.name == "hello"));
        assert!(file.symbols.iter().any(|s| s.name == "world"));
    }

    /// When `new_body` does NOT supply its own doc comment, the existing
    /// attached `/// ...` doc must be preserved. Previously the splice
    /// range extended past the doc unconditionally, silently deleting it.
    #[tokio::test]
    async fn test_replace_symbol_body_preserves_attached_doc_without_new_doc() {
        let original = b"/// Adds two numbers.\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/lib.rs".to_string(),
            name: "add".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "pub fn add(a: i32, b: i32) -> i32 {\n    a.saturating_add(b)\n}".to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("replaced"), "result was: {result}");

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("/// Adds two numbers."),
            "existing doc must be preserved; disk was:\n{on_disk}"
        );
        assert!(
            on_disk.contains("saturating_add"),
            "new body must be written; disk was:\n{on_disk}"
        );
        // No duplicated doc line.
        assert_eq!(
            on_disk.matches("/// Adds two numbers.").count(),
            1,
            "doc must appear exactly once; disk was:\n{on_disk}"
        );
    }

    /// When `new_body` DOES supply its own doc comment, the existing doc
    /// must be replaced — not kept alongside, which would duplicate it.
    #[tokio::test]
    async fn test_replace_symbol_body_replaces_attached_doc_when_new_body_supplies_one() {
        let original = b"/// Old doc.\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/lib.rs".to_string(),
            name: "add".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "/// New doc.\npub fn add(a: i32, b: i32) -> i32 {\n    a.saturating_add(b)\n}"
                .to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("replaced"), "result was: {result}");

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("/// New doc."),
            "new doc must be written; disk was:\n{on_disk}"
        );
        assert!(
            !on_disk.contains("/// Old doc."),
            "old doc must be replaced when new one is supplied; disk was:\n{on_disk}"
        );
        // Exactly one doc line (no duplication).
        assert_eq!(
            on_disk.matches("/// ").count(),
            1,
            "exactly one doc line expected; disk was:\n{on_disk}"
        );
    }

    /// Attributes like `#[inline]` attached to the symbol must be preserved
    /// when `new_body` has neither a doc nor the attribute. This is the
    /// `#[test]`-duplication bug we hit while building Unit 5's tests.
    #[tokio::test]
    async fn test_replace_symbol_body_preserves_attribute_without_duplicating_it() {
        let original = b"#[inline]\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/lib.rs".to_string(),
            name: "add".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "pub fn add(a: i32, b: i32) -> i32 {\n    a.saturating_add(b)\n}".to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("replaced"), "result was: {result}");

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            on_disk.matches("#[inline]").count(),
            1,
            "attribute must appear exactly once; disk was:\n{on_disk}"
        );
        assert!(
            on_disk.contains("saturating_add"),
            "new body must be written; disk was:\n{on_disk}"
        );
    }

    /// Orphaned doc comments (separated from the symbol by a blank line)
    /// must NOT be swallowed when new_body has no doc of its own. This is
    /// the narrowest behavioral delta between the old and new splice math,
    /// and the case the plan's Risks table explicitly flagged for fixture
    /// coverage.
    #[tokio::test]
    async fn test_replace_symbol_body_preserves_orphan_doc_without_new_doc() {
        let original =
            b"/// Intro remark about the next item.\n\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/lib.rs".to_string(),
            name: "add".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "pub fn add(a: i32, b: i32) -> i32 {\n    a.saturating_add(b)\n}".to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("replaced"), "result was: {result}");

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("/// Intro remark about the next item."),
            "orphaned doc must survive a body-only replace; disk was:\n{on_disk}"
        );
        assert!(
            on_disk.contains("saturating_add"),
            "new body must be written; disk was:\n{on_disk}"
        );
        assert_eq!(
            on_disk.matches("/// Intro remark about the next item.").count(),
            1,
            "orphan doc must not duplicate; disk was:\n{on_disk}"
        );
    }

    /// TypeScript/JSDoc variant: `/** ... */` doc block on a separate line
    /// must be preserved when new_body has no doc. Plan Risks table named
    /// this specifically — multi-language coverage proves the helper works
    /// off of the language-agnostic doc-marker list in edit.rs.
    #[tokio::test]
    async fn test_replace_symbol_body_preserves_jsdoc_without_new_doc() {
        let original = b"/** Adds two numbers. */\nexport function add(a: number, b: number): number {\n    return a + b;\n}\n";
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("math.ts");
        std::fs::write(&file_path, original).unwrap();
        let parse_result = crate::parsing::process_file("src/math.ts", original, LanguageId::TypeScript);
        let indexed = IndexedFile::from_parse_result(parse_result, original.to_vec());
        let index = make_live_index_ready(vec![("src/math.ts".to_string(), indexed)]);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/math.ts".to_string(),
            name: "add".to_string(),
            kind: None,
            symbol_line: None,
            new_body:
                "export function add(a: number, b: number): number {\n    return a + b;\n}"
                    .to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("replaced"), "result was: {result}");

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("/** Adds two numbers. */"),
            "JSDoc block must survive a body-only replace; disk was:\n{on_disk}"
        );
        assert_eq!(
            on_disk.matches("/** Adds two numbers. */").count(),
            1,
            "JSDoc must not duplicate; disk was:\n{on_disk}"
        );
    }

    #[tokio::test]
    async fn test_replace_symbol_body_preserves_indentation() {
        // Simulates a method inside a class — symbol is indented 4 spaces.
        let original = b"mod outer {\n    fn inner() {\n        old_body();\n    }\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        // Provide unindented replacement — tool should auto-indent to match.
        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/lib.rs".to_string(),
            name: "inner".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "fn inner() {\n    new_body();\n}".to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("replaced"), "result: {result}");

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        // Every line of the replacement should be indented 4 spaces.
        assert!(
            on_disk.contains("    fn inner() {\n        new_body();\n    }"),
            "indentation preserved: {on_disk}"
        );
    }

    #[tokio::test]
    async fn test_replace_symbol_body_not_found() {
        let original = b"fn hello() {}\n";
        let (_dir, server, _) = setup_edit_test(original);

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "nonexistent.rs".to_string(),
            name: "foo".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "fn foo() {}".to_string(),
            dry_run: None,
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(
            result.contains("not found") || result.contains("Not found"),
            "result: {result}"
        );
    }

    #[tokio::test]
    async fn test_insert_symbol_after_works() {
        let original = b"fn hello() {\n    println!(\"hello\");\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::InsertSymbolInput {
            path: "src/lib.rs".to_string(),
            name: "hello".to_string(),
            kind: None,
            symbol_line: None,
            content: "fn world() {\n    println!(\"world\");\n}".to_string(),
            position: None, // defaults to "after"
            dry_run: None,
            working_directory: None,
        };
        let result = server.insert_symbol(Parameters(input)).await;
        assert!(result.contains("inserted"), "result: {result}");
        assert!(result.contains("after"), "result: {result}");
        assert!(result.contains("Edit safety: structural-edit-safe"));
        assert!(result.contains("Write semantics: atomic write + reindex"));
        assert!(result.contains("Evidence: symbol anchor `src/lib.rs:1`"));

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(on_disk.contains("hello"), "original intact: {on_disk}");
        assert!(on_disk.contains("world"), "new symbol: {on_disk}");

        let guard = server.index.read();
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(file.symbols.iter().any(|s| s.name == "world"));
    }

    #[tokio::test]
    async fn test_insert_symbol_before_works() {
        let original = b"fn world() {\n    println!(\"world\");\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::InsertSymbolInput {
            path: "src/lib.rs".to_string(),
            name: "world".to_string(),
            kind: None,
            symbol_line: None,
            content: "fn hello() {\n    println!(\"hello\");\n}".to_string(),
            position: Some("before".to_string()),
            dry_run: None,
            working_directory: None,
        };
        let result = server.insert_symbol(Parameters(input)).await;
        assert!(result.contains("inserted"), "result: {result}");
        assert!(result.contains("before"), "result: {result}");

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        let hello_pos = on_disk.find("hello").unwrap();
        let world_pos = on_disk.find("world").unwrap();
        assert!(hello_pos < world_pos, "hello before world: {on_disk}");
    }

    #[tokio::test]
    async fn test_delete_symbol_removes_and_reindexes() {
        let original = b"fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::DeleteSymbolInput {
            path: "src/lib.rs".to_string(),
            name: "hello".to_string(),
            kind: None,
            symbol_line: None,
            dry_run: None,
            working_directory: None,
        };
        let result = server.delete_symbol(Parameters(input)).await;
        assert!(result.contains("deleted"), "result: {result}");
        assert!(result.contains("Edit safety: structural-edit-safe"));
        assert!(result.contains("Write semantics: atomic write + reindex"));
        assert!(result.contains("Evidence: symbol anchor `src/lib.rs:1`"));

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(!on_disk.contains("hello"), "hello removed: {on_disk}");
        assert!(on_disk.contains("world"), "world intact: {on_disk}");

        let guard = server.index.read();
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(!file.symbols.iter().any(|s| s.name == "hello"));
        assert!(file.symbols.iter().any(|s| s.name == "world"));
    }

    #[test]
    fn test_check_edit_capability_blocks_structural_for_frontend() {
        // replace_symbol_body requires StructuralEditSafe; Html is TextEditSafe → blocked
        let warning = SymForgeServer::check_edit_capability(
            &crate::domain::LanguageId::Html,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "replace_symbol_body",
        );
        assert!(
            warning.is_some(),
            "replace_symbol_body should be blocked for HTML"
        );
        assert!(warning.as_ref().unwrap().contains("edit safety blocked"));
        assert!(
            warning
                .as_ref()
                .unwrap()
                .contains("Required safety: structural-edit-safe")
        );
        assert!(
            warning
                .as_ref()
                .unwrap()
                .contains("Available safety: text-edit-safe")
        );
    }

    #[test]
    fn test_check_edit_capability_allows_text_edit_for_frontend() {
        // edit_within_symbol requires TextEditSafe; Css is TextEditSafe → allowed
        let warning = SymForgeServer::check_edit_capability(
            &crate::domain::LanguageId::Css,
            crate::parsing::config_extractors::EditCapability::TextEditSafe,
            "edit_within_symbol",
        );
        assert!(
            warning.is_none(),
            "edit_within_symbol should be allowed for CSS"
        );
    }

    #[tokio::test]
    async fn test_edit_within_symbol_replaces_text() {
        let original = b"fn hello() {\n    println!(\"hello\");\n}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let input = crate::protocol::edit::EditWithinSymbolInput {
            path: "src/lib.rs".to_string(),
            name: "hello".to_string(),
            kind: None,
            symbol_line: None,
            old_text: "\"hello\"".to_string(),
            new_text: "\"HELLO\"".to_string(),
            replace_all: false,
            dry_run: None,
            working_directory: None,
        };
        let result = server.edit_within_symbol(Parameters(input)).await;
        assert!(result.contains("edited within"), "result: {result}");
        assert!(result.contains("1 replacement"), "result: {result}");
        assert!(result.contains("Edit safety: text-edit-safe"));
        assert!(result.contains("Write semantics: atomic write + reindex"));
        assert!(result.contains("Evidence: symbol anchor `src/lib.rs:1`"));

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(on_disk.contains("HELLO"), "edited: {on_disk}");
        assert!(!on_disk.contains("\"hello\""), "old text gone: {on_disk}");
    }

    #[tokio::test]
    async fn test_replace_symbol_body_reports_disk_refreshed_authority_for_stale_file() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let stale_indexed = b"fn hello() {\n    println!(\"stale\");\n}\n";
        let fresh_on_disk = b"fn hello() {\n    println!(\"fresh\");\n}\n";
        let file_path = src_dir.join("lib.rs");
        std::fs::write(&file_path, fresh_on_disk).unwrap();

        let result = crate::parsing::process_file("src/lib.rs", stale_indexed, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, stale_indexed.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/lib.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = crate::protocol::edit::ReplaceSymbolBodyInput {
            path: "src/lib.rs".to_string(),
            name: "hello".to_string(),
            kind: None,
            symbol_line: None,
            new_body: "fn hello() {\n    println!(\"new body\");\n}".to_string(),
            dry_run: Some(true),
            working_directory: None,
        };
        let result = server.replace_symbol_body(Parameters(input)).await;
        assert!(result.contains("Source authority: disk-refreshed"));
        assert!(result.contains("Write semantics: dry run (no writes)"));

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("fresh"),
            "dry run should leave disk untouched: {on_disk}"
        );
    }

    #[tokio::test]
    async fn test_edit_within_symbol_not_found_text() {
        let original = b"fn hello() {\n    println!(\"hello\");\n}\n";
        let (_dir, server, _) = setup_edit_test(original);

        let input = crate::protocol::edit::EditWithinSymbolInput {
            path: "src/lib.rs".to_string(),
            name: "hello".to_string(),
            kind: None,
            symbol_line: None,
            old_text: "nonexistent".to_string(),
            new_text: "replacement".to_string(),
            replace_all: false,
            dry_run: None,
            working_directory: None,
        };
        let result = server.edit_within_symbol(Parameters(input)).await;
        assert!(result.contains("not found within"), "result: {result}");
    }

    #[tokio::test]
    async fn test_replace_symbol_body_dry_run_skips_write() {
        let original = b"fn foo() { old }\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let result = server
            .replace_symbol_body(Parameters(crate::protocol::edit::ReplaceSymbolBodyInput {
                path: "src/lib.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                new_body: "fn foo() { new }".to_string(),
                dry_run: Some(true),
                working_directory: None,
            }))
            .await;

        assert!(
            result.contains("[DRY RUN]"),
            "should show dry run: {result}"
        );
        assert!(result.contains("Path authority: repository-bound"));
        assert!(result.contains("Write semantics: dry run (no writes)"));
        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("old"),
            "file should be unchanged: {on_disk}"
        );
    }

    #[tokio::test]
    async fn test_insert_symbol_dry_run_skips_write() {
        let original = b"fn anchor() {}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let result = server
            .insert_symbol(Parameters(crate::protocol::edit::InsertSymbolInput {
                path: "src/lib.rs".to_string(),
                name: "anchor".to_string(),
                kind: None,
                symbol_line: None,
                content: "fn new_fn() {}".to_string(),
                position: Some("after".to_string()),
                dry_run: Some(true),
                working_directory: None,
            }))
            .await;

        assert!(
            result.contains("[DRY RUN]"),
            "should show dry run: {result}"
        );
        assert!(result.contains("Path authority: repository-bound"));
        assert!(result.contains("Write semantics: dry run (no writes)"));
        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            !on_disk.contains("new_fn"),
            "file should be unchanged: {on_disk}"
        );
    }

    #[tokio::test]
    async fn test_delete_symbol_dry_run_skips_write() {
        let original = b"fn target() {}\nfn keep() {}\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let result = server
            .delete_symbol(Parameters(crate::protocol::edit::DeleteSymbolInput {
                path: "src/lib.rs".to_string(),
                name: "target".to_string(),
                kind: None,
                symbol_line: None,
                dry_run: Some(true),
                working_directory: None,
            }))
            .await;

        assert!(
            result.contains("[DRY RUN]"),
            "should show dry run: {result}"
        );
        assert!(result.contains("Path authority: repository-bound"));
        assert!(result.contains("Write semantics: dry run (no writes)"));
        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("target"),
            "file should be unchanged: {on_disk}"
        );
    }

    #[tokio::test]
    async fn test_edit_within_symbol_dry_run_skips_write() {
        let original = b"fn foo() { old_text }\n";
        let (_dir, server, file_path) = setup_edit_test(original);

        let result = server
            .edit_within_symbol(Parameters(crate::protocol::edit::EditWithinSymbolInput {
                path: "src/lib.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                old_text: "old_text".to_string(),
                new_text: "new_text".to_string(),
                replace_all: false,
                dry_run: Some(true),
                working_directory: None,
            }))
            .await;

        assert!(
            result.contains("[DRY RUN]"),
            "should show dry run: {result}"
        );
        assert!(result.contains("Path authority: repository-bound"));
        assert!(result.contains("Write semantics: dry run (no writes)"));
        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("old_text"),
            "file should be unchanged: {on_disk}"
        );
    }

    // ── Tier 2 batch tool integration tests ──

    #[tokio::test]
    async fn test_batch_edit_applies_across_files() {
        use crate::protocol::edit::{BatchEditInput, EditOperation, SingleEdit};

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let a_content = b"fn alpha() { old }\n";
        let b_content = b"fn beta() { keep }\n";
        std::fs::write(src_dir.join("a.rs"), a_content).unwrap();
        std::fs::write(src_dir.join("b.rs"), b_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/a.rs", a_content as &[u8]),
            ("src/b.rs", b_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchEditInput {
            edits: vec![
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
            ],
            dry_run: Some(false),
            working_directory: None,
        };
        let result = server.batch_edit(Parameters(input)).await;
        assert!(result.contains("2 edit(s)"), "result: {result}");
        assert!(
            result.contains("Edit safety: structural-edit-safe"),
            "result: {result}"
        );
        assert!(result.contains("Match type: exact"), "result: {result}");
        assert!(
            result.contains("Write semantics: transactional write + rollback + reindex"),
            "result: {result}"
        );

        let a = std::fs::read_to_string(src_dir.join("a.rs")).unwrap();
        assert!(a.contains("new"), "a.rs: {a}");
        let b = std::fs::read_to_string(src_dir.join("b.rs")).unwrap();
        assert!(!b.contains("beta"), "b.rs: {b}");
    }

    #[tokio::test]
    async fn test_batch_edit_reports_disk_refreshed_authority_for_stale_file() {
        use crate::protocol::edit::{BatchEditInput, EditOperation, SingleEdit};

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let stale_indexed = b"fn alpha() { stale }\n";
        let fresh_on_disk = b"fn alpha() { fresh }\n";
        let file_path = src_dir.join("a.rs");
        std::fs::write(&file_path, fresh_on_disk).unwrap();

        let result = crate::parsing::process_file("src/a.rs", stale_indexed, LanguageId::Rust);
        let indexed = crate::live_index::store::IndexedFile::from_parse_result(
            result,
            stale_indexed.to_vec(),
        );
        let index = make_live_index_ready(vec![("src/a.rs".to_string(), indexed)]);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchEditInput {
            edits: vec![SingleEdit {
                path: "src/a.rs".to_string(),
                name: "alpha".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn alpha() { next }".to_string(),
                },
                working_directory: None,
            }],
            dry_run: Some(true),
            working_directory: None,
        };
        let result = server.batch_edit(Parameters(input)).await;
        assert!(
            result.contains("Source authority: disk-refreshed"),
            "result: {result}"
        );
        assert!(
            result.contains("Write semantics: dry run (no writes)"),
            "result: {result}"
        );

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("fresh"),
            "dry run should leave disk untouched: {on_disk}"
        );
    }

    #[tokio::test]
    async fn test_batch_rename_renames_def_and_refs() {
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let lib_content = b"fn old_name() {}\n";
        let main_content = b"fn caller() { old_name(); }\n";
        std::fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        std::fs::write(src_dir.join("main.rs"), main_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", lib_content as &[u8]),
            ("src/main.rs", main_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "old_name".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "new_name".to_string(),
            dry_run: None,
            code_only: None,
            working_directory: None,
        };
        let result = server.batch_rename(Parameters(input)).await;
        assert!(result.contains("Renamed"), "result: {result}");
        assert!(result.contains("new_name"), "result: {result}");
        assert!(
            result.contains("Match type: constrained"),
            "result: {result}"
        );
        assert!(
            result.contains("Write semantics: transactional write + rollback + reindex"),
            "result: {result}"
        );

        let lib = std::fs::read_to_string(src_dir.join("lib.rs")).unwrap();
        assert!(lib.contains("new_name"), "lib.rs: {lib}");
        assert!(!lib.contains("old_name"), "lib.rs: {lib}");
    }

    #[tokio::test]
    async fn test_batch_rename_reports_disk_refreshed_authority_after_reconcile() {
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let stale_lib = b"fn old_name() { stale(); }\n";
        let stale_main = b"fn caller() { old_name(); stale(); }\n";
        let fresh_lib = b"fn old_name() { fresh(); }\n";
        let fresh_main = b"fn caller() { old_name(); fresh(); }\n";
        std::fs::write(src_dir.join("lib.rs"), fresh_lib).unwrap();
        std::fs::write(src_dir.join("main.rs"), fresh_main).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", stale_lib as &[u8]),
            ("src/main.rs", stale_main as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "old_name".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "new_name".to_string(),
            dry_run: Some(true),
            code_only: None,
            working_directory: None,
        };
        let result = server.batch_rename(Parameters(input)).await;
        assert!(
            result.contains("Source authority: disk-refreshed"),
            "result: {result}"
        );
        assert!(
            result.contains("Write semantics: dry run (no writes)"),
            "result: {result}"
        );

        let lib = std::fs::read_to_string(src_dir.join("lib.rs")).unwrap();
        assert!(
            lib.contains("fresh"),
            "dry run should leave disk untouched: {lib}"
        );
    }

    #[tokio::test]
    async fn test_batch_rename_catches_path_qualified_calls() {
        // Regression: batch_rename must catch Type::new() path-qualified calls,
        // not just simple name references. The index tracks "new" as the call
        // target, but "Widget" as a path prefix must also be renamed.
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let lib_content = b"pub struct Widget { pub name: String }\nimpl Widget {\n    pub fn new(name: &str) -> Self { Widget { name: name.to_string() } }\n}\n";
        let main_content =
            b"use crate::Widget;\nfn make() -> Widget {\n    Widget::new(\"default\")\n}\n";
        std::fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        std::fs::write(src_dir.join("main.rs"), main_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", lib_content as &[u8]),
            ("src/main.rs", main_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "Widget".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "Gadget".to_string(),
            dry_run: None,
            code_only: None,
            working_directory: None,
        };
        let result = server.batch_rename(Parameters(input)).await;
        assert!(result.contains("Renamed"), "result: {result}");

        // Verify disk: Widget::new() in main.rs must become Gadget::new()
        let main = std::fs::read_to_string(src_dir.join("main.rs")).unwrap();
        assert!(
            !main.contains("Widget"),
            "main.rs should have no 'Widget' left after rename, got: {main}"
        );
        assert!(
            main.contains("Gadget::new"),
            "main.rs should contain Gadget::new, got: {main}"
        );
    }

    #[tokio::test]
    async fn test_search_text_agrees_with_disk_after_rename() {
        // Regression: after batch_rename, search_text must agree with on-disk
        // content. If rename misses some sites, search_text must still find
        // the old name (because it's still on disk), not report zero matches.
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let lib_content = b"pub struct Widget { pub name: String }\nimpl Widget {\n    pub fn new(name: &str) -> Self { Widget { name: name.to_string() } }\n}\n";
        let main_content =
            b"use crate::Widget;\nfn make() -> Widget {\n    Widget::new(\"default\")\n}\n";
        std::fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        std::fs::write(src_dir.join("main.rs"), main_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", lib_content as &[u8]),
            ("src/main.rs", main_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        // Do the rename
        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "Widget".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "Gadget".to_string(),
            dry_run: None,
            code_only: None,
            working_directory: None,
        };
        let _result = server.batch_rename(Parameters(input)).await;

        // Now check: if "Widget" is still on disk anywhere, search_text must find it.
        let disk_has_widget = std::fs::read_to_string(src_dir.join("main.rs"))
            .unwrap()
            .contains("Widget")
            || std::fs::read_to_string(src_dir.join("lib.rs"))
                .unwrap()
                .contains("Widget");

        if disk_has_widget {
            // search_text should find it too — index must not lie
            let search_input = crate::protocol::tools::SearchTextInput {
                query: Some("Widget".to_string()),
                terms: None,
                path_prefix: None,
                glob: None,
                exclude_glob: None,
                language: None,
                regex: None,
                case_sensitive: None,
                whole_word: None,
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
                ranked: None,
                context: None,
                limit: None,
                max_per_file: None,
                include_tests: None,
                include_generated: None,
                estimate: None,
                max_tokens: None,
                structural: None,
            };
            let search_result = server.search_text(Parameters(search_input)).await;
            assert!(
                !search_result.contains("0 matches"),
                "search_text says 0 matches but Widget is still on disk! Index/disk desync. search_result: {search_result}"
            );
        }
    }

    #[tokio::test]
    async fn test_batch_rename_catches_qualified_call() {
        // Supplemental qualified scan must catch OldType::new() and rename it.
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let lib_content =
            b"pub struct OldType;\nimpl OldType {\n    pub fn new() -> Self { OldType }\n}\n";
        let main_content = b"fn make() { let _ = OldType::new(); }\n";
        std::fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        std::fs::write(src_dir.join("main.rs"), main_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", lib_content as &[u8]),
            ("src/main.rs", main_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "OldType".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "NewType".to_string(),
            dry_run: None,
            code_only: None,
            working_directory: None,
        };
        let result = server.batch_rename(Parameters(input)).await;
        assert!(result.contains("Renamed"), "result: {result}");

        let main = std::fs::read_to_string(src_dir.join("main.rs")).unwrap();
        assert!(
            main.contains("NewType::new()"),
            "main.rs should contain NewType::new(), got: {main}"
        );
        assert!(
            !main.contains("OldType"),
            "main.rs should have no OldType left, got: {main}"
        );
    }

    #[tokio::test]
    async fn test_batch_rename_dry_run_separates_confident_uncertain() {
        // Dry run must show separate sections for confident and uncertain matches.
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // lib.rs: definition only (confident)
        // main.rs: code usage (confident) + comment usage (uncertain)
        let lib_content = b"pub struct OldType;\n";
        let main_content =
            b"fn use_it() { let _ = OldType::new(); }\n// OldType::new() creates an instance\n";
        std::fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        std::fs::write(src_dir.join("main.rs"), main_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", lib_content as &[u8]),
            ("src/main.rs", main_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "OldType".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "NewType".to_string(),
            dry_run: Some(true),
            code_only: None,
            working_directory: None,
        };
        let result = server.batch_rename(Parameters(input)).await;

        assert!(
            result.contains("Confident matches"),
            "dry_run must have confident section, got: {result}"
        );
        assert!(
            result.contains("Uncertain matches"),
            "dry_run must have uncertain section, got: {result}"
        );
        assert!(
            result.contains("NOT applied"),
            "uncertain section must say NOT applied, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_batch_rename_uncertain_not_applied_by_default() {
        // Uncertain matches (comments) must NOT be modified during a live rename.
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // File with OldType only in a comment — no code usage.
        let lib_content = b"pub struct OldType;\n";
        let main_content = b"fn dummy() {}\n// OldType::new() creates an instance\n";
        std::fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        std::fs::write(src_dir.join("main.rs"), main_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", lib_content as &[u8]),
            ("src/main.rs", main_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "OldType".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "NewType".to_string(),
            dry_run: None,
            code_only: None,
            working_directory: None,
        };
        let result = server.batch_rename(Parameters(input)).await;
        assert!(result.contains("Renamed"), "result: {result}");

        // Comment in main.rs must remain unchanged.
        let main = std::fs::read_to_string(src_dir.join("main.rs")).unwrap();
        assert!(
            main.contains("OldType"),
            "comment in main.rs must not be modified, got: {main}"
        );
        // But the result should surface the uncertain match as a warning.
        assert!(
            result.contains("Uncertain matches") || result.contains("NOT applied"),
            "result should warn about uncertain match, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_batch_rename_scopes_common_name_to_target() {
        // Renaming "new" scoped to Target should not touch SomeOther::new().
        use crate::protocol::edit::BatchRenameInput;

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let lib_content =
            b"pub struct Target;\nimpl Target {\n    pub fn new() -> Self { Target }\n}\npub struct SomeOther;\nimpl SomeOther {\n    pub fn new() -> Self { SomeOther }\n}\n";
        let main_content = b"fn make() { let _a = Target::new(); let _b = SomeOther::new(); }\n";
        std::fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        std::fs::write(src_dir.join("main.rs"), main_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/lib.rs", lib_content as &[u8]),
            ("src/main.rs", main_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        // Rename the Target struct itself, not its "new" method.
        let input = BatchRenameInput {
            path: "src/lib.rs".to_string(),
            name: "Target".to_string(),
            kind: None,
            symbol_line: None,
            new_name: "Renamed".to_string(),
            dry_run: None,
            code_only: None,
            working_directory: None,
        };
        let result = server.batch_rename(Parameters(input)).await;
        assert!(result.contains("Renamed"), "result: {result}");

        let main = std::fs::read_to_string(src_dir.join("main.rs")).unwrap();
        assert!(
            main.contains("Renamed::new()"),
            "Target::new() must become Renamed::new(), got: {main}"
        );
        assert!(
            main.contains("SomeOther::new()"),
            "SomeOther::new() must be untouched, got: {main}"
        );
        assert!(
            !main.contains("Target::"),
            "Target:: reference must be gone, got: {main}"
        );
    }

    #[tokio::test]
    async fn test_batch_insert_adds_to_multiple_files() {
        use crate::protocol::edit::{BatchInsertInput, InsertPosition, InsertTarget};

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let a_content = b"fn handler_a() {}\n";
        let b_content = b"fn handler_b() {}\n";
        std::fs::write(src_dir.join("a.rs"), a_content).unwrap();
        std::fs::write(src_dir.join("b.rs"), b_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/a.rs", a_content as &[u8]),
            ("src/b.rs", b_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

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
            dry_run: Some(false),
            working_directory: None,
        };
        let result = server.batch_insert(Parameters(input)).await;
        assert!(result.contains("2 edit(s)"), "result: {result}");
        assert!(
            result.contains("Edit safety: structural-edit-safe"),
            "result: {result}"
        );
        assert!(result.contains("Match type: exact"), "result: {result}");
        assert!(
            result.contains("Write semantics: transactional write + rollback + reindex"),
            "result: {result}"
        );

        let a = std::fs::read_to_string(src_dir.join("a.rs")).unwrap();
        assert!(a.contains("logging"), "a.rs: {a}");
        let b = std::fs::read_to_string(src_dir.join("b.rs")).unwrap();
        assert!(b.contains("logging"), "b.rs: {b}");
    }

    #[tokio::test]
    async fn test_batch_insert_reports_disk_refreshed_authority_for_stale_file() {
        use crate::protocol::edit::{BatchInsertInput, InsertPosition, InsertTarget};

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let stale_indexed = b"fn handler_a() { stale(); }\n";
        let fresh_on_disk = b"fn handler_a() { fresh(); }\n";
        let file_path = src_dir.join("a.rs");
        std::fs::write(&file_path, fresh_on_disk).unwrap();

        let result = crate::parsing::process_file("src/a.rs", stale_indexed, LanguageId::Rust);
        let indexed = crate::live_index::store::IndexedFile::from_parse_result(
            result,
            stale_indexed.to_vec(),
        );
        let index = make_live_index_ready(vec![("src/a.rs".to_string(), indexed)]);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

        let input = BatchInsertInput {
            content: "fn logging() {}".to_string(),
            position: InsertPosition::After,
            targets: vec![InsertTarget {
                path: "src/a.rs".to_string(),
                name: "handler_a".to_string(),
                kind: None,
                symbol_line: None,
                working_directory: None,
            }],
            dry_run: Some(true),
            working_directory: None,
        };
        let result = server.batch_insert(Parameters(input)).await;
        assert!(
            result.contains("Source authority: disk-refreshed"),
            "result: {result}"
        );
        assert!(
            result.contains("Write semantics: dry run (no writes)"),
            "result: {result}"
        );

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.contains("fresh"),
            "dry run should leave disk untouched: {on_disk}"
        );
    }

    #[tokio::test]
    async fn test_batch_insert_dry_run_skips_write() {
        use crate::protocol::edit::{BatchInsertInput, InsertPosition, InsertTarget};

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let a_content = b"fn handler_a() {}\n";
        let b_content = b"fn handler_b() {}\n";
        std::fs::write(src_dir.join("a.rs"), a_content).unwrap();
        std::fs::write(src_dir.join("b.rs"), b_content).unwrap();

        let mut files = vec![];
        for (path, content) in [
            ("src/a.rs", a_content as &[u8]),
            ("src/b.rs", b_content as &[u8]),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed =
                crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
            files.push((path.to_string(), indexed));
        }
        let index = make_live_index_ready(files);
        let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

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
            working_directory: None,
        };
        let result = server.batch_insert(Parameters(input)).await;
        assert!(result.contains("[DRY RUN]"), "result: {result}");
        assert!(result.contains("Match type: exact"), "result: {result}");
        assert!(
            result.contains("Write semantics: dry run (no writes)"),
            "result: {result}"
        );

        // Files must be unchanged.
        let a = std::fs::read_to_string(src_dir.join("a.rs")).unwrap();
        assert!(!a.contains("logging"), "dry_run must not write: {a}");
        let b = std::fs::read_to_string(src_dir.join("b.rs")).unwrap();
        assert!(!b.contains("logging"), "dry_run must not write: {b}");
    }

    #[tokio::test]
    async fn test_ask_reports_exact_route_confidence() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"fn helper() {}\nfn caller() { helper(); }\n";
        std::fs::write(src_dir.join("lib.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/lib.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/lib.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "who calls helper".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;
        assert!(
            result.contains("Route confidence: exact"),
            "result: {result}"
        );
        assert!(
            result.contains("Chosen tool: find_references"),
            "result: {result}"
        );
        assert!(
            result.contains("Invocation: find_references"),
            "result: {result}"
        );
        assert!(
            result.contains("matched explicit caller/reference phrasing"),
            "result: {result}"
        );
    }

    #[tokio::test]
    async fn test_ask_reports_inferred_route_confidence() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"struct LiveIndex;\n";
        std::fs::write(src_dir.join("lib.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/lib.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/lib.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "LiveIndex".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;
        assert!(
            result.contains("Route confidence: inferred"),
            "result: {result}"
        );
        assert!(
            result.contains("Chosen tool: search_symbols"),
            "result: {result}"
        );
        assert!(result.contains("Suggested next step:"), "result: {result}");
    }

    #[tokio::test]
    async fn test_ask_reports_fallback_route_confidence() {
        let server = make_server(make_live_index_ready(vec![]));
        let input = super::SmartQueryInput {
            query: "error handling patterns".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;
        assert!(
            result.contains("Route confidence: fallback"),
            "result: {result}"
        );
        assert!(result.contains("Chosen tool: explore"), "result: {result}");
        assert!(result.contains("Suggested next step:"), "result: {result}");
    }

    #[tokio::test]
    async fn test_ask_upgrades_exact_symbol_understanding_query() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"struct BusActor;\nimpl BusActor { fn handle(&self) {} }\n";
        std::fs::write(src_dir.join("bus.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/bus.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/bus.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "how does BusActor work?".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;

        assert!(
            result.contains("Route confidence: inferred"),
            "result: {result}"
        );
        assert!(
            result.contains("Chosen tool: get_symbol_context"),
            "result: {result}"
        );
        assert!(
            result.contains("Invocation: get_symbol_context(name=\"BusActor\")"),
            "result: {result}"
        );
        assert!(
            result.contains("detected an exact indexed symbol inside a broad explanation query"),
            "result: {result}"
        );
    }

    #[tokio::test]
    async fn test_ask_upgrades_multi_definition_symbol_prefers_src() {
        // Symbol "MyHandler" appears in both src/ and tests/; the src/ definition
        // must win and the query must still upgrade to get_symbol_context.
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let tests_dir = dir.path().join("tests");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&tests_dir).unwrap();

        // src/handler.rs — large definition (lines 0-40)
        let src_content: &[u8] = b"struct MyHandler;\nimpl MyHandler {\n    fn run(&self) {}\n    fn stop(&self) {}\n    fn pause(&self) {}\n    fn resume(&self) {}\n    fn status(&self) {}\n    fn reset(&self) {}\n    fn init(&self) {}\n    fn close(&self) {}\n    fn open(&self) {}\n    fn flush(&self) {}\n    fn drain(&self) {}\n    fn process(&self) {}\n    fn submit(&self) {}\n    fn cancel(&self) {}\n    fn wait(&self) {}\n    fn poll(&self) {}\n    fn tick(&self) {}\n    fn advance(&self) {}\n    fn check(&self) {}\n}\n";
        std::fs::write(src_dir.join("handler.rs"), src_content).unwrap();

        // tests/test_handler.rs — short test stub (lines 0-5)
        let test_content: &[u8] = b"struct MyHandler;\n#[test]\nfn it_works() {}\n";
        std::fs::write(tests_dir.join("test_handler.rs"), test_content).unwrap();

        let src_result =
            crate::parsing::process_file("src/handler.rs", src_content, LanguageId::Rust);
        let src_indexed = crate::live_index::store::IndexedFile::from_parse_result(
            src_result,
            src_content.to_vec(),
        );

        let test_result =
            crate::parsing::process_file("tests/test_handler.rs", test_content, LanguageId::Rust);
        let test_indexed = crate::live_index::store::IndexedFile::from_parse_result(
            test_result,
            test_content.to_vec(),
        );

        let server = make_server_with_root(
            make_live_index_ready(vec![
                ("src/handler.rs".to_string(), src_indexed),
                ("tests/test_handler.rs".to_string(), test_indexed),
            ]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "how does MyHandler work?".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;

        assert!(
            result.contains("Chosen tool: get_symbol_context"),
            "multi-definition symbol should still upgrade to get_symbol_context: {result}"
        );
        assert!(
            result.contains("Invocation: get_symbol_context(name=\"MyHandler\")"),
            "result: {result}"
        );
    }

    #[tokio::test]
    async fn test_ask_keeps_broad_explore_for_generic_understanding_query() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"fn handle() {}\n";
        std::fs::write(src_dir.join("lib.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/lib.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/lib.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "how does handle work?".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;

        assert!(result.contains("Chosen tool: explore"), "result: {result}");
        assert!(
            !result.contains("Chosen tool: get_symbol_context"),
            "generic symbol names should not hijack broad explain queries: {result}"
        );
    }

    #[tokio::test]
    async fn test_ask_upgrades_broad_type_query_to_find_implementations() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"trait Actor {}\nstruct BusActor;\nimpl Actor for BusActor {}\nstruct WorkerActor;\nimpl Actor for WorkerActor {}\n";
        std::fs::write(src_dir.join("actors.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/actors.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/actors.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "what are the main actor types?".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;

        assert!(
            result.contains("Route confidence: inferred"),
            "result: {result}"
        );
        assert!(
            result.contains("Chosen tool: find_references"),
            "result: {result}"
        );
        assert!(
            result
                .contains("Invocation: find_references(name=\"Actor\", mode=\"implementations\")"),
            "result: {result}"
        );
        assert!(
            result.contains("trait-like symbol inside a broad explanation query"),
            "result: {result}"
        );
    }

    #[tokio::test]
    async fn test_ask_keeps_broad_explore_for_generic_type_query_without_trait_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"trait Actor {}\nstruct BusActor;\nimpl Actor for BusActor {}\n";
        std::fs::write(src_dir.join("actors.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/actors.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/actors.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "what are the main types?".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;

        assert!(result.contains("Chosen tool: explore"), "result: {result}");
        assert!(
            !result.contains("Chosen tool: find_references"),
            "generic type phrasing should stay broad without a distinctive trait candidate: {result}"
        );
    }

    #[tokio::test]
    async fn test_ask_preserves_path_scope_hint_for_caller_query() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"fn helper() {}\nfn caller() { helper(); }\n";
        std::fs::write(src_dir.join("lib.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/lib.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/lib.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::SmartQueryInput {
            query: "who calls helper in src/lib.rs".to_string(),
            max_tokens: None,
        };
        let result = server.ask(Parameters(input)).await;

        assert!(
            result.contains("Invocation: find_references(name=\"helper\", path=\"src/lib.rs\")"),
            "result: {result}"
        );
        assert!(result.contains("src/lib.rs"), "result: {result}");
        assert!(result.contains("caller"), "result: {result}");
    }

    #[tokio::test]
    async fn test_edit_plan_accepts_path_qualified_symbol_target() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let content = b"fn helper() {}\n";
        std::fs::write(src_dir.join("lib.rs"), content).unwrap();

        let result = crate::parsing::process_file("src/lib.rs", content, LanguageId::Rust);
        let indexed =
            crate::live_index::store::IndexedFile::from_parse_result(result, content.to_vec());
        let server = make_server_with_root(
            make_live_index_ready(vec![("src/lib.rs".to_string(), indexed)]),
            Some(dir.path().to_path_buf()),
        );

        let input = super::EditPlanInput {
            target: "src/lib.rs::helper".to_string(),
        };
        let result = server.edit_plan(Parameters(input)).await;

        assert!(
            result.contains("Found 1 symbol(s) matching"),
            "result: {result}"
        );
        assert!(result.contains("src/lib.rs"), "result: {result}");
        assert!(result.contains("helper"), "result: {result}");
        assert!(!result.contains("not found"), "result: {result}");
    }

    #[test]
    fn test_filter_paths_code_only() {
        let paths = vec![
            "src/main.rs".to_string(),
            "README.md".to_string(),
            "Cargo.toml".to_string(),
            "src/lib.rs".to_string(),
            ".github/workflows/ci.yml".to_string(),
            "package.json".to_string(),
        ];
        let result = super::filter_paths_by_prefix_and_language(paths, None, None, true).unwrap();
        assert_eq!(result, vec!["src/main.rs", "src/lib.rs"]);
    }

    #[test]
    fn test_parse_language_filter_frontend_and_data_languages() {
        use super::parse_language_filter;
        use crate::domain::index::LanguageId;

        // Frontend languages previously missing from the filter
        assert_eq!(
            parse_language_filter(Some("html")),
            Ok(Some(LanguageId::Html))
        );
        assert_eq!(
            parse_language_filter(Some("HTML")),
            Ok(Some(LanguageId::Html))
        );
        assert_eq!(
            parse_language_filter(Some("css")),
            Ok(Some(LanguageId::Css))
        );
        assert_eq!(
            parse_language_filter(Some("CSS")),
            Ok(Some(LanguageId::Css))
        );
        assert_eq!(
            parse_language_filter(Some("scss")),
            Ok(Some(LanguageId::Scss))
        );
        assert_eq!(
            parse_language_filter(Some("SCSS")),
            Ok(Some(LanguageId::Scss))
        );

        // Data / config languages previously missing
        assert_eq!(
            parse_language_filter(Some("json")),
            Ok(Some(LanguageId::Json))
        );
        assert_eq!(
            parse_language_filter(Some("JSON")),
            Ok(Some(LanguageId::Json))
        );
        assert_eq!(
            parse_language_filter(Some("toml")),
            Ok(Some(LanguageId::Toml))
        );
        assert_eq!(
            parse_language_filter(Some("TOML")),
            Ok(Some(LanguageId::Toml))
        );
        assert_eq!(
            parse_language_filter(Some("yaml")),
            Ok(Some(LanguageId::Yaml))
        );
        assert_eq!(
            parse_language_filter(Some("YAML")),
            Ok(Some(LanguageId::Yaml))
        );
        assert_eq!(
            parse_language_filter(Some("markdown")),
            Ok(Some(LanguageId::Markdown))
        );
        assert_eq!(
            parse_language_filter(Some("md")),
            Ok(Some(LanguageId::Markdown))
        );
        assert_eq!(
            parse_language_filter(Some("env")),
            Ok(Some(LanguageId::Env))
        );

        // Existing languages still work
        assert_eq!(
            parse_language_filter(Some("Rust")),
            Ok(Some(LanguageId::Rust))
        );
        assert_eq!(
            parse_language_filter(Some("TypeScript")),
            Ok(Some(LanguageId::TypeScript))
        );

        // Empty / None returns Ok(None)
        assert_eq!(parse_language_filter(None), Ok(None));
        assert_eq!(parse_language_filter(Some("")), Ok(None));

        // Unknown language returns an error
        assert!(parse_language_filter(Some("COBOL")).is_err());
    }

    #[test]
    fn test_normalize_search_text_glob() {
        use super::normalize_search_text_glob;
        // Bare filename → auto-prefix with **/
        assert_eq!(
            normalize_search_text_glob(Some("foo.rs")),
            Some("**/foo.rs".to_string())
        );
        // Path with separator → no prefix
        assert_eq!(
            normalize_search_text_glob(Some("src/foo.rs")),
            Some("src/foo.rs".to_string())
        );
        // Already has glob char → no prefix
        assert_eq!(
            normalize_search_text_glob(Some("*.rs")),
            Some("*.rs".to_string())
        );
        assert_eq!(
            normalize_search_text_glob(Some("[test]*.rs")),
            Some("[test]*.rs".to_string())
        );
        assert_eq!(
            normalize_search_text_glob(Some("src/**/*.ts")),
            Some("src/**/*.ts".to_string())
        );
        // Leading ./ stripped, then bare → auto-prefix
        assert_eq!(
            normalize_search_text_glob(Some("./foo.rs")),
            Some("**/foo.rs".to_string())
        );
        // Backslash replaced, leading / stripped, then bare → auto-prefix
        assert_eq!(
            normalize_search_text_glob(Some("\\foo.rs")),
            Some("**/foo.rs".to_string())
        );
        // Empty / whitespace → None
        assert_eq!(normalize_search_text_glob(Some("")), None);
        assert_eq!(normalize_search_text_glob(Some("  ")), None);
        assert_eq!(normalize_search_text_glob(None), None);
    }

    #[test]
    fn test_lenient_option_vec_native_array() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "super::lenient_option_vec")]
            items: Option<Vec<String>>,
        }
        let t: T = serde_json::from_str(r#"{"items": ["a", "b"]}"#).unwrap();
        assert_eq!(t.items, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn test_lenient_option_vec_stringified_array() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "super::lenient_option_vec")]
            items: Option<Vec<String>>,
        }
        let t: T = serde_json::from_str(r#"{"items": "[\"a\", \"b\"]"}"#).unwrap();
        assert_eq!(t.items, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn test_lenient_option_vec_null() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "super::lenient_option_vec")]
            items: Option<Vec<String>>,
        }
        let t: T = serde_json::from_str(r#"{"items": null}"#).unwrap();
        assert_eq!(t.items, None);
    }

    #[test]
    fn test_lenient_option_vec_empty_string() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "super::lenient_option_vec")]
            items: Option<Vec<String>>,
        }
        let t: T = serde_json::from_str(r#"{"items": ""}"#).unwrap();
        assert_eq!(t.items, None);
    }

    #[test]
    fn test_lenient_vec_required_native() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct T {
            #[serde(deserialize_with = "super::lenient_vec_required")]
            items: Vec<String>,
        }
        let t: T = serde_json::from_str(r#"{"items": ["x"]}"#).unwrap();
        assert_eq!(t.items, vec!["x".to_string()]);
    }

    #[test]
    fn test_lenient_vec_required_stringified() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct T {
            #[serde(deserialize_with = "super::lenient_vec_required")]
            items: Vec<String>,
        }
        let t: T = serde_json::from_str(r#"{"items": "[\"x\"]"}"#).unwrap();
        assert_eq!(t.items, vec!["x".to_string()]);
    }

    #[test]
    fn test_diff_symbols_compact_shows_omission_note() {
        let dir = init_git_repo();
        let a_path = dir.path().join("a.rs");
        let b_path = dir.path().join("b.rs");
        std::fs::write(&a_path, "fn old_func() {}\n").unwrap();
        std::fs::write(&b_path, "// comment\n").unwrap();
        run_git(dir.path(), &["add", "."]);
        run_git(dir.path(), &["commit", "-m", "base"]);

        std::fs::write(&a_path, "fn old_func() {}\nfn new_func() {}\n").unwrap();
        std::fs::write(&b_path, "// changed comment\n").unwrap();
        run_git(dir.path(), &["add", "."]);
        run_git(dir.path(), &["commit", "-m", "changes"]);

        let repo = crate::git::GitRepo::open(dir.path()).expect("open git repo");

        let result = super::format::diff_symbols_result_view(
            "HEAD~1",
            "HEAD",
            &["a.rs", "b.rs"],
            &repo,
            true,
            false,
        );

        assert!(
            result.contains("1 file(s) with only non-symbol changes omitted"),
            "compact mode should note omitted files: {result}"
        );
    }

    #[tokio::test]
    async fn test_diff_symbols_reports_trust_envelope() {
        let repo = init_git_repo();
        let file_path = repo.path().join("src/lib.rs");
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(&file_path, "fn old_func() {}\n").unwrap();
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "base"]);
        std::fs::write(&file_path, "fn old_func() {}\nfn new_func() {}\n").unwrap();
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "changes"]);

        let server = make_server_with_root(
            make_live_index_ready(vec![]),
            Some(repo.path().to_path_buf()),
        );
        let result = server
            .diff_symbols(Parameters(super::DiffSymbolsInput {
                base: Some("HEAD~1".to_string()),
                target: Some("HEAD".to_string()),
                path_prefix: None,
                language: None,
                code_only: None,
                compact: Some(true),
                summary_only: None,
                estimate: None,
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("Match type: exact (git ref diff)"),
            "diff_symbols should report match type: {result}"
        );
        assert!(
            result.contains("Source authority: git ref diff"),
            "diff_symbols should report git authority: {result}"
        );
        assert!(
            result.contains("Parse state: high (tree-sitter AST extraction for supported languages, regex fallback for others)"),
            "diff_symbols should report AST parse state: {result}"
        );
        assert!(
            result.contains("Scope: git diff `HEAD~1`...`HEAD`; compact output"),
            "diff_symbols should report scope: {result}"
        );
    }

    // Frecency bump/no-bump wiring is verified end-to-end against a real
    // `FrecencyStore` on a tempdir in `tests/frecency_ranking.rs`. That suite
    // is the canonical home for those assertions.
}
