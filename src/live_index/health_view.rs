use std::time::{Duration, SystemTime};

use crate::domain::LanguageId;
use crate::domain::index::{AdmissionTier, SkipReason};
use crate::watcher_state::{WatcherInfo, WatcherState};

use super::query::normalize_path_query;
use super::search::{NoiseClass, NoisePolicy};
use super::store::{IndexState, IndexedFile, LiveIndex, ParseStatus};
pub struct HealthStats {
    pub file_count: usize,
    pub symbol_count: usize,
    pub parsed_count: usize,
    pub partial_parse_count: usize,
    /// Partial parses that are not explicitly classified as expected vendor noise.
    pub unexpected_partial_parse_count: usize,
    /// Expected partial parses from the vendored tree-sitter-scss C/header parser source.
    pub expected_vendor_partial_parse_count: usize,
    /// Expected partial parses from framework template syntax that the host
    /// tree-sitter grammar cannot model (SF-004: Angular `@if`/`@for`/... in
    /// `.html`, which tree-sitter-html 0.23.2 has no rules for).
    pub expected_framework_partial_parse_count: usize,
    /// Expected partial parses from a known host-language grammar limitation
    /// (SF-003: `import('mod').Member[]` import-type arrays that
    /// tree-sitter-typescript 0.23.2 mis-parses). These are valid source, not
    /// repo-owned defects, so they are bucketed separately from unexpected
    /// partials — but they MUST still be accounted for so the quarantine
    /// registry total matches the header partial count.
    pub expected_language_partial_parse_count: usize,
    pub failed_count: usize,
    pub load_duration: Duration,
    /// Current state of the file watcher.
    pub watcher_state: WatcherState,
    /// Total number of file-system events processed by the watcher.
    pub events_processed: u64,
    /// Wall-clock time of the most recent event processed, if any.
    pub last_event_at: Option<SystemTime>,
    /// Effective debounce window in milliseconds.
    pub debounce_window_ms: u64,
    /// Number of watcher overflow/reconciliation triggers observed.
    pub overflow_count: u64,
    /// Wall-clock time of the most recent overflow event.
    pub last_overflow_at: Option<SystemTime>,
    /// Total stale files refreshed by reconciliation sweeps.
    pub stale_files_found: u64,
    /// Wall-clock time of the most recent reconciliation sweep.
    pub last_reconcile_at: Option<SystemTime>,
    /// Sorted, deduplicated list of files with partial-parse status.
    pub partial_parse_files: Vec<String>,
    /// Sorted, deduplicated list of unexpected partial-parse files.
    pub unexpected_partial_parse_files: Vec<String>,
    /// Sorted, deduplicated list of expected vendored partial-parse files.
    pub expected_vendor_partial_parse_files: Vec<String>,
    /// Sorted, deduplicated list of expected framework template partial-parse files.
    pub expected_framework_partial_parse_files: Vec<String>,
    /// Sorted, deduplicated list of expected host-language-limitation partial-parse
    /// files (SF-003 TypeScript import-type arrays).
    pub expected_language_partial_parse_files: Vec<String>,
    /// Sorted, deduplicated list of files with failed parse status and their error messages.
    pub failed_files: Vec<(String, String)>,
    /// Admission tier counts: (Tier1 indexed, Tier2 metadata-only, Tier3 hard-skipped).
    pub tier_counts: (usize, usize, usize),
    /// Reason the index is empty at startup (e.g. no safe root, auto-index off).
    /// Surfaced as a banner in `health` output so MCP clients see why no symbols loaded.
    pub local_empty_reason: Option<String>,
    /// SF-009: number of Tier-1 (recognized-extension, symbol-bearing) indexed
    /// files that are NOT git-tracked AND NOT gitignored. Surfaced so a user can
    /// SEE when the index holds non-version-controlled scratch source without
    /// changing what gets admitted. FAILS OPEN to `0` when there is no git
    /// repository / no readable index (so a non-git tempdir does not report
    /// every file as untracked). Computed via the git index (`git ls-files`
    /// semantics), NOT the `ignore` crate, which has no tracked-files concept.
    pub untracked_indexed: usize,
}

pub const EXPECTED_VENDOR_PARTIAL_PARSE_REASON: &str =
    "expected vendor: tree-sitter-scss C/header parser limitation";

