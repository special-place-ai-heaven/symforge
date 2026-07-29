use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::{ArcSwap, ArcSwapOption};
use parking_lot::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use rayon::prelude::*;
use tracing::{error, info, warn};

use super::knowledge_authority::{
    AuthorityLimits, AuthorityTemporalIndex, KnowledgeAuthorityView, build_knowledge_authority,
};
use super::knowledge_bridge::{BridgeLimits, KnowledgeBridge, build_knowledge_bridge};
use super::query::RepoOutlineView;
use crate::domain::ParseDiagnostic;
use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason, SkippedFile};
use crate::domain::{
    CatalogEntry, CoverageStatus, FileClassification, FileDisposition, FileOutcome,
    FileProcessingResult, FreshnessReason, FreshnessStatus, HardSkipReason, HistoryCoverage,
    HistoryLimit, LanguageId, ManifestResourceUsage, MetadataOnlyReason, ProjectStateDir,
    ReferenceRecord, RepositoryManifest, ScoutDecision, SourceId, SourceIdentity,
    SourceResponseEnvelope, SourceVersion, StatePlacement, SymbolRecord, WorkingTreeState,
    find_enclosing_symbol,
};
use crate::{discovery, parsing};

/// Normalize a filesystem root into a stable comparison key.
///
/// Used to record the root an in-memory index was built from
/// ([`LiveIndex::indexed_root`]) and to compare it against the current target
/// root in `SymForgeServer::ensure_local_index`, so a changed project root
/// always invalidates a stale index regardless of `\\?\` UNC prefixes,
/// trailing separators, or path-separator/case differences.
///
/// Both sides of any root comparison MUST flow through this single helper so
/// the steady-state path (same project, repeated calls) never reloads on a
/// cosmetic path difference. We delegate to `dunce::canonicalize`, which on
/// Windows strips the extended-length `\\?\` prefix and normalizes separators
/// while resolving symlinks; on non-Windows it falls back to
/// `std::fs::canonicalize`. When canonicalization fails (e.g. the path does not
/// exist on disk — common in unit tests), we fall back to the input path so the
/// comparison still works as a literal, normalized-once key.
pub(crate) fn normalize_root(root: &Path) -> PathBuf {
    dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

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

/// Env override for the peak concurrent in-memory read budget enforced during
/// the admission gate. Value is parsed as a byte count.
const MAX_INFLIGHT_BYTES_ENV: &str = "SYMFORGE_MAX_INFLIGHT_BYTES";

/// Default peak concurrent in-memory read budget: 512 MiB. High enough that
/// normal repositories never block on it (their files are tiny), but low
/// enough to bound peak resident memory when a tree contains many large
/// recognized-source files that would otherwise be read fully in parallel.
const DEFAULT_MAX_INFLIGHT_BYTES: u64 = 512 * 1024 * 1024;

/// Bounds the peak concurrent in-memory bytes consumed by full-file reads.
///
/// The cumulative byte cap in discovery limits total tree size, but says
/// nothing about how much is resident *at once*. The admission gate reads every
/// `Normal`-tier file fully into a `Vec<u8>` in parallel via Rayon. Today the
/// per-file read ceiling on that path is `METADATA_ONLY_BYTES` (1 MiB) — larger
/// files are classified `MetadataOnly` and skipped without a read — so peak is
/// already `num_threads * up-to-1-MiB`. Every admitted read reserves its declared
/// size before access and holds that permit through parse and staged hand-off.
/// Callers reject a file larger than the total budget before allocation.
struct InflightByteBudget {
    total: u64,
    state: std::sync::Mutex<u64>,
    available: std::sync::Condvar,
}

impl InflightByteBudget {
    fn new(total: u64) -> Self {
        // A zero budget would deadlock acquisition; clamp to at least 1 byte.
        let total = total.max(1);
        Self {
            total,
            state: std::sync::Mutex::new(total),
            available: std::sync::Condvar::new(),
        }
    }

    fn from_env() -> Self {
        let total = std::env::var(MAX_INFLIGHT_BYTES_ENV)
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(DEFAULT_MAX_INFLIGHT_BYTES);
        Self::new(total)
    }

    /// Acquire permits for `bytes`, blocking until enough budget is free.
    ///
    /// The requested amount is clamped to the total budget so that a single
    /// file larger than the entire budget never deadlocks — it waits until the
    /// budget is fully free, then runs alone. Returns an owned guard that
    /// releases the permits (and wakes waiters) on drop. The guard is owned (it
    /// holds an `Arc` to the budget) so it can travel alongside the file's bytes
    /// and be released only once those bytes are dropped.
    fn acquire(self: &Arc<Self>, bytes: u64) -> InflightPermit {
        let want = bytes.min(self.total).max(1);
        let mut available = self.state.lock().expect("inflight budget mutex poisoned");
        while *available < want {
            available = self
                .available
                .wait(available)
                .expect("inflight budget condvar poisoned");
        }
        *available -= want;
        InflightPermit {
            budget: Arc::clone(self),
            held: want,
        }
    }

    /// Currently free budget, in bytes. Test-only observation hook.
    #[cfg(test)]
    fn available_bytes(&self) -> u64 {
        *self.state.lock().expect("inflight budget mutex poisoned")
    }
}

/// Configured peak in-flight admission byte budget (M-001 health surface).
///
/// This is operational health state, not manifest identity (data-model.md
/// §ManifestResourceUsage: "In-flight/peak/derived-state usage is operational
/// health state"), so it is reconstructed from the same env source the
/// admission gate uses rather than stored in the published generation.
pub fn configured_inflight_byte_budget() -> u64 {
    InflightByteBudget::from_env().total
}

/// RAII permit that returns its held bytes to the [`InflightByteBudget`] and
/// wakes waiters on drop. Dropping it is the point at which the large file's
/// bytes are considered no longer in flight. Owned (holds an `Arc`) so it can be
/// carried across Rayon stages alongside the bytes it accounts for.
struct InflightPermit {
    budget: Arc<InflightByteBudget>,
    held: u64,
}

impl Drop for InflightPermit {
    fn drop(&mut self) {
        let mut available = self
            .budget
            .state
            .lock()
            .expect("inflight budget mutex poisoned");
        *available += self.held;
        // Wake all waiters: a large release may unblock several small waiters.
        self.budget.available.notify_all();
    }
}

/// Independent resident-byte accounting for content that has crossed from the
/// transient read/parse pipeline into the staged index. A successful hand-off
/// reserves staged capacity first and only then releases the transient permit,
/// so the bytes are continuously accounted without retaining permits for the
/// entire cold-load generation.
struct StagedContentAccounting {
    ceiling: u64,
    used: AtomicU64,
}

impl StagedContentAccounting {
    fn new(ceiling: u64) -> Self {
        Self {
            ceiling,
            used: AtomicU64::new(0),
        }
    }

    fn handoff(&self, bytes: u64, permit: Option<InflightPermit>) -> bool {
        let reserved = self
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.ceiling)
            })
            .is_ok();
        drop(permit);
        reserved
    }

    #[cfg(test)]
    fn used_bytes(&self) -> u64 {
        self.used.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StableReadLimits {
    per_file_bytes: u64,
    inflight_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StableReadPass {
    bytes: Option<Vec<u8>>,
    length: u64,
    hash: [u8; 32],
    handle_before: crate::domain::FileStamp,
    handle_after: crate::domain::FileStamp,
    path_after: crate::domain::FileStamp,
}

trait StableReadAccess {
    fn first_pass(&self, path: &Path, max_bytes: usize) -> std::io::Result<StableReadPass>;
    fn second_pass(&self, path: &Path, max_bytes: usize) -> std::io::Result<StableReadPass>;
}

struct FilesystemStableReadAccess;

fn file_stamp_from_metadata(metadata: &std::fs::Metadata) -> crate::domain::FileStamp {
    crate::domain::FileStamp {
        size: metadata.len(),
        created_hint: metadata.created().ok(),
        modified_hint: metadata.modified().ok(),
        platform_id: None,
    }
}

impl FilesystemStableReadAccess {
    fn read_pass(
        path: &Path,
        max_bytes: usize,
        retain_bytes: bool,
    ) -> std::io::Result<StableReadPass> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        let mut file = std::fs::File::open(path)?;
        let handle_before = file_stamp_from_metadata(&file.metadata()?);
        if usize::try_from(handle_before.size).map_or(true, |size| size > max_bytes) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stable read exceeded its pre-authorized bound",
            ));
        }

        let mut bytes = retain_bytes.then(Vec::new);
        if let Some(bytes) = bytes.as_mut() {
            bytes
                .try_reserve_exact(handle_before.size as usize)
                .map_err(|_| std::io::Error::other("stable read reservation failed"))?;
        }
        let mut hasher = Sha256::new();
        let mut length = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            length = length
                .checked_add(read as u64)
                .ok_or_else(|| std::io::Error::other("stable read length overflow"))?;
            if length > max_bytes as u64 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stable read exceeded its pre-authorized bound",
                ));
            }
            hasher.update(&buffer[..read]);
            if let Some(bytes) = bytes.as_mut() {
                bytes.extend_from_slice(&buffer[..read]);
            }
        }

        let handle_after = file_stamp_from_metadata(&file.metadata()?);
        let path_after = file_stamp_from_metadata(&std::fs::metadata(path)?);
        Ok(StableReadPass {
            bytes,
            length,
            hash: hasher.finalize().into(),
            handle_before,
            handle_after,
            path_after,
        })
    }
}

impl StableReadAccess for FilesystemStableReadAccess {
    fn first_pass(&self, path: &Path, max_bytes: usize) -> std::io::Result<StableReadPass> {
        Self::read_pass(path, max_bytes, true)
    }

    fn second_pass(&self, path: &Path, max_bytes: usize) -> std::io::Result<StableReadPass> {
        Self::read_pass(path, max_bytes, false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StableReadOutcome {
    Accepted {
        bytes: Vec<u8>,
        hash: [u8; 32],
    },
    HardSkip {
        reason: HardSkipReason,
    },
    Unreadable {
        stage: crate::domain::AccessStage,
        kind: crate::domain::AccessErrorKind,
    },
    UnstableDuringRead,
}

fn stable_read_with_access(
    path: &Path,
    scout_stamp: &crate::domain::FileStamp,
    limits: StableReadLimits,
    access: &impl StableReadAccess,
) -> StableReadOutcome {
    let Ok(max_bytes) = usize::try_from(scout_stamp.size) else {
        return StableReadOutcome::HardSkip {
            reason: HardSkipReason::PerFileCeiling,
        };
    };
    if scout_stamp.size > limits.per_file_bytes || scout_stamp.size > limits.inflight_bytes {
        return StableReadOutcome::HardSkip {
            reason: HardSkipReason::PerFileCeiling,
        };
    }

    let first = match access.first_pass(path, max_bytes) {
        Ok(first) => first,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            return StableReadOutcome::UnstableDuringRead;
        }
        Err(error) => {
            return StableReadOutcome::Unreadable {
                stage: crate::domain::AccessStage::FullRead,
                kind: discovery::access_error_kind(error.kind()),
            };
        }
    };
    if &first.handle_before != scout_stamp
        || &first.handle_after != scout_stamp
        || &first.path_after != scout_stamp
    {
        return StableReadOutcome::UnstableDuringRead;
    }
    let Some(bytes) = first.bytes else {
        return StableReadOutcome::UnstableDuringRead;
    };
    if first.length != scout_stamp.size
        || usize::try_from(first.length).ok() != Some(bytes.len())
        || first.hash != crate::hash::digest(&bytes)
    {
        return StableReadOutcome::UnstableDuringRead;
    }

    let second = match access.second_pass(path, max_bytes) {
        Ok(second) => second,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            return StableReadOutcome::UnstableDuringRead;
        }
        Err(error) => {
            return StableReadOutcome::Unreadable {
                stage: crate::domain::AccessStage::FullRead,
                kind: discovery::access_error_kind(error.kind()),
            };
        }
    };
    if &second.handle_before != scout_stamp
        || &second.handle_after != scout_stamp
        || &second.path_after != scout_stamp
        || second.length != first.length
        || second.hash != first.hash
    {
        return StableReadOutcome::UnstableDuringRead;
    }

    StableReadOutcome::Accepted {
        bytes,
        hash: first.hash,
    }
}

fn stable_read_with_retries(
    path: &Path,
    scout_stamp: &crate::domain::FileStamp,
    limits: StableReadLimits,
    access: &impl StableReadAccess,
) -> StableReadOutcome {
    const MAX_ATTEMPTS: usize = 3;

    for _ in 0..MAX_ATTEMPTS {
        let outcome = stable_read_with_access(path, scout_stamp, limits, access);
        if !matches!(outcome, StableReadOutcome::UnstableDuringRead) {
            return outcome;
        }
    }
    StableReadOutcome::UnstableDuringRead
}

pub(crate) fn stable_read_file(
    path: &Path,
    scout_stamp: &crate::domain::FileStamp,
) -> StableReadOutcome {
    stable_read_with_retries(
        path,
        scout_stamp,
        StableReadLimits {
            per_file_bytes: crate::domain::index::HARD_SKIP_BYTES,
            inflight_bytes: InflightByteBudget::from_env().total,
        },
        &FilesystemStableReadAccess,
    )
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
    /// SF-STRESS-009: counts of partial parses heuristically (path-based) bucketed
    /// as machine-generated, test-fixture, or template-DSL noise. Carried across
    /// the daemon-proxy boundary so proxied health reports the same buckets.
    pub expected_generated_partial_parse_count: usize,
    pub expected_test_fixture_partial_parse_count: usize,
    pub expected_template_dsl_partial_parse_count: usize,
    /// SF-004: count of partial parses excused as a framework template grammar
    /// limitation (Angular `@if`/`@for`/... in `.html`). Carried across the
    /// daemon-proxy boundary so proxied health reports the same third bucket.
    pub expected_framework_partial_parse_count: usize,
    /// SF-003: count of partial parses excused as a host-language grammar
    /// limitation (TypeScript `import('mod').Member[]` import-type arrays).
    /// Carried across the daemon-proxy boundary so proxied health reports the
    /// same bucket and the registry total stays in sync with the header.
    pub expected_language_partial_parse_count: usize,
    pub failed_count: usize,
    pub partial_parse_files: Vec<String>,
    pub unexpected_partial_parse_files: Vec<String>,
    pub expected_vendor_partial_parse_files: Vec<String>,
    /// SF-STRESS-009: bounded lists of the heuristic generated/test-fixture/
    /// template-DSL partial-parse buckets.
    pub expected_generated_partial_parse_files: Vec<String>,
    pub expected_test_fixture_partial_parse_files: Vec<String>,
    pub expected_template_dsl_partial_parse_files: Vec<String>,
    /// SF-004: bounded list of framework-template partial-parse files.
    pub expected_framework_partial_parse_files: Vec<String>,
    /// SF-003: bounded list of host-language-limitation partial-parse files.
    pub expected_language_partial_parse_files: Vec<String>,
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
    /// SF-009: count of Tier-1 indexed files that are NOT git-tracked AND NOT
    /// gitignored. Carried across the daemon-proxy boundary (no serde) so
    /// proxied health reports the same "indexed untracked files: N" surfacing.
    /// Fails open to `0` (see `HealthStats::untracked_indexed`).
    pub untracked_indexed: usize,
    /// Normalized filesystem root the published index was built from. `None`
    /// for an empty bootstrap index. Read by `SymForgeServer::ensure_local_index`
    /// to detect a project switch (root mismatch) and force a fresh reload, so
    /// no caller must remember to call `reset_to_empty` to avoid serving a stale
    /// project. Always populated via [`normalize_root`] for stable comparison.
    pub indexed_root: Option<PathBuf>,
}

/// One immutable externally observable repository generation.
///
/// Gate E grows this core bundle with source identity, canonical manifest, and
/// code-signal metadata. Live content, health, and outline already share this
/// single publication root so readers cannot observe mixed swaps.
pub struct CodeSignalsSnapshot {
    pub state: super::git_temporal::GitTemporalState,
    pub temporal: Arc<super::git_temporal::GitTemporalIndex>,
    pub computed_for_content_generation: u64,
    pub computed_for_source_version: SourceVersion,
    pub coverage: Arc<HistoryCoverage>,
}

pub struct PublishedGeneration {
    pub publication_generation: u64,
    pub content_generation: u64,
    pub project_generation: u64,
    pub source: Option<Arc<SourceIdentity>>,
    pub source_version: Option<Arc<SourceVersion>>,
    pub freshness: Arc<FreshnessStatus>,
    pub manifest: Option<Arc<RepositoryManifest>>,
    pub code_signals: Arc<CodeSignalsSnapshot>,
    pub bridge: Arc<KnowledgeBridge>,
    pub authority: Arc<KnowledgeAuthorityView>,
    pub live: Arc<LiveIndex>,
    pub health: Arc<PublishedIndexState>,
    pub outline: Arc<RepoOutlineView>,
}

impl PublishedGeneration {
    pub fn source_response_envelope(&self) -> Option<SourceResponseEnvelope> {
        let manifest = self.manifest.as_ref()?;
        Some(SourceResponseEnvelope {
            source: self.source.as_ref()?.as_ref().clone(),
            source_version: self.source_version.as_ref()?.as_ref().clone(),
            publication_generation: self.publication_generation,
            content_generation: self.content_generation,
            freshness: self.freshness.as_ref().clone(),
            manifest_digest: manifest.digest.clone(),
            coverage: manifest.coverage,
        })
    }
}

pub struct PublishedSourceSet {
    pub registry_generation: u64,
    pub current_source_id: SourceId,
    pub sources: BTreeMap<SourceId, Arc<PublishedGeneration>>,
}

impl PublishedSourceSet {
    pub fn current_generation(&self) -> Arc<PublishedGeneration> {
        Arc::clone(
            self.sources
                .get(&self.current_source_id)
                .expect("published source set must contain its current source"),
        )
    }

    /// Build the next source set after a P0 (current-worktree) publish.
    ///
    /// Preserves every P1 ref/worktree lane, replaces only the current lane,
    /// and bumps `registry_generation`. If the current source identity changed
    /// (e.g. branch/working-tree switch) the prior current lane is dropped so it
    /// is not stranded as a phantom lane. This mirrors the P1 lane discipline in
    /// `publish_ref_source`: a publish only ever rewrites its own source entry.
    fn next_after_current_publish(
        &self,
        current_source_id: SourceId,
        current_generation: Arc<PublishedGeneration>,
    ) -> PublishedSourceSet {
        let mut sources = self.sources.clone();
        if self.current_source_id != current_source_id {
            sources.remove(&self.current_source_id);
        }
        sources.insert(current_source_id.clone(), current_generation);
        PublishedSourceSet {
            registry_generation: self.registry_generation.saturating_add(1),
            current_source_id,
            sources,
        }
    }
}

fn unbound_source_id() -> SourceId {
    SourceId::new("symforge:unbound-source")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationFence {
    pub publication_generation: u64,
    pub content_generation: u64,
    pub project_generation: u64,
}

pub struct PreparedKnowledgeBridge {
    fence: PublicationFence,
    bridge: Arc<KnowledgeBridge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityPublicationFence {
    pub publication: PublicationFence,
    pub source: Option<SourceIdentity>,
    pub source_version: Option<SourceVersion>,
}

/// Exact repository state a background git-temporal computation is allowed to
/// describe. Publication-only refreshes do not invalidate the work, while any
/// content, source identity, or source-version movement does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitTemporalPublicationFence {
    pub project_generation: u64,
    pub content_generation: u64,
    pub source: Option<SourceIdentity>,
    pub source_version: Option<SourceVersion>,
}

pub struct PreparedKnowledgeAuthority {
    fence: AuthorityPublicationFence,
    authority: Arc<KnowledgeAuthorityView>,
}

fn capture_published_manifest(
    live: &LiveIndex,
    scout_plan: Option<&discovery::ScoutPlan>,
) -> Option<Arc<RepositoryManifest>> {
    let root = live.indexed_root.as_deref()?;
    let canonical_root = dunce::canonicalize(root).ok()?;
    let project_id = crate::discovery::project_id_for_canonical_root(&canonical_root);
    let captured = match super::persist::capture_repository_source(&canonical_root, &project_id) {
        Ok(captured) => captured,
        Err(error) => {
            warn!(%error, "failed to capture published source identity");
            return None;
        }
    };
    let coverage = scout_plan.map_or_else(
        || {
            if manifest_requires_degraded_coverage(live) {
                CoverageStatus::Degraded
            } else {
                CoverageStatus::Complete
            }
        },
        |plan| plan.coverage,
    );
    let issues = scout_plan
        .map(|plan| plan.issues.clone())
        .unwrap_or_default();
    let usage = scout_plan
        .map(|plan| plan.usage)
        .unwrap_or_else(|| ManifestResourceUsage {
            catalog_entries: live.manifest_entries.len() as u64,
            catalog_metadata_bytes: serde_json::to_vec(&live.manifest_entries)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or_default(),
            admitted_content_bytes: live.files.values().map(|file| file.byte_len).sum(),
        });
    match RepositoryManifest::new(
        1,
        1,
        crate::knowledge::SECRET_POLICY_VERSION,
        captured.source,
        captured.source_version,
        coverage,
        live.manifest_entries.clone(),
        issues,
        usage,
    ) {
        Ok(manifest) => Some(Arc::new(manifest)),
        Err(error) => {
            warn!(%error, "failed to build canonical published manifest");
            None
        }
    }
}

fn capture_history_coverage(
    live: &LiveIndex,
    temporal_state: &super::git_temporal::GitTemporalState,
) -> Arc<HistoryCoverage> {
    let Some(root) = live.indexed_root.as_deref() else {
        return Arc::new(HistoryCoverage {
            complete_to_root: false,
            limitations: vec![HistoryLimit::Unavailable],
        });
    };
    let mut coverage = super::persist::capture_history_coverage(root);
    let mut add_limit = |limit| {
        if !coverage.limitations.contains(&limit) {
            coverage.limitations.push(limit);
        }
    };
    match temporal_state {
        super::git_temporal::GitTemporalState::Ready => {
            // The shipped temporal index intentionally analyzes a bounded
            // commit/time window and does not follow renames across paths.
            add_limit(HistoryLimit::WindowLimited);
            add_limit(HistoryLimit::RenameFollowLimited);
        }
        super::git_temporal::GitTemporalState::Pending
        | super::git_temporal::GitTemporalState::Computing
        | super::git_temporal::GitTemporalState::Unavailable(_) => {
            add_limit(HistoryLimit::Unavailable);
        }
    }
    coverage.complete_to_root = false;
    Arc::new(coverage)
}

fn build_published_authority(
    live: &LiveIndex,
    source: Option<&SourceIdentity>,
    source_version: Option<&SourceVersion>,
    content_generation: u64,
    bridge: &KnowledgeBridge,
    code_signals: &CodeSignalsSnapshot,
    manifest: Option<&RepositoryManifest>,
) -> Arc<KnowledgeAuthorityView> {
    let (Some(source), Some(source_version)) = (source, source_version) else {
        return Arc::new(KnowledgeAuthorityView::default());
    };
    let temporal =
        AuthorityTemporalIndex::from_components(live, Some(source_version), code_signals);
    Arc::new(build_knowledge_authority(
        live,
        source,
        source_version,
        content_generation,
        bridge,
        &temporal,
        manifest.map_or(crate::knowledge::SECRET_POLICY_VERSION, |manifest| {
            manifest.secret_policy_version
        }),
        &AuthorityLimits::default(),
    ))
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
    /// Canonical catalog/disposition state for the current live generation.
    ///
    /// This is the runtime `RepositoryManifest.entries` lane. Gate E wraps it in
    /// the fully versioned manifest/publication bundle; until then it already is
    /// the sole disposition authority. Legacy `SkippedFile` responses are
    /// projected from these entries and are never stored independently.
    pub(crate) manifest_entries: Vec<CatalogEntry>,
    /// Per-workspace co-change store, present when policy warms it or when
    /// lazy policy finds an existing store at startup.
    pub(crate) coupling_store: Option<Arc<super::coupling::CouplingStore>>,
    /// Reason this index started empty, if any. Set at construction time by
    /// the startup-plan branch; surfaced in `health` output as an actionable
    /// banner. `None` when the index has files or after a reload.
    pub(crate) local_empty_reason: Arc<parking_lot::RwLock<Option<String>>>,
    /// Normalized filesystem root this index was built from, recorded so a
    /// changed target root can invalidate a stale in-memory index without any
    /// caller having to remember to call `reset_to_empty`. `None` for an empty
    /// bootstrap index (no root has been loaded yet). Populated via
    /// [`normalize_root`] on full load and reload so comparisons are stable
    /// across `\\?\` prefixes, trailing separators, and separator/case
    /// differences. See `SymForgeServer::ensure_local_index`.
    pub(crate) indexed_root: Option<PathBuf>,
}

/// Lightweight snapshot of a symbol for pre-update diffing in `analyze_file_impact`.
///
/// Stored in [`SharedIndexHandle::pre_update_snapshots`] so the impact tool can
/// compare against the state *before* the watcher or edit tools re-indexed.
#[derive(Clone, Debug)]
pub struct PreUpdateSnapshot {
    pub content: Vec<u8>,
    pub symbols: Vec<PreUpdateSymbol>,
}

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
    published_source_set: ArcSwap<PublishedSourceSet>,
    /// Typed owner for every durable per-project side store associated with
    /// the currently published project generation.
    project_state_dir: ArcSwapOption<ProjectStateDir>,
    /// Resolved state/control subtrees that are outside the active source
    /// generation even when their names are not universally reserved.
    source_exclusions: ArcSwap<discovery::SourceExclusions>,
    /// Immutable metadata-first catalog that authorized the current execution
    /// generation. `None` only for an empty/snapshot compatibility bootstrap.
    scout_plan: ArcSwapOption<discovery::ScoutPlan>,
    /// Typed freshness is kept beside the live/scout generation until Gate E
    /// folds both into the final single publication bundle.
    freshness_status: ArcSwap<FreshnessStatus>,
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
    /// One running git-temporal computation plus one replaceable latest
    /// request. This bounds background work during watcher bursts.
    pub(super) git_temporal_jobs: Mutex<super::git_temporal::GitTemporalJobQueue>,
    /// Pre-update file snapshots: saved automatically by `update_file` before
    /// the index entry is replaced. Consumed (take) by `analyze_file_impact` to
    /// compute accurate diffs even when the watcher re-indexes before the hook fires.
    pre_update_snapshots: Mutex<HashMap<String, PreUpdateSnapshot>>,
    /// Single-flights P1 local-ref/worktree reconcile. A reconcile pass reads the
    /// latest git topology, so two overlapping passes are redundant AND unsafe: the
    /// older pass's deletion step, working from a stale `local_branch_refs`
    /// snapshot, could cross-delete a lane the newer pass just published. We
    /// `try_lock` (never block — reconcile must never stall P0): if a pass is
    /// already running, a second caller skips, because the running pass already
    /// reflects the newest refs.
    ref_reconcile_lock: Mutex<()>,
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
        Self::new_with_scout_plan(index, None)
    }

