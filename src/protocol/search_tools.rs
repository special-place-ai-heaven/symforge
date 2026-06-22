//! Search-oriented MCP tool input types and pure request parsing helpers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::index::LanguageId;
use crate::live_index::search;

use super::read_tools::{lenient_bool, lenient_option_vec, lenient_u32, lenient_u64};

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
    /// When true, include vendored/third-party paths (vendor/, node_modules/, third_party/).
    /// Default false -- vendor noise dominates symbol lookup in repos with embedded grammars.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_vendor: Option<bool>,
    /// When true, include personal tooling paths (.claude/gsd-*).
    /// Default false -- personal sidecars rarely answer codebase symbol questions.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_personal_tooling: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Feature 012 (Phase 3): target a SINGLE open project by id/alias instead of
    /// the session's active project. Mutually exclusive with `projects`. Must be a
    /// project id/alias, NEVER a filesystem path (a path is rejected with a
    /// corrective error pointing at `index_folder(add:true)`). Omitting both
    /// `project` and `projects` targets the active project (today's behavior).
    #[serde(default)]
    pub project: Option<String>,
    /// Feature 012 (Phase 3): target an EXPLICIT subset of open projects by
    /// id/alias, or `["*"]` for every open project. Mutually exclusive with
    /// `project`. An empty list is rejected (no silent "all"). Daemon-only.
    #[serde(default)]
    pub projects: Option<Vec<String>>,
}

/// Input for `search_text`.
#[derive(Deserialize, Serialize, JsonSchema, Default)]
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
    /// Feature 012 (Phase 3): target a SINGLE open project by id/alias instead of
    /// the session's active project. Mutually exclusive with `projects`; must be a
    /// project id/alias, never a path. Omitting both targets the active project.
    #[serde(default)]
    pub project: Option<String>,
    /// Feature 012 (Phase 3): target an EXPLICIT subset of open projects by
    /// id/alias, or `["*"]` for every open project. Mutually exclusive with
    /// `project`; an empty list is rejected. Daemon-only.
    #[serde(default)]
    pub projects: Option<Vec<String>>,
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
    /// Deprecated since v7.x: prefer `rank_by="path+cochange"` with
    /// `anchor_path=<path>`. This compatibility path still finds files that
    /// frequently co-change with this file via git temporal coupling, but is
    /// scheduled for removal in v8.x.
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
    /// When true, append a compact ranking explanation unless ranking diagnostics are disabled by policy.
    /// Missing values default off; `SYMFORGE_DEBUG_RANKING=1` may still default diagnostics on operationally.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub debug_ranking: Option<bool>,
    /// Optional ranking mode. `"frecency"` requests call-time frecency
    /// resolution: available session or persistent history is fused with
    /// path ranking, otherwise the response includes explicit fallback,
    /// unavailable, or disabled-by-policy evidence.
    /// `"path+cochange"` fuses path match with the coupling store when
    /// `anchor_path` is set and ready coupling data exists; otherwise the
    /// response reports call-time preparation, fallback, unavailable, stale,
    /// or disabled-by-policy evidence.
    /// Any other value (including `None`) preserves the default
    /// tier-based ordering exactly.
    ///
    /// The separate `changed_with=` branch is preserved for compatibility.
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
    /// Feature 012 (Phase 3): target a SINGLE open project by id/alias instead of
    /// the session's active project. Mutually exclusive with `projects`; must be a
    /// project id/alias, never a path. Omitting both targets the active project.
    #[serde(default)]
    pub project: Option<String>,
    /// Feature 012 (Phase 3): target an EXPLICIT subset of open projects by
    /// id/alias, or `["*"]` for every open project. Mutually exclusive with
    /// `project`; an empty list is rejected. Daemon-only.
    #[serde(default)]
    pub projects: Option<Vec<String>>,
}

pub(crate) fn parse_language_filter(input: Option<&str>) -> Result<Option<LanguageId>, String> {
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

pub(crate) fn normalize_search_text_glob(input: Option<&str>) -> Option<String> {
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

/// Attempt to fix double-escaped regex character classes (e.g., `\\s` -> `\s`).
/// Returns `Some(fixed)` if the pattern contains likely double-escaped sequences,
/// `None` otherwise.
pub(crate) fn fix_common_double_escapes(pattern: &str) -> Option<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\\\\([sdwbntSDWB])").expect("static regex")
    });
    if !RE.is_match(pattern) {
        return None;
    }
    Some(RE.replace_all(pattern, r"\$1").to_string())
}
