use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecommendedWatcher as NotifyRecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, NoCache, new_debouncer_opt,
};
use tracing::{debug, error, warn};

use crate::domain::{FileDisposition, LanguageId};
use crate::live_index::store::SharedIndex;

// Watcher state snapshot types live in the engine-safe `watcher_state` module so
// the engine's health stats can use them in `embed` builds; the notify-based
// runtime below is server-only.
pub use crate::watcher_state::{WatcherInfo, WatcherState};
// Relocated to `live_index::single_file` (task #24) so the embed facade can
// expose the SAME single-file admission seam; the watcher keeps its exact
// call surface through this re-export.
#[cfg(test)]
pub(crate) use crate::live_index::single_file::read_and_index_with_stable_read;
pub(crate) use crate::live_index::single_file::{
    ReindexResult, admit_and_index_single_path, maybe_reindex, read_and_index,
};

/// Tracks event bursts to adaptively extend the debounce window.
///
/// Debounce logic:
/// - Base window: 200ms
/// - Burst window: 500ms (when >BURST_THRESHOLD events in a 200ms window)
/// - Resets to 200ms after QUIET_SECS of inactivity
pub struct BurstTracker {
    pub event_count: u32,
    pub window_start: Instant,
    pub last_event_at: Instant,
    pub extended: bool,
}

impl BurstTracker {
    const BURST_THRESHOLD: u32 = 3;
    const BASE_MS: u64 = 200;
    const BURST_MS: u64 = 500;
    const QUIET_SECS: u64 = 5;

    /// Create a new BurstTracker with all counters at zero.
    pub fn new() -> Self {
        let now = Instant::now();
        BurstTracker {
            event_count: 0,
            window_start: now,
            last_event_at: now,
            extended: false,
        }
    }

    /// Record an event at the given instant, updating burst state.
    ///
    /// Window logic: if `now - window_start > BASE_MS`, start a new window
    /// and reset count to 1. Otherwise increment count.
    /// If count exceeds BURST_THRESHOLD, set extended=true.
    /// Always updates last_event_at.
    pub fn update(&mut self, now: Instant) {
        let window_duration = now.duration_since(self.window_start);
        if window_duration > Duration::from_millis(Self::BASE_MS) {
            // Start a new window
            self.window_start = now;
            self.event_count = 1;
            self.extended = false;
        } else {
            self.event_count += 1;
            if self.event_count > Self::BURST_THRESHOLD {
                self.extended = true;
            }
        }
        self.last_event_at = now;
    }

    /// Returns the effective debounce window in milliseconds.
    ///
    /// - If last event was more than QUIET_SECS ago, return BASE_MS (quiet reset)
    /// - If in burst mode (extended=true), return BURST_MS
    /// - Otherwise return BASE_MS
    pub fn effective_debounce_ms(&self) -> u64 {
        let since_last = self.last_event_at.elapsed();
        if since_last > Duration::from_secs(Self::QUIET_SECS) {
            return Self::BASE_MS;
        }
        if self.extended {
            Self::BURST_MS
        } else {
            Self::BASE_MS
        }
    }
}

impl Default for BurstTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Plan 02: Event processing, path normalization, content hash skip, ENOENT
// ---------------------------------------------------------------------------

#[must_use]
pub(crate) enum FreshenResult {
    Fresh,
    StaleReindexed,
    StaleRemoved,
    GenerationMismatch,
}

