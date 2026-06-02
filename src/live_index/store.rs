use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use parking_lot::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use rayon::prelude::*;
use tracing::{error, info, warn};

use super::query::RepoOutlineView;
use crate::domain::ParseDiagnostic;
use crate::domain::index::{AdmissionTier, SkippedFile};
use crate::domain::{
    FileClassification, FileOutcome, FileProcessingResult, LanguageId, ReferenceRecord,
    SymbolRecord, find_enclosing_symbol,
};
use crate::{discovery, parsing};

#[cfg(windows)]
const INDEXING_THREAD_STACK_SIZE_ENV: &str = "SYMFORGE_INDEXING_THREAD_STACK_BYTES";
#[cfg(windows)]
const DEFAULT_INDEXING_THREAD_STACK_BYTES: usize = 4 * 1024 * 1024;
#[cfg(windows)]
const MIN_INDEXING_THREAD_STACK_BYTES: usize = 3 * 1024 * 1024;

static INDEXING_THREAD_POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

#[cfg(windows)]
fn indexing_thread_stack_size() -> usize {
    match std::env::var(INDEXING_THREAD_STACK_SIZE_ENV) {
        Ok(raw) => match raw.parse::<usize>() {
            Ok(bytes) if bytes >= MIN_INDEXING_THREAD_STACK_BYTES => bytes,
            Ok(bytes) => {
                warn!(
                    env = INDEXING_THREAD_STACK_SIZE_ENV,
                    requested = bytes,
                    minimum = MIN_INDEXING_THREAD_STACK_BYTES,
                    "indexing thread stack size too small; using Windows minimum"
                );
                MIN_INDEXING_THREAD_STACK_BYTES
            }
            Err(error) => {
                warn!(
                    env = INDEXING_THREAD_STACK_SIZE_ENV,
                    value = %raw,
                    %error,
                    default = DEFAULT_INDEXING_THREAD_STACK_BYTES,
                    "invalid indexing thread stack size; using default"
                );
                DEFAULT_INDEXING_THREAD_STACK_BYTES
            }
        },
        Err(_) => DEFAULT_INDEXING_THREAD_STACK_BYTES,
    }
}

fn indexing_thread_pool() -> &'static rayon::ThreadPool {
    INDEXING_THREAD_POOL.get_or_init(|| {
        let builder = rayon::ThreadPoolBuilder::new()
            .thread_name(|index| format!("symforge-index-{}", index));

        #[cfg(windows)]
        let builder = {
            let stack_size = indexing_thread_stack_size();
            info!(
                stack_size,
                env = INDEXING_THREAD_STACK_SIZE_ENV,
                "initializing indexing thread pool with explicit worker stack size"
            );
            builder.stack_size(stack_size)
        };

        builder
            .build()
            .expect("indexing thread pool should initialize")
    })
}

/// Per-file parse status stored in the index.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParseStatus {
    /// File parsed successfully with no syntax errors.
    Parsed,
    /// File parsed but tree-sitter reported syntax errors; symbols were still extracted.
    PartialParse { warning: String },
    /// File could not be parsed at all; symbols list is empty but content bytes are stored.
    Failed { error: String },
}

/// A single indexed file — all data needed for query and display.
#[derive(Clone, Debug)]
pub struct IndexedFile {
    pub relative_path: String,
    pub language: LanguageId,
    pub classification: FileClassification,
    /// Raw file bytes stored in memory (LIDX-03 — zero disk I/O on read path).
    pub content: Vec<u8>,
    /// Symbols extracted by the parser.
    pub symbols: Vec<SymbolRecord>,
    pub parse_status: ParseStatus,
    pub parse_diagnostic: Option<ParseDiagnostic>,
    pub byte_len: u64,
    pub content_hash: String,
    /// Cross-references extracted by xref::extract_references (Phase 4).
    pub references: Vec<ReferenceRecord>,
    /// Import alias map for this file: alias -> original name.
    pub alias_map: HashMap<String, String>,
    /// Unix timestamp (seconds) of the file's mtime when it was last indexed.
    /// Used by the freshness guard to detect files that changed on disk after indexing.
    /// Zero means mtime was not recorded (indexed before this field was added).
    pub mtime_secs: u64,
}

/// Identifies a single reference within a specific file.
/// Used as a value in `LiveIndex::reverse_index`.
#[derive(Clone, Debug)]
pub struct ReferenceLocation {
    /// Relative path of the file containing the reference.
    pub file_path: String,
    /// Index into `IndexedFile::references` for the specific `ReferenceRecord`.
    pub reference_idx: u32,
}

impl IndexedFile {
    pub fn from_parse_result(result: FileProcessingResult, content: Vec<u8>) -> Self {
        let parse_status = match &result.outcome {
            FileOutcome::Processed => ParseStatus::Parsed,
            FileOutcome::PartialParse { warning } => ParseStatus::PartialParse {
                warning: warning.clone(),
            },
            FileOutcome::Failed { error } => ParseStatus::Failed {
                error: error.clone(),
            },
        };

        // Destructure the result so we can consume references while borrowing symbols.
        let FileProcessingResult {
            relative_path,
            language,
            classification,
            outcome: _,
            parse_diagnostic,
            symbols,
            byte_len,
            content_hash,
            references: raw_references,
            alias_map,
        } = result;

        // Build a set of symbol byte ranges so we can filter definition-site hits
        // (Pitfall 1: a reference whose byte_range exactly matches a symbol's byte_range
        // is the definition itself — not a usage site).
        let symbol_byte_ranges: std::collections::HashSet<(u32, u32)> =
            symbols.iter().map(|s| s.byte_range).collect();

        // Assign enclosing_symbol_index for each reference and skip definition sites.
        let references: Vec<ReferenceRecord> = raw_references
            .into_iter()
            .filter(|r| !symbol_byte_ranges.contains(&r.byte_range))
            .map(|mut r| {
                if r.enclosing_symbol_index.is_none() {
                    r.enclosing_symbol_index = find_enclosing_symbol(&symbols, r.line_range.0);
                }
                r
            })
            .collect();

        IndexedFile {
            relative_path,
            language,
            classification,
            content,
            symbols,
            parse_status,
            parse_diagnostic,
            byte_len,
            content_hash,
            references,
            alias_map,
            mtime_secs: 0,
        }
    }

    /// Set the mtime recorded at index time. Call after `from_parse_result` for
    /// callers that have the file metadata available.
    pub fn with_mtime(mut self, mtime_secs: u64) -> Self {
        self.mtime_secs = mtime_secs;
        self
    }
}

impl AsRef<IndexedFile> for IndexedFile {
    fn as_ref(&self) -> &IndexedFile {
        self
    }
}

/// Tracks parse failures during index loading for the circuit breaker.
pub struct CircuitBreakerState {
    total: AtomicUsize,
    failed: AtomicUsize,
    tripped: AtomicBool,
    /// Failure threshold as a fraction (e.g., 0.20 = 20%).
    threshold: f64,
    /// First few failure details (path, reason) for summary reporting.
    failure_details: Mutex<Vec<(String, String)>>,
}

impl Clone for CircuitBreakerState {
    fn clone(&self) -> Self {
        Self {
            total: AtomicUsize::new(self.total.load(Ordering::Relaxed)),
            failed: AtomicUsize::new(self.failed.load(Ordering::Relaxed)),
            tripped: AtomicBool::new(self.tripped.load(Ordering::Relaxed)),
            threshold: self.threshold,
            failure_details: Mutex::new(self.failure_details.lock().clone()),
        }
    }
}

impl CircuitBreakerState {
    /// Create with an explicit threshold (for testability).
    pub fn new(threshold: f64) -> Self {
        Self {
            total: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            tripped: AtomicBool::new(false),
            threshold,
            failure_details: Mutex::new(Vec::new()),
        }
    }

    /// Create using the `SYMFORGE_CB_THRESHOLD` env var, defaulting to 0.20.
    pub fn from_env() -> Self {
        let threshold = std::env::var("SYMFORGE_CB_THRESHOLD")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.20);
        Self::new(threshold)
    }

    pub fn record_success(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self, path: &str, reason: &str) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.failed.fetch_add(1, Ordering::Relaxed);

        let mut details = self.failure_details.lock();
        if details.len() < 5 {
            details.push((path.to_string(), reason.to_string()));
        }
    }

    /// Returns `true` when the failure rate exceeds the threshold.
    ///
    /// IMPORTANT: returns `false` when fewer than 5 files have been processed
    /// (minimum-file guard prevents spurious trips on tiny repos).
    pub fn should_abort(&self) -> bool {
        let total = self.total.load(Ordering::Relaxed);
        if total < 5 {
            return false;
        }
        let failed = self.failed.load(Ordering::Relaxed);
        let rate = failed as f64 / total as f64;
        if rate > self.threshold {
            self.tripped.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Relaxed)
    }

    /// One-line summary plus top failure details.
    pub fn summary(&self) -> String {
        let total = self.total.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        let rate = if total > 0 {
            (failed as f64 / total as f64 * 100.0) as u32
        } else {
            0
        };

        let details = self.failure_details.lock();
        let top_failures: Vec<String> = details
            .iter()
            .take(3)
            .map(|(p, r)| format!("  - {p}: {r}"))
            .collect();

        let mut msg = format!(
            "circuit breaker tripped: {failed}/{total} files failed ({rate}% > {}%)",
            (self.threshold * 100.0) as u32
        );
        if !top_failures.is_empty() {
            msg.push_str("\nTop failures:\n");
            msg.push_str(&top_failures.join("\n"));
        }
        msg
    }
}

/// Overall state of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexState {
    /// Index was constructed with empty() — no files loaded yet.
    Empty,
    Loading,
    Ready,
    CircuitBreakerTripped {
        summary: String,
    },
}

/// Where the current in-memory index contents were sourced from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IndexLoadSource {
    EmptyBootstrap,
    FreshLoad,
    SnapshotRestore,
}

const SNAPSHOT_VERIFY_MISMATCH_PATH_LIMIT: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotVerifyReport {
    pub mismatch_count: usize,
    pub mismatched_paths: Vec<String>,
}

impl SnapshotVerifyReport {
    pub fn from_mismatched_paths(mut paths: Vec<String>) -> Self {
        paths.sort();
        paths.dedup();
        let mismatch_count = paths.len();
        paths.truncate(SNAPSHOT_VERIFY_MISMATCH_PATH_LIMIT);
        Self {
            mismatch_count,
            mismatched_paths: paths,
        }
    }

    pub fn empty() -> Self {
        Self {
            mismatch_count: 0,
            mismatched_paths: Vec::new(),
        }
    }

    pub fn omitted_path_count(&self) -> usize {
        self.mismatch_count
            .saturating_sub(self.mismatched_paths.len())
    }
}

/// Reconciliation status after restoring from a persisted snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotVerifyState {
    NotNeeded,
    Pending,
    Running,
    Completed(SnapshotVerifyReport),
}

impl SnapshotVerifyState {
    pub fn completed_without_mismatches() -> Self {
        Self::Completed(SnapshotVerifyReport::empty())
    }

    pub fn completed_with_mismatches(paths: Vec<String>) -> Self {
        Self::Completed(SnapshotVerifyReport::from_mismatched_paths(paths))
    }
}

/// Compact published status label for handle-level state consumers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PublishedIndexStatus {
    Empty,
    Loading,
    Ready,
    Degraded,
}

/// Lightweight published state captured from the live index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedIndexState {
    pub generation: u64,
    pub status: PublishedIndexStatus,
    pub degraded_summary: Option<String>,
    pub file_count: usize,
    pub parsed_count: usize,
    pub partial_parse_count: usize,
    pub unexpected_partial_parse_count: usize,
    pub expected_vendor_partial_parse_count: usize,
    pub failed_count: usize,
    pub partial_parse_files: Vec<String>,
    pub unexpected_partial_parse_files: Vec<String>,
    pub expected_vendor_partial_parse_files: Vec<String>,
    pub failed_files: Vec<(String, String)>,
    pub symbol_count: usize,
    pub loaded_at_system: SystemTime,
    pub load_duration: Duration,
    pub load_source: IndexLoadSource,
    pub snapshot_verify_state: SnapshotVerifyState,
    pub is_empty: bool,
    /// Admission tier counts: (Tier1 indexed, Tier2 metadata-only, Tier3 hard-skipped).
    pub tier_counts: (usize, usize, usize),
    /// Reason the index is empty at startup (LocalEmpty branch). Surfaced as
    /// a banner in `health` output. `None` when the index has files.
    pub local_empty_reason: Option<String>,
}

