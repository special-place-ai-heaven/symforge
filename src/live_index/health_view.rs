use std::time::{Duration, SystemTime};

use crate::domain::LanguageId;
use crate::domain::index::{AdmissionTier, SkipReason};
use crate::watcher::{WatcherInfo, WatcherState};

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
    /// Sorted, deduplicated list of files with failed parse status and their error messages.
    pub failed_files: Vec<(String, String)>,
    /// Admission tier counts: (Tier1 indexed, Tier2 metadata-only, Tier3 hard-skipped).
    pub tier_counts: (usize, usize, usize),
    /// Reason the index is empty at startup (e.g. no safe root, auto-index off).
    /// Surfaced as a banner in `health` output so MCP clients see why no symbols loaded.
    pub local_empty_reason: Option<String>,
}

pub const EXPECTED_VENDOR_PARTIAL_PARSE_REASON: &str =
    "expected vendor: tree-sitter-scss C/header parser limitation";

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
        for (path, file) in &self.files {
            if matches!(file.parse_status, ParseStatus::PartialParse { .. }) {
                partial_parse_files.push(path.clone());
                if is_expected_vendor_partial_parse_noise(path, file, self.gitignore.as_ref()) {
                    expected_vendor_partial_parse_files.push(path.clone());
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
            failed_files,
            tier_counts: self.tier_counts(),
            local_empty_reason: self.local_empty_reason(),
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