pub const EXPECTED_FRAMEWORK_PARTIAL_PARSE_REASON: &str = "expected framework: Angular template control-flow not supported by tree-sitter-html; \
symbols extracted best-effort";

pub const EXPECTED_LANGUAGE_PARTIAL_PARSE_REASON: &str = "expected language: TypeScript import-type array (import('mod').Member[]) not supported by \
tree-sitter-typescript 0.23.2; symbols extracted best-effort";

/// Angular control-flow block keywords (`@if`/`@for`/`@switch`/`@defer`).
///
/// `scan_angular_text` emits these verbatim as the symbol name, so matching on
/// the extracted symbol name is precise — only an Angular control-flow construct
/// produces a symbol with one of these names. `@let` is excluded here because the
/// scanner emits the binding's *variable name* (not the literal `@let`) and a
/// `@let` declaration has no relational `>` operator, so it is not the parse
/// trigger this bucket excuses.
const ANGULAR_CONTROL_FLOW_KEYWORDS: [&str; 4] = ["@if", "@for", "@switch", "@defer"];

/// SF-004: recognize a partial parse whose ONLY cause is Angular template
/// control-flow (`@if (a > b) {`, `@for`, `@switch`, `@defer`, `@else if`) in a
/// `.html` file. `tree-sitter-html 0.23.2` has zero Angular rules; the `<`/`>`
/// relational operator inside the control expression is lexed as a tag delimiter,
/// producing an ERROR node even though SymForge text-scans the construct and still
/// extracts symbols.
///
/// Soundness — this delegates to
/// [`crate::parsing::is_expected_angular_template_control_flow_limitation`], which
/// validates the WHOLE file via neutralize-and-reparse (the proven SF-003
/// pattern), NOT a single diagnostic line. The cheap pre-gate here keeps the
/// expensive re-parse off the hot path:
///   1. the file is HTML and a partial parse, AND
///   2. at least one extracted symbol is an Angular control-flow block (named
///      `@if`/`@for`/`@switch`/`@defer`; these come ONLY from `scan_angular_text`).
///
/// Only when the pre-gate passes do we re-parse: the file is excused iff
/// neutralizing the relational operators inside the Angular openers makes the
/// whole file parse completely clean. Any unrelated defect (unclosed `<div>`,
/// stray `</div>`/erroneous_end_tag, broken attribute anywhere) keeps the
/// transformed parse dirty, so the real defect is never masked.
pub(crate) fn is_expected_framework_partial_parse(file: &IndexedFile) -> bool {
    use crate::domain::SymbolKind;

    if !matches!(file.language, LanguageId::Html) {
        return false;
    }
    if !matches!(file.parse_status, ParseStatus::PartialParse { .. }) {
        return false;
    }

    let has_angular_control_flow_symbol = file.symbols.iter().any(|s| {
        s.kind == SymbolKind::Module && ANGULAR_CONTROL_FLOW_KEYWORDS.contains(&s.name.as_str())
    });
    if !has_angular_control_flow_symbol {
        return false;
    }

    // Sound confirmation: neutralize ONLY the Angular control-flow relational
    // operators and re-parse the WHOLE file. A clean re-parse proves those
    // operators were the sole cause; any unrelated defect keeps it dirty.
    crate::parsing::is_expected_angular_template_control_flow_limitation(
        &file.language,
        &file.content,
    )
}

fn is_expected_vendor_partial_parse_noise(
    path: &str,
    file: &IndexedFile,
    gitignore: Option<&ignore::gitignore::Gitignore>,
) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let is_tree_sitter_scss_c_or_header = normalized.starts_with("vendor/tree-sitter-scss/src/")
        && (normalized.ends_with(".c") || normalized.ends_with(".h"));

    is_tree_sitter_scss_c_or_header
        && file.classification.is_vendor
        && matches!(file.language, LanguageId::C | LanguageId::Cpp)
        && matches!(
            NoisePolicy::classify_path(path, gitignore),
            NoiseClass::Vendor
        )
}

/// Owned per-path admission-tier lookup result for protocol handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionTierLookupView {
    pub tier: AdmissionTier,
    pub path: String,
    pub size: Option<u64>,
    pub extension: Option<String>,
    pub language: Option<LanguageId>,
    pub reason: Option<SkipReason>,
}