/// The in-memory index: file contents and parsed symbols for all discovered files.
#[derive(Clone)]
pub struct LiveIndex {
    /// Keyed by `relative_path` (forward-slash normalized).
    pub(crate) files: HashMap<String, Arc<IndexedFile>>,
    pub(crate) loaded_at: Instant,
    /// Wall-clock time when index was last loaded. Used by what_changed tool.
    pub(crate) loaded_at_system: SystemTime,
    pub(crate) load_duration: Duration,
    pub(crate) cb_state: CircuitBreakerState,
    /// True when constructed with empty() and reload() has not been called.
    pub(crate) is_empty: bool,
    /// Provenance for the current live contents.
    pub(crate) load_source: IndexLoadSource,
    /// Snapshot reconciliation status for snapshot-restored indices.
    pub(crate) snapshot_verify_state: SnapshotVerifyState,
    /// Repo-level reverse index: reference name -> all locations in the index.
    /// Updated incrementally on single-file mutations (update_file, remove_file);
    /// rebuilt from scratch on bulk operations (load, reload, snapshot restore).
    pub(crate) reverse_index: HashMap<String, Vec<ReferenceLocation>>,
    /// Secondary path index: lowercase basename -> sorted matching relative paths.
    pub(crate) files_by_basename: HashMap<String, Vec<String>>,
    /// Secondary path index: lowercase directory component -> sorted matching relative paths.
    pub(crate) files_by_dir_component: HashMap<String, Vec<String>>,
    /// Trigram search index for file-level text search acceleration.
    pub(crate) trigram_index: super::trigram::TrigramIndex,
    /// Compiled gitignore patterns loaded at index time. Used by NoisePolicy
    /// to classify files as vendor/generated/ignored noise.
    pub(crate) gitignore: Option<ignore::gitignore::Gitignore>,
    /// Files that were not fully indexed (Tier 2 metadata-only or Tier 3 hard-skipped).
    pub(crate) skipped_files: Vec<SkippedFile>,
    /// Per-workspace co-change store, present when policy warms it or when
    /// lazy policy finds an existing store at startup.
    pub(crate) coupling_store: Option<Arc<super::coupling::CouplingStore>>,
    /// Reason this index started empty, if any. Set at construction time by
    /// the startup-plan branch; surfaced in `health` output as an actionable
    /// banner. `None` when the index has files or after a reload.
    pub(crate) local_empty_reason: Arc<parking_lot::RwLock<Option<String>>>,
}

/// Lightweight snapshot of a symbol for pre-update diffing in `analyze_file_impact`.
///
/// Stored in [`SharedIndexHandle::pre_update_symbols`] so the impact tool can
/// compare against the state *before* the watcher or edit tools re-indexed.
#[derive(Clone, Debug)]
pub struct PreUpdateSymbol {
    pub name: String,
    pub kind: String,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
}

/// Central shared handle for the live in-memory index.
///
/// Uses `ArcSwap` for lock-free concurrent reads. Readers load an `Arc<LiveIndex>` snapshot
/// without blocking; writers serialize through `write_mutex`, clone-mutate-swap the live
/// index, then atomically publish derived state. A failed mutation is simply discarded —
/// readers never observe a partially-mutated index.
///
/// `published_state`, `published_repo_outline`, and `git_temporal` also use `ArcSwap`
/// for contention-free reads (previously `RwLock<Arc<T>>`).
pub struct SharedIndexHandle {
    live: ArcSwap<LiveIndex>,
    /// Serializes writers — only one mutation in flight at a time.
    write_mutex: Mutex<()>,
    published_state: ArcSwap<PublishedIndexState>,
    published_repo_outline: ArcSwap<RepoOutlineView>,
    /// Publish-versioning counter for `PublishedIndexState`; bumped on every publish.
    next_generation: AtomicU64,
    /// Project-identity counter for fencing stale watcher mutations; bumped only on reload.
    project_generation: AtomicU64,
    /// Project generation that was last produced by an explicit index_folder reset.
    last_reset_project_generation: AtomicU64,
    /// Telemetry counter for fenced mutations rejected due to stale project generation.
    rejected_stale_mutations: AtomicU64,
    /// Git temporal intelligence — independently swapped side-table with
    /// per-file churn, ownership, and co-change data. Populated asynchronously
    /// after index load/reload completes.
    git_temporal: ArcSwap<super::git_temporal::GitTemporalIndex>,
    /// Pre-update symbol snapshots: saved automatically by `update_file` before
    /// the index entry is replaced. Consumed (take) by `analyze_file_impact` to
    /// compute accurate diffs even when the watcher re-indexes before the hook fires.
    pre_update_symbols: Mutex<HashMap<String, Vec<PreUpdateSymbol>>>,
}

/// Write guard that republishes lightweight handle state when mutated data is released.
///
/// Holds an owned clone of the `LiveIndex`. On drop, if any mutation occurred (via
/// `DerefMut`), the modified index is swapped into the `ArcSwap` and published state
/// is refreshed. If no mutation occurred, the clone is simply discarded.
pub struct SharedIndexWriteGuard<'a> {
    handle: &'a SharedIndexHandle,
    _mutex: MutexGuard<'a, ()>,
    index: Option<LiveIndex>,
    dirty: bool,
}

impl SharedIndexHandle {
    pub fn new(index: LiveIndex) -> Self {
        let published_state = Arc::new(PublishedIndexState::capture(0, &index));
        let published_repo_outline = Arc::new(index.capture_repo_outline_view());
        Self {
            live: ArcSwap::new(Arc::new(index)),
            write_mutex: Mutex::new(()),
            published_state: ArcSwap::new(published_state),
            published_repo_outline: ArcSwap::new(published_repo_outline),
            next_generation: AtomicU64::new(1),
            project_generation: AtomicU64::new(0),
            last_reset_project_generation: AtomicU64::new(0),
            rejected_stale_mutations: AtomicU64::new(0),
            git_temporal: ArcSwap::new(Arc::new(super::git_temporal::GitTemporalIndex::pending())),
            pre_update_symbols: Mutex::new(HashMap::new()),
        }
    }

    pub fn shared(index: LiveIndex) -> Arc<Self> {
        Arc::new(Self::new(index))
    }

    /// Lock-free read: returns a guard that derefs to `&LiveIndex`.
    ///
    /// The returned guard holds a snapshot of the index at the time of the call.
    /// Concurrent writes do not affect the snapshot — they swap in a new `Arc`
    /// that subsequent `read()` calls will see.
    pub fn read(&self) -> arc_swap::Guard<Arc<LiveIndex>> {
        self.live.load()
    }