    fn new_with_scout_plan(
        index: LiveIndex,
        scout_plan: Option<Arc<discovery::ScoutPlan>>,
    ) -> Self {
        Self::new_with_scout_plan_and_code_signals(index, scout_plan, None)
    }

    fn new_with_scout_plan_and_code_signals(
        index: LiveIndex,
        scout_plan: Option<Arc<discovery::ScoutPlan>>,
        code_signals: Option<CodeSignalsSnapshot>,
    ) -> Self {
        let manifest = capture_published_manifest(&index, scout_plan.as_deref());
        let source = manifest
            .as_ref()
            .map(|manifest| Arc::new(manifest.source.clone()));
        let source_version = manifest
            .as_ref()
            .map(|manifest| Arc::new(manifest.source_version.clone()));
        let code_signals = Arc::new(code_signals.unwrap_or_else(|| {
            let temporal = Arc::new(super::git_temporal::GitTemporalIndex::pending());
            let coverage = capture_history_coverage(&index, &temporal.state);
            CodeSignalsSnapshot {
                state: temporal.state.clone(),
                temporal,
                computed_for_content_generation: 0,
                computed_for_source_version: source_version.as_deref().cloned().unwrap_or(
                    SourceVersion {
                        branch: None,
                        commit: None,
                        working_tree: WorkingTreeState::Unknown,
                    },
                ),
                coverage,
            }
        }));
        let temporal = Arc::clone(&code_signals.temporal);
        let freshness_status = if index.is_empty
            || !matches!(index.snapshot_verify_state, SnapshotVerifyState::NotNeeded)
        {
            FreshnessStatus::Verifying
        } else {
            FreshnessStatus::Current
        };
        let published_state = Arc::new(PublishedIndexState::capture(0, &index));
        let published_repo_outline = Arc::new(index.capture_repo_outline_view());
        let bridge = source
            .as_deref()
            .map(|source| {
                Arc::new(build_knowledge_bridge(
                    &index,
                    source,
                    0,
                    &BridgeLimits::default(),
                ))
            })
            .unwrap_or_else(|| Arc::new(KnowledgeBridge::default()));
        let authority = build_published_authority(
            &index,
            source.as_deref(),
            source_version.as_deref(),
            0,
            &bridge,
            &code_signals,
            manifest.as_deref(),
        );
        let live = Arc::new(index);
        let published_generation = Arc::new(PublishedGeneration {
            publication_generation: 0,
            content_generation: 0,
            project_generation: 0,
            source: source.clone(),
            source_version,
            freshness: Arc::new(freshness_status.clone()),
            manifest,
            code_signals,
            bridge,
            authority,
            live: Arc::clone(&live),
            health: Arc::clone(&published_state),
            outline: Arc::clone(&published_repo_outline),
        });
        let current_source_id = source
            .as_ref()
            .map(|source| source.source_id.clone())
            .unwrap_or_else(unbound_source_id);
        let mut sources = BTreeMap::new();
        sources.insert(current_source_id.clone(), published_generation);
        let published_source_set = Arc::new(PublishedSourceSet {
            registry_generation: 0,
            current_source_id,
            sources,
        });
        Self {
            live: ArcSwap::new(live),
            published_source_set: ArcSwap::new(published_source_set),
            project_state_dir: ArcSwapOption::empty(),
            source_exclusions: ArcSwap::new(Arc::new(discovery::SourceExclusions::default())),
            scout_plan: ArcSwapOption::new(scout_plan),
            freshness_status: ArcSwap::new(Arc::new(freshness_status)),
            write_mutex: Mutex::new(()),
            published_state: ArcSwap::new(published_state),
            published_repo_outline: ArcSwap::new(published_repo_outline),
            next_generation: AtomicU64::new(1),
            project_generation: AtomicU64::new(0),
            last_reset_project_generation: AtomicU64::new(0),
            rejected_stale_mutations: AtomicU64::new(0),
            git_temporal: ArcSwap::new(temporal),
            git_temporal_jobs: Mutex::new(super::git_temporal::GitTemporalJobQueue::default()),
            pre_update_snapshots: Mutex::new(HashMap::new()),
            ref_reconcile_lock: Mutex::new(()),
        }
    }