impl LiveIndex {
    /// Capture per-path admission-tier metadata without changing tool response behavior.
    pub fn capture_admission_tier_lookup_view(
        &self,
        relative_path: &str,
    ) -> Option<AdmissionTierLookupView> {
        let path = normalize_path_query(relative_path);
        if let Some(file) = self.files.get(&path) {
            return Some(AdmissionTierLookupView {
                tier: AdmissionTier::Normal,
                path: file.relative_path.clone(),
                size: Some(file.byte_len),
                extension: None,
                language: Some(file.language.clone()),
                reason: None,
            });
        }

        self.skipped_files
            .iter()
            .find(|skipped| normalize_path_query(&skipped.path) == path)
            .map(|skipped| AdmissionTierLookupView {
                tier: skipped.tier(),
                path: skipped.path.clone(),
                size: Some(skipped.size),
                extension: skipped.extension.clone(),
                language: None,
                reason: skipped.reason(),
            })
    }

    /// Number of indexed files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total symbols across all indexed files.
    pub fn symbol_count(&self) -> usize {
        self.files.values().map(|f| f.symbols.len()).sum()
    }

    /// `true` when the index has been loaded and the circuit breaker has NOT tripped.
    pub fn is_ready(&self) -> bool {
        if self.is_empty {
            return false;
        }
        !self.cb_state.is_tripped()
    }

    /// Returns the current index state.
    pub fn index_state(&self) -> IndexState {
        if self.is_empty {
            return IndexState::Empty;
        }
        if self.cb_state.is_tripped() {
            IndexState::CircuitBreakerTripped {
                summary: self.cb_state.summary(),
            }
        } else {
            IndexState::Ready
        }
    }

    /// Returns the wall-clock time when the index was last loaded.
    pub fn loaded_at_system(&self) -> SystemTime {
        self.loaded_at_system
    }

    /// SF-009: count Tier-1 indexed files that are NOT git-tracked AND NOT
    /// gitignored — i.e. recognized-extension source files that have been admitted
    /// into the index but are not under version control. Surfacing only; this does
    /// NOT change admission.
    ///
    /// **Fails open to `0`** when:
    ///   - the index has no recorded root (`indexed_root` is `None`), or
    ///   - no git repository is discoverable from that root, or
    ///   - the git index cannot be read (e.g. a freshly `git init`-ed repo with no
    ///     index yet).
    ///
    /// Without this fail-open, every file in a non-git working tree would count as
    /// "untracked", which is noise, not signal. The tracked set is derived from the
    /// git index (`git ls-files` semantics) via [`crate::git::GitRepo`], NOT the
    /// `ignore` crate — the `ignore` crate models gitignore rules but has no concept
    /// of which files are tracked.
    ///
    /// Covers BOTH discovery paths uniformly: it reads `self.files` (the Tier-1
    /// population) regardless of whether the index was built by `LiveIndex::load`
    /// (`discover_all_files`) or the watcher's `build_reload_data`
    /// (`discover_files`). The two paths admit different Tier-2 populations, but the
    /// Tier-1 set this count inspects lives in `self.files` either way.
    fn count_untracked_indexed(&self) -> usize {
        // Fail open: no recorded root means we cannot anchor a git lookup.
        let Some(root) = self.indexed_root.as_ref() else {
            return 0;
        };

        // Fail open: no git repo discoverable, or no readable index.
        let Ok(git_repo) = crate::git::GitRepo::open(root) else {
            return 0;
        };
        let Ok(tracked) = git_repo.tracked_paths() else {
            return 0;
        };

        // Empty tracked set: treat as fail-open. A repo with a readable but empty
        // index (no committed/staged files) would otherwise flag every indexed file
        // as untracked, which is the every-file-counts failure mode we avoid.
        if tracked.is_empty() {
            return 0;
        }

        let tracked_set: std::collections::HashSet<&str> =
            tracked.iter().map(|p| p.as_str()).collect();

        self.files
            .keys()
            .filter(|path| !tracked_set.contains(path.as_str()) && !self.is_path_gitignored(path))
            .count()
    }