    /// Acquire exclusive write access. The returned guard holds an owned clone
    /// of the current `LiveIndex`. Mutations via `DerefMut` mark the guard
    /// dirty; on drop the modified index is swapped in and published.
    pub fn write(&self) -> SharedIndexWriteGuard<'_> {
        let mutex = self.write_mutex.lock();
        let snapshot = (*self.live.load_full()).clone();
        SharedIndexWriteGuard {
            handle: self,
            _mutex: mutex,
            index: Some(snapshot),
            dirty: false,
        }
    }

    /// Lock-free read of the published state snapshot.
    pub fn published_state(&self) -> Arc<PublishedIndexState> {
        self.published_state.load_full()
    }

    /// Lock-free read of the published repo outline.
    pub fn published_repo_outline(&self) -> Arc<RepoOutlineView> {
        self.published_repo_outline.load_full()
    }

    pub fn current_project_generation(&self) -> u64 {
        self.project_generation.load(Ordering::Acquire)
    }

    pub fn current_reset_project_generation(&self) -> Option<u64> {
        match self.last_reset_project_generation.load(Ordering::Acquire) {
            0 => None,
            generation => Some(generation),
        }
    }

    pub fn mark_index_folder_reset(&self) -> u64 {
        let generation = self.current_project_generation();
        self.last_reset_project_generation
            .store(generation, Ordering::Release);
        generation
    }

    #[allow(dead_code)]
    pub fn current_rejected_stale_mutations(&self) -> u64 {
        self.rejected_stale_mutations.load(Ordering::Relaxed)
    }

    pub(crate) fn note_rejected_stale_mutation(&self) {
        self.rejected_stale_mutations
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn reload(&self, root: &Path) -> anyhow::Result<()> {
        // Build new index data OUTSIDE the write lock (file I/O + parsing).
        // Only the final swap acquires the mutex, reducing block time from
        // seconds (full I/O) to milliseconds (in-memory index rebuild).
        let data = LiveIndex::build_reload_data(root)?;
        let _wg = self.write_mutex.lock();
        let mut live = (*self.live.load_full()).clone();
        live.apply_reload_data(data);
        self.swap_and_publish(live);
        self.project_generation.fetch_add(1, Ordering::AcqRel);
        self.last_reset_project_generation
            .store(0, Ordering::Release);
        Ok(())
    }

    /// Drop all indexed state and publish a fresh empty index.
    ///
    /// Used to invalidate a stale in-process index after the project has been
    /// switched out-of-band (e.g. a daemon-proxy `index_folder` rebinds the
    /// shared session to a new workspace). Bumps `project_generation` so any
    /// in-flight watcher mutations carrying the old generation are fenced, and
    /// clears any captured pre-update symbol snapshots so they cannot leak into
    /// a later impact diff for the wrong project.
    ///
    /// After this returns, `published_state().file_count == 0`, so the next
    /// local-fallback path (`ensure_local_index`) reloads from the current
    /// repo root instead of serving the previous project.
    pub fn reset_to_empty(&self) {
        let _wg = self.write_mutex.lock();
        self.swap_and_publish(LiveIndex::empty_live_index());
        self.project_generation.fetch_add(1, Ordering::AcqRel);
        self.last_reset_project_generation
            .store(0, Ordering::Release);
        self.pre_update_symbols.lock().clear();
    }

    pub fn update_file(&self, path: String, file: IndexedFile) {
        let _wg = self.write_mutex.lock();
        let current = self.live.load_full();
        // Capture pre-update symbols so analyze_file_impact can diff correctly
        // even when the watcher re-indexes before the hook fires.
        if let Some(existing) = current.get_file(&path) {
            let snapshot: Vec<PreUpdateSymbol> = existing
                .symbols
                .iter()
                .map(|s| PreUpdateSymbol {
                    name: s.name.clone(),
                    kind: s.kind.to_string(),
                    line_range: s.line_range,
                    byte_range: s.byte_range,
                })
                .collect();
            self.pre_update_symbols
                .lock()
                .insert(path.clone(), snapshot);
        }
        let mut live = (*current).clone();
        let path_clone = path.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            live.update_file(path, file);
        }));
        match result {
            Ok(()) => self.swap_and_publish(live),
            Err(panic_info) => {
                // Clone-mutate-swap means the original index is untouched on panic —
                // no repair needed, just log and discard the failed clone.
                let msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown");
                tracing::error!(
                    "index mutation panicked for '{}': {} — original index preserved",
                    path_clone,
                    msg
                );
            }
        }
    }

    pub fn update_file_at_generation(
        &self,
        path: &str,
        file: IndexedFile,
        expected_gen: u64,
    ) -> bool {
        let _wg = self.write_mutex.lock();
        let current_gen = self.project_generation.load(Ordering::Acquire);
        if current_gen != expected_gen {
            self.rejected_stale_mutations
                .fetch_add(1, Ordering::Relaxed);
            tracing::trace!(
                path,
                expected_gen,
                current_gen,
                "rejecting stale indexed-file update"
            );
            return false;
        }

        let current = self.live.load_full();
        // Capture pre-update symbols so analyze_file_impact can diff correctly
        // even when the watcher re-indexes before the hook fires.
        if let Some(existing) = current.get_file(path) {
            let snapshot: Vec<PreUpdateSymbol> = existing
                .symbols
                .iter()
                .map(|s| PreUpdateSymbol {
                    name: s.name.clone(),
                    kind: s.kind.to_string(),
                    line_range: s.line_range,
                    byte_range: s.byte_range,
                })
                .collect();
            self.pre_update_symbols
                .lock()
                .insert(path.to_string(), snapshot);
        }
        let mut live = (*current).clone();
        let path_owned = path.to_string();
        let path_clone = path_owned.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            live.update_file(path_owned, file);
        }));
        match result {
            Ok(()) => self.swap_and_publish(live),
            Err(panic_info) => {
                // Clone-mutate-swap means the original index is untouched on panic —
                // no repair needed, just log and discard the failed clone.
                let msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown");
                tracing::error!(
                    "index mutation panicked for '{}': {} — original index preserved",
                    path_clone,
                    msg
                );
            }
        }
        true
    }

    /// Update only the stored mtime for a file without re-parsing.
    ///
    /// Used by the watcher when a file's content hash matches but its mtime has
    /// drifted (e.g., after `git rebase` or `touch`). Without this, the
    /// reconciliation loop detects the mtime difference and re-checks the file
    /// on every sweep, causing an infinite stale → hash-skip → stale loop.
    pub fn touch_mtime(&self, path: &str, new_mtime: u64) {
        let _wg = self.write_mutex.lock();
        let current = self.live.load_full();
        if let Some(file) = current.files.get(path)
            && file.mtime_secs != new_mtime
        {
            let mut live = (*current).clone();
            let mut updated = (**live.files.get(path).unwrap()).clone();
            updated.mtime_secs = new_mtime;
            live.files.insert(path.to_string(), Arc::new(updated));
            self.live.store(Arc::new(live));
            // mtime-only change doesn't affect published state
        }
    }

    pub fn touch_mtime_at_generation(&self, path: &str, new_mtime: u64, expected_gen: u64) -> bool {
        let _wg = self.write_mutex.lock();
        let current_gen = self.project_generation.load(Ordering::Acquire);
        if current_gen != expected_gen {
            self.rejected_stale_mutations
                .fetch_add(1, Ordering::Relaxed);
            tracing::trace!(
                path,
                expected_gen,
                current_gen,
                "rejecting stale mtime touch"
            );
            return false;
        }

        let current = self.live.load_full();
        if let Some(file) = current.files.get(path)
            && file.mtime_secs != new_mtime
        {
            let mut live = (*current).clone();
            let mut updated = (**live.files.get(path).unwrap()).clone();
            updated.mtime_secs = new_mtime;
            live.files.insert(path.to_string(), Arc::new(updated));
            self.live.store(Arc::new(live));
            // mtime-only change doesn't affect published state
        }
        true
    }

    pub fn add_file(&self, path: String, file: IndexedFile) {
        let _wg = self.write_mutex.lock();
        let mut live = (*self.live.load_full()).clone();
        let path_clone = path.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            live.add_file(path, file);
        }));
        match result {
            Ok(()) => self.swap_and_publish(live),
            Err(panic_info) => {
                let msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown");
                tracing::error!(
                    "index add panicked for '{}': {} — original index preserved",
                    path_clone,
                    msg
                );
            }
        }
    }

    pub fn remove_file(&self, path: &str) {
        let _wg = self.write_mutex.lock();
        let mut live = (*self.live.load_full()).clone();
        let path_owned = path.to_string();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            live.remove_file(path);
        }));
        match result {
            Ok(()) => self.swap_and_publish(live),
            Err(panic_info) => {
                let msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown");
                tracing::error!(
                    "index remove panicked for '{}': {} — original index preserved",
                    path_owned,
                    msg
                );
            }
        }
    }

    pub fn remove_file_at_generation(&self, path: &str, expected_gen: u64) -> bool {
        let _wg = self.write_mutex.lock();
        let current_gen = self.project_generation.load(Ordering::Acquire);
        if current_gen != expected_gen {
            self.rejected_stale_mutations
                .fetch_add(1, Ordering::Relaxed);
            tracing::trace!(
                path,
                expected_gen,
                current_gen,
                "rejecting stale file removal"
            );
            return false;
        }

        let mut live = (*self.live.load_full()).clone();
        let path_owned = path.to_string();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            live.remove_file(path);
        }));
        match result {
            Ok(()) => self.swap_and_publish(live),
            Err(panic_info) => {
                let msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown");
                tracing::error!(
                    "index remove panicked for '{}': {} — original index preserved",
                    path_owned,
                    msg
                );
            }
        }
        true
    }

    pub fn mark_snapshot_verify_running(&self) {
        let _wg = self.write_mutex.lock();
        let mut live = (*self.live.load_full()).clone();
        live.mark_snapshot_verify_running();
        self.swap_and_publish(live);
    }

    pub fn mark_snapshot_verify_completed(&self, mismatched_paths: Vec<String>) {
        let _wg = self.write_mutex.lock();
        let mut live = (*self.live.load_full()).clone();
        live.mark_snapshot_verify_completed(mismatched_paths);
        self.swap_and_publish(live);
    }

    /// Swap a new `LiveIndex` into the `ArcSwap` and publish derived state.
    ///
    /// Must be called while holding `write_mutex`.
    fn swap_and_publish(&self, live: LiveIndex) {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let published_state = Arc::new(PublishedIndexState::capture(generation, &live));
        let published_repo_outline = Arc::new(live.capture_repo_outline_view());
        self.live.store(Arc::new(live));
        self.published_state.store(published_state);
        self.published_repo_outline.store(published_repo_outline);
    }

    /// Lock-free read of the git temporal index.
    pub fn git_temporal(&self) -> Arc<super::git_temporal::GitTemporalIndex> {
        self.git_temporal.load_full()
    }

    /// Take (consume) the pre-update symbol snapshot for a file, if any.
    ///
    /// Used by `analyze_file_impact` to get the symbols from *before* the last
    /// `update_file` call — prevents the watcher race where the index is already
    /// updated to the post-edit state before the hook fires.
    pub fn take_pre_update_symbols(&self, path: &str) -> Option<Vec<PreUpdateSymbol>> {
        self.pre_update_symbols.lock().remove(path)
    }

    /// Atomically replace the git temporal index with a new version.
    pub fn update_git_temporal(&self, index: super::git_temporal::GitTemporalIndex) {
        self.git_temporal.store(Arc::new(index));
    }

    pub fn update_git_temporal_at_generation(
        &self,
        index: super::git_temporal::GitTemporalIndex,
        expected_gen: u64,
    ) -> bool {
        let _wg = self.write_mutex.lock();
        let current_gen = self.project_generation.load(Ordering::Acquire);
        if current_gen != expected_gen {
            self.rejected_stale_mutations
                .fetch_add(1, Ordering::Relaxed);
            tracing::trace!(
                expected_gen,
                current_gen,
                "rejecting stale git temporal publication"
            );
            return false;
        }

        self.git_temporal.store(Arc::new(index));
        true
    }

    /// Set the empty-index reason on the live LiveIndex. Used by the startup
    /// LocalEmpty branch so `health` can surface why the index is empty.
    pub fn set_local_empty_reason(&self, reason: Option<String>) {
        self.live.load().set_local_empty_reason(reason);
    }
}

impl<'a> Deref for SharedIndexWriteGuard<'a> {
    type Target = LiveIndex;

    fn deref(&self) -> &Self::Target {
        self.index
            .as_ref()
            .expect("SharedIndexWriteGuard used after drop")
    }
}

impl DerefMut for SharedIndexWriteGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty = true;
        self.index
            .as_mut()
            .expect("SharedIndexWriteGuard used after drop")
    }
}

impl Drop for SharedIndexWriteGuard<'_> {
    fn drop(&mut self) {
        if self.dirty
            && let Some(live) = self.index.take()
        {
            self.handle.swap_and_publish(live);
        }
    }
}

/// Thread-safe shared handle to the index.
pub type SharedIndex = Arc<SharedIndexHandle>;

impl PublishedIndexState {
    fn capture(generation: u64, index: &LiveIndex) -> Self {
        let (status, degraded_summary) = match index.index_state() {
            IndexState::Empty => (PublishedIndexStatus::Empty, None),
            IndexState::Loading => (PublishedIndexStatus::Loading, None),
            IndexState::Ready => (PublishedIndexStatus::Ready, None),
            IndexState::CircuitBreakerTripped { summary } => {
                (PublishedIndexStatus::Degraded, Some(summary))
            }
        };
        let stats = index.health_stats();
        Self {
            generation,
            status,
            degraded_summary,
            file_count: stats.file_count,
            parsed_count: stats.parsed_count,
            partial_parse_count: stats.partial_parse_count,
            unexpected_partial_parse_count: stats.unexpected_partial_parse_count,
            expected_vendor_partial_parse_count: stats.expected_vendor_partial_parse_count,
            failed_count: stats.failed_count,
            partial_parse_files: stats.partial_parse_files.into_iter().take(10).collect(),
            unexpected_partial_parse_files: stats
                .unexpected_partial_parse_files
                .into_iter()
                .take(10)
                .collect(),
            expected_vendor_partial_parse_files: stats
                .expected_vendor_partial_parse_files
                .into_iter()
                .take(10)
                .collect(),
            failed_files: stats.failed_files.into_iter().take(10).collect(),
            symbol_count: stats.symbol_count,
            loaded_at_system: index.loaded_at_system,
            load_duration: stats.load_duration,
            load_source: index.load_source,
            snapshot_verify_state: index.snapshot_verify_state.clone(),
            is_empty: index.is_empty,
            tier_counts: stats.tier_counts,
            local_empty_reason: stats.local_empty_reason,
        }
    }

    pub fn status_label(&self) -> &'static str {
        match self.status {
            PublishedIndexStatus::Empty => "Empty",
            PublishedIndexStatus::Loading => "Loading",
            PublishedIndexStatus::Ready => "Ready",
            PublishedIndexStatus::Degraded => "Degraded",
        }
    }
}

/// Secondary indices derived from a single `files` map snapshot.
/// Invariant: these indices are one coherent snapshot derived from exactly
/// the `files` map they are paired with. Grouping them enforces this.
pub(crate) struct DerivedIndices {
    pub trigram_index: super::trigram::TrigramIndex,
    pub reverse_index: HashMap<String, Vec<ReferenceLocation>>,
    pub files_by_basename: HashMap<String, Vec<String>>,
    pub files_by_dir_component: HashMap<String, Vec<String>>,
}

impl DerivedIndices {
    /// Build all derived indices from a file map. Pure function — no side effects,
    /// no locks, safe to call from any thread.
    pub(crate) fn build_from_files(files: &HashMap<String, Arc<IndexedFile>>) -> Self {
        let (files_by_basename, files_by_dir_component) = build_path_indices_from_files(files);
        Self {
            trigram_index: super::trigram::TrigramIndex::build_from_files(files),
            reverse_index: build_reverse_index_from_files(files),
            files_by_basename,
            files_by_dir_component,
        }
    }
}

/// Pre-computed reload data built outside any lock.
///
/// Contains everything needed to swap into a `LiveIndex` under the write lock.
/// All derived indices are pre-built so that `apply_reload_data` is pure field
/// assignment (microseconds, not milliseconds).
///
/// # Failure boundaries
///
/// `build_reload_data()` is all-or-nothing and side-effect-free with respect to
/// the live index state. Only `apply_reload_data()` mutates the live state, and
/// it cannot fail — it's pure assignment.
pub(crate) struct ReloadData {
    pub files: HashMap<String, Arc<IndexedFile>>,
    pub cb_state: CircuitBreakerState,
    pub load_duration: Duration,
    pub gitignore: Option<ignore::gitignore::Gitignore>,
    pub derived: DerivedIndices,
    pub skipped_files: Vec<SkippedFile>,
    pub coupling_store: Option<Arc<super::coupling::CouplingStore>>,
}

/// Build a reverse index from a file map (standalone, no `&self` needed).
pub(crate) fn build_reverse_index_from_files(
    files: &HashMap<String, Arc<IndexedFile>>,
) -> HashMap<String, Vec<ReferenceLocation>> {
    let mut idx: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();
    for (file_path, indexed_file) in files {
        for (reference_idx, reference) in indexed_file.references.iter().enumerate() {
            idx.entry(reference.name.clone())
                .or_default()
                .push(ReferenceLocation {
                    file_path: file_path.clone(),
                    reference_idx: reference_idx as u32,
                });
        }
    }
    idx
}

