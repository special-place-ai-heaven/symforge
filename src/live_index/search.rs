use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};

use crate::domain::{FileClass, FileClassification, LanguageId, SymbolKind};
use crate::live_index::LiveIndex;
use crate::live_index::query::{SearchFilesHit, SearchFilesTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolMatchTier {
    Exact = 0,
    Prefix = 1,
    Substring = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSearchHit {
    pub tier: SymbolMatchTier,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSearchResult {
    pub file_count: usize,
    pub hits: Vec<SymbolSearchHit>,
    pub overflow_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PathScope {
    #[default]
    Any,
    Exact(String),
    Prefix(String),
}

impl PathScope {
    pub const fn any() -> Self {
        Self::Any
    }

    pub fn exact(path: impl Into<String>) -> Self {
        Self::Exact(path.into())
    }

    pub fn prefix(path_prefix: impl Into<String>) -> Self {
        Self::Prefix(path_prefix.into())
    }

    pub fn matches(&self, path: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(exact_path) => path == exact_path,
            Self::Prefix(path_prefix) => {
                let normalized_prefix = path_prefix.trim_end_matches('/');
                normalized_prefix.is_empty()
                    || path == normalized_prefix
                    || path
                        .strip_prefix(normalized_prefix)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    All,
    #[default]
    Code,
    Text,
    Binary,
}

impl SearchScope {
    pub const fn allows(self, classification: &FileClassification) -> bool {
        match self {
            Self::All => true,
            Self::Code => matches!(classification.class, FileClass::Code),
            Self::Text => matches!(classification.class, FileClass::Text),
            Self::Binary => matches!(classification.class, FileClass::Binary),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultLimit(usize);

impl ResultLimit {
    pub const fn new(limit: usize) -> Self {
        Self(limit)
    }

    pub const fn symbol_search_default() -> Self {
        Self(50)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for ResultLimit {
    fn default() -> Self {
        Self::symbol_search_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentContext {
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub around_line: Option<u32>,
    pub around_match: Option<String>,
    pub match_occurrence: Option<u32>,
    pub around_symbol: Option<String>,
    pub symbol_line: Option<u32>,
    pub context_lines: Option<u32>,
    pub chunk_index: Option<u32>,
    pub max_lines: Option<u32>,
    pub show_line_numbers: bool,
    pub header: bool,
    /// The mode used for content selection (e.g. "lines", "symbol", "match", "chunk").
    pub mode_name: Option<String>,
    /// Whether the mode was explicitly specified by the caller or inferred from flags.
    pub mode_explicit: bool,
}

impl ContentContext {
    pub fn line_range(start_line: Option<u32>, end_line: Option<u32>) -> Self {
        Self::line_range_with_format(start_line, end_line, false, false)
    }

    pub fn line_range_with_format(
        start_line: Option<u32>,
        end_line: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            start_line,
            end_line,
            around_line: None,
            around_match: None,
            match_occurrence: None,
            around_symbol: None,
            symbol_line: None,
            context_lines: None,
            chunk_index: None,
            max_lines: None,
            show_line_numbers,
            header,
            mode_name: None,
            mode_explicit: false,
        }
    }

    pub fn around_line(
        around_line: u32,
        context_lines: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            start_line: None,
            end_line: None,
            around_line: Some(around_line),
            around_match: None,
            match_occurrence: None,
            around_symbol: None,
            symbol_line: None,
            context_lines,
            chunk_index: None,
            max_lines: None,
            show_line_numbers,
            header,
            mode_name: None,
            mode_explicit: false,
        }
    }

    pub fn around_match(
        around_match: impl Into<String>,
        context_lines: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self::around_match_occurrence(around_match, None, context_lines, show_line_numbers, header)
    }

    pub fn around_match_occurrence(
        around_match: impl Into<String>,
        match_occurrence: Option<u32>,
        context_lines: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            start_line: None,
            end_line: None,
            around_line: None,
            around_match: Some(around_match.into()),
            match_occurrence,
            around_symbol: None,
            symbol_line: None,
            context_lines,
            chunk_index: None,
            max_lines: None,
            show_line_numbers,
            header,
            mode_name: None,
            mode_explicit: false,
        }
    }

    pub fn around_symbol(
        around_symbol: impl Into<String>,
        symbol_line: Option<u32>,
        context_lines: Option<u32>,
    ) -> Self {
        Self::around_symbol_with_max_lines(
            around_symbol,
            symbol_line,
            context_lines,
            None,
            false,
            false,
        )
    }

    pub fn around_symbol_with_max_lines(
        around_symbol: impl Into<String>,
        symbol_line: Option<u32>,
        context_lines: Option<u32>,
        max_lines: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            start_line: None,
            end_line: None,
            around_line: None,
            around_match: None,
            match_occurrence: None,
            around_symbol: Some(around_symbol.into()),
            symbol_line,
            context_lines,
            chunk_index: None,
            max_lines,
            show_line_numbers,
            header,
            mode_name: None,
            mode_explicit: false,
        }
    }

    pub fn chunk(chunk_index: u32, max_lines: u32) -> Self {
        Self {
            start_line: None,
            end_line: None,
            around_line: None,
            around_match: None,
            match_occurrence: None,
            around_symbol: None,
            symbol_line: None,
            context_lines: None,
            chunk_index: Some(chunk_index),
            max_lines: Some(max_lines),
            show_line_numbers: false,
            header: false,
            mode_name: None,
            mode_explicit: false,
        }
    }
}

/// Semantic noise classification for files matched by gitignore or path heuristics.
///
/// This is **suppressive** — files are down-ranked/tagged in explore and repo_map,
/// but remain visible to search_text, search_symbols, and get_file_context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoiseClass {
    /// Not noise — normal project source.
    #[default]
    None,
    /// Third-party / vendored dependency (vendor/, node_modules/, etc.).
    Vendor,
    /// Machine-generated output (.lock, .min.js, /dist/, etc.).
    Generated,
    /// Matched by .gitignore but not classified as vendor or generated.
    Ignored,
}

impl NoiseClass {
    /// Human-readable tag for file tree rendering (empty string for None).
    pub fn tag(self) -> &'static str {
        match self {
            NoiseClass::None => "",
            NoiseClass::Vendor => "[vendor]",
            NoiseClass::Generated => "[generated]",
            NoiseClass::Ignored => "[ignored]",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoisePolicy {
    pub include_generated: bool,
    pub include_tests: bool,
    pub include_vendor: bool,
    pub include_ignored: bool,
}

impl NoisePolicy {
    pub const fn permissive() -> Self {
        Self {
            include_generated: true,
            include_tests: true,
            include_vendor: true,
            include_ignored: true,
        }
    }

    pub const fn hide_classified_noise() -> Self {
        Self {
            include_generated: false,
            include_tests: false,
            include_vendor: false,
            include_ignored: false,
        }
    }

    pub const fn allows(self, classification: &FileClassification) -> bool {
        (self.include_generated || !classification.is_generated)
            && (self.include_tests || !classification.is_test)
            && (self.include_vendor || !classification.is_vendor)
    }

    /// Classify a file path into a `NoiseClass` using heuristic rules first,
    /// then falling back to gitignore patterns.
    ///
    /// Heuristic priority:
    /// 1. Vendor directories: `vendor/`, `node_modules/`, `third_party/`, etc.
    /// 2. Generated artifacts: `.lock`, `.min.js`, `.min.css`, `/dist/`, `/generated/`, etc.
    /// 3. Gitignore catch-all: matched by .gitignore but not heuristic → `Ignored`
    pub fn classify_path(
        path: &str,
        gitignore: Option<&ignore::gitignore::Gitignore>,
    ) -> NoiseClass {
        let lower = path.replace('\\', "/").to_ascii_lowercase();
        let segments: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
        let basename = segments.last().copied().unwrap_or("");

        // 1. Vendor heuristic. Shares VENDOR_PATH_SEGMENTS with
        // FileClassification::for_code_path so search_symbols/search_text agree
        // with explore/repo_map/search_files on what counts as vendored
        // (SF-STRESS-011).
        let is_vendor = segments
            .iter()
            .any(|s| crate::domain::index::VENDOR_PATH_SEGMENTS.contains(s));
        if is_vendor {
            return NoiseClass::Vendor;
        }

        // 2. Generated heuristic. Directory roots share GENERATED_PATH_SEGMENTS
        // with FileClassification; the basename suffixes below additionally cover
        // lockfiles and source maps that are not directory-keyed.
        let is_generated = segments
            .iter()
            .any(|s| crate::domain::index::GENERATED_PATH_SEGMENTS.contains(s))
            || basename.ends_with(".lock")
            || basename.contains("-lock.")
            || basename.ends_with(".min.js")
            || basename.ends_with(".min.css")
            || basename.ends_with(".map")
            || basename.contains(".generated.")
            || basename.contains(".gen.")
            || basename.ends_with(".g.dart")
            || basename.ends_with(".pb.go")
            || basename.ends_with(".designer.cs");
        if is_generated {
            return NoiseClass::Generated;
        }

        // 3. Gitignore catch-all
        // The `ignore` crate asserts that paths are relative (!path.has_root()).
        // Guard against absolute paths reaching here (e.g., from watcher events
        // or concurrent agents passing unsanitized paths).
        if let Some(gi) = gitignore {
            let p = std::path::Path::new(path);
            if !p.has_root() {
                let matched = gi.matched_path_or_any_parents(path, false);
                if matched.is_ignore() {
                    return NoiseClass::Ignored;
                }
            }
        }

        NoiseClass::None
    }

    /// Whether a file with the given noise class should be hidden in suppressive views.
    pub fn should_hide(self, class: NoiseClass) -> bool {
        match class {
            NoiseClass::None => false,
            NoiseClass::Vendor => !self.include_vendor,
            NoiseClass::Generated => !self.include_generated,
            NoiseClass::Ignored => !self.include_ignored,
        }
    }
}

impl Default for NoisePolicy {
    fn default() -> Self {
        Self::permissive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SymbolSearchOptions {
    pub path_scope: PathScope,
    pub search_scope: SearchScope,
    pub result_limit: ResultLimit,
    pub noise_policy: NoisePolicy,
    pub include_personal_tooling: bool,
    pub language_filter: Option<LanguageId>,
}

impl SymbolSearchOptions {
    pub fn for_current_code_search(result_limit: usize) -> Self {
        Self {
            path_scope: PathScope::any(),
            search_scope: SearchScope::Code,
            result_limit: ResultLimit::new(result_limit),
            noise_policy: NoisePolicy {
                include_generated: false,
                include_tests: false,
                include_vendor: true,
                include_ignored: false,
            },
            include_personal_tooling: true,
            language_filter: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextSearchOptions {
    pub path_scope: PathScope,
    pub search_scope: SearchScope,
    pub noise_policy: NoisePolicy,
    pub include_personal_tooling: bool,
    pub language_filter: Option<LanguageId>,
    pub total_limit: usize,
    pub max_per_file: usize,
    pub glob: Option<String>,
    pub exclude_glob: Option<String>,
    pub context: Option<usize>,
    pub case_sensitive: Option<bool>,
    pub whole_word: bool,
    pub ranked: bool,
    /// Optional pre-computed churn scores keyed by relative file path.
    /// When `Some`, the ranking code uses these instead of the default `0.0`.
    /// Populated by the protocol handler from `GitTemporalIndex` when available.
    pub churn_scores: Option<HashMap<String, f32>>,
}

impl Default for TextSearchOptions {
    fn default() -> Self {
        Self {
            path_scope: PathScope::default(),
            search_scope: SearchScope::default(),
            noise_policy: NoisePolicy::default(),
            include_personal_tooling: true,
            language_filter: None,
            total_limit: 50,
            max_per_file: 5,
            glob: None,
            exclude_glob: None,
            context: None,
            case_sensitive: None,
            whole_word: false,
            ranked: false,
            churn_scores: None,
        }
    }
}

impl TextSearchOptions {
    pub fn for_current_code_search() -> Self {
        Self {
            path_scope: PathScope::any(),
            search_scope: SearchScope::Code,
            noise_policy: NoisePolicy {
                include_generated: false,
                include_tests: false,
                include_vendor: true,
                include_ignored: false,
            },
            include_personal_tooling: true,
            language_filter: None,
            total_limit: 50,
            max_per_file: 5,
            glob: None,
            exclude_glob: None,
            context: None,
            case_sensitive: None,
            whole_word: false,
            ranked: false,
            churn_scores: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContentOptions {
    pub path_scope: PathScope,
    pub content_context: ContentContext,
}

impl FileContentOptions {
    pub fn for_explicit_path_read(
        path: impl Into<String>,
        start_line: Option<u32>,
        end_line: Option<u32>,
    ) -> Self {
        Self::for_explicit_path_read_with_format(path, start_line, end_line, false, false)
    }

    pub fn for_explicit_path_read_with_format(
        path: impl Into<String>,
        start_line: Option<u32>,
        end_line: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            path_scope: PathScope::exact(path),
            content_context: ContentContext::line_range_with_format(
                start_line,
                end_line,
                show_line_numbers,
                header,
            ),
        }
    }

    pub fn for_explicit_path_read_around_line(
        path: impl Into<String>,
        around_line: u32,
        context_lines: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            path_scope: PathScope::exact(path),
            content_context: ContentContext::around_line(
                around_line,
                context_lines,
                show_line_numbers,
                header,
            ),
        }
    }

    pub fn for_explicit_path_read_around_match(
        path: impl Into<String>,
        around_match: impl Into<String>,
        match_occurrence: Option<u32>,
        context_lines: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            path_scope: PathScope::exact(path),
            content_context: ContentContext::around_match_occurrence(
                around_match,
                match_occurrence,
                context_lines,
                show_line_numbers,
                header,
            ),
        }
    }

    pub fn for_explicit_path_read_chunk(
        path: impl Into<String>,
        chunk_index: u32,
        max_lines: u32,
    ) -> Self {
        Self {
            path_scope: PathScope::exact(path),
            content_context: ContentContext::chunk(chunk_index, max_lines),
        }
    }

    pub fn for_explicit_path_read_around_symbol(
        path: impl Into<String>,
        around_symbol: impl Into<String>,
        symbol_line: Option<u32>,
        context_lines: Option<u32>,
        max_lines: Option<u32>,
        show_line_numbers: bool,
        header: bool,
    ) -> Self {
        Self {
            path_scope: PathScope::exact(path),
            content_context: ContentContext::around_symbol_with_max_lines(
                around_symbol,
                symbol_line,
                context_lines,
                max_lines,
                show_line_numbers,
                header,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclosingMatchSymbol {
    pub name: String,
    pub kind: String,
    pub line_range: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLineMatch {
    pub line_number: usize,
    pub line: String,
    pub enclosing_symbol: Option<EnclosingMatchSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRenderedLine {
    pub line_number: usize,
    pub line: String,
    pub is_match: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextDisplayLine {
    Separator,
    Line(TextRenderedLine),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerEntry {
    pub file: String,
    pub symbol: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFileMatches {
    pub path: String,
    pub matches: Vec<TextLineMatch>,
    pub rendered_lines: Option<Vec<TextDisplayLine>>,
    pub callers: Option<Vec<CallerEntry>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSearchResult {
    pub label: String,
    pub total_matches: usize,
    pub files: Vec<TextFileMatches>,
    /// Matches that were found but suppressed by noise policy (e.g., inside test modules).
    pub suppressed_by_noise: usize,
    pub overflow_count: usize,
}

pub const SUPPRESSED_TEXT_MATCH_DISPLAY_CAP: usize = 100;

/// Hard per-line character cap applied at capture time to every source line
/// stored for text-search output. A single committed minified line can be
/// hundreds of kilobytes; emitting it verbatim detonates the tool result and
/// can blow an agent's entire context budget (SF-STRESS-008). Capping at
/// capture time means every downstream renderer (default/symbol/usage modes,
/// context windows) inherits the bound for free.
pub const MAX_DISPLAY_LINE_CHARS: usize = 2000;

/// Truncate a source line to a bounded, char-boundary-safe head excerpt with an
/// honest marker when it exceeds [`MAX_DISPLAY_LINE_CHARS`].
///
/// Slicing is done over `char_indices` so multibyte UTF-8 (e.g. U+2028 in
/// minified bundles) never causes a byte-boundary panic. Lines at or under the
/// cap are returned unchanged. The marker reports the number of characters
/// omitted so the result stays honest about what was elided.
fn truncate_display_line(line: &str) -> String {
    // Fast path: most lines are short. `len()` (bytes) is a cheap upper bound on
    // char count, so a line whose byte length fits the cap cannot exceed it in
    // chars and needs no scan.
    if line.len() <= MAX_DISPLAY_LINE_CHARS {
        return line.to_string();
    }

    let total_chars = line.chars().count();
    if total_chars <= MAX_DISPLAY_LINE_CHARS {
        return line.to_string();
    }

    // Take a char-boundary-safe head excerpt of the first MAX_DISPLAY_LINE_CHARS
    // characters. nth(n) yields the byte offset of the (n+1)-th char start.
    let cut_byte = line
        .char_indices()
        .nth(MAX_DISPLAY_LINE_CHARS)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(line.len());
    let omitted = total_chars - MAX_DISPLAY_LINE_CHARS;
    format!(
        "{}... [line truncated, {omitted} chars omitted]",
        &line[..cut_byte]
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSearchError {
    EmptyRegexQuery,
    EmptyQueryOrTerms,
    InvalidRegex {
        pattern: String,
        error: String,
    },
    InvalidGlob {
        field: &'static str,
        pattern: String,
        error: String,
    },
    UnsupportedWholeWordRegex,
    InvalidStructuralPattern {
        pattern: String,
        error: String,
    },
    UnsupportedStructuralLanguage {
        pattern: String,
        sample_error: String,
    },
}

struct CompiledTextGlobFilters {
    include: Option<GlobMatcher>,
    exclude: Option<GlobMatcher>,
}

impl CompiledTextGlobFilters {
    fn matches(&self, path: &str) -> bool {
        self.include
            .as_ref()
            .is_none_or(|include| include.is_match(path))
            && self
                .exclude
                .as_ref()
                .is_none_or(|exclude| !exclude.is_match(path))
    }
}

struct ScoredSymbolMatch {
    tier: SymbolMatchTier,
    tiebreak: u32,
    name: String,
    path: String,
    kind: String,
    line: u32,
}

/// Returns true when `filter` matches the given `kind`, accepting both the
/// Display string (e.g. `"let"` for `Variable`, `"fn"` for `Function`) and
/// common semantic aliases (e.g. `"variable"`, `"function"`, `"method"`).
fn kind_filter_matches(filter: &str, kind: &crate::domain::SymbolKind) -> bool {
    if kind.to_string().eq_ignore_ascii_case(filter) {
        return true;
    }
    // Semantic aliases: users naturally write "variable", "function", "method"
    // even though the display strings are "let", "fn", "fn".
    matches!(
        (filter.to_ascii_lowercase().as_str(), kind),
        ("variable", crate::domain::SymbolKind::Variable)
            | ("function", crate::domain::SymbolKind::Function)
            | ("method", crate::domain::SymbolKind::Method)
            | ("module", crate::domain::SymbolKind::Module)
            | ("constant", crate::domain::SymbolKind::Constant)
    )
}

pub fn search_symbols(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
    result_limit: usize,
) -> SymbolSearchResult {
    let options = SymbolSearchOptions::for_current_code_search(result_limit);
    search_symbols_with_options(index, query, kind_filter, &options)
}

pub fn search_symbols_with_options(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
    options: &SymbolSearchOptions,
) -> SymbolSearchResult {
    let query_lower = query.to_lowercase();
    let mut matches: Vec<ScoredSymbolMatch> = Vec::new();

    let mut paths: Vec<&String> = index.all_files().map(|(path, _)| path).collect();
    paths.sort();

    for path in paths {
        let file = index
            .get_file(path)
            .expect("path from all_files must exist");
        if !options.path_scope.matches(path)
            || !options.search_scope.allows(&file.classification)
            || !options.noise_policy.allows(&file.classification)
            || (!options.include_personal_tooling
                && crate::live_index::query::is_personal_tooling_path(path))
            || options
                .language_filter
                .as_ref()
                .is_some_and(|language| &file.language != language)
        {
            continue;
        }
        // Precompute test module byte ranges so we can skip symbols inside
        // inline `mod tests` blocks even when the file itself is not a test file.
        let test_module_ranges: Vec<(u32, u32)> =
            if !options.noise_policy.include_tests && !file.classification.is_test {
                file.symbols
                    .iter()
                    .filter(|s| {
                        s.kind == crate::domain::SymbolKind::Module
                            && matches!(s.name.as_str(), "tests" | "test")
                    })
                    .map(|s| s.byte_range)
                    .collect()
            } else {
                vec![]
            };

        for sym in &file.symbols {
            // Skip symbols inside inline test modules (e.g. `mod tests { struct T; }`)
            if !test_module_ranges.is_empty()
                && test_module_ranges
                    .iter()
                    .any(|&(start, end)| sym.byte_range.0 >= start && sym.byte_range.1 <= end)
            {
                continue;
            }

            if let Some(filter) = kind_filter
                && !filter.eq_ignore_ascii_case("all")
                && !kind_filter_matches(filter, &sym.kind)
            {
                continue;
            }

            let name_lower = sym.name.to_lowercase();
            if !name_lower.contains(&query_lower) {
                continue;
            }

            let (tier, tiebreak) = if name_lower == query_lower {
                (SymbolMatchTier::Exact, 0u32)
            } else if name_lower.starts_with(&query_lower) {
                (SymbolMatchTier::Prefix, sym.name.len() as u32)
            } else {
                let pos = name_lower.find(&query_lower).unwrap_or(0) as u32;
                (SymbolMatchTier::Substring, pos)
            };

            matches.push(ScoredSymbolMatch {
                tier,
                tiebreak,
                name: sym.name.clone(),
                path: path.clone(),
                kind: sym.kind.to_string(),
                line: sym.line_range.0 + 1,
            });
        }
    }

    matches.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(a.tiebreak.cmp(&b.tiebreak))
            .then(a.name.cmp(&b.name))
    });

    let total_matches = matches.len();
    let overflow_count = total_matches.saturating_sub(options.result_limit.get());
    let hits: Vec<SymbolSearchHit> = matches
        .into_iter()
        .take(options.result_limit.get())
        .map(|m| SymbolSearchHit {
            tier: m.tier,
            name: m.name,
            path: m.path,
            kind: m.kind,
            line: m.line,
        })
        .collect();

    let file_count = hits
        .iter()
        .map(|h| h.path.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();

    SymbolSearchResult {
        file_count,
        hits,
        overflow_count,
    }
}

pub fn search_text(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
) -> Result<TextSearchResult, TextSearchError> {
    search_text_with_options(
        index,
        query,
        terms,
        regex,
        &TextSearchOptions::for_current_code_search(),
    )
}

pub fn search_text_with_options(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
    options: &TextSearchOptions,
) -> Result<TextSearchResult, TextSearchError> {
    let compiled_globs = compile_text_glob_filters(options)?;
    let case_sensitive = options.case_sensitive.unwrap_or(regex);
    let normalized_terms: Vec<String> = match terms {
        Some(raw_terms) if !raw_terms.is_empty() => raw_terms
            .iter()
            .map(|term| term.trim())
            .filter(|term| !term.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => query
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| vec![text.to_string()])
            .unwrap_or_default(),
    };

    if regex {
        if options.whole_word {
            return Err(TextSearchError::UnsupportedWholeWordRegex);
        }
        let pattern = query
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or("");
        if pattern.is_empty() {
            return Err(TextSearchError::EmptyRegexQuery);
        }

        let regex = match regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()
        {
            Ok(regex) => regex,
            Err(error) => {
                return Err(TextSearchError::InvalidRegex {
                    pattern: pattern.to_string(),
                    error: error.to_string(),
                });
            }
        };

        let mut candidate_paths = Vec::new();
        let mut suppressed_candidate_paths = Vec::new();
        for (path, file) in index.all_files() {
            if !file_matches_text_base_scope(path, file, options, &compiled_globs) {
                continue;
            }
            if file_hidden_by_search_policy(path, file, options) {
                suppressed_candidate_paths.push(path.clone());
            } else {
                candidate_paths.push(path.clone());
            }
        }
        let label = format!("regex '{pattern}'");
        let mut result = collect_text_matches(
            index,
            candidate_paths,
            |line| regex.is_match(line),
            label.clone(),
            options,
        );
        result.suppressed_by_noise =
            result
                .suppressed_by_noise
                .saturating_add(count_suppressed_text_matches(
                    index,
                    suppressed_candidate_paths,
                    |line| regex.is_match(line),
                    label,
                    options,
                ));
        return Ok(result);
    }

    if normalized_terms.is_empty() {
        return Err(TextSearchError::EmptyQueryOrTerms);
    }

    let mut candidate_paths = HashSet::new();
    let mut suppressed_candidate_paths = HashSet::new();
    for term in &normalized_terms {
        for path in index.trigram_index.search(term.as_bytes(), &index.files) {
            let Some(file) = index.get_file(&path) else {
                continue;
            };
            if !file_matches_text_base_scope(&path, file, options, &compiled_globs) {
                continue;
            }
            if file_hidden_by_search_policy(&path, file, options) {
                suppressed_candidate_paths.insert(path);
            } else {
                candidate_paths.insert(path);
            }
        }
    }

    let lowered_terms = (!case_sensitive).then(|| {
        normalized_terms
            .iter()
            .map(|term| term.to_lowercase())
            .collect::<Vec<_>>()
    });
    let whole_word_matcher = options
        .whole_word
        .then(|| compile_literal_whole_word_matcher(&normalized_terms, case_sensitive));

    // Aho-Corasick automaton for multi-term OR searches: matches all patterns in
    // a single pass over each line, eliminating per-line .to_lowercase() allocation
    // for case-insensitive searches.  Only used when: >1 term, not whole-word mode,
    // and all terms are ASCII (aho-corasick only does ASCII case folding).
    let all_ascii = normalized_terms.iter().all(|t| t.is_ascii());
    let multi_term_ac =
        (normalized_terms.len() > 1 && !options.whole_word && all_ascii).then(|| {
            aho_corasick::AhoCorasick::builder()
                .ascii_case_insensitive(!case_sensitive)
                .build(&normalized_terms)
                .expect("literal terms always build")
        });

    let label = if normalized_terms.len() == 1 {
        format!("'{}'", normalized_terms[0])
    } else {
        format!("terms [{}]", normalized_terms.join(", "))
    };

    // When multiple OR terms are provided, scale up the total_limit so each term
    // gets a fair share of results.  Without this, high-frequency terms dominate
    // the ranked file list and exhaust the limit before rarer terms appear.
    let effective_options;
    let opts = if normalized_terms.len() > 1 {
        effective_options = TextSearchOptions {
            total_limit: options.total_limit.saturating_mul(normalized_terms.len()),
            max_per_file: options.max_per_file,
            case_sensitive: options.case_sensitive,
            whole_word: options.whole_word,
            path_scope: options.path_scope.clone(),
            language_filter: options.language_filter.clone(),
            glob: options.glob.clone(),
            exclude_glob: options.exclude_glob.clone(),
            search_scope: options.search_scope,
            noise_policy: options.noise_policy,
            include_personal_tooling: options.include_personal_tooling,
            context: options.context,
            ranked: options.ranked,
            churn_scores: options.churn_scores.clone(),
        };
        &effective_options
    } else {
        options
    };

    let line_matches = |line: &str| -> bool {
        if let Some(matcher) = whole_word_matcher.as_ref() {
            return matcher.is_match(line);
        }

        if let Some(ac) = multi_term_ac.as_ref() {
            return ac.is_match(line);
        }

        // Single-term path (or non-ASCII multi-term fallback)
        if case_sensitive {
            normalized_terms.iter().any(|term| line.contains(term))
        } else {
            let lowered = line.to_lowercase();
            lowered_terms
                .as_ref()
                .expect("lowered terms should exist for case-insensitive search")
                .iter()
                .any(|term| lowered.contains(term))
        }
    };

    let mut result = collect_text_matches(
        index,
        candidate_paths.into_iter().collect(),
        |line| line_matches(line),
        label.clone(),
        opts,
    );
    result.suppressed_by_noise =
        result
            .suppressed_by_noise
            .saturating_add(count_suppressed_text_matches(
                index,
                suppressed_candidate_paths.into_iter().collect(),
                |line| line_matches(line),
                label,
                opts,
            ));
    Ok(result)
}

fn count_suppressed_text_matches<F>(
    index: &LiveIndex,
    candidate_paths: Vec<String>,
    mut is_match: F,
    _label: String,
    _options: &TextSearchOptions,
) -> usize
where
    F: FnMut(&str) -> bool,
{
    if candidate_paths.is_empty() {
        return 0;
    }

    let mut suppressed = 0usize;
    for path in candidate_paths {
        let Some(file) = index.get_file(&path) else {
            continue;
        };
        let content_str = String::from_utf8_lossy(&file.content);

        for line in content_str.lines() {
            let line = line.trim_end_matches('\r');
            if !is_match(line) {
                continue;
            }
            suppressed = suppressed.saturating_add(1);
            if suppressed > SUPPRESSED_TEXT_MATCH_DISPLAY_CAP {
                return SUPPRESSED_TEXT_MATCH_DISPLAY_CAP + 1;
            }
        }
    }
    suppressed
}

fn file_matches_text_base_scope(
    path: &str,
    file: &crate::live_index::IndexedFile,
    options: &TextSearchOptions,
    glob_filters: &CompiledTextGlobFilters,
) -> bool {
    options.path_scope.matches(path)
        && glob_filters.matches(path)
        && options.search_scope.allows(&file.classification)
        && options
            .language_filter
            .as_ref()
            .is_none_or(|language| &file.language == language)
}

fn file_hidden_by_search_policy(
    path: &str,
    file: &crate::live_index::IndexedFile,
    options: &TextSearchOptions,
) -> bool {
    !options.noise_policy.allows(&file.classification)
        || (!options.include_personal_tooling
            && crate::live_index::query::is_personal_tooling_path(path))
}

fn file_matches_text_options(
    path: &str,
    file: &crate::live_index::IndexedFile,
    options: &TextSearchOptions,
    glob_filters: &CompiledTextGlobFilters,
) -> bool {
    file_matches_text_base_scope(path, file, options, glob_filters)
        && !file_hidden_by_search_policy(path, file, options)
}

/// Structural (AST-pattern) search across indexed files.
///
/// Uses ast-grep to match a tree-sitter AST pattern against source files.
/// Returns results in the same `TextSearchResult` format as text search so
/// the existing rendering pipeline can display them unchanged.
pub fn search_structural(
    index: &LiveIndex,
    pattern: &str,
    options: &TextSearchOptions,
) -> Result<TextSearchResult, TextSearchError> {
    search_structural_with_compiler(
        index,
        pattern,
        options,
        crate::parsing::ast_grep::compile_structural_pattern,
    )
}

fn search_structural_with_compiler(
    index: &LiveIndex,
    pattern: &str,
    options: &TextSearchOptions,
    mut compile_pattern: impl FnMut(
        &str,
        &LanguageId,
        bool,
    ) -> Result<
        crate::parsing::ast_grep::CompiledStructuralPattern,
        String,
    >,
) -> Result<TextSearchResult, TextSearchError> {
    let compiled_globs = compile_text_glob_filters(options)?;

    let mut files: Vec<TextFileMatches> = Vec::new();
    let mut total_matches = 0usize;
    let mut suppressed_by_noise = 0usize;
    // Cache key includes the TSX flavor because `.tsx` and `.ts` share the
    // TypeScript LanguageId but compile against different tree-sitter grammars.
    let mut compiled_patterns: HashMap<
        (LanguageId, bool, String),
        Result<crate::parsing::ast_grep::CompiledStructuralPattern, String>,
    > = HashMap::new();
    // Track whether at least one candidate successfully compiled the
    // pattern. If every candidate errored, we must distinguish:
    //   * at least one candidate rejected the pattern as syntactically
    //     invalid — propagate as InvalidStructuralPattern (pattern bug).
    //   * every candidate was in a language ast-grep does not support —
    //     propagate as UnsupportedStructuralLanguage (index/filter bug).
    // Conflating the two was the original Unit 5 bug; split buckets fix it.
    let mut any_parse_succeeded = false;
    let mut first_syntax_error: Option<String> = None;
    let mut first_unsupported_error: Option<String> = None;

    // Collect candidate files filtered by options.
    let mut candidates: Vec<(String, crate::domain::index::LanguageId)> = index
        .all_files()
        .filter(|(path, file)| file_matches_text_options(path, file, options, &compiled_globs))
        .map(|(path, file)| (path.clone(), file.language.clone()))
        .collect();
    candidates.sort_by(|a, b| a.0.cmp(&b.0));

    for (path, lang) in &candidates {
        if total_matches >= options.total_limit {
            break;
        }

        let file = match index.get_file(path) {
            Some(f) => f,
            None => continue,
        };

        let content_str = String::from_utf8_lossy(&file.content);

        let is_tsx = LanguageId::is_tsx_path(path);
        let compiled = compiled_patterns
            .entry((lang.clone(), is_tsx, pattern.to_string()))
            .or_insert_with(|| compile_pattern(pattern, lang, is_tsx));
        let structural_matches = match compiled {
            Ok(compiled_pattern) => {
                any_parse_succeeded = true;
                crate::parsing::ast_grep::structural_search_with_compiled(
                    &content_str,
                    compiled_pattern,
                )
            }
            Err(e) => {
                // compile_structural_pattern returns two distinguishable error
                // shapes: "structural search not supported for …" for
                // config languages, and "invalid structural pattern: …"
                // for ast-grep Pattern::try_new failures.
                let e = e.clone();
                if e.starts_with("invalid structural pattern") {
                    if first_syntax_error.is_none() {
                        first_syntax_error = Some(e);
                    }
                } else if first_unsupported_error.is_none() {
                    first_unsupported_error = Some(e);
                }
                continue;
            }
        };

        if structural_matches.is_empty() {
            continue;
        }

        // Pre-compute Rust test module line ranges for noise filtering.
        let test_ranges: Vec<(u32, u32)> = if !options.noise_policy.include_tests
            && *lang == crate::domain::index::LanguageId::Rust
        {
            compute_test_ranges(file)
        } else {
            Vec::new()
        };

        let remaining = options.total_limit.saturating_sub(total_matches);
        let per_file_limit = options.max_per_file.min(remaining);

        let mut matches: Vec<TextLineMatch> = Vec::new();
        for sm in &structural_matches {
            let line_idx = sm.start_line;

            // Skip matches inside Rust #[cfg(test)] modules.
            if !test_ranges.is_empty() {
                let line_num = line_idx as u32;
                if test_ranges
                    .iter()
                    .any(|&(start, end)| line_num >= start && line_num <= end)
                {
                    suppressed_by_noise += 1;
                    continue;
                }
            }

            // Build display line: first line of matched text + captures summary.
            let first_line = sm.text.lines().next().unwrap_or(&sm.text);
            let display = if sm.captures.is_empty() {
                first_line.to_string()
            } else {
                let caps: Vec<String> = sm
                    .captures
                    .iter()
                    .map(|(name, val)| {
                        let short = if val.len() > 40 {
                            format!("{}...", &val[..37])
                        } else {
                            val.clone()
                        };
                        format!("${name}={short}")
                    })
                    .collect();
                format!("{first_line}  // {}", caps.join(", "))
            };

            matches.push(TextLineMatch {
                line_number: line_idx + 1,
                line: display,
                enclosing_symbol: file
                    .symbols
                    .iter()
                    .filter(|s| {
                        s.line_range.0 <= (line_idx as u32) && s.line_range.1 >= (line_idx as u32)
                    })
                    .max_by_key(|s| s.depth)
                    .map(|s| EnclosingMatchSymbol {
                        name: s.name.clone(),
                        kind: s.kind.to_string(),
                        line_range: s.line_range,
                    }),
            });
            if matches.len() >= per_file_limit {
                break;
            }
        }

        if !matches.is_empty() {
            let rendered_lines = options
                .context
                .map(|ctx| build_context_rendered_lines(&content_str, &matches, ctx));
            total_matches += matches.len();
            files.push(TextFileMatches {
                path: path.clone(),
                matches,
                rendered_lines,
                callers: None,
            });
        }
    }

    // If no candidate compiled the pattern, surface the right error kind.
    // Syntax errors win over "unsupported language" because a syntax error
    // means the USER has a bug in their pattern; the language-unsupported
    // bucket only matters when every candidate was in a config language.
    if !any_parse_succeeded {
        if let Some(error) = first_syntax_error {
            return Err(TextSearchError::InvalidStructuralPattern {
                pattern: pattern.to_string(),
                error,
            });
        }
        if let Some(sample_error) = first_unsupported_error {
            return Err(TextSearchError::UnsupportedStructuralLanguage {
                pattern: pattern.to_string(),
                sample_error,
            });
        }
    }

    Ok(TextSearchResult {
        label: format!("structural '{pattern}'"),
        total_matches,
        files,
        suppressed_by_noise,
        overflow_count: 0,
    })
}

fn compile_text_glob_filters(
    options: &TextSearchOptions,
) -> Result<CompiledTextGlobFilters, TextSearchError> {
    Ok(CompiledTextGlobFilters {
        include: compile_text_glob("glob", options.glob.as_deref())?,
        exclude: compile_text_glob("exclude_glob", options.exclude_glob.as_deref())?,
    })
}

fn compile_text_glob(
    field: &'static str,
    pattern: Option<&str>,
) -> Result<Option<GlobMatcher>, TextSearchError> {
    let Some(pattern) = pattern.filter(|pattern| !pattern.is_empty()) else {
        return Ok(None);
    };

    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|error| TextSearchError::InvalidGlob {
            field,
            pattern: pattern.to_string(),
            error: error.to_string(),
        })?;

    Ok(Some(glob.compile_matcher()))
}

fn compile_literal_whole_word_matcher(terms: &[String], case_sensitive: bool) -> regex::Regex {
    let pattern = format!(
        r"\b(?:{})\b",
        terms
            .iter()
            .map(|term| regex::escape(term))
            .collect::<Vec<_>>()
            .join("|")
    );

    regex::RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .expect("escaped literal whole-word matcher should compile")
}

/// Returns a 0.0–1.0 priority score based on symbol kind.
pub(crate) fn symbol_kind_priority(kind: &str) -> f32 {
    match kind {
        "function" | "method" | "async_function" | "generator" => 1.0,
        "class" | "struct" | "enum" | "interface" | "trait" | "union" => 0.8,
        "impl" | "implementation" => 0.7,
        "module" | "namespace" => 0.5,
        "constant" | "const" => 0.4,
        "variable" | "type" | "type_alias" => 0.3,
        "key" | "section" | "property" | "field" => 0.2,
        _ => 0.1,
    }
}

/// Compute a semantic importance score for a file's search results.
/// Combines match count, caller connectivity, and symbol kind.
fn compute_importance_score(
    file_matches: &TextFileMatches,
    match_count_max: usize,
    reverse_index: &HashMap<String, Vec<super::store::ReferenceLocation>>,
    churn_score: f32,
) -> f32 {
    let match_count_norm = if match_count_max > 0 {
        file_matches.matches.len() as f32 / match_count_max as f32
    } else {
        0.0
    };

    // Find max caller count among enclosing symbols
    let max_callers = file_matches
        .matches
        .iter()
        .filter_map(|m| m.enclosing_symbol.as_ref())
        .map(|sym| {
            reverse_index
                .get(&sym.name)
                .map(|refs| refs.len())
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);
    // Calibrated for medium repos; high-caller utilities saturate at 1.0.
    const CALLER_COUNT_NORMALIZER: f32 = 20.0;
    let caller_norm = (max_callers as f32 / CALLER_COUNT_NORMALIZER).min(1.0);

    // Find max kind priority among enclosing symbols
    let max_kind = file_matches
        .matches
        .iter()
        .filter_map(|m| m.enclosing_symbol.as_ref())
        .map(|sym| symbol_kind_priority(&sym.kind))
        .fold(0.0f32, f32::max);

    // Composite score
    0.30 * match_count_norm + 0.40 * caller_norm + 0.15 * churn_score + 0.15 * max_kind
}

fn compute_test_ranges(file: &crate::live_index::IndexedFile) -> Vec<(u32, u32)> {
    file.symbols
        .iter()
        .filter(|s| s.name == "tests" && s.kind == SymbolKind::Module)
        .map(|s| s.line_range)
        .collect()
}

fn attach_text_match_context(
    index: &LiveIndex,
    options: &TextSearchOptions,
    file_matches: &mut TextFileMatches,
) {
    let Some(context) = options.context else {
        return;
    };
    let Some(file) = index.get_file(&file_matches.path) else {
        return;
    };
    let content_str = String::from_utf8_lossy(&file.content);
    file_matches.rendered_lines = Some(build_context_rendered_lines(
        &content_str,
        &file_matches.matches,
        context,
    ));
}

fn collect_text_matches<F>(
    index: &LiveIndex,
    candidate_paths: Vec<String>,
    mut is_match: F,
    label: String,
    options: &TextSearchOptions,
) -> TextSearchResult
where
    F: FnMut(&str) -> bool,
{
    struct PathMatchBucket {
        path: String,
        visible_count: usize,
        matches: Vec<TextLineMatch>,
    }

    // Single scan per file: count every visible hit for overflow reporting while
    // retaining only the bounded match lines needed for output/ranking.
    let mut buckets: Vec<PathMatchBucket> = Vec::new();
    let mut suppressed_by_noise: usize = 0;
    let mut all_visible_matches: usize = 0;
    for path in candidate_paths {
        let file = match index.get_file(&path) {
            Some(file) => file,
            None => continue,
        };
        let content_str = String::from_utf8_lossy(&file.content);

        // Pre-compute Rust test module line ranges for noise filtering.
        let test_ranges: Vec<(u32, u32)> =
            if !options.noise_policy.include_tests && file.language == LanguageId::Rust {
                compute_test_ranges(file)
            } else {
                Vec::new()
            };

        let mut visible_count = 0usize;
        let mut suppressed = 0usize;
        let mut matches: Vec<TextLineMatch> = Vec::new();
        for (line_idx, line) in content_str.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if !is_match(line) {
                continue;
            }
            // Skip matches inside Rust #[cfg(test)] modules.
            if !test_ranges.is_empty() {
                let line_num = line_idx as u32;
                if test_ranges
                    .iter()
                    .any(|&(start, end)| line_num >= start && line_num <= end)
                {
                    suppressed += 1;
                    continue;
                }
            }
            visible_count += 1;
            if matches.len() < options.max_per_file {
                matches.push(TextLineMatch {
                    line_number: line_idx + 1,
                    line: truncate_display_line(line),
                    enclosing_symbol: file
                        .symbols
                        .iter()
                        .filter(|s| {
                            s.line_range.0 <= (line_idx as u32)
                                && s.line_range.1 >= (line_idx as u32)
                        })
                        .max_by_key(|s| s.depth)
                        .map(|s| EnclosingMatchSymbol {
                            name: s.name.clone(),
                            kind: s.kind.to_string(),
                            line_range: s.line_range,
                        }),
                });
            }
        }
        suppressed_by_noise += suppressed;
        all_visible_matches += visible_count;
        if visible_count > 0 {
            buckets.push(PathMatchBucket {
                path,
                visible_count,
                matches,
            });
        }
    }

    // Sort by match count descending, alphabetical tiebreak.
    buckets.sort_by(|a, b| {
        b.visible_count
            .cmp(&a.visible_count)
            .then_with(|| a.path.cmp(&b.path))
    });

    let mut files: Vec<TextFileMatches> = Vec::new();
    let mut total_matches = 0usize;

    // When ranked, collect from all files (up to a safety cap) so the ranker
    // can reorder across the full set.  The total_limit is applied *after*
    // ranking to trim the final output.  Without this, high-match-count files
    // exhaust the budget before diverse but important files are visited.
    const RANKED_FILE_CAP: usize = 500;

    for (file_idx, mut bucket) in buckets.into_iter().enumerate() {
        if options.ranked {
            if file_idx >= RANKED_FILE_CAP {
                break;
            }
        } else if total_matches >= options.total_limit {
            break;
        }

        let per_file_limit = if options.ranked {
            options.max_per_file
        } else {
            let remaining_total = options.total_limit.saturating_sub(total_matches);
            options.max_per_file.min(remaining_total)
        };

        if per_file_limit == 0 {
            break;
        }

        if bucket.matches.len() > per_file_limit {
            bucket.matches.truncate(per_file_limit);
        }

        if !bucket.matches.is_empty() {
            total_matches += bucket.matches.len();
            let mut file_matches = TextFileMatches {
                path: bucket.path,
                matches: bucket.matches,
                rendered_lines: None,
                callers: None,
            };
            if !options.ranked {
                attach_text_match_context(index, options, &mut file_matches);
            }
            files.push(file_matches);
        }
    }

    // Apply semantic re-ranking when requested.
    if options.ranked {
        let match_count_max = files.iter().map(|f| f.matches.len()).max().unwrap_or(1);
        let churn = options.churn_scores.as_ref();
        files.sort_by(|a, b| {
            let churn_a = churn.and_then(|m| m.get(&a.path).copied()).unwrap_or(0.0);
            let churn_b = churn.and_then(|m| m.get(&b.path).copied()).unwrap_or(0.0);
            let score_a =
                compute_importance_score(a, match_count_max, &index.reverse_index, churn_a);
            let score_b =
                compute_importance_score(b, match_count_max, &index.reverse_index, churn_b);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to total_limit *after* ranking so the most important files
        // survive regardless of their raw match count.
        let mut remaining = options.total_limit;
        files.retain_mut(|f| {
            if remaining == 0 {
                return false;
            }
            if f.matches.len() > remaining {
                f.matches.truncate(remaining);
            }
            remaining -= f.matches.len();
            true
        });
        total_matches = files.iter().map(|f| f.matches.len()).sum();
        for file_matches in &mut files {
            attach_text_match_context(index, options, file_matches);
        }
    }

    TextSearchResult {
        label,
        total_matches,
        files,
        suppressed_by_noise,
        overflow_count: all_visible_matches.saturating_sub(total_matches),
    }
}

fn build_context_rendered_lines(
    content: &str,
    matches: &[TextLineMatch],
    context: usize,
) -> Vec<TextDisplayLine> {
    if matches.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = content
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut windows: Vec<(usize, usize)> = Vec::new();
    for line_match in matches {
        let start = line_match.line_number.saturating_sub(context).max(1);
        let end = (line_match.line_number + context).min(lines.len());

        if let Some((_, last_end)) = windows.last_mut()
            && start <= *last_end + 1
        {
            *last_end = (*last_end).max(end);
            continue;
        }
        windows.push((start, end));
    }

    let match_lines: HashSet<usize> = matches
        .iter()
        .map(|line_match| line_match.line_number)
        .collect();
    let mut rendered: Vec<TextDisplayLine> = Vec::new();

    for (window_idx, (start, end)) in windows.into_iter().enumerate() {
        if window_idx > 0 {
            rendered.push(TextDisplayLine::Separator);
        }
        for line_number in start..=end {
            rendered.push(TextDisplayLine::Line(TextRenderedLine {
                line_number,
                line: truncate_display_line(lines[line_number - 1]),
                is_match: match_lines.contains(&line_number),
            }));
        }
    }

    rendered
}

// ── File-search frecency fusion ────────────────────────────────────────────
//
// Opt-in rerank used by `search_files` when `rank_by="frecency"` is set and
// call-time frecency history is available. Default callers keep the existing
// tier-based comparator in `query::capture_search_files_view`.
//
// Contract: `combined = 0.6 * path_match + 0.3 * co_change + 0.1 * frecency_norm`
// with frecency normalized against the max raw score in the candidate set so
// every contribution sits on the same `[0, 1]` scale. Anchors the fusion
// weights from the spec's Implementation Notes §"Decay + fusion starting
// parameters".

/// Path-match signal contribution derived from the candidate's tier. Strong
/// (exact or suffix) paths earn the full `1.0`; basenames a solid `0.6`;
/// loose-component and prefix hits `0.3`. Co-change hits don't participate
/// in path-match (they flow through the co-change signal instead).
pub fn tier_path_match_score(tier: SearchFilesTier) -> f64 {
    match tier {
        SearchFilesTier::StrongPath => 1.0,
        SearchFilesTier::Basename => 0.6,
        SearchFilesTier::LoosePath => 0.3,
        SearchFilesTier::CoChange => 0.0,
        // Tier-2 metadata-only paths earn no path-match credit; the hard rank
        // floor in `reorder_hits_by_frecency_fusion` keeps them below Tier-1.
        SearchFilesTier::MetadataOnly => 0.0,
    }
}

/// Per-hit fused score. Returned alongside the re-ranked hits so callers
/// rendering `SYMFORGE_DEBUG_RANKING=1` can show the breakdown without
/// re-deriving anything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrecencyFusionBreakdown {
    pub path_match: f64,
    pub co_change: f64,
    pub frecency_normalized: f64,
    pub combined: f64,
}

/// Re-rank a candidate set by the frecency fusion policy. The returned
/// vector preserves one-to-one correspondence with the input `hits`: each
/// hit keeps its position in the result vec, and the parallel
/// `breakdowns` vec reports the per-hit scores for call-time or default-on
/// ranking diagnostics. Callers can then sort or filter however they need;
/// the `reorder_by_combined` helper is the
/// common case.
pub fn score_hits_by_frecency_fusion(
    hits: &[SearchFilesHit],
    frecency_scores: &HashMap<PathBuf, f64>,
) -> Vec<FrecencyFusionBreakdown> {
    let max_frecency = frecency_scores.values().copied().fold(0.0_f64, f64::max);
    hits.iter()
        .map(|hit| {
            let path_match = tier_path_match_score(hit.tier);
            let co_change = hit.coupling_score.map(f64::from).unwrap_or(0.0);
            let raw = frecency_scores
                .get(Path::new(hit.path.as_str()))
                .copied()
                .unwrap_or(0.0);
            let frecency_normalized = if max_frecency > 0.0 {
                (raw / max_frecency).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let combined = 0.6 * path_match + 0.3 * co_change + 0.1 * frecency_normalized;
            FrecencyFusionBreakdown {
                path_match,
                co_change,
                frecency_normalized,
                combined,
            }
        })
        .collect()
}

/// Re-sort `hits` by descending combined score using the provided
/// `breakdowns` (must be parallel to `hits`). Ties break stably by path so
/// ordering is deterministic across runs.
pub fn reorder_hits_by_frecency_fusion(
    mut hits: Vec<SearchFilesHit>,
    breakdowns: &[FrecencyFusionBreakdown],
) -> Vec<SearchFilesHit> {
    debug_assert_eq!(hits.len(), breakdowns.len());
    let mut indexed: Vec<(usize, SearchFilesHit)> = hits.drain(..).enumerate().collect();
    indexed.sort_by(|a, b| {
        // Rank floor: Tier-2 metadata-only hits never outrank a Tier-1 hit, even
        // if frecency would score them higher. `false` (Tier-1) precedes `true`.
        let a_meta = a.1.tier.is_metadata_only();
        let b_meta = b.1.tier.is_metadata_only();
        if a_meta != b_meta {
            return a_meta.cmp(&b_meta);
        }
        let sa = breakdowns[a.0].combined;
        let sb = breakdowns[b.0].combined;
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.path.cmp(&b.1.path))
    });
    indexed.into_iter().map(|(_, hit)| hit).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant, SystemTime};

    use super::*;
    use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, ParseStatus};
    use crate::live_index::trigram::TrigramIndex;

    fn make_symbol(name: &str, kind: SymbolKind, line: u32) -> SymbolRecord {
        let byte_range = (0, 0);
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (line, line),
            doc_byte_range: None,
        }
    }

    fn make_file_with_classification(
        path: &str,
        content: &str,
        symbols: Vec<SymbolRecord>,
        classification: crate::domain::FileClassification,
    ) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                classification,
                content: content.as_bytes().to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: content.len() as u64,
                content_hash: "hash".to_string(),
                references: Vec::new(),
                alias_map: HashMap::new(),
                mtime_secs: 0,
            },
        )
    }

    fn make_file(path: &str, content: &str, symbols: Vec<SymbolRecord>) -> (String, IndexedFile) {
        make_file_with_classification(
            path,
            content,
            symbols,
            crate::domain::FileClassification::for_code_path(path),
        )
    }

    fn make_index(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        let file_map: HashMap<String, std::sync::Arc<IndexedFile>> = files
            .into_iter()
            .map(|(path, file)| (path, std::sync::Arc::new(file)))
            .collect();
        let trigram_index = TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
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
            coupling_store: None,
            local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        index.rebuild_path_indices();
        index
    }

    #[test]
    fn test_search_module_symbol_search_respects_tiers_and_limit() {
        let index = make_index(vec![
            make_file(
                "src/a.rs",
                "",
                vec![
                    make_symbol("job", SymbolKind::Function, 1),
                    make_symbol("jobQueue", SymbolKind::Function, 2),
                    make_symbol("enqueueJob", SymbolKind::Function, 3),
                ],
            ),
            make_file(
                "src/b.rs",
                "",
                vec![make_symbol("jobber", SymbolKind::Method, 4)],
            ),
        ]);

        let result = search_symbols(&index, "job", None, 3);

        assert_eq!(result.file_count, 2);
        assert_eq!(result.hits.len(), 3);
        assert_eq!(result.hits[0].tier, SymbolMatchTier::Exact);
        assert_eq!(result.hits[0].name, "job");
        assert_eq!(result.hits[1].tier, SymbolMatchTier::Prefix);
        assert_eq!(result.hits[1].name, "jobber");
        assert_eq!(result.hits[2].tier, SymbolMatchTier::Prefix);
        assert_eq!(result.hits[2].name, "jobQueue");
        assert_eq!(result.overflow_count, 1);
    }

    #[test]
    fn test_search_module_symbol_search_kind_filter_allows_all_keyword() {
        let index = make_index(vec![make_file(
            "src/a.rs",
            "",
            vec![
                make_symbol("job", SymbolKind::Function, 1),
                make_symbol("job", SymbolKind::Method, 2),
            ],
        )]);

        let result = search_symbols(&index, "job", Some("all"), 50);

        assert_eq!(result.hits.len(), 2);
    }

    #[test]
    fn test_kind_filter_matches_semantic_aliases() {
        use crate::domain::SymbolKind;
        // Display strings match exactly
        assert!(kind_filter_matches("let", &SymbolKind::Variable));
        assert!(kind_filter_matches("fn", &SymbolKind::Function));
        assert!(kind_filter_matches("fn", &SymbolKind::Method));
        assert!(kind_filter_matches("mod", &SymbolKind::Module));
        assert!(kind_filter_matches("const", &SymbolKind::Constant));
        // Semantic aliases also match
        assert!(kind_filter_matches("variable", &SymbolKind::Variable));
        assert!(kind_filter_matches("Variable", &SymbolKind::Variable));
        assert!(kind_filter_matches("function", &SymbolKind::Function));
        assert!(kind_filter_matches("method", &SymbolKind::Method));
        assert!(kind_filter_matches("module", &SymbolKind::Module));
        assert!(kind_filter_matches("constant", &SymbolKind::Constant));
        // Non-matching pairs
        assert!(!kind_filter_matches("variable", &SymbolKind::Function));
        assert!(!kind_filter_matches("function", &SymbolKind::Variable));
        assert!(!kind_filter_matches("let", &SymbolKind::Constant));
    }

    #[test]
    fn test_search_symbols_kind_variable_alias_finds_scss_variables() {
        // Regression: search_symbols(kind="variable") must find Variable-kind symbols.
        // Previously, SymbolKind::Variable.to_string() == "let", so kind="variable" matched nothing.
        let index = make_index(vec![make_file(
            "styles/tokens.scss",
            "",
            vec![
                make_symbol("$primary-color", SymbolKind::Variable, 1),
                make_symbol("$gap", SymbolKind::Variable, 2),
                make_symbol("flex-center", SymbolKind::Function, 3),
            ],
        )]);

        let result = search_symbols(&index, "", Some("variable"), 50);
        let names: Vec<&str> = result.hits.iter().map(|h| h.name.as_str()).collect();
        assert!(
            names.contains(&"$primary-color"),
            "kind='variable' should find $primary-color, got: {:?}",
            names
        );
        assert!(
            names.contains(&"$gap"),
            "kind='variable' should find $gap, got: {:?}",
            names
        );
        assert!(
            !names.contains(&"flex-center"),
            "kind='variable' should NOT find mixin, got: {:?}",
            names
        );
    }

    #[test]
    fn test_search_module_text_search_terms_are_trimmed_and_grouped() {
        let index = make_index(vec![
            make_file("src/a.rs", "TODO one\nother\nFIXME two\n", Vec::new()),
            make_file("src/b.rs", "todo lower\n", Vec::new()),
        ]);
        let terms = vec![" TODO ".to_string(), "".to_string(), "FIXME".to_string()];

        let result = search_text(&index, None, Some(&terms), false).expect("search should work");

        assert_eq!(result.label, "terms [TODO, FIXME]");
        assert_eq!(result.total_matches, 3);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].path, "src/a.rs");
        assert_eq!(result.files[0].matches[0].line_number, 1);
        assert_eq!(result.files[1].path, "src/b.rs");
        assert_eq!(result.overflow_count, 0);
    }

    #[test]
    fn test_search_module_text_search_reports_overflow_when_limit_truncates() {
        let index = make_index(vec![make_file(
            "src/a.rs",
            "needle one\nneedle two\nneedle three\n",
            Vec::new(),
        )]);
        let options = TextSearchOptions {
            total_limit: 2,
            max_per_file: 2,
            ..TextSearchOptions::default()
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search");

        assert_eq!(result.total_matches, 2);
        assert_eq!(result.overflow_count, 1);
    }

    // ── SF-STRESS-008: per-line truncation guards ──────────────────────────
    //
    // A single committed minified line (hundreds of KB) must never be emitted
    // verbatim; the snippet renderer would otherwise detonate the tool result
    // (~123k tokens for one 490KB line) and consume an agent's whole context.

    #[test]
    fn test_truncate_display_line_caps_overlong_line_with_honest_marker() {
        let long = "a".repeat(500_000);
        let out = truncate_display_line(&long);
        // Head excerpt is exactly the cap; marker accounts for the rest.
        assert!(
            out.starts_with(&"a".repeat(MAX_DISPLAY_LINE_CHARS)),
            "excerpt must retain the first MAX_DISPLAY_LINE_CHARS chars"
        );
        assert!(
            out.contains("[line truncated,"),
            "must carry an honest truncation marker, got len {}",
            out.len()
        );
        let expected_omitted = 500_000 - MAX_DISPLAY_LINE_CHARS;
        assert!(
            out.contains(&format!("{expected_omitted} chars omitted")),
            "marker must report the exact omitted-char count"
        );
        // Output is the excerpt plus a short fixed marker, nowhere near 500KB.
        assert!(
            out.len() < MAX_DISPLAY_LINE_CHARS + 64,
            "truncated line must stay close to the cap, got {} bytes",
            out.len()
        );
    }

    #[test]
    fn test_truncate_display_line_is_char_boundary_safe_on_multibyte() {
        // Mix of multibyte chars (U+2028 line separator, emoji, CJK) repeated
        // past the cap. Byte slicing here would panic; char_indices must not.
        let unit = "a\u{2028}\u{1F600}\u{4E2D}"; // 1 + 3 + 4 + 3 = 11 bytes, 4 chars
        let long = unit.repeat(MAX_DISPLAY_LINE_CHARS); // 4 * cap chars >> cap
        let out = truncate_display_line(&long); // must not panic
        assert!(out.contains("[line truncated,"));
        // The excerpt prefix must itself be valid UTF-8 (guaranteed by &str),
        // and contain exactly MAX_DISPLAY_LINE_CHARS chars before the marker.
        let head = out.split("... [line truncated,").next().unwrap();
        assert_eq!(
            head.chars().count(),
            MAX_DISPLAY_LINE_CHARS,
            "excerpt must hold exactly the cap in chars"
        );
    }

    #[test]
    fn test_truncate_display_line_passes_short_lines_through_unchanged() {
        let short = "let x = 42; // needle here";
        assert_eq!(truncate_display_line(short), short);
        // A line exactly at the cap is untouched.
        let at_cap = "x".repeat(MAX_DISPLAY_LINE_CHARS);
        assert_eq!(truncate_display_line(&at_cap), at_cap);
    }

    #[test]
    fn test_search_text_bounds_a_500kb_single_line_file() {
        // Reproduce the corpus detonation: one ~500KB minified line holding the
        // needle. Pre-fix this emits the line verbatim (~490KB / ~123k tokens);
        // post-fix the stored match line is bounded and carries the marker.
        let mut line = "x".repeat(250_000);
        line.push_str("needle");
        line.push_str(&"y".repeat(250_000));
        let content = format!("{line}\n");
        let index = make_index(vec![make_file("dist/scripts.js", &content, Vec::new())]);

        let result = search_text_with_options(
            &index,
            Some("needle"),
            None,
            false,
            &TextSearchOptions::default(),
        )
        .expect("search should work");

        assert_eq!(result.total_matches, 1, "the needle line must still match");
        let stored = &result.files[0].matches[0].line;
        assert!(
            stored.chars().count() <= MAX_DISPLAY_LINE_CHARS + 64,
            "stored match line must be bounded, got {} chars",
            stored.chars().count()
        );
        assert!(
            stored.contains("[line truncated,"),
            "bounded match line must carry the truncation marker"
        );

        // The renderer emits the stored `line` verbatim, so bounding total
        // stored-line bytes bounds the whole tool result. Pre-fix this sum was
        // ~500KB for this single match; post-fix it is a few KB. With a default
        // max_tokens budget of, say, 2000 tokens (~8KB), the result can no
        // longer be blown by one line.
        let total_line_bytes: usize = result
            .files
            .iter()
            .flat_map(|f| f.matches.iter())
            .map(|m| m.line.len())
            .sum();
        assert!(
            total_line_bytes < 8_000,
            "total stored match-line bytes must stay well under an 8KB (~2k token) budget, got {total_line_bytes}"
        );
    }

    #[test]
    fn test_search_text_context_window_truncates_overlong_neighbor_lines() {
        // Neighbors of a minified line are themselves minified; the context
        // renderer must cap them too, not just the match line.
        let huge_neighbor = "z".repeat(400_000);
        let content = format!("{huge_neighbor}\nneedle here\n{huge_neighbor}\n");
        let index = make_index(vec![make_file("dist/bundle.js", &content, Vec::new())]);
        let options = TextSearchOptions {
            context: Some(1),
            ..TextSearchOptions::default()
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search should work");

        let rendered = result.files[0]
            .rendered_lines
            .as_ref()
            .expect("context mode materializes rendered lines");
        for display in rendered {
            if let TextDisplayLine::Line(rl) = display {
                assert!(
                    rl.line.chars().count() <= MAX_DISPLAY_LINE_CHARS + 64,
                    "every context line (match or neighbor) must be bounded, got {} chars on line {}",
                    rl.line.chars().count(),
                    rl.line_number
                );
            }
        }
    }

    #[test]
    fn test_search_module_text_search_empty_regex_query_errors() {
        let index = make_index(vec![make_file("src/a.rs", "content", Vec::new())]);

        let result = search_text(&index, Some(" "), None, true);

        assert_eq!(result, Err(TextSearchError::EmptyRegexQuery));
    }

    #[test]
    fn test_search_module_symbol_search_with_options_respects_path_scope_and_noise_policy() {
        let index = make_index(vec![
            make_file(
                "src/job.rs",
                "",
                vec![make_symbol("job", SymbolKind::Function, 1)],
            ),
            make_file(
                "tests/generated/job_test.rs",
                "",
                vec![make_symbol("jobNoise", SymbolKind::Function, 2)],
            ),
        ]);
        let options = SymbolSearchOptions {
            path_scope: PathScope::prefix("src/"),
            noise_policy: NoisePolicy::hide_classified_noise(),
            ..Default::default()
        };

        let result = search_symbols_with_options(&index, "job", None, &options);

        assert_eq!(result.file_count, 1);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].path, "src/job.rs");
        assert_eq!(result.hits[0].name, "job");
    }

    #[test]
    fn test_search_module_symbol_search_hides_generated_and_test_noise_by_default() {
        let index = make_index(vec![
            make_file(
                "src/job.rs",
                "",
                vec![make_symbol("Job", SymbolKind::Class, 1)],
            ),
            make_file(
                "src/generated/job_generated.rs",
                "",
                vec![make_symbol("JobGenerated", SymbolKind::Class, 2)],
            ),
            make_file(
                "tests/job_test.rs",
                "",
                vec![make_symbol("JobTest", SymbolKind::Class, 3)],
            ),
        ]);

        let result = search_symbols(&index, "job", None, 50);

        assert_eq!(result.file_count, 1);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].path, "src/job.rs");
        assert_eq!(result.hits[0].name, "Job");
    }

    #[test]
    fn test_search_module_symbol_search_with_options_respects_path_language_and_limit() {
        let rust_model = make_file(
            "src/models/job.rs",
            "",
            vec![
                make_symbol("Job", SymbolKind::Class, 1),
                make_symbol("JobRunner", SymbolKind::Function, 2),
            ],
        );
        let mut ts_ui = make_file(
            "src/ui/job.ts",
            "",
            vec![
                make_symbol("JobCard", SymbolKind::Class, 3),
                make_symbol("JobList", SymbolKind::Class, 4),
            ],
        );
        ts_ui.1.language = LanguageId::TypeScript;
        let noise = make_file(
            "tests/job_test.rs",
            "",
            vec![make_symbol("JobTest", SymbolKind::Function, 5)],
        );
        let index = make_index(vec![rust_model, ts_ui, noise]);
        let options = SymbolSearchOptions {
            path_scope: PathScope::prefix("src/ui"),
            search_scope: SearchScope::Code,
            result_limit: ResultLimit::new(1),
            noise_policy: NoisePolicy::permissive(),
            include_personal_tooling: true,
            language_filter: Some(LanguageId::TypeScript),
        };

        let result = search_symbols_with_options(&index, "job", Some("class"), &options);

        assert_eq!(result.file_count, 1);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].path, "src/ui/job.ts");
        assert_eq!(result.hits[0].name, "JobCard");
        assert_eq!(result.hits[0].kind, "class");
    }

    #[test]
    fn test_search_module_text_search_with_options_respects_scope_and_path() {
        let mut text_classification =
            crate::domain::FileClassification::for_code_path("docs/readme.md");
        text_classification.class = FileClass::Text;
        let index = make_index(vec![
            make_file_with_classification(
                "docs/readme.md",
                "needle in docs\n",
                Vec::new(),
                text_classification,
            ),
            make_file("src/lib.rs", "needle in code\n", Vec::new()),
        ]);
        let options = TextSearchOptions {
            path_scope: PathScope::prefix("docs/"),
            search_scope: SearchScope::Text,
            ..Default::default()
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search should work");

        assert_eq!(result.total_matches, 1);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "docs/readme.md");
        assert_eq!(result.files[0].matches[0].line, "needle in docs");
    }

    #[test]
    fn test_path_scope_prefix_requires_directory_boundary() {
        assert!(PathScope::prefix("src").matches("src/lib.rs"));
        assert!(PathScope::prefix("src").matches("src"));
        assert!(!PathScope::prefix("src").matches("srcgen/lib.rs"));
    }

    #[test]
    fn test_current_code_symbol_search_options_are_explicit() {
        let options = SymbolSearchOptions::for_current_code_search(17);

        assert_eq!(options.path_scope, PathScope::Any);
        assert_eq!(options.search_scope, SearchScope::Code);
        assert_eq!(options.result_limit, ResultLimit::new(17));
        assert_eq!(
            options.noise_policy,
            NoisePolicy {
                include_generated: false,
                include_tests: false,
                include_vendor: true,
                include_ignored: false,
            }
        );
        assert_eq!(options.language_filter, None);
    }

    #[test]
    fn test_current_code_text_search_options_are_explicit() {
        let options = TextSearchOptions::for_current_code_search();

        assert_eq!(options.path_scope, PathScope::Any);
        assert_eq!(options.search_scope, SearchScope::Code);
        assert_eq!(
            options.noise_policy,
            NoisePolicy {
                include_generated: false,
                include_tests: false,
                include_vendor: true,
                include_ignored: false,
            }
        );
        assert_eq!(options.total_limit, 50);
        assert_eq!(options.max_per_file, 5);
        assert_eq!(options.language_filter, None);
        assert_eq!(options.glob, None);
        assert_eq!(options.exclude_glob, None);
        assert_eq!(options.context, None);
        assert_eq!(options.case_sensitive, None);
        assert!(!options.whole_word);
    }

    #[test]
    fn test_search_module_text_search_with_options_respects_language_noise_and_caps() {
        let (path_a, mut file_a) = make_file(
            "src/app.ts",
            "needle one\nneedle two\nneedle three\n",
            Vec::new(),
        );
        file_a.language = LanguageId::TypeScript;

        let (path_b, mut file_b) =
            make_file("src/lib.ts", "needle four\nneedle five\n", Vec::new());
        file_b.language = LanguageId::TypeScript;

        let (path_c, file_c) = make_file(
            "tests/generated/noisy.ts",
            "needle hidden\nneedle hidden two\n",
            Vec::new(),
        );

        let (path_d, file_d) =
            make_file("src/lib.rs", "needle rust\nneedle rust two\n", Vec::new());

        let index = make_index(vec![
            (path_a, file_a),
            (path_b, file_b),
            (path_c, file_c),
            (path_d, file_d),
        ]);
        let options = TextSearchOptions {
            path_scope: PathScope::prefix("src"),
            search_scope: SearchScope::Code,
            noise_policy: NoisePolicy {
                include_generated: false,
                include_tests: false,
                include_vendor: true,
                include_ignored: false,
            },
            include_personal_tooling: true,
            language_filter: Some(LanguageId::TypeScript),
            total_limit: 3,
            max_per_file: 2,
            glob: None,
            exclude_glob: None,
            context: None,
            case_sensitive: None,
            whole_word: false,
            ranked: false,
            churn_scores: None,
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search should work");

        assert_eq!(result.total_matches, 3);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].path, "src/app.ts");
        assert_eq!(result.files[0].matches.len(), 2);
        assert_eq!(result.files[1].path, "src/lib.ts");
        assert_eq!(result.files[1].matches.len(), 1);
    }

    #[test]
    fn test_search_module_text_search_filters_obsidian_but_keeps_wiki_markdown() {
        let index = make_index(vec![
            make_file("wiki/notes.md", "needle visible\n", Vec::new()),
            make_file(
                "wiki/.obsidian/workspace.json",
                "needle hidden workspace\n",
                Vec::new(),
            ),
            make_file(
                ".obsidian/plugins/dataview/styles.css",
                "needle hidden style\n",
                Vec::new(),
            ),
        ]);
        let options = TextSearchOptions {
            include_personal_tooling: false,
            total_limit: 10,
            max_per_file: 5,
            ..TextSearchOptions::default()
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search should work");

        assert_eq!(result.total_matches, 1);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "wiki/notes.md");
        assert_eq!(result.suppressed_by_noise, 2);
    }

    #[test]
    fn test_search_module_text_search_with_options_respects_glob_filters() {
        let index = make_index(vec![
            make_file("src/app.ts", "needle app\n", Vec::new()),
            make_file("src/app.spec.ts", "needle spec\n", Vec::new()),
            make_file("src/nested/feature.ts", "needle nested\n", Vec::new()),
            make_file("src/lib.rs", "needle rust\n", Vec::new()),
        ]);
        let options = TextSearchOptions {
            path_scope: PathScope::prefix("src"),
            search_scope: SearchScope::Code,
            noise_policy: NoisePolicy {
                include_generated: false,
                include_tests: false,
                include_vendor: true,
                include_ignored: false,
            },
            include_personal_tooling: true,
            language_filter: None,
            total_limit: 10,
            max_per_file: 5,
            glob: Some("src/**/*.ts".to_string()),
            exclude_glob: Some("**/*.spec.ts".to_string()),
            context: None,
            case_sensitive: None,
            whole_word: false,
            ranked: false,
            churn_scores: None,
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search should work");

        assert_eq!(result.total_matches, 2);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].path, "src/app.ts");
        assert_eq!(result.files[1].path, "src/nested/feature.ts");
    }

    #[test]
    fn test_search_module_text_search_invalid_glob_returns_error() {
        let index = make_index(vec![make_file("src/app.ts", "needle app\n", Vec::new())]);
        let options = TextSearchOptions {
            glob: Some("[".to_string()),
            ..TextSearchOptions::for_current_code_search()
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options);

        assert!(matches!(result, Err(TextSearchError::InvalidGlob { .. })));
    }

    #[test]
    fn test_search_module_text_search_with_context_merges_windows_and_marks_matches() {
        let index = make_index(vec![make_file(
            "src/lib.rs",
            "line 1\nline 2\nneedle 3\nline 4\nneedle 5\nline 6\nline 7\nline 8\nneedle 9\nline 10\n",
            Vec::new(),
        )]);
        let options = TextSearchOptions {
            context: Some(1),
            ..TextSearchOptions::for_current_code_search()
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search should work");

        assert_eq!(result.total_matches, 3);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].matches.len(), 3);

        let rendered = result.files[0]
            .rendered_lines
            .as_ref()
            .expect("context mode should materialize rendered lines");

        assert_eq!(rendered.len(), 9, "expected merged windows plus separator");
        assert!(matches!(
            rendered[0],
            TextDisplayLine::Line(TextRenderedLine {
                line_number: 2,
                is_match: false,
                ..
            })
        ));
        assert!(matches!(
            rendered[1],
            TextDisplayLine::Line(TextRenderedLine {
                line_number: 3,
                is_match: true,
                ..
            })
        ));
        assert!(matches!(rendered[5], TextDisplayLine::Separator));
        assert!(matches!(
            rendered[7],
            TextDisplayLine::Line(TextRenderedLine {
                line_number: 9,
                is_match: true,
                ..
            })
        ));
    }

    #[test]
    fn test_search_module_text_search_with_options_respects_case_sensitive_literal_matching() {
        let index = make_index(vec![make_file(
            "src/lib.rs",
            "Needle exact\nneedle lower\n",
            Vec::new(),
        )]);

        let sensitive = search_text_with_options(
            &index,
            Some("Needle"),
            None,
            false,
            &TextSearchOptions {
                case_sensitive: Some(true),
                ..TextSearchOptions::for_current_code_search()
            },
        )
        .expect("case-sensitive search should work");

        assert_eq!(sensitive.total_matches, 1);
        assert_eq!(sensitive.files[0].matches[0].line_number, 1);

        let insensitive = search_text_with_options(
            &index,
            Some("Needle"),
            None,
            false,
            &TextSearchOptions::for_current_code_search(),
        )
        .expect("default literal search should work");

        assert_eq!(insensitive.total_matches, 2);
    }

    #[test]
    fn test_search_module_text_search_with_options_respects_whole_word_boundaries() {
        let index = make_index(vec![make_file(
            "src/lib.rs",
            "needle\nneedle_case\nprefixneedle\nneedle suffix\n",
            Vec::new(),
        )]);
        let result = search_text_with_options(
            &index,
            Some("needle"),
            None,
            false,
            &TextSearchOptions {
                whole_word: true,
                ..TextSearchOptions::for_current_code_search()
            },
        )
        .expect("whole-word search should work");

        assert_eq!(result.total_matches, 2);
        assert_eq!(result.files[0].matches[0].line_number, 1);
        assert_eq!(result.files[0].matches[1].line_number, 4);
    }

    #[test]
    fn test_search_module_text_search_with_options_respects_whole_word_terms() {
        let index = make_index(vec![make_file(
            "src/lib.rs",
            "TODO\nTODO_NOTE\nFIXME!\n",
            Vec::new(),
        )]);
        let terms = vec!["todo".to_string(), "fixme".to_string()];
        let result = search_text_with_options(
            &index,
            None,
            Some(&terms),
            false,
            &TextSearchOptions {
                whole_word: true,
                ..TextSearchOptions::for_current_code_search()
            },
        )
        .expect("whole-word multi-term search should work");

        assert_eq!(result.total_matches, 2);
        assert_eq!(result.files[0].matches[0].line, "TODO");
        assert_eq!(result.files[0].matches[1].line, "FIXME!");
    }

    #[test]
    fn test_search_module_text_search_regex_can_opt_into_case_insensitive_matching() {
        let index = make_index(vec![make_file("src/lib.rs", "needle lower\n", Vec::new())]);
        let result = search_text_with_options(
            &index,
            Some("Needle"),
            None,
            true,
            &TextSearchOptions {
                case_sensitive: Some(false),
                ..TextSearchOptions::for_current_code_search()
            },
        )
        .expect("case-insensitive regex search should work");

        assert_eq!(result.total_matches, 1);
        assert_eq!(result.files[0].matches[0].line, "needle lower");
    }

    #[test]
    fn test_search_module_text_search_rejects_regex_whole_word_combination() {
        let index = make_index(vec![make_file("src/lib.rs", "needle lower\n", Vec::new())]);
        let result = search_text_with_options(
            &index,
            Some("needle"),
            None,
            true,
            &TextSearchOptions {
                whole_word: true,
                ..TextSearchOptions::for_current_code_search()
            },
        );

        assert!(matches!(
            result,
            Err(TextSearchError::UnsupportedWholeWordRegex)
        ));
    }

    #[test]
    fn test_explicit_path_read_options_are_exact() {
        let options = FileContentOptions::for_explicit_path_read("src/lib.rs", Some(2), Some(4));

        assert_eq!(
            options.path_scope,
            PathScope::Exact("src/lib.rs".to_string())
        );
        assert_eq!(
            options.content_context,
            ContentContext::line_range(Some(2), Some(4))
        );
    }

    #[test]
    fn test_explicit_path_read_options_preserve_format_flags() {
        let options = FileContentOptions::for_explicit_path_read_with_format(
            "src/lib.rs",
            Some(2),
            Some(4),
            true,
            true,
        );

        assert_eq!(
            options.path_scope,
            PathScope::Exact("src/lib.rs".to_string())
        );
        assert_eq!(
            options.content_context,
            ContentContext::line_range_with_format(Some(2), Some(4), true, true)
        );
    }

    #[test]
    fn test_explicit_path_read_around_line_options_are_exact() {
        let options = FileContentOptions::for_explicit_path_read_around_line(
            "src/lib.rs",
            3,
            Some(1),
            false,
            false,
        );

        assert_eq!(
            options.path_scope,
            PathScope::Exact("src/lib.rs".to_string())
        );
        assert_eq!(
            options.content_context,
            ContentContext::around_line(3, Some(1), false, false)
        );
    }

    #[test]
    fn test_explicit_path_read_around_match_options_are_exact() {
        let options = FileContentOptions::for_explicit_path_read_around_match(
            "src/lib.rs",
            "needle",
            None,
            Some(1),
            false,
            false,
        );

        assert_eq!(
            options.path_scope,
            PathScope::Exact("src/lib.rs".to_string())
        );
        assert_eq!(
            options.content_context,
            ContentContext::around_match("needle", Some(1), false, false)
        );
    }

    #[test]
    fn test_explicit_path_read_chunk_options_are_exact() {
        let options = FileContentOptions::for_explicit_path_read_chunk("src/lib.rs", 2, 2);

        assert_eq!(
            options.path_scope,
            PathScope::Exact("src/lib.rs".to_string())
        );
        assert_eq!(options.content_context, ContentContext::chunk(2, 2));
    }

    #[test]
    fn test_explicit_path_read_around_symbol_options_are_exact() {
        let options = FileContentOptions::for_explicit_path_read_around_symbol(
            "src/lib.rs",
            "connect",
            Some(3),
            Some(1),
            None,
            false,
            false,
        );

        assert_eq!(
            options.path_scope,
            PathScope::Exact("src/lib.rs".to_string())
        );
        assert_eq!(
            options.content_context,
            ContentContext::around_symbol("connect", Some(3), Some(1))
        );
    }

    // ── NoiseClass + NoisePolicy tests ──────────────────────────────────

    #[test]
    fn test_noise_policy_classifies_vendor_paths() {
        assert_eq!(
            NoisePolicy::classify_path("vendor/github.com/pkg/errors/errors.go", None),
            NoiseClass::Vendor
        );
        assert_eq!(
            NoisePolicy::classify_path("node_modules/lodash/index.js", None),
            NoiseClass::Vendor
        );
        assert_eq!(
            NoisePolicy::classify_path("third_party/protobuf/src/lib.rs", None),
            NoiseClass::Vendor
        );
        assert_eq!(
            NoisePolicy::classify_path(".venv/lib/site-packages/flask/app.py", None),
            NoiseClass::Vendor
        );
        assert_eq!(
            NoisePolicy::classify_path("bower_components/jquery/jquery.js", None),
            NoiseClass::Vendor
        );
    }

    #[test]
    fn test_noise_policy_classifies_generated_paths() {
        assert_eq!(
            NoisePolicy::classify_path("Cargo.lock", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("package-lock.json", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("assets/app.min.js", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("styles/theme.min.css", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("dist/bundle.js", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("src/generated/api.rs", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("proto/service.pb.go", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("lib/model.g.dart", None),
            NoiseClass::Generated
        );
        assert_eq!(
            NoisePolicy::classify_path("Forms/Form1.Designer.cs", None),
            NoiseClass::Generated
        );
    }

    /// SF-STRESS-011: the broadened vendor/generated/test segment sets must be
    /// recognized, and `NoiseClass::classify_path` must AGREE with
    /// `FileClassification::for_code_path` on the same paths (they share the
    /// segment-set source of truth) so search and explore no longer disagree.
    #[test]
    fn test_noise_classifiers_agree_on_corpus_paths() {
        use crate::domain::FileClassification;

        // deps/ vendored dependency (redis, node) — previously an undocumented gap.
        assert_eq!(
            NoisePolicy::classify_path("deps/hiredis/sds.c", None),
            NoiseClass::Vendor
        );
        assert!(FileClassification::for_code_path("deps/hiredis/sds.c").is_vendor);

        // dist/ build output (laravel) — now shared by both classifiers.
        assert_eq!(
            NoisePolicy::classify_path("dist/css/app.css", None),
            NoiseClass::Generated
        );
        assert!(FileClassification::for_code_path("dist/css/app.css").is_generated);

        // .min.css / .map generated assets (mojo bootstrap.css, source maps).
        assert_eq!(
            NoisePolicy::classify_path("public/bootstrap.min.css", None),
            NoiseClass::Generated
        );
        assert!(FileClassification::for_code_path("public/bootstrap.min.css").is_generated);
        assert!(FileClassification::for_code_path("dist/app.js.map").is_generated);

        // test_data / __snapshots__ fixtures (rust-analyzer) — FileClassification
        // is the test-aware classifier; classify_path does not model tests, so we
        // only assert FileClassification here.
        assert!(FileClassification::for_code_path("crates/parser/test_data/err/0001.rs").is_test);
        assert!(
            FileClassification::for_code_path("src/__snapshots__/Button.test.tsx.snap").is_test
        );
    }

    #[test]
    fn test_noise_policy_classifies_normal_paths() {
        assert_eq!(
            NoisePolicy::classify_path("src/main.rs", None),
            NoiseClass::None
        );
        assert_eq!(
            NoisePolicy::classify_path("lib/utils.ts", None),
            NoiseClass::None
        );
    }

    #[test]
    fn test_noise_policy_should_hide_respects_flags() {
        let restrictive = NoisePolicy::hide_classified_noise();
        assert!(!restrictive.should_hide(NoiseClass::None));
        assert!(restrictive.should_hide(NoiseClass::Vendor));
        assert!(restrictive.should_hide(NoiseClass::Generated));
        assert!(restrictive.should_hide(NoiseClass::Ignored));

        let permissive = NoisePolicy::permissive();
        assert!(!permissive.should_hide(NoiseClass::None));
        assert!(!permissive.should_hide(NoiseClass::Vendor));
        assert!(!permissive.should_hide(NoiseClass::Generated));
        assert!(!permissive.should_hide(NoiseClass::Ignored));

        // Custom: include vendor but not generated
        let custom = NoisePolicy {
            include_vendor: true,
            include_generated: false,
            include_tests: true,
            include_ignored: false,
        };
        assert!(!custom.should_hide(NoiseClass::None));
        assert!(!custom.should_hide(NoiseClass::Vendor));
        assert!(custom.should_hide(NoiseClass::Generated));
        assert!(custom.should_hide(NoiseClass::Ignored)); // Ignored follows include_ignored
    }

    #[test]
    fn test_noise_class_tag() {
        assert_eq!(NoiseClass::None.tag(), "");
        assert_eq!(NoiseClass::Vendor.tag(), "[vendor]");
        assert_eq!(NoiseClass::Generated.tag(), "[generated]");
        assert_eq!(NoiseClass::Ignored.tag(), "[ignored]");
    }

    #[test]
    fn test_gitignore_vendor_classified() {
        use ignore::gitignore::GitignoreBuilder;

        let tmp = tempfile::tempdir().unwrap();
        let mut builder = GitignoreBuilder::new(tmp.path());
        builder.add_line(None, "build/").unwrap();
        builder.add_line(None, "*.log").unwrap();
        let gi = builder.build().unwrap();

        // "build/output.js" matches gitignore but not vendor/generated heuristic → Ignored
        assert_eq!(
            NoisePolicy::classify_path("build/output.js", Some(&gi)),
            NoiseClass::Ignored
        );
        // "debug.log" matches gitignore → Ignored
        assert_eq!(
            NoisePolicy::classify_path("debug.log", Some(&gi)),
            NoiseClass::Ignored
        );
        // "src/main.rs" does NOT match gitignore → None
        assert_eq!(
            NoisePolicy::classify_path("src/main.rs", Some(&gi)),
            NoiseClass::None
        );
        // Vendor heuristic takes priority over gitignore
        assert_eq!(
            NoisePolicy::classify_path("node_modules/pkg/index.js", Some(&gi)),
            NoiseClass::Vendor
        );
    }

    #[test]
    fn test_gitignore_negation_exempts_file() {
        use ignore::gitignore::GitignoreBuilder;

        let tmp = tempfile::tempdir().unwrap();
        let mut builder = GitignoreBuilder::new(tmp.path());
        builder.add_line(None, "build/").unwrap();
        builder.add_line(None, "!build/important.js").unwrap();
        let gi = builder.build().unwrap();

        // Negated file should NOT be classified as noise
        assert_eq!(
            NoisePolicy::classify_path("build/important.js", Some(&gi)),
            NoiseClass::None
        );
        // Non-negated file in same dir should still be noise
        assert_eq!(
            NoisePolicy::classify_path("build/other.js", Some(&gi)),
            NoiseClass::Ignored
        );
    }

    #[test]
    fn test_symbol_kind_priority() {
        assert_eq!(symbol_kind_priority("function"), 1.0);
        assert_eq!(symbol_kind_priority("method"), 1.0);
        assert_eq!(symbol_kind_priority("async_function"), 1.0);
        assert_eq!(symbol_kind_priority("struct"), 0.8);
        assert_eq!(symbol_kind_priority("class"), 0.8);
        assert_eq!(symbol_kind_priority("enum"), 0.8);
        assert_eq!(symbol_kind_priority("trait"), 0.8);
        assert_eq!(symbol_kind_priority("impl"), 0.7);
        assert_eq!(symbol_kind_priority("module"), 0.5);
        assert_eq!(symbol_kind_priority("constant"), 0.4);
        assert_eq!(symbol_kind_priority("variable"), 0.3);
        assert_eq!(symbol_kind_priority("key"), 0.2);
        assert!(symbol_kind_priority("unknown") > 0.0);
        assert_eq!(symbol_kind_priority("unknown"), 0.1);
    }

    #[test]
    fn test_ranked_reorders_by_importance() {
        use crate::live_index::store::ReferenceLocation;

        // Create two TextFileMatches: one with many callers, one with few.
        let file_a = TextFileMatches {
            path: "a.rs".to_string(),
            matches: vec![TextLineMatch {
                line_number: 10,
                line: "let x = 1;".to_string(),
                enclosing_symbol: Some(EnclosingMatchSymbol {
                    name: "dead_code_fn".to_string(),
                    kind: "function".to_string(),
                    line_range: (1, 20),
                }),
            }],
            rendered_lines: None,
            callers: None,
        };

        let file_b = TextFileMatches {
            path: "b.rs".to_string(),
            matches: vec![TextLineMatch {
                line_number: 5,
                line: "let y = 2;".to_string(),
                enclosing_symbol: Some(EnclosingMatchSymbol {
                    name: "popular_fn".to_string(),
                    kind: "function".to_string(),
                    line_range: (1, 10),
                }),
            }],
            rendered_lines: None,
            callers: None,
        };

        // Build a reverse index where popular_fn has 20 callers, dead_code_fn has 0.
        let mut reverse_index: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();
        let refs: Vec<ReferenceLocation> = (0..20)
            .map(|i| ReferenceLocation {
                file_path: format!("caller_{}.rs", i),
                reference_idx: 0,
            })
            .collect();
        reverse_index.insert("popular_fn".to_string(), refs);
        // dead_code_fn has no entries in reverse_index.

        let match_count_max = 1;

        let score_a = compute_importance_score(&file_a, match_count_max, &reverse_index, 0.0);
        let score_b = compute_importance_score(&file_b, match_count_max, &reverse_index, 0.0);

        // file_b (popular_fn) should score higher than file_a (dead_code_fn)
        assert!(
            score_b > score_a,
            "Expected popular_fn score ({}) > dead_code_fn score ({})",
            score_b,
            score_a,
        );
    }

    #[test]
    fn test_compute_importance_score_with_churn() {
        use crate::live_index::store::ReferenceLocation;

        let file = TextFileMatches {
            path: "c.rs".to_string(),
            matches: vec![TextLineMatch {
                line_number: 1,
                line: "test".to_string(),
                enclosing_symbol: Some(EnclosingMatchSymbol {
                    name: "some_fn".to_string(),
                    kind: "function".to_string(),
                    line_range: (1, 5),
                }),
            }],
            rendered_lines: None,
            callers: None,
        };

        let reverse_index: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();

        let score_no_churn = compute_importance_score(&file, 1, &reverse_index, 0.0);
        let score_high_churn = compute_importance_score(&file, 1, &reverse_index, 1.0);

        // Higher churn should increase the score.
        assert!(
            score_high_churn > score_no_churn,
            "Expected high churn score ({}) > no churn score ({})",
            score_high_churn,
            score_no_churn,
        );
    }

    // ── Frecency fusion ────────────────────────────────────────────────────

    fn make_file_hit(path: &str, tier: SearchFilesTier) -> SearchFilesHit {
        SearchFilesHit {
            tier,
            path: path.to_string(),
            coupling_score: None,
            shared_commits: None,
            metadata_reason: None,
        }
    }

    #[test]
    fn tier_path_match_score_maps_tiers_to_expected_weights() {
        assert_eq!(tier_path_match_score(SearchFilesTier::StrongPath), 1.0);
        assert_eq!(tier_path_match_score(SearchFilesTier::Basename), 0.6);
        assert_eq!(tier_path_match_score(SearchFilesTier::LoosePath), 0.3);
        assert_eq!(tier_path_match_score(SearchFilesTier::CoChange), 0.0);
    }

    #[test]
    fn score_hits_by_frecency_fusion_returns_zero_frecency_when_store_empty() {
        let hits = vec![
            make_file_hit("src/a.rs", SearchFilesTier::StrongPath),
            make_file_hit("src/b.rs", SearchFilesTier::Basename),
        ];
        let scores: HashMap<PathBuf, f64> = HashMap::new();
        let breakdowns = score_hits_by_frecency_fusion(&hits, &scores);
        assert_eq!(breakdowns.len(), 2);
        for b in &breakdowns {
            assert_eq!(b.frecency_normalized, 0.0);
        }
        // combined = 0.6 * path_match (co_change and frecency both 0)
        assert!((breakdowns[0].combined - 0.6).abs() < f64::EPSILON);
        assert!((breakdowns[1].combined - 0.36).abs() < f64::EPSILON);
    }

    #[test]
    fn score_hits_by_frecency_fusion_normalizes_against_max_in_set() {
        let hits = vec![
            make_file_hit("src/peak.rs", SearchFilesTier::LoosePath),
            make_file_hit("src/mid.rs", SearchFilesTier::LoosePath),
            make_file_hit("src/cold.rs", SearchFilesTier::LoosePath),
        ];
        let mut scores = HashMap::new();
        scores.insert(PathBuf::from("src/peak.rs"), 10.0);
        scores.insert(PathBuf::from("src/mid.rs"), 2.5);
        // "src/cold.rs" intentionally missing — treated as 0.
        let breakdowns = score_hits_by_frecency_fusion(&hits, &scores);
        assert!((breakdowns[0].frecency_normalized - 1.0).abs() < f64::EPSILON);
        assert!((breakdowns[1].frecency_normalized - 0.25).abs() < f64::EPSILON);
        assert_eq!(breakdowns[2].frecency_normalized, 0.0);
    }

    #[test]
    fn score_hits_by_frecency_fusion_applies_weights_060_030_010() {
        // Single candidate with a full-score path_match, nonzero co_change,
        // and the only frecency row (→ normalized == 1.0). Final combined
        // must equal 0.6 + 0.3 * 0.5 + 0.1 = 0.85.
        let mut hit = make_file_hit("src/star.rs", SearchFilesTier::StrongPath);
        hit.coupling_score = Some(0.5);
        let hits = vec![hit];
        let mut scores = HashMap::new();
        scores.insert(PathBuf::from("src/star.rs"), 1.0);
        let breakdowns = score_hits_by_frecency_fusion(&hits, &scores);
        let b = breakdowns[0];
        assert!((b.path_match - 1.0).abs() < f64::EPSILON);
        assert!((b.co_change - 0.5).abs() < f64::EPSILON);
        assert!((b.frecency_normalized - 1.0).abs() < f64::EPSILON);
        assert!((b.combined - (0.6 + 0.15 + 0.1)).abs() < f64::EPSILON);
    }

    #[test]
    fn reorder_hits_by_frecency_fusion_sorts_by_combined_descending() {
        let hits = vec![
            make_file_hit("src/c.rs", SearchFilesTier::LoosePath),
            make_file_hit("src/a.rs", SearchFilesTier::LoosePath),
            make_file_hit("src/b.rs", SearchFilesTier::LoosePath),
        ];
        // All three tied on path_match (LoosePath → 0.3). Give each a
        // different raw frecency so the normalized contribution breaks
        // ties and re-orders the set.
        let mut scores = HashMap::new();
        scores.insert(PathBuf::from("src/c.rs"), 1.0);
        scores.insert(PathBuf::from("src/a.rs"), 10.0);
        scores.insert(PathBuf::from("src/b.rs"), 5.0);
        let breakdowns = score_hits_by_frecency_fusion(&hits, &scores);
        let reordered = reorder_hits_by_frecency_fusion(hits, &breakdowns);
        assert_eq!(reordered[0].path, "src/a.rs");
        assert_eq!(reordered[1].path, "src/b.rs");
        assert_eq!(reordered[2].path, "src/c.rs");
    }

    #[test]
    fn reorder_hits_by_frecency_fusion_breaks_combined_ties_by_path() {
        // Two hits with identical tier and no frecency → identical combined.
        // Tiebreak must be stable-alphabetical by path for determinism.
        let hits = vec![
            make_file_hit("src/z.rs", SearchFilesTier::Basename),
            make_file_hit("src/a.rs", SearchFilesTier::Basename),
        ];
        let scores: HashMap<PathBuf, f64> = HashMap::new();
        let breakdowns = score_hits_by_frecency_fusion(&hits, &scores);
        let reordered = reorder_hits_by_frecency_fusion(hits, &breakdowns);
        assert_eq!(reordered[0].path, "src/a.rs");
        assert_eq!(reordered[1].path, "src/z.rs");
    }

    #[test]
    fn recent_single_bump_outranks_old_ten_bumps_via_fusion_math() {
        // Spec fixture: "File touched 5 min ago outranks file touched 6
        // months ago with 10× hits". Pinned here at the fusion-math layer
        // (no DB involved) so the math stays correct even before the
        // end-to-end integration test is wired.
        //
        // Decay uses 7-day half-life. 5 minutes is negligible decay
        // → score_b ≈ 1.0. 6 months ≈ 26 half-lives → score_a ≈ 10 * 2^-26
        // ≈ 1.5e-7. After normalizing against max (== score_b), file_a's
        // contribution is essentially 0 and file_b's is 1.0.
        let hits = vec![
            make_file_hit("src/file_a.rs", SearchFilesTier::LoosePath),
            make_file_hit("src/file_b.rs", SearchFilesTier::LoosePath),
        ];
        let mut scores = HashMap::new();
        scores.insert(PathBuf::from("src/file_a.rs"), 1.5e-7);
        scores.insert(PathBuf::from("src/file_b.rs"), 1.0);
        let breakdowns = score_hits_by_frecency_fusion(&hits, &scores);
        let reordered = reorder_hits_by_frecency_fusion(hits, &breakdowns);
        assert_eq!(
            reordered[0].path, "src/file_b.rs",
            "recent single bump must outrank old 10x bumps"
        );
    }

    /// Regression: Unit 5 originally bundled "every file was an
    /// unsupported config language" with "every file rejected the pattern
    /// syntax." The TOML-only case below proves the former path: we must
    /// get `UnsupportedStructuralLanguage`, not a generic "invalid pattern"
    /// error that would mislead the caller into thinking their pattern is
    /// broken.
    #[test]
    fn search_structural_surfaces_unsupported_language_when_only_config_files_indexed() {
        let toml_content = "[package]\nname = \"thing\"\n";
        let file = IndexedFile {
            relative_path: "Cargo.toml".to_string(),
            language: LanguageId::Toml,
            classification: crate::domain::FileClassification::for_code_path("Cargo.toml"),
            content: toml_content.as_bytes().to_vec(),
            symbols: Vec::new(),
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: toml_content.len() as u64,
            content_hash: "hash".to_string(),
            references: Vec::new(),
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let index = make_index(vec![("Cargo.toml".to_string(), file)]);

        let options = TextSearchOptions::default();
        let pattern = "fn $NAME($$$) { $$$ }";
        let result = search_structural(&index, pattern, &options);

        match result {
            Err(TextSearchError::UnsupportedStructuralLanguage {
                pattern: err_pattern,
                sample_error,
            }) => {
                assert_eq!(err_pattern, pattern);
                assert!(
                    sample_error.contains("not supported"),
                    "sample error must quote the underlying language rejection; got: {sample_error}"
                );
            }
            Err(TextSearchError::InvalidStructuralPattern { .. }) => panic!(
                "regression: TOML-only index must not masquerade as an invalid-pattern error"
            ),
            Err(other) => panic!("expected UnsupportedStructuralLanguage, got {other:?}"),
            Ok(result) => panic!(
                "expected error propagation, got Ok with {} match(es) in {} file(s)",
                result.total_matches,
                result.files.len()
            ),
        }
    }

    /// Regression: with a mixed index (a supported language plus a config
    /// language), a pattern that `Pattern::try_new` rejects as syntactically
    /// invalid must surface as `InvalidStructuralPattern`, not masked behind
    /// the config-language error that would otherwise populate
    /// `first_unsupported_error`. Syntax errors win.
    #[test]
    fn search_structural_prefers_syntax_error_over_unsupported_language() {
        // Rust file — supported by ast-grep.
        let rust_content = "fn main() { println!(\"hi\"); }\n";
        let rust_file = IndexedFile {
            relative_path: "src/a.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/a.rs"),
            content: rust_content.as_bytes().to_vec(),
            symbols: Vec::new(),
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: rust_content.len() as u64,
            content_hash: "hash".to_string(),
            references: Vec::new(),
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        // TOML file — sorts before src/a.rs, so without syntax-wins logic
        // its "not supported" error would populate first_*_error first.
        let toml_content = "[package]\nname = \"x\"\n";
        let toml_file = IndexedFile {
            relative_path: "Cargo.toml".to_string(),
            language: LanguageId::Toml,
            classification: crate::domain::FileClassification::for_code_path("Cargo.toml"),
            content: toml_content.as_bytes().to_vec(),
            symbols: Vec::new(),
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: toml_content.len() as u64,
            content_hash: "hash".to_string(),
            references: Vec::new(),
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let index = make_index(vec![
            ("Cargo.toml".to_string(), toml_file),
            ("src/a.rs".to_string(), rust_file),
        ]);

        // Empty pattern is the most portable "definitely invalid" input we
        // can pass to ast-grep without depending on grammar-specific quirks.
        let options = TextSearchOptions::default();
        let bad_pattern = "";
        let result = search_structural(&index, bad_pattern, &options);

        match result {
            Err(TextSearchError::InvalidStructuralPattern { .. }) => {
                // Correct — syntax error propagated despite the TOML file
                // contributing an earlier "unsupported" error.
            }
            Err(TextSearchError::UnsupportedStructuralLanguage { .. }) => {
                // If ast-grep happens to accept the empty pattern against
                // Rust and returns zero matches, we'd hit Ok below, not
                // this branch — so reaching here would mean we regressed to
                // the old conflation bug.
                panic!(
                    "regression: syntax-error pattern must not be masked by the TOML \
                     file's unsupported-language error"
                );
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
            Ok(_) => {
                // ast-grep accepted the empty pattern; skip silently. The
                // intent of this test only holds when Pattern::try_new
                // rejects the chosen "bad_pattern". If ast-grep is more
                // lenient than assumed, this test simply does not exercise
                // the precedence path — the TOML-only test still proves
                // UnsupportedStructuralLanguage is wired.
            }
        }
    }

    #[test]
    fn search_structural_zero_match_result_surfaces_structural_label() {
        // Valid pattern, but nothing matches in the indexed content.
        // Result should be Ok with empty files, and label must identify it
        // as structural so the renderer can give a structural-aware hint.
        let (path, file) = make_file(
            "src/a.rs",
            "fn main() { println!(\"hi\"); }\n",
            vec![make_symbol("main", SymbolKind::Function, 1)],
        );
        let index = make_index(vec![(path, file)]);

        let options = TextSearchOptions::default();
        // Rust-specific pattern that won't match the `fn main` body above.
        let result = search_structural(&index, "struct $NAME { $$$ }", &options)
            .expect("valid pattern must not error");
        assert_eq!(result.total_matches, 0);
        assert!(result.files.is_empty());
        assert!(
            result.label.starts_with("structural "),
            "label must start with `structural ` so the renderer can detect structural searches; got: {}",
            result.label
        );
    }

    #[test]
    fn search_structural_reuses_compiled_pattern_per_language_per_request() {
        let pattern = "fn $NAME($$$) { $$$ }";
        let index = make_index(vec![
            make_file(
                "src/a.rs",
                "fn alpha() {}\n",
                vec![make_symbol("alpha", SymbolKind::Function, 1)],
            ),
            make_file(
                "src/b.rs",
                "fn beta() {}\n",
                vec![make_symbol("beta", SymbolKind::Function, 1)],
            ),
            make_file(
                "src/c.rs",
                "fn gamma() {}\n",
                vec![make_symbol("gamma", SymbolKind::Function, 1)],
            ),
        ]);

        let mut compile_calls_by_language: HashMap<LanguageId, usize> = HashMap::new();
        let result = search_structural_with_compiler(
            &index,
            pattern,
            &TextSearchOptions::default(),
            |pattern, lang, is_tsx| {
                *compile_calls_by_language.entry(lang.clone()).or_default() += 1;
                crate::parsing::ast_grep::compile_structural_pattern(pattern, lang, is_tsx)
            },
        )
        .expect("valid structural pattern should search all candidate files");

        assert_eq!(result.total_matches, 3);
        assert_eq!(compile_calls_by_language.get(&LanguageId::Rust), Some(&1));
    }
}