    /// Acquire the single-flight guard for a P1 ref/worktree reconcile pass.
    ///
    /// Returns `None` when a pass is already in flight — the caller must SKIP
    /// rather than block, because the running pass already reflects the newest
    /// git refs (finding F). The guard is held for the whole reconcile.
    pub(crate) fn try_lock_ref_reconcile(&self) -> Option<MutexGuard<'_, ()>> {
        self.ref_reconcile_lock.try_lock()
    }

    /// Build a full published bundle for one local Git ref source (Gate L L-G07).
    ///
    /// Models the current-lane bundle in `new_with_scout_plan_and_code_signals`,
    /// but for a `SourceLocation::GitRef` source assembled from Git blobs rather
    /// than a filesystem walk. Temporal signals are `Pending` (a ref source has no
    /// resident working tree to walk); the manifest, bridge, and authority are the
    /// same builders the current lane uses, so a ref document links only to ref
    /// code of its own source identity.
    ///
    /// Per-source generations (L-R06/L-R13): `publication_generation` is drawn from
    /// the same monotonic `next_generation` dispenser the P0 path uses, so an
    /// all-source envelope can observe a ref lane republish. `content_generation`
    /// advances only when THIS lane's tip commit actually moves (it is carried
    /// forward unchanged for an identical-tip republish). Building this bundle never
    /// mutates the P0 current lane's own `PublishedGeneration`, which
    /// `publish_ref_source` leaves byte-identical, so P0 generations stay put.
    pub(crate) fn build_ref_source_generation(
        &self,
        index: LiveIndex,
        repository_id: crate::domain::index::RepositoryId,
        ref_name: &str,
        tip_commit: &str,
        scout_coverage: crate::domain::CoverageStatus,
    ) -> Arc<PublishedGeneration> {
        let source_id = crate::domain::index::SourceId::new(format!(
            "symforge:git-ref:{}:{ref_name}",
            repository_id.as_str()
        ));

        // Monotonic publication ticket from the shared dispenser — the same atomic
        // the P0 swap path pulls from — so this lane's publication_generation is
        // meaningful and strictly advances on every republish (L-R06).
        let publication_generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        // content_generation advances only when this lane's tip moved. We read the
        // currently-published lane for this exact source id and compare its recorded
        // commit; an identical-tip republish carries the same content generation.
        // ponytail: best-effort read outside publish_ref_source's writer lock; a
        // reconcile pass is single-flighted (see `try_lock_ref_reconcile`) and a
        // ref lane is not republished concurrently with itself, so the TOCTOU window
        // cannot double-count. Fold this read under the writer lock if that changes.
        let content_generation = {
            let published = self.published_source_set.load_full();
            match published.sources.get(&source_id) {
                Some(previous) => {
                    let previous_tip = previous
                        .source_version
                        .as_ref()
                        .and_then(|version| version.commit.as_deref());
                    if previous_tip == Some(tip_commit) {
                        previous.content_generation
                    } else {
                        previous.content_generation.saturating_add(1)
                    }
                }
                None => 1,
            }
        };

        let source = Arc::new(SourceIdentity {
            repository_id,
            source_id,
            location: crate::domain::index::SourceLocation::GitRef {
                name: ref_name.to_string(),
            },
        });
        let source_version = Arc::new(SourceVersion {
            branch: Some(ref_name.to_string()),
            commit: Some(tip_commit.to_string()),
            working_tree: WorkingTreeState::NotApplicable,
        });

        let temporal = Arc::new(super::git_temporal::GitTemporalIndex::pending());
        let coverage = capture_history_coverage(&index, &temporal.state);
        let code_signals = Arc::new(CodeSignalsSnapshot {
            state: temporal.state.clone(),
            temporal,
            computed_for_content_generation: content_generation,
            computed_for_source_version: (*source_version).clone(),
            coverage,
        });

        // ponytail: ref-lane manifest carries indexed-file usage only; withheld/
        // catalog-only entries are not enumerated (empty `entries`). No Gate L
        // contract requires P1 manifest catalog-entry parity — L-R06 (tasks.md)
        // scopes the all-source envelope to per-source generation/digest/coverage/
        // review-hash, which the generations above satisfy. Populate from the
        // `LocalRefCatalog` (incl. its withheld routing decisions) if a future
        // contract requires ref-lane catalog parity.
        let usage = ManifestResourceUsage {
            catalog_entries: index.files.len() as u64,
            catalog_metadata_bytes: 0,
            admitted_content_bytes: index.files.values().map(|file| file.byte_len).sum(),
        };
        let manifest = RepositoryManifest::new(
            1,
            1,
            crate::knowledge::SECRET_POLICY_VERSION,
            (*source).clone(),
            (*source_version).clone(),
            scout_coverage,
            Vec::new(),
            Vec::new(),
            usage,
        )
        .ok()
        .map(Arc::new);

        let freshness = if index.is_empty {
            FreshnessStatus::Verifying
        } else {
            FreshnessStatus::Current
        };
        let published_state =
            Arc::new(PublishedIndexState::capture(publication_generation, &index));
        let outline = Arc::new(index.capture_repo_outline_view());
        let bridge = Arc::new(build_knowledge_bridge(
            &index,
            &source,
            content_generation,
            &BridgeLimits::default(),
        ));
        let authority = build_published_authority(
            &index,
            Some(&source),
            Some(&source_version),
            content_generation,
            &bridge,
            &code_signals,
            manifest.as_deref(),
        );
        let live = Arc::new(index);
        Arc::new(PublishedGeneration {
            publication_generation,
            content_generation,
            project_generation: 0,
            source: Some(source),
            source_version: Some(source_version),
            freshness: Arc::new(freshness),
            manifest,
            code_signals,
            bridge,
            authority,
            live,
            health: published_state,
            outline,
        })
    }

    /// Reconcile one local-ref (P1) source bundle into the published source set.
    ///
    /// Copies the current source map under the single publication writer lock,
    /// inserts or replaces ONLY this ref lane, bumps `registry_generation`, and
    /// swaps once. The current worktree (P0) bundle is left byte-identical, so a
    /// P1 add/update/remove never advances the current source's publication,
    /// content, or project generation (L-R12/L-R13).
    pub(crate) fn publish_ref_source(&self, generation: Arc<PublishedGeneration>) {
        let source_id = generation
            .source
            .as_ref()
            .expect("ref-source generation carries a source identity")
            .source_id
            .clone();
        let _guard = self.write_mutex.lock();
        let current = self.published_source_set.load_full();
        let mut sources = current.sources.clone();
        sources.insert(source_id, generation);
        self.published_source_set
            .store(Arc::new(PublishedSourceSet {
                registry_generation: current.registry_generation.saturating_add(1),
                current_source_id: current.current_source_id.clone(),
                sources,
            }));
    }

    /// Remove one local-ref (P1) source lane from the published source set.
    ///
    /// Same discipline as `publish_ref_source`: copy under the writer lock, remove
    /// only the named lane, bump `registry_generation`, swap once. The current
    /// source lane cannot be removed. Returns whether a lane was removed.
    ///
    /// Production caller: `reconcile_local_ref_topology` (L-G05/L-R03), which
    /// invalidates the lane of a deleted or newly-checked-out branch.
    pub(crate) fn remove_ref_source(&self, source_id: &crate::domain::index::SourceId) -> bool {
        let _guard = self.write_mutex.lock();
        let current = self.published_source_set.load_full();
        if source_id == &current.current_source_id || !current.sources.contains_key(source_id) {
            return false;
        }
        let mut sources = current.sources.clone();
        sources.remove(source_id);
        self.published_source_set
            .store(Arc::new(PublishedSourceSet {
                registry_generation: current.registry_generation.saturating_add(1),
                current_source_id: current.current_source_id.clone(),
                sources,
            }));
        true
    }

    pub fn shared(index: LiveIndex) -> Arc<Self> {
        Arc::new(Self::new(index))
    }

    #[cfg(test)]
    pub(crate) fn shared_with_code_signals(
        index: LiveIndex,
        code_signals: CodeSignalsSnapshot,
    ) -> Arc<Self> {
        Arc::new(Self::new_with_scout_plan_and_code_signals(
            index,
            None,
            Some(code_signals),
        ))
    }

    pub(crate) fn shared_with_source_exclusions_and_code_signals(
        index: LiveIndex,
        source_exclusions: discovery::SourceExclusions,
        code_signals: CodeSignalsSnapshot,
    ) -> Arc<Self> {
        let handle = Self::new_with_scout_plan_and_code_signals(index, None, Some(code_signals));
        handle.source_exclusions.store(Arc::new(source_exclusions));
        Arc::new(handle)
    }

    fn shared_with_scout_plan(
        index: LiveIndex,
        scout_plan: discovery::ScoutPlan,
        source_exclusions: discovery::SourceExclusions,
    ) -> Arc<Self> {
        let is_degraded = matches!(scout_plan.coverage, crate::domain::CoverageStatus::Degraded);
        let handle = Self::new_with_scout_plan(index, Some(Arc::new(scout_plan)));
        handle.source_exclusions.store(Arc::new(source_exclusions));
        if is_degraded {
            handle
                .freshness_status
                .store(Arc::new(FreshnessStatus::Degraded {
                    last_valid_content_generation: 0,
                    reason_codes: vec![FreshnessReason::ReconciliationPending],
                }));
        }
        Arc::new(handle)
    }

    pub fn shared_for_state_placement(
        index: LiveIndex,
        root: &Path,
        state_placement: &StatePlacement,
    ) -> Arc<Self> {
        let handle = Self::new(index);
        handle
            .source_exclusions
            .store(Arc::new(discovery::SourceExclusions::for_state_placement(
                root,
                state_placement,
            )));
        handle
            .project_state_dir
            .store(state_placement.directory().cloned().map(Arc::new));
        Arc::new(handle)
    }

    pub fn shared_for_state_placement_with_code_signals(
        index: LiveIndex,
        root: &Path,
        state_placement: &StatePlacement,
        code_signals: CodeSignalsSnapshot,
    ) -> Arc<Self> {
        let handle = Self::new_with_scout_plan_and_code_signals(index, None, Some(code_signals));
        handle
            .source_exclusions
            .store(Arc::new(discovery::SourceExclusions::for_state_placement(
                root,
                state_placement,
            )));
        handle
            .project_state_dir
            .store(state_placement.directory().cloned().map(Arc::new));
        Arc::new(handle)
    }

    #[must_use]
    pub fn project_state_dir(&self) -> Option<Arc<ProjectStateDir>> {
        self.project_state_dir.load_full()
    }

    #[must_use]
    pub fn scout_plan(&self) -> Option<Arc<discovery::ScoutPlan>> {
        self.scout_plan.load_full()
    }

    #[must_use]
    pub(crate) fn source_exclusions(&self) -> Arc<discovery::SourceExclusions> {
        self.source_exclusions.load_full()
    }

    fn scout_plan_with_entry_locked(
        &self,
        entry: crate::domain::ScoutedEntry,
        live: &LiveIndex,
    ) -> anyhow::Result<Option<Arc<discovery::ScoutPlan>>> {
        let Some(current) = self.scout_plan.load_full() else {
            return Ok(None);
        };
        let mut plan = (*current).clone();
        let entry_changed = match plan
            .entries
            .iter()
            .position(|existing| existing.path == entry.path)
        {
            Some(position) if plan.entries[position] == entry => false,
            Some(position) => {
                plan.entries[position] = entry;
                true
            }
            None => {
                plan.entries.push(entry);
                true
            }
        };
        discovery::refresh_scout_plan(&mut plan)?;
        if manifest_requires_degraded_coverage(live) {
            plan.coverage = crate::domain::CoverageStatus::Degraded;
        }
        if !entry_changed && plan.coverage == current.coverage {
            return Ok(None);
        }
        Ok(Some(Arc::new(plan)))
    }

    fn scout_plan_without_path_locked(
        &self,
        path: &str,
    ) -> anyhow::Result<Option<Arc<discovery::ScoutPlan>>> {
        let Some(current) = self.scout_plan.load_full() else {
            return Ok(None);
        };
        let mut plan = (*current).clone();
        let entry_count = plan.entries.len();
        plan.entries
            .retain(|entry| entry.path.normalized_utf8.as_deref() != Some(path));
        if plan.entries.len() == entry_count {
            return Ok(None);
        }
        discovery::refresh_scout_plan(&mut plan)?;
        Ok(Some(Arc::new(plan)))
    }

    pub(crate) fn publish_reconciled_scout_plan_at_generation(
        &self,
        baseline: Option<&discovery::ScoutPlan>,
        mut scout_plan: discovery::ScoutPlan,
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
                "rejecting stale scout-plan publication"
            );
            return false;
        }

        if let Some(current) = self.scout_plan.load_full() {
            let baseline_entries: HashMap<crate::domain::CatalogPath, crate::domain::ScoutedEntry> =
                baseline
                    .map(|plan| {
                        plan.entries
                            .iter()
                            .cloned()
                            .map(|entry| (entry.path.clone(), entry))
                            .collect()
                    })
                    .unwrap_or_default();
            let current_entries: HashMap<crate::domain::CatalogPath, crate::domain::ScoutedEntry> =
                current
                    .entries
                    .iter()
                    .cloned()
                    .map(|entry| (entry.path.clone(), entry))
                    .collect();
            let mut fresh_entries: HashMap<
                crate::domain::CatalogPath,
                crate::domain::ScoutedEntry,
            > = scout_plan
                .entries
                .drain(..)
                .map(|entry| (entry.path.clone(), entry))
                .collect();
            let changed_paths: HashSet<crate::domain::CatalogPath> = baseline_entries
                .keys()
                .chain(current_entries.keys())
                .cloned()
                .collect();

            for path in changed_paths {
                if current_entries.get(&path) == baseline_entries.get(&path) {
                    continue;
                }
                match current_entries.get(&path) {
                    Some(entry) => {
                        fresh_entries.insert(path, entry.clone());
                    }
                    None => {
                        fresh_entries.remove(&path);
                    }
                }
            }
            scout_plan.entries = fresh_entries.into_values().collect();
        }
        if let Err(error) = discovery::refresh_scout_plan(&mut scout_plan) {
            tracing::error!(%error, "failed to refresh reconciled scout-plan accounting");
            return false;
        }
        if manifest_requires_degraded_coverage(&self.live.load_full()) {
            scout_plan.coverage = crate::domain::CoverageStatus::Degraded;
        }
        self.scout_plan.store(Some(Arc::new(scout_plan)));
        true
    }

    pub(crate) fn terminal_dispositions(
        &self,
    ) -> Arc<Vec<(String, crate::domain::FileDisposition)>> {
        Arc::new(
            self.live
                .load_full()
                .manifest_entries
                .iter()
                .map(|entry| {
                    (
                        scouted_catalog_path(&entry.path).to_string(),
                        entry.disposition.clone(),
                    )
                })
                .collect(),
        )
    }

    #[must_use]
    pub fn freshness_status(&self) -> Arc<FreshnessStatus> {
        Arc::clone(&self.published_generation().freshness)
    }

    pub(crate) fn set_freshness_status(&self, status: FreshnessStatus) {
        let _wg = self.write_mutex.lock();
        self.freshness_status.store(Arc::new(status));
        let live = (*self.live.load_full()).clone();
        self.swap_and_publish_retaining_content(live);
    }

    /// Lock-free read: returns a guard that derefs to `&LiveIndex`.
    ///
    /// The returned guard holds a snapshot of the index at the time of the call.
    /// Concurrent writes do not affect the snapshot — they swap in a new `Arc`
    /// that subsequent `read()` calls will see.
    pub fn read(&self) -> Arc<LiveIndex> {
        Arc::clone(&self.published_generation().live)
    }

    pub fn published_generation(&self) -> Arc<PublishedGeneration> {
        self.published_source_set.load_full().current_generation()
    }

    pub fn published_source_set(&self) -> Arc<PublishedSourceSet> {
        self.published_source_set.load_full()
    }

    pub fn publication_fence(&self) -> PublicationFence {
        let published = self.published_generation();
        PublicationFence {
            publication_generation: published.publication_generation,
            content_generation: published.content_generation,
            project_generation: published.project_generation,
        }
    }

    pub fn matches_publication_fence(&self, expected: PublicationFence) -> bool {
        self.publication_fence() == expected
    }

    pub fn prepare_bridge_rebuild(&self) -> PreparedKnowledgeBridge {
        let published = self.published_generation();
        let fence = PublicationFence {
            publication_generation: published.publication_generation,
            content_generation: published.content_generation,
            project_generation: published.project_generation,
        };
        let bridge = published
            .source
            .as_deref()
            .map(|source| {
                Arc::new(build_knowledge_bridge(
                    &published.live,
                    source,
                    published.content_generation,
                    &BridgeLimits::default(),
                ))
            })
            .unwrap_or_else(|| Arc::new(KnowledgeBridge::default()));
        PreparedKnowledgeBridge { fence, bridge }
    }

    pub fn publish_prepared_bridge(&self, prepared: PreparedKnowledgeBridge) -> bool {
        let _write_guard = self.write_mutex.lock();
        let previous_source_set = self.published_source_set.load_full();
        let previous = previous_source_set.current_generation();
        let current_fence = PublicationFence {
            publication_generation: previous.publication_generation,
            content_generation: previous.content_generation,
            project_generation: previous.project_generation,
        };
        if current_fence != prepared.fence {
            self.rejected_stale_mutations.fetch_add(1, Ordering::AcqRel);
            return false;
        }

        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let authority = build_published_authority(
            &previous.live,
            previous.source.as_deref(),
            previous.source_version.as_deref(),
            previous.content_generation,
            &prepared.bridge,
            &previous.code_signals,
            previous.manifest.as_deref(),
        );
        let published_generation = Arc::new(PublishedGeneration {
            publication_generation: generation,
            content_generation: previous.content_generation,
            project_generation: previous.project_generation,
            source: previous.source.clone(),
            source_version: previous.source_version.clone(),
            freshness: Arc::clone(&previous.freshness),
            manifest: previous.manifest.clone(),
            code_signals: Arc::clone(&previous.code_signals),
            bridge: prepared.bridge,
            authority,
            live: Arc::clone(&previous.live),
            health: Arc::clone(&previous.health),
            outline: Arc::clone(&previous.outline),
        });
        self.published_source_set
            .store(Arc::new(previous_source_set.next_after_current_publish(
                previous_source_set.current_source_id.clone(),
                published_generation,
            )));
        true
    }

    pub fn prepare_authority_rebuild(&self) -> PreparedKnowledgeAuthority {
        let published = self.published_generation();
        let fence = AuthorityPublicationFence {
            publication: PublicationFence {
                publication_generation: published.publication_generation,
                content_generation: published.content_generation,
                project_generation: published.project_generation,
            },
            source: published.source.as_deref().cloned(),
            source_version: published.source_version.as_deref().cloned(),
        };
        let authority = build_published_authority(
            &published.live,
            published.source.as_deref(),
            published.source_version.as_deref(),
            published.content_generation,
            &published.bridge,
            &published.code_signals,
            published.manifest.as_deref(),
        );
        PreparedKnowledgeAuthority { fence, authority }
    }

    pub fn publish_prepared_authority(&self, prepared: PreparedKnowledgeAuthority) -> bool {
        let _write_guard = self.write_mutex.lock();
        let previous_source_set = self.published_source_set.load_full();
        let previous = previous_source_set.current_generation();
        let current_fence = AuthorityPublicationFence {
            publication: PublicationFence {
                publication_generation: previous.publication_generation,
                content_generation: previous.content_generation,
                project_generation: previous.project_generation,
            },
            source: previous.source.as_deref().cloned(),
            source_version: previous.source_version.as_deref().cloned(),
        };
        if current_fence != prepared.fence {
            self.rejected_stale_mutations.fetch_add(1, Ordering::AcqRel);
            return false;
        }

        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let published_generation = Arc::new(PublishedGeneration {
            publication_generation: generation,
            content_generation: previous.content_generation,
            project_generation: previous.project_generation,
            source: previous.source.clone(),
            source_version: previous.source_version.clone(),
            freshness: Arc::clone(&previous.freshness),
            manifest: previous.manifest.clone(),
            code_signals: Arc::clone(&previous.code_signals),
            bridge: Arc::clone(&previous.bridge),
            authority: prepared.authority,
            live: Arc::clone(&previous.live),
            health: Arc::clone(&previous.health),
            outline: Arc::clone(&previous.outline),
        });
        self.published_source_set
            .store(Arc::new(previous_source_set.next_after_current_publish(
                previous_source_set.current_source_id.clone(),
                published_generation,
            )));
        true
    }

    pub fn refresh_source_metadata(&self) -> bool {
        let _write_guard = self.write_mutex.lock();
        let previous = self.published_generation();
        let live = self.live.load_full();
        let scout_plan = self.scout_plan.load_full();
        let candidate = capture_published_manifest(&live, scout_plan.as_deref());
        let candidate_source = candidate.as_ref().map(|manifest| &manifest.source);
        let candidate_version = candidate.as_ref().map(|manifest| &manifest.source_version);
        if candidate_source == previous.source.as_deref()
            && candidate_version == previous.source_version.as_deref()
        {
            return false;
        }
        self.swap_and_publish_retaining_content((*live).clone());
        true
    }

    /// Capture the exact content/source version a git-temporal result will
    /// describe. Refresh first so bytes-identical commits and branch movement
    /// are fenced even when the filesystem watcher has no content event.
    pub fn git_temporal_publication_fence(&self) -> GitTemporalPublicationFence {
        self.refresh_source_metadata();
        let published = self.published_generation();
        GitTemporalPublicationFence {
            project_generation: published.project_generation,
            content_generation: published.content_generation,
            source: published.source.as_deref().cloned(),
            source_version: published.source_version.as_deref().cloned(),
        }
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
        Arc::clone(&self.published_generation().health)
    }

    /// Lock-free read of the published repo outline.
    pub fn published_repo_outline(&self) -> Arc<RepoOutlineView> {
        Arc::clone(&self.published_generation().outline)
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
        self.reload_for_binding(root, None)
    }

    /// Reload while explicitly controlling whether source-local project state
    /// is eligible for this binding.
    pub(crate) fn reload_for_binding(
        &self,
        root: &Path,
        project_state_dir: Option<ProjectStateDir>,
    ) -> anyhow::Result<()> {
        self.reload_for_binding_with_exclusions(
            root,
            project_state_dir,
            discovery::SourceExclusions::default(),
        )
    }

    pub fn reload_for_state_placement(
        &self,
        root: &Path,
        state_placement: &StatePlacement,
    ) -> anyhow::Result<()> {
        self.reload_for_binding_with_exclusions(
            root,
            state_placement.directory().cloned(),
            discovery::SourceExclusions::for_state_placement(root, state_placement),
        )
    }

    pub(crate) fn reload_for_binding_with_exclusions(
        &self,
        root: &Path,
        project_state_dir: Option<ProjectStateDir>,
        source_exclusions: discovery::SourceExclusions,
    ) -> anyhow::Result<()> {
        // Build new index data OUTSIDE the write lock (file I/O + parsing).
        // Only the final swap acquires the mutex, reducing block time from
        // seconds (full I/O) to milliseconds (in-memory index rebuild).
        let data = LiveIndex::build_reload_data_for_binding_with_exclusions(
            root,
            project_state_dir.as_ref(),
            &source_exclusions,
        )?;
        let scout_plan = Arc::clone(&data.scout_plan);
        let is_degraded = matches!(scout_plan.coverage, crate::domain::CoverageStatus::Degraded);
        let live = LiveIndex::from_reload_data(data);
        let _wg = self.write_mutex.lock();
        self.source_exclusions.store(Arc::new(source_exclusions));
        self.scout_plan.store(Some(scout_plan));
        self.freshness_status.store(Arc::new(if is_degraded {
            FreshnessStatus::Degraded {
                last_valid_content_generation: self.published_state.load().generation,
                reason_codes: vec![FreshnessReason::ReconciliationPending],
            }
        } else {
            FreshnessStatus::Current
        }));
        self.project_state_dir
            .store(project_state_dir.map(Arc::new));
        self.project_generation.fetch_add(1, Ordering::AcqRel);
        self.swap_and_publish(live);
        self.last_reset_project_generation
            .store(0, Ordering::Release);
        Ok(())
    }

    pub(crate) fn is_source_excluded(&self, relative_path: &Path) -> bool {
        self.source_exclusions
            .load()
            .excludes_relative(relative_path)
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
        self.source_exclusions
            .store(Arc::new(discovery::SourceExclusions::default()));
        self.scout_plan.store(None);
        self.freshness_status
            .store(Arc::new(FreshnessStatus::Verifying));
        self.project_generation.fetch_add(1, Ordering::AcqRel);
        self.swap_and_publish(LiveIndex::empty_live_index());
        self.last_reset_project_generation
            .store(0, Ordering::Release);
        self.pre_update_snapshots.lock().clear();
    }

    pub fn update_file(&self, path: String, file: IndexedFile) {
        let _wg = self.write_mutex.lock();
        let current = self.live.load_full();
        // Capture pre-update symbols so analyze_file_impact can diff correctly
        // even when the watcher re-indexes before the hook fires.
        if let Some(existing) = current.get_file(&path) {
            self.pre_update_snapshots.lock().insert(
                path.clone(),
                PreUpdateSnapshot {
                    content: existing.content.clone(),
                    symbols: existing
                        .symbols
                        .iter()
                        .map(|s| PreUpdateSymbol {
                            name: s.name.clone(),
                            kind: s.kind.to_string(),
                            line_range: s.line_range,
                            byte_range: s.byte_range,
                        })
                        .collect(),
                },
            );
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
            self.pre_update_snapshots.lock().insert(
                path.to_string(),
                PreUpdateSnapshot {
                    content: existing.content.clone(),
                    symbols: existing
                        .symbols
                        .iter()
                        .map(|s| PreUpdateSymbol {
                            name: s.name.clone(),
                            kind: s.kind.to_string(),
                            line_range: s.line_range,
                            byte_range: s.byte_range,
                        })
                        .collect(),
                },
            );
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

    /// Publish one admitted file across the content, derived-index, catalog,
    /// and terminal-disposition lanes under one generation fence.
    pub(crate) fn publish_indexed_file_at_generation(
        &self,
        path: &str,
        file: IndexedFile,
        scouted: crate::domain::ScoutedEntry,
        targets: crate::domain::IndexTargets,
        expected_gen: u64,
        expected_publication_gen: u64,
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
                "rejecting stale indexed-file publication"
            );
            return false;
        }
        let current_publication_gen = self.published_state.load().generation;
        if current_publication_gen != expected_publication_gen {
            tracing::trace!(
                path,
                expected_publication_gen,
                current_publication_gen,
                "rejecting indexed-file publication prepared from a stale source bundle"
            );
            return false;
        }

        let current = self.live.load_full();
        if let Some(existing) = current.get_file(path) {
            self.pre_update_snapshots.lock().insert(
                path.to_string(),
                PreUpdateSnapshot {
                    content: existing.content.clone(),
                    symbols: existing
                        .symbols
                        .iter()
                        .map(|symbol| PreUpdateSymbol {
                            name: symbol.name.clone(),
                            kind: symbol.kind.to_string(),
                            line_range: symbol.line_range,
                            byte_range: symbol.byte_range,
                        })
                        .collect(),
                },
            );
        }

        let parse_status = match &file.parse_status {
            ParseStatus::Parsed => crate::domain::index::ParseStatus::Parsed,
            ParseStatus::PartialParse { .. } => crate::domain::index::ParseStatus::PartialParse,
            ParseStatus::Failed { .. } => crate::domain::index::ParseStatus::Failed,
        };
        let disposition = FileDisposition::Indexed {
            targets,
            parse_status,
        };
        let manifest_entry =
            catalog_entry_from_scout(&scouted, disposition, Some(file.content_hash.clone()));
        let mut live = (*current).clone();
        let path_owned = path.to_string();
        let path_for_log = path_owned.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            live.update_file(path_owned, file);
            live.upsert_manifest_entry(manifest_entry);
        }));
        if let Err(panic_info) = result {
            let msg = panic_info
                .downcast_ref::<String>()
                .map(|message| message.as_str())
                .or_else(|| panic_info.downcast_ref::<&str>().copied())
                .unwrap_or("unknown");
            tracing::error!(
                "indexed-file publication panicked for '{}': {} — original index preserved",
                path_for_log,
                msg
            );
            return false;
        }

        let updated_scout_plan = match self.scout_plan_with_entry_locked(scouted, &live) {
            Ok(plan) => plan,
            Err(error) => {
                tracing::error!(path, %error, "failed to refresh indexed-file scout plan");
                return false;
            }
        };

        if let Some(plan) = updated_scout_plan {
            self.scout_plan.store(Some(plan));
        }
        self.swap_and_publish(live);
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
            self.swap_and_publish_retaining_content(live);
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
            self.swap_and_publish_retaining_content(live);
        }
        true
    }

    pub(crate) fn publish_hash_skip_at_generation(
        &self,
        path: &str,
        new_mtime: u64,
        scouted: crate::domain::ScoutedEntry,
        targets: crate::domain::IndexTargets,
        expected_gen: u64,
        expected_publication_gen: u64,
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
                "rejecting stale hash-skip publication"
            );
            return false;
        }
        let current_publication_gen = self.published_state.load().generation;
        if current_publication_gen != expected_publication_gen {
            tracing::trace!(
                path,
                expected_publication_gen,
                current_publication_gen,
                "rejecting hash-skip prepared from a stale source bundle"
            );
            return false;
        }

        let current = self.live.load_full();
        let mut live = (*current).clone();
        let mut live_changed = false;
        let Some(file) = current.files.get(path) else {
            return false;
        };
        if file.mtime_secs != new_mtime && new_mtime != 0 {
            let mut updated = (**file).clone();
            updated.mtime_secs = new_mtime;
            live.files.insert(path.to_string(), Arc::new(updated));
            live_changed = true;
        }
        let parse_status = match &file.parse_status {
            ParseStatus::Parsed => crate::domain::index::ParseStatus::Parsed,
            ParseStatus::PartialParse { .. } => crate::domain::index::ParseStatus::PartialParse,
            ParseStatus::Failed { .. } => crate::domain::index::ParseStatus::Failed,
        };
        let indexed_disposition = FileDisposition::Indexed {
            targets,
            parse_status,
        };
        let manifest_entry = catalog_entry_from_scout(
            &scouted,
            indexed_disposition,
            Some(file.content_hash.clone()),
        );
        let manifest_changed = !current
            .manifest_entries
            .iter()
            .any(|entry| entry == &manifest_entry);
        if manifest_changed {
            live.upsert_manifest_entry(manifest_entry);
        }
        let updated_scout_plan = match self.scout_plan_with_entry_locked(scouted, &live) {
            Ok(plan) => plan,
            Err(error) => {
                tracing::error!(path, %error, "failed to refresh hash-skip scout plan");
                return false;
            }
        };
        if !live_changed && updated_scout_plan.is_none() && !manifest_changed {
            return true;
        }
        if let Some(plan) = updated_scout_plan {
            self.scout_plan.store(Some(plan));
        }
        self.swap_and_publish(live);
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
        self.remove_file_with_fences(path, expected_gen, None, None)
    }

    pub fn remove_file_at_publication_fence(&self, path: &str, expected: PublicationFence) -> bool {
        self.remove_file_with_fences(path, expected.project_generation, None, Some(expected))
    }

    pub(crate) fn remove_file_if_scout_entry_at_generation(
        &self,
        path: &str,
        expected_entry: &crate::domain::ScoutedEntry,
        expected_gen: u64,
    ) -> bool {
        self.remove_file_with_fences(path, expected_gen, Some(expected_entry), None)
    }

    fn remove_file_with_fences(
        &self,
        path: &str,
        expected_gen: u64,
        expected_scout_entry: Option<&crate::domain::ScoutedEntry>,
        expected_publication: Option<PublicationFence>,
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
                "rejecting stale file removal"
            );
            return false;
        }
        if let Some(expected_publication) = expected_publication {
            let current_publication = self.publication_fence();
            if current_publication != expected_publication {
                tracing::trace!(
                    path,
                    ?expected_publication,
                    ?current_publication,
                    "rejecting stale file removal publication"
                );
                return false;
            }
        }
        if let Some(expected_scout_entry) = expected_scout_entry {
            let current_plan = self.scout_plan.load_full();
            let current_entry = current_plan.as_ref().and_then(|plan| {
                plan.entries
                    .iter()
                    .find(|entry| entry.path == expected_scout_entry.path)
            });
            if current_entry != Some(expected_scout_entry) {
                tracing::trace!(
                    path,
                    "rejecting file removal because its scouted base changed"
                );
                return false;
            }
        }

        let mut live = (*self.live.load_full()).clone();
        let path_owned = path.to_string();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            live.remove_file(path);
            live.remove_manifest_entry(path);
        }));
        if let Err(panic_info) = result {
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
            return false;
        }

        let updated_scout_plan = match self.scout_plan_without_path_locked(path) {
            Ok(plan) => plan,
            Err(error) => {
                tracing::error!(path, %error, "failed to refresh scout plan after removal");
                return false;
            }
        };

        if let Some(plan) = updated_scout_plan {
            self.scout_plan.store(Some(plan));
        }
        self.swap_and_publish(live);
        true
    }

    /// Publish one metadata-terminal observation under the same generation
    /// fence and writer boundary as the compatibility live-index projection.
    pub(crate) fn publish_terminal_disposition_at_generation(
        &self,
        path: &str,
        scouted: crate::domain::ScoutedEntry,
        disposition: crate::domain::FileDisposition,
        expected_gen: u64,
        expected_publication_gen: u64,
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
                "rejecting stale terminal disposition"
            );
            return false;
        }
        let current_publication_gen = self.published_state.load().generation;
        if current_publication_gen != expected_publication_gen {
            tracing::trace!(
                path,
                expected_publication_gen,
                current_publication_gen,
                "rejecting terminal disposition prepared from a stale source bundle"
            );
            return false;
        }
        let manifest_entry = catalog_entry_from_scout(&scouted, disposition.clone(), None);
        let mut live = (*self.live.load_full()).clone();
        let retains_last_valid_content = matches!(
            &disposition,
            crate::domain::FileDisposition::Unreadable { .. }
                | crate::domain::FileDisposition::UnstableDuringRead
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if !retains_last_valid_content {
                live.remove_file(path);
            }
            live.upsert_manifest_entry(manifest_entry);
        }));
        if let Err(panic_info) = result {
            let msg = panic_info
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| panic_info.downcast_ref::<&str>().copied())
                .unwrap_or("unknown");
            tracing::error!(
                "terminal disposition mutation panicked for '{}': {} — original index preserved",
                path,
                msg
            );
            return false;
        }

        let updated_scout_plan = match self.scout_plan_with_entry_locked(scouted, &live) {
            Ok(plan) => plan,
            Err(error) => {
                tracing::error!(path, %error, "failed to refresh terminal scout plan");
                return false;
            }
        };

        if let Some(plan) = updated_scout_plan {
            self.scout_plan.store(Some(plan));
        }
        if retains_last_valid_content {
            let last_valid_content_generation = self.published_generation().content_generation;
            self.freshness_status
                .store(Arc::new(FreshnessStatus::Degraded {
                    last_valid_content_generation,
                    reason_codes: vec![FreshnessReason::ObservationFailed],
                }));
            self.swap_and_publish_retaining_content(live);
        } else {
            self.swap_and_publish(live);
        }
        true
    }

    pub fn mark_snapshot_verify_running(&self) {
        let expected = self.publication_fence();
        let _ = self.mark_snapshot_verify_running_at_fence(expected);
    }

    pub fn mark_snapshot_verify_running_at_generation(&self, expected_gen: u64) -> bool {
        let expected = self.publication_fence();
        if expected.project_generation != expected_gen {
            self.note_rejected_stale_mutation();
            return false;
        }
        self.mark_snapshot_verify_running_at_fence(expected)
            .is_some()
    }

    pub fn mark_snapshot_verify_running_at_fence(
        &self,
        expected: PublicationFence,
    ) -> Option<PublicationFence> {
        let _wg = self.write_mutex.lock();
        if self.publication_fence() != expected {
            self.note_rejected_stale_mutation();
            return None;
        }
        let mut live = (*self.live.load_full()).clone();
        live.mark_snapshot_verify_running();
        self.freshness_status
            .store(Arc::new(FreshnessStatus::Verifying));
        self.swap_and_publish_retaining_content(live);
        Some(self.publication_fence())
    }

    pub fn mark_snapshot_verify_completed(&self, mismatched_paths: Vec<String>) {
        let expected = self.publication_fence();
        let _ = self.mark_snapshot_verify_completed_at_fence(expected, mismatched_paths);
    }

    pub fn mark_snapshot_verify_completed_at_generation(
        &self,
        expected_gen: u64,
        mismatched_paths: Vec<String>,
    ) -> bool {
        let expected = self.publication_fence();
        if expected.project_generation != expected_gen {
            self.note_rejected_stale_mutation();
            return false;
        }
        self.mark_snapshot_verify_completed_at_fence(expected, mismatched_paths)
    }

    pub fn mark_snapshot_verify_completed_at_fence(
        &self,
        expected: PublicationFence,
        mismatched_paths: Vec<String>,
    ) -> bool {
        let _wg = self.write_mutex.lock();
        if self.publication_fence() != expected {
            self.note_rejected_stale_mutation();
            return false;
        }
        let mut live = (*self.live.load_full()).clone();
        let freshness = if mismatched_paths.is_empty() {
            FreshnessStatus::Current
        } else {
            FreshnessStatus::Degraded {
                last_valid_content_generation: expected.content_generation,
                reason_codes: vec![FreshnessReason::SnapshotVerificationFailed],
            }
        };
        live.mark_snapshot_verify_completed(mismatched_paths);
        self.freshness_status.store(Arc::new(freshness));
        self.swap_and_publish_retaining_content(live);
        true
    }

    /// Swap a new `LiveIndex` into the `ArcSwap` and publish derived state.
    ///
    /// Must be called while holding `write_mutex`.
    fn swap_and_publish(&self, live: LiveIndex) {
        self.swap_and_publish_with_content_change_and_hook(live, true, || {});
    }

    fn swap_and_publish_retaining_content(&self, live: LiveIndex) {
        self.swap_and_publish_with_content_change_and_hook(live, false, || {});
    }

    #[cfg(test)]
    fn swap_and_publish_with_hook<F>(&self, live: LiveIndex, after_live_swap: F)
    where
        F: FnOnce(),
    {
        self.swap_and_publish_with_content_change_and_hook(live, true, after_live_swap);
    }

    fn swap_and_publish_with_content_change_and_hook<F>(
        &self,
        live: LiveIndex,
        content_changed: bool,
        after_live_swap: F,
    ) where
        F: FnOnce(),
    {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let published_state = Arc::new(PublishedIndexState::capture(generation, &live));
        let published_repo_outline = Arc::new(live.capture_repo_outline_view());
        let previous_source_set = self.published_source_set.load_full();
        let previous = previous_source_set.current_generation();
        let content_generation = if content_changed {
            previous.content_generation.saturating_add(1)
        } else {
            previous.content_generation
        };
        let freshness = self.freshness_status.load_full();
        let scout_plan = self.scout_plan.load_full();
        let manifest = capture_published_manifest(&live, scout_plan.as_deref());
        let temporal = self.git_temporal.load_full();
        let history_coverage = capture_history_coverage(&live, &temporal.state);
        let source = manifest
            .as_ref()
            .map(|manifest| Arc::new(manifest.source.clone()));
        let source_version = manifest
            .as_ref()
            .map(|manifest| Arc::new(manifest.source_version.clone()));
        let code_signals = if Arc::ptr_eq(&temporal, &previous.code_signals.temporal) {
            Arc::clone(&previous.code_signals)
        } else {
            Arc::new(CodeSignalsSnapshot {
                state: temporal.state.clone(),
                temporal,
                computed_for_content_generation: content_generation,
                computed_for_source_version: source_version.as_deref().cloned().unwrap_or(
                    SourceVersion {
                        branch: None,
                        commit: None,
                        working_tree: WorkingTreeState::Unknown,
                    },
                ),
                coverage: history_coverage,
            })
        };
        let bridge = source
            .as_deref()
            .map(|source| {
                Arc::new(build_knowledge_bridge(
                    &live,
                    source,
                    content_generation,
                    &BridgeLimits::default(),
                ))
            })
            .unwrap_or_else(|| Arc::new(KnowledgeBridge::default()));
        let authority = build_published_authority(
            &live,
            source.as_deref(),
            source_version.as_deref(),
            content_generation,
            &bridge,
            &code_signals,
            manifest.as_deref(),
        );
        let live = Arc::new(live);
        let published_generation = Arc::new(PublishedGeneration {
            publication_generation: generation,
            content_generation,
            project_generation: self.project_generation.load(Ordering::Acquire),
            source: source.clone(),
            source_version,
            freshness,
            manifest,
            code_signals,
            bridge,
            authority,
            live: Arc::clone(&live),
            health: Arc::clone(&published_state),
            outline: Arc::clone(&published_repo_outline),
        });
        let current_source_id = source
            .as_ref()
            .map(|source| source.source_id.clone())
            .unwrap_or_else(unbound_source_id);
        let published_source_set = Arc::new(
            previous_source_set.next_after_current_publish(current_source_id, published_generation),
        );
        self.live.store(live);
        after_live_swap();
        self.published_state.store(published_state);
        self.published_repo_outline.store(published_repo_outline);
        self.published_source_set.store(published_source_set);
    }

    /// Lock-free read of the git temporal index.
    pub fn git_temporal(&self) -> Arc<super::git_temporal::GitTemporalIndex> {
        Arc::clone(&self.published_generation().code_signals.temporal)
    }

    /// Take (consume) the pre-update snapshot for a file, if any.
    ///
    /// Used by `analyze_file_impact` to get the file bytes and symbols from
    /// *before* the last `update_file` call — prevents the watcher race where
    /// the index is already updated to the post-edit state before the hook fires.
    pub fn take_pre_update_snapshot(&self, path: &str) -> Option<PreUpdateSnapshot> {
        self.pre_update_snapshots.lock().remove(path)
    }

    /// Backward-compatible accessor for callers that only need the symbol half.
    pub fn take_pre_update_symbols(&self, path: &str) -> Option<Vec<PreUpdateSymbol>> {
        self.take_pre_update_snapshot(path)
            .map(|snapshot| snapshot.symbols)
    }

    /// Atomically replace the git temporal index with a new version.
    pub fn update_git_temporal(&self, index: super::git_temporal::GitTemporalIndex) {
        let _wg = self.write_mutex.lock();
        self.git_temporal.store(Arc::new(index));
        let live = (*self.live.load_full()).clone();
        self.swap_and_publish_retaining_content(live);
    }

    pub fn update_git_temporal_at_fence(
        &self,
        index: super::git_temporal::GitTemporalIndex,
        fence: &GitTemporalPublicationFence,
    ) -> bool {
        let _wg = self.write_mutex.lock();
        let live = self.live.load_full();
        let scout_plan = self.scout_plan.load_full();
        let candidate = capture_published_manifest(&live, scout_plan.as_deref());
        let candidate_source = candidate.as_ref().map(|manifest| &manifest.source);
        let candidate_version = candidate.as_ref().map(|manifest| &manifest.source_version);
        let published = self.published_generation();
        let current_gen = self.project_generation.load(Ordering::Acquire);
        let matches = current_gen == fence.project_generation
            && published.content_generation == fence.content_generation
            && candidate_source == fence.source.as_ref()
            && candidate_version == fence.source_version.as_ref();
        if !matches {
            self.rejected_stale_mutations
                .fetch_add(1, Ordering::Relaxed);
            if candidate.is_some()
                && (candidate_source != published.source.as_deref()
                    || candidate_version != published.source_version.as_deref())
            {
                self.swap_and_publish_retaining_content((*live).clone());
            }
            tracing::trace!(
                expected_project_generation = fence.project_generation,
                current_gen,
                expected_content_generation = fence.content_generation,
                current_content_generation = published.content_generation,
                "rejecting stale git temporal publication"
            );
            return false;
        }

        self.git_temporal.store(Arc::new(index));
        self.swap_and_publish_retaining_content((*live).clone());
        true
    }

    /// Set the empty-index reason on the live LiveIndex. Used by the startup
    /// LocalEmpty branch so `health` can surface why the index is empty.
    pub fn set_local_empty_reason(&self, reason: Option<String>) {
        let _wg = self.write_mutex.lock();
        let mut live = (*self.live.load_full()).clone();
        live.local_empty_reason = Arc::new(parking_lot::RwLock::new(reason));
        self.swap_and_publish_retaining_content(live);
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

/// SF-STRESS-010: per-category cap on the quarantine path lists published across
/// the daemon-proxy boundary. Raised from the previous hard `10` so the full
/// list is retrievable via the `health` `quarantine_limit` paging parameter in
/// daemon mode (production default). The `*_count` fields stay uncapped, so the
/// rendered `total`/`omitted` remain truthful even if a list reaches this cap.
/// Kept in sync with [`crate::protocol::format::PARSE_QUARANTINE_MAX_LIMIT`].
const PUBLISHED_QUARANTINE_LIST_CAP: usize = 1000;

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
            expected_generated_partial_parse_count: stats.expected_generated_partial_parse_count,
            expected_test_fixture_partial_parse_count: stats
                .expected_test_fixture_partial_parse_count,
            expected_template_dsl_partial_parse_count: stats
                .expected_template_dsl_partial_parse_count,
            expected_framework_partial_parse_count: stats.expected_framework_partial_parse_count,
            expected_language_partial_parse_count: stats.expected_language_partial_parse_count,
            failed_count: stats.failed_count,
            // SF-STRESS-010: publish up to PUBLISHED_QUARANTINE_LIST_CAP entries
            // per category (was 10) so the daemon-proxy boundary no longer
            // destroys the lists before the formatter can page them. The `*_count`
            // fields above stay UNCAPPED (true totals), so showing/omitted stay
            // honest even when a list hits the publish cap.
            partial_parse_files: stats
                .partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            unexpected_partial_parse_files: stats
                .unexpected_partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            expected_vendor_partial_parse_files: stats
                .expected_vendor_partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            expected_generated_partial_parse_files: stats
                .expected_generated_partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            expected_test_fixture_partial_parse_files: stats
                .expected_test_fixture_partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            expected_template_dsl_partial_parse_files: stats
                .expected_template_dsl_partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            expected_framework_partial_parse_files: stats
                .expected_framework_partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            expected_language_partial_parse_files: stats
                .expected_language_partial_parse_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            failed_files: stats
                .failed_files
                .into_iter()
                .take(PUBLISHED_QUARANTINE_LIST_CAP)
                .collect(),
            symbol_count: stats.symbol_count,
            loaded_at_system: index.loaded_at_system,
            load_duration: stats.load_duration,
            load_source: index.load_source,
            snapshot_verify_state: index.snapshot_verify_state.clone(),
            is_empty: index.is_empty,
            tier_counts: stats.tier_counts,
            local_empty_reason: stats.local_empty_reason,
            untracked_indexed: stats.untracked_indexed,
            indexed_root: index.indexed_root.clone(),
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
    pub scout_plan: Arc<discovery::ScoutPlan>,
    pub manifest_entries: Vec<CatalogEntry>,
    pub cb_state: CircuitBreakerState,
    pub load_duration: Duration,
    pub gitignore: Option<ignore::gitignore::Gitignore>,
    pub derived: DerivedIndices,
    pub coupling_store: Option<Arc<super::coupling::CouplingStore>>,
    /// Normalized root this reload was built from. Carried through so
    /// `apply_reload_data` can record it on the live index (root-mismatch
    /// invalidation in `ensure_local_index`).
    pub indexed_root: PathBuf,
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

/// Outcome of running admission and parsing for one discovered file.
#[allow(clippy::large_enum_variant)]
enum AdmissionOutcome {
    Parsed(String, IndexedFile, crate::domain::IndexTargets),
    Terminal {
        path: String,
        disposition: crate::domain::FileDisposition,
    },
}

pub(super) fn scouted_catalog_path(path: &crate::domain::CatalogPath) -> &str {
    path.normalized_utf8
        .as_deref()
        .unwrap_or(path.public_id.as_str())
}

fn scouted_entry_path(entry: &crate::domain::ScoutedEntry) -> &str {
    scouted_catalog_path(&entry.path)
}

fn manifest_requires_degraded_coverage(live: &LiveIndex) -> bool {
    live.manifest_entries.iter().any(|entry| {
        matches!(
            entry.disposition,
            FileDisposition::Unreadable { .. }
                | FileDisposition::UnstableDuringRead
                | FileDisposition::AbortedCircuitBreaker
        )
    })
}

fn catalog_entry_from_scout(
    scouted: &crate::domain::ScoutedEntry,
    disposition: FileDisposition,
    content_hash: Option<String>,
) -> CatalogEntry {
    CatalogEntry {
        path: scouted.path.clone(),
        size: scouted.stamp.size,
        language: scouted.language.clone(),
        classification: scouted.classification,
        disposition,
        content_hash,
    }
}

fn manifest_entries_from_scout(
    scout_plan: &discovery::ScoutPlan,
    terminal_dispositions: Vec<(String, FileDisposition)>,
    files: &HashMap<String, Arc<IndexedFile>>,
) -> anyhow::Result<Vec<CatalogEntry>> {
    let mut dispositions: HashMap<String, FileDisposition> =
        terminal_dispositions.into_iter().collect();
    let mut entries = Vec::with_capacity(scout_plan.entries.len());

    for scouted in &scout_plan.entries {
        let path = scouted_entry_path(scouted);
        let disposition = dispositions.remove(path).ok_or_else(|| {
            anyhow::anyhow!("manifest disposition missing for scouted path {path}")
        })?;
        let content_hash = match &disposition {
            FileDisposition::Indexed { .. } => Some(
                files
                    .get(path)
                    .ok_or_else(|| anyhow::anyhow!("indexed manifest path missing content {path}"))?
                    .content_hash
                    .clone(),
            ),
            _ => None,
        };
        entries.push(catalog_entry_from_scout(scouted, disposition, content_hash));
    }

    if !dispositions.is_empty() {
        let mut unexpected: Vec<_> = dispositions.into_keys().collect();
        unexpected.sort();
        anyhow::bail!(
            "manifest dispositions without scout entries: {}",
            unexpected.join(", ")
        );
    }
    entries.sort_by_cached_key(|entry| {
        let path = entry
            .path
            .normalized_utf8
            .as_deref()
            .unwrap_or(entry.path.public_id.as_str());
        (path.to_lowercase(), path.to_string())
    });
    Ok(entries)
}

pub(super) fn compatibility_admission_decision(entry: &CatalogEntry) -> Option<AdmissionDecision> {
    let (tier, reason) = match &entry.disposition {
        FileDisposition::Indexed { .. } => return None,
        FileDisposition::MetadataOnly { reason } => (
            AdmissionTier::MetadataOnly,
            match reason {
                MetadataOnlyReason::Lockfile => SkipReason::DependencyLockfile,
                MetadataOnlyReason::Binary => SkipReason::BinaryContent,
                MetadataOnlyReason::OversizedData => SkipReason::SizeThreshold,
                MetadataOnlyReason::GeneratedOrVendor => {
                    let path = scouted_catalog_path(&entry.path);
                    let admission =
                        discovery::classify_admission(Path::new(path), entry.size, None);
                    if admission.reason == Some(SkipReason::DenylistedExtension) {
                        SkipReason::DenylistedExtension
                    } else if entry.classification.is_generated
                        || entry.classification.is_vendor
                        || Path::new(path).parent().is_some_and(|parent| {
                            parent.components().any(|component| {
                                discovery::is_generated_output_dir_name(
                                    &component.as_os_str().to_string_lossy(),
                                )
                            })
                        })
                    {
                        SkipReason::GeneratedOutput
                    } else {
                        // The canonical manifest deliberately collapses legacy
                        // generated/vendor/untracked admission reasons. A path
                        // with none of the generated/vendor signals can only be
                        // the opt-in untracked demotion.
                        SkipReason::Untracked
                    }
                }
                MetadataOnlyReason::SensitivePath { .. }
                | MetadataOnlyReason::SensitiveContent { .. }
                | MetadataOnlyReason::LfsPointer { .. }
                | MetadataOnlyReason::PlatformPathCollision
                | MetadataOnlyReason::UnsupportedPathEncoding
                | MetadataOnlyReason::PathMetadataTooLarge
                | MetadataOnlyReason::UnsupportedTextEncoding => SkipReason::UnsupportedLanguage,
            },
        ),
        FileDisposition::HardSkip { reason } => (
            AdmissionTier::HardSkip,
            match reason {
                HardSkipReason::PerFileCeiling => SkipReason::SizeCeiling,
                HardSkipReason::ArtifactType => SkipReason::DenylistedExtension,
            },
        ),
        FileDisposition::Unreadable { .. }
        | FileDisposition::UnstableDuringRead
        | FileDisposition::AbortedCircuitBreaker => {
            (AdmissionTier::MetadataOnly, SkipReason::UnsupportedLanguage)
        }
    };
    Some(AdmissionDecision::skip(tier, reason))
}

fn disposition_from_admission(decision: AdmissionDecision) -> crate::domain::FileDisposition {
    match decision.tier {
        AdmissionTier::HardSkip => crate::domain::FileDisposition::HardSkip {
            reason: if matches!(decision.reason, Some(SkipReason::SizeCeiling)) {
                HardSkipReason::PerFileCeiling
            } else {
                HardSkipReason::ArtifactType
            },
        },
        AdmissionTier::MetadataOnly => crate::domain::FileDisposition::MetadataOnly {
            reason: match decision.reason {
                Some(SkipReason::DependencyLockfile) => MetadataOnlyReason::Lockfile,
                Some(SkipReason::BinaryContent) => MetadataOnlyReason::Binary,
                Some(SkipReason::SizeThreshold) => MetadataOnlyReason::OversizedData,
                Some(SkipReason::DenylistedExtension)
                | Some(SkipReason::GeneratedOutput)
                | Some(SkipReason::Untracked) => MetadataOnlyReason::GeneratedOrVendor,
                Some(SkipReason::UnsupportedLanguage) | Some(SkipReason::SizeCeiling) | None => {
                    MetadataOnlyReason::UnsupportedTextEncoding
                }
            },
        },
        AdmissionTier::Normal => crate::domain::FileDisposition::MetadataOnly {
            reason: MetadataOnlyReason::UnsupportedTextEncoding,
        },
    }
}

fn terminal_admission_outcome(
    entry: &discovery::DiscoveredEntry,
    _decision: AdmissionDecision,
    disposition: crate::domain::FileDisposition,
) -> AdmissionOutcome {
    AdmissionOutcome::Terminal {
        path: entry.relative_path.clone(),
        disposition,
    }
}

/// Parsed file map plus the skip records and circuit-breaker state produced by a
/// single admission + parse pass. Returned by [`admit_and_parse_entries`] so the
/// `load` and reload callers share one pipeline and differ only in how they wrap
/// the result.
pub(crate) struct AdmitParseResult {
    pub files: HashMap<String, Arc<IndexedFile>>,
    pub terminal_dispositions: Vec<(String, crate::domain::FileDisposition)>,
    pub coverage: crate::domain::CoverageStatus,
    pub cb_state: CircuitBreakerState,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CircuitBreakerScope {
    source: PathBuf,
    lane: crate::domain::IndexTargets,
    stage: String,
}

impl CircuitBreakerScope {
    fn new(source: PathBuf, lane: crate::domain::IndexTargets, stage: impl Into<String>) -> Self {
        Self {
            source,
            lane,
            stage: stage.into(),
        }
    }
}

struct ParseFoldResult {
    files: HashMap<String, Arc<IndexedFile>>,
    dispositions: Vec<(String, crate::domain::FileDisposition)>,
    coverage: crate::domain::CoverageStatus,
    cb_state: CircuitBreakerState,
}

fn fold_parse_results_for_scope(
    mut parse_results: Vec<(String, IndexedFile)>,
    cb_state: CircuitBreakerState,
    scope: CircuitBreakerScope,
) -> ParseFoldResult {
    parse_results.sort_by(|left, right| left.0.cmp(&right.0));

    let mut files = HashMap::with_capacity(parse_results.len());
    let mut dispositions = Vec::with_capacity(parse_results.len());
    let mut remaining = parse_results.into_iter();
    let mut cb_tripped = false;

    while let Some((path, indexed_file)) = remaining.next() {
        let parse_status = match &indexed_file.parse_status {
            ParseStatus::Parsed => crate::domain::index::ParseStatus::Parsed,
            ParseStatus::PartialParse { .. } => crate::domain::index::ParseStatus::PartialParse,
            ParseStatus::Failed { error } => {
                cb_state.record_failure(&path, error);
                crate::domain::index::ParseStatus::Failed
            }
        };
        if !matches!(indexed_file.parse_status, ParseStatus::Failed { .. }) {
            cb_state.record_success();
        }

        dispositions.push((
            path.clone(),
            crate::domain::FileDisposition::Indexed {
                targets: scope.lane,
                parse_status,
            },
        ));
        files.insert(path, Arc::new(indexed_file));

        if cb_state.should_abort() {
            error!("{}", cb_state.summary());
            cb_tripped = true;
            dispositions
                .extend(remaining.map(|(path, _)| {
                    (path, crate::domain::FileDisposition::AbortedCircuitBreaker)
                }));
            break;
        }
    }

    if cb_tripped {
        cb_state.tripped.store(true, Ordering::Relaxed);
    }

    ParseFoldResult {
        files,
        dispositions,
        coverage: if cb_tripped {
            crate::domain::CoverageStatus::Degraded
        } else {
            crate::domain::CoverageStatus::Complete
        },
        cb_state,
    }
}

#[derive(Clone)]
struct PlannedIngest {
    stamp: crate::domain::FileStamp,
    targets: crate::domain::IndexTargets,
}

struct LegacyExecutionProjection {
    entries: Vec<discovery::DiscoveredEntry>,
    ingest_plans: HashMap<String, PlannedIngest>,
    terminal_dispositions: Vec<(String, crate::domain::FileDisposition)>,
}

/// Project an authoritative metadata-first scout into the legacy execution
/// pipeline while retaining exactly one outcome slot for every scout entry.
/// Entries executable by the current code parser carry their immutable stamp
/// and targets into stable-read execution; every other scout decision is
/// represented immediately as a terminal manifest disposition.
fn project_scout_for_legacy_execution(plan: &discovery::ScoutPlan) -> LegacyExecutionProjection {
    let mut executable = Vec::new();
    let mut ingest_plans = HashMap::new();
    let mut terminal_dispositions = Vec::new();

    for entry in &plan.entries {
        let path = entry
            .path
            .normalized_utf8
            .clone()
            .unwrap_or_else(|| entry.path.public_id.clone());
        match &entry.decision {
            ScoutDecision::Ingest { targets } => {
                let Some(relative_path) = entry.path.normalized_utf8.clone() else {
                    terminal_dispositions.push((
                        path,
                        crate::domain::FileDisposition::MetadataOnly {
                            reason: MetadataOnlyReason::UnsupportedPathEncoding,
                        },
                    ));
                    continue;
                };
                let Some(absolute_path) = entry.absolute_path.clone() else {
                    terminal_dispositions.push((
                        path,
                        crate::domain::FileDisposition::Unreadable {
                            stage: crate::domain::AccessStage::Metadata,
                            kind: crate::domain::AccessErrorKind::Other,
                        },
                    ));
                    continue;
                };
                ingest_plans.insert(
                    relative_path.clone(),
                    PlannedIngest {
                        stamp: entry.stamp.clone(),
                        targets: *targets,
                    },
                );
                executable.push(discovery::DiscoveredEntry {
                    relative_os_path: PathBuf::from(&relative_path),
                    relative_path,
                    absolute_path,
                    file_size: entry.stamp.size,
                    language: entry.language.clone(),
                    classification: entry.classification,
                });
            }
            ScoutDecision::MetadataOnly { reason } => {
                terminal_dispositions.push((
                    path,
                    crate::domain::FileDisposition::MetadataOnly {
                        reason: reason.clone(),
                    },
                ));
            }
            ScoutDecision::HardSkip { reason } => {
                terminal_dispositions.push((
                    path,
                    crate::domain::FileDisposition::HardSkip { reason: *reason },
                ));
            }
            ScoutDecision::Unavailable { stage, kind } => {
                terminal_dispositions.push((
                    path,
                    crate::domain::FileDisposition::Unreadable {
                        stage: *stage,
                        kind: *kind,
                    },
                ));
            }
        }
    }

    terminal_dispositions.sort_by(|left, right| left.0.cmp(&right.0));
    LegacyExecutionProjection {
        entries: executable,
        ingest_plans,
        terminal_dispositions,
    }
}

/// Run the shared admission gate and parser over a set of discovered entries.
///
/// This is the SINGLE pipeline used by both [`LiveIndex::load`] (initial load)
/// and [`LiveIndex::build_reload_data`] (full reindex / `index_folder`), so both
/// paths classify every discovered file into Tier 1/2/3, retain one canonical
/// manifest disposition per path, and respect the in-flight byte governor
/// identically.
///
/// Pipeline shape (same as the original `load`):
///   * Phase 1 — size + basename classification (no I/O beyond the walk).
///   * Phase 2 — unknown-language files are read (binary-sniffed) and recorded as
///     metadata-only skips; they are never parsed.
///   * Phase 3 — recognized-language files are read (binary-sniffed); a content
///     sniff can still reclassify to Tier 2. SF-009 untracked demotion applies
///     here when `exclude_untracked_set` is `Some`.
///   * Parse — admitted candidates are parsed in parallel.
///   * Circuit breaker — files are folded into a map sequentially under a fresh
///     `CircuitBreakerState`, aborting if the failure ratio trips.
///
/// `exclude_untracked_set` carries the SF-009 opt-in semantics: `None` (the
/// default and the fail-open result for non-git trees) demotes nothing; `Some`
/// demotes recognized-extension files that are not git-tracked to Tier 2.
///
/// `generated_output_demotions` carries the F5 policy: discovered file paths
/// under UNTRACKED generated-output dirs (see
/// `discovery::untracked_generated_output_demotions`) are demoted to Tier 2
/// without reading their content. An empty set demotes nothing.
fn admit_and_parse_entries(
    entries: &[crate::discovery::DiscoveredEntry],
    ingest_plans: &HashMap<String, PlannedIngest>,
    exclude_untracked_set: &Option<std::collections::HashSet<String>>,
    generated_output_demotions: &std::collections::HashSet<String>,
    source_scope: PathBuf,
) -> AdmitParseResult {
    use crate::discovery::classify_admission;

    // Transient bytes and staged resident bytes are governed independently.
    // The immutable scout plan fixes both the per-entry stamp and the maximum
    // staged content charge before any worker allocates.
    let inflight_budget = Arc::new(InflightByteBudget::from_env());
    let staged_ceiling = ingest_plans.values().fold(0_u64, |total, planned| {
        total.saturating_add(planned.stamp.size)
    });
    let staged_accounting = Arc::new(StagedContentAccounting::new(staged_ceiling));

    let outcomes: Vec<AdmissionOutcome> = indexing_thread_pool().install(|| {
        entries
            .par_iter()
            .map(|entry| {
                let Some(planned) = ingest_plans.get(&entry.relative_path) else {
                    return terminal_admission_outcome(
                        entry,
                        AdmissionDecision::skip(
                            AdmissionTier::MetadataOnly,
                            SkipReason::UnsupportedLanguage,
                        ),
                        crate::domain::FileDisposition::Unreadable {
                            stage: crate::domain::AccessStage::Metadata,
                            kind: crate::domain::AccessErrorKind::Other,
                        },
                    );
                };
                // Re-run metadata-only policy before any permit/allocation. The
                // immutable scout remains authoritative; this is a defense-in-
                // depth parity check for policy changes between stages.
                let decision_pre = classify_admission(&entry.absolute_path, entry.file_size, None);
                if !matches!(decision_pre.tier, AdmissionTier::Normal) {
                    let disposition = disposition_from_admission(decision_pre);
                    return terminal_admission_outcome(entry, decision_pre, disposition);
                }

                // Generated and untracked demotions are metadata decisions and
                // therefore happen before stable-read I/O.
                if generated_output_demotions.contains(&entry.relative_path) {
                    let decision = AdmissionDecision::skip(
                        AdmissionTier::MetadataOnly,
                        SkipReason::GeneratedOutput,
                    );
                    let disposition = disposition_from_admission(decision);
                    return terminal_admission_outcome(entry, decision, disposition);
                }
                if let Some(tracked) = exclude_untracked_set.as_ref()
                    && !tracked.contains(&entry.relative_path)
                {
                    let decision =
                        AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::Untracked);
                    let disposition = disposition_from_admission(decision);
                    return terminal_admission_outcome(entry, decision, disposition);
                }

                // A request larger than the total in-flight budget is terminal
                // before permit acquisition or allocation (FR-005/C-R07).
                if planned.stamp.size > inflight_budget.total {
                    let decision =
                        AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling);
                    return terminal_admission_outcome(
                        entry,
                        decision,
                        crate::domain::FileDisposition::HardSkip {
                            reason: HardSkipReason::PerFileCeiling,
                        },
                    );
                }

                let permit = Some(inflight_budget.acquire(planned.stamp.size));
                let stable = stable_read_with_retries(
                    &entry.absolute_path,
                    &planned.stamp,
                    StableReadLimits {
                        per_file_bytes: crate::domain::index::HARD_SKIP_BYTES,
                        inflight_bytes: inflight_budget.total,
                    },
                    &FilesystemStableReadAccess,
                );
                let (bytes, accepted_hash) = match stable {
                    StableReadOutcome::Accepted { bytes, hash } => (bytes, hash),
                    StableReadOutcome::HardSkip { reason } => {
                        let decision = AdmissionDecision::skip(
                            AdmissionTier::HardSkip,
                            SkipReason::SizeCeiling,
                        );
                        return terminal_admission_outcome(
                            entry,
                            decision,
                            crate::domain::FileDisposition::HardSkip { reason },
                        );
                    }
                    StableReadOutcome::Unreadable { stage, kind } => {
                        return terminal_admission_outcome(
                            entry,
                            AdmissionDecision::skip(
                                AdmissionTier::MetadataOnly,
                                SkipReason::UnsupportedLanguage,
                            ),
                            crate::domain::FileDisposition::Unreadable { stage, kind },
                        );
                    }
                    StableReadOutcome::UnstableDuringRead => {
                        return terminal_admission_outcome(
                            entry,
                            AdmissionDecision::skip(
                                AdmissionTier::MetadataOnly,
                                SkipReason::UnsupportedLanguage,
                            ),
                            crate::domain::FileDisposition::UnstableDuringRead,
                        );
                    }
                };

                let decision_post =
                    classify_admission(&entry.absolute_path, entry.file_size, Some(&bytes));
                if !matches!(decision_post.tier, AdmissionTier::Normal) {
                    let disposition = disposition_from_admission(decision_post);
                    return terminal_admission_outcome(entry, decision_post, disposition);
                }

                // Stable bytes cross one content-policy boundary before parsing,
                // hashing, or publication. A metadata-only result consumes the
                // owned buffer here, so positive or indeterminate detector bytes
                // cannot reach any resident, snapshot, search, or analytics lane.
                if let crate::knowledge::StableContentAdmission::MetadataOnly(reason) =
                    crate::knowledge::classify_stable_content(
                        &entry.relative_path,
                        planned.targets,
                        &bytes,
                    )
                {
                    return terminal_admission_outcome(
                        entry,
                        AdmissionDecision::skip(
                            AdmissionTier::MetadataOnly,
                            SkipReason::UnsupportedLanguage,
                        ),
                        crate::domain::FileDisposition::MetadataOnly { reason },
                    );
                }
                let language = entry.language.clone().unwrap_or(LanguageId::Text);

                let mtime_secs = planned
                    .stamp
                    .modified_hint
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0);
                let relative_path = entry.relative_path.clone();
                let classification =
                    FileClassification::for_indexed_path(&relative_path, planned.targets);
                let result = parsing::process_file_with_classification(
                    &relative_path,
                    &bytes,
                    language,
                    classification,
                );
                let indexed = IndexedFile::from_parse_result(result, bytes).with_mtime(mtime_secs);
                debug_assert_eq!(crate::hash::digest(&indexed.content), accepted_hash);
                let resident_bytes = u64::try_from(indexed.content.len()).unwrap_or(u64::MAX);
                if !staged_accounting.handoff(resident_bytes, permit) {
                    let decision =
                        AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling);
                    return terminal_admission_outcome(
                        entry,
                        decision,
                        crate::domain::FileDisposition::HardSkip {
                            reason: HardSkipReason::PerFileCeiling,
                        },
                    );
                }
                AdmissionOutcome::Parsed(relative_path, indexed, planned.targets)
            })
            .collect()
    });

    let mut code_results: Vec<(String, IndexedFile)> = Vec::new();
    let mut knowledge_results: Vec<(String, IndexedFile)> = Vec::new();
    let mut combined_results: Vec<(String, IndexedFile)> = Vec::new();
    let mut terminal_dispositions = Vec::new();
    let mut execution_degraded = false;

    for outcome in outcomes {
        match outcome {
            AdmissionOutcome::Parsed(path, indexed, targets) => match targets {
                crate::domain::IndexTargets::Code => code_results.push((path, indexed)),
                crate::domain::IndexTargets::Knowledge => {
                    knowledge_results.push((path, indexed));
                }
                crate::domain::IndexTargets::CodeAndKnowledge => {
                    combined_results.push((path, indexed));
                }
            },
            AdmissionOutcome::Terminal { path, disposition } => {
                execution_degraded |= matches!(
                    disposition,
                    crate::domain::FileDisposition::Unreadable { .. }
                        | crate::domain::FileDisposition::UnstableDuringRead
                );
                terminal_dispositions.push((path, disposition));
            }
        }
    }

    let staged_count = code_results.len() + knowledge_results.len() + combined_results.len();
    info!(
        "admission + parse: {} staged, {} terminal",
        staged_count,
        terminal_dispositions.len()
    );

    let mut files = HashMap::with_capacity(staged_count);
    let mut breaker_coverage = crate::domain::CoverageStatus::Complete;
    let mut cb_state = None;

    for (lane, parse_results) in [
        (crate::domain::IndexTargets::Code, code_results),
        (crate::domain::IndexTargets::Knowledge, knowledge_results),
        (
            crate::domain::IndexTargets::CodeAndKnowledge,
            combined_results,
        ),
    ] {
        if parse_results.is_empty() {
            continue;
        }
        let ParseFoldResult {
            files: lane_files,
            dispositions,
            coverage,
            cb_state: lane_cb_state,
        } = fold_parse_results_for_scope(
            parse_results,
            CircuitBreakerState::from_env(),
            CircuitBreakerScope::new(source_scope.clone(), lane, "parse"),
        );
        files.extend(lane_files);
        terminal_dispositions.extend(dispositions);
        if matches!(coverage, crate::domain::CoverageStatus::Degraded) {
            breaker_coverage = crate::domain::CoverageStatus::Degraded;
        }

        // The legacy LiveIndex health field can represent only one breaker.
        // Preserve the first tripped scope deterministically; manifest coverage
        // retains the complete multi-lane result for reconciliation.
        if cb_state
            .as_ref()
            .is_none_or(|current: &CircuitBreakerState| {
                !current.is_tripped() && lane_cb_state.is_tripped()
            })
        {
            cb_state = Some(lane_cb_state);
        }
    }

    terminal_dispositions.sort_by(|left, right| left.0.cmp(&right.0));
    let coverage = if execution_degraded
        || matches!(breaker_coverage, crate::domain::CoverageStatus::Degraded)
    {
        crate::domain::CoverageStatus::Degraded
    } else {
        crate::domain::CoverageStatus::Complete
    };

    AdmitParseResult {
        files,
        terminal_dispositions,
        coverage,
        cb_state: cb_state.unwrap_or_else(CircuitBreakerState::from_env),
    }
}