/// Build path indices (basename + dir component) from a file map.
pub(crate) fn build_path_indices_from_files(
    files: &HashMap<String, Arc<IndexedFile>>,
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut by_basename: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_dir_component: HashMap<String, Vec<String>> = HashMap::new();
    for path in files.keys() {
        if let Some(basename) = basename_key(path) {
            insert_sorted_unique(by_basename.entry(basename).or_default(), path);
        }
        for component in dir_component_keys(path) {
            insert_sorted_unique(by_dir_component.entry(component).or_default(), path);
        }
    }
    (by_basename, by_dir_component)
}

impl LiveIndex {
    /// Load all source files under `root` into memory in parallel (Rayon), parse them,
    /// and return a `SharedIndex`.
    ///
    /// This function is **synchronous** — it must complete before the async tokio runtime
    /// needs the index. Rayon handles internal parallelism.
    pub fn load(root: &Path) -> anyhow::Result<SharedIndex> {
        let start = Instant::now();

        info!("LiveIndex::load starting at {:?}", root);

        // 1. Discover ALL files (not just known-language ones) so the admission gate
        //    can classify every file, including those with denylisted or unknown extensions.
        let all_entries = discovery::discover_all_files(root)?;
        info!(
            "discovered {} total files (pre-admission)",
            all_entries.len()
        );

        // 2. Run admission gate in parallel.
        //    For files that pass Tier-1 initially (size/extension checks), we read content
        //    and re-run the binary sniff before committing to parse.
        //    Files that are non-Normal skip reading entirely.
        use crate::discovery::classify_admission;
        use crate::domain::index::{AdmissionTier, SkippedFile};

        enum AdmissionOutcome {
            Parse {
                relative_path: String,
                language: crate::domain::LanguageId,
                classification: crate::domain::FileClassification,
                bytes: Vec<u8>,
                mtime_secs: u64,
            },
            Skip(SkippedFile),
        }

        let outcomes: Vec<AdmissionOutcome> = indexing_thread_pool().install(|| {
            all_entries
                .par_iter()
                .filter_map(|entry| {
                    // Phase 1: size + extension check (no I/O beyond what the walk gave us).
                    let decision_pre = classify_admission(
                        &entry.absolute_path,
                        entry.file_size,
                        None, // no content yet
                    );

                    match decision_pre.tier {
                        AdmissionTier::HardSkip | AdmissionTier::MetadataOnly => {
                            // No need to read content — already decided.
                            let sf = SkippedFile {
                                path: entry.relative_path.clone(),
                                size: entry.file_size,
                                extension: entry
                                    .absolute_path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|s| s.to_string()),
                                decision: decision_pre,
                            };
                            return Some(AdmissionOutcome::Skip(sf));
                        }
                        AdmissionTier::Normal => {}
                    }

                    // Phase 2: we tentatively have Tier-1. If the file has no recognized
                    // language, we cannot parse it — skip it as metadata-only.
                    let language = match &entry.language {
                        Some(lang) => lang.clone(),
                        None => {
                            // Unknown extension, not on denylist, under size limit.
                            // Read content to do binary sniff, then store as skipped.
                            let bytes = match std::fs::read(&entry.absolute_path) {
                                Ok(b) => b,
                                Err(e) => {
                                    warn!("failed to read {:?}: {}", entry.absolute_path, e);
                                    return None;
                                }
                            };
                            let decision_post = classify_admission(
                                &entry.absolute_path,
                                entry.file_size,
                                Some(&bytes),
                            );
                            let sf = SkippedFile {
                                path: entry.relative_path.clone(),
                                size: entry.file_size,
                                extension: entry
                                    .absolute_path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|s| s.to_string()),
                                decision: decision_post,
                            };
                            return Some(AdmissionOutcome::Skip(sf));
                        }
                    };

                    // Phase 3: read content and do binary sniff before passing to parser.
                    let bytes = match std::fs::read(&entry.absolute_path) {
                        Ok(b) => b,
                        Err(e) => {
                            warn!("failed to read {:?}: {}", entry.absolute_path, e);
                            return None;
                        }
                    };
                    let mtime_secs = std::fs::metadata(&entry.absolute_path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    let decision_post =
                        classify_admission(&entry.absolute_path, entry.file_size, Some(&bytes));

                    match decision_post.tier {
                        AdmissionTier::HardSkip | AdmissionTier::MetadataOnly => {
                            // Binary sniff reclassified this file — do NOT parse.
                            let sf = SkippedFile {
                                path: entry.relative_path.clone(),
                                size: entry.file_size,
                                extension: entry
                                    .absolute_path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|s| s.to_string()),
                                decision: decision_post,
                            };
                            Some(AdmissionOutcome::Skip(sf))
                        }
                        AdmissionTier::Normal => Some(AdmissionOutcome::Parse {
                            relative_path: entry.relative_path.clone(),
                            language,
                            classification: entry.classification,
                            bytes,
                            mtime_secs,
                        }),
                    }
                })
                .collect()
        });

        // 3. Split outcomes into parse candidates and skipped files.
        let mut skipped_files: Vec<SkippedFile> = Vec::new();
        let mut to_parse: Vec<(
            String,
            crate::domain::LanguageId,
            crate::domain::FileClassification,
            Vec<u8>,
            u64, // mtime_secs
        )> = Vec::new();

        for outcome in outcomes {
            match outcome {
                AdmissionOutcome::Skip(sf) => skipped_files.push(sf),
                AdmissionOutcome::Parse {
                    relative_path,
                    language,
                    classification,
                    bytes,
                    mtime_secs,
                } => {
                    to_parse.push((relative_path, language, classification, bytes, mtime_secs));
                }
            }
        }

        info!(
            "admission gate: {} to parse, {} skipped",
            to_parse.len(),
            skipped_files.len()
        );

        // 4. Parse all admitted files in parallel via Rayon.
        let mut parse_results: Vec<(String, IndexedFile)> = indexing_thread_pool().install(|| {
            to_parse
                .into_par_iter()
                .map(
                    |(relative_path, language, classification, bytes, mtime_secs)| {
                        let result = parsing::process_file_with_classification(
                            &relative_path,
                            &bytes,
                            language,
                            classification,
                        );
                        let indexed =
                            IndexedFile::from_parse_result(result, bytes).with_mtime(mtime_secs);
                        (relative_path, indexed)
                    },
                )
                .collect()
        });

        // 5. Sort by path for deterministic circuit-breaker evaluation order.
        parse_results.sort_by(|a, b| a.0.cmp(&b.0));

        // 6. Build HashMap sequentially, running circuit breaker checks.
        let cb_state = CircuitBreakerState::from_env();
        let mut files: HashMap<String, Arc<IndexedFile>> =
            HashMap::with_capacity(parse_results.len());

        let mut cb_tripped = false;
        for (path, indexed_file) in parse_results {
            match &indexed_file.parse_status {
                ParseStatus::Failed { error } => {
                    cb_state.record_failure(&path, error);
                }
                _ => {
                    cb_state.record_success();
                }
            }

            if cb_state.should_abort() {
                let summary = cb_state.summary();
                error!("{}", summary);
                cb_tripped = true;
                // Still insert the file before breaking
                files.insert(path, Arc::new(indexed_file));
                break;
            }

            files.insert(path, Arc::new(indexed_file));
        }

        if cb_tripped {
            cb_state.tripped.store(true, Ordering::Relaxed);
        }

        let load_duration = start.elapsed();
        info!(
            "LiveIndex loaded: {} files, {} symbols, {} skipped, {:?}",
            files.len(),
            files.values().map(|f| f.symbols.len()).sum::<usize>(),
            skipped_files.len(),
            load_duration
        );

        let trigram_index = super::trigram::TrigramIndex::build_from_files(&files);
        let gitignore = discovery::load_gitignore(root);
        let coupling_store = super::coupling::init_coupling_store(root);

        let mut index = LiveIndex {
            files,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration,
            cb_state,
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
            gitignore,
            skipped_files,
            coupling_store,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        // Hook registration must be unconditional so a flag flipped after
        // boot still captures edits. The DB-touching reset-policy work is
        // deferred to the first commitment-tool bump (lazy via
        // `cached_store_for`) per ADR 0011 — discovery-only sessions leave
        // no frecency footprint.
        crate::live_index::frecency::ensure_bump_hook_registered();

        Ok(SharedIndexHandle::shared(index))
    }

    /// Build a bare, empty `LiveIndex` value (no files loaded).
    ///
    /// Shared by [`LiveIndex::empty`] (initial bootstrap) and
    /// [`SharedIndexHandle::reset_to_empty`] (project-switch invalidation) so
    /// both produce identical empty state.
    pub(crate) fn empty_live_index() -> LiveIndex {
        LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: true,
            load_source: IndexLoadSource::EmptyBootstrap,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: super::trigram::TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    /// Create an empty `SharedIndex` with no files loaded.
    ///
    /// Used when `SYMFORGE_AUTO_INDEX=false`. The caller must call `reload()` to populate it.
    /// Returns `IndexState::Empty` and `is_ready() == false` until reloaded.
    pub fn empty() -> SharedIndex {
        SharedIndexHandle::shared(Self::empty_live_index())
    }

    /// Set the reason this index is empty (for `health` banner). Call at startup
    /// from the LocalEmpty branch.
    pub fn set_local_empty_reason(&self, reason: Option<String>) {
        *self.local_empty_reason.write() = reason;
    }

    /// Read the empty-index reason, if any.
    pub fn local_empty_reason(&self) -> Option<String> {
        self.local_empty_reason.read().clone()
    }

    pub fn coupling_store(&self) -> Option<&super::coupling::CouplingStore> {
        self.coupling_store.as_deref()
    }

    pub fn add_skipped_file(&mut self, sf: SkippedFile) {
        self.skipped_files.push(sf);
    }

    pub fn skipped_files(&self) -> &[SkippedFile] {
        &self.skipped_files
    }

    /// Returns (tier1_count, tier2_count, tier3_count).
    /// Tier 1 = number of indexed files (self.files.len()).
    /// Tier 2/3 = from skipped_files.
    pub fn tier_counts(&self) -> (usize, usize, usize) {
        let tier1 = self.files.len();
        let mut tier2 = 0;
        let mut tier3 = 0;
        for sf in &self.skipped_files {
            match sf.tier() {
                AdmissionTier::MetadataOnly => tier2 += 1,
                AdmissionTier::HardSkip => tier3 += 1,
                AdmissionTier::Normal => {} // shouldn't happen
            }
        }
        (tier1, tier2, tier3)
    }

    /// Build reload data without holding any lock. Performs all file I/O and
    /// parsing via Rayon. The returned `ReloadData` is applied under the write
    /// lock via `apply_reload_data` — reducing lock hold time from seconds to
    /// milliseconds.
    pub(crate) fn build_reload_data(root: &Path) -> anyhow::Result<ReloadData> {
        let start = Instant::now();

        info!("LiveIndex::build_reload_data starting at {:?}", root);

        if !root.exists() {
            anyhow::bail!(
                "discovery error: root path does not exist: {}",
                root.display()
            );
        }

        // 1. Discover all source files
        let discovered = discovery::discover_files(root)?;
        info!("discovered {} source files", discovered.len());

        // 2. Parse all files in parallel via Rayon
        let parse_results: Vec<(String, IndexedFile)> = indexing_thread_pool().install(|| {
            discovered
                .par_iter()
                .filter_map(|df| {
                    let bytes = match std::fs::read(&df.absolute_path) {
                        Ok(b) => b,
                        Err(e) => {
                            warn!("failed to read {:?}: {}", df.absolute_path, e);
                            return None;
                        }
                    };

                    let mtime_secs = std::fs::metadata(&df.absolute_path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    let result = parsing::process_file_with_classification(
                        &df.relative_path,
                        &bytes,
                        df.language.clone(),
                        df.classification,
                    );
                    let indexed =
                        IndexedFile::from_parse_result(result, bytes).with_mtime(mtime_secs);
                    Some((df.relative_path.clone(), indexed))
                })
                .collect()
        });

        // 3. Build new file map with fresh circuit breaker
        let new_cb = CircuitBreakerState::from_env();
        let mut new_files: HashMap<String, Arc<IndexedFile>> =
            HashMap::with_capacity(parse_results.len());

        let mut cb_tripped = false;
        for (path, indexed_file) in parse_results {
            match &indexed_file.parse_status {
                ParseStatus::Failed { error } => {
                    new_cb.record_failure(&path, error);
                }
                _ => {
                    new_cb.record_success();
                }
            }

            if new_cb.should_abort() {
                let summary = new_cb.summary();
                error!("{}", summary);
                cb_tripped = true;
                new_files.insert(path, Arc::new(indexed_file));
                break;
            }

            new_files.insert(path, Arc::new(indexed_file));
        }

        if cb_tripped {
            new_cb.tripped.store(true, Ordering::Relaxed);
        }

        let load_duration = start.elapsed();
        info!(
            "LiveIndex::build_reload_data done: {} files, {} symbols, {:?}",
            new_files.len(),
            new_files.values().map(|f| f.symbols.len()).sum::<usize>(),
            load_duration
        );

        // Pre-build all derived indices outside any lock.
        let derived = DerivedIndices::build_from_files(&new_files);

        Ok(ReloadData {
            files: new_files,
            cb_state: new_cb,
            load_duration,
            gitignore: discovery::load_gitignore(root),
            derived,
            // NOTE: reload does not track skipped files; health tier counts
            // will show 0 for Tier 2/3 after reload.
            skipped_files: Vec::new(),
            coupling_store: super::coupling::init_coupling_store(root),
        })
    }

    /// Apply pre-built reload data under the write lock. Pure field assignment —
    /// all derived indices are pre-built in `ReloadData`, so this takes
    /// microseconds instead of milliseconds. Cannot fail.
    pub(crate) fn apply_reload_data(&mut self, data: ReloadData) {
        self.files = data.files;
        self.loaded_at = Instant::now();
        self.loaded_at_system = SystemTime::now();
        self.load_duration = data.load_duration;
        self.cb_state = data.cb_state;
        self.is_empty = false;
        self.load_source = IndexLoadSource::FreshLoad;
        self.snapshot_verify_state = SnapshotVerifyState::NotNeeded;
        self.trigram_index = data.derived.trigram_index;
        self.reverse_index = data.derived.reverse_index;
        self.files_by_basename = data.derived.files_by_basename;
        self.files_by_dir_component = data.derived.files_by_dir_component;
        self.gitignore = data.gitignore;
        self.skipped_files = data.skipped_files;
        self.coupling_store = data.coupling_store;
    }

    /// Replaces all files, resets circuit breaker, and updates timestamps.
    /// On success sets `is_empty = false`. On error the index remains in its previous state
    /// (but partial results may have been loaded).
    ///
    /// NOTE: This method does all I/O under `&mut self`. Prefer calling
    /// `build_reload_data` outside the lock and then `apply_reload_data` under
    /// the lock when called via `SharedIndexHandle::reload`.
    pub fn reload(&mut self, root: &Path) -> anyhow::Result<()> {
        let data = Self::build_reload_data(root)?;
        self.apply_reload_data(data);
        Ok(())
    }

    /// Insert or replace a single file in the index without a full reload.
    ///
    /// Updates `loaded_at_system` to reflect the mutation time.
    /// If the file already exists, its entry is replaced atomically.
    pub fn update_file(&mut self, path: String, file: IndexedFile) {
        // Capture old reference names BEFORE replacing the file, so we can
        // clean up stale reverse index entries after the insert.
        let old_ref_names: Vec<String> = self
            .files
            .get(&path)
            .map(|f| f.references.iter().map(|r| r.name.clone()).collect())
            .unwrap_or_default();
        let had_existing = !old_ref_names.is_empty() || self.files.contains_key(&path);

        // SAFETY: Insert the new file into the primary store FIRST.
        // This ensures the file is always present in `self.files` even if
        // auxiliary index updates panic (e.g., from concurrent access or
        // gitignore assertion failures). Auxiliary indices may become
        // temporarily stale, but the file won't vanish from the index.
        self.files.insert(path.clone(), Arc::new(file));

        // Clean up old auxiliary indices using captured state.
        if had_existing {
            self.remove_path_indices_for_path(&path);
        }
        // Remove old reverse index entries using the captured old reference names
        // (not the new file's references, which are already in self.files).
        for name in &old_ref_names {
            if let Some(locs) = self.reverse_index.get_mut(name) {
                locs.retain(|loc| loc.file_path != path);
                if locs.is_empty() {
                    self.reverse_index.remove(name);
                }
            }
        }
        self.trigram_index
            .update_file(&path, &self.files[&path].content);
        self.insert_reverse_index_for_path(&path);
        self.insert_path_indices_for_path(&path);
        self.is_empty = false;
        self.loaded_at_system = SystemTime::now();
    }

    /// Returns `true` when `relative_path` is excluded by the repository's
    /// gitignore rules, using the same matcher loaded at discovery time.
    ///
    /// This mirrors the `ignore::WalkBuilder` behaviour of the initial scan so
    /// the live watcher never indexes paths the initial walk would have pruned —
    /// most importantly SymForge's own gitignored `.symforge/` state directory
    /// (e.g. `tee/*.rs` edit snapshots), which would otherwise leak into
    /// reference and search results and grow the index unbounded across a
    /// session. Whitelisted paths (such as `.github/` via `!/.github/`) and
    /// committed, non-ignored `vendor/` trees are reported as not ignored.
    pub(crate) fn is_path_gitignored(&self, relative_path: &str) -> bool {
        let Some(gitignore) = self.gitignore.as_ref() else {
            return false;
        };
        // The `ignore` crate asserts that paths are relative; guard against
        // absolute paths that could reach here from unsanitized watcher events.
        if std::path::Path::new(relative_path).has_root() {
            return false;
        }
        gitignore
            .matched_path_or_any_parents(relative_path, false)
            .is_ignore()
    }

    /// Insert a new file into the index (alias for `update_file`).
    ///
    /// Semantically identical to `update_file` — if the file already exists
    /// it is replaced. The name `add_file` is provided for clarity at call sites
    /// where the caller knows the file is new.
    pub fn add_file(&mut self, path: String, file: IndexedFile) {
        self.update_file(path, file);
    }

    /// Remove a single file from the index by its relative path.
    ///
    /// If the path is not present, this is a no-op (no timestamp update).
    /// If the path is found and removed, `loaded_at_system` is updated.
    pub fn remove_file(&mut self, path: &str) {
        self.remove_reverse_index_for_path(path);
        if self.files.remove(path).is_some() {
            self.trigram_index.remove_file(path);
            self.remove_path_indices_for_path(path);
            self.loaded_at_system = SystemTime::now();
        }
    }

    /// Remove reverse index entries for a single file path.
    /// Must be called BEFORE removing the file from `self.files`.
    fn remove_reverse_index_for_path(&mut self, path: &str) {
        if let Some(file) = self.files.get(path) {
            let names: Vec<String> = file.references.iter().map(|r| r.name.clone()).collect();
            for name in names {
                if let Some(locs) = self.reverse_index.get_mut(&name) {
                    locs.retain(|loc| loc.file_path != path);
                    if locs.is_empty() {
                        self.reverse_index.remove(&name);
                    }
                }
            }
        }
    }

    /// Insert reverse index entries for a single file path.
    /// Must be called AFTER inserting the file into `self.files`.
    fn insert_reverse_index_for_path(&mut self, path: &str) {
        if let Some(file) = self.files.get(path) {
            for (reference_idx, reference) in file.references.iter().enumerate() {
                self.reverse_index
                    .entry(reference.name.clone())
                    .or_default()
                    .push(ReferenceLocation {
                        file_path: path.to_string(),
                        reference_idx: reference_idx as u32,
                    });
            }
        }
    }

    /// Rebuild `reverse_index` from scratch using current `self.files`.
    ///
    /// Used by incremental callers (load, snapshot restore, tests).
    /// For bulk reload, prefer `DerivedIndices::build_from_files` outside the lock.
    pub(crate) fn rebuild_reverse_index(&mut self) {
        self.reverse_index = build_reverse_index_from_files(&self.files);
    }

    /// Rebuild path indices (basename + dir component) from current `self.files`.
    ///
    /// Used by incremental callers (load, snapshot restore, tests).
    /// For bulk reload, prefer `DerivedIndices::build_from_files` outside the lock.
    pub(crate) fn rebuild_path_indices(&mut self) {
        let (by_basename, by_dir_component) = build_path_indices_from_files(&self.files);
        self.files_by_basename = by_basename;
        self.files_by_dir_component = by_dir_component;
    }

    fn insert_path_indices_for_path(&mut self, path: &str) {
        if let Some(basename) = basename_key(path) {
            insert_sorted_unique(self.files_by_basename.entry(basename).or_default(), path);
        }

        for component in dir_component_keys(path) {
            insert_sorted_unique(
                self.files_by_dir_component.entry(component).or_default(),
                path,
            );
        }
    }

    fn remove_path_indices_for_path(&mut self, path: &str) {
        if let Some(basename) = basename_key(path)
            && let Some(paths) = self.files_by_basename.get_mut(&basename)
        {
            remove_sorted_path(paths, path);
            if paths.is_empty() {
                self.files_by_basename.remove(&basename);
            }
        }

        for component in dir_component_keys(path) {
            if let Some(paths) = self.files_by_dir_component.get_mut(&component) {
                remove_sorted_path(paths, path);
                if paths.is_empty() {
                    self.files_by_dir_component.remove(&component);
                }
            }
        }
    }

    /// Returns where the current in-memory contents came from.
    pub fn load_source(&self) -> IndexLoadSource {
        self.load_source
    }

    /// Returns the current snapshot reconciliation state.
    pub fn snapshot_verify_state(&self) -> SnapshotVerifyState {
        self.snapshot_verify_state.clone()
    }

    pub(crate) fn mark_snapshot_verify_running(&mut self) {
        if self.load_source == IndexLoadSource::SnapshotRestore {
            self.snapshot_verify_state = SnapshotVerifyState::Running;
        }
    }

    pub(crate) fn mark_snapshot_verify_completed(&mut self, mismatched_paths: Vec<String>) {
        if self.load_source == IndexLoadSource::SnapshotRestore {
            self.snapshot_verify_state =
                SnapshotVerifyState::completed_with_mismatches(mismatched_paths);
        }
    }
}

fn basename_key(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
}

fn dir_component_keys(path: &str) -> Vec<String> {
    let components: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect();
    if components.len() <= 1 {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut keys = Vec::new();
    for component in &components[..components.len() - 1] {
        let key = component.to_ascii_lowercase();
        if seen.insert(key.clone()) {
            keys.push(key);
        }
    }
    keys.sort();
    keys
}

fn insert_sorted_unique(paths: &mut Vec<String>, path: &str) {
    match paths.binary_search_by(|existing| existing.as_str().cmp(path)) {
        Ok(_) => {}
        Err(pos) => paths.insert(pos, path.to_string()),
    }
}

fn remove_sorted_path(paths: &mut Vec<String>, path: &str) {
    if let Ok(pos) = paths.binary_search_by(|existing| existing.as_str().cmp(path)) {
        paths.remove(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        FileOutcome, LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord,
    };
    use std::fs;
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    static COUPLING_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    struct CouplingEnvGuard {
        previous: Option<String>,
    }

    #[allow(unsafe_code)] // test-only env guard serializes coupling flag mutation.
    impl CouplingEnvGuard {
        fn set(value: Option<&str>) -> Self {
            let previous =
                std::env::var(crate::live_index::coupling::lifecycle::COUPLING_FLAG_ENV).ok();
            // SAFETY: callers hold COUPLING_ENV_LOCK; relevant tests run single-threaded.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(
                        crate::live_index::coupling::lifecycle::COUPLING_FLAG_ENV,
                        value,
                    ),
                    None => std::env::remove_var(
                        crate::live_index::coupling::lifecycle::COUPLING_FLAG_ENV,
                    ),
                }
            }
            Self { previous }
        }
    }

    #[allow(unsafe_code)] // test-only env guard restores serialized coupling flag mutation.
    impl Drop for CouplingEnvGuard {
        fn drop(&mut self) {
            // SAFETY: callers hold COUPLING_ENV_LOCK; relevant tests run single-threaded.
            unsafe {
                match self.previous.as_deref() {
                    Some(value) => std::env::set_var(
                        crate::live_index::coupling::lifecycle::COUPLING_FLAG_ENV,
                        value,
                    ),
                    None => std::env::remove_var(
                        crate::live_index::coupling::lifecycle::COUPLING_FLAG_ENV,
                    ),
                }
            }
        }
    }

    fn dummy_symbol() -> SymbolRecord {
        let byte_range = (0, 10);
        SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (0, 1),
            doc_byte_range: None,
        }
    }

    fn make_result(outcome: FileOutcome, symbols: Vec<SymbolRecord>) -> FileProcessingResult {
        FileProcessingResult {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("test.rs"),
            outcome,
            parse_diagnostic: None,
            symbols,
            byte_len: 42,
            content_hash: "abc123".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
        }
    }

    // --- IndexedFile::from_parse_result ---

    #[test]
    fn test_indexed_file_maps_processed_status() {
        let result = make_result(FileOutcome::Processed, vec![dummy_symbol()]);
        let indexed = IndexedFile::from_parse_result(result, b"fn foo() {}".to_vec());
        assert_eq!(indexed.parse_status, ParseStatus::Parsed);
        assert_eq!(indexed.symbols.len(), 1);
    }

    #[test]
    fn test_indexed_file_maps_partial_parse_keeps_symbols() {
        let result = make_result(
            FileOutcome::PartialParse {
                warning: "syntax error".to_string(),
            },
            vec![dummy_symbol()],
        );
        let indexed = IndexedFile::from_parse_result(result, b"fn bad(".to_vec());
        assert!(matches!(
            indexed.parse_status,
            ParseStatus::PartialParse { .. }
        ));
        assert_eq!(
            indexed.symbols.len(),
            1,
            "symbols kept even on partial parse"
        );
    }

    #[test]
    fn test_indexed_file_maps_failed_status_empty_symbols_content_preserved() {
        let result = make_result(
            FileOutcome::Failed {
                error: "parse failed".to_string(),
            },
            vec![],
        );
        let content = b"some content bytes".to_vec();
        let indexed = IndexedFile::from_parse_result(result, content.clone());
        assert!(matches!(indexed.parse_status, ParseStatus::Failed { .. }));
        assert!(indexed.symbols.is_empty(), "failed parse has no symbols");
        assert_eq!(
            indexed.content, content,
            "content bytes stored even on failure"
        );
    }

    // --- CircuitBreakerState ---

    #[test]
    fn test_circuit_breaker_does_not_trip_at_20pct_of_10_files() {
        // 20% of 10 = exactly threshold — NOT exceeded
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..8 {
            cb.record_success();
        }
        for i in 0..2 {
            cb.record_failure(&format!("file{i}.rs"), "error");
        }
        assert!(
            !cb.should_abort(),
            "2/10 = 20% should NOT trip (threshold not exceeded)"
        );
    }

    #[test]
    fn test_circuit_breaker_trips_at_30pct_of_10_files() {
        // 30% > 20% threshold — SHOULD trip
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("file{i}.rs"), "error");
        }
        assert!(cb.should_abort(), "3/10 = 30% should trip");
    }

    #[test]
    fn test_circuit_breaker_does_not_trip_on_tiny_repos() {
        // Fewer than 5 files processed — minimum-file guard must prevent tripping
        let cb = CircuitBreakerState::new(0.20);
        cb.record_failure("a.rs", "err");
        cb.record_failure("b.rs", "err");
        cb.record_failure("c.rs", "err");
        // 3 total, all failed — but < 5 minimum threshold
        assert!(
            !cb.should_abort(),
            "< 5 files processed: circuit breaker must not trip"
        );
    }

    #[test]
    fn test_circuit_breaker_threshold_configurable() {
        // Use a strict threshold of 0.10 (10%)
        let cb = CircuitBreakerState::new(0.10);
        for _ in 0..9 {
            cb.record_success();
        }
        cb.record_failure("file.rs", "error");
        // 1/10 = 10% = threshold, NOT exceeded
        assert!(!cb.should_abort(), "10% == threshold, not exceeded");

        // Now one more failure puts it at 2/11 ~ 18.2% > 10% — but we add 1 more success first
        let cb2 = CircuitBreakerState::new(0.10);
        for _ in 0..8 {
            cb2.record_success();
        }
        for i in 0..2 {
            cb2.record_failure(&format!("file{i}.rs"), "error");
        }
        // 2/10 = 20% > 10% threshold
        assert!(cb2.should_abort(), "20% > 10% threshold should trip");
    }

    // --- LiveIndex::load ---

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_live_index_load_valid_files_produces_ready_state() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn alpha() {}");
        write_file(tmp.path(), "b.py", "def beta(): pass");
        write_file(tmp.path(), "c.js", "function gamma() {}");
        write_file(tmp.path(), "d.ts", "function delta(): void {}");
        write_file(tmp.path(), "e.go", "package main\nfunc epsilon() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read();
        assert!(
            !index.cb_state.is_tripped(),
            "valid files should not trip circuit breaker"
        );
        assert_eq!(index.file_count(), 5);
        assert_eq!(index.load_source(), IndexLoadSource::FreshLoad);
        assert_eq!(
            index.snapshot_verify_state(),
            SnapshotVerifyState::NotNeeded
        );
    }

    #[test]
    fn coupling_store_accessor_is_none_when_flag_unset() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        let _env = CouplingEnvGuard::set(None);
        let tmp = TempDir::new().unwrap();
        git2::Repository::init(tmp.path()).unwrap();
        write_file(tmp.path(), "src/lib.rs", "pub fn alpha() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
        assert!(shared.read().coupling_store().is_none());
        assert!(
            !db_path.exists(),
            "flag-off load must not create the coupling database"
        );
    }

    #[test]
    fn coupling_store_accessor_is_some_when_flag_enabled_for_git_workspace() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        let _env = CouplingEnvGuard::set(Some("1"));
        let tmp = TempDir::new().unwrap();
        git2::Repository::init(tmp.path()).unwrap();
        write_file(tmp.path(), "src/lib.rs", "pub fn alpha() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read();
        let store = index
            .coupling_store()
            .expect("flag-on git workspace should expose coupling store");
        assert_eq!(
            store.schema_version().unwrap(),
            crate::live_index::coupling::schema::CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn test_live_index_load_circuit_breaker_not_tripped_with_all_languages() {
        // All 16 languages now parse successfully (tree-sitter 0.26 + ABI-compatible grammars).
        // A mix of language files should not trip the circuit breaker.
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn alpha() {}");
        write_file(tmp.path(), "b.py", "def beta(): pass");
        write_file(tmp.path(), "c.js", "function gamma() {}");
        // Swift, PHP, Perl now parse successfully — CB should not trip
        write_file(tmp.path(), "x.swift", "class A {}");
        write_file(tmp.path(), "y.php", "<?php class B {}");
        write_file(tmp.path(), "z.pl", "sub greet { print \"hi\"; }");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read();
        assert!(
            !index.cb_state.is_tripped(),
            "all-parseable files should not trip circuit breaker"
        );
    }

    #[test]
    fn test_live_index_file_count() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn a() {}");
        write_file(tmp.path(), "b.rs", "fn b() {}");
        write_file(tmp.path(), "c.rs", "fn c() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read();
        assert_eq!(index.file_count(), 3);
    }

    #[test]
    fn test_live_index_symbol_count() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn foo() {}\nfn bar() {}");
        write_file(tmp.path(), "b.rs", "fn baz() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read();
        // a.rs: 2 symbols, b.rs: 1 symbol → total 3
        assert_eq!(index.symbol_count(), 3);
    }

    // --- LiveIndex::empty() and reload() ---

    #[test]
    fn test_live_index_empty_has_zero_files() {
        let shared = LiveIndex::empty();
        let index = shared.read();
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.load_source(), IndexLoadSource::EmptyBootstrap);
        assert_eq!(
            index.snapshot_verify_state(),
            SnapshotVerifyState::NotNeeded
        );
    }

    #[test]
    fn test_shared_index_handle_preserves_read_write_access() {
        let shared = LiveIndex::empty();
        {
            let mut live = shared.write();
            live.add_file(
                "src/new.rs".to_string(),
                make_indexed_file_for_mutation("src/new.rs"),
            );
        }

        let index = shared.read();
        assert!(index.get_file("src/new.rs").is_some());
    }

    #[test]
    fn test_shared_index_handle_published_state_tracks_generation_and_counts() {
        let shared = LiveIndex::empty();
        let initial = shared.published_state();
        assert_eq!(initial.generation, 0);
        assert_eq!(initial.status, PublishedIndexStatus::Empty);
        assert_eq!(initial.degraded_summary, None);
        assert_eq!(initial.file_count, 0);
        assert_eq!(initial.parsed_count, 0);
        assert_eq!(initial.partial_parse_count, 0);
        assert_eq!(initial.failed_count, 0);
        assert_eq!(initial.load_source, IndexLoadSource::EmptyBootstrap);

        shared.add_file(
            "src/new.rs".to_string(),
            make_indexed_file_for_mutation("src/new.rs"),
        );
        let after_add = shared.published_state();
        assert_eq!(after_add.generation, 1);
        assert_eq!(after_add.status, PublishedIndexStatus::Ready);
        assert_eq!(after_add.degraded_summary, None);
        assert_eq!(after_add.file_count, 1);
        assert_eq!(after_add.parsed_count, 1);
        assert_eq!(after_add.partial_parse_count, 0);
        assert_eq!(after_add.failed_count, 0);
        assert_eq!(after_add.symbol_count, 1);

        shared.remove_file("src/new.rs");
        let after_remove = shared.published_state();
        assert_eq!(after_remove.generation, 2);
        assert_eq!(after_remove.status, PublishedIndexStatus::Ready);
        assert_eq!(after_remove.degraded_summary, None);
        assert_eq!(after_remove.file_count, 0);
        assert_eq!(after_remove.symbol_count, 0);
    }

    #[test]
    fn test_reset_to_empty_invalidates_populated_index_and_bumps_generation() {
        // Populate a handle with a file (simulating a stale OLD-project local index).
        let shared = LiveIndex::empty();
        shared.add_file(
            "src/old_project.rs".to_string(),
            make_indexed_file_for_mutation("src/old_project.rs"),
        );
        let before = shared.published_state();
        assert_eq!(before.file_count, 1, "precondition: index has stale file");
        let project_gen_before = shared.current_project_generation();

        // Reset (the operation index_folder's daemon branch now performs on switch).
        shared.reset_to_empty();

        let after = shared.published_state();
        assert_eq!(
            after.file_count, 0,
            "reset_to_empty must drop all indexed files so ensure_local_index reloads the new root"
        );
        assert_eq!(
            after.symbol_count, 0,
            "reset_to_empty must drop all symbols"
        );
        assert_eq!(
            after.status,
            PublishedIndexStatus::Empty,
            "reset_to_empty must publish Empty status"
        );
        assert_eq!(
            after.load_source,
            IndexLoadSource::EmptyBootstrap,
            "reset_to_empty must mark the index as a fresh empty bootstrap"
        );
        assert!(
            shared.read().get_file("src/old_project.rs").is_none(),
            "stale file must be unreachable after reset"
        );
        assert!(
            shared.current_project_generation() > project_gen_before,
            "reset_to_empty must bump project generation to fence stale watcher mutations"
        );
    }

    #[test]
    fn rejected_stale_mutations_counter_increments_on_fence_rejection() {
        let dir_a = TempDir::new().unwrap();
        write_file(dir_a.path(), "src/a.rs", "pub fn from_a() {}\n");
        let shared = LiveIndex::load(dir_a.path()).unwrap();
        let gen_a = shared.current_project_generation();

        assert_eq!(shared.current_rejected_stale_mutations(), 0);

        let dir_b = TempDir::new().unwrap();
        write_file(dir_b.path(), "src/b.rs", "pub fn from_b() {}\n");
        shared.reload(dir_b.path()).unwrap();

        assert!(
            shared.current_project_generation() > gen_a,
            "reload must advance project generation before stale mutations are checked"
        );
        assert!(!shared.remove_file_at_generation("src/a.rs", gen_a));
        assert_eq!(shared.current_rejected_stale_mutations(), 1);

        let indexed = make_indexed_file_for_mutation("src/stale.rs");
        assert!(!shared.update_file_at_generation("src/stale.rs", indexed, gen_a));
        assert_eq!(shared.current_rejected_stale_mutations(), 2);
    }

    #[test]
    fn test_shared_index_handle_write_guard_publishes_on_drop() {
        let shared = LiveIndex::empty();

        {
            let mut live = shared.write();
            live.add_file(
                "src/new.rs".to_string(),
                make_indexed_file_for_mutation("src/new.rs"),
            );
        }

        let after_add = shared.published_state();
        assert_eq!(after_add.generation, 1);
        assert_eq!(after_add.status, PublishedIndexStatus::Ready);
        assert_eq!(after_add.degraded_summary, None);
        assert_eq!(after_add.file_count, 1);

        {
            let mut live = shared.write();
            live.remove_file("src/new.rs");
        }

        let after_remove = shared.published_state();
        assert_eq!(after_remove.generation, 2);
        assert_eq!(after_remove.status, PublishedIndexStatus::Ready);
        assert_eq!(after_remove.degraded_summary, None);
        assert_eq!(after_remove.file_count, 0);
    }

    #[test]
    fn test_shared_index_handle_published_state_tracks_verify_transitions() {
        let mut live = make_empty_live_index();
        live.is_empty = false;
        live.load_source = IndexLoadSource::SnapshotRestore;
        live.snapshot_verify_state = SnapshotVerifyState::Pending;
        let shared = SharedIndexHandle::shared(live);

        let initial = shared.published_state();
        assert_eq!(initial.file_count, 0);
        assert_eq!(initial.partial_parse_count, 0);
        assert_eq!(initial.failed_count, 0);

        shared.mark_snapshot_verify_running();
        let running = shared.published_state();
        assert_eq!(running.generation, 1);
        assert_eq!(running.status, PublishedIndexStatus::Ready);
        assert_eq!(running.degraded_summary, None);
        assert_eq!(running.snapshot_verify_state, SnapshotVerifyState::Running);
        assert_eq!(running.file_count, initial.file_count);
        assert_eq!(running.partial_parse_count, initial.partial_parse_count);
        assert_eq!(running.failed_count, initial.failed_count);

        shared.mark_snapshot_verify_completed(Vec::new());
        let completed = shared.published_state();
        assert_eq!(completed.generation, 2);
        assert_eq!(
            completed.snapshot_verify_state,
            SnapshotVerifyState::completed_without_mismatches()
        );
        assert_eq!(completed.file_count, initial.file_count);
        assert_eq!(completed.partial_parse_count, initial.partial_parse_count);
        assert_eq!(completed.failed_count, initial.failed_count);
    }

    #[test]
    fn test_shared_index_handle_published_state_bounds_snapshot_verify_mismatch_paths() {
        let mut live = make_empty_live_index();
        live.is_empty = false;
        live.load_source = IndexLoadSource::SnapshotRestore;
        live.snapshot_verify_state = SnapshotVerifyState::Pending;
        let shared = SharedIndexHandle::shared(live);

        let mismatch_paths = (0..12)
            .rev()
            .map(|i| format!("src/mismatch_{i:02}.rs"))
            .collect::<Vec<_>>();
        shared.mark_snapshot_verify_completed(mismatch_paths);

        let completed = shared.published_state();
        match &completed.snapshot_verify_state {
            SnapshotVerifyState::Completed(report) => {
                assert_eq!(report.mismatch_count, 12);
                assert_eq!(report.mismatched_paths.len(), 10);
                assert_eq!(report.mismatched_paths[0], "src/mismatch_00.rs");
                assert_eq!(report.mismatched_paths[9], "src/mismatch_09.rs");
                assert_eq!(report.omitted_path_count(), 2);
            }
            other => panic!("expected completed snapshot verify report, got {other:?}"),
        }
    }

    #[test]
    fn test_shared_index_handle_published_state_captures_degraded_summary() {
        let mut live = make_empty_live_index();
        live.is_empty = false;
        for _ in 0..3 {
            live.cb_state.record_failure("src/bad.rs", "parse failure");
        }
        for _ in 0..7 {
            live.cb_state.record_success();
        }
        assert!(live.cb_state.should_abort(), "circuit breaker should trip");
        let shared = SharedIndexHandle::shared(live);

        let published = shared.published_state();
        assert_eq!(published.status, PublishedIndexStatus::Degraded);
        assert!(
            published
                .degraded_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("circuit breaker tripped")),
            "expected degraded summary, got {:?}",
            published.degraded_summary
        );
    }

    #[test]
    fn test_shared_index_handle_published_repo_outline_tracks_mutations() {
        let shared = LiveIndex::empty();

        let initial = shared.published_repo_outline();
        assert_eq!(initial.total_files, 0);
        assert_eq!(initial.total_symbols, 0);
        assert!(initial.files.is_empty());

        shared.add_file(
            "src/main.rs".to_string(),
            make_indexed_file_for_mutation("src/main.rs"),
        );
        let after_add = shared.published_repo_outline();
        assert_eq!(after_add.total_files, 1);
        assert_eq!(after_add.total_symbols, 1);
        assert_eq!(after_add.files[0].relative_path, "src/main.rs");

        {
            let mut live = shared.write();
            live.remove_file("src/main.rs");
        }
        let after_remove = shared.published_repo_outline();
        assert_eq!(after_remove.total_files, 0);
        assert_eq!(after_remove.total_symbols, 0);
        assert!(after_remove.files.is_empty());
    }

    #[test]
    fn test_live_index_empty_returns_empty_state() {
        let shared = LiveIndex::empty();
        let index = shared.read();
        assert_eq!(index.index_state(), IndexState::Empty);
    }

    #[test]
    fn test_live_index_empty_is_not_ready() {
        let shared = LiveIndex::empty();
        let index = shared.read();
        assert!(!index.is_ready(), "empty index should not be ready");
    }

    #[test]
    fn test_live_index_reload_loads_files_and_becomes_ready() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn alpha() {}");
        write_file(tmp.path(), "b.rs", "fn beta() {}");

        let shared = LiveIndex::empty();
        {
            let mut index = shared.write();
            index.reload(tmp.path()).expect("reload should succeed");
        }
        let index = shared.read();
        assert_eq!(index.file_count(), 2);
        assert!(index.is_ready(), "after reload should be ready");
        assert_eq!(index.index_state(), IndexState::Ready);
        assert_eq!(index.load_source(), IndexLoadSource::FreshLoad);
        assert_eq!(
            index.snapshot_verify_state(),
            SnapshotVerifyState::NotNeeded
        );
    }

    #[test]
    fn test_live_index_reload_invalid_root_returns_error() {
        let shared = LiveIndex::empty();
        let mut index = shared.write();
        let result = index.reload(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(
            result.is_err(),
            "reload on invalid root should return error"
        );
    }

    #[test]
    fn test_live_index_loaded_at_system_is_recent() {
        use std::time::SystemTime;
        let before = SystemTime::now();
        let shared = LiveIndex::empty();
        let index = shared.read();
        let after = SystemTime::now();
        let ts = index.loaded_at_system();
        assert!(
            ts >= before,
            "loaded_at_system should be >= before creation"
        );
        assert!(ts <= after, "loaded_at_system should be <= after creation");
    }

    #[test]
    fn test_concurrent_readers_no_deadlock() {
        use std::thread;

        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn foo() {}");
        write_file(tmp.path(), "b.rs", "fn bar() {}");
        write_file(tmp.path(), "c.rs", "fn baz() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let shared_clone = Arc::clone(&shared);
                thread::spawn(move || {
                    let index = shared_clone.read();
                    let _ = index.file_count();
                    let _ = index.symbol_count();
                })
            })
            .collect();

        for h in handles {
            h.join().expect("reader thread should not panic");
        }
    }

    // --- LiveIndex mutation methods ---

    fn make_indexed_file_for_mutation(path: &str) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols: vec![dummy_symbol()],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 12,
            content_hash: "abc123".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        }
    }

    fn make_empty_live_index() -> LiveIndex {
        LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    #[test]
    fn test_live_index_load_builds_path_indices() {
        let dir = TempDir::new().expect("failed to create tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");
        fs::create_dir_all(dir.path().join("tests")).expect("failed to create tests dir");
        write_file(dir.path(), "src/lib.rs", "pub fn lib_fn() {}");
        write_file(dir.path(), "tests/lib.rs", "fn test_lib() {}");

        let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
        let index = shared.read();

        assert_eq!(
            index.files_by_basename.get("lib.rs"),
            Some(&vec!["src/lib.rs".to_string(), "tests/lib.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("src"),
            Some(&vec!["src/lib.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("tests"),
            Some(&vec!["tests/lib.rs".to_string()])
        );
    }

    #[test]
    fn test_live_index_reload_rebuilds_path_indices() {
        let dir = TempDir::new().expect("failed to create tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");
        write_file(dir.path(), "src/alpha.rs", "fn alpha() {}");

        let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");

        fs::remove_file(dir.path().join("src/alpha.rs")).expect("failed to remove alpha");
        fs::create_dir_all(dir.path().join("tests")).expect("failed to create tests dir");
        write_file(dir.path(), "tests/beta.rs", "fn beta() {}");

        {
            let mut index = shared.write();
            index.reload(dir.path()).expect("reload should succeed");
        }

        let index = shared.read();
        assert!(!index.files_by_basename.contains_key("alpha.rs"));
        assert_eq!(
            index.files_by_basename.get("beta.rs"),
            Some(&vec!["tests/beta.rs".to_string()])
        );
        assert!(!index.files_by_dir_component.contains_key("src"));
        assert_eq!(
            index.files_by_dir_component.get("tests"),
            Some(&vec!["tests/beta.rs".to_string()])
        );
    }

    #[test]
    fn test_dir_component_keys_deduplicate_and_accept_backslashes() {
        assert_eq!(
            dir_component_keys("src\\live_index\\src\\store.rs"),
            vec!["live_index".to_string(), "src".to_string()]
        );
    }

    #[test]
    fn test_update_file_inserts_and_updates_timestamp() {
        let mut index = make_empty_live_index();
        let before = SystemTime::now();
        let file = make_indexed_file_for_mutation("src/new.rs");
        index.update_file("src/new.rs".to_string(), file);
        let after = SystemTime::now();

        assert!(
            index.get_file("src/new.rs").is_some(),
            "file should be inserted"
        );
        assert_eq!(
            index.files_by_basename.get("new.rs"),
            Some(&vec!["src/new.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("src"),
            Some(&vec!["src/new.rs".to_string()])
        );
        let ts = index.loaded_at_system;
        assert!(ts >= before, "loaded_at_system should be >= before update");
        assert!(ts <= after, "loaded_at_system should be <= after update");
    }

    #[test]
    fn test_update_file_replaces_existing() {
        let mut index = make_empty_live_index();
        let file1 = IndexedFile {
            relative_path: "src/foo.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/foo.rs"),
            content: b"fn old() {}".to_vec(),
            symbols: vec![],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 11,
            content_hash: "old_hash".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        };
        index.update_file("src/foo.rs".to_string(), file1);

        let file2 = IndexedFile {
            relative_path: "src/foo.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/foo.rs"),
            content: b"fn new() {}".to_vec(),
            symbols: vec![dummy_symbol()],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 11,
            content_hash: "new_hash".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        };
        index.update_file("src/foo.rs".to_string(), file2);

        let retrieved = index.get_file("src/foo.rs").unwrap();
        assert_eq!(
            retrieved.content_hash, "new_hash",
            "should have replaced the file"
        );
        assert_eq!(index.file_count(), 1, "should still have exactly 1 file");
        assert_eq!(
            index.files_by_basename.get("foo.rs"),
            Some(&vec!["src/foo.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("src"),
            Some(&vec!["src/foo.rs".to_string()])
        );
    }

    #[test]
    fn test_add_file_inserts_new() {
        let mut index = make_empty_live_index();
        assert_eq!(index.file_count(), 0);

        let file = make_indexed_file_for_mutation("src/new.rs");
        index.add_file("src/new.rs".to_string(), file);

        assert_eq!(
            index.file_count(),
            1,
            "file count should increase by 1 after add_file"
        );
        assert!(index.get_file("src/new.rs").is_some());
    }

    #[test]
    fn test_remove_file_removes_existing() {
        let mut index = make_empty_live_index();
        let file = make_indexed_file_for_mutation("src/to_delete.rs");
        index.update_file("src/to_delete.rs".to_string(), file);
        assert_eq!(index.file_count(), 1);

        index.remove_file("src/to_delete.rs");
        assert!(
            index.get_file("src/to_delete.rs").is_none(),
            "file should be removed"
        );
        assert_eq!(index.file_count(), 0);
        assert!(!index.files_by_basename.contains_key("to_delete.rs"));
        assert!(!index.files_by_dir_component.contains_key("src"));
    }

    #[test]
    fn test_remove_file_nonexistent_is_noop() {
        let mut index = make_empty_live_index();
        // Set a known timestamp
        let known_ts = index.loaded_at_system;
        // Small sleep to ensure any timestamp update would be different
        std::thread::sleep(Duration::from_millis(5));

        index.remove_file("nonexistent.rs");

        assert_eq!(
            index.loaded_at_system, known_ts,
            "loaded_at_system must NOT change when removing non-existent file"
        );
    }

    #[test]
    fn test_file_count_after_mutations() {
        let mut index = make_empty_live_index();
        assert_eq!(index.file_count(), 0);

        index.add_file("a.rs".to_string(), make_indexed_file_for_mutation("a.rs"));
        assert_eq!(index.file_count(), 1);

        index.add_file("b.rs".to_string(), make_indexed_file_for_mutation("b.rs"));
        assert_eq!(index.file_count(), 2);

        index.update_file("a.rs".to_string(), make_indexed_file_for_mutation("a.rs"));
        assert_eq!(index.file_count(), 2, "update does not add a new entry");

        index.remove_file("a.rs");
        assert_eq!(index.file_count(), 1);

        index.remove_file("nonexistent.rs");
        assert_eq!(
            index.file_count(),
            1,
            "removing nonexistent does not change count"
        );
    }

    // --- Cross-reference fields and reverse index ---

    fn make_ref(name: &str, kind: ReferenceKind, line: u32) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind,
            byte_range: (0, 1),
            line_range: (line, line),
            enclosing_symbol_index: None,
        }
    }

    fn make_indexed_file_with_refs(path: &str, refs: Vec<ReferenceRecord>) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols: vec![],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        }
    }

    #[test]
    fn test_indexed_file_from_parse_result_transfers_refs_and_alias_map() {
        use std::collections::HashMap;
        let mut alias_map = HashMap::new();
        alias_map.insert("Map".to_string(), "HashMap".to_string());
        let refs = vec![make_ref("foo", ReferenceKind::Call, 1)];

        let result = FileProcessingResult {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("test.rs"),
            outcome: FileOutcome::Processed,
            parse_diagnostic: None,
            symbols: vec![],
            byte_len: 0,
            content_hash: "abc".to_string(),
            references: refs.clone(),
            alias_map: alias_map.clone(),
        };

        let indexed = IndexedFile::from_parse_result(result, vec![]);
        assert_eq!(indexed.references.len(), 1);
        assert_eq!(indexed.references[0].name, "foo");
        assert_eq!(
            indexed.alias_map.get("Map").map(|s| s.as_str()),
            Some("HashMap")
        );
    }

    #[test]
    fn test_rebuild_reverse_index_builds_name_to_locations() {
        let mut index = make_empty_live_index();

        let refs_a = vec![
            make_ref("process", ReferenceKind::Call, 5),
            make_ref("load", ReferenceKind::Call, 10),
        ];
        let refs_b = vec![make_ref("process", ReferenceKind::Call, 3)];

        index.add_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a),
        );
        index.add_file(
            "b.rs".to_string(),
            make_indexed_file_with_refs("b.rs", refs_b),
        );

        // process appears in both files
        let locs = index
            .reverse_index
            .get("process")
            .expect("process should be in reverse index");
        assert_eq!(locs.len(), 2, "process referenced in 2 files");

        // load appears only in a.rs
        let locs_load = index
            .reverse_index
            .get("load")
            .expect("load should be in reverse index");
        assert_eq!(locs_load.len(), 1);
        assert_eq!(locs_load[0].file_path, "a.rs");
        assert_eq!(locs_load[0].reference_idx, 1);
    }

    #[test]
    fn test_rebuild_reverse_index_consistent_after_update_file() {
        let mut index = make_empty_live_index();

        let refs_old = vec![make_ref("old_func", ReferenceKind::Call, 1)];
        index.add_file(
            "src.rs".to_string(),
            make_indexed_file_with_refs("src.rs", refs_old),
        );
        assert!(index.reverse_index.contains_key("old_func"));

        let refs_new = vec![make_ref("new_func", ReferenceKind::Call, 1)];
        index.update_file(
            "src.rs".to_string(),
            make_indexed_file_with_refs("src.rs", refs_new),
        );

        assert!(
            !index.reverse_index.contains_key("old_func"),
            "stale entry should be gone"
        );
        assert!(
            index.reverse_index.contains_key("new_func"),
            "new entry should be present"
        );
    }

    #[test]
    fn test_rebuild_reverse_index_excludes_removed_file() {
        let mut index = make_empty_live_index();

        let refs = vec![make_ref("target_fn", ReferenceKind::Call, 2)];
        index.add_file(
            "will_delete.rs".to_string(),
            make_indexed_file_with_refs("will_delete.rs", refs),
        );
        assert!(index.reverse_index.contains_key("target_fn"));

        index.remove_file("will_delete.rs");
        assert!(
            !index.reverse_index.contains_key("target_fn"),
            "removed file's refs should be gone"
        );
    }

    #[test]
    fn test_reference_location_fields() {
        let loc = ReferenceLocation {
            file_path: "src/main.rs".to_string(),
            reference_idx: 3,
        };
        assert_eq!(loc.file_path, "src/main.rs");
        assert_eq!(loc.reference_idx, 3);
    }

    #[test]
    fn test_empty_live_index_has_empty_reverse_index() {
        let index = make_empty_live_index();
        assert!(
            index.reverse_index.is_empty(),
            "fresh index should have empty reverse index"
        );
    }

    #[test]
    fn test_incremental_reverse_index_matches_full_rebuild() {
        let mut index = make_empty_live_index();

        // Add two files with overlapping references
        let refs_a = vec![
            make_ref("shared_fn", ReferenceKind::Call, 1),
            make_ref("only_a", ReferenceKind::Call, 5),
        ];
        let refs_b = vec![
            make_ref("shared_fn", ReferenceKind::Call, 2),
            make_ref("only_b", ReferenceKind::Call, 8),
        ];
        index.add_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a),
        );
        index.add_file(
            "b.rs".to_string(),
            make_indexed_file_with_refs("b.rs", refs_b),
        );

        // Update a.rs with new references (triggers incremental update)
        let refs_a_new = vec![
            make_ref("shared_fn", ReferenceKind::Call, 1),
            make_ref("replaced_a", ReferenceKind::Call, 10),
        ];
        index.update_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a_new),
        );

        // Snapshot the incremental result
        let incremental: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        // Now do a full rebuild and compare
        index.rebuild_reverse_index();
        let full_rebuild: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        assert_eq!(
            incremental, full_rebuild,
            "incremental update should produce same result as full rebuild"
        );

        // Verify specific expectations
        assert!(
            !index.reverse_index.contains_key("only_a"),
            "only_a should be gone after update"
        );
        assert!(
            index.reverse_index.contains_key("replaced_a"),
            "replaced_a should be present"
        );
        assert!(
            index.reverse_index.contains_key("only_b"),
            "only_b should still be present from b.rs"
        );
        let shared = index.reverse_index.get("shared_fn").unwrap();
        assert_eq!(shared.len(), 2, "shared_fn still referenced in both files");
    }

    #[test]
    fn test_incremental_reverse_index_remove() {
        let mut index = make_empty_live_index();

        let refs_a = vec![
            make_ref("common", ReferenceKind::Call, 1),
            make_ref("unique_a", ReferenceKind::Call, 3),
        ];
        let refs_b = vec![
            make_ref("common", ReferenceKind::Call, 2),
            make_ref("unique_b", ReferenceKind::Call, 4),
        ];
        index.add_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a),
        );
        index.add_file(
            "b.rs".to_string(),
            make_indexed_file_with_refs("b.rs", refs_b),
        );

        // Remove a.rs
        index.remove_file("a.rs");

        // unique_a should be gone entirely
        assert!(
            !index.reverse_index.contains_key("unique_a"),
            "unique_a should be removed with a.rs"
        );
        // unique_b should remain
        assert!(
            index.reverse_index.contains_key("unique_b"),
            "unique_b should survive"
        );
        // common should only have b.rs
        let common_locs = index
            .reverse_index
            .get("common")
            .expect("common should still exist from b.rs");
        assert_eq!(common_locs.len(), 1);
        assert_eq!(common_locs[0].file_path, "b.rs");

        // Verify incremental matches full rebuild
        let incremental: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        index.rebuild_reverse_index();
        let full_rebuild: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        assert_eq!(
            incremental, full_rebuild,
            "incremental remove should match full rebuild"
        );
    }

    // --- CR2: circuit-breaker determinism test ---

    #[test]
    fn test_circuit_breaker_deterministic_after_sort() {
        // Simulate what the store does: collect parse results from par_iter (nondeterministic
        // order), sort by path, then walk sequentially recording success/failure.
        // We verify that two different orderings of the same results, after sorting,
        // produce the same trip point.

        // 10 entries: "a/f00.rs"–"a/f04.rs" succeed, "a/f05.rs"–"a/f09.rs" fail (50% failure).
        // After sorting alphabetically the failures are always in positions 5-9.
        // The circuit breaker threshold is 20%, min-file guard is 5.
        // After processing f05 (6 total, 1 fail so far) rate=16% → no trip.
        // After processing f06 (7 total, 2 fail) rate=28% → trips.

        let mut results: Vec<(String, bool)> = vec![
            ("a/f00.rs".to_string(), true),
            ("a/f01.rs".to_string(), true),
            ("a/f02.rs".to_string(), true),
            ("a/f03.rs".to_string(), true),
            ("a/f04.rs".to_string(), true),
            ("a/f05.rs".to_string(), false),
            ("a/f06.rs".to_string(), false),
            ("a/f07.rs".to_string(), false),
            ("a/f08.rs".to_string(), false),
            ("a/f09.rs".to_string(), false),
        ];

        // Helper: run CB logic over a slice and return the path where it tripped.
        let run_cb = |items: &[(String, bool)]| -> Option<String> {
            let cb = CircuitBreakerState::new(0.20);
            for (path, ok) in items {
                if *ok {
                    cb.record_success();
                } else {
                    cb.record_failure(path, "parse error");
                }
                if cb.should_abort() {
                    return Some(path.clone());
                }
            }
            None
        };

        // Sorted order → deterministic trip point.
        results.sort_by(|a, b| a.0.cmp(&b.0));
        let trip_sorted = run_cb(&results);

        // Reversed order (simulates a different par_iter ordering).
        results.reverse();
        results.sort_by(|a, b| a.0.cmp(&b.0)); // sort again — same as before
        let trip_sorted2 = run_cb(&results);

        // Both sorted runs must trip at the same file.
        assert_eq!(
            trip_sorted, trip_sorted2,
            "sorted runs must trip at the same path"
        );
        assert!(trip_sorted.is_some(), "circuit breaker should have tripped");

        // Without sorting (reverse order): failures come first, CB trips earlier.
        let mut reversed: Vec<(String, bool)> = results.clone();
        reversed.reverse(); // failures first
        let trip_unsorted = run_cb(&reversed);

        // The unsorted trip path differs from the sorted one, proving sort matters.
        // (Both will trip, but at different paths.)
        assert_ne!(
            trip_sorted, trip_unsorted,
            "unsorted order should trip at a different (earlier) path, proving sort is needed"
        );
    }

    #[test]
    fn test_tier_counts() {
        use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason, SkippedFile};

        let mut index = make_empty_live_index();
        assert_eq!(index.tier_counts(), (0, 0, 0));

        index.add_skipped_file(SkippedFile {
            path: "model.bin".into(),
            size: 1000,
            extension: Some("bin".into()),
            decision: AdmissionDecision::skip(
                AdmissionTier::MetadataOnly,
                SkipReason::DenylistedExtension,
            ),
        });
        index.add_skipped_file(SkippedFile {
            path: "huge.dat".into(),
            size: 200_000_000,
            extension: Some("dat".into()),
            decision: AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling),
        });

        assert_eq!(index.tier_counts(), (0, 1, 1));
    }
}
