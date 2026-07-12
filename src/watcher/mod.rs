use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecommendedWatcher as NotifyRecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, NoCache, new_debouncer_opt,
};
use tracing::{debug, error, trace, warn};

use crate::discovery::classify_admission;
use crate::domain::index::{AdmissionTier, SkippedFile};
use crate::domain::{FileClassification, LanguageId};
use crate::live_index::store::{IndexedFile, SharedIndex};
use crate::{hash, parsing};

// Watcher state snapshot types live in the engine-safe `watcher_state` module so
// the engine's health stats can use them in `embed` builds; the notify-based
// runtime below is server-only.
pub use crate::watcher_state::{WatcherInfo, WatcherState};

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

/// Result of a single re-index attempt for one file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReindexResult {
    /// Content hash matched existing entry — tree-sitter parse was skipped.
    HashSkip,
    /// File was re-parsed and the index was updated.
    Reindexed,
    /// File classified as Tier 2/3 by the admission gate — NOT parsed/inserted.
    /// Any prior Tier-1 entry was removed and a skip record recorded. The index
    /// remains free of this path's symbols.
    Skipped,
    /// ENOENT observed by `read_and_index`; caller decides whether to retry or treat as confirmed-absent.
    NotFound,
    /// File was not found (ENOENT) — it has been removed from the index.
    Removed,
    /// File could not be read for a reason other than ENOENT.
    ReadError(String),
}

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

/// Return the `LanguageId` for a file path based on its extension.
///
/// Returns `None` for unsupported or missing extensions.
pub(crate) fn supported_language(path: &Path) -> Option<LanguageId> {
    let ext = path.extension()?.to_str()?;
    LanguageId::from_extension(ext)
}

/// Return `true` for Create, Modify, or Remove events; `false` for Access and others.
pub(crate) fn is_relevant_event(event: &DebouncedEvent) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Content-hash-gated single-file re-index.
///
/// Reads the file, compares its hash against the existing index entry, and
/// skips the expensive tree-sitter parse when the hash matches.
///
/// # Lock discipline
/// The write lock is **never** held during the tree-sitter parse. The sequence is:
/// 1. Read file bytes (no lock)
/// 2. Acquire read lock → compare hash → drop read lock
/// 3. Parse (no lock)
/// 4. Acquire write lock → update_file → drop write lock
pub(crate) fn maybe_reindex(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: LanguageId,
    expected_gen: u64,
) -> ReindexResult {
    match read_and_index(
        relative_path,
        abs_path,
        shared,
        language.clone(),
        expected_gen,
    ) {
        ReindexResult::NotFound => {}
        other => return other,
    }

    let delays_ms = [50u64, 200, 500];
    for delay_ms in delays_ms {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        match read_and_index(
            relative_path,
            abs_path,
            shared,
            language.clone(),
            expected_gen,
        ) {
            ReindexResult::NotFound => continue,
            other => return other,
        }
    }

    if shared.remove_file_at_generation(relative_path, expected_gen) {
        warn!("watcher: file not found after retries, removed from index: {relative_path}");
    } else {
        trace!(
            "watcher: file not found after retries, stale generation rejected remove: {relative_path}"
        );
    }
    ReindexResult::Removed
}

/// Recover the project root by walking up from the absolute event path once per
/// component of the relative path. Both come from the same watcher event (or
/// freshen-on-read call), so the suffix relationship holds by construction;
/// `None` only if the relative path is deeper than the absolute one.
fn project_root_from_paths(abs_path: &Path, relative_path: &str) -> Option<PathBuf> {
    let depth = Path::new(relative_path).components().count();
    abs_path.ancestors().nth(depth).map(|p| p.to_path_buf())
}