impl LiveIndex {
    /// Load all source files under `root` into memory in parallel (Rayon), parse them,
    /// and return a `SharedIndex`.
    ///
    /// This function is **synchronous** — it must complete before the async tokio runtime
    /// needs the index. Rayon handles internal parallelism.
    pub fn load(root: &Path) -> anyhow::Result<SharedIndex> {
        Self::load_with_project_state(root, None, discovery::SourceExclusions::default())
    }

    pub fn load_for_state_placement(
        root: &Path,
        state_placement: &StatePlacement,
    ) -> anyhow::Result<SharedIndex> {
        let shared = Self::load_with_project_state(
            root,
            state_placement.directory(),
            discovery::SourceExclusions::for_state_placement(root, state_placement),
        )?;
        shared
            .project_state_dir
            .store(state_placement.directory().cloned().map(Arc::new));
        Ok(shared)
    }

    fn load_with_project_state(
        root: &Path,
        project_state_dir: Option<&ProjectStateDir>,
        source_exclusions: discovery::SourceExclusions,
    ) -> anyhow::Result<SharedIndex> {
        let start = Instant::now();

        info!("LiveIndex::load starting at {:?}", root);

        // 1. Build the authoritative metadata-first catalog. Every later content
        //    action is projected from this immutable plan; load never performs a
        //    second independent filesystem walk.
        let mut scout_plan = discovery::scout_repository_with_exclusions(root, &source_exclusions)?;
        let projection = project_scout_for_legacy_execution(&scout_plan);
        info!(
            "scouted {} catalog entries ({} executable by the legacy index)",
            scout_plan.entries.len(),
            projection.entries.len(),
        );

        // SF-009 opt-in: when `SYMFORGE_EXCLUDE_UNTRACKED` is enabled, compute
        // the git-tracked path set so recognized-extension files that are not
        // under version control can be demoted to Tier-2 below. `None` (the
        // default, and the fail-open result for non-git trees) means "demote
        // nothing", so admission defaults are unchanged. Files reaching the
        // admission gate are already non-gitignored (the `ignore`-crate walk in
        // `discover_all_files` prunes gitignored paths), so an untracked check
        // alone is sufficient here.
        let exclude_untracked_set = discovery::tracked_path_set_for_exclusion(root);

        // F5: demote files under untracked generated-output dirs (dist/build/
        // out/cache/*-out/… with no tracked file beneath) to Tier-2. Empty for
        // non-git trees and under `SYMFORGE_INDEX_GENERATED_OUTPUT=1`.
        let generated_output_demotions =
            discovery::untracked_generated_output_demotions(root, &projection.entries);
        let LegacyExecutionProjection {
            entries: all_entries,
            ingest_plans,
            terminal_dispositions: mut scout_terminal_dispositions,
        } = projection;

        // 2. Run the shared admission + parse pipeline. This classifies every
        //    discovered file into a terminal manifest disposition, reads admitted files under the in-flight byte
        //    governor, parses them in parallel, and applies the circuit breaker.
        //    The exact same pipeline backs `build_reload_data` (the reload /
        //    `index_folder` path), so both surfaces report identical tiering.
        let AdmitParseResult {
            files,
            mut terminal_dispositions,
            coverage,
            cb_state,
        } = admit_and_parse_entries(
            &all_entries,
            &ingest_plans,
            &exclude_untracked_set,
            &generated_output_demotions,
            normalize_root(root),
        );
        if matches!(coverage, crate::domain::CoverageStatus::Degraded) {
            scout_plan.coverage = crate::domain::CoverageStatus::Degraded;
        }
        terminal_dispositions.append(&mut scout_terminal_dispositions);
        terminal_dispositions.sort_by(|left, right| left.0.cmp(&right.0));
        debug_assert_eq!(terminal_dispositions.len(), scout_plan.entries.len());
        let manifest_entries =
            manifest_entries_from_scout(&scout_plan, terminal_dispositions, &files)?;

        let load_duration = start.elapsed();
        info!(
            "LiveIndex loaded: {} files, {} symbols, {} manifest entries, {:?}",
            files.len(),
            files.values().map(|f| f.symbols.len()).sum::<usize>(),
            manifest_entries.len(),
            load_duration
        );

        let trigram_index = super::trigram::TrigramIndex::build_from_files(&files);
        let gitignore = discovery::load_gitignore(root);
        let coupling_store = project_state_dir
            .and_then(|state_dir| super::coupling::init_coupling_store(root, state_dir));

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
            manifest_entries,
            coupling_store,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            // Record the normalized root this fresh index was built from so a
            // later project switch invalidates it (root-mismatch reload).
            indexed_root: Some(normalize_root(root)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        // Hook registration must be unconditional so a flag flipped after
        // boot still captures edits. The DB-touching reset-policy work is
        // deferred to the first commitment-tool bump (lazy via
        // `cached_store_for`) per ADR 0011 — discovery-only sessions leave
        // no frecency footprint.
        crate::live_index::frecency::ensure_bump_hook_registered();

        Ok(SharedIndexHandle::shared_with_scout_plan(
            index,
            scout_plan,
            source_exclusions,
        ))
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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            // An empty bootstrap index has no root: any non-empty target root
            // is therefore a mismatch, which is the desired behaviour (the next
            // local fallback reloads from the current root).
            indexed_root: None,
        }
    }