    /// Compute health statistics for the index.
    ///
    /// Watcher fields are populated with safe defaults (Off state, zero counts).
    /// Use `health_stats_with_watcher` when a watcher is active.
    pub fn health_stats(&self) -> HealthStats {
        let mut parsed_count = 0usize;
        let mut partial_parse_count = 0usize;
        let mut failed_count = 0usize;
        let mut symbol_count = 0usize;

        for file in self.files.values() {
            symbol_count += file.symbols.len();
            match &file.parse_status {
                ParseStatus::Parsed => parsed_count += 1,
                ParseStatus::PartialParse { .. } => partial_parse_count += 1,
                ParseStatus::Failed { .. } => failed_count += 1,
            }
        }

        let mut partial_parse_files = Vec::new();
        let mut unexpected_partial_parse_files = Vec::new();
        let mut expected_vendor_partial_parse_files = Vec::new();
        let mut expected_framework_partial_parse_files = Vec::new();
        let mut expected_language_partial_parse_files = Vec::new();
        for (path, file) in &self.files {
            if matches!(file.parse_status, ParseStatus::PartialParse { .. }) {
                partial_parse_files.push(path.clone());
                if is_expected_vendor_partial_parse_noise(path, file, self.gitignore.as_ref()) {
                    expected_vendor_partial_parse_files.push(path.clone());
                } else if is_expected_framework_partial_parse(file) {
                    // SF-004: a partial parse caused only by Angular template
                    // control-flow (`@if`/`@for`/... in `.html`) that
                    // tree-sitter-html cannot model. Symbols are still extracted
                    // best-effort; this is a known framework limitation, not a
                    // repo-owned defect, so it is bucketed separately and never
                    // counted as an unexpected partial.
                    expected_framework_partial_parse_files.push(path.clone());
                } else if crate::parsing::is_expected_typescript_import_type_array_limitation(
                    &file.language,
                    &file.content,
                    crate::domain::LanguageId::is_tsx_path(path),
                ) {
                    // SF-003: a partial parse caused only by the known
                    // tree-sitter-typescript 0.23.2 import-type-array grammar
                    // limitation is valid TypeScript. It is never counted as an
                    // unexpected repo-owned partial (it is not a real defect), but
                    // it IS bucketed as an expected language-grammar partial so the
                    // quarantine registry accounts for every partial parse — the
                    // header partial count and the registry total stay in sync.
                    expected_language_partial_parse_files.push(path.clone());
                } else {
                    unexpected_partial_parse_files.push(path.clone());
                }
            }
        }
        partial_parse_files.sort();
        partial_parse_files.dedup();
        unexpected_partial_parse_files.sort();
        unexpected_partial_parse_files.dedup();
        expected_vendor_partial_parse_files.sort();
        expected_vendor_partial_parse_files.dedup();
        expected_framework_partial_parse_files.sort();
        expected_framework_partial_parse_files.dedup();
        expected_language_partial_parse_files.sort();
        expected_language_partial_parse_files.dedup();

        let mut failed_files: Vec<(String, String)> = self
            .files
            .iter()
            .filter_map(|(path, f)| {
                if let ParseStatus::Failed { error } = &f.parse_status {
                    Some((path.clone(), error.clone()))
                } else {
                    None
                }
            })
            .collect();
        failed_files.sort_by(|a, b| a.0.cmp(&b.0));

        HealthStats {
            file_count: self.files.len(),
            symbol_count,
            parsed_count,
            partial_parse_count,
            unexpected_partial_parse_count: unexpected_partial_parse_files.len(),
            expected_vendor_partial_parse_count: expected_vendor_partial_parse_files.len(),
            expected_framework_partial_parse_count: expected_framework_partial_parse_files.len(),
            expected_language_partial_parse_count: expected_language_partial_parse_files.len(),
            failed_count,
            load_duration: self.load_duration,
            watcher_state: WatcherState::Off,
            events_processed: 0,
            last_event_at: None,
            debounce_window_ms: 200,
            overflow_count: 0,
            last_overflow_at: None,
            stale_files_found: 0,
            last_reconcile_at: None,
            partial_parse_files,
            unexpected_partial_parse_files,
            expected_vendor_partial_parse_files,
            expected_framework_partial_parse_files,
            expected_language_partial_parse_files,
            failed_files,
            tier_counts: self.tier_counts(),
            local_empty_reason: self.local_empty_reason(),
            untracked_indexed: self.count_untracked_indexed(),
        }
    }