fn read_and_index(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: LanguageId,
    expected_gen: u64,
) -> ReindexResult {
    // 1. Read mtime BEFORE content to avoid TOCTOU: if the file is written
    //    between stat and read, we get an mtime that is older-or-equal to the
    //    content we actually parsed, so a future watcher event will correctly
    //    detect staleness. The reverse order (read then stat) can record a
    //    newer mtime paired with older content, permanently hiding the change.
    let metadata = match std::fs::metadata(abs_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ReindexResult::NotFound,
        Err(e) => {
            warn!("watcher: failed to stat {relative_path}: {e}");
            return ReindexResult::ReadError(e.to_string());
        }
    };
    let file_size = metadata.len();
    let mtime_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bytes = match std::fs::read(abs_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ReindexResult::NotFound,
        Err(e) => {
            warn!("watcher: failed to read {relative_path}: {e}");
            return ReindexResult::ReadError(e.to_string());
        }
    };

    // 2. Compute hash and check against existing entry (read lock, dropped before parse)
    let new_hash = hash::digest_hex(&bytes);
    {
        let index = shared.read();
        if let Some(existing) = index.get_file(relative_path)
            && existing.content_hash == new_hash
        {
            // Content unchanged but mtime may have drifted (e.g., git rebase, touch).
            // Update the stored mtime so the reconciliation loop doesn't re-check
            // this file on every sweep. Without this, a hash-skip leaves the old
            // mtime in the index, causing an infinite stale → hash-skip → stale loop.
            let needs_mtime_touch = existing.mtime_secs != mtime_secs && mtime_secs != 0;
            drop(index); // release read lock before potential write
            if needs_mtime_touch {
                shared.touch_mtime_at_generation(relative_path, mtime_secs, expected_gen);
            }
            debug!("watcher: hash-skip {relative_path}");
            return ReindexResult::HashSkip;
        }
        // read lock dropped here
    }

    // SF-025: scan-policy gate — keep the single-file (re)index path symmetric
    // with the bulk discovery walk. `discover_all_files` uses `ignore::WalkBuilder`
    // with its default `.hidden(true)`, so a hidden dotfile/dotdir such as
    // `.github/workflows/ci.yml` or `.travis.yml` is NEVER discovered, admitted,
    // counted, or recorded on a fresh load — it simply does not exist as far as
    // the index is concerned. The watcher (FS events + freshen-on-read) had no
    // such rule, so a single `get_file_context` on a tracked hidden file would
    // parse and INSERT it, making index membership query-history-dependent: the
    // file was invisible to `search_files` until someone happened to read it, and
    // identical health calls disagreed across processes. Apply the SAME exclusion
    // here, BEFORE the admission gate. Mirror "the walk never saw it" exactly: do
    // not parse, do not insert, and do not mint a skip record (the walk mints
    // none either) — but DO drop any stale Tier-1/skip record a prior build may
    // have left for this path, so the index converges to the bulk-load shape.
    if crate::discovery::path_has_hidden_component(relative_path) {
        drop(bytes);
        let removed = shared.remove_file_at_generation(relative_path, expected_gen);
        let cleared = shared.clear_skipped_at_generation(relative_path, expected_gen);
        if removed || cleared {
            debug!("watcher: scan-policy hidden-path eviction {relative_path}");
        } else {
            trace!("watcher: scan-policy hidden-path skip (no prior record) {relative_path}");
        }
        return ReindexResult::Skipped;
    }

    // 3. Admission gate — the single choke point for ALL single-file (re)index
    //    paths (watcher FS events and freshen-on-read both reach here). A file
    //    that classifies as Tier 2/3 must NOT be parsed or inserted, even if it
    //    was previously Tier 1 (e.g. a source file that grew past 1MB). We
    //    already have the on-disk size and content in hand, so run the full
    //    Phase-1 + content-sniff classification in one call.
    let decision = classify_admission(abs_path, file_size, Some(&bytes));
    match decision.tier {
        AdmissionTier::HardSkip | AdmissionTier::MetadataOnly => {
            // Do NOT parse or insert. Demote atomically: drop any prior Tier-1
            // entry and upsert the skip record (dedup by path). Drop the bytes
            // now — they are never parsed.
            let extension = abs_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string());
            drop(bytes);
            let sf = SkippedFile {
                path: relative_path.to_string(),
                size: file_size,
                extension,
                decision,
            };
            if shared.demote_to_skipped_at_generation(relative_path, sf, expected_gen) {
                debug!("watcher: admission-skip {relative_path} (Tier 2/3)");
            } else {
                trace!("watcher: admission-skip stale generation rejected: {relative_path}");
            }
            return ReindexResult::Skipped;
        }
        AdmissionTier::Normal => {}
    }

    // 3b. F5 parity: the bulk discovery walk demotes files under UNTRACKED
    //     generated-output directories to Tier-2 metadata-only; without the same
    //     policy here, a generated directory created (or repopulated) after the
    //     initial load would silently re-enter Tier 1 through watcher events.
    //     `is_untracked_generated_output_path` checks the path shape FIRST
    //     (pure string work) and consults git evidence only when a
    //     generated-looking component is present, so ordinary events never scan
    //     the tracked set. Fails open on non-git trees; honors the
    //     `SYMFORGE_INDEX_GENERATED_OUTPUT` opt-in; a tracked file (or any
    //     tracked sibling under the same prefix) rescues the path back to
    //     Tier 1 exactly like the bulk walk.
    if let Some(root) = project_root_from_paths(abs_path, relative_path)
        && crate::discovery::is_untracked_generated_output_path(&root, relative_path)
    {
        let extension = abs_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());
        drop(bytes);
        let sf = SkippedFile {
            path: relative_path.to_string(),
            size: file_size,
            extension,
            decision: crate::domain::index::AdmissionDecision::skip(
                AdmissionTier::MetadataOnly,
                crate::domain::index::SkipReason::GeneratedOutput,
            ),
        };
        if shared.demote_to_skipped_at_generation(relative_path, sf, expected_gen) {
            debug!("watcher: generated-output demotion {relative_path}");
        } else {
            trace!("watcher: generated-output demotion stale generation rejected: {relative_path}");
        }
        return ReindexResult::Skipped;
    }

    // 4. Parse outside the lock (Tier-1 only).
    let result = parsing::process_file_with_classification(
        relative_path,
        &bytes,
        language,
        FileClassification::for_code_path(relative_path),
    );
    let indexed = IndexedFile::from_parse_result(result, bytes).with_mtime(mtime_secs);

    // 5. Acquire write lock and update.
    shared.update_file_at_generation(relative_path, indexed, expected_gen);

    // 6. Clear any stale Tier-2/3 skip record for this path: a file that was
    //    previously demoted (e.g. oversized) can shrink back under the threshold
    //    and become Tier 1 again. Without this, it would be double-counted as
    //    both indexed and skipped. No-op (single cheap scan) in the common case.
    shared.clear_skipped_at_generation(relative_path, expected_gen);

    debug!("watcher: re-indexed {relative_path}");
    ReindexResult::Reindexed
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
    let language = match supported_language(abs_path) {
        Some(l) => l,
        None => return FreshenResult::Fresh,
    };

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
    // Re-sync the fence to the CURRENT generation when the live index still
    // serves our repo_root, so a same-root reload that advanced the generation
    // after watcher spawn (cold start) no longer permanently rejects. A
    // cross-project retarget keeps the stale spawn generation and is rejected.
    let fence_gen = effective_fence_generation(shared, repo_root, expected_gen);

    let paths: Vec<String> = {
        let index = shared.read();
        index.all_files().map(|(p, _)| p.clone()).collect()
    };

    let mut stale_count = 0usize;
    for relative_path in &paths {
        if should_stop() {
            break;
        }
        let abs_path = repo_root.join(relative_path);
        // Count ONLY genuine repairs. A `GenerationMismatch` outcome is a no-op
        // that repaired zero bytes (the store rejected the stale-generation
        // mutation, incrementing `rejected_stale_mutations` instead), so folding
        // it into the repair count would falsely inflate health's "reconcile
        // repairs" figure during any restart/reset window.
        match freshen_file_if_stale(relative_path, &abs_path, shared, fence_gen) {
            FreshenResult::StaleReindexed | FreshenResult::StaleRemoved => stale_count += 1,
            FreshenResult::Fresh | FreshenResult::GenerationMismatch => {}
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
    stale_count
}

pub(crate) fn reconcile_stale_files(repo_root: &Path, shared: &SharedIndex) -> usize {
    let expected_gen = shared.current_project_generation();
    reconcile_stale_files_with_stop(repo_root, shared, || false, expected_gen)
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
    /// into paired Modify(Name) events, which `process_events` never consumes —
    /// it treats Create==Modify as reindex and Remove as drop, so a rename that
    /// arrives as Remove+Create behaves identically. Crucially, `FileIdMap`
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
    for event in events {
        if should_stop() {
            break;
        }
        if !is_relevant_event(&event) {
            continue;
        }

        for abs_path in &event.paths {
            if should_stop() {
                break;
            }
            // Normalize path — skip if outside repo_root or can't be normalized
            let relative_path = match normalize_event_path(abs_path, repo_root) {
                Some(r) => r,
                None => continue,
            };

            // Filter to supported languages only
            let language = match supported_language(abs_path) {
                Some(l) => l,
                None => continue,
            };

            // Mirror discovery's gitignore-aware walk: never index paths the
            // initial scan would have pruned. Without this the watcher picks
            // up files created under gitignored directories during a session —
            // most importantly SymForge's own `.symforge/` state dir (e.g.
            // `tee/*.rs` edit snapshots) — polluting search and reference
            // results and growing the index unbounded.
            if shared.read().is_path_gitignored(&relative_path) {
                continue;
            }

            match event.kind {
                EventKind::Remove(_) => {
                    shared.remove_file_at_generation(&relative_path, expected_gen);

                    let mut info = watcher_info.lock();
                    info.events_processed += 1;
                    info.last_event_at = Some(SystemTime::now());
                }
                EventKind::Create(_) | EventKind::Modify(_) => {
                    // Update burst tracker for this path
                    let now = Instant::now();
                    let tracker = burst_trackers.entry(abs_path.clone()).or_default();
                    tracker.update(now);
                    let debounce_ms = tracker.effective_debounce_ms();

                    maybe_reindex(&relative_path, abs_path, shared, language, expected_gen);

                    let mut info = watcher_info.lock();
                    info.events_processed += 1;
                    info.last_event_at = Some(SystemTime::now());
                    info.debounce_window_ms = debounce_ms;
                }
                _ => {}
            }
        }
    }

    // Evict burst trackers that have been idle longer than 2 × QUIET_SECS to
    // prevent the map from growing unbounded over the lifetime of the watcher.
    // NOTE: eviction only runs after file-change events, not during overflow
    // reconciliation, so trackers for paths not recently seen are cleaned up
    // lazily on the next incoming event.
    let evict_threshold = Duration::from_secs(BurstTracker::QUIET_SECS * 2);
    burst_trackers.retain(|_, tracker| tracker.last_event_at.elapsed() < evict_threshold);
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
                            let stale = reconcile_stale_files_with_stop(
                                &root_clone,
                                &shared_clone,
                                || stop_for_reconcile.load(Ordering::Acquire),
                                expected_gen_for_reconcile,
                            );
                            if stop_for_reconcile.load(Ordering::Acquire) {
                                return;
                            }
                            let mut info = watcher_info_clone.lock();
                            info.stale_files_found += stale as u64;
                            info.last_reconcile_at = Some(SystemTime::now());
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
                            let mut overflow_detected = false;
                            for err in &errors {
                                warn!("watcher: notify error: {err}");
                                // Detect watcher buffer overflow / rescan events.
                                if matches!(
                                    err.kind,
                                    notify::ErrorKind::Io(_)
                                        | notify::ErrorKind::Generic(_)
                                        | notify::ErrorKind::MaxFilesWatch
                                ) {
                                    overflow_detected = true;
                                }
                            }
                            if overflow_detected {
                                warn!(
                                    "watcher: buffer overflow detected — running full reconciliation"
                                );
                                let shared_clone = shared.clone();
                                let root_clone = repo_root.clone();
                                let watcher_info_clone = watcher_info.clone();
                                let stop_for_reconcile = Arc::clone(&stop_token);
                                let expected_gen_for_reconcile = expected_gen;
                                tokio::task::spawn_blocking(move || {
                                    let stale = reconcile_stale_files_with_stop(
                                        &root_clone,
                                        &shared_clone,
                                        || stop_for_reconcile.load(Ordering::Acquire),
                                        expected_gen_for_reconcile,
                                    );
                                    if stop_for_reconcile.load(Ordering::Acquire) {
                                        return;
                                    }
                                    let mut info = watcher_info_clone.lock();
                                    info.overflow_count += 1;
                                    info.last_overflow_at = Some(SystemTime::now());
                                    info.stale_files_found += stale as u64;
                                    info.last_reconcile_at = Some(SystemTime::now());
                                });
                            }
                            session_errors += u32::try_from(errors.len()).unwrap_or(u32::MAX);
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
    use crate::domain::index::SkipReason;
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
            .skipped_files()
            .iter()
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
        assert_eq!(supported_language(path), None);
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
                skipped_files: Vec::new(),
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
    fn reindex_refuses_hidden_path_to_match_bulk_walk() {
        // SF-025: the bulk discovery walk skips hidden dotfiles/dotdirs, so a
        // tracked hidden file like `.github/workflows/ci.yml` is never indexed on
        // a fresh load. The single-file (re)index choke point must apply the SAME
        // scan-policy exclusion — otherwise a freshen-on-read (or watcher event)
        // would parse and INSERT it, making index membership query-history-
        // dependent. Here `.github/workflows/ci.yml` is a SUPPORTED-language
        // (Yaml) file with content; the choke point must still refuse it and the
        // index must stay empty, so health counts are invariant.
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
                skipped_files: Vec::new(),
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
            "hidden-path file must be scan-policy skipped, not parsed/inserted"
        );

        let idx = shared.read();
        assert!(
            idx.get_file(rel_path).is_none(),
            "hidden-path file must NOT be inserted into the parsed index"
        );
        // It must also not leave a skip record — the bulk walk records none
        // either (it simply never discovers hidden paths), so the index converges
        // to the exact same shape regardless of read history.
        assert_eq!(
            idx.tier_counts(),
            (0, 0, 0),
            "hidden-path skip must not mint a tier record; index stays empty"
        );
    }

    #[test]
    fn reindex_still_admits_visible_supported_file() {
        // Control for the hidden-path gate: a VISIBLE supported file at the same
        // choke point is still parsed and inserted normally — the SF-025 fix only
        // excludes hidden paths, never visible source.
        use crate::domain::LanguageId;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let rel_path = "config/ci.yml";
        let abs_path = tmp.path().join("config").join("ci.yml");
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
                skipped_files: Vec::new(),
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
            "visible supported file must be parsed and inserted"
        );

        let idx = shared.read();
        assert!(
            idx.get_file(rel_path).is_some(),
            "visible supported file must be present in the parsed index"
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
            skipped_files: Vec::new(),
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
                skipped_files: Vec::new(),
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
                skipped_files: Vec::new(),
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
                idx.skipped_files()
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
            idx.skipped_files()
                .iter()
                .filter(|sf| sf.path == lock_rel)
                .count(),
            1,
            "skip record must remain de-duplicated (exactly one) after re-skip"
        );
        let sf = idx
            .skipped_files()
            .iter()
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
            .skipped_files()
            .iter()
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
                .skipped_files()
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
                .skipped_files()
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
                .skipped_files()
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
            idx.skipped_files()
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