    /// Build a queryable `LiveIndex` directly from an in-memory set of already
    /// parsed files, with no filesystem walk or root.
    ///
    /// This backs Gate-L local-ref sources (`local_ref_scout::build_ref_source_index`):
    /// their `IndexedFile`s come from Git blob bytes routed through the shared
    /// parser/secret adapters, not from disk, so there is no source root to scout
    /// and no `.gitignore`/coupling store to load. Reverse and path indices are
    /// rebuilt so the resulting index answers symbol/reference/text queries exactly
    /// like a filesystem-loaded one.
    pub(crate) fn from_source_files(files: HashMap<String, Arc<IndexedFile>>) -> LiveIndex {
        let is_empty = files.is_empty();
        let trigram_index = super::trigram::TrigramIndex::build_from_files(&files);
        let mut index = LiveIndex {
            files,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
            gitignore: None,
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
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

    fn upsert_manifest_entry(&mut self, entry: CatalogEntry) {
        let path = scouted_catalog_path(&entry.path).to_string();
        self.manifest_entries
            .retain(|existing| scouted_catalog_path(&existing.path) != path);
        self.manifest_entries.push(entry);
        self.manifest_entries.sort_by_cached_key(|entry| {
            let path = entry
                .path
                .normalized_utf8
                .as_deref()
                .unwrap_or(entry.path.public_id.as_str());
            (path.to_lowercase(), path.to_string())
        });
    }

    fn remove_manifest_entry(&mut self, path: &str) -> bool {
        let before = self.manifest_entries.len();
        self.manifest_entries
            .retain(|entry| scouted_catalog_path(&entry.path) != path);
        self.manifest_entries.len() != before
    }

    /// Project the legacy Tier-2/3 response from canonical manifest entries.
    /// No compatibility state is retained after this call returns.
    pub fn compatibility_skipped_files(&self) -> Vec<SkippedFile> {
        self.manifest_entries
            .iter()
            .filter_map(|entry| {
                let decision = compatibility_admission_decision(entry)?;
                let path = scouted_catalog_path(&entry.path).to_string();
                let extension = Path::new(&path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(ToOwned::to_owned);
                Some(SkippedFile {
                    path,
                    size: entry.size,
                    extension,
                    decision,
                })
            })
            .collect()
    }

    /// Returns (tier1_count, tier2_count, tier3_count).
    /// All tiers are projected from canonical manifest dispositions. The
    /// file-map fallback exists only for legacy synthetic indices that have no
    /// manifest entries at all.
    pub fn tier_counts(&self) -> (usize, usize, usize) {
        let mut tier1 = 0;
        let mut tier2 = 0;
        let mut tier3 = 0;
        for entry in &self.manifest_entries {
            match entry.disposition {
                FileDisposition::Indexed { .. } => tier1 += 1,
                FileDisposition::HardSkip { .. } => tier3 += 1,
                FileDisposition::MetadataOnly { .. }
                | FileDisposition::Unreadable { .. }
                | FileDisposition::UnstableDuringRead
                | FileDisposition::AbortedCircuitBreaker => tier2 += 1,
            }
        }
        if self.manifest_entries.is_empty() {
            tier1 = self.files.len();
        }
        (tier1, tier2, tier3)
    }

    /// Build reload data without holding any lock. Performs all file I/O and
    /// parsing via Rayon. The returned `ReloadData` is applied under the write
    /// lock via `apply_reload_data` — reducing lock hold time from seconds to
    /// milliseconds.
    pub(crate) fn build_reload_data(root: &Path) -> anyhow::Result<ReloadData> {
        Self::build_reload_data_for_binding(root, None)
    }

    pub(crate) fn build_reload_data_for_binding(
        root: &Path,
        project_state_dir: Option<&ProjectStateDir>,
    ) -> anyhow::Result<ReloadData> {
        Self::build_reload_data_for_binding_with_exclusions(
            root,
            project_state_dir,
            &discovery::SourceExclusions::default(),
        )
    }

    pub(crate) fn build_reload_data_for_binding_with_exclusions(
        root: &Path,
        project_state_dir: Option<&ProjectStateDir>,
        source_exclusions: &discovery::SourceExclusions,
    ) -> anyhow::Result<ReloadData> {
        let start = Instant::now();

        info!("LiveIndex::build_reload_data starting at {:?}", root);

        if !root.exists() {
            anyhow::bail!(
                "discovery error: root path does not exist: {}",
                root.display()
            );
        }

        // 1. Build the same authoritative metadata-first catalog used by cold
        //    load. Reload performs no independent compatibility walk.
        let mut scout_plan = discovery::scout_repository_with_exclusions(root, source_exclusions)?;
        let projection = project_scout_for_legacy_execution(&scout_plan);
        info!(
            "scouted {} catalog entries ({} executable by the legacy index)",
            scout_plan.entries.len(),
            projection.entries.len(),
        );

        // SF-009 opt-in: compute the git-tracked set so untracked
        // recognized-extension files can be demoted out of Tier-1 below.
        // `None` (default + fail-open for non-git trees) means "keep
        // everything", so admission defaults are unchanged. With the unified
        // pipeline a demoted file now retains a Tier-2 manifest disposition (it
        // was silently dropped before), so compatibility health projections
        // agree across both discovery paths.
        let exclude_untracked_set = discovery::tracked_path_set_for_exclusion(root);

        // F5: same untracked generated-output demotion as `load`, so initial
        // load and reload report identical tiering.
        let generated_output_demotions =
            discovery::untracked_generated_output_demotions(root, &projection.entries);
        let LegacyExecutionProjection {
            entries: all_entries,
            ingest_plans,
            terminal_dispositions: mut scout_terminal_dispositions,
        } = projection;

        // 2. Run the shared admission + parse pipeline (identical to the one
        //    `LiveIndex::load` uses). This reads admitted files under the
        //    in-flight byte governor, parses in parallel, applies the circuit
        //    breaker, and records one terminal disposition per catalog entry.
        let AdmitParseResult {
            files: new_files,
            mut terminal_dispositions,
            coverage,
            cb_state: new_cb,
        } = admit_and_parse_entries(
            &all_entries,
            &ingest_plans,
            &exclude_untracked_set,
            &generated_output_demotions,
            normalize_root(root),
        );
        if matches!(coverage, crate::domain::CoverageStatus::Degraded) {
            scout_plan.coverage = crate::domain::CoverageStatus::Degraded;
        }
        terminal_dispositions.append(&mut scout_terminal_dispositions);
        terminal_dispositions.sort_by(|left, right| left.0.cmp(&right.0));
        debug_assert_eq!(terminal_dispositions.len(), scout_plan.entries.len());
        let manifest_entries =
            manifest_entries_from_scout(&scout_plan, terminal_dispositions, &new_files)?;

        let load_duration = start.elapsed();
        info!(
            "LiveIndex::build_reload_data done: {} files, {} symbols, {} manifest entries, {:?}",
            new_files.len(),
            new_files.values().map(|f| f.symbols.len()).sum::<usize>(),
            manifest_entries.len(),
            load_duration
        );

        // Pre-build all derived indices outside any lock.
        let derived = DerivedIndices::build_from_files(&new_files);

        Ok(ReloadData {
            files: new_files,
            scout_plan: Arc::new(scout_plan),
            manifest_entries,
            cb_state: new_cb,
            load_duration,
            gitignore: discovery::load_gitignore(root),
            derived,
            coupling_store: project_state_dir
                .and_then(|state_dir| super::coupling::init_coupling_store(root, state_dir)),
            // Record the normalized root so the reloaded index advertises which
            // project it now serves (root-mismatch invalidation).
            indexed_root: normalize_root(root),
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
        self.local_empty_reason.write().take();
        self.load_source = IndexLoadSource::FreshLoad;
        self.snapshot_verify_state = SnapshotVerifyState::NotNeeded;
        self.trigram_index = data.derived.trigram_index;
        self.reverse_index = data.derived.reverse_index;
        self.files_by_basename = data.derived.files_by_basename;
        self.files_by_dir_component = data.derived.files_by_dir_component;
        self.gitignore = data.gitignore;
        self.manifest_entries = data.manifest_entries;
        self.coupling_store = data.coupling_store;
        self.indexed_root = Some(data.indexed_root);
    }

    fn from_reload_data(data: ReloadData) -> Self {
        let mut live = Self::empty_live_index();
        live.apply_reload_data(data);
        live
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
    /// If the file already exists, its content and canonical manifest entry are
    /// replaced atomically. Existing target routing is preserved; callers that
    /// need to change routing must use the explicit admission publication path.
    pub fn update_file(&mut self, path: String, file: IndexedFile) {
        // Capture old reference names BEFORE replacing the file, so we can
        // clean up stale reverse index entries after the insert.
        let old_ref_names: Vec<String> = self
            .files
            .get(&path)
            .map(|f| f.references.iter().map(|r| r.name.clone()).collect())
            .unwrap_or_default();
        let had_existing = !old_ref_names.is_empty() || self.files.contains_key(&path);

        let (catalog_path, targets) = self
            .manifest_entries
            .iter()
            .find(|entry| scouted_catalog_path(&entry.path) == path)
            .map(|entry| {
                let targets = match entry.disposition {
                    FileDisposition::Indexed { targets, .. } => targets,
                    _ if file.language.is_code_language() => crate::domain::IndexTargets::Code,
                    _ => crate::domain::IndexTargets::Knowledge,
                };
                (entry.path.clone(), targets)
            })
            .unwrap_or_else(|| {
                let targets = if file.language.is_code_language() {
                    crate::domain::IndexTargets::Code
                } else {
                    crate::domain::IndexTargets::Knowledge
                };
                (
                    crate::domain::CatalogPath {
                        public_id: path.clone(),
                        normalized_utf8: Some(path.clone()),
                    },
                    targets,
                )
            });
        let parse_status = match &file.parse_status {
            ParseStatus::Parsed => crate::domain::index::ParseStatus::Parsed,
            ParseStatus::PartialParse { .. } => crate::domain::index::ParseStatus::PartialParse,
            ParseStatus::Failed { .. } => crate::domain::index::ParseStatus::Failed,
        };
        let manifest_entry = CatalogEntry {
            path: catalog_path,
            size: file.byte_len,
            language: Some(file.language.clone()),
            classification: file.classification,
            disposition: FileDisposition::Indexed {
                targets,
                parse_status,
            },
            content_hash: Some(file.content_hash.clone()),
        };

        // SAFETY: Insert the new file into the primary store FIRST.
        // This ensures the file is always present in `self.files` even if
        // auxiliary index updates panic (e.g., from concurrent access or
        // gitignore assertion failures). Auxiliary indices may become
        // temporarily stale, but the file won't vanish from the index.
        self.files.insert(path.clone(), Arc::new(file));
        self.upsert_manifest_entry(manifest_entry);

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
    /// Indexed bytes and the canonical manifest entry are cleared together. If
    /// neither lane contains the path, this is a no-op (no timestamp update).
    pub fn remove_file(&mut self, path: &str) {
        self.remove_reverse_index_for_path(path);
        let removed_file = self.files.remove(path).is_some();
        let removed_manifest = self.remove_manifest_entry(path);
        if removed_file {
            self.trigram_index.remove_file(path);
            self.remove_path_indices_for_path(path);
        }
        if removed_file || removed_manifest {
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

    fn runtime_canary() -> String {
        ["runtime", "-", "canary", "-", "segment"].concat()
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

    // ── Reload path admission tiering (index_folder) ──────────────────────
    //
    // Regression tests for the unified admission pipeline: `build_reload_data`
    // (the reload / `index_folder` path) must run the SAME admission gate as
    // `LiveIndex::load`, so Tier-2/3 files are recorded in `skipped_files` and
    // `tier_counts()` is correct after a reload — not the old structural
    // `N/0/0`.
    mod reload_admission_tiering {
        use super::*;
        use crate::domain::index::SkipReason;

        /// Build a fresh in-memory index by running the reload pipeline over
        /// `root`, mirroring exactly what `SharedIndexHandle::reload` does
        /// (`build_reload_data` then `apply_reload_data`).
        fn reload_index(root: &Path) -> LiveIndex {
            let data = LiveIndex::build_reload_data(root).expect("reload should succeed");
            let mut index = LiveIndex::empty_live_index();
            index.apply_reload_data(data);
            index
        }

        #[test]
        fn reload_demotes_lockfile_to_tier2_with_dependency_lockfile_reason() {
            let tmp = TempDir::new().unwrap();
            // One normal source file (Tier 1) and one dependency lockfile that
            // must be demoted to Tier 2 metadata-only.
            write_file(tmp.path(), "src/main.rs", "fn main() {}\n");
            write_file(
                tmp.path(),
                "package-lock.json",
                "{ \"name\": \"x\", \"lockfileVersion\": 3, \"packages\": {} }\n",
            );

            let index = reload_index(tmp.path());

            // The lockfile must NOT be a Tier-1 indexed file.
            assert!(
                !index.files.contains_key("package-lock.json"),
                "lockfile must not be parsed/indexed on reload; files = {:?}",
                index.files.keys().collect::<Vec<_>>()
            );
            assert!(
                index.files.contains_key("src/main.rs"),
                "normal source must still be indexed on reload"
            );

            // It MUST appear in skipped_files with the lockfile skip reason.
            let lockfile_skip = index
                .compatibility_skipped_files()
                .into_iter()
                .find(|sf| sf.path.replace('\\', "/") == "package-lock.json")
                .expect("lockfile must be recorded in skipped_files on reload");
            assert_eq!(
                lockfile_skip.tier(),
                AdmissionTier::MetadataOnly,
                "lockfile must be Tier-2 metadata-only"
            );
            assert_eq!(
                lockfile_skip.reason(),
                Some(SkipReason::DependencyLockfile),
                "lockfile skip reason must be DependencyLockfile"
            );

            // tier_counts() must report (1 indexed, 1 metadata-only, 0 hard-skip).
            assert_eq!(
                index.tier_counts(),
                (1, 1, 0),
                "reload tier counts must be (tier1=1, tier2=1, tier3=0)"
            );
        }

        #[test]
        fn reload_demotes_oversized_text_file_to_tier2_size_threshold() {
            let tmp = TempDir::new().unwrap();
            // A normal source file (Tier 1) plus a >1MB text file. The big file
            // has a recognized extension (.json), so this proves general
            // size-based admission — not just the lockfile special case — now
            // works on the reload path.
            write_file(tmp.path(), "src/lib.rs", "pub fn helper() -> i32 { 42 }\n");
            // METADATA_ONLY_BYTES is 1 MiB; 1.5 MiB clears it comfortably.
            let big = "x".repeat(1_500_000);
            write_file(tmp.path(), "data/big.json", &big);

            let index = reload_index(tmp.path());

            assert!(
                !index.files.contains_key("data/big.json"),
                "oversized file must not be parsed/indexed on reload"
            );
            assert!(
                index.files.contains_key("src/lib.rs"),
                "normal source must still be indexed on reload"
            );

            let big_skip = index
                .compatibility_skipped_files()
                .into_iter()
                .find(|sf| sf.path.replace('\\', "/") == "data/big.json")
                .expect("oversized file must be recorded in skipped_files on reload");
            assert_eq!(big_skip.tier(), AdmissionTier::MetadataOnly);
            assert_eq!(
                big_skip.reason(),
                Some(SkipReason::SizeThreshold),
                "oversized file skip reason must be SizeThreshold"
            );

            assert_eq!(
                index.tier_counts(),
                (1, 1, 0),
                "reload tier counts must be (tier1=1, tier2=1, tier3=0)"
            );
        }
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

        let project_state =
            crate::domain::ProjectStateDir::new(tmp.path().join(crate::paths::SYMFORGE_DIR_NAME));
        let placement = crate::domain::StatePlacement::ProjectLocal {
            directory: project_state.clone(),
        };
        let shared = LiveIndex::load_for_state_placement(tmp.path(), &placement).unwrap();
        let db_path = crate::live_index::coupling::lifecycle::coupling_db_path(&project_state);
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

        let project_state =
            crate::domain::ProjectStateDir::new(tmp.path().join(crate::paths::SYMFORGE_DIR_NAME));
        let placement = crate::domain::StatePlacement::ProjectLocal {
            directory: project_state,
        };
        let shared = LiveIndex::load_for_state_placement(tmp.path(), &placement).unwrap();
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

    // --- InflightByteBudget (Finding 2: peak-concurrent read footprint) ---

    static INFLIGHT_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    struct InflightEnvGuard {
        previous: Option<String>,
    }

    #[allow(unsafe_code)] // test-only env guard; callers hold INFLIGHT_ENV_LOCK.
    impl InflightEnvGuard {
        fn set(value: Option<&str>) -> Self {
            let previous = std::env::var(MAX_INFLIGHT_BYTES_ENV).ok();
            // SAFETY: callers hold INFLIGHT_ENV_LOCK; tests run single-threaded.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(MAX_INFLIGHT_BYTES_ENV, value),
                    None => std::env::remove_var(MAX_INFLIGHT_BYTES_ENV),
                }
            }
            Self { previous }
        }
    }

    #[allow(unsafe_code)] // test-only env guard restores the serialized flag.
    impl Drop for InflightEnvGuard {
        fn drop(&mut self) {
            // SAFETY: callers hold INFLIGHT_ENV_LOCK; tests run single-threaded.
            unsafe {
                match self.previous.as_deref() {
                    Some(value) => std::env::set_var(MAX_INFLIGHT_BYTES_ENV, value),
                    None => std::env::remove_var(MAX_INFLIGHT_BYTES_ENV),
                }
            }
        }
    }

    #[test]
    fn stable_read_refuses_over_ceiling_before_allocation() {
        struct PanicStableReadAccess;

        impl StableReadAccess for PanicStableReadAccess {
            fn first_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                panic!("over-ceiling input must be rejected before allocation/read")
            }

            fn second_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                panic!("over-ceiling input must be rejected before allocation/read")
            }
        }

        let scout_stamp = crate::domain::FileStamp {
            size: 2_048,
            created_hint: None,
            modified_hint: None,
            platform_id: None,
        };
        let outcome = stable_read_with_access(
            Path::new("oversized.rs"),
            &scout_stamp,
            StableReadLimits {
                per_file_bytes: 1_024,
                inflight_bytes: 4_096,
            },
            &PanicStableReadAccess,
        );

        assert!(matches!(
            outcome,
            StableReadOutcome::HardSkip {
                reason: HardSkipReason::PerFileCeiling
            }
        ));
    }

    #[test]
    fn read_larger_than_inflight_budget_is_terminal_hard_skip() {
        struct PanicOverInflightAccess;

        impl StableReadAccess for PanicOverInflightAccess {
            fn first_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                panic!("over-inflight input must be rejected before allocation/read")
            }

            fn second_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                panic!("over-inflight input must be rejected before allocation/read")
            }
        }

        let stamp = crate::domain::FileStamp {
            size: 2_048,
            created_hint: None,
            modified_hint: None,
            platform_id: None,
        };
        let outcome = stable_read_with_access(
            Path::new("over-inflight.rs"),
            &stamp,
            StableReadLimits {
                per_file_bytes: 4_096,
                inflight_bytes: 1_024,
            },
            &PanicOverInflightAccess,
        );

        assert!(matches!(
            outcome,
            StableReadOutcome::HardSkip {
                reason: HardSkipReason::PerFileCeiling
            }
        ));
    }

    #[test]
    fn stable_read_rejects_changed_manifest_stamp() {
        use std::cell::Cell;

        struct ChangedStampAccess<'a> {
            first_calls: &'a Cell<usize>,
            second_calls: &'a Cell<usize>,
            first: StableReadPass,
        }

        impl StableReadAccess for ChangedStampAccess<'_> {
            fn first_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                self.first_calls.set(self.first_calls.get() + 1);
                Ok(self.first.clone())
            }

            fn second_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                self.second_calls.set(self.second_calls.get() + 1);
                panic!("changed first-pass stamp must reject before the second read")
            }
        }

        let scout_stamp = crate::domain::FileStamp {
            size: 4,
            created_hint: None,
            modified_hint: Some(std::time::UNIX_EPOCH),
            platform_id: None,
        };
        let changed_stamp = crate::domain::FileStamp {
            modified_hint: Some(std::time::UNIX_EPOCH + Duration::from_secs(1)),
            ..scout_stamp.clone()
        };
        let first_calls = Cell::new(0);
        let second_calls = Cell::new(0);
        let access = ChangedStampAccess {
            first_calls: &first_calls,
            second_calls: &second_calls,
            first: StableReadPass {
                bytes: Some(b"same".to_vec()),
                length: 4,
                hash: crate::hash::digest(b"same"),
                handle_before: changed_stamp.clone(),
                handle_after: changed_stamp.clone(),
                path_after: changed_stamp,
            },
        };

        let outcome = stable_read_with_access(
            Path::new("changed.rs"),
            &scout_stamp,
            StableReadLimits {
                per_file_bytes: 1_024,
                inflight_bytes: 1_024,
            },
            &access,
        );

        assert!(matches!(outcome, StableReadOutcome::UnstableDuringRead));
        assert_eq!(first_calls.get(), 1);
        assert_eq!(
            second_calls.get(),
            0,
            "changed scout/open-handle state must short-circuit the second pass"
        );
    }

    #[test]
    fn read_failure_retains_unreadable_disposition() {
        use std::cell::Cell;

        struct FailingReadAccess<'a> {
            first_calls: &'a Cell<usize>,
            second_calls: &'a Cell<usize>,
        }

        impl StableReadAccess for FailingReadAccess<'_> {
            fn first_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                self.first_calls.set(self.first_calls.get() + 1);
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "injected read refusal",
                ))
            }

            fn second_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                self.second_calls.set(self.second_calls.get() + 1);
                panic!("failed first read must not attempt a second pass")
            }
        }

        let stamp = crate::domain::FileStamp {
            size: 4,
            created_hint: None,
            modified_hint: None,
            platform_id: None,
        };
        let first_calls = Cell::new(0);
        let second_calls = Cell::new(0);
        let access = FailingReadAccess {
            first_calls: &first_calls,
            second_calls: &second_calls,
        };

        let outcome = stable_read_with_access(
            Path::new("locked.rs"),
            &stamp,
            StableReadLimits {
                per_file_bytes: 1_024,
                inflight_bytes: 1_024,
            },
            &access,
        );

        assert!(matches!(
            outcome,
            StableReadOutcome::Unreadable {
                stage: crate::domain::AccessStage::FullRead,
                kind: crate::domain::AccessErrorKind::PermissionDenied,
            }
        ));
        assert_eq!(first_calls.get(), 1);
        assert_eq!(second_calls.get(), 0);
    }

    #[test]
    fn stable_read_double_pass_rejects_same_stamp_torn_write() {
        struct ScriptedStableReadAccess {
            first: StableReadPass,
            second: StableReadPass,
        }

        impl StableReadAccess for ScriptedStableReadAccess {
            fn first_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                Ok(self.first.clone())
            }

            fn second_pass(
                &self,
                _path: &Path,
                _max_bytes: usize,
            ) -> std::io::Result<StableReadPass> {
                Ok(self.second.clone())
            }
        }

        let stamp = crate::domain::FileStamp {
            size: 4,
            created_hint: None,
            modified_hint: Some(std::time::UNIX_EPOCH),
            platform_id: None,
        };
        let pass = |bytes: Option<Vec<u8>>, payload: &[u8]| StableReadPass {
            bytes,
            length: 4,
            hash: crate::hash::digest(payload),
            handle_before: stamp.clone(),
            handle_after: stamp.clone(),
            path_after: stamp.clone(),
        };
        let limits = StableReadLimits {
            per_file_bytes: 1_024,
            inflight_bytes: 1_024,
        };

        let accepted = stable_read_with_access(
            Path::new("stable.rs"),
            &stamp,
            limits,
            &ScriptedStableReadAccess {
                first: pass(Some(b"same".to_vec()), b"same"),
                second: pass(None, b"same"),
            },
        );
        assert!(matches!(
            accepted,
            StableReadOutcome::Accepted { ref bytes, hash }
                if bytes == b"same" && hash == crate::hash::digest(b"same")
        ));

        let torn = stable_read_with_access(
            Path::new("torn.rs"),
            &stamp,
            limits,
            &ScriptedStableReadAccess {
                first: pass(Some(b"same".to_vec()), b"same"),
                second: pass(None, b"torn"),
            },
        );
        assert!(matches!(torn, StableReadOutcome::UnstableDuringRead));
    }

    #[test]
    fn filesystem_stable_read_accepts_unchanged_bytes_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exact.rs");
        let expected = b"fn exact() {\r\n    println!(\"byte exact\");\r\n}\r\n";
        std::fs::write(&path, expected).unwrap();
        let stamp = file_stamp_from_metadata(&std::fs::metadata(&path).unwrap());

        let outcome = stable_read_with_retries(
            &path,
            &stamp,
            StableReadLimits {
                per_file_bytes: crate::domain::index::HARD_SKIP_BYTES,
                inflight_bytes: crate::domain::index::HARD_SKIP_BYTES,
            },
            &FilesystemStableReadAccess,
        );

        assert!(matches!(
            outcome,
            StableReadOutcome::Accepted { bytes, hash }
                if bytes == expected && hash == crate::hash::digest(expected)
        ));
    }

    #[test]
    fn inflight_budget_releases_on_permit_drop() {
        let budget = Arc::new(InflightByteBudget::new(1000));
        assert_eq!(budget.available_bytes(), 1000);

        let permit_a = budget.acquire(400);
        assert_eq!(budget.available_bytes(), 600);

        let permit_b = budget.acquire(600);
        assert_eq!(budget.available_bytes(), 0);

        drop(permit_a);
        assert_eq!(budget.available_bytes(), 400);

        drop(permit_b);
        assert_eq!(budget.available_bytes(), 1000);
    }

    #[test]
    fn inflight_permit_releases_at_staged_handoff_without_deadlock() {
        let inflight = Arc::new(InflightByteBudget::new(4));
        let staged = StagedContentAccounting::new(8);

        let first = inflight.acquire(4);
        assert_eq!(inflight.available_bytes(), 0);
        assert!(staged.handoff(4, Some(first)));
        assert_eq!(staged.used_bytes(), 4);
        assert_eq!(inflight.available_bytes(), 4);

        // This immediate second acquisition would deadlock if the first permit
        // remained attached to bytes already owned by the staged index.
        let second = inflight.acquire(4);
        assert_eq!(inflight.available_bytes(), 0);
        assert!(staged.handoff(4, Some(second)));
        assert_eq!(staged.used_bytes(), 8);
        assert_eq!(inflight.available_bytes(), 4);
    }

    #[test]
    fn inflight_budget_clamps_oversized_request_to_total() {
        // A request larger than the whole budget must not deadlock: it is
        // clamped to the total so the file still reads (alone) and is admitted.
        let budget = Arc::new(InflightByteBudget::new(256));
        let permit = budget.acquire(10_000_000);
        assert_eq!(budget.available_bytes(), 0, "clamped to the full budget");
        drop(permit);
        assert_eq!(budget.available_bytes(), 256);
    }

    #[test]
    fn inflight_budget_zero_total_does_not_deadlock() {
        // A zero/garbage budget is clamped to at least 1 byte so acquisition
        // always makes progress rather than blocking forever.
        let budget = Arc::new(InflightByteBudget::new(0));
        assert!(budget.available_bytes() >= 1);
        let permit = budget.acquire(123);
        drop(permit);
        assert!(budget.available_bytes() >= 1);
    }

    #[test]
    fn inflight_budget_blocks_until_capacity_frees() {
        use std::sync::mpsc;
        use std::thread;

        // Budget only fits one large file at a time. A second acquirer must
        // block until the first releases — proving the peak bound is enforced,
        // not merely advisory.
        let budget = Arc::new(InflightByteBudget::new(512 * 1024));
        let first = budget.acquire(512 * 1024);
        assert_eq!(budget.available_bytes(), 0);

        let (tx, rx) = mpsc::channel();
        let budget_clone = Arc::clone(&budget);
        let waiter = thread::spawn(move || {
            let _permit = budget_clone.acquire(512 * 1024);
            tx.send(()).expect("send acquisition signal");
            // Hold briefly so the main thread can observe the depleted budget.
            thread::sleep(Duration::from_millis(20));
        });

        // The waiter must NOT have acquired yet — budget is full.
        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "second acquirer should block while the budget is exhausted"
        );

        // Release the first permit; the waiter should now proceed.
        drop(first);
        rx.recv_timeout(Duration::from_secs(2))
            .expect("waiter should acquire once budget frees");
        waiter.join().expect("waiter thread joins");
        assert_eq!(budget.available_bytes(), 512 * 1024);
    }

    #[test]
    fn load_under_tight_inflight_budget_still_indexes_all_large_files() {
        let _lock = INFLIGHT_ENV_LOCK.lock().unwrap();
        // Tight budget: 512 KiB total, well below the combined size of several
        // over-threshold files. They must all still be indexed — only the PEAK
        // concurrent read footprint is bounded, never which files are admitted.
        let _env = InflightEnvGuard::set(Some(&(512 * 1024).to_string()));

        let tmp = TempDir::new().unwrap();
        // Build valid Rust files below the per-file in-flight ceiling so each can
        // complete even though their combined size exceeds the total budget.
        const FNS_PER_FILE: u32 = 24_000;
        let mut body = String::with_capacity(400 * 1024);
        for i in 0..FNS_PER_FILE {
            use std::fmt::Write;
            writeln!(body, "fn f_{i:07}() {{}}").unwrap();
        }
        assert!(
            (body.len() as u64) < crate::domain::index::METADATA_ONLY_BYTES,
            "fixture ({} bytes) must stay Normal-tier (under the {} metadata ceiling)",
            body.len(),
            crate::domain::index::METADATA_ONLY_BYTES
        );

        const FILE_COUNT: usize = 6;
        for n in 0..FILE_COUNT {
            write_file(tmp.path(), &format!("big_{n}.rs"), &body);
        }
        // A tiny file to confirm small files index alongside the governed ones.
        write_file(tmp.path(), "tiny.rs", "fn tiny() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read();

        assert!(
            !index.cb_state.is_tripped(),
            "valid large files must not trip the circuit breaker"
        );
        // Coverage invariant: every over-threshold file plus the tiny file is
        // indexed despite the tight in-flight budget.
        assert_eq!(
            index.file_count(),
            FILE_COUNT + 1,
            "all large files plus the tiny file must remain indexed under the cap"
        );
        // Each large file's symbols were extracted, proving content was read and
        // parsed, not skipped. FILE_COUNT * FNS_PER_FILE functions, plus tiny().
        assert!(
            index.symbol_count() as u64 >= FILE_COUNT as u64 * FNS_PER_FILE as u64,
            "large files must be fully parsed ({} symbols), not metadata-skipped",
            index.symbol_count()
        );
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
    fn published_generation_is_atomic_under_concurrent_reloads() {
        let shared = LiveIndex::empty();
        let mut replacement = (*shared.live.load_full()).clone();
        replacement.add_file(
            "src/new_generation.rs".to_string(),
            make_indexed_file_for_mutation("src/new_generation.rs"),
        );

        let _writer = shared.write_mutex.lock();
        shared.swap_and_publish_with_hook(replacement, || {
            let live_file_count = shared.read().file_count();
            let health_file_count = shared.published_state().file_count;
            let outline_file_count = shared.published_repo_outline().total_files;
            assert_eq!(
                live_file_count, health_file_count,
                "live content and health must come from one captured generation"
            );
            assert_eq!(
                health_file_count, outline_file_count,
                "health and outline must come from one captured generation"
            );
        });
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
        shared.write().manifest_entries = vec![make_manifest_entry(
            "src/old_project.rs",
            1,
            FileDisposition::Indexed {
                targets: crate::domain::IndexTargets::Code,
                parse_status: crate::domain::index::ParseStatus::Parsed,
            },
        )];

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
        assert_eq!(
            shared.published_generation().project_generation,
            shared.current_project_generation(),
            "the replacement project's generation must be captured inside the same publication root"
        );
        assert!(
            shared.terminal_dispositions().is_empty(),
            "reset_to_empty must drop terminal dispositions from the previous project"
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
        assert_eq!(
            shared.published_generation().project_generation,
            shared.current_project_generation(),
            "reload must publish the new project generation inside the replacement root"
        );
        assert!(!shared.remove_file_at_generation("src/a.rs", gen_a));
        assert_eq!(shared.current_rejected_stale_mutations(), 1);

        let indexed = make_indexed_file_for_mutation("src/stale.rs");
        assert!(!shared.update_file_at_generation("src/stale.rs", indexed, gen_a));
        assert_eq!(shared.current_rejected_stale_mutations(), 2);
    }

    #[test]
    fn reload_builds_a_direct_replacement_without_mutating_previous_generation() {
        let shared = LiveIndex::empty();
        shared.set_local_empty_reason(Some("prior-generation-reason".to_string()));
        let previous = shared.published_generation();
        assert_eq!(
            previous.live.local_empty_reason().as_deref(),
            Some("prior-generation-reason")
        );
        let replacement = TempDir::new().unwrap();
        write_file(
            replacement.path(),
            "src/replacement.rs",
            "pub fn replacement() {}\n",
        );

        shared.reload(replacement.path()).unwrap();

        let current = shared.published_generation();
        assert_eq!(current.live.local_empty_reason(), None);
        assert_eq!(
            previous.live.local_empty_reason().as_deref(),
            Some("prior-generation-reason"),
            "reload must not clear an Arc-owned field inside the previously published root"
        );
        assert!(!Arc::ptr_eq(&previous.live, &current.live));
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
    fn mtime_only_update_is_visible_in_published_root_without_advancing_content_generation() {
        let shared = LiveIndex::empty();
        shared.add_file(
            "src/touched.rs".to_string(),
            make_indexed_file_for_mutation("src/touched.rs"),
        );
        let before = shared.published_generation();
        let before_mtime = before
            .live
            .get_file("src/touched.rs")
            .expect("fixture file")
            .mtime_secs;
        let next_mtime = before_mtime.saturating_add(42);

        shared.touch_mtime("src/touched.rs", next_mtime);

        let after = shared.published_generation();
        assert_eq!(
            after
                .live
                .get_file("src/touched.rs")
                .expect("published fixture file")
                .mtime_secs,
            next_mtime,
            "mtime-only updates must be visible through the immutable publication root"
        );
        assert!(after.publication_generation > before.publication_generation);
        assert_eq!(after.content_generation, before.content_generation);
    }

    #[test]
    fn local_empty_reason_is_published_in_the_same_immutable_root() {
        let shared = LiveIndex::empty();
        let before = shared.published_generation();

        shared.set_local_empty_reason(Some("workspace is not bound".to_string()));

        let after = shared.published_generation();
        assert_eq!(
            after.health.local_empty_reason.as_deref(),
            Some("workspace is not bound")
        );
        assert_eq!(
            after.live.local_empty_reason().as_deref(),
            Some("workspace is not bound")
        );
        assert!(after.publication_generation > before.publication_generation);
        assert_eq!(after.content_generation, before.content_generation);
        assert_eq!(before.live.local_empty_reason(), None);
    }

    #[test]
    fn published_source_set_is_the_single_atomic_root_for_current_source() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/lib.rs", "pub fn source_set() {}\n");
        let shared = LiveIndex::load(tmp.path()).unwrap();
        let before = shared.published_source_set();
        let before_current = before.current_generation();
        assert_eq!(before.sources.len(), 1);
        assert_eq!(
            before_current
                .manifest
                .as_ref()
                .expect("bound manifest")
                .source
                .source_id,
            before.current_source_id
        );

        shared.update_file(
            "src/next.rs".to_string(),
            make_indexed_file_for_mutation("src/next.rs"),
        );

        let after = shared.published_source_set();
        assert!(after.registry_generation > before.registry_generation);
        assert!(Arc::ptr_eq(&before_current, &before.current_generation()));
        assert!(!Arc::ptr_eq(&before_current, &after.current_generation()));
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
        assert_eq!(running.status, PublishedIndexStatus::Loading);
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
    fn unbound_bootstrap_rebinds_writable_project_without_restart() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn alpha() {}");
        write_file(tmp.path(), "b.rs", "fn beta() {}");

        let shared = LiveIndex::empty();
        {
            let index = shared.read();
            index.set_local_empty_reason(Some("workspace is not bound".to_owned()));
            assert!(index.local_empty_reason().is_some());
        }
        {
            let mut index = shared.write();
            index.reload(tmp.path()).expect("reload should succeed");
        }
        let index = shared.read();
        assert_eq!(index.file_count(), 2);
        assert!(index.is_ready(), "after reload should be ready");
        assert_eq!(index.index_state(), IndexState::Ready);
        assert_eq!(index.load_source(), IndexLoadSource::FreshLoad);
        assert_eq!(index.local_empty_reason(), None);
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
    fn failed_reload_preserves_previous_generation() {
        let tmp = TempDir::new().unwrap();
        let shared = LiveIndex::empty();
        shared.add_file(
            "src/retained.rs".to_string(),
            make_indexed_file_for_mutation("src/retained.rs"),
        );
        let before = shared.published_generation();

        let result = shared.reload(&tmp.path().join("missing-repository"));

        assert!(result.is_err(), "invalid reload input must fail");
        let after = shared.published_generation();
        assert!(
            Arc::ptr_eq(&before, &after),
            "failed replacement construction must retain the exact published generation"
        );
    }

    #[test]
    fn failed_observation_publishes_degraded_last_valid_wrapper() {
        let path = "src/retained.rs";
        let shared = LiveIndex::empty();
        shared.add_file(path.to_string(), make_indexed_file_for_mutation(path));
        let before = shared.published_generation();
        let before_file = Arc::clone(before.live.files.get(path).unwrap());
        let scouted = crate::domain::ScoutedEntry {
            path: crate::domain::CatalogPath {
                public_id: path.to_string(),
                normalized_utf8: Some(path.to_string()),
            },
            absolute_path: None,
            stamp: crate::domain::FileStamp {
                size: before_file.byte_len,
                created_hint: None,
                modified_hint: None,
                platform_id: None,
            },
            language: Some(LanguageId::Rust),
            classification: before_file.classification,
            decision: crate::domain::ScoutDecision::Unavailable {
                stage: crate::domain::AccessStage::FullRead,
                kind: crate::domain::AccessErrorKind::PermissionDenied,
            },
        };

        assert!(shared.publish_terminal_disposition_at_generation(
            path,
            scouted,
            FileDisposition::Unreadable {
                stage: crate::domain::AccessStage::FullRead,
                kind: crate::domain::AccessErrorKind::PermissionDenied,
            },
            shared.current_project_generation(),
            before.publication_generation,
        ));

        let after = shared.published_generation();
        assert!(after.publication_generation > before.publication_generation);
        assert_eq!(
            after.content_generation, before.content_generation,
            "failed observation must not mint a new content generation"
        );
        assert!(Arc::ptr_eq(
            &before_file,
            after.live.files.get(path).unwrap()
        ));
        assert!(matches!(
            &*shared.freshness_status(),
            FreshnessStatus::Degraded {
                last_valid_content_generation,
                reason_codes,
            } if *last_valid_content_generation == before.content_generation
                && reason_codes == &[FreshnessReason::ObservationFailed]
        ));
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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        }
    }

    fn make_manifest_entry(path: &str, size: u64, disposition: FileDisposition) -> CatalogEntry {
        CatalogEntry {
            path: crate::domain::CatalogPath {
                public_id: path.to_string(),
                normalized_utf8: Some(path.to_string()),
            },
            size,
            language: None,
            classification: FileClassification {
                class: crate::domain::FileClass::Text,
                is_generated: false,
                is_test: false,
                is_vendor: false,
                is_config: false,
            },
            disposition,
            content_hash: None,
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
        assert_eq!(index.tier_counts(), (1, 0, 0));

        index.remove_file("src/to_delete.rs");
        assert!(
            index.get_file("src/to_delete.rs").is_none(),
            "file should be removed"
        );
        assert_eq!(index.file_count(), 0);
        assert_eq!(
            index.tier_counts(),
            (0, 0, 0),
            "removal must clear the canonical manifest entry with the indexed bytes"
        );
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
    fn publishing_and_removing_a_ref_source_bumps_registry_without_touching_current_lane() {
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let before = handle.published_source_set();
        let before_registry = before.registry_generation;
        let current_id = before.current_source_id.clone();
        let current_lane = before.current_generation();
        let (before_pub, before_content, before_project) = (
            current_lane.publication_generation,
            current_lane.content_generation,
            current_lane.project_generation,
        );

        let ref_gen = handle.build_ref_source_generation(
            LiveIndex::from_source_files(HashMap::new()),
            crate::domain::index::RepositoryId::new("repo-under-test"),
            "refs/heads/feature",
            "0123456789abcdef0123456789abcdef01234567",
            crate::domain::CoverageStatus::Complete,
        );
        let ref_source_id = ref_gen
            .source
            .as_ref()
            .expect("ref source identity")
            .source_id
            .clone();
        assert_ne!(ref_source_id, current_id);
        handle.publish_ref_source(ref_gen);

        let after = handle.published_source_set();
        assert_eq!(
            after.registry_generation,
            before_registry + 1,
            "registry bumps"
        );
        assert_eq!(after.sources.len(), 2);
        assert!(after.sources.contains_key(&ref_source_id));
        assert_eq!(after.current_source_id, current_id);
        let after_current = after.current_generation();
        assert_eq!(after_current.publication_generation, before_pub);
        assert_eq!(after_current.content_generation, before_content);
        assert_eq!(
            after_current.project_generation, before_project,
            "a P1 ref add must not advance the current lane's generations"
        );

        assert!(handle.remove_ref_source(&ref_source_id));
        let removed = handle.published_source_set();
        assert_eq!(removed.registry_generation, before_registry + 2);
        assert_eq!(removed.sources.len(), 1);
        assert!(!removed.sources.contains_key(&ref_source_id));
        assert!(
            !handle.remove_ref_source(&current_id),
            "the current lane can never be removed as a ref source"
        );
    }

    #[test]
    fn p0_publishes_preserve_published_ref_lanes() {
        // L-R13 / L-G07 regression: a P0 (current-worktree) publish must replace
        // only the current lane and preserve every published P1 ref lane. Before
        // the fix, swap_and_publish / publish_prepared_{bridge,authority} rebuilt
        // the source map with only the current lane, silently dropping refs.
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let current_id = handle.published_source_set().current_source_id.clone();

        let ref_gen = handle.build_ref_source_generation(
            LiveIndex::from_source_files(HashMap::new()),
            crate::domain::index::RepositoryId::new("repo-under-test"),
            "refs/heads/feature",
            "0123456789abcdef0123456789abcdef01234567",
            crate::domain::CoverageStatus::Complete,
        );
        let ref_source_id = ref_gen
            .source
            .as_ref()
            .expect("ref source identity")
            .source_id
            .clone();
        handle.publish_ref_source(ref_gen);
        assert_eq!(handle.published_source_set().sources.len(), 2);

        // A P0 content publish must keep the ref lane and advance the current lane.
        let before = handle.published_source_set();
        handle.swap_and_publish(LiveIndex::from_source_files(HashMap::new()));
        let after = handle.published_source_set();
        assert!(
            after.sources.contains_key(&ref_source_id),
            "a P0 content publish must not drop a published ref lane"
        );
        assert_eq!(after.current_source_id, current_id);
        assert!(
            after.registry_generation > before.registry_generation,
            "a source-map change advances registry_generation"
        );
        assert!(
            after.current_generation().publication_generation
                > before.current_generation().publication_generation,
            "the current lane advances on a P0 publish"
        );

        // A prepared-authority P0 publish must also keep the ref lane.
        let prepared = handle.prepare_authority_rebuild();
        assert!(handle.publish_prepared_authority(prepared));
        let after_auth = handle.published_source_set();
        assert!(
            after_auth.sources.contains_key(&ref_source_id),
            "a P0 authority publish must not drop a published ref lane"
        );
        assert_eq!(after_auth.sources.len(), 2);
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
    fn circuit_breaker_tail_retains_aborted_dispositions() {
        let parse_results = (0..10)
            .map(|index| {
                let path = format!("a/f{index:02}.rs");
                let outcome = if index < 5 {
                    FileOutcome::Processed
                } else {
                    FileOutcome::Failed {
                        error: "parse failed".to_string(),
                    }
                };
                let mut result = make_result(outcome, vec![]);
                result.relative_path = path.clone();
                let indexed = IndexedFile::from_parse_result(result, b"fixture".to_vec());
                (path, indexed)
            })
            .collect();

        let folded = fold_parse_results_for_scope(
            parse_results,
            CircuitBreakerState::new(0.20),
            CircuitBreakerScope::new(
                PathBuf::from("source-a"),
                crate::domain::IndexTargets::Code,
                "parse",
            ),
        );
        let aborted_paths: Vec<_> = folded
            .dispositions
            .iter()
            .filter_map(|(path, disposition)| {
                matches!(
                    disposition,
                    crate::domain::FileDisposition::AbortedCircuitBreaker
                )
                .then_some(path.as_str())
            })
            .collect();

        assert_eq!(folded.dispositions.len(), 10);
        assert_eq!(aborted_paths, ["a/f07.rs", "a/f08.rs", "a/f09.rs"]);
        assert!(folded.files.contains_key("a/f06.rs"));
        assert!(!folded.files.contains_key("a/f07.rs"));
    }

    #[test]
    fn circuit_breaker_trip_is_scoped_and_degraded() {
        let results = |fail_after: Option<usize>| {
            (0..10)
                .map(|index| {
                    let path = format!("a/f{index:02}.rs");
                    let outcome = if fail_after.is_some_and(|start| index >= start) {
                        FileOutcome::Failed {
                            error: "parse failed".to_string(),
                        }
                    } else {
                        FileOutcome::Processed
                    };
                    let mut result = make_result(outcome, vec![]);
                    result.relative_path = path.clone();
                    (
                        path,
                        IndexedFile::from_parse_result(result, b"fixture".to_vec()),
                    )
                })
                .collect()
        };
        let tripped_scope = CircuitBreakerScope::new(
            PathBuf::from("source-a"),
            crate::domain::IndexTargets::Code,
            "parse",
        );
        let unaffected_scopes = [
            CircuitBreakerScope::new(
                PathBuf::from("source-a"),
                crate::domain::IndexTargets::Knowledge,
                "parse",
            ),
            CircuitBreakerScope::new(
                PathBuf::from("source-a"),
                crate::domain::IndexTargets::Code,
                "stable-read",
            ),
            CircuitBreakerScope::new(
                PathBuf::from("source-b"),
                crate::domain::IndexTargets::Code,
                "parse",
            ),
        ];

        let tripped = fold_parse_results_for_scope(
            results(Some(5)),
            CircuitBreakerState::new(0.20),
            tripped_scope.clone(),
        );
        assert_eq!(tripped.coverage, crate::domain::CoverageStatus::Degraded);
        assert_eq!(
            tripped
                .dispositions
                .iter()
                .filter(|(_, disposition)| matches!(
                    disposition,
                    crate::domain::FileDisposition::AbortedCircuitBreaker
                ))
                .count(),
            3
        );
        for scope in unaffected_scopes {
            let unaffected =
                fold_parse_results_for_scope(results(None), CircuitBreakerState::new(0.20), scope);
            assert_eq!(unaffected.coverage, crate::domain::CoverageStatus::Complete);
            assert_eq!(unaffected.files.len(), 10);
            assert!(
                unaffected
                    .dispositions
                    .iter()
                    .all(|(_, disposition)| !matches!(
                        disposition,
                        crate::domain::FileDisposition::AbortedCircuitBreaker
                    ))
            );
        }
    }

    /// SF-009: surfacing of indexed-but-untracked files, and the opt-in
    /// exclude-untracked admission policy. The mechanism in the original bug report
    /// (text scratch files inflating symbol counts) is REFUTED — these tests both
    /// document that the reported bug does not reproduce and exercise the real
    /// surfacing fix.
    mod sf009_untracked_surfacing {
        use super::*;
        use crate::discovery::{self, EXCLUDE_UNTRACKED_ENV};

        /// Serialize all tests that touch the process-global
        /// `SYMFORGE_EXCLUDE_UNTRACKED` env var.
        static EXCLUDE_UNTRACKED_ENV_LOCK: StdMutex<()> = StdMutex::new(());

        /// RAII guard for the process-global `SYMFORGE_EXCLUDE_UNTRACKED` env var.
        /// Restores the previous value on drop so the flag never leaks across tests.
        /// Callers must hold `EXCLUDE_UNTRACKED_ENV_LOCK` for the guard's lifetime.
        struct ExcludeUntrackedEnvGuard {
            previous: Option<String>,
        }

        #[allow(unsafe_code)] // test-only env guard serializes the exclude-untracked flag.
        impl ExcludeUntrackedEnvGuard {
            fn set(value: Option<&str>) -> Self {
                let previous = std::env::var(EXCLUDE_UNTRACKED_ENV).ok();
                // SAFETY: callers hold EXCLUDE_UNTRACKED_ENV_LOCK; these tests run single-threaded.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(EXCLUDE_UNTRACKED_ENV, v),
                        None => std::env::remove_var(EXCLUDE_UNTRACKED_ENV),
                    }
                }
                Self { previous }
            }

            /// Set the live env var to `value` WITHOUT changing the saved
            /// original, so restore-on-drop stays correct across phase
            /// transitions.
            fn apply(&self, value: Option<&str>) {
                // SAFETY: callers hold EXCLUDE_UNTRACKED_ENV_LOCK; these tests run single-threaded.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(EXCLUDE_UNTRACKED_ENV, v),
                        None => std::env::remove_var(EXCLUDE_UNTRACKED_ENV),
                    }
                }
            }
        }

        #[allow(unsafe_code)] // test-only env guard restores the serialized exclude-untracked flag.
        impl Drop for ExcludeUntrackedEnvGuard {
            fn drop(&mut self) {
                // SAFETY: callers hold EXCLUDE_UNTRACKED_ENV_LOCK; these tests run single-threaded.
                unsafe {
                    match self.previous.as_deref() {
                        Some(v) => std::env::set_var(EXCLUDE_UNTRACKED_ENV, v),
                        None => std::env::remove_var(EXCLUDE_UNTRACKED_ENV),
                    }
                }
            }
        }

        fn git(root: &Path, args: &[&str]) {
            let status = crate::process_util::hidden_command("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git command should run");
            assert!(
                status.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&status.stderr)
            );
        }

        fn init_repo(root: &Path) {
            git(root, &["init"]);
            git(root, &["config", "user.email", "test@test.com"]);
            git(root, &["config", "user.name", "Test"]);
        }

        /// Documents that the report's stated mechanism does NOT reproduce:
        /// repository-owned dotfiles are deliberately discovered. Safe UTF-8
        /// text is generic knowledge and therefore resident, but still produces
        /// zero code symbols.
        #[test]
        fn report_bug_does_not_reproduce_generic_text_has_zero_code_symbols() {
            let tmp = TempDir::new().unwrap();
            init_repo(tmp.path());
            write_file(tmp.path(), "src/main.rs", "fn main() {}");
            write_file(tmp.path(), "notes.txt", "scratch notes, unknown ext");
            write_file(tmp.path(), ".probe.txt", "dotfile scratch");
            git(tmp.path(), &["add", "src/main.rs", "notes.txt"]);
            git(tmp.path(), &["commit", "-m", "init"]);

            let entries = discovery::discover_all_files(tmp.path()).unwrap();
            let paths: Vec<&str> = entries.iter().map(|e| e.relative_path.as_str()).collect();

            // Repository-owned hidden knowledge is discoverable; hidden-path
            // filtering is reserved for VCS/runtime internals.
            assert!(
                paths.contains(&".probe.txt"),
                "repository-owned dotfile should reach admission: {paths:?}"
            );
            // Both text files use the authoritative generic-text language.
            let notes = entries
                .iter()
                .find(|e| e.relative_path == "notes.txt")
                .expect("notes.txt should be discovered");
            assert_eq!(notes.language, Some(LanguageId::Text));
            let probe = entries
                .iter()
                .find(|e| e.relative_path == ".probe.txt")
                .expect(".probe.txt should be discovered");
            assert_eq!(probe.language, Some(LanguageId::Text));

            // Both files are resident knowledge, but neither contributes symbols.
            let shared = LiveIndex::load(tmp.path()).unwrap();
            let index = shared.read();
            for path in ["notes.txt", ".probe.txt"] {
                let file = index
                    .get_file(path)
                    .unwrap_or_else(|| panic!("generic knowledge file missing: {path}"));
                assert_eq!(file.language, LanguageId::Text);
                assert!(file.symbols.is_empty());
                assert!(matches!(
                    index
                        .manifest_entries
                        .iter()
                        .find(|entry| entry.path.normalized_utf8.as_deref() == Some(path))
                        .map(|entry| &entry.disposition),
                    Some(FileDisposition::Indexed {
                        targets: crate::domain::IndexTargets::Knowledge,
                        ..
                    })
                ));
            }
        }

        /// A non-dotfile untracked recognized-extension source file is surfaced as
        /// `untracked_indexed == 1` and rendered in the health line.
        #[test]
        fn untracked_recognized_ext_file_is_surfaced() {
            let tmp = TempDir::new().unwrap();
            init_repo(tmp.path());
            write_file(tmp.path(), "src/main.rs", "fn main() {}");
            git(tmp.path(), &["add", "src/main.rs"]);
            git(tmp.path(), &["commit", "-m", "init"]);
            // Untracked recognized-extension source (NOT git-added).
            write_file(tmp.path(), "scratch.rs", "fn scratch() {}");

            let shared = LiveIndex::load(tmp.path()).unwrap();
            let index = shared.read();
            let stats = index.health_stats();

            assert_eq!(
                stats.untracked_indexed, 1,
                "exactly one untracked recognized-ext indexed file expected"
            );

            // The health-report rendering lives in the server-gated `protocol`
            // module, so assert it only when `server` is compiled. The engine-side
            // `untracked_indexed` stat above is still exercised under `embed`.
            #[cfg(feature = "server")]
            {
                let report = crate::protocol::format::health_report_from_stats("Ready", &stats, 0);
                assert!(
                    report.contains("indexed untracked files: 1"),
                    "health line should surface the untracked count: {report}"
                );
            }
        }

        /// FAIL-OPEN: a plain tempdir with NO git repository must report
        /// `untracked_indexed == 0` — NOT every-file-counts.
        #[test]
        fn fail_open_no_git_repo_reports_zero() {
            let tmp = TempDir::new().unwrap();
            // No `git init`. Several recognized-extension source files.
            write_file(tmp.path(), "src/main.rs", "fn main() {}");
            write_file(tmp.path(), "src/lib.rs", "pub fn lib() {}");
            write_file(tmp.path(), "scratch.rs", "fn scratch() {}");

            let shared = LiveIndex::load(tmp.path()).unwrap();
            let index = shared.read();
            let stats = index.health_stats();

            assert_eq!(
                stats.untracked_indexed, 0,
                "with no git repo the feature must fail open to 0, not count every file"
            );
            // Server-gated formatter assertion (see note above); the engine-side
            // fail-open `untracked_indexed == 0` check above runs under `embed`.
            #[cfg(feature = "server")]
            {
                let report = crate::protocol::format::health_report_from_stats("Ready", &stats, 0);
                assert!(
                    !report.contains("indexed untracked files:"),
                    "no untracked line should appear when the count is 0: {report}"
                );
            }
        }

        /// A fully-tracked repo reports `untracked_indexed == 0`.
        #[test]
        fn fully_tracked_repo_reports_zero() {
            let tmp = TempDir::new().unwrap();
            init_repo(tmp.path());
            write_file(tmp.path(), "src/main.rs", "fn main() {}");
            write_file(tmp.path(), "src/lib.rs", "pub fn lib() {}");
            git(tmp.path(), &["add", "."]);
            git(tmp.path(), &["commit", "-m", "init"]);

            let shared = LiveIndex::load(tmp.path()).unwrap();
            let index = shared.read();
            let stats = index.health_stats();

            assert_eq!(
                stats.untracked_indexed, 0,
                "a fully-tracked repo must report zero untracked indexed files"
            );
        }

        /// Opt-in `SYMFORGE_EXCLUDE_UNTRACKED` demotes untracked recognized-ext
        /// files out of Tier-1; with the default OFF it is a strict no-op.
        #[test]
        fn exclude_untracked_env_gate_demotes_only_when_enabled() {
            let _lock = EXCLUDE_UNTRACKED_ENV_LOCK.lock().unwrap();
            // One RAII guard for the whole test: it captures the ORIGINAL value once
            // and restores it on drop (even on panic). Phase transitions use
            // `apply()`, which mutates the live env WITHOUT touching the saved
            // original, so the restore-on-drop is always correct.
            let env = ExcludeUntrackedEnvGuard::set(None);

            let tmp = TempDir::new().unwrap();
            init_repo(tmp.path());
            write_file(tmp.path(), "src/main.rs", "fn main() {}");
            git(tmp.path(), &["add", "src/main.rs"]);
            git(tmp.path(), &["commit", "-m", "init"]);
            write_file(tmp.path(), "scratch.rs", "fn scratch() {}");

            // Default OFF: untracked file is still admitted (Tier-1), only surfaced.
            {
                assert!(!discovery::exclude_untracked_enabled());
                let shared = LiveIndex::load(tmp.path()).unwrap();
                let index = shared.read();
                assert!(
                    index.files.contains_key("scratch.rs"),
                    "default policy must still index the untracked file (admission unchanged)"
                );
                assert_eq!(index.health_stats().untracked_indexed, 1);
            }

            // Opt-in ON: untracked recognized-ext file is demoted to Tier-2.
            env.apply(Some("1"));
            {
                assert!(discovery::exclude_untracked_enabled());
                let shared = LiveIndex::load(tmp.path()).unwrap();
                let index = shared.read();
                assert!(
                    !index.files.contains_key("scratch.rs"),
                    "with the opt-in policy ON the untracked file is demoted out of Tier-1"
                );
                assert!(
                    index.files.contains_key("src/main.rs"),
                    "tracked files remain Tier-1 under the opt-in policy"
                );
                // Demoted to Tier-2: recorded as a skipped file with the Untracked reason.
                assert!(
                    index
                        .compatibility_skipped_files()
                        .iter()
                        .any(|sf| sf.path == "scratch.rs"
                            && sf.reason() == Some(crate::domain::index::SkipReason::Untracked)),
                    "demoted file should be a Tier-2 skip with the Untracked reason"
                );
                // It is no longer a Tier-1 file, so it does not count as untracked-indexed.
                assert_eq!(index.health_stats().untracked_indexed, 0);
            }
            // `env` restores the original env state on drop.
        }
    }

    #[test]
    fn test_tier_counts() {
        let mut index = make_empty_live_index();
        assert_eq!(index.tier_counts(), (0, 0, 0));

        index.manifest_entries.extend([
            make_manifest_entry(
                "model.bin",
                1000,
                FileDisposition::MetadataOnly {
                    reason: MetadataOnlyReason::GeneratedOrVendor,
                },
            ),
            make_manifest_entry(
                "huge.dat",
                200_000_000,
                FileDisposition::HardSkip {
                    reason: HardSkipReason::PerFileCeiling,
                },
            ),
        ]);

        assert_eq!(index.tier_counts(), (0, 1, 1));
    }

    /// SF-012(A): a small, non-binary file whose extension maps to no supported
    /// grammar must be admitted Tier-2 (metadata-only / unsupported-language) by
    /// the real `LiveIndex::load` admission pipeline — not dropped into a
    /// contradictory Tier-1/Normal `SkippedFile` that vanishes from tier counts.
    /// This is the corpus case (redis `.tcl`/`.sh`, phoenix `.eex`, extensionless
    /// `LICENSE`/`Makefile`).
    #[test]
    fn load_routes_unknown_utf8_files_as_generic_knowledge() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/main.rs", "fn main() {}");
        write_file(
            tmp.path(),
            "tests/unit/foo.tcl",
            "proc foo {} { return 1 }\n",
        );
        write_file(tmp.path(), "scripts/setup.sh", "#!/bin/sh\necho hi\n");
        write_file(tmp.path(), "LICENSE", "MIT License\n");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read();

        let (tier1, tier2, tier3) = index.tier_counts();
        assert_eq!(
            tier1, 4,
            "safe UTF-8 files must be resident Tier-1 evidence"
        );
        assert_eq!(tier2, 0);
        assert_eq!(tier3, 0);

        for path in ["tests/unit/foo.tcl", "scripts/setup.sh", "LICENSE"] {
            let file = index
                .get_file(path)
                .unwrap_or_else(|| panic!("generic knowledge file missing: {path}"));
            assert_eq!(file.language, LanguageId::Text);
            assert!(file.classification.is_text());
            assert!(file.symbols.is_empty());
            assert!(matches!(
                index
                    .manifest_entries
                    .iter()
                    .find(|entry| entry.path.normalized_utf8.as_deref() == Some(path))
                    .map(|entry| &entry.disposition),
                Some(crate::domain::FileDisposition::Indexed {
                    targets: crate::domain::IndexTargets::Knowledge,
                    ..
                })
            ));
        }
    }

    #[test]
    fn unknown_utf8_file_becomes_generic_knowledge() {
        let tmp = TempDir::new().unwrap();
        let exact = b"first fact\r\nsecond fact without final newline";
        let path = tmp.path().join("notes").join("facts.unknown-format");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, exact).unwrap();

        let shared = LiveIndex::load(tmp.path()).expect("generic UTF-8 load must succeed");
        let index = shared.read();
        let file = index
            .get_file("notes/facts.unknown-format")
            .expect("safe unknown UTF-8 must become resident knowledge");

        assert_eq!(file.language, LanguageId::Text);
        assert!(file.classification.is_text());
        assert_eq!(file.content.as_slice(), exact);
        assert!(file.symbols.is_empty());

        let disposition = index
            .manifest_entries
            .iter()
            .find(|entry| {
                entry.path.normalized_utf8.as_deref() == Some("notes/facts.unknown-format")
            })
            .map(|entry| &entry.disposition)
            .expect("generic knowledge must retain a manifest disposition");
        assert!(matches!(
            disposition,
            FileDisposition::Indexed {
                targets: crate::domain::IndexTargets::Knowledge,
                parse_status: crate::domain::index::ParseStatus::Parsed
            }
        ));
    }

    #[test]
    fn sensitive_files_remain_catalog_only_with_zero_value_leakage() {
        let tmp = TempDir::new().unwrap();
        let canary = runtime_canary();
        let password_kw = ["pass", "word"].concat();
        let token_kw = ["to", "ken"].concat();
        write_file(tmp.path(), ".env", &format!("{password_kw}={canary}\n"));
        write_file(
            tmp.path(),
            "notes/guide.txt",
            &format!("{token_kw}={canary}\n"),
        );

        let shared = LiveIndex::load(tmp.path()).expect("sensitive fixture must load safely");
        let index = shared.read();
        assert!(index.get_file(".env").is_none());
        assert!(index.get_file("notes/guide.txt").is_none());

        let env = index
            .manifest_entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some(".env"))
            .expect("sensitive path must remain cataloged");
        assert!(matches!(
            env.disposition,
            FileDisposition::MetadataOnly {
                reason: MetadataOnlyReason::SensitivePath { .. }
            }
        ));
        assert!(env.content_hash.is_none());

        let content = index
            .manifest_entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some("notes/guide.txt"))
            .expect("detector-positive content must remain cataloged");
        assert!(matches!(
            content.disposition,
            FileDisposition::MetadataOnly {
                reason: MetadataOnlyReason::SensitiveContent { .. }
            }
        ));
        assert!(content.content_hash.is_none());

        let serialized = serde_json::to_vec(&index.manifest_entries).unwrap();
        assert!(
            !serialized
                .windows(canary.len())
                .any(|window| window == canary.as_bytes())
        );
    }

    #[test]
    fn safe_template_path_still_runs_content_detector() {
        let tmp = TempDir::new().unwrap();
        let canary = runtime_canary();
        let password_kw = ["pass", "word"].concat();
        write_file(
            tmp.path(),
            ".env.example",
            &format!("{password_kw}={canary}\n"),
        );

        let shared = LiveIndex::load(tmp.path()).expect("safe template fixture must load");
        let index = shared.read();
        let entry = index
            .manifest_entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some(".env.example"))
            .expect("template must remain cataloged");
        assert!(matches!(
            entry.disposition,
            FileDisposition::MetadataOnly {
                reason: MetadataOnlyReason::SensitiveContent { .. }
            }
        ));
        assert!(entry.content_hash.is_none());
        assert!(index.get_file(".env.example").is_none());
    }

    #[test]
    fn lfs_pointer_is_catalog_only_and_never_knowledge_searchable() {
        let tmp = TempDir::new().unwrap();
        write_file(
            tmp.path(),
            "docs/asset.txt",
            "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n",
        );

        let shared = LiveIndex::load(tmp.path()).expect("LFS pointer fixture must load");
        let index = shared.read();
        let entry = index
            .manifest_entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some("docs/asset.txt"))
            .expect("pointer must remain cataloged");
        assert!(matches!(
            entry.disposition,
            FileDisposition::MetadataOnly {
                reason: MetadataOnlyReason::LfsPointer { .. }
            }
        ));
        assert!(entry.content_hash.is_none());
        assert!(index.get_file("docs/asset.txt").is_none());
    }

    #[test]
    fn non_utf8_text_is_catalog_only_without_lossy_evidence() {
        let tmp = TempDir::new().unwrap();
        let mut bytes = vec![b'a'; crate::domain::index::BINARY_SNIFF_BYTES];
        bytes.extend_from_slice(&[0xff, 0xfe, b'!']);
        let path = tmp.path().join("notes.txt");
        std::fs::write(&path, &bytes).unwrap();

        let shared = LiveIndex::load(tmp.path()).expect("invalid text must fail closed");
        let index = shared.read();
        let entry = index
            .manifest_entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some("notes.txt"))
            .expect("invalid text must remain cataloged");
        assert!(matches!(
            entry.disposition,
            FileDisposition::MetadataOnly {
                reason: MetadataOnlyReason::UnsupportedTextEncoding
            }
        ));
        assert!(entry.content_hash.is_none());
        assert!(index.get_file("notes.txt").is_none());
    }

    #[test]
    fn load_and_reload_publish_authoritative_scout_plan() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/main.rs", "fn main() {}\n");
        write_file(tmp.path(), "notes.txt", "repository knowledge\n");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let initial = shared
            .scout_plan()
            .expect("fresh load must retain its authoritative scout plan");
        assert_eq!(initial.entries.len(), 2);
        assert_eq!(
            shared
                .terminal_dispositions()
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            vec!["notes.txt", "src/main.rs"]
        );
        assert!(initial.entries.iter().any(|entry| {
            entry.path.normalized_utf8.as_deref() == Some("src/main.rs")
                && matches!(
                    entry.decision,
                    crate::domain::ScoutDecision::Ingest {
                        targets: crate::domain::IndexTargets::Code
                    }
                )
        }));
        assert!(initial.entries.iter().any(|entry| {
            entry.path.normalized_utf8.as_deref() == Some("notes.txt")
                && matches!(
                    entry.decision,
                    crate::domain::ScoutDecision::Ingest {
                        targets: crate::domain::IndexTargets::Knowledge
                    }
                )
        }));

        write_file(tmp.path(), "README.md", "# Added later\n");
        shared.reload(tmp.path()).unwrap();
        let reloaded = shared
            .scout_plan()
            .expect("reload must atomically replace the authoritative scout plan");
        assert_eq!(reloaded.entries.len(), 3);
        assert_eq!(
            shared
                .terminal_dispositions()
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            vec!["notes.txt", "README.md", "src/main.rs"]
        );
        assert!(
            reloaded
                .entries
                .iter()
                .any(|entry| entry.path.normalized_utf8.as_deref() == Some("README.md"))
        );
    }

    #[test]
    fn legacy_execution_projection_retains_one_outcome_per_scout_entry() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/main.rs", "fn main() {}\n");
        write_file(tmp.path(), "notes.txt", "repository knowledge\n");
        write_file(tmp.path(), "Cargo.lock", "version = 4\n");

        let scout = discovery::scout_repository(tmp.path()).unwrap();
        let projection = project_scout_for_legacy_execution(&scout);
        let accounted = projection.entries.len() + projection.terminal_dispositions.len();
        let mut paths: Vec<_> = projection
            .entries
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .chain(
                projection
                    .terminal_dispositions
                    .iter()
                    .map(|(path, _)| path.as_str()),
            )
            .collect();
        paths.sort_unstable();
        paths.dedup();

        assert_eq!(accounted, scout.entries.len());
        assert_eq!(paths.len(), scout.entries.len());
    }

    #[test]
    fn compatibility_skips_are_projected_from_manifest_entries() {
        let mut index = make_empty_live_index();
        index.manifest_entries = vec![crate::domain::CatalogEntry {
            path: crate::domain::CatalogPath {
                public_id: "Cargo.lock".to_string(),
                normalized_utf8: Some("Cargo.lock".to_string()),
            },
            size: 128,
            language: Some(crate::domain::LanguageId::Toml),
            classification: crate::domain::FileClassification {
                class: crate::domain::FileClass::Text,
                is_generated: false,
                is_test: false,
                is_vendor: false,
                is_config: true,
            },
            disposition: crate::domain::FileDisposition::MetadataOnly {
                reason: crate::domain::MetadataOnlyReason::Lockfile,
            },
            content_hash: None,
        }];

        let projected = index.compatibility_skipped_files();
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].path, "Cargo.lock");
        assert_eq!(projected[0].size, 128);
        assert_eq!(
            projected[0].decision.reason,
            Some(SkipReason::DependencyLockfile)
        );
        assert_eq!(index.tier_counts(), (0, 1, 0));
    }

    /// SF-025 / SF-012: the health/admission counters must RECONCILE on any
    /// index instant. Two invariants, asserted on a synthetic index built from
    /// the public accessors:
    ///   1. tier1 + tier2 + tier3 == manifest entries
    ///      (every record lands in exactly one tier — no silent drops).
    ///   2. parsed + partial + failed == file_count
    ///      (every indexed file is in exactly one parse state).
    #[test]
    fn tier_and_parse_counters_reconcile_on_synthetic_index() {
        let mut index = make_empty_live_index();

        // Three Tier-1 files in distinct parse states.
        let mut parsed = make_indexed_file_for_mutation("src/parsed.rs");
        parsed.parse_status = ParseStatus::Parsed;
        index.update_file("src/parsed.rs".into(), parsed);

        let mut partial = make_indexed_file_for_mutation("src/partial.rs");
        partial.parse_status = ParseStatus::PartialParse {
            warning: "partial".into(),
        };
        index.update_file("src/partial.rs".into(), partial);

        let mut failed = make_indexed_file_for_mutation("src/failed.rs");
        failed.parse_status = ParseStatus::Failed {
            error: "boom".into(),
        };
        index.update_file("src/failed.rs".into(), failed);

        let manifest_entry = |path: &str, size: u64, disposition: FileDisposition| CatalogEntry {
            path: crate::domain::CatalogPath {
                public_id: path.to_string(),
                normalized_utf8: Some(path.to_string()),
            },
            size,
            language: None,
            classification: FileClassification {
                class: crate::domain::FileClass::Text,
                is_generated: false,
                is_test: false,
                is_vendor: false,
                is_config: false,
            },
            disposition,
            content_hash: None,
        };
        index.manifest_entries.extend([
            manifest_entry(
                "vendor.lock",
                100,
                FileDisposition::MetadataOnly {
                    reason: MetadataOnlyReason::Lockfile,
                },
            ),
            manifest_entry(
                "tests/foo.tcl",
                50,
                FileDisposition::MetadataOnly {
                    reason: MetadataOnlyReason::UnsupportedTextEncoding,
                },
            ),
            manifest_entry(
                "huge.bin",
                200_000_000,
                FileDisposition::HardSkip {
                    reason: HardSkipReason::PerFileCeiling,
                },
            ),
        ]);

        // Invariant 1: tier sum == total records (indexed + skipped).
        let (tier1, tier2, tier3) = index.tier_counts();
        let discovered = index.file_count() + index.compatibility_skipped_files().len();
        assert_eq!(
            tier1 + tier2 + tier3,
            discovered,
            "tier sum ({}+{}+{}) must equal discovered records ({})",
            tier1,
            tier2,
            tier3,
            discovered
        );
        assert_eq!(tier1, index.file_count(), "Tier-1 must equal indexed files");
        assert_eq!((tier1, tier2, tier3), (3, 2, 1));

        // Invariant 2: parse states partition the indexed files exactly.
        let stats = index.health_stats();
        assert_eq!(
            stats.parsed_count + stats.partial_parse_count + stats.failed_count,
            stats.file_count,
            "parsed+partial+failed must equal indexed file count"
        );
        assert_eq!(stats.file_count, 3);
        assert_eq!(stats.parsed_count, 1);
        assert_eq!(stats.partial_parse_count, 1);
        assert_eq!(stats.failed_count, 1);
    }
}
