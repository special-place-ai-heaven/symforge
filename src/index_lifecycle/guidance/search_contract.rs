//! Shared search request contracts and option defaults.

use super::filters::{normalize_path_prefix, normalize_search_text_glob, parse_language_filter};
use super::serde_input::{lenient_bool, lenient_option_vec, lenient_u32, lenient_u64};
use crate::live_index::search;
use serde::{Deserialize, Serialize};

/// Input for `search_symbols`.
#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
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
    /// When true, include vendored/third-party paths (vendor/, node_modules/, third_party/).
    /// Default false -- vendor noise dominates symbol lookup in repos with embedded grammars.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_vendor: Option<bool>,
    /// When true, include personal tooling paths (.claude/gsd-*).
    /// Default false -- personal sidecars rarely answer codebase symbol questions.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_personal_tooling: Option<bool>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Optional open-project ID/alias; exclusive with `projects`. Omit both for
    /// the active project. IDs/aliases only, never paths; path selectors error
    /// with guidance to `index_folder(add:true)`.
    #[serde(default)]
    pub project: Option<String>,
    /// Feature 012 (Phase 3): target an EXPLICIT subset of open projects by
    /// id/alias, or `["*"]` for every open project. Mutually exclusive with
    /// `project`. An empty list is rejected (no silent "all"). Daemon-only.
    // `#[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]` keeps this a plain `type: "array"`
    // schema, NOT the `type: ["array", "null"]` union that strict MCP clients
    // reject (mirrors `SearchTextInput::terms`; enforced by
    // `tests/strict_client_schema_compat.rs`); serde keeps the field optional.
    #[serde(default)]
    #[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]
    pub projects: Option<Vec<String>>,
}

/// Input for `search_text`.
#[derive(Deserialize, Serialize, Default)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
pub struct SearchTextInput {
    /// Search query (case-insensitive substring match unless `regex` is true).
    pub query: Option<String>,
    /// Optional list of terms to match with OR semantics.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]
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
    /// When true, include test files AND `#[cfg(test)]` modules in the result
    /// set. Both are EXCLUDED by default, so a query that only matches test
    /// code returns zero source hits (a hint reports how many test matches were
    /// suppressed); set this true to see them.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_tests: Option<bool>,
    /// When true, include vendored/third-party paths (vendor/, node_modules/, third_party/).
    /// Default false -- vendor noise dominates results in repos with embedded grammars.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_vendor: Option<bool>,
    /// When true, include personal tooling paths (.claude/gsd-*).
    /// Default false -- personal sidecars rarely answer code questions.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_personal_tooling: Option<bool>,
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
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget; output truncates at a line boundary.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// When true, interpret `query` as an ast-grep structural pattern instead of a text search.
    /// Matches AST patterns using tree-sitter. Use `$VAR` for single-node metavariables
    /// and `$$$` for multi-node wildcards. Example: `fn $NAME($$$) { $$$ }`.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub structural: Option<bool>,
    /// Optional open-project ID/alias; exclusive with `projects`. Omit both for
    /// the active project. IDs/aliases only, never paths.
    #[serde(default)]
    pub project: Option<String>,
    /// Feature 012 (Phase 3): target an EXPLICIT subset of open projects by
    /// id/alias, or `["*"]` for every open project. Mutually exclusive with
    /// `project`; an empty list is rejected. Daemon-only.
    // `#[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]` keeps this a plain `type: "array"`
    // schema, NOT the `type: ["array", "null"]` union that strict MCP clients
    // reject (mirrors `SearchTextInput::terms`; enforced by
    // `tests/strict_client_schema_compat.rs`); serde keeps the field optional.
    #[serde(default)]
    #[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]
    pub projects: Option<Vec<String>>,
}

/// Input for `search_files`.
#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
pub struct SearchFilesInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    /// Daemon only: select open projects by ID/name or `["*"]` for all. Exclusive
    /// with `project`; applies only to plain fuzzy queries. Resolve/coupling modes
    /// stay single-project.
    // `#[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]` keeps this a plain `type: "array"`
    // schema, NOT the `type: ["array", "null"]` union that strict MCP clients
    // reject (mirrors the sibling discovery verbs; enforced by
    // `tests/strict_client_schema_compat.rs`); serde keeps the field optional.
    #[serde(default)]
    #[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]
    pub projects: Option<Vec<String>>,
    /// Filename, folder name, or partial path. Required for search and resolve modes. Optional when `changed_with` is provided.
    #[serde(default)]
    pub query: String,
    /// Optional maximum number of matches to return (default 20, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Optional current file path to boost local results.
    pub current_file: Option<String>,
    /// Deprecated since v7.x: prefer `rank_by="path+cochange"` with
    /// `anchor_path=<path>`. This compatibility path still finds files that
    /// frequently co-change with this file via git temporal coupling, but is
    /// scheduled for removal in v8.x.
    pub changed_with: Option<String>,
    /// Set to true for exact path resolution mode: resolves an ambiguous filename or partial path to one exact project path.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub resolve: Option<bool>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// When true, append a compact ranking explanation unless ranking diagnostics are disabled by policy.
    /// Missing values default off; `SYMFORGE_DEBUG_RANKING=1` may still default diagnostics on operationally.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub debug_ranking: Option<bool>,
    /// Optional ranking: `"frecency"` fuses available session/persistent history
    /// with path ranking; otherwise the response reports fallback, unavailable,
    /// or policy-disabled evidence. `"path+cochange"` fuses path match with
    /// coupling when `anchor_path` is set and data is ready; otherwise it reports
    /// preparation, fallback, unavailable, stale, or policy-disabled evidence.
    /// Other values (including `None`) keep default tier ordering. The separate
    /// `changed_with=` branch remains compatible.
    #[serde(default)]
    pub rank_by: Option<String>,
    /// Anchor file used as the co-change pivot when `rank_by="path+cochange"`.
    #[serde(default)]
    pub anchor_path: Option<String>,
    /// When true, include vendored/third-party paths (vendor/, node_modules/, third_party/).
    /// Default false -- vendor noise dominates path lookup in repos with embedded grammars.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_vendor: Option<bool>,
    /// When true, include personal tooling paths (.claude/gsd-*).
    /// Default false -- personal sidecars rarely answer codebase path questions.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_personal_tooling: Option<bool>,
    /// Optional relative path prefix scope, for example `src/` or `src/protocol`.
    pub path_prefix: Option<String>,
}

pub(crate) fn search_symbols_options_from_input(
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
            include_vendor: input.include_vendor.unwrap_or(false),
            include_ignored: false,
        },
        include_personal_tooling: input.include_personal_tooling.unwrap_or(false),
        language_filter: parse_language_filter(input.language.as_deref())?,
    })
}

pub(crate) fn search_text_options_from_input(
    input: &SearchTextInput,
) -> Result<search::TextSearchOptions, String> {
    let is_regex = input.regex.unwrap_or(false);
    let is_ranked = input.ranked.unwrap_or(false);

    // When regex mode is active the user is doing a targeted, precise search
    // and expects completeness over test noise by default. Vendor remains
    // opt-in because embedded grammars and dependencies can dominate result
    // caps before project code is seen.
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
            include_vendor: input.include_vendor.unwrap_or(false),
            include_ignored: false,
        },
        include_personal_tooling: input.include_personal_tooling.unwrap_or(false),
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