/// Strip `\\?\` Windows extended-length path prefix and normalize backslashes.
///
/// Returns the relative forward-slash path if `abs_path` is inside `repo_root`,
/// or `None` if it lies outside.
pub(crate) fn normalize_event_path(abs_path: &Path, repo_root: &Path) -> Option<String> {
    let raw_path = abs_path.to_string_lossy();

    // Strip \\?\ prefix (Windows extended-length format)
    let stripped_raw: &str = if let Some(stripped) = raw_path.strip_prefix(r"\\?\") {
        stripped
    } else {
        raw_path.as_ref()
    };

    let clean_abs = Path::new(stripped_raw);

    // Try strip_prefix with the original repo_root first, then with its own \\?\ stripped
    let relative = clean_abs.strip_prefix(repo_root).or_else(|_| {
        let root_raw = repo_root.to_string_lossy();
        let stripped_root: &str = if let Some(stripped) = root_raw.strip_prefix(r"\\?\") {
            stripped
        } else {
            return clean_abs.strip_prefix(repo_root);
        };
        clean_abs.strip_prefix(Path::new(stripped_root))
    });

    relative
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// Return the authoritative `LanguageId` for a complete repository path.
/// Extensionless narrative files and configuration dotfiles are path-classified.
pub(crate) fn supported_language(path: &Path) -> Option<LanguageId> {
    path.to_str().and_then(LanguageId::from_path)
}

/// Return `true` for Create, Modify, or Remove events; `false` for Access and others.
pub(crate) fn is_relevant_event(event: &DebouncedEvent) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Mtime-based freshness check for a single file.
///
/// Compares the file's current mtime on disk against the value stored in the
/// index. If they differ (or the file is not yet indexed), re-indexes it
/// immediately before the caller proceeds.
///
/// Returns a structured freshness outcome so callers can distinguish a
/// confirmed deletion from a stale project-generation mismatch.
pub(crate) fn freshen_file_if_stale(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    expected_gen: u64,
) -> FreshenResult {
    if shared.current_project_generation() != expected_gen {
        let _ = shared.remove_file_at_generation(relative_path, expected_gen);
        return FreshenResult::GenerationMismatch;
    }

    // 1. Stat the file on disk
    let disk_mtime = std::fs::metadata(abs_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // 2. Compare against indexed mtime (read lock, released immediately)
    let indexed_mtime = {
        let index = shared.read();
        index
            .get_file(relative_path)
            .map(|f| f.mtime_secs)
            .unwrap_or(u64::MAX)
    };

    if disk_mtime == 0 && indexed_mtime == 0 {
        return FreshenResult::Fresh; // both unknown — treat as fresh to avoid churn
    }
    if disk_mtime != 0 && disk_mtime == indexed_mtime {
        return FreshenResult::Fresh; // already fresh
    }

    // 3. Stale — re-index
    let language = supported_language(abs_path);

    debug!("freshness guard: stale file detected, re-indexing {relative_path}");
    let result = maybe_reindex(relative_path, abs_path, shared, language, expected_gen);
    if shared.current_project_generation() != expected_gen {
        let _ = shared.remove_file_at_generation(relative_path, expected_gen);
        return FreshenResult::GenerationMismatch;
    }

    match result {
        ReindexResult::HashSkip | ReindexResult::Reindexed | ReindexResult::ReadError(_) => {
            FreshenResult::StaleReindexed
        }
        // Admission demoted the file to Tier 2/3: the index WAS reconciled
        // (any prior Tier-1 entry removed, skip record recorded), so the file is
        // no longer parsed/indexed. Report it as a refresh — the caller's stale
        // state has been resolved — without claiming the file is still indexed.
        ReindexResult::Skipped => FreshenResult::StaleReindexed,
        ReindexResult::NotFound | ReindexResult::Removed => FreshenResult::StaleRemoved,
    }
}

/// Resolve the generation the watcher should fence its mutations against for
/// the commit boundary about to run.
///
/// The watcher snapshots `spawn_gen` ONCE at spawn (`run_watcher_with_stop`).
/// On COLD START the fire-and-forget `bg_index.reload(&bg_root)` bumps the
/// project generation AFTER that snapshot, so a fence pinned to `spawn_gen`
/// would reject (and remove) every subsequent edit forever. This heals that:
/// when the generation has advanced but the live index STILL serves our own
/// `repo_root`, the advance was a same-project reload (cold-start or in-place
/// reindex) and we adopt the current generation so mutations commit again.
///
/// The fence stays correct for the genuine cross-project race: a retarget
/// reload swaps `indexed_root` to a DIFFERENT root, so we keep the stale
/// `spawn_gen` and the store's under-lock check rejects the now-foreign
/// mutation (see `slipped_past_cancellation_fence_increments_counter`).
///
/// Ordering: `reload` publishes the new live index (with its new
/// `indexed_root`) BEFORE bumping the generation (`AcqRel`), and we read the
/// generation before the live root, so a `spawn_gen`-equal read never pairs an
/// old generation with a new root. The value returned here is only a *better
/// guess* than the frozen snapshot — the store re-checks the generation under
/// its write lock, so any residual race still rejects rather than corrupts.
pub(crate) fn effective_fence_generation(
    shared: &SharedIndex,
    repo_root: &Path,
    spawn_gen: u64,
) -> u64 {
    let current_gen = shared.current_project_generation();
    if current_gen == spawn_gen {
        return spawn_gen;
    }
    // Generation advanced since spawn. Adopt it only if the live index still
    // serves our repo_root (same-project reload); otherwise keep the stale
    // spawn generation so the store fence rejects the foreign mutation.
    let target = crate::live_index::store::normalize_root(repo_root);
    let same_root = shared
        .read()
        .indexed_root
        .as_deref()
        .map(crate::live_index::store::normalize_root)
        .is_some_and(|root| root == target);
    if same_root { current_gen } else { spawn_gen }
}

/// Walk all indexed files and re-index any whose on-disk mtime differs from
/// the stored value. Returns the number of stale files re-indexed.
///
/// Called on watcher overflow and by the periodic reconciliation timer.
///
/// `spawn_gen` is the watcher's spawn-time generation snapshot. The fence value
/// actually used for each file is re-synced via [`effective_fence_generation`]
/// so a same-root reload (cold-start heal) no longer permanently rejects, while
/// a cross-project retarget still rejects.
pub(crate) fn reconcile_stale_files_with_stop(
    repo_root: &Path,
    shared: &SharedIndex,
    should_stop: impl Fn() -> bool,
    expected_gen: u64,
) -> usize {
    reconcile_stale_files_with_stop_and_hook(
        repo_root,
        shared,
        should_stop,
        expected_gen,
        || {
            crate::discovery::scout_repository_with_exclusions(
                repo_root,
                &shared.source_exclusions(),
            )
        },
        || {},
    )
    .repaired
}

fn reconcile_stale_files_with_stop_and_hook<S>(
    repo_root: &Path,
    shared: &SharedIndex,
    should_stop: impl Fn() -> bool,
    expected_gen: u64,
    scout: S,
    after_scout: impl FnOnce(),
) -> ReconciliationAttempt
where
    S: FnOnce() -> anyhow::Result<crate::discovery::ScoutPlan>,
{
    // Re-sync the fence to the CURRENT generation when the live index still
    // serves our repo_root, so a same-root reload that advanced the generation
    // after watcher spawn (cold start) no longer permanently rejects. A
    // cross-project retarget keeps the stale spawn generation and is rejected.
    let fence_gen = effective_fence_generation(shared, repo_root, expected_gen);

    let previous_plan = shared.scout_plan();
    let previous_entries = previous_plan
        .as_ref()
        .map(|plan| {
            plan.entries
                .iter()
                .filter_map(|entry| Some((entry.path.normalized_utf8.clone()?, entry.clone())))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let transient_paths: HashSet<String> = shared
        .terminal_dispositions()
        .iter()
        .filter(|(_, disposition)| {
            matches!(
                disposition,
                FileDisposition::Unreadable { .. }
                    | FileDisposition::UnstableDuringRead
                    | FileDisposition::AbortedCircuitBreaker
            )
        })
        .map(|(path, _)| path.clone())
        .collect();
    let fresh_plan = match scout() {
        Ok(plan) => Some(plan),
        Err(error) => {
            warn!("reconciliation rescout failed: {error}");
            None
        }
    };
    let rescout_failed = fresh_plan.is_none();
    after_scout();

    let paths: Vec<String> = {
        let index = shared.read();
        index.all_files().map(|(p, _)| p.clone()).collect()
    };

    let mut stale_count = 0usize;
    if let Some(fresh_plan) = fresh_plan {
        let fresh_paths: HashSet<String> = fresh_plan
            .entries
            .iter()
            .filter_map(|entry| entry.path.normalized_utf8.clone())
            .collect();
        let changed_entries: Vec<(String, PathBuf, Option<LanguageId>)> = fresh_plan
            .entries
            .iter()
            .filter_map(|entry| {
                let relative_path = entry.path.normalized_utf8.clone()?;
                if previous_entries.get(&relative_path) == Some(entry)
                    && !transient_paths.contains(&relative_path)
                {
                    return None;
                }
                Some((relative_path, entry.absolute_path.clone()?, entry.language))
            })
            .collect();
        let removed_paths: Vec<(String, crate::domain::ScoutedEntry)> =
            if fresh_plan.coverage == crate::domain::CoverageStatus::Complete {
                previous_entries
                    .iter()
                    .filter(|(path, _)| !fresh_paths.contains(path.as_str()))
                    .map(|(path, entry)| (path.clone(), entry.clone()))
                    .collect()
            } else {
                Vec::new()
            };

        let mut repairs_applied = 0usize;
        for (relative_path, absolute_path, language) in changed_entries {
            if should_stop() {
                return stale_count.into();
            }
            match read_and_index(&relative_path, &absolute_path, shared, language, fence_gen) {
                ReindexResult::Reindexed | ReindexResult::HashSkip | ReindexResult::Skipped => {
                    repairs_applied += 1
                }
                ReindexResult::NotFound | ReindexResult::Removed | ReindexResult::ReadError(_) => {}
            }
        }
        for (relative_path, expected_entry) in removed_paths {
            if should_stop() {
                return stale_count.into();
            }
            if shared.remove_file_if_scout_entry_at_generation(
                &relative_path,
                &expected_entry,
                fence_gen,
            ) {
                repairs_applied += 1;
            }
        }

        if should_stop() {
            return stale_count.into();
        }
        if shared.publish_reconciled_scout_plan_at_generation(
            previous_plan.as_deref(),
            fresh_plan,
            fence_gen,
        ) {
            stale_count += repairs_applied;
        }
    } else {
        // A failed rescout cannot authorize additions or catalog removals. Keep
        // the prior Tier-1 repair path as a bounded fallback for known content.
        for relative_path in &paths {
            if should_stop() {
                break;
            }
            let abs_path = repo_root.join(relative_path);
            match freshen_file_if_stale(relative_path, &abs_path, shared, fence_gen) {
                FreshenResult::StaleReindexed | FreshenResult::StaleRemoved => stale_count += 1,
                FreshenResult::Fresh | FreshenResult::GenerationMismatch => {}
            }
        }
    }

    if stale_count > 0 {
        // Collect stale paths for diagnostic logging to help debug reconciliation loops.
        let stale_paths: Vec<&str> = paths
            .iter()
            .filter(|p| {
                let abs = repo_root.join(p.as_str());
                let disk = std::fs::metadata(&abs)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let indexed = {
                    let idx = shared.read();
                    idx.get_file(p).map(|f| f.mtime_secs).unwrap_or(0)
                };
                disk != indexed
            })
            .map(|p| p.as_str())
            .take(5)
            .collect();
        if stale_paths.is_empty() {
            warn!("reconciliation re-indexed {stale_count} file(s) (now fresh)");
        } else {
            warn!(
                "reconciliation found {stale_count} stale file(s), still divergent: {}",
                stale_paths.join(", ")
            );
        }
    }
    ReconciliationAttempt {
        repaired: stale_count,
        retry_degraded: rescout_failed
            || shared
                .scout_plan()
                .is_some_and(|plan| plan.coverage == crate::domain::CoverageStatus::Degraded),
    }
}

pub(crate) fn reconcile_stale_files(repo_root: &Path, shared: &SharedIndex) -> usize {
    let expected_gen = shared.current_project_generation();
    reconcile_stale_files_with_stop(repo_root, shared, || false, expected_gen)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReconciliationAttempt {
    repaired: usize,
    retry_degraded: bool,
}

impl From<usize> for ReconciliationAttempt {
    fn from(repaired: usize) -> Self {
        Self {
            repaired,
            retry_degraded: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReconciliationCause {
    FreshInstance,
    Periodic,
    Overflow,
}

fn reconcile_for_cause(
    repo_root: &Path,
    shared: &SharedIndex,
    watcher_info: &Arc<Mutex<WatcherInfo>>,
    stop_token: &AtomicBool,
    expected_gen: u64,
    cause: ReconciliationCause,
) -> usize {
    reconcile_for_cause_with(
        repo_root,
        shared,
        watcher_info,
        stop_token,
        expected_gen,
        cause,
        || {
            reconcile_stale_files_with_stop_and_hook(
                repo_root,
                shared,
                || stop_token.load(Ordering::Acquire),
                expected_gen,
                || {
                    crate::discovery::scout_repository_with_exclusions(
                        repo_root,
                        &shared.source_exclusions(),
                    )
                },
                || {},
            )
        },
        std::thread::sleep,
    )
}

#[allow(clippy::too_many_arguments)]
fn reconcile_for_cause_with<R, S, T>(
    repo_root: &Path,
    shared: &SharedIndex,
    watcher_info: &Arc<Mutex<WatcherInfo>>,
    stop_token: &AtomicBool,
    expected_gen: u64,
    cause: ReconciliationCause,
    mut reconcile_once: R,
    mut sleep: S,
) -> usize
where
    R: FnMut() -> T,
    T: Into<ReconciliationAttempt>,
    S: FnMut(Duration),
{
    const MAX_DEGRADED_ATTEMPTS: usize = 5;
    const INITIAL_DEGRADED_DELAY: Duration = Duration::from_millis(50);
    const MAX_DEGRADED_DELAY: Duration = Duration::from_secs(1);

    let active_generation = shared.current_project_generation();
    let effective_generation = effective_fence_generation(shared, repo_root, expected_gen);
    let batch_belongs_to_active_project = effective_generation == active_generation;
    let mut repaired = 0usize;
    let mut delay = INITIAL_DEGRADED_DELAY;
    for attempt in 1..=MAX_DEGRADED_ATTEMPTS {
        let outcome: ReconciliationAttempt = reconcile_once().into();
        repaired = repaired.saturating_add(outcome.repaired);
        if stop_token.load(Ordering::Acquire)
            || !batch_belongs_to_active_project
            || shared.current_project_generation() != active_generation
        {
            return repaired;
        }

        let coverage_degraded = shared
            .scout_plan()
            .is_some_and(|plan| plan.coverage == crate::domain::CoverageStatus::Degraded);
        if !outcome.retry_degraded && !coverage_degraded {
            break;
        }
        if attempt == MAX_DEGRADED_ATTEMPTS {
            warn!(
                "watcher: reconciliation remains explicitly degraded after {MAX_DEGRADED_ATTEMPTS} attempts"
            );
            break;
        }

        sleep(delay);
        delay = delay.saturating_mul(2).min(MAX_DEGRADED_DELAY);
    }

    let now = SystemTime::now();
    {
        let mut info = watcher_info.lock();
        if cause == ReconciliationCause::Overflow {
            info.overflow_count += 1;
            info.last_overflow_at = Some(now);
        }
        info.stale_files_found += repaired as u64;
        info.last_reconcile_at = Some(now);
    }
    // A settled, current result is skipped by the temporal queue. Calling on
    // every reconciliation also detects bytes-identical ref movement, which
    // has no filesystem content event.
    crate::live_index::git_temporal::spawn_git_temporal_computation(
        shared.clone(),
        repo_root.to_path_buf(),
        effective_generation,
    );
    repaired
}

// ---------------------------------------------------------------------------
// Plan 02: Watcher lifecycle — start_watcher, run_watcher, restart-with-backoff
// ---------------------------------------------------------------------------

/// Owns the debouncer and the receiving end of the event channel.
///
/// Dropping this struct stops the OS-level file watcher.
pub struct WatcherHandle {
    /// The debouncer owns the OS watcher thread; dropping it stops watching.
    ///
    /// `NoCache` (not the platform-default `FileIdMap` on Windows) disables the
    /// file-ID tracking cache. `FileIdMap` exists only to stitch rename events
    /// into paired Modify(Name) events. `process_events` instead collapses each
    /// debounced batch by normalized path and converges from current filesystem
    /// truth, so Remove+Create remains sufficient. Crucially, `FileIdMap`
    /// would otherwise run a full `WalkDir` (one open-handle syscall per entry,
    /// including 100k+ gitignored `target/`/`node_modules/` entries) at
    /// `watch()`, per Create during build floods, and again on overflow rescan.
    _debouncer: Debouncer<NotifyRecommendedWatcher, NoCache>,
    /// Receive end of the synchronous channel from the notify callback.
    pub event_rx: std::sync::mpsc::Receiver<DebounceEventResult>,
}

/// Owned together: signal `stop_token`, then bounded-await `task`.
/// See H.1b's `abort_watcher_task` for the canonical shutdown sequence.
pub struct WatcherTaskHandle {
    pub task: tokio::task::JoinHandle<()>,
    pub stop_token: Arc<AtomicBool>,
}

/// Create a new debouncer watching `repo_root` recursively.
///
/// `debounce_ms` controls the debounce window (base 200ms, extended to 500ms during bursts).
/// Uses `std::sync::mpsc` (not tokio) because notify's callback runs on its own OS thread.
pub(crate) fn start_watcher(
    repo_root: &Path,
    debounce_ms: u64,
) -> Result<WatcherHandle, notify::Error> {
    let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();

    // Use `NoCache` instead of the platform-default cache. On Windows the
    // default `RecommendedCache` is `FileIdMap`, which walks the entire tree
    // with one open-handle syscall per entry at `watch()` time (and again on
    // every Create / overflow rescan) to maintain rename-stitching state that
    // `process_events` never uses. On large trees with many gitignored entries
    // (`target/`, `node_modules/`) that walk dominates watcher startup latency.
    let mut debouncer = new_debouncer_opt::<_, NotifyRecommendedWatcher, NoCache>(
        Duration::from_millis(debounce_ms),
        None,
        move |result: DebounceEventResult| {
            let _ = tx.send(result);
        },
        NoCache::new(),
        notify::Config::default(),
    )?;

    debouncer.watch(repo_root, RecursiveMode::Recursive)?;

    Ok(WatcherHandle {
        _debouncer: debouncer,
        event_rx: rx,
    })
}

pub(crate) fn process_events(
    events: Vec<DebouncedEvent>,
    repo_root: &Path,
    shared: &SharedIndex,
    burst_trackers: &mut HashMap<PathBuf, BurstTracker>,
    watcher_info: &Arc<Mutex<WatcherInfo>>,
    should_stop: &dyn Fn() -> bool,
    expected_gen: u64,
) {
    let content_generation_before = shared.published_generation().content_generation;

    struct PendingPath {
        absolute_path: PathBuf,
        relative_path: String,
        raw_event_count: u64,
        saw_write_hint: bool,
    }

    // A debounced atomic save commonly contains a now-vanished temporary path
    // before the live destination path. Collapse the batch by normalized path
    // and let current filesystem truth decide remove versus observe. This keeps
    // one stale temporary hint from spending the NotFound retry budget while a
    // newer destination hint waits behind it, and bounds digest/publication
    // work to once per path per batch.
    let mut pending_paths: HashMap<String, PendingPath> = HashMap::new();
    for event in events {
        if should_stop() {
            break;
        }
        if !is_relevant_event(&event) {
            continue;
        }
        let saw_write_hint = matches!(&event.kind, EventKind::Create(_) | EventKind::Modify(_));

        for abs_path in &event.paths {
            if should_stop() {
                break;
            }
            // Normalize path — skip if outside repo_root or can't be normalized
            let relative_path = match normalize_event_path(abs_path, repo_root) {
                Some(r) => r,
                None => continue,
            };

            // Mirror discovery's gitignore-aware walk: never index paths the
            // initial scan would have pruned. Without this the watcher picks
            // up files created under gitignored directories during a session —
            // most importantly SymForge's own `.symforge/` state dir (e.g.
            // `tee/*.rs` edit snapshots) — polluting search and reference
            // results and growing the index unbounded.
            let relative = Path::new(&relative_path);
            if crate::discovery::path_is_hard_scope_excluded(relative)
                || shared.is_source_excluded(relative)
                || shared.read().is_path_gitignored(&relative_path)
            {
                continue;
            }

            let pending = pending_paths
                .entry(relative_path.clone())
                .or_insert_with(|| PendingPath {
                    absolute_path: abs_path.clone(),
                    relative_path,
                    raw_event_count: 0,
                    saw_write_hint: false,
                });
            pending.absolute_path = abs_path.clone();
            pending.raw_event_count = pending.raw_event_count.saturating_add(1);
            pending.saw_write_hint |= saw_write_hint;
        }
    }

    let mut pending_paths: Vec<_> = pending_paths
        .into_values()
        .map(|pending| {
            let definitely_missing = matches!(
                std::fs::symlink_metadata(&pending.absolute_path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            );
            (definitely_missing, pending)
        })
        .collect();
    pending_paths.sort_by_cached_key(|(definitely_missing, pending)| {
        (
            *definitely_missing,
            pending.relative_path.to_lowercase(),
            pending.relative_path.clone(),
        )
    });

    for (definitely_missing, pending) in pending_paths {
        if should_stop() {
            break;
        }

        let mut debounce_ms = None;
        if definitely_missing {
            shared.remove_file_at_generation(&pending.relative_path, expected_gen);
        } else {
            if pending.saw_write_hint {
                let tracker = burst_trackers
                    .entry(pending.absolute_path.clone())
                    .or_default();
                tracker.update(Instant::now());
                debounce_ms = Some(tracker.effective_debounce_ms());
            }

            // Language inference is a target hint, never a scope filter. Unknown
            // extensions still reach metadata-first admission/cataloging.
            let language = supported_language(&pending.absolute_path);
            if matches!(
                read_and_index(
                    &pending.relative_path,
                    &pending.absolute_path,
                    shared,
                    language,
                    expected_gen,
                ),
                ReindexResult::NotFound
            ) {
                // The path disappeared after the batch observation. Converge to
                // current disk truth without serially sleeping in the event lane;
                // any later create hint or periodic reconciliation can re-admit it.
                shared.remove_file_at_generation(&pending.relative_path, expected_gen);
            }
        }

        let mut info = watcher_info.lock();
        info.events_processed = info
            .events_processed
            .saturating_add(pending.raw_event_count);
        info.last_event_at = Some(SystemTime::now());
        if let Some(debounce_ms) = debounce_ms {
            info.debounce_window_ms = debounce_ms;
        }
    }

    // Evict burst trackers that have been idle longer than 2 × QUIET_SECS to
    // prevent the map from growing unbounded over the lifetime of the watcher.
    // NOTE: eviction only runs after file-change events, not during overflow
    // reconciliation, so trackers for paths not recently seen are cleaned up
    // lazily on the next incoming event.
    let evict_threshold = Duration::from_secs(BurstTracker::QUIET_SECS * 2);
    burst_trackers.retain(|_, tracker| tracker.last_event_at.elapsed() < evict_threshold);

    if shared.published_generation().content_generation != content_generation_before {
        crate::live_index::git_temporal::spawn_git_temporal_computation(
            shared.clone(),
            repo_root.to_path_buf(),
            expected_gen,
        );
    }
}

fn handle_notify_errors(errors: &[notify::Error], on_overflow: impl FnOnce()) -> u32 {
    let mut overflow_detected = false;
    for error in errors {
        warn!("watcher: notify error: {error}");
        overflow_detected |= matches!(
            error.kind,
            notify::ErrorKind::Io(_)
                | notify::ErrorKind::Generic(_)
                | notify::ErrorKind::MaxFilesWatch
        );
    }
    if overflow_detected {
        warn!("watcher: buffer overflow detected — running full reconciliation");
        on_overflow();
    }
    u32::try_from(errors.len()).unwrap_or(u32::MAX)
}

/// Main watcher supervision loop. Spawned as a background tokio task by `main.rs`.
///
/// Lifecycle:
/// 1. Set state to Starting (watch not yet registered)
/// 2. Loop: start_watcher → on Ok set Active → process events → restart on error
///    with 1s backoff (state stays Starting while retrying)
/// 3. After 3 consecutive failures: set state to Degraded and stop
pub async fn run_watcher_with_stop(
    repo_root: PathBuf,
    shared: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    stop_token: Arc<AtomicBool>,
) {
    let expected_gen = shared.current_project_generation();
    if stop_token.load(Ordering::Acquire) {
        let mut info = watcher_info.lock();
        info.state = WatcherState::Off;
        return;
    }

    {
        // Mark Starting until the recursive filesystem watch is actually
        // registered. The transition to Active happens only when start_watcher
        // returns Ok (below). Historically the slow step on large trees was not
        // the OS-level watch registration but the debouncer's `FileIdMap` cache,
        // which walked the whole tree (one open-handle syscall per entry) at
        // `watch()` time; `start_watcher` now uses `NoCache` to skip that walk.
        // We still report Active only after Ok so a registration failure is not
        // misreported as a healthy watcher.
        let mut info = watcher_info.lock();
        info.state = WatcherState::Starting;
    }

    let mut consecutive_failures: u32 = 0;
    const MAX_FAILURES: u32 = 3;
    let mut cancelled = false;

    'watcher: loop {
        if stop_token.load(Ordering::Acquire) {
            cancelled = true;
            break;
        }

        // Read the current recommended debounce window (updated by the burst tracker).
        let debounce_ms = watcher_info.lock().debounce_window_ms;
        match start_watcher(&repo_root, debounce_ms) {
            Err(e) => {
                consecutive_failures += 1;
                warn!(
                    "watcher: start_watcher failed (attempt {}): {}",
                    consecutive_failures, e
                );
                if consecutive_failures >= MAX_FAILURES {
                    let mut info = watcher_info.lock();
                    info.state = WatcherState::Degraded;
                    error!(
                        "watcher: entering degraded mode after {} consecutive failures",
                        MAX_FAILURES
                    );
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            Ok(handle) => {
                consecutive_failures = 0;

                // Registration starts a fresh watcher instance. Events lost
                // before this point cannot be recovered from the new channel,
                // so repair that uncertainty with an immediate full manifest
                // reconciliation before consuming incremental hints.
                let shared_for_fresh = shared.clone();
                let root_for_fresh = repo_root.clone();
                let watcher_info_for_fresh = watcher_info.clone();
                let stop_for_fresh = Arc::clone(&stop_token);
                let expected_gen_for_fresh = expected_gen;
                if let Err(error) = tokio::task::spawn_blocking(move || {
                    reconcile_for_cause(
                        &root_for_fresh,
                        &shared_for_fresh,
                        &watcher_info_for_fresh,
                        &stop_for_fresh,
                        expected_gen_for_fresh,
                        ReconciliationCause::FreshInstance,
                    );
                })
                .await
                {
                    warn!("watcher: fresh-instance reconciliation panicked: {error}");
                }
                if stop_token.load(Ordering::Acquire) {
                    cancelled = true;
                    break 'watcher;
                }

                // `Active` means the watcher is ready to consume events, not
                // merely that the OS handle exists. Keep the state at
                // `Starting` while the mandatory fresh-instance repair runs;
                // events arriving meanwhile remain queued by the registered
                // handle and are consumed below.
                {
                    let mut info = watcher_info.lock();
                    info.state = WatcherState::Active;
                }

                let mut burst_trackers: HashMap<PathBuf, BurstTracker> = HashMap::new();
                let mut session_errors: u32 = 0;
                const MAX_SESSION_ERRORS: u32 = 10;
                // Poll timeout: yield to tokio between checks to avoid blocking the executor.
                const RECV_TIMEOUT_MS: u64 = 50;

                // Reconciliation interval from env (default 30s, 0 to disable).
                let reconcile_interval_secs: u64 = std::env::var("SYMFORGE_RECONCILE_INTERVAL")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(30);
                let mut last_reconcile = Instant::now();

                loop {
                    if stop_token.load(Ordering::Acquire) {
                        cancelled = true;
                        break 'watcher;
                    }

                    // Periodic reconciliation sweep (belt-and-suspenders against missed events).
                    if reconcile_interval_secs > 0
                        && last_reconcile.elapsed() >= Duration::from_secs(reconcile_interval_secs)
                    {
                        let shared_clone = shared.clone();
                        let root_clone = repo_root.clone();
                        let watcher_info_clone = watcher_info.clone();
                        let stop_for_reconcile = Arc::clone(&stop_token);
                        let expected_gen_for_reconcile = expected_gen;
                        tokio::task::spawn_blocking(move || {
                            reconcile_for_cause(
                                &root_clone,
                                &shared_clone,
                                &watcher_info_clone,
                                &stop_for_reconcile,
                                expected_gen_for_reconcile,
                                ReconciliationCause::Periodic,
                            );
                        });
                        // Coupling store refresh runs on its own task so a
                        // slow delta never delays stale-file reconciliation.
                        // Gates on SYMFORGE_COUPLING internally and holds a
                        // per-workspace guard against concurrent refreshes.
                        let root_for_coupling = repo_root.clone();
                        let stop_for_coupling = Arc::clone(&stop_token);
                        let spawn_gen_for_coupling = expected_gen;
                        let shared_for_coupling = shared.clone();
                        tokio::task::spawn_blocking(move || {
                            if stop_for_coupling.load(Ordering::Acquire) {
                                return;
                            }
                            // Re-sync against a same-root reload (cold start) so
                            // the coupling refresh heals like the file reconcile;
                            // a retarget keeps the stale spawn gen and no-ops.
                            let expected_gen_for_coupling = effective_fence_generation(
                                &shared_for_coupling,
                                &root_for_coupling,
                                spawn_gen_for_coupling,
                            );
                            crate::live_index::coupling::refresh_on_reconcile_tick(
                                &root_for_coupling,
                                expected_gen_for_coupling,
                                &shared_for_coupling,
                            );
                        });
                        last_reconcile = Instant::now();
                    }

                    match handle.event_rx.try_recv() {
                        Ok(Ok(events)) => {
                            // Run process_events in spawn_blocking to avoid
                            // starving tokio worker threads during file I/O
                            // and tree-sitter parsing.
                            let shared_clone = shared.clone();
                            let root_clone = repo_root.clone();
                            let watcher_info_clone = watcher_info.clone();
                            let stop_for_events = Arc::clone(&stop_token);
                            let spawn_gen_for_events = expected_gen;
                            let mut trackers = std::mem::take(&mut burst_trackers);
                            match tokio::task::spawn_blocking(move || {
                                // Re-sync the fence at the commit boundary: a
                                // same-root reload (cold start) that advanced the
                                // generation after watcher spawn must no longer
                                // reject events; a cross-project retarget still
                                // keeps the stale spawn gen and is rejected.
                                let expected_gen_for_events = effective_fence_generation(
                                    &shared_clone,
                                    &root_clone,
                                    spawn_gen_for_events,
                                );
                                process_events(
                                    events,
                                    &root_clone,
                                    &shared_clone,
                                    &mut trackers,
                                    &watcher_info_clone,
                                    &|| stop_for_events.load(Ordering::Acquire),
                                    expected_gen_for_events,
                                );
                                trackers
                            })
                            .await
                            {
                                Ok(t) => burst_trackers = t,
                                // Intentional: on panic the burst trackers reset to empty.
                                // This is acceptable — burst tracking is a performance
                                // optimization, not a correctness requirement.
                                Err(e) => warn!("watcher: process_events panicked: {e}"),
                            }
                        }
                        Ok(Err(errors)) => {
                            let observed_errors = handle_notify_errors(&errors, || {
                                let shared_clone = shared.clone();
                                let root_clone = repo_root.clone();
                                let watcher_info_clone = watcher_info.clone();
                                let stop_for_reconcile = Arc::clone(&stop_token);
                                let expected_gen_for_reconcile = expected_gen;
                                tokio::task::spawn_blocking(move || {
                                    reconcile_for_cause(
                                        &root_clone,
                                        &shared_clone,
                                        &watcher_info_clone,
                                        &stop_for_reconcile,
                                        expected_gen_for_reconcile,
                                        ReconciliationCause::Overflow,
                                    );
                                });
                            });
                            session_errors += observed_errors;
                            if session_errors >= MAX_SESSION_ERRORS {
                                warn!("watcher: too many session errors, restarting watcher");
                                break;
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // No event ready — yield to tokio async executor
                            // instead of blocking the worker thread.
                            tokio::time::sleep(Duration::from_millis(RECV_TIMEOUT_MS)).await;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            // Channel closed — debouncer dropped or OS watcher died
                            warn!("watcher: event channel closed, restarting");
                            break;
                        }
                    }
                }

                // Inner loop exited — count as a failure and try to restart
                consecutive_failures += 1;
                if consecutive_failures >= MAX_FAILURES {
                    let mut info = watcher_info.lock();
                    info.state = WatcherState::Degraded;
                    error!(
                        "watcher: entering degraded mode after {} consecutive failures",
                        MAX_FAILURES
                    );
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    if cancelled {
        let mut info = watcher_info.lock();
        info.state = WatcherState::Off;
    }
}

pub async fn run_watcher(
    repo_root: PathBuf,
    shared: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
) {
    let stop_token = Arc::new(AtomicBool::new(false));
    run_watcher_with_stop(repo_root, shared, watcher_info, stop_token).await;
}

/// Spawn a new watcher task.
///
/// Called by `index_folder` after a full reload to restart the watcher
/// on the new root path.
pub fn restart_watcher(
    repo_root: PathBuf,
    shared: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    prev: Option<WatcherTaskHandle>,
) -> WatcherTaskHandle {
    {
        // A (re)start has been initiated: mark Starting (not Off) so health can
        // distinguish "watcher is coming up" from "watcher is not running". The
        // spawned task may wait up to 2s for the previous watcher to stop and
        // then register a recursive filesystem watch, which is the slow part on
        // large trees — health reads during that window should not report Off.
        let mut info = watcher_info.lock();
        info.state = WatcherState::Starting;
    }
    let stop_token = Arc::new(AtomicBool::new(false));
    let stop_for_task = Arc::clone(&stop_token);
    let task = tokio::spawn(async move {
        if let Some(prev) = prev {
            prev.stop_token.store(true, Ordering::Release);
            let mut old_task = prev.task;
            if tokio::time::timeout(Duration::from_secs(2), &mut old_task)
                .await
                .is_err()
            {
                warn!("watcher: previous watcher did not stop within 2s; aborting task");
                old_task.abort();
            }
        }
        run_watcher_with_stop(repo_root, shared, watcher_info, stop_for_task).await;
    });
    WatcherTaskHandle { task, stop_token }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::index::{AdmissionTier, SkipReason};
    use crate::domain::{MetadataOnlyReason, ScoutDecision};
    use std::time::Duration;
    use tempfile::TempDir;

    static GENERATED_OUTPUT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct GeneratedOutputEnvGuard(Option<std::ffi::OsString>);

    impl GeneratedOutputEnvGuard {
        #[allow(unsafe_code)]
        fn set(value: Option<&str>) -> Self {
            let previous = std::env::var_os("SYMFORGE_INDEX_GENERATED_OUTPUT");
            // SAFETY: generated-output tests serialize mutations with
            // GENERATED_OUTPUT_ENV_LOCK and restore the prior value on drop.
            unsafe {
                match value {
                    Some(value) => std::env::set_var("SYMFORGE_INDEX_GENERATED_OUTPUT", value),
                    None => std::env::remove_var("SYMFORGE_INDEX_GENERATED_OUTPUT"),
                }
            }
            Self(previous)
        }
    }

    #[allow(unsafe_code)]
    impl Drop for GeneratedOutputEnvGuard {
        fn drop(&mut self) {
            // SAFETY: the guard is dropped while GENERATED_OUTPUT_ENV_LOCK is held.
            unsafe {
                match &self.0 {
                    Some(value) => std::env::set_var("SYMFORGE_INDEX_GENERATED_OUTPUT", value),
                    None => std::env::remove_var("SYMFORGE_INDEX_GENERATED_OUTPUT"),
                }
            }
        }
    }

    fn create_test_source(root: &Path, relative_path: &str, content: &[u8]) -> PathBuf {
        let absolute_path = root.join(relative_path);
        std::fs::create_dir_all(
            absolute_path
                .parent()
                .expect("test source must have a parent directory"),
        )
        .expect("create test source directory");
        std::fs::write(&absolute_path, content).expect("write test source");
        absolute_path
    }

    fn runtime_canary() -> String {
        ["runtime", "-", "canary", "-", "watcher"].concat()
    }

    fn stage_test_path(repository: &git2::Repository, relative_path: &str) {
        let mut index = repository.index().expect("open git index");
        index
            .add_path(Path::new(relative_path))
            .expect("stage test path");
        index.write().expect("write git index");
    }

    fn init_test_git_repository(root: &Path) -> git2::Repository {
        let repository = git2::Repository::init(root).expect("initialize git repository");
        create_test_source(root, "src/main.rs", b"fn main() {}\n");
        stage_test_path(&repository, "src/main.rs");
        repository
    }

    fn assert_generated_output_skip(shared: &SharedIndex, relative_path: &str) {
        let index = shared.read();
        assert!(
            index.get_file(relative_path).is_none(),
            "{relative_path} must not be present in Tier 1"
        );
        let skipped = index
            .compatibility_skipped_files()
            .into_iter()
            .find(|skipped| skipped.path == relative_path)
            .unwrap_or_else(|| panic!("{relative_path} must have a skip record"));
        assert_eq!(skipped.decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(skipped.decision.reason, Some(SkipReason::GeneratedOutput));
    }

    // --- BurstTracker tests from Plan 01 (preserved) ---

    #[test]
    fn test_watcher_state_variants() {
        // All four variants exist and are distinct.
        let active = WatcherState::Active;
        let starting = WatcherState::Starting;
        let degraded = WatcherState::Degraded;
        let off = WatcherState::Off;
        assert_ne!(active, degraded);
        assert_ne!(active, off);
        assert_ne!(degraded, off);
        assert_ne!(starting, active);
        assert_ne!(starting, degraded);
        assert_ne!(starting, off);
    }

    #[test]
    fn test_restart_watcher_sets_starting_state() {
        // restart_watcher must publish Starting synchronously when a (re)start
        // is initiated — not Off — so a health probe during startup does not
        // mistake "watcher coming up" for "watcher not running". We drive this
        // on a current-thread runtime so the spawned supervision task cannot be
        // polled before we read the state: the synchronous lock write in
        // restart_watcher is the contract under test.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();

        let shared = crate::live_index::store::LiveIndex::empty();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        assert_eq!(
            watcher_info.lock().state,
            WatcherState::Off,
            "precondition: default watcher state is Off"
        );

        let tmp = TempDir::new().unwrap();
        let handle = restart_watcher(
            tmp.path().to_path_buf(),
            shared,
            Arc::clone(&watcher_info),
            None,
        );

        // The current-thread runtime has not been driven, so the spawned task
        // is still pending: the only state write that has executed is the
        // synchronous Starting transition inside restart_watcher.
        assert_eq!(
            watcher_info.lock().state,
            WatcherState::Starting,
            "restart_watcher should publish Starting synchronously, not Off"
        );

        // Tear down: signal stop and abort the pending task without driving the
        // runtime to completion.
        handle.stop_token.store(true, Ordering::Release);
        handle.task.abort();
    }

    #[test]
    fn test_watcher_info_default() {
        let info = WatcherInfo::default();
        assert_eq!(info.state, WatcherState::Off);
        assert_eq!(info.events_processed, 0);
        assert!(info.last_event_at.is_none());
        assert_eq!(info.debounce_window_ms, 200);
    }

    #[test]
    fn test_burst_tracker_new() {
        let tracker = BurstTracker::new();
        assert_eq!(tracker.event_count, 0);
        assert!(!tracker.extended);
    }

    #[test]
    fn test_burst_tracker_extends_window() {
        // 4 events within 200ms -> extended=true, effective=500
        let mut tracker = BurstTracker::new();
        let start = Instant::now();
        // Simulate 4 rapid events within the same 200ms window
        tracker.update(start + Duration::from_millis(10));
        tracker.update(start + Duration::from_millis(20));
        tracker.update(start + Duration::from_millis(30));
        tracker.update(start + Duration::from_millis(40));
        assert!(tracker.extended, "4 events in window should trigger burst");
        assert_eq!(tracker.effective_debounce_ms(), 500);
    }

    #[test]
    fn test_burst_tracker_resets_after_quiet() {
        // After last event > 5s ago, effective should return 200
        let mut tracker = BurstTracker::new();
        let past = Instant::now() - Duration::from_secs(10);
        // We simulate this by forcing extended=true and setting last_event_at in the past
        tracker.extended = true;
        tracker.last_event_at = past;
        assert_eq!(
            tracker.effective_debounce_ms(),
            200,
            "after quiet period, should reset to 200ms"
        );
    }

    #[test]
    fn test_burst_tracker_new_window_resets_count() {
        // An event after >200ms gap should start a fresh window with count=1, extended=false
        let mut tracker = BurstTracker::new();
        let t0 = Instant::now();
        // First burst: 4 events
        tracker.update(t0 + Duration::from_millis(10));
        tracker.update(t0 + Duration::from_millis(20));
        tracker.update(t0 + Duration::from_millis(30));
        tracker.update(t0 + Duration::from_millis(40));
        assert!(tracker.extended, "should be extended after burst");

        // Event after 300ms gap
        tracker.update(t0 + Duration::from_millis(350));
        assert_eq!(tracker.event_count, 1, "count should reset to 1 after gap");
        assert!(!tracker.extended, "extended should reset after new window");
    }

    #[test]
    fn test_burst_tracker_base_debounce_no_burst() {
        // Under threshold: effective should remain 200ms
        let mut tracker = BurstTracker::new();
        let t0 = Instant::now();
        tracker.update(t0 + Duration::from_millis(10));
        tracker.update(t0 + Duration::from_millis(20));
        // Only 2 events, under BURST_THRESHOLD of 3
        assert!(!tracker.extended);
        assert_eq!(tracker.effective_debounce_ms(), 200);
    }

    // --- Plan 02: Path normalization tests ---

    #[test]
    #[cfg(windows)]
    fn test_normalize_event_path_basic() {
        // Windows-style absolute path: strip root prefix, normalize slashes
        let abs = Path::new(r"C:\repo\src\main.rs");
        let root = Path::new(r"C:\repo");
        let result = normalize_event_path(abs, root);
        assert_eq!(result, Some("src/main.rs".to_string()));
    }

    #[test]
    #[cfg(windows)]
    fn test_normalize_event_path_unc_prefix() {
        // Windows extended-length path with \\?\ prefix
        let abs = Path::new(r"\\?\C:\repo\src\main.rs");
        let root = Path::new(r"C:\repo");
        let result = normalize_event_path(abs, root);
        assert_eq!(result, Some("src/main.rs".to_string()));
    }

    #[test]
    #[cfg(windows)]
    fn test_normalize_event_path_outside_repo() {
        // Path is completely outside the repo root — should return None
        let abs = Path::new(r"C:\other\file.rs");
        let root = Path::new(r"C:\repo");
        let result = normalize_event_path(abs, root);
        assert_eq!(result, None);
    }

    #[test]
    fn test_normalize_event_path_forward_slash() {
        // Forward-slash paths (Linux/macOS) should also work
        let abs = Path::new("/home/user/project/src/lib.rs");
        let root = Path::new("/home/user/project");
        let result = normalize_event_path(abs, root);
        assert_eq!(result, Some("src/lib.rs".to_string()));
    }

    #[test]
    fn test_normalize_event_path_nested_subdir() {
        let abs = Path::new("/repo/a/b/c.rs");
        let root = Path::new("/repo");
        let result = normalize_event_path(abs, root);
        assert_eq!(result, Some("a/b/c.rs".to_string()));
    }

    // --- Plan 02: Language filter tests ---

    #[test]
    fn test_supported_language_rs() {
        let path = Path::new("src/main.rs");
        assert_eq!(supported_language(path), Some(LanguageId::Rust));
    }

    #[test]
    fn test_supported_language_py() {
        let path = Path::new("scripts/build.py");
        assert_eq!(supported_language(path), Some(LanguageId::Python));
    }

    #[test]
    fn test_supported_language_ts() {
        let path = Path::new("src/app.ts");
        assert_eq!(supported_language(path), Some(LanguageId::TypeScript));
    }

    #[test]
    fn test_supported_language_go() {
        let path = Path::new("main.go");
        assert_eq!(supported_language(path), Some(LanguageId::Go));
    }

    #[test]
    fn test_supported_language_java() {
        let path = Path::new("Main.java");
        assert_eq!(supported_language(path), Some(LanguageId::Java));
    }

    #[test]
    fn test_supported_language_txt() {
        let path = Path::new("README.txt");
        assert_eq!(supported_language(path), Some(LanguageId::Text));
    }

    #[test]
    fn test_supported_language_md() {
        let path = Path::new("README.md");
        assert_eq!(supported_language(path), Some(LanguageId::Markdown));
    }

    #[test]
    fn test_supported_language_no_extension() {
        let path = Path::new("Makefile");
        assert_eq!(supported_language(path), None);
    }

    #[test]
    fn test_supported_language_extensionless_knowledge() {
        let path = Path::new("docs/README");
        assert_eq!(supported_language(path), Some(LanguageId::Text));
    }

    #[test]
    fn watcher_admits_sparse_gguf_before_read() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("main.rs"), b"fn main() {}").unwrap();
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        let relative_path = "weights.gguf";
        let absolute_path = tmp.path().join(relative_path);
        std::fs::File::create(&absolute_path)
            .unwrap()
            .set_len(crate::domain::index::HARD_SKIP_BYTES + 1)
            .unwrap();
        let result = read_and_index_with_stable_read(
            relative_path,
            &absolute_path,
            &shared,
            None::<LanguageId>,
            expected_gen,
            |_path, _stamp| panic!("metadata ceiling must short-circuit before full read"),
        );
        assert_eq!(result, ReindexResult::Skipped);

        let dispositions = shared.terminal_dispositions();
        assert_eq!(
            dispositions
                .iter()
                .find(|(path, _)| path == relative_path)
                .map(|(_, disposition)| disposition),
            Some(&crate::domain::FileDisposition::HardSkip {
                reason: crate::domain::HardSkipReason::PerFileCeiling,
            }),
            "unknown-extension files must reach metadata admission and hard-skip before full read"
        );
        assert!(
            shared.read().get_file(relative_path).is_none(),
            "a sparse artifact must never enter the in-memory content index"
        );
    }

    #[test]
    fn watcher_file_change_publishes_all_lanes_once() {
        let tmp = TempDir::new().unwrap();
        let relative_path = "main.rs";
        let absolute_path = tmp.path().join(relative_path);
        std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
        std::fs::write(tmp.path().join("docs/guide.md"), b"[runtime](../main.rs)\n").unwrap();
        std::fs::File::create(&absolute_path)
            .unwrap()
            .set_len(crate::domain::index::METADATA_ONLY_CODE_BYTES + 1)
            .unwrap();
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        let publication_before = shared.published_state().generation;
        assert!(
            shared.read().get_file(relative_path).is_none(),
            "precondition: oversized code starts catalog-only"
        );
        assert!(matches!(
            shared.published_generation().bridge.forward[0].resolution,
            crate::live_index::knowledge_bridge::BridgeResolution::Missing
        ));

        std::fs::write(
            &absolute_path,
            b"fn updated() { helper(); }\nfn helper() {}",
        )
        .unwrap();
        assert_eq!(
            maybe_reindex(
                relative_path,
                &absolute_path,
                &shared,
                LanguageId::Rust,
                expected_gen,
            ),
            ReindexResult::Reindexed
        );

        let publication_after = shared.published_state();
        assert_eq!(
            publication_after.generation,
            publication_before + 1,
            "one logical watcher update must publish exactly one generation"
        );
        let index = shared.read();
        let indexed = index
            .get_file(relative_path)
            .expect("updated code content must be present");
        assert_eq!(
            indexed.content,
            b"fn updated() { helper(); }\nfn helper() {}"
        );
        assert!(
            indexed
                .symbols
                .iter()
                .any(|symbol| symbol.name == "updated"),
            "derived symbol state must come from the same update"
        );
        assert!(
            index
                .compatibility_skipped_files()
                .iter()
                .all(|skipped| skipped.path != relative_path),
            "the updated path cannot remain in the catalog-only projection"
        );
        drop(index);
        assert_eq!(
            shared
                .terminal_dispositions()
                .iter()
                .find(|(path, _)| path == relative_path)
                .map(|(_, disposition)| disposition),
            Some(&FileDisposition::Indexed {
                targets: crate::domain::IndexTargets::Code,
                parse_status: crate::domain::index::ParseStatus::Parsed,
            }),
            "content, target, and parse disposition must publish together"
        );
        let published = shared.published_generation();
        assert_eq!(
            published.authority.content_generation, published.content_generation,
            "authority must be derived inside the watcher publication"
        );
        assert_eq!(
            published.authority.source.as_ref(),
            published.source.as_deref(),
            "authority and bundle must carry one source identity"
        );
        assert_eq!(
            published.authority.source_version.as_ref(),
            published.source_version.as_deref(),
            "authority and bundle must carry one source-version tip"
        );
        assert!(
            published
                .authority
                .records
                .iter()
                .any(|record| record.unit.path == "docs/guide.md"),
            "the same watcher publication must contain the affected knowledge unit"
        );
        let link = &published.bridge.forward[0];
        assert_eq!(
            link.evidence.content_generation,
            published.content_generation
        );
        let crate::live_index::knowledge_bridge::BridgeResolution::ResolvedExact(anchor) =
            &link.resolution
        else {
            panic!("the watcher publication must repair its bridge in the same root");
        };
        assert_eq!(anchor.content_generation, published.content_generation);
        assert_eq!(
            published
                .bridge
                .reverse_exact
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            1
        );
    }

    #[test]
    fn watcher_content_policy_withholds_sensitive_bytes_before_publication() {
        let tmp = TempDir::new().unwrap();
        let relative_path = ".env.example";
        let seed = format!("{}=placeholder\n", ["to", "ken"].concat());
        let absolute_path = create_test_source(tmp.path(), relative_path, seed.as_bytes());
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        assert!(
            shared.read().get_file(relative_path).is_some(),
            "safe template content must establish a resident precondition"
        );

        let payload = format!("{}={}\n", ["to", "ken"].concat(), runtime_canary());
        std::fs::write(&absolute_path, payload.as_bytes()).unwrap();
        let expected_gen = shared.current_project_generation();
        assert_eq!(
            maybe_reindex(
                relative_path,
                &absolute_path,
                &shared,
                LanguageId::Env,
                expected_gen,
            ),
            ReindexResult::Skipped
        );

        let index = shared.read();
        assert!(
            index.get_file(relative_path).is_none(),
            "detector-positive bytes must not remain resident"
        );
        assert!(index.files.values().all(|file| {
            !file
                .content
                .windows(runtime_canary().len())
                .any(|window| window == runtime_canary().as_bytes())
        }));
        let disposition = index
            .manifest_entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some(relative_path))
            .map(|entry| &entry.disposition)
            .expect("catalog entry must remain visible");
        assert!(matches!(
            disposition,
            FileDisposition::MetadataOnly {
                reason: MetadataOnlyReason::SensitiveContent {
                    rule_ids,
                    finding_count
                }
            } if !rule_ids.is_empty() && *finding_count > 0
        ));
    }

    #[test]
    fn reconcile_rescout_discovers_new_text_files() {
        let project = TempDir::new().unwrap();
        let source_dir = project.path().join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("lib.rs"), b"fn seed() {}").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();

        let notes_dir = project.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        std::fs::write(
            notes_dir.join("new-guide.md"),
            b"# New guide\n\nFresh knowledge.\n",
        )
        .unwrap();
        std::fs::write(notes_dir.join("new-facts.txt"), b"one fresh fact\n").unwrap();

        assert_eq!(
            reconcile_stale_files(project.path(), &shared),
            2,
            "a fresh manifest diff must discover both missed creates"
        );

        let plan = shared
            .scout_plan()
            .expect("reconciliation must publish the fresh scout plan");
        for relative_path in ["notes/new-guide.md", "notes/new-facts.txt"] {
            let entry = plan
                .entries
                .iter()
                .find(|entry| entry.path.normalized_utf8.as_deref() == Some(relative_path))
                .unwrap_or_else(|| panic!("fresh scout plan omitted {relative_path}"));
            assert!(
                matches!(
                    entry.decision,
                    ScoutDecision::Ingest {
                        targets: crate::domain::IndexTargets::Knowledge
                    }
                ),
                "new text must retain its authoritative knowledge target"
            );
        }

        let index = shared.read();
        let markdown = index
            .get_file("notes/new-guide.md")
            .expect("currently executable Markdown must be indexed");
        assert_eq!(markdown.content, b"# New guide\n\nFresh knowledge.\n");
        drop(index);
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, _)| path == "notes/new-facts.txt"),
            "a text target awaiting Gate F extraction must still retain a terminal outcome"
        );
    }

    #[test]
    fn reconcile_repairs_bridge_create_and_remove_in_the_same_published_root() {
        let project = TempDir::new().unwrap();
        std::fs::create_dir_all(project.path().join("docs")).unwrap();
        std::fs::write(
            project.path().join("docs/guide.md"),
            b"[runtime](../src/runtime.rs)\n",
        )
        .unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        assert!(matches!(
            shared.published_generation().bridge.forward[0].resolution,
            crate::live_index::knowledge_bridge::BridgeResolution::Missing
        ));

        std::fs::create_dir_all(project.path().join("src")).unwrap();
        std::fs::write(
            project.path().join("src/runtime.rs"),
            b"pub fn runtime() {}\n",
        )
        .unwrap();
        assert_eq!(reconcile_stale_files(project.path(), &shared), 1);
        let created = shared.published_generation();
        let created_link = &created.bridge.forward[0];
        assert_eq!(
            created_link.evidence.content_generation,
            created.content_generation
        );
        let crate::live_index::knowledge_bridge::BridgeResolution::ResolvedExact(anchor) =
            &created_link.resolution
        else {
            panic!("reconciliation must publish the repaired exact edge atomically");
        };
        assert_eq!(anchor.content_generation, created.content_generation);
        assert_eq!(
            created
                .bridge
                .reverse_exact
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            1
        );

        std::fs::remove_file(project.path().join("src/runtime.rs")).unwrap();
        assert_eq!(reconcile_stale_files(project.path(), &shared), 1);
        let removed = shared.published_generation();
        assert!(matches!(
            removed.bridge.forward[0].resolution,
            crate::live_index::knowledge_bridge::BridgeResolution::Missing
        ));
        assert!(removed.bridge.reverse_exact.is_empty());
    }

    #[test]
    fn reconcile_rescout_tracks_catalog_only_shrink_and_delete() {
        let project = TempDir::new().unwrap();
        let shrink_relative = "catalog-shrink.json";
        let delete_relative = "catalog-delete.json";
        let shrink_path = project.path().join(shrink_relative);
        let delete_path = project.path().join(delete_relative);
        for path in [&shrink_path, &delete_path] {
            std::fs::File::create(path)
                .unwrap()
                .set_len(crate::domain::index::METADATA_ONLY_BYTES + 1)
                .unwrap();
        }

        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        {
            let index = shared.read();
            assert_eq!(index.tier_counts(), (0, 2, 0));
            assert!(index.get_file(shrink_relative).is_none());
            assert!(index.get_file(delete_relative).is_none());
        }

        std::fs::write(&shrink_path, b"{\"fresh\":true}\n").unwrap();
        std::fs::remove_file(&delete_path).unwrap();

        assert_eq!(
            reconcile_stale_files(project.path(), &shared),
            2,
            "manifest reconciliation must repair a catalog-only shrink and delete"
        );

        let index = shared.read();
        assert!(
            index.get_file(shrink_relative).is_some(),
            "the shrunk file must be admitted and indexed"
        );
        assert!(index.get_file(delete_relative).is_none());
        assert!(
            index
                .compatibility_skipped_files()
                .iter()
                .all(|entry| entry.path != shrink_relative && entry.path != delete_relative),
            "neither repaired path may retain a stale catalog projection"
        );
        drop(index);

        let dispositions = shared.terminal_dispositions();
        assert!(matches!(
            dispositions
                .iter()
                .find(|(path, _)| path == shrink_relative)
                .map(|(_, disposition)| disposition),
            Some(FileDisposition::Indexed { .. })
        ));
        assert!(
            dispositions.iter().all(|(path, _)| path != delete_relative),
            "deleted catalog-only state must be removed from every retained lane"
        );

        let plan = shared.scout_plan().expect("fresh manifest must publish");
        assert!(
            plan.entries.iter().any(|entry| {
                entry.path.normalized_utf8.as_deref() == Some(shrink_relative)
                    && matches!(entry.decision, ScoutDecision::Ingest { .. })
            }),
            "the fresh manifest must carry the shrunk path as ingested"
        );
        assert!(
            plan.entries
                .iter()
                .all(|entry| entry.path.normalized_utf8.as_deref() != Some(delete_relative))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn overflow_fresh_instance_repairs_missed_create_delete() {
        let project = TempDir::new().unwrap();
        let missed_delete = "missed-delete.rs";
        let missed_create = "missed-create.rs";
        std::fs::write(project.path().join(missed_delete), b"fn old() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        std::fs::remove_file(project.path().join(missed_delete)).unwrap();
        std::fs::write(
            project.path().join(missed_create),
            b"fn created_before_watch() {}\n",
        )
        .unwrap();

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = Arc::new(AtomicBool::new(false));
        let watcher_task = tokio::spawn(run_watcher_with_stop(
            project.path().to_path_buf(),
            shared.clone(),
            Arc::clone(&watcher_info),
            Arc::clone(&stop_token),
        ));
        let fresh_reconcile = tokio::time::timeout(Duration::from_millis(750), async {
            loop {
                if watcher_info.lock().last_reconcile_at.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        stop_token.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_secs(2), watcher_task)
            .await
            .expect("watcher must stop promptly")
            .expect("watcher task must not panic");

        assert!(
            fresh_reconcile.is_ok(),
            "a newly registered watcher must immediately reconcile uncertainty"
        );
        {
            let index = shared.read();
            assert!(index.get_file(missed_delete).is_none());
            assert!(index.get_file(missed_create).is_some());
        }

        let overflow_delete = missed_create;
        let overflow_create = "overflow-create.rs";
        std::fs::remove_file(project.path().join(overflow_delete)).unwrap();
        std::fs::write(
            project.path().join(overflow_create),
            b"fn created_during_overflow() {}\n",
        )
        .unwrap();
        assert_eq!(
            reconcile_stale_files(project.path(), &shared),
            2,
            "the full-manifest engine used after overflow must repair missed create/delete"
        );
        let index = shared.read();
        assert!(index.get_file(overflow_delete).is_none());
        assert!(index.get_file(overflow_create).is_some());
    }

    #[test]
    fn notify_overflow_error_routes_to_reconciliation_cause() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("main.rs"), b"fn main() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);
        let expected_gen = shared.current_project_generation();
        let errors = [notify::Error::generic("synthetic overflow")];

        let observed = handle_notify_errors(&errors, || {
            reconcile_for_cause(
                project.path(),
                &shared,
                &watcher_info,
                &stop_token,
                expected_gen,
                ReconciliationCause::Overflow,
            );
        });

        assert_eq!(observed, 1);
        let info = watcher_info.lock();
        assert_eq!(info.overflow_count, 1);
        assert!(info.last_overflow_at.is_some());
    }

    #[test]
    fn stale_file_batch_cannot_mutate_any_lane() {
        let project_a = TempDir::new().unwrap();
        let project_b = TempDir::new().unwrap();
        std::fs::write(project_a.path().join("old-a.rs"), b"fn old_a() {}\n").unwrap();
        std::fs::write(
            project_b.path().join("current-b.rs"),
            b"fn current_b() {}\n",
        )
        .unwrap();
        std::fs::File::create(project_b.path().join("catalog-b.json"))
            .unwrap()
            .set_len(crate::domain::index::METADATA_ONLY_BYTES + 1)
            .unwrap();

        let shared = crate::live_index::LiveIndex::load(project_a.path()).unwrap();
        let stale_generation = shared.current_project_generation();
        shared.reload(project_b.path()).unwrap();

        let publication_before = shared.published_state().generation;
        let files_before = {
            let index = shared.read();
            let mut files = index
                .all_files()
                .map(|(path, file)| (path.clone(), file.content_hash.clone()))
                .collect::<Vec<_>>();
            files.sort();
            files
        };
        let skipped_before = {
            let index = shared.read();
            let mut skipped = index
                .compatibility_skipped_files()
                .iter()
                .map(|entry| {
                    (
                        entry.path.clone(),
                        entry.size,
                        entry.decision.tier,
                        entry.decision.reason,
                    )
                })
                .collect::<Vec<_>>();
            skipped.sort_by(|left, right| left.0.cmp(&right.0));
            skipped
        };
        let plan_before = shared
            .scout_plan()
            .expect("project B must have a scout plan")
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.path.normalized_utf8.clone(),
                    entry.stamp.clone(),
                    entry.decision.clone(),
                )
            })
            .collect::<Vec<_>>();
        let dispositions_before = (*shared.terminal_dispositions()).clone();
        let freshness_before = (*shared.freshness_status()).clone();
        let rejected_before = shared.current_rejected_stale_mutations();

        std::fs::remove_file(project_a.path().join("old-a.rs")).unwrap();
        std::fs::write(project_a.path().join("new-a.rs"), b"fn new_a() {}\n").unwrap();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);
        assert_eq!(
            reconcile_for_cause(
                project_a.path(),
                &shared,
                &watcher_info,
                &stop_token,
                stale_generation,
                ReconciliationCause::Periodic,
            ),
            0
        );

        assert_eq!(shared.published_state().generation, publication_before);
        let files_after = {
            let index = shared.read();
            let mut files = index
                .all_files()
                .map(|(path, file)| (path.clone(), file.content_hash.clone()))
                .collect::<Vec<_>>();
            files.sort();
            files
        };
        assert_eq!(files_after, files_before, "content/derived lane mutated");
        let skipped_after = {
            let index = shared.read();
            let mut skipped = index
                .compatibility_skipped_files()
                .iter()
                .map(|entry| {
                    (
                        entry.path.clone(),
                        entry.size,
                        entry.decision.tier,
                        entry.decision.reason,
                    )
                })
                .collect::<Vec<_>>();
            skipped.sort_by(|left, right| left.0.cmp(&right.0));
            skipped
        };
        assert_eq!(skipped_after, skipped_before, "catalog lane mutated");
        let plan_after = shared
            .scout_plan()
            .expect("project B scout plan must survive")
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.path.normalized_utf8.clone(),
                    entry.stamp.clone(),
                    entry.decision.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(plan_after, plan_before, "manifest lane mutated");
        assert_eq!(*shared.terminal_dispositions(), dispositions_before);
        assert_eq!(*shared.freshness_status(), freshness_before);
        assert!(shared.current_rejected_stale_mutations() > rejected_before);
        let info = watcher_info.lock();
        assert_eq!(info.stale_files_found, 0);
        assert!(
            info.last_reconcile_at.is_none(),
            "a rejected stale batch cannot claim the active project was reconciled"
        );
    }

    #[test]
    fn reconcile_racing_watcher_event_loses_neither_update() {
        let project = TempDir::new().unwrap();
        let reconcile_relative = "reconcile.rs";
        let watcher_relative = "watcher.rs";
        let reconcile_path = project.path().join(reconcile_relative);
        let watcher_path = project.path().join(watcher_relative);
        std::fs::write(&reconcile_path, b"fn reconcile_initial() {}\n").unwrap();
        std::fs::write(&watcher_path, b"fn watcher_initial() {}\n").unwrap();

        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        std::fs::write(
            &reconcile_path,
            b"fn reconcile_newer() { reconcile_helper(); }\nfn reconcile_helper() {}\n",
        )
        .unwrap();

        let scouted = Arc::new(std::sync::Barrier::new(2));
        let resume = Arc::new(std::sync::Barrier::new(2));
        let thread_shared = Arc::clone(&shared);
        let thread_root = project.path().to_path_buf();
        let thread_scouted = Arc::clone(&scouted);
        let thread_resume = Arc::clone(&resume);
        let reconcile_thread = std::thread::spawn(move || {
            reconcile_stale_files_with_stop_and_hook(
                &thread_root,
                &thread_shared,
                || false,
                expected_gen,
                || {
                    crate::discovery::scout_repository_with_exclusions(
                        &thread_root,
                        &thread_shared.source_exclusions(),
                    )
                },
                || {
                    thread_scouted.wait();
                    thread_resume.wait();
                },
            )
        });

        scouted.wait();
        let watcher_content =
            b"fn watcher_newer() { watcher_helper(); watcher_helper(); }\nfn watcher_helper() {}\n";
        std::fs::write(&watcher_path, watcher_content).unwrap();
        let watcher_result = maybe_reindex(
            watcher_relative,
            &watcher_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );
        resume.wait();
        let repaired = reconcile_thread.join().unwrap();

        assert_eq!(watcher_result, ReindexResult::Reindexed);
        assert_eq!(
            repaired.repaired, 1,
            "the stale reconcile input must be repaired"
        );
        let index = shared.read();
        let reconciled = index
            .get_file(reconcile_relative)
            .expect("reconciliation update must survive the race");
        assert!(
            reconciled
                .symbols
                .iter()
                .any(|symbol| symbol.name == "reconcile_newer")
        );
        let watched = index
            .get_file(watcher_relative)
            .expect("watcher update must survive the race");
        assert_eq!(watched.content, watcher_content);
        assert!(
            watched
                .symbols
                .iter()
                .any(|symbol| symbol.name == "watcher_newer")
        );
        drop(index);

        let watcher_size = std::fs::metadata(&watcher_path).unwrap().len();
        let plan = shared
            .scout_plan()
            .expect("the reconciled scout plan must remain available");
        let watcher_entry = plan
            .entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some(watcher_relative))
            .expect("the reconciled scout plan must retain the watcher path");
        assert_eq!(
            watcher_entry.stamp.size, watcher_size,
            "reconciliation must not overwrite the racing watcher event with its stale scout entry"
        );
    }

    #[test]
    fn degraded_walk_retries_until_complete_even_when_digest_equal() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("main.rs"), b"fn main() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        let complete_plan = (*shared.scout_plan().expect("cold scout plan")).clone();
        let mut degraded_plan = complete_plan.clone();
        degraded_plan.issues.push(crate::domain::ScoutIssue {
            path_id: Some("transient-walk-entry".to_string()),
            safe_path: Some("temporarily-locked".to_string()),
            kind: crate::domain::ScoutIssueKind::DirectoryEntryUnreadable {
                kind: crate::domain::AccessErrorKind::PermissionDenied,
            },
            safe_message: "directory entry unavailable".to_string(),
        });
        crate::discovery::refresh_scout_plan(&mut degraded_plan).unwrap();
        assert_eq!(degraded_plan.entries, complete_plan.entries);
        assert_eq!(
            degraded_plan.coverage,
            crate::domain::CoverageStatus::Degraded
        );

        let mut plans =
            std::collections::VecDeque::from([degraded_plan.clone(), degraded_plan, complete_plan]);
        let mut attempts = 0usize;
        let mut delays = Vec::new();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);
        let repaired = reconcile_for_cause_with(
            project.path(),
            &shared,
            &watcher_info,
            &stop_token,
            expected_gen,
            ReconciliationCause::Periodic,
            || {
                attempts += 1;
                let next_plan = plans.pop_front().expect("bounded scout observation");
                let baseline = shared.scout_plan();
                assert!(shared.publish_reconciled_scout_plan_at_generation(
                    baseline.as_deref(),
                    next_plan,
                    expected_gen,
                ));
                0
            },
            |delay| delays.push(delay),
        );

        assert_eq!(repaired, 0, "equal entry observations need no file repair");
        assert_eq!(attempts, 3, "Degraded equality must keep retrying");
        assert_eq!(
            delays,
            [Duration::from_millis(50), Duration::from_millis(100)]
        );
        assert_eq!(
            shared.scout_plan().expect("converged plan").coverage,
            crate::domain::CoverageStatus::Complete
        );
    }

    #[test]
    fn unchanged_complete_reconcile_does_not_publish_content_generation() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("main.rs"), b"fn main() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let generation_before = shared.published_state().generation;

        assert_eq!(reconcile_stale_files(project.path(), &shared), 0);
        assert_eq!(
            shared.published_state().generation,
            generation_before,
            "an unchanged Complete observation cannot publish a content generation"
        );
        assert_eq!(
            shared.scout_plan().expect("complete plan").coverage,
            crate::domain::CoverageStatus::Complete
        );
    }

    #[test]
    fn degraded_rescout_missing_path_retains_last_valid_state() {
        let project = TempDir::new().unwrap();
        let relative_path = "main.rs";
        let content = b"fn still_valid() {}\n";
        std::fs::write(project.path().join(relative_path), content).unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        let mut degraded_plan = (*shared.scout_plan().expect("cold scout plan")).clone();
        degraded_plan
            .entries
            .retain(|entry| entry.path.normalized_utf8.as_deref() != Some(relative_path));
        degraded_plan.issues.push(crate::domain::ScoutIssue {
            path_id: Some("transient-walk-entry".to_string()),
            safe_path: Some(relative_path.to_string()),
            kind: crate::domain::ScoutIssueKind::DirectoryEntryUnreadable {
                kind: crate::domain::AccessErrorKind::PermissionDenied,
            },
            safe_message: "directory entry unavailable".to_string(),
        });
        crate::discovery::refresh_scout_plan(&mut degraded_plan).unwrap();
        assert_eq!(
            degraded_plan.coverage,
            crate::domain::CoverageStatus::Degraded
        );

        reconcile_stale_files_with_stop_and_hook(
            project.path(),
            &shared,
            || false,
            expected_gen,
            || Ok(degraded_plan),
            || {},
        );

        let index = shared.read();
        assert_eq!(
            index
                .get_file(relative_path)
                .expect("degraded absence cannot delete last-valid state")
                .content,
            content
        );
        drop(index);
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, disposition)| {
                    path == relative_path && matches!(disposition, FileDisposition::Indexed { .. })
                })
        );
    }

    #[test]
    fn failed_rescout_uses_bounded_degraded_retry_schedule() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("main.rs"), b"fn main() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);
        let mut attempts = 0usize;
        let mut delays = Vec::new();

        reconcile_for_cause_with(
            project.path(),
            &shared,
            &watcher_info,
            &stop_token,
            expected_gen,
            ReconciliationCause::Periodic,
            || {
                attempts += 1;
                reconcile_stale_files_with_stop_and_hook(
                    project.path(),
                    &shared,
                    || false,
                    expected_gen,
                    || anyhow::bail!("transient rescout failure"),
                    || {},
                )
            },
            |delay| delays.push(delay),
        );

        assert_eq!(attempts, 5, "a failed rescout must remain retryable");
        assert_eq!(
            delays,
            [
                Duration::from_millis(50),
                Duration::from_millis(100),
                Duration::from_millis(200),
                Duration::from_millis(400),
            ]
        );
    }

    #[test]
    fn transient_unreadable_self_heals_via_reconciliation() {
        let project = TempDir::new().unwrap();
        let relative_path = "main.rs";
        let absolute_path = project.path().join(relative_path);
        let content = b"fn still_current() {}\n";
        std::fs::write(&absolute_path, content).unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        let observed = read_and_index_with_stable_read(
            relative_path,
            &absolute_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
            |_path, _stamp| crate::live_index::store::StableReadOutcome::Unreadable {
                stage: crate::domain::AccessStage::FullRead,
                kind: crate::domain::AccessErrorKind::PermissionDenied,
            },
        );
        assert!(matches!(observed, ReindexResult::ReadError(_)));
        assert_eq!(
            shared.scout_plan().expect("degraded plan").coverage,
            crate::domain::CoverageStatus::Degraded,
            "a transient full-read failure must defeat equal-entry no-op"
        );
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, disposition)| path == relative_path
                    && matches!(disposition, FileDisposition::Unreadable { .. }))
        );
        assert_eq!(
            shared
                .read()
                .get_file(relative_path)
                .expect("degraded observation must retain last-valid content")
                .content,
            content
        );
        assert_eq!(
            shared.read().tier_counts(),
            (0, 1, 0),
            "the manifest disposition is authoritative while last-valid bytes are retained"
        );
        assert_eq!(
            shared
                .read()
                .capture_admission_tier_lookup_view(relative_path)
                .expect("transient path remains catalog-visible")
                .tier,
            crate::domain::index::AdmissionTier::MetadataOnly,
            "compatibility lookup must follow the manifest, not retained bytes"
        );

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);
        assert_eq!(
            reconcile_for_cause(
                project.path(),
                &shared,
                &watcher_info,
                &stop_token,
                expected_gen,
                ReconciliationCause::Periodic,
            ),
            1,
            "equal-entry transient state must be re-observed"
        );
        assert_eq!(
            shared.scout_plan().expect("converged plan").coverage,
            crate::domain::CoverageStatus::Complete
        );
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, disposition)| path == relative_path
                    && matches!(disposition, FileDisposition::Indexed { .. }))
        );
        assert_eq!(shared.read().tier_counts(), (1, 0, 0));
        assert_eq!(
            shared
                .read()
                .get_file(relative_path)
                .expect("last-valid content remains indexed")
                .content,
            content
        );
    }

    #[test]
    fn transient_unstable_read_self_heals_via_reconciliation() {
        let project = TempDir::new().unwrap();
        let relative_path = "main.rs";
        let absolute_path = project.path().join(relative_path);
        std::fs::write(&absolute_path, b"fn stable() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        assert!(matches!(
            read_and_index_with_stable_read(
                relative_path,
                &absolute_path,
                &shared,
                LanguageId::Rust,
                expected_gen,
                |_path, _stamp| crate::live_index::store::StableReadOutcome::UnstableDuringRead,
            ),
            ReindexResult::ReadError(_)
        ));
        assert_eq!(
            shared.scout_plan().expect("degraded plan").coverage,
            crate::domain::CoverageStatus::Degraded
        );
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, disposition)| path == relative_path
                    && matches!(disposition, FileDisposition::UnstableDuringRead))
        );

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);
        assert_eq!(
            reconcile_for_cause(
                project.path(),
                &shared,
                &watcher_info,
                &stop_token,
                expected_gen,
                ReconciliationCause::Periodic,
            ),
            1
        );
        assert_eq!(
            shared.scout_plan().expect("converged plan").coverage,
            crate::domain::CoverageStatus::Complete
        );
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, disposition)| path == relative_path
                    && matches!(disposition, FileDisposition::Indexed { .. }))
        );
    }

    #[test]
    fn sibling_recovery_cannot_hide_another_read_transient() {
        let project = TempDir::new().unwrap();
        let first_relative = "first.rs";
        let second_relative = "second.rs";
        let first_path = project.path().join(first_relative);
        let second_path = project.path().join(second_relative);
        std::fs::write(&first_path, b"fn first() {}\n").unwrap();
        std::fs::write(&second_path, b"fn second() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        for (relative_path, absolute_path) in [
            (first_relative, &first_path),
            (second_relative, &second_path),
        ] {
            assert!(matches!(
                read_and_index_with_stable_read(
                    relative_path,
                    absolute_path,
                    &shared,
                    LanguageId::Rust,
                    expected_gen,
                    |_path, _stamp| {
                        crate::live_index::store::StableReadOutcome::UnstableDuringRead
                    },
                ),
                ReindexResult::ReadError(_)
            ));
        }
        assert_eq!(
            shared.scout_plan().expect("degraded plan").coverage,
            crate::domain::CoverageStatus::Degraded
        );

        std::fs::write(&second_path, b"fn second_recovered() {}\n").unwrap();
        assert!(matches!(
            read_and_index(
                second_relative,
                &second_path,
                &shared,
                LanguageId::Rust,
                expected_gen,
            ),
            ReindexResult::Reindexed
        ));

        assert_eq!(
            shared
                .scout_plan()
                .expect("remaining transient must keep coverage degraded")
                .coverage,
            crate::domain::CoverageStatus::Degraded
        );
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, disposition)| {
                    path == first_relative
                        && matches!(disposition, FileDisposition::UnstableDuringRead)
                })
        );
    }

    #[test]
    fn unchanged_circuit_breaker_abort_is_retried_by_reconciliation() {
        let project = TempDir::new().unwrap();
        let relative_path = "main.rs";
        let absolute_path = project.path().join(relative_path);
        std::fs::write(&absolute_path, b"fn main() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        let scouted = shared
            .scout_plan()
            .expect("cold scout plan")
            .entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some(relative_path))
            .expect("scouted main.rs")
            .clone();
        let publication_gen = shared.published_state().generation;

        assert!(shared.publish_terminal_disposition_at_generation(
            relative_path,
            scouted,
            FileDisposition::AbortedCircuitBreaker,
            expected_gen,
            publication_gen,
        ));
        assert_eq!(
            shared.scout_plan().expect("degraded plan").coverage,
            crate::domain::CoverageStatus::Degraded
        );

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);
        assert_eq!(
            reconcile_for_cause(
                project.path(),
                &shared,
                &watcher_info,
                &stop_token,
                expected_gen,
                ReconciliationCause::Periodic,
            ),
            1,
            "an unchanged breaker-aborted path must be re-observed"
        );
        assert!(
            shared
                .terminal_dispositions()
                .iter()
                .any(|(path, disposition)| {
                    path == relative_path && matches!(disposition, FileDisposition::Indexed { .. })
                })
        );
        assert_eq!(
            shared.scout_plan().expect("repaired plan").coverage,
            crate::domain::CoverageStatus::Complete
        );
    }

    #[test]
    fn uncertainty_signal_retriggers_settled_degradation() {
        let project = TempDir::new().unwrap();
        std::fs::write(project.path().join("main.rs"), b"fn main() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        let complete_plan = (*shared.scout_plan().expect("cold scout plan")).clone();
        let mut degraded_plan = complete_plan.clone();
        degraded_plan.issues.push(crate::domain::ScoutIssue {
            path_id: Some("persistent-walk-entry".to_string()),
            safe_path: Some("persistently-unavailable".to_string()),
            kind: crate::domain::ScoutIssueKind::DirectoryEntryUnreadable {
                kind: crate::domain::AccessErrorKind::PermissionDenied,
            },
            safe_message: "directory entry unavailable".to_string(),
        });
        crate::discovery::refresh_scout_plan(&mut degraded_plan).unwrap();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop_token = AtomicBool::new(false);

        let mut settled_attempts = 0usize;
        let mut settled_delays = Vec::new();
        reconcile_for_cause_with(
            project.path(),
            &shared,
            &watcher_info,
            &stop_token,
            expected_gen,
            ReconciliationCause::Periodic,
            || {
                settled_attempts += 1;
                let baseline = shared.scout_plan();
                assert!(shared.publish_reconciled_scout_plan_at_generation(
                    baseline.as_deref(),
                    degraded_plan.clone(),
                    expected_gen,
                ));
                0
            },
            |delay| settled_delays.push(delay),
        );
        assert_eq!(
            settled_attempts, 5,
            "persistent degradation must be bounded"
        );
        assert_eq!(settled_delays.len(), 4);
        assert_eq!(
            shared.scout_plan().expect("settled plan").coverage,
            crate::domain::CoverageStatus::Degraded
        );

        let mut retrigger_plans = std::collections::VecDeque::from([degraded_plan, complete_plan]);
        let mut retrigger_attempts = 0usize;
        let mut retrigger_delays = Vec::new();
        reconcile_for_cause_with(
            project.path(),
            &shared,
            &watcher_info,
            &stop_token,
            expected_gen,
            ReconciliationCause::Overflow,
            || {
                retrigger_attempts += 1;
                let next_plan = retrigger_plans
                    .pop_front()
                    .expect("new uncertainty retry observation");
                let baseline = shared.scout_plan();
                assert!(shared.publish_reconciled_scout_plan_at_generation(
                    baseline.as_deref(),
                    next_plan,
                    expected_gen,
                ));
                0
            },
            |delay| retrigger_delays.push(delay),
        );

        assert_eq!(
            retrigger_attempts, 2,
            "a new uncertainty signal must reopen repair"
        );
        assert_eq!(retrigger_delays, [Duration::from_millis(50)]);
        assert_eq!(
            shared.scout_plan().expect("recovered plan").coverage,
            crate::domain::CoverageStatus::Complete
        );
        assert_eq!(watcher_info.lock().overflow_count, 1);
    }

    // --- Plan 04-02: watcher incremental xref update (XREF-08) ---

    /// Proves that after `maybe_reindex` re-parses a file, the reverse_index
    /// reflects the new references and the old references are gone.
    ///
    /// We write a Rust file with an initial function call, confirm the reverse
    /// index contains it, then overwrite the file with a different call, call
    /// maybe_reindex again, and confirm the index now reflects the new call.
    #[test]
    fn test_maybe_reindex_updates_reverse_index_on_change() {
        use crate::domain::LanguageId;
        use crate::live_index::store::IndexedFile;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let rs_path = tmp
            .path()
            .join("tests")
            .join("generated")
            .join("lib.generated.rs");
        std::fs::create_dir_all(rs_path.parent().unwrap()).unwrap();

        // --- Initial content: calls `old_function` ---
        let initial_content = b"fn entry() { old_function(); }";
        std::fs::write(&rs_path, initial_content).unwrap();

        // Build the initial shared index by parsing the file directly.
        let rel_path = "tests/generated/lib.generated.rs";
        let shared: crate::live_index::store::SharedIndex = {
            let result = crate::parsing::process_file(rel_path, initial_content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, initial_content.to_vec());
            let mut index = crate::live_index::store::LiveIndex {
                files: std::collections::HashMap::new(),
                loaded_at: std::time::Instant::now(),
                loaded_at_system: std::time::SystemTime::now(),
                load_duration: std::time::Duration::ZERO,
                cb_state: crate::live_index::store::CircuitBreakerState::new(0.20),
                is_empty: false,
                load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
                snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
                reverse_index: std::collections::HashMap::new(),
                files_by_basename: std::collections::HashMap::new(),
                files_by_dir_component: std::collections::HashMap::new(),
                trigram_index: crate::live_index::trigram::TrigramIndex::new(),
                gitignore: None,
                manifest_entries: Vec::new(),
                coupling_store: None,
                local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
                indexed_root: None,
            };
            index.update_file(rel_path.to_string(), indexed);
            crate::live_index::SharedIndexHandle::shared(index)
        };

        // Confirm the reverse index contains "old_function".
        {
            let idx = shared.read();
            assert!(
                idx.reverse_index.contains_key("old_function"),
                "reverse_index should contain 'old_function' after initial parse"
            );
        }

        // --- Updated content: calls `new_function` instead ---
        let updated_content = b"fn entry() { new_function(); }";
        std::fs::write(&rs_path, updated_content).unwrap();

        // maybe_reindex detects a hash change and re-parses.
        let expected_gen = shared.current_project_generation();
        let result = maybe_reindex(rel_path, &rs_path, &shared, LanguageId::Rust, expected_gen);
        assert_eq!(
            result,
            ReindexResult::Reindexed,
            "file should be re-parsed on content change"
        );

        // Confirm reverse index now has "new_function" and not "old_function".
        {
            let idx = shared.read();
            assert!(
                idx.reverse_index.contains_key("new_function"),
                "reverse_index should contain 'new_function' after re-index"
            );
            assert!(
                !idx.reverse_index.contains_key("old_function"),
                "reverse_index should NOT contain 'old_function' after re-index"
            );
            let file = idx
                .get_file(rel_path)
                .expect("reindexed file should still exist");
            assert!(file.classification.is_code());
            assert!(file.classification.is_test);
            assert!(file.classification.is_generated);
        }
    }

    #[test]
    fn reindex_hard_excludes_runtime_state_without_gitignore_or_file_io() {
        // The hard-scope gate precedes metadata/content reads. An absent supported
        // file under `.symforge` therefore returns Skipped immediately rather than
        // exercising NotFound retries or minting index state.
        use crate::domain::LanguageId;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let rel_path = ".symforge/tee/missing.rs";
        let abs_path = tmp.path().join(".symforge").join("tee").join("missing.rs");

        let shared: crate::live_index::store::SharedIndex = {
            let index = crate::live_index::store::LiveIndex {
                files: std::collections::HashMap::new(),
                loaded_at: std::time::Instant::now(),
                loaded_at_system: std::time::SystemTime::now(),
                load_duration: std::time::Duration::ZERO,
                cb_state: crate::live_index::store::CircuitBreakerState::new(0.20),
                is_empty: false,
                load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
                snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
                reverse_index: std::collections::HashMap::new(),
                files_by_basename: std::collections::HashMap::new(),
                files_by_dir_component: std::collections::HashMap::new(),
                trigram_index: crate::live_index::trigram::TrigramIndex::new(),
                gitignore: None,
                manifest_entries: Vec::new(),
                coupling_store: None,
                local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
                indexed_root: None,
            };
            crate::live_index::SharedIndexHandle::shared(index)
        };

        let expected_gen = shared.current_project_generation();
        let result = maybe_reindex(rel_path, &abs_path, &shared, LanguageId::Yaml, expected_gen);
        assert_eq!(
            result,
            ReindexResult::Skipped,
            "runtime-state path must be hard-scope skipped before file I/O"
        );

        let idx = shared.read();
        assert!(
            idx.get_file(rel_path).is_none(),
            "runtime-state file must NOT be inserted into the parsed index"
        );
        assert_eq!(
            idx.tier_counts(),
            (0, 0, 0),
            "hard-scope skip must not mint a tier record; index stays empty"
        );
    }

    #[test]
    fn state_placement_nested_global_dir_is_excluded_from_scout_and_watcher() {
        use crate::discovery::{SourceExclusions, scout_repository_with_exclusions};
        use crate::domain::{
            AccessErrorKind, LanguageId, ProjectId, ProjectStateDir, StatePlacement,
            UserLocalPlacementReason,
        };
        use crate::live_index::store::LiveIndex;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let source_path = tmp.path().join("src").join("lib.rs");
        let state_path = tmp
            .path()
            .join("operator-state")
            .join("projects")
            .join("project-v1")
            .join("state.rs");
        std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        std::fs::write(&source_path, b"pub fn source() {}\n").unwrap();
        std::fs::write(&state_path, b"pub fn runtime_state() {}\n").unwrap();

        let placement = StatePlacement::UserLocal {
            directory: ProjectStateDir::new(state_path.parent().unwrap().to_path_buf()),
            root_id: ProjectId("project-v1".to_string()),
            reason: UserLocalPlacementReason::ProjectLocalUnavailable {
                safe_reason: AccessErrorKind::PermissionDenied,
            },
        };
        let exclusions = SourceExclusions::for_state_placement(tmp.path(), &placement);

        let plan = scout_repository_with_exclusions(tmp.path(), &exclusions)
            .expect("scout repository with resolved state exclusion");
        let scouted_paths: Vec<&str> = plan
            .entries
            .iter()
            .filter_map(|entry| entry.path.normalized_utf8.as_deref())
            .collect();
        assert!(scouted_paths.contains(&"src/lib.rs"));
        assert!(
            !scouted_paths.contains(&"operator-state/projects/project-v1/state.rs"),
            "resolved user-local state nested under the source root must stay outside the scout manifest: {scouted_paths:?}"
        );

        let shared = LiveIndex::empty();
        shared
            .reload_for_binding_with_exclusions(
                tmp.path(),
                placement.directory().cloned(),
                exclusions,
            )
            .expect("reload with resolved state exclusion");
        assert!(shared.read().get_file("src/lib.rs").is_some());
        assert!(
            shared
                .read()
                .get_file("operator-state/projects/project-v1/state.rs")
                .is_none(),
            "bulk reload must not ingest the nested state subtree"
        );

        let expected_gen = shared.current_project_generation();
        let result = maybe_reindex(
            "operator-state/projects/project-v1/state.rs",
            &state_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );
        assert_eq!(
            result,
            ReindexResult::Skipped,
            "watcher/freshen ingestion must reject the same nested state subtree"
        );
        assert!(
            shared
                .read()
                .get_file("operator-state/projects/project-v1/state.rs")
                .is_none()
        );
    }

    #[test]
    fn reindex_admits_repository_owned_hidden_knowledge_to_match_bulk_walk() {
        // Repository-owned hidden knowledge is in source scope and must have the
        // same membership under bulk load, watcher events, and freshen-on-read.
        use crate::domain::LanguageId;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let rel_path = ".github/workflows/ci.yml";
        let abs_path = tmp.path().join(".github").join("workflows").join("ci.yml");
        std::fs::create_dir_all(abs_path.parent().unwrap()).unwrap();
        std::fs::write(&abs_path, "name: ci\non: [push]\n").unwrap();

        let shared: crate::live_index::store::SharedIndex = {
            let index = crate::live_index::store::LiveIndex {
                files: std::collections::HashMap::new(),
                loaded_at: std::time::Instant::now(),
                loaded_at_system: std::time::SystemTime::now(),
                load_duration: std::time::Duration::ZERO,
                cb_state: crate::live_index::store::CircuitBreakerState::new(0.20),
                is_empty: false,
                load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
                snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
                reverse_index: std::collections::HashMap::new(),
                files_by_basename: std::collections::HashMap::new(),
                files_by_dir_component: std::collections::HashMap::new(),
                trigram_index: crate::live_index::trigram::TrigramIndex::new(),
                gitignore: None,
                manifest_entries: Vec::new(),
                coupling_store: None,
                local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
                indexed_root: None,
            };
            crate::live_index::SharedIndexHandle::shared(index)
        };

        let expected_gen = shared.current_project_generation();
        let result = maybe_reindex(rel_path, &abs_path, &shared, LanguageId::Yaml, expected_gen);
        assert_eq!(
            result,
            ReindexResult::Reindexed,
            "repository-owned hidden knowledge must be parsed and inserted"
        );

        let idx = shared.read();
        assert!(
            idx.get_file(rel_path).is_some(),
            "repository-owned hidden knowledge must be present in the parsed index"
        );
    }

    #[test]
    fn process_events_predicate_skips_gitignored_state_dir() {
        use crate::live_index::store::LiveIndex;
        use ignore::gitignore::GitignoreBuilder;

        // Reproduce SymForge's own root ignore rules: ignore every root-level dot
        // directory but explicitly re-include `.github` (as the repo's .gitignore
        // does via `/.*/` + `!/.github/`).
        let mut builder = GitignoreBuilder::new("/repo");
        builder.add_line(None, "/.*/").unwrap();
        builder.add_line(None, "!/.github/").unwrap();
        let gitignore = builder.build().unwrap();

        let index = LiveIndex {
            files: std::collections::HashMap::new(),
            loaded_at: std::time::Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: std::time::Duration::ZERO,
            cb_state: crate::live_index::store::CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: std::collections::HashMap::new(),
            files_by_basename: std::collections::HashMap::new(),
            files_by_dir_component: std::collections::HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
            gitignore: Some(gitignore),
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };

        // SymForge's own gitignored state dir must never be indexed, even though
        // tee snapshots are `.rs` files with a supported language.
        assert!(index.is_path_gitignored(".symforge/tee/1780038581944-000040-handlers.rs"));
        assert!(index.is_path_gitignored(".claude/settings.local.json"));
        // Real source, whitelisted `.github`, and committed `vendor/` stay indexable.
        assert!(!index.is_path_gitignored("src/sidecar/handlers.rs"));
        assert!(!index.is_path_gitignored(".github/workflows/ci.yml"));
        assert!(!index.is_path_gitignored("vendor/tree-sitter-scss/src/parser.c"));
        // Absolute paths are rejected defensively (the `ignore` crate requires
        // relative paths).
        assert!(!index.is_path_gitignored("/abs/path.rs"));
    }

    /// Confirms that maybe_reindex returns HashSkip when content has not changed.
    #[test]
    fn test_maybe_reindex_hash_skip_on_unchanged_content() {
        use crate::domain::LanguageId;
        use crate::live_index::store::IndexedFile;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let rs_path = tmp.path().join("a.rs");
        let content = b"fn foo() {}";
        std::fs::write(&rs_path, content).unwrap();

        let rel_path = "a.rs";
        let shared: crate::live_index::store::SharedIndex = {
            let result = crate::parsing::process_file(rel_path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            let mut index = crate::live_index::store::LiveIndex {
                files: std::collections::HashMap::new(),
                loaded_at: std::time::Instant::now(),
                loaded_at_system: std::time::SystemTime::now(),
                load_duration: std::time::Duration::ZERO,
                cb_state: crate::live_index::store::CircuitBreakerState::new(0.20),
                is_empty: false,
                load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
                snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
                reverse_index: std::collections::HashMap::new(),
                files_by_basename: std::collections::HashMap::new(),
                files_by_dir_component: std::collections::HashMap::new(),
                trigram_index: crate::live_index::trigram::TrigramIndex::new(),
                gitignore: None,
                manifest_entries: Vec::new(),
                coupling_store: None,
                local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
                indexed_root: None,
            };
            index.update_file(rel_path.to_string(), indexed);
            crate::live_index::SharedIndexHandle::shared(index)
        };

        // File content unchanged — expect HashSkip.
        let expected_gen = shared.current_project_generation();
        let result = maybe_reindex(rel_path, &rs_path, &shared, LanguageId::Rust, expected_gen);
        assert_eq!(
            result,
            ReindexResult::HashSkip,
            "unchanged content should produce HashSkip"
        );
    }

    #[test]
    fn test_read_and_index_preserves_crlf_bytes_and_hash() {
        use crate::domain::LanguageId;

        let tmp = TempDir::new().unwrap();
        let rs_path = tmp.path().join("src").join("lib.rs");
        std::fs::create_dir_all(rs_path.parent().unwrap()).unwrap();
        let content = b"fn entry() {\r\n    watched_call();\r\n}\r\n";
        std::fs::write(&rs_path, content).unwrap();

        let rel_path = "src/lib.rs";
        let shared: crate::live_index::store::SharedIndex = {
            let index = crate::live_index::store::LiveIndex {
                files: std::collections::HashMap::new(),
                loaded_at: std::time::Instant::now(),
                loaded_at_system: std::time::SystemTime::now(),
                load_duration: std::time::Duration::ZERO,
                cb_state: crate::live_index::store::CircuitBreakerState::new(0.20),
                is_empty: false,
                load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
                snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
                reverse_index: std::collections::HashMap::new(),
                files_by_basename: std::collections::HashMap::new(),
                files_by_dir_component: std::collections::HashMap::new(),
                trigram_index: crate::live_index::trigram::TrigramIndex::new(),
                gitignore: None,
                manifest_entries: Vec::new(),
                coupling_store: None,
                local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
                indexed_root: None,
            };
            crate::live_index::SharedIndexHandle::shared(index)
        };

        let expected_gen = shared.current_project_generation();
        let result = read_and_index(rel_path, &rs_path, &shared, LanguageId::Rust, expected_gen);
        assert_eq!(result, ReindexResult::Reindexed);

        let idx = shared.read();
        let file = idx
            .get_file(rel_path)
            .expect("watcher should index the CRLF file");
        assert_eq!(file.content, content);
        assert_eq!(file.byte_len, content.len() as u64);
        assert_eq!(file.content_hash, crate::hash::digest_hex(content));
    }

    #[test]
    fn test_maybe_reindex_retries_transient_not_found() {
        let tmp = TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let rel_path = "src/flaky.rs";
        let abs_path = tmp.path().join(rel_path);
        let content = b"fn flaky() -> usize { 1 }";
        std::fs::write(&abs_path, content).unwrap();

        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        std::fs::remove_file(&abs_path).unwrap();

        let restore_path = abs_path.clone();
        let restore = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            std::fs::write(&restore_path, content).unwrap();
        });

        let result = maybe_reindex(rel_path, &abs_path, &shared, LanguageId::Rust, expected_gen);
        restore.join().unwrap();

        assert_ne!(
            result,
            ReindexResult::Removed,
            "transient NotFound should be retried instead of removed immediately"
        );
        let index = shared.read();
        assert!(
            index.get_file(rel_path).is_some(),
            "transiently missing file should remain indexed after retry succeeds"
        );
    }

    #[test]
    fn test_maybe_reindex_removes_persistent_not_found() {
        let tmp = TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let rel_path = "src/deleted.rs";
        let abs_path = tmp.path().join(rel_path);
        std::fs::write(&abs_path, b"fn deleted() {}").unwrap();

        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        std::fs::remove_file(&abs_path).unwrap();

        let result = maybe_reindex(rel_path, &abs_path, &shared, LanguageId::Rust, expected_gen);

        assert_eq!(
            result,
            ReindexResult::Removed,
            "persistent NotFound should remove after bounded retries"
        );
        let index = shared.read();
        assert!(
            index.get_file(rel_path).is_none(),
            "persistently missing file should be removed from the index"
        );
    }

    #[test]
    fn slipped_past_cancellation_fence_increments_counter() {
        let project_a = TempDir::new().unwrap();
        let project_b = TempDir::new().unwrap();
        let a_src = project_a.path().join("src");
        let b_src = project_b.path().join("src");
        std::fs::create_dir_all(&a_src).unwrap();
        std::fs::create_dir_all(&b_src).unwrap();
        std::fs::write(a_src.join("a.rs"), b"fn a() {}").unwrap();
        std::fs::write(b_src.join("b.rs"), b"fn b() {}").unwrap();

        let shared = crate::live_index::LiveIndex::load(project_a.path()).unwrap();
        let stale_gen = shared.current_project_generation();
        let rejected_before = shared.current_rejected_stale_mutations();
        shared.reload(project_b.path()).unwrap();

        let repairs =
            reconcile_stale_files_with_stop(project_a.path(), &shared, || false, stale_gen);

        assert_eq!(
            repairs, 0,
            "a GenerationMismatch reconcile repairs zero bytes; it is a rejected \
             mutation, not a repair, so it must not count toward the repair total"
        );
        assert!(
            shared.current_rejected_stale_mutations() > rejected_before,
            "stale-generation watcher reconcile should be rejected by the fence"
        );
        let index = shared.read();
        assert!(
            index.get_file("src/b.rs").is_some(),
            "B file should survive stale-generation reconcile"
        );
    }

    /// Cold-start regression: a SAME-ROOT reload (the fire-and-forget
    /// `bg_index.reload(&bg_root)` main.rs runs when no snapshot exists) bumps
    /// the project generation AFTER the watcher captured `expected_gen` at spawn.
    /// The watcher's reconcile must SELF-HEAL against that advance — re-index the
    /// edited file — instead of pinning the stale spawn generation and removing
    /// the file forever via `GenerationMismatch`.
    ///
    /// Distinguishing signal: the live index still serves the watcher's own
    /// `repo_root` (`indexed_root` unchanged), so the advance is a same-project
    /// reload, not a cross-project retarget (which `indexed_root` would show and
    /// which `slipped_past_cancellation_fence_increments_counter` proves must
    /// still be rejected).
    #[test]
    fn cold_start_same_root_reload_reconcile_heals_not_removes() {
        let project = TempDir::new().unwrap();
        let src_dir = project.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let rel = "src/heals.rs";
        let abs = project.path().join(rel);
        std::fs::write(&abs, b"fn before() {}").unwrap();

        // Load the index for our project; this is the state the watcher spawns
        // against. `spawn_gen` is exactly what `run_watcher_with_stop` snapshots
        // once at L721.
        let shared = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let spawn_gen = shared.current_project_generation();
        assert!(
            shared.read().get_file(rel).is_some(),
            "precondition: file indexed after load"
        );

        // Simulate the cold-start fire-and-forget reload: SAME root, generation
        // advances past the watcher's spawn snapshot.
        shared.reload(project.path()).unwrap();
        assert_ne!(
            shared.current_project_generation(),
            spawn_gen,
            "reload must advance the project generation past the spawn snapshot"
        );

        // Edit the tracked file on disk (bump mtime + change content) so the
        // reconcile sweep sees it as stale and tries to re-index it.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&abs, b"fn before() {}\nfn healed() {}").unwrap();

        // Reconcile pinned to the STALE spawn generation, exactly as the watcher
        // does today (`expected_gen_for_reconcile = expected_gen`).
        let _ = reconcile_stale_files_with_stop(project.path(), &shared, || false, spawn_gen);

        // The edit must be INDEXED, not GenerationMismatch-removed.
        let index = shared.read();
        let file = index.get_file(rel).unwrap_or_else(|| {
            panic!(
                "cold-start same-root reload must NOT remove the edited file; \
                 stale spawn generation self-healed instead of rejecting"
            )
        });
        assert!(
            file.symbols.iter().any(|s| s.name == "healed"),
            "the reconcile must re-index the edited file so the new symbol appears"
        );
    }

    /// Repair-count honesty: `reconcile_stale_files_with_stop` returns the number
    /// of GENUINE repairs (files actually re-indexed or removed), which health
    /// renders as "reconcile repairs". A `GenerationMismatch` outcome is a NO-OP
    /// that repaired zero bytes (a cross-project retarget kept the stale fence and
    /// the store rejected the mutation), so it must NOT inflate the repair count.
    ///
    /// This is deliberately distinct from the store's `rejected_stale_mutations`
    /// counter, which DOES increment for the rejection (asserted by
    /// `slipped_past_cancellation_fence_increments_counter`). The two figures
    /// answer different questions: "how many files did we repair" vs. "how many
    /// stale-generation mutations did the fence reject".
    #[test]
    fn reconcile_repair_count_excludes_generation_mismatch_noops() {
        // --- Case 1: pure GenerationMismatch no-ops must NOT count as repairs. ---
        let project_a = TempDir::new().unwrap();
        let project_b = TempDir::new().unwrap();
        std::fs::create_dir_all(project_a.path().join("src")).unwrap();
        std::fs::create_dir_all(project_b.path().join("src")).unwrap();
        std::fs::write(project_a.path().join("src/a.rs"), b"fn a() {}").unwrap();
        std::fs::write(project_b.path().join("src/b.rs"), b"fn b() {}").unwrap();

        let shared = crate::live_index::LiveIndex::load(project_a.path()).unwrap();
        let stale_gen = shared.current_project_generation();
        // Retarget to B: advances the generation AND swaps `indexed_root`, so
        // `effective_fence_generation` keeps the stale spawn gen and every file
        // reconcile below resolves to `GenerationMismatch` (a repaired-zero no-op).
        shared.reload(project_b.path()).unwrap();

        let repairs =
            reconcile_stale_files_with_stop(project_a.path(), &shared, || false, stale_gen);
        assert_eq!(
            repairs, 0,
            "GenerationMismatch no-ops repair zero bytes and must not inflate the \
             repair count (they are rejected mutations, not repairs)"
        );

        // --- Case 2: a genuine StaleReindexed edit MUST count as a repair. ---
        let project = TempDir::new().unwrap();
        std::fs::create_dir_all(project.path().join("src")).unwrap();
        let rel = "src/edited.rs";
        let abs = project.path().join(rel);
        std::fs::write(&abs, b"fn before() {}").unwrap();

        let shared2 = crate::live_index::LiveIndex::load(project.path()).unwrap();
        let expected_gen = shared2.current_project_generation();

        // Edit the tracked file so the reconcile sweep sees it as stale.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&abs, b"fn before() {}\nfn after() {}").unwrap();

        let repairs2 =
            reconcile_stale_files_with_stop(project.path(), &shared2, || false, expected_gen);
        assert_eq!(
            repairs2, 1,
            "a genuinely stale, re-indexed file must count as one repair"
        );
    }

    /// `effective_fence_generation` must ADOPT a generation advanced by a
    /// same-root reload (cold-start heal) but KEEP the stale spawn generation
    /// after a cross-project retarget (so the store fence still rejects the now
    /// foreign mutation). This is the exact discriminator the cold-start fix
    /// relies on; the store's own under-lock generation check remains the final
    /// arbiter for any residual race.
    #[test]
    fn effective_fence_generation_adopts_same_root_keeps_after_retarget() {
        let project_a = TempDir::new().unwrap();
        let project_b = TempDir::new().unwrap();
        std::fs::create_dir_all(project_a.path().join("src")).unwrap();
        std::fs::create_dir_all(project_b.path().join("src")).unwrap();
        std::fs::write(project_a.path().join("src/a.rs"), b"fn a() {}").unwrap();
        std::fs::write(project_b.path().join("src/b.rs"), b"fn b() {}").unwrap();

        let shared = crate::live_index::LiveIndex::load(project_a.path()).unwrap();
        let spawn_gen = shared.current_project_generation();

        // No reload yet: the effective fence is the spawn snapshot unchanged.
        assert_eq!(
            effective_fence_generation(&shared, project_a.path(), spawn_gen),
            spawn_gen,
            "no generation advance -> keep spawn snapshot"
        );

        // Same-root reload: adopt the advanced generation (cold-start heal).
        shared.reload(project_a.path()).unwrap();
        let after_same_root = shared.current_project_generation();
        assert_ne!(after_same_root, spawn_gen);
        assert_eq!(
            effective_fence_generation(&shared, project_a.path(), spawn_gen),
            after_same_root,
            "same-root reload -> adopt the current generation so mutations commit"
        );

        // Cross-project retarget: KEEP the stale spawn generation so the store
        // fence rejects a mutation now computed against a foreign index.
        shared.reload(project_b.path()).unwrap();
        assert_eq!(
            effective_fence_generation(&shared, project_a.path(), spawn_gen),
            spawn_gen,
            "cross-project retarget -> keep stale spawn gen so the fence rejects"
        );
    }

    // --- Admission tiering on single-file (re)index paths (SF: admission bypass) ---

    /// The single-file reindex choke point must NOT re-admit a Tier-2 lockfile.
    ///
    /// Reproduces the bypass: after a bulk load demotes `package-lock.json` to
    /// Tier 2, a watcher modify event (or freshen-on-read) used to call
    /// `maybe_reindex` -> `read_and_index`, which re-parsed the lockfile and
    /// inserted it as Tier 1 with full symbols. The admission gate now returns
    /// `Skipped`: the file stays OUT of `files`, its skip record stays intact
    /// (no duplicate), and tier counts are unchanged.
    #[test]
    fn test_maybe_reindex_admission_skips_lockfile() {
        let tmp = TempDir::new().unwrap();
        // A real source file (Tier 1) plus a dependency lockfile (Tier 2).
        std::fs::write(tmp.path().join("main.rs"), b"fn main() {}").unwrap();
        let lock_rel = "package-lock.json";
        let lock_abs = tmp.path().join(lock_rel);
        std::fs::write(&lock_abs, br#"{"name":"x","lockfileVersion":3}"#).unwrap();

        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        // Baseline: lockfile demoted to Tier 2 by the bulk admission gate.
        let (t1_before, t2_before, t3_before) = {
            let idx = shared.read();
            assert!(
                idx.get_file(lock_rel).is_none(),
                "lockfile must not be a Tier-1 file after bulk load"
            );
            assert_eq!(
                idx.compatibility_skipped_files()
                    .iter()
                    .filter(|sf| sf.path == lock_rel)
                    .count(),
                1,
                "lockfile must have exactly one skip record after bulk load"
            );
            idx.tier_counts()
        };
        assert_eq!((t1_before, t2_before, t3_before), (1, 1, 0));

        // Simulate the single-file freshen/watcher path: re-touch and reindex.
        // `LanguageId::Json` is what the watcher resolves for `.json`, so this is
        // exactly the call the real event/freshen path makes.
        std::fs::write(&lock_abs, br#"{"name":"x","lockfileVersion":3,"extra":1}"#).unwrap();
        let result = maybe_reindex(lock_rel, &lock_abs, &shared, LanguageId::Json, expected_gen);
        assert_eq!(
            result,
            ReindexResult::Skipped,
            "lockfile must be admission-skipped, not re-parsed into the index"
        );

        let idx = shared.read();
        assert!(
            idx.get_file(lock_rel).is_none(),
            "lockfile must STILL be absent from Tier-1 files after the reindex attempt"
        );
        assert_eq!(
            idx.compatibility_skipped_files()
                .iter()
                .filter(|sf| sf.path == lock_rel)
                .count(),
            1,
            "skip record must remain de-duplicated (exactly one) after re-skip"
        );
        let sf = idx
            .compatibility_skipped_files()
            .into_iter()
            .find(|sf| sf.path == lock_rel)
            .expect("skip record must survive");
        assert_eq!(sf.decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(
            sf.decision.reason,
            Some(crate::domain::index::SkipReason::DependencyLockfile),
            "lockfile skip reason must be preserved"
        );
        assert_eq!(
            idx.tier_counts(),
            (t1_before, t2_before, t3_before),
            "tier counts must be unchanged by the admission-skipped reindex"
        );
    }

    /// A file that was Tier 1 but grew past the 1MB threshold must be DEMOTED
    /// (removed from `files`, recorded as Tier-2 SizeThreshold) by the freshen
    /// path — not re-parsed and re-inserted.
    #[test]
    fn test_freshen_admission_demotes_grown_file() {
        use crate::domain::index::{AdmissionTier, SkipReason};

        let tmp = TempDir::new().unwrap();
        let rel = "big.rs";
        let abs = tmp.path().join(rel);
        // Small valid Rust source -> Tier 1.
        std::fs::write(&abs, b"fn small() {}\n").unwrap();

        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        {
            let idx = shared.read();
            assert!(
                idx.get_file(rel).is_some(),
                "small source file must be Tier 1 after load"
            );
            assert_eq!(idx.tier_counts(), (1, 0, 0));
        }

        // Grow the file past the 4MB CODE metadata-only threshold (still valid
        // Rust; code languages get METADATA_ONLY_CODE_BYTES, dogfood #1/#7),
        // then bump mtime so the freshen path detects staleness.
        let mut grown = b"fn big() {}\n".to_vec();
        grown.resize(4_400_000, b' ');
        std::fs::write(&abs, &grown).unwrap();
        // Ensure the on-disk mtime differs from the indexed one so the freshen
        // mtime comparison fires (writes within the same second can otherwise
        // share an mtime). Backdate via std's `FileTimes` — no extra dep.
        {
            let f = std::fs::File::options().write(true).open(&abs).unwrap();
            let old = std::time::SystemTime::now() - std::time::Duration::from_secs(120);
            f.set_times(std::fs::FileTimes::new().set_modified(old))
                .unwrap();
        }

        let outcome = freshen_file_if_stale(rel, &abs, &shared, expected_gen);
        assert!(
            matches!(outcome, FreshenResult::StaleReindexed),
            "freshen should report the stale file was reconciled"
        );

        let idx = shared.read();
        assert!(
            idx.get_file(rel).is_none(),
            "grown file must be REMOVED from Tier-1 files (Tier 1 -> Tier 2 transition)"
        );
        let sf = idx
            .compatibility_skipped_files()
            .into_iter()
            .find(|sf| sf.path == rel)
            .expect("grown file must have a Tier-2 skip record");
        assert_eq!(sf.decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(sf.decision.reason, Some(SkipReason::SizeThreshold));
        assert_eq!(
            idx.tier_counts(),
            (0, 1, 0),
            "tier counts must reflect the demotion: 0 Tier-1, 1 Tier-2"
        );
    }

    #[test]
    fn test_new_generated_output_directory_stays_metadata_only() {
        let _env_lock = GENERATED_OUTPUT_ENV_LOCK.lock().unwrap();
        let _env = GeneratedOutputEnvGuard::set(None);
        let tmp = TempDir::new().unwrap();
        let _repository = init_test_git_repository(tmp.path());
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        let relative_path = "graphify-out/cache/new.rs";
        let absolute_path =
            create_test_source(tmp.path(), relative_path, b"fn generated_after_load() {}\n");
        let result = maybe_reindex(
            relative_path,
            &absolute_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );

        assert_eq!(
            result,
            ReindexResult::Skipped,
            "a generated directory created after load must follow bulk admission"
        );
        assert_generated_output_skip(&shared, relative_path);
        assert_eq!(shared.read().tier_counts(), (1, 1, 0));
    }

    #[test]
    fn test_bulk_demoted_generated_output_stays_metadata_only_on_watcher_event() {
        let _env_lock = GENERATED_OUTPUT_ENV_LOCK.lock().unwrap();
        let _env = GeneratedOutputEnvGuard::set(None);
        let tmp = TempDir::new().unwrap();
        let _repository = init_test_git_repository(tmp.path());
        let relative_path = "graphify-out/cache/existing.rs";
        let absolute_path = create_test_source(
            tmp.path(),
            relative_path,
            b"fn generated_before_load() {}\n",
        );
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        assert_generated_output_skip(&shared, relative_path);
        std::fs::write(&absolute_path, b"fn generated_before_load_changed() {}\n").unwrap();
        let result = maybe_reindex(
            relative_path,
            &absolute_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );

        assert_eq!(
            result,
            ReindexResult::Skipped,
            "watcher must not promote a bulk-demoted generated file"
        );
        assert_generated_output_skip(&shared, relative_path);
        assert_eq!(
            shared
                .read()
                .compatibility_skipped_files()
                .iter()
                .filter(|skipped| skipped.path == relative_path)
                .count(),
            1,
            "repeated demotion must retain exactly one skip record"
        );
    }

    #[test]
    fn test_tracked_and_prefix_rescue_re_admit_generated_output() {
        let _env_lock = GENERATED_OUTPUT_ENV_LOCK.lock().unwrap();
        let _env = GeneratedOutputEnvGuard::set(None);
        let tmp = TempDir::new().unwrap();
        let repository = init_test_git_repository(tmp.path());
        let tracked_relative_path = "graphify-out/cache/tracked.rs";
        let sibling_relative_path = "graphify-out/cache/sibling.rs";
        let tracked_absolute_path = create_test_source(
            tmp.path(),
            tracked_relative_path,
            b"fn tracked_generated() {}\n",
        );
        let sibling_absolute_path = create_test_source(
            tmp.path(),
            sibling_relative_path,
            b"fn untracked_sibling() {}\n",
        );
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        assert_generated_output_skip(&shared, tracked_relative_path);
        assert_generated_output_skip(&shared, sibling_relative_path);
        stage_test_path(&repository, tracked_relative_path);

        let tracked_result = maybe_reindex(
            tracked_relative_path,
            &tracked_absolute_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );
        let sibling_result = maybe_reindex(
            sibling_relative_path,
            &sibling_absolute_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );

        assert_eq!(tracked_result, ReindexResult::Reindexed);
        assert_eq!(
            sibling_result,
            ReindexResult::Reindexed,
            "one tracked file must rescue the entire generated-output prefix"
        );
        let index = shared.read();
        assert!(index.get_file(tracked_relative_path).is_some());
        assert!(index.get_file(sibling_relative_path).is_some());
        assert!(
            index
                .compatibility_skipped_files()
                .iter()
                .all(|skipped| skipped.path != tracked_relative_path
                    && skipped.path != sibling_relative_path),
            "Tier-2 skip records must be cleared after tracked-prefix rescue"
        );
        assert_eq!(index.tier_counts(), (3, 0, 0));
    }

    #[test]
    fn test_generated_output_opt_in_re_admits_tier_one() {
        let _env_lock = GENERATED_OUTPUT_ENV_LOCK.lock().unwrap();
        let _env = GeneratedOutputEnvGuard::set(None);
        let tmp = TempDir::new().unwrap();
        let _repository = init_test_git_repository(tmp.path());
        let relative_path = "graphify-out/cache/opted_in.rs";
        let absolute_path = create_test_source(tmp.path(), relative_path, b"fn opted_in() {}\n");
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();

        assert_generated_output_skip(&shared, relative_path);
        let _enabled = GeneratedOutputEnvGuard::set(Some("1"));
        let result = maybe_reindex(
            relative_path,
            &absolute_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );

        assert_eq!(result, ReindexResult::Reindexed);
        let index = shared.read();
        assert!(index.get_file(relative_path).is_some());
        assert!(
            index
                .compatibility_skipped_files()
                .iter()
                .all(|skipped| skipped.path != relative_path)
        );
        assert_eq!(index.tier_counts(), (2, 0, 0));
    }

    #[test]
    fn test_generated_output_watcher_non_git_tree_fails_open() {
        let _env_lock = GENERATED_OUTPUT_ENV_LOCK.lock().unwrap();
        let _env = GeneratedOutputEnvGuard::set(None);
        let tmp = TempDir::new().unwrap();
        create_test_source(tmp.path(), "src/main.rs", b"fn main() {}\n");
        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        let relative_path = "graphify-out/cache/non_git.rs";
        let absolute_path =
            create_test_source(tmp.path(), relative_path, b"fn non_git_generated() {}\n");

        let result = maybe_reindex(
            relative_path,
            &absolute_path,
            &shared,
            LanguageId::Rust,
            expected_gen,
        );

        assert_eq!(
            result,
            ReindexResult::Reindexed,
            "without readable Git evidence the watcher must fail open"
        );
        let index = shared.read();
        assert!(index.get_file(relative_path).is_some());
        assert_eq!(index.tier_counts(), (2, 0, 0));
    }

    /// A previously-skipped file (Tier 2 oversized) that shrinks back under the
    /// threshold must be re-admitted as Tier 1 AND have its stale skip record
    /// cleared, so it is never double-counted as both indexed and skipped.
    #[test]
    fn test_maybe_reindex_clears_stale_skip_on_shrink() {
        let tmp = TempDir::new().unwrap();
        let rel = "shrink.rs";
        let abs = tmp.path().join(rel);
        // Start oversized -> Tier 2 (SizeThreshold). Code files demote above
        // METADATA_ONLY_CODE_BYTES (4MB), not the 1MB data threshold.
        let mut big = b"fn shrink() {}\n".to_vec();
        big.resize(4_400_000, b' ');
        std::fs::write(&abs, &big).unwrap();

        let shared = crate::live_index::LiveIndex::load(tmp.path()).unwrap();
        let expected_gen = shared.current_project_generation();
        {
            let idx = shared.read();
            assert!(
                idx.get_file(rel).is_none(),
                "oversized file must start Tier 2"
            );
            assert_eq!(idx.tier_counts(), (0, 1, 0));
        }

        // Shrink it back under the threshold.
        std::fs::write(&abs, b"fn shrink() {}\n").unwrap();
        let result = maybe_reindex(rel, &abs, &shared, LanguageId::Rust, expected_gen);
        assert_eq!(
            result,
            ReindexResult::Reindexed,
            "shrunk file must be re-admitted as Tier 1"
        );

        let idx = shared.read();
        assert!(
            idx.get_file(rel).is_some(),
            "shrunk file must now be a Tier-1 indexed file"
        );
        assert_eq!(
            idx.compatibility_skipped_files()
                .iter()
                .filter(|sf| sf.path == rel)
                .count(),
            0,
            "stale Tier-2 skip record must be cleared on Tier-1 re-admission"
        );
        assert_eq!(
            idx.tier_counts(),
            (1, 0, 0),
            "no double-counting: 1 Tier-1, 0 Tier-2"
        );
    }
}