    /// Compute health statistics, populating watcher fields from the provided `WatcherInfo`.
    ///
    /// Use this variant when the file watcher is active and its state should be reflected
    /// in health reports.
    pub fn health_stats_with_watcher(&self, watcher: &WatcherInfo) -> HealthStats {
        let mut stats = self.health_stats();
        stats.watcher_state = watcher.state.clone();
        stats.events_processed = watcher.events_processed;
        stats.last_event_at = watcher.last_event_at;
        stats.debounce_window_ms = watcher.debounce_window_ms;
        stats.overflow_count = watcher.overflow_count;
        stats.last_overflow_at = watcher.last_overflow_at;
        stats.stale_files_found = watcher.stale_files_found;
        stats.last_reconcile_at = watcher.last_reconcile_at;
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FileClassification;
    use crate::live_index::store::{IndexedFile, ParseStatus};
    use std::sync::Arc;

    fn partial_ts_file(path: &str, content: &str) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::TypeScript,
            classification: FileClassification::for_code_path(path),
            content: content.as_bytes().to_vec(),
            symbols: Vec::new(),
            parse_status: ParseStatus::PartialParse {
                warning: "syntax error".to_string(),
            },
            parse_diagnostic: None,
            byte_len: content.len() as u64,
            content_hash: "test-hash".to_string(),
            references: Vec::new(),
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        }
    }

    fn index_with_files(files: Vec<IndexedFile>) -> LiveIndex {
        let mut index = LiveIndex::empty_live_index();
        index.is_empty = false;
        for file in files {
            index
                .files
                .insert(file.relative_path.clone(), Arc::new(file));
        }
        index
    }

    /// Regression for the health accounting mismatch: a TypeScript file whose
    /// partial parse is excused as the known SF-003 import-type-array grammar
    /// limitation MUST still be accounted for in the quarantine registry.
    ///
    /// Before the fix it was counted in `partial_parse_count` (the header) but
    /// landed in NO category vector, so the registry `total` (which sums the
    /// named buckets) was less than the header partial count — the excused file
    /// was invisible to every diagnostic list.
    #[test]
    fn sf003_excused_partial_is_accounted_in_quarantine_registry() {
        // The exact reported SF-003 repro shape (tree-sitter-typescript 0.23.2
        // mis-parses the `[]` suffix on an import-type member). This genuinely
        // produces a partial parse but is a known grammar limitation, not a
        // repo-owned defect.
        let excused = partial_ts_file("src/app.ts", "type S = import('rxjs').Subscription[];");
        // A genuinely broken TypeScript file — a real repo-owned partial.
        let broken = partial_ts_file("src/broken.ts", "function f( { return ;");

        let stats = index_with_files(vec![excused, broken]).health_stats();

        // Header counts both as partial.
        assert_eq!(
            stats.partial_parse_count, 2,
            "both files are ParseStatus::PartialParse"
        );

        // The registry total must equal partial + failed: every partial file is
        // accounted for in exactly one named category.
        let registry_total = stats.unexpected_partial_parse_count
            + stats.expected_vendor_partial_parse_count
            + stats.expected_framework_partial_parse_count
            + stats.expected_language_partial_parse_count
            + stats.failed_count;
        assert_eq!(
            registry_total,
            stats.partial_parse_count + stats.failed_count,
            "registry total must account for every partial parse (header={}, \
             unexpected={}, vendor={}, framework={}, language={}, failed={})",
            stats.partial_parse_count,
            stats.unexpected_partial_parse_count,
            stats.expected_vendor_partial_parse_count,
            stats.expected_framework_partial_parse_count,
            stats.expected_language_partial_parse_count,
            stats.failed_count,
        );

        // The SF-003 file is bucketed as an expected language-grammar partial,
        // NOT as an unexpected repo-owned defect.
        assert_eq!(
            stats.expected_language_partial_parse_count, 1,
            "the SF-003 import-type-array file is an expected language partial"
        );
        assert!(
            stats
                .expected_language_partial_parse_files
                .contains(&"src/app.ts".to_string()),
            "the SF-003 file must appear in the expected-language partial list"
        );

        // The genuinely broken file remains an unexpected repo-owned partial.
        assert_eq!(stats.unexpected_partial_parse_count, 1);
        assert!(
            stats
                .unexpected_partial_parse_files
                .contains(&"src/broken.ts".to_string()),
            "the genuinely broken file stays an unexpected partial"
        );
    }
}
