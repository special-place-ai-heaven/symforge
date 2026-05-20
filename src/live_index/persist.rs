/// LiveIndex persistence: serialize on shutdown, load on startup.
///
/// Uses postcard (compact binary) for fast round-trips.
/// Atomic write (tmp → rename) to prevent corruption on crash.
/// Background verification corrects stale entries after loading a snapshot.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::domain::{FileClassification, LanguageId, ReferenceRecord, SymbolRecord};
use crate::live_index::store::{
    CircuitBreakerState, IndexLoadSource, IndexedFile, LiveIndex, ParseStatus, SnapshotVerifyState,
};
use crate::paths;

// ── Constants ─────────────────────────────────────────────────────────────────

use crate::domain::ParseDiagnostic;

const CURRENT_VERSION: u32 = 4;
const INDEX_FILENAME: &str = "index.bin";
const INDEX_TMP_FILENAME: &str = "index.bin.tmp";
pub const SNAPSHOT_RESET_SCOPE_LABEL: &str = ".symforge/index.bin,.symforge/index.bin.tmp";

// ── Snapshot types ────────────────────────────────────────────────────────────

/// Serializable snapshot of all per-file data in a `LiveIndex`.
///
/// Does NOT include non-serializable fields (Instant, AtomicUsize, RwLock).
/// Reverse index and trigram index are rebuilt from snapshot on load.
#[derive(Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub version: u32,
    pub files: HashMap<String, IndexedFileSnapshot>,
}

/// Serializable snapshot of a single indexed file.
#[derive(Serialize, Deserialize, Clone)]
pub struct IndexedFileSnapshot {
    pub relative_path: String,
    pub language: LanguageId,
    pub classification: FileClassification,
    pub content: Vec<u8>,
    pub symbols: Vec<SymbolRecord>,
    pub parse_status: ParseStatus,
    pub parse_diagnostic: Option<ParseDiagnostic>,
    pub byte_len: u64,
    pub content_hash: String,
    pub references: Vec<ReferenceRecord>,
    pub alias_map: HashMap<String, String>,
    /// Seconds since UNIX epoch of the file's last modification time at index time.
    /// Used by stat_check_files for mtime comparison.
    pub mtime_secs: u64,
}

// ── Result type for stat checking ─────────────────────────────────────────────

/// Result of a stat-based freshness check of the loaded index.
pub struct StatCheckResult {
    /// Files whose on-disk mtime or size differs from the indexed values.
    pub changed: Vec<String>,
    /// Files in the index that no longer exist on disk.
    pub deleted: Vec<String>,
    /// Files on disk that are not in the index (new since snapshot was taken).
    pub new_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotResetReport {
    removed: Vec<PathBuf>,
    missing: Vec<PathBuf>,
}

impl SnapshotResetReport {
    pub fn removed_count(&self) -> usize {
        self.removed.len()
    }

    pub fn missing_count(&self) -> usize {
        self.missing.len()
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Serialize `index` to `index.bin` inside the project's data directory.
///
/// Uses an atomic write pattern (write to tmp, then rename) so a crash during
/// write never leaves a partially-written file.
///
/// Returns `Ok(())` on success. Non-fatal — caller logs and continues.
pub fn serialize_index(index: &LiveIndex, project_root: &Path) -> anyhow::Result<()> {
    let snapshot_input = capture_snapshot_build_input(index);
    serialize_captured_snapshot(snapshot_input, project_root)
}

fn capture_snapshot_build_input(index: &LiveIndex) -> SnapshotBuildInput {
    SnapshotBuildInput {
        files: index.files.clone(),
    }
}

fn serialize_captured_snapshot(
    snapshot_input: SnapshotBuildInput,
    project_root: &Path,
) -> anyhow::Result<()> {
    let snapshot = build_snapshot(snapshot_input, project_root);
    write_snapshot(snapshot, project_root)
}

pub fn serialize_shared_index(
    shared: &crate::live_index::store::SharedIndex,
    project_root: &Path,
) -> anyhow::Result<()> {
    let snapshot_input = {
        let guard = shared.read();
        capture_snapshot_build_input(&guard)
    };
    serialize_captured_snapshot(snapshot_input, project_root)
}

pub fn reset_snapshot_state(project_root: &Path) -> anyhow::Result<SnapshotResetReport> {
    let dir = paths::resolve_symforge_dir(project_root);
    let targets = [dir.join(INDEX_FILENAME), dir.join(INDEX_TMP_FILENAME)];
    let mut removed = Vec::new();
    let mut missing = Vec::new();

    for target in targets {
        match std::fs::remove_file(&target) {
            Ok(()) => removed.push(target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing.push(target),
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "removing snapshot reset target {}: {}",
                    target.display(),
                    error
                ));
            }
        }
    }

    Ok(SnapshotResetReport { removed, missing })
}

fn write_snapshot(snapshot: IndexSnapshot, project_root: &Path) -> anyhow::Result<()> {
    // Serialize with postcard
    let bytes = postcard::to_stdvec(&snapshot)?;

    let dir = paths::ensure_symforge_dir(project_root)?;

    // Atomic write: tmp file then rename
    let final_path = dir.join(INDEX_FILENAME);
    let tmp_path = dir.join(INDEX_TMP_FILENAME);

    std::fs::write(&tmp_path, &bytes).map_err(|e| {
        anyhow::anyhow!(
            "writing index snapshot tmp at {}: {}",
            tmp_path.display(),
            e
        )
    })?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| {
        anyhow::anyhow!(
            "renaming index snapshot {} -> {}: {}",
            tmp_path.display(),
            final_path.display(),
            e
        )
    })?;

    info!(
        bytes = bytes.len(),
        files = snapshot.files.len(),
        path = %final_path.display(),
        "index serialized to project data dir"
    );

    Ok(())
}

/// Load an `IndexSnapshot` from the project's data directory.
///
/// Returns `None` (not panic) on:
/// - file not found (first run or crash)
/// - version mismatch (schema upgrade)
/// - corrupt / truncated bytes
pub fn load_snapshot(project_root: &Path) -> Option<IndexSnapshot> {
    let path = paths::resolve_symforge_dir(project_root).join(INDEX_FILENAME);

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            // File not found is the normal case on first run
            return None;
        }
    };

    let snapshot: IndexSnapshot = match postcard::from_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            warn!("failed to deserialize index snapshot (corrupt?): {e}");
            return None;
        }
    };

    if snapshot.version != CURRENT_VERSION {
        warn!(
            "index snapshot version mismatch: got {}, expected {} — will re-index",
            snapshot.version, CURRENT_VERSION
        );
        return None;
    }

    Some(snapshot)
}

pub fn snapshot_to_live_index(snapshot: IndexSnapshot) -> LiveIndex {
    let mut files: HashMap<String, Arc<IndexedFile>> = HashMap::with_capacity(snapshot.files.len());

    for (path, snap_file) in snapshot.files {
        let indexed_file = IndexedFile {
            relative_path: snap_file.relative_path,
            language: snap_file.language,
            classification: snap_file.classification,
            content: snap_file.content,
            symbols: snap_file.symbols,
            parse_status: snap_file.parse_status,
            parse_diagnostic: snap_file.parse_diagnostic,
            byte_len: snap_file.byte_len,
            content_hash: snap_file.content_hash,
            references: snap_file.references,
            alias_map: snap_file.alias_map,
            mtime_secs: snap_file.mtime_secs,
        };
        files.insert(path, Arc::new(indexed_file));
    }

    let trigram_index = super::trigram::TrigramIndex::build_from_files(&files);

    let mut index = LiveIndex {
        files,
        loaded_at: Instant::now(),
        loaded_at_system: SystemTime::now(),
        load_duration: Duration::ZERO,
        cb_state: CircuitBreakerState::new(0.20),
        is_empty: false,
        load_source: IndexLoadSource::SnapshotRestore,
        snapshot_verify_state: SnapshotVerifyState::Pending,
        reverse_index: HashMap::new(),
        files_by_basename: HashMap::new(),
        files_by_dir_component: HashMap::new(),
        trigram_index,
        gitignore: None,
        skipped_files: Vec::new(),
        coupling_store: None,
        local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
    };
    index.rebuild_reverse_index();
    index.rebuild_path_indices();
    index
}

/// Stat-check all files in the index against disk to find changed/deleted/new files.
///
/// Compares `byte_len` and `mtime_secs` stored in the snapshot against current
/// filesystem metadata. Files with differing size or mtime are in `changed`.
/// Files with `ENOENT` go to `deleted`. Files on disk not in the index go to `new_files`.
pub fn stat_check_files(
    index: &LiveIndex,
    snapshot_mtimes: &HashMap<String, u64>,
    root: &Path,
) -> StatCheckResult {
    let verify_view = capture_verify_view(index);
    stat_check_files_from_view(&verify_view, snapshot_mtimes, root)
}

fn stat_check_files_from_view(
    verify_view: &VerifyIndexView,
    snapshot_mtimes: &HashMap<String, u64>,
    root: &Path,
) -> StatCheckResult {
    let known_paths: std::collections::HashSet<&str> = verify_view
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    let mut changed = Vec::new();
    let mut deleted = Vec::new();

    // Check each indexed file against disk
    for file in &verify_view.files {
        let abs_path = root.join(
            file.relative_path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        match std::fs::metadata(&abs_path) {
            Ok(meta) => {
                let on_disk_size = meta.len();
                let on_disk_mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let stored_mtime = snapshot_mtimes
                    .get(&file.relative_path)
                    .copied()
                    .unwrap_or(0);

                if on_disk_size != file.byte_len || on_disk_mtime != stored_mtime {
                    changed.push(file.relative_path.clone());
                }
            }
            Err(_) => {
                // File gone
                deleted.push(file.relative_path.clone());
            }
        }
    }

    // Find new files (on disk but not in index)
    let new_files = match crate::discovery::discover_files(root) {
        Ok(discovered) => discovered
            .into_iter()
            .filter(|df| !known_paths.contains(df.relative_path.as_str()))
            .map(|df| df.relative_path)
            .collect(),
        Err(e) => {
            warn!("stat_check_files: discover_files failed: {e}");
            Vec::new()
        }
    };

    StatCheckResult {
        changed,
        deleted,
        new_files,
    }
}

/// Select approximately `sample_pct` of files and check their content hashes.
///
/// Returns paths of files whose on-disk content hash differs from the index.
/// Default: 10% (pass 0.10).
pub fn spot_verify_sample(index: &LiveIndex, root: &Path, sample_pct: f64) -> Vec<String> {
    let verify_view = capture_verify_view(index);
    spot_verify_sample_from_view(&verify_view, root, sample_pct)
}

fn spot_verify_sample_from_view(
    verify_view: &VerifyIndexView,
    root: &Path,
    sample_pct: f64,
) -> Vec<String> {
    if verify_view.files.is_empty() {
        return Vec::new();
    }

    // Deterministic pseudo-random sample: every Nth file
    let total = verify_view.files.len();
    let sample_size = ((total as f64 * sample_pct).ceil() as usize)
        .max(1)
        .min(total);
    let step = if sample_size == 0 {
        1
    } else {
        total / sample_size
    };
    let step = step.max(1);

    let mut mismatches = Vec::new();

    for file in verify_view.files.iter().step_by(step) {
        let abs_path = root.join(
            file.relative_path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let on_disk_hash = crate::hash::digest_hex(&bytes);
        if on_disk_hash != file.content_hash {
            mismatches.push(file.relative_path.clone());
        }
    }

    mismatches
}

// ── FrecencyStore init hook ───────────────────────────────────────────────────

/// Open the per-workspace `FrecencyStore` and apply the graduated HEAD-change
/// reset policy at session startup.
///
/// Startup persistence is gated on the persistent collection policy. With
/// `SYMFORGE_FRECENCY` unset (the default session policy), this is a no-op and
/// the database is never touched at boot.
///
/// With persistent collection enabled:
///
/// 1. Open the SQLite store at `<project_root>/.symforge/frecency.db`,
///    creating the file and parent directory if missing.
/// 2. Look up the stored HEAD SHA from the previous session.
/// 3. Resolve the current HEAD via `git2`. If the project is not a git
///    repository (or git otherwise fails), silently no-op — the feature must
///    not break the tool it hooks into.
/// 4. Compute the commit distance between stored and current HEAD. A transient
///    `Err` here aborts the cycle and preserves the stored HEAD so the next
///    session retries; `Ok(None)` signals "unrelated history / branch change"
///    which the policy correctly maps to a zero reset.
/// 5. Apply the graduated policy via [`FrecencyStore::reset_or_halve_on_head_change`],
///    which also persists `current_head` as the new stored HEAD.
///
/// Any error along the happy path is silently dropped: a bad store, a git read
/// failure, or a SQLite transaction failure must never crash the live-index
/// boot path. The next session retries.
///
/// Spec: §"Reset-on-HEAD-change: graduated, not binary" on
/// `[[SymForge Frecency-Weighted File Ranking]]`.
pub fn init_frecency_store(project_root: &Path) {
    // Hook registration is unconditional — the hook body resolves collection
    // policy at call time, so a test that flips `SYMFORGE_FRECENCY` after boot
    // still sees edits follow the current policy.
    crate::live_index::frecency::ensure_bump_hook_registered();
    if crate::live_index::frecency::collection_policy_from_env()
        != crate::capability::FrecencyCollectionPolicy::Persistent
    {
        return;
    }
    let db_path = project_root.join(crate::paths::SYMFORGE_FRECENCY_DB_PATH);
    let _ = run_frecency_init(&db_path, project_root);
}

/// Body of [`init_frecency_store`] with the env-flag check stripped out.
///
/// Split so unit tests can drive the work against a known db path + git repo
/// without process-wide env mutation.
fn run_frecency_init(db_path: &Path, repo_root: &Path) -> Result<(), String> {
    let store =
        crate::live_index::frecency::FrecencyStore::open(db_path).map_err(|e| e.to_string())?;
    store.apply_head_reset_policy(repo_root)
}

// ── Private helpers ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct SnapshotBuildInput {
    files: HashMap<String, Arc<IndexedFile>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VerifyFileView {
    relative_path: String,
    byte_len: u64,
    content_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VerifyIndexView {
    files: Vec<VerifyFileView>,
}

fn capture_verify_view(index: &LiveIndex) -> VerifyIndexView {
    let mut files: Vec<VerifyFileView> = index
        .files
        .iter()
        .map(|(path, file)| VerifyFileView {
            relative_path: path.clone(),
            byte_len: file.byte_len,
            content_hash: file.content_hash.clone(),
        })
        .collect();
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    VerifyIndexView { files }
}

/// Convert captured live-index data to `IndexSnapshot`.
fn build_snapshot(snapshot_input: SnapshotBuildInput, project_root: &Path) -> IndexSnapshot {
    let mut snap_files = HashMap::with_capacity(snapshot_input.files.len());

    for (path, file) in snapshot_input.files {
        // Try to get mtime from disk for the snapshot
        let abs_path = project_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let mtime_secs = std::fs::metadata(&abs_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        snap_files.insert(
            path.clone(),
            IndexedFileSnapshot {
                relative_path: file.relative_path.clone(),
                language: file.language.clone(),
                classification: file.classification,
                content: file.content.clone(),
                symbols: file.symbols.clone(),
                parse_status: file.parse_status.clone(),
                parse_diagnostic: file.parse_diagnostic.clone(),
                byte_len: file.byte_len,
                content_hash: file.content_hash.clone(),
                references: file.references.clone(),
                alias_map: file.alias_map.clone(),
                mtime_secs,
            },
        );
    }

    IndexSnapshot {
        version: CURRENT_VERSION,
        files: snap_files,
    }
}

/// Background task: verify a loaded index against disk and re-parse stale files.
///
/// Run after `snapshot_to_live_index` to bring the index to current disk state.
/// Non-blocking for queries — writes are protected by the index's RwLock.
pub async fn background_verify(
    index: crate::live_index::store::SharedIndex,
    root: std::path::PathBuf,
    snapshot_mtimes: HashMap<String, u64>,
) {
    index.mark_snapshot_verify_running();

    // 1. Stat-check all files (fast: just metadata reads)
    let verify_view = {
        let guard = index.read();
        capture_verify_view(&guard)
    };
    let stat_result = stat_check_files_from_view(&verify_view, &snapshot_mtimes, &root);

    let changed_count = stat_result.changed.len();
    let deleted_count = stat_result.deleted.len();
    let new_count = stat_result.new_files.len();

    // 2. Remove deleted files
    if !stat_result.deleted.is_empty() {
        for path in &stat_result.deleted {
            index.remove_file(path);
        }
    }

    // 3. Re-parse changed files
    let to_reparse: Vec<String> = stat_result
        .changed
        .into_iter()
        .chain(stat_result.new_files.into_iter())
        .collect();

    for rel_path in &to_reparse {
        let abs_path = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(e) => {
                warn!("background_verify: failed to read {rel_path}: {e}");
                continue;
            }
        };

        // Detect language from path
        let ext = std::path::Path::new(rel_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let language = match crate::domain::LanguageId::from_extension(ext) {
            Some(lang) => lang,
            None => continue,
        };

        let result = crate::parsing::process_file_with_classification(
            rel_path,
            &bytes,
            language,
            FileClassification::for_code_path(rel_path),
        );
        let mtime_secs = std::fs::metadata(&abs_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let indexed_file = IndexedFile::from_parse_result(result, bytes).with_mtime(mtime_secs);

        index.update_file(rel_path.clone(), indexed_file);
    }

    // 4. Spot-verify sample (10%) for content hash mismatches
    let verify_view = {
        let guard = index.read();
        capture_verify_view(&guard)
    };
    let spot_mismatches = spot_verify_sample_from_view(&verify_view, &root, 0.10);

    let spot_count = spot_mismatches.len();

    // Re-parse spot-check mismatches
    for rel_path in &spot_mismatches {
        let abs_path = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(e) => {
                warn!("background_verify spot-check: failed to read {rel_path}: {e}");
                continue;
            }
        };

        let ext = std::path::Path::new(rel_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let language = match crate::domain::LanguageId::from_extension(ext) {
            Some(lang) => lang,
            None => continue,
        };

        let result = crate::parsing::process_file_with_classification(
            rel_path,
            &bytes,
            language,
            FileClassification::for_code_path(rel_path),
        );
        let mtime_secs = std::fs::metadata(&abs_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let indexed_file = IndexedFile::from_parse_result(result, bytes).with_mtime(mtime_secs);

        index.update_file(rel_path.clone(), indexed_file);
    }

    index.mark_snapshot_verify_completed();

    info!(
        "background verify complete: {} changed, {} deleted, {} new, {} spot-check mismatches",
        changed_count, deleted_count, new_count, spot_count
    );
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{
        IndexLoadSource, IndexedFile, ParseStatus, SnapshotVerifyState,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime};
    use tempfile::TempDir;

    mod git_test_helpers {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/git/test_helpers.rs"
        ));
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_symbol(name: &str) -> SymbolRecord {
        let byte_range = (0, 10);
        SymbolRecord {
            name: name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (0, 1),
            doc_byte_range: None,
        }
    }

    fn make_reference(name: &str) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (5, 10),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        }
    }

    fn make_indexed_file(path: &str, content: &[u8]) -> IndexedFile {
        let mut alias_map = HashMap::new();
        alias_map.insert("Alias".to_string(), "Original".to_string());
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.to_vec(),
            symbols: vec![make_symbol("my_func")],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: content.len() as u64,
            content_hash: crate::hash::digest_hex(content),
            references: vec![make_reference("other_func")],
            alias_map,
            mtime_secs: 0,
        }
    }

    fn make_live_index_with_files(files: Vec<(&str, &[u8])>) -> LiveIndex {
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        for (path, content) in files {
            file_map.insert(path.to_string(), Arc::new(make_indexed_file(path, content)));
        }
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
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
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
    }

    // ── Round-trip tests ──────────────────────────────────────────────────────

    #[test]
    fn test_round_trip_preserves_files_symbols_references_content() {
        let tmp = TempDir::new().unwrap();
        let content = b"fn my_func() { other_func(); }";
        let index =
            make_live_index_with_files(vec![("tests/generated/main.generated.rs", content)]);

        // Serialize
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        // Load
        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        // Verify
        assert_eq!(loaded.files.len(), 1);
        let file = loaded
            .files
            .get("tests/generated/main.generated.rs")
            .expect("file should be present");
        assert_eq!(file.content, content);
        assert_eq!(file.symbols.len(), 1);
        assert_eq!(file.symbols[0].name, "my_func");
        assert_eq!(file.references.len(), 1);
        assert_eq!(file.references[0].name, "other_func");
        assert!(file.classification.is_code());
        assert!(file.classification.is_test);
        assert!(file.classification.is_generated);
        assert_eq!(
            file.alias_map.get("Alias").map(|s| s.as_str()),
            Some("Original")
        );
    }

    #[test]
    fn test_round_trip_empty_index() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![]);

        serialize_index(&index, tmp.path()).expect("serialize empty index should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(loaded.files.len(), 0);
    }

    #[test]
    fn test_snapshot_to_live_index_marks_snapshot_restore_pending_verify() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}")]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(loaded.load_source(), IndexLoadSource::SnapshotRestore);
        assert_eq!(loaded.snapshot_verify_state(), SnapshotVerifyState::Pending);
    }

    #[tokio::test]
    async fn test_background_verify_marks_snapshot_verify_completed() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}\n").unwrap();

        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let snapshot_mtimes = snapshot
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.mtime_secs))
            .collect::<HashMap<_, _>>();
        let loaded = snapshot_to_live_index(snapshot);
        let shared = crate::live_index::SharedIndexHandle::shared(loaded);

        {
            let guard = shared.read();
            assert_eq!(guard.load_source(), IndexLoadSource::SnapshotRestore);
            assert_eq!(guard.snapshot_verify_state(), SnapshotVerifyState::Pending);
        }

        let before = shared.published_state();
        assert_eq!(before.file_count, 1);
        assert_eq!(before.partial_parse_count, 0);
        assert_eq!(before.failed_count, 0);

        background_verify(shared.clone(), tmp.path().to_path_buf(), snapshot_mtimes).await;

        let guard = shared.read();
        assert_eq!(guard.load_source(), IndexLoadSource::SnapshotRestore);
        assert_eq!(
            guard.snapshot_verify_state(),
            SnapshotVerifyState::Completed
        );
        drop(guard);

        let published = shared.published_state();
        assert_eq!(
            published.snapshot_verify_state,
            SnapshotVerifyState::Completed
        );
        assert!(
            published.generation >= 2,
            "expected published generation to advance through verify transitions"
        );
        assert_eq!(published.file_count, before.file_count);
        assert_eq!(published.partial_parse_count, before.partial_parse_count);
        assert_eq!(published.failed_count, before.failed_count);
    }

    #[tokio::test]
    async fn test_background_verify_deleted_file_changes_published_counts() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}\n").unwrap();

        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let snapshot_mtimes = snapshot
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.mtime_secs))
            .collect::<HashMap<_, _>>();
        let loaded = snapshot_to_live_index(snapshot);
        let shared = crate::live_index::SharedIndexHandle::shared(loaded);

        let before = shared.published_state();
        assert_eq!(before.file_count, 1);
        assert_eq!(before.partial_parse_count, 0);
        assert_eq!(before.failed_count, 0);

        std::fs::remove_file(&file_path).expect("remove indexed file");
        background_verify(shared.clone(), tmp.path().to_path_buf(), snapshot_mtimes).await;

        let published = shared.published_state();
        assert!(
            published.generation >= 2,
            "expected published generation to advance through verify transitions"
        );
        assert_eq!(
            published.snapshot_verify_state,
            SnapshotVerifyState::Completed
        );
        assert_eq!(published.file_count, 0);
        assert_eq!(published.parsed_count, 0);
        assert_eq!(published.partial_parse_count, 0);
        assert_eq!(published.failed_count, 0);
    }

    #[test]
    fn test_build_snapshot_resolves_mtime_against_project_root() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path().join("project");
        let file_path = project_root.join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}\n").unwrap();

        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        let snapshot = build_snapshot(capture_snapshot_build_input(&index), &project_root);

        let expected_mtime = std::fs::metadata(&file_path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert_eq!(
            snapshot.files.get("src/main.rs").unwrap().mtime_secs,
            expected_mtime
        );
    }

    #[test]
    fn test_capture_verify_view_sorts_paths() {
        let index = make_live_index_with_files(vec![
            ("src/z.rs", b"fn z() {}\n"),
            ("src/a.rs", b"fn a() {}\n"),
            ("src/m.rs", b"fn m() {}\n"),
        ]);

        let view = capture_verify_view(&index);
        let paths: Vec<&str> = view
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();

        assert_eq!(paths, vec!["src/a.rs", "src/m.rs", "src/z.rs"]);
    }

    #[test]
    fn test_round_trip_multiple_files() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![
            ("a.rs", b"fn alpha() {}"),
            ("b.rs", b"fn beta() {}"),
            ("c.py", b"def gamma(): pass"),
        ]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(loaded.files.len(), 3);
        assert!(loaded.files.contains_key("a.rs"));
        assert!(loaded.files.contains_key("b.rs"));
        assert!(loaded.files.contains_key("c.py"));
    }

    #[test]
    fn test_round_trip_preserves_parse_status_variants() {
        let tmp = TempDir::new().unwrap();
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();

        let partial_diagnostic = crate::domain::ParseDiagnostic {
            parser: "toml_edit".to_string(),
            message: "missing closing quote".to_string(),
            line: Some(4),
            column: Some(17),
            byte_span: Some((43, 56)),
            fallback_used: true,
        };
        let failed_diagnostic = crate::domain::ParseDiagnostic {
            parser: "toml_edit".to_string(),
            message: "invalid table header".to_string(),
            line: Some(1),
            column: Some(2),
            byte_span: Some((0, 8)),
            fallback_used: false,
        };

        file_map.insert(
            "ok.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "ok.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("ok.rs"),
                content: b"fn foo() {}".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: 11,
                content_hash: "hash1".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            }),
        );

        file_map.insert(
            "partial.toml".to_string(),
            Arc::new(IndexedFile {
                relative_path: "partial.toml".to_string(),
                language: LanguageId::Toml,
                classification: crate::domain::FileClassification::for_code_path("partial.toml"),
                content: b"[package]\nname = \"symforge\"\ninvalid = \"unterminated\n".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::PartialParse {
                    warning: partial_diagnostic.summary(),
                },
                parse_diagnostic: Some(partial_diagnostic.clone()),
                byte_len: 52,
                content_hash: "hash2".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            }),
        );

        file_map.insert(
            "fail.toml".to_string(),
            Arc::new(IndexedFile {
                relative_path: "fail.toml".to_string(),
                language: LanguageId::Toml,
                classification: crate::domain::FileClassification::for_code_path("fail.toml"),
                content: b"[invalid\nno closing".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Failed {
                    error: failed_diagnostic.summary(),
                },
                parse_diagnostic: Some(failed_diagnostic.clone()),
                byte_len: 19,
                content_hash: "hash3".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            }),
        );

        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
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
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("load should succeed");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(
            loaded.files.get("ok.rs").unwrap().parse_status,
            ParseStatus::Parsed
        );

        let partial = loaded.files.get("partial.toml").unwrap();
        assert!(matches!(
            partial.parse_status,
            ParseStatus::PartialParse { .. }
        ));
        assert_eq!(partial.parse_diagnostic, Some(partial_diagnostic));

        let failed = loaded.files.get("fail.toml").unwrap();
        assert!(matches!(failed.parse_status, ParseStatus::Failed { .. }));
        assert_eq!(failed.parse_diagnostic, Some(failed_diagnostic));
    }

    // ── Format pin: query equivalence across persist → restore ────────────────

    // Tripwire: bumping the persisted-index format version MUST be a deliberate
    // decision. If this assertion fails, the format is changing — stop and
    // escalate per `.octogent/tentacles/live-index/CONTEXT.md` §No-surprise rule.
    #[test]
    fn test_persist_format_version_is_pinned() {
        assert_eq!(
            CURRENT_VERSION, 4,
            "persist format version changed — a format bump breaks every existing \
             user's .symforge/index.bin and requires orchestrator approval"
        );
    }

    /// Round-trip regression on a non-trivial index spanning 3 languages, a
    /// cross-file reference, and a partial-parse diagnostic. Asserts that a
    /// representative set of public query functions returns identical results
    /// before and after `persist → restore`. This is the contract that protects
    /// existing users from silent format regressions.
    #[test]
    fn test_round_trip_preserves_query_equivalence_multilang_xref_partial() {
        use crate::domain::{ReferenceKind, SymbolKind};

        let tmp = TempDir::new().unwrap();

        // ── Build a non-trivial index ─────────────────────────────────────────
        // Rust file: defines `my_func`, calls `other_func` (xref into Python).
        let rust_content = b"fn my_func() { other_func(); }";
        let rust_symbol = SymbolRecord {
            name: "my_func".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 11),
            item_byte_range: Some((0, 30)),
            line_range: (0, 0),
            doc_byte_range: None,
        };
        let rust_xref = ReferenceRecord {
            name: "other_func".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (15, 25),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        };

        // Python file: defines `other_func`. Carries a partial-parse diagnostic.
        let python_content = b"def other_func():\n    pass\n";
        let python_symbol = SymbolRecord {
            name: "other_func".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 17),
            item_byte_range: Some((0, 27)),
            line_range: (0, 1),
            doc_byte_range: None,
        };
        let python_diagnostic = crate::domain::ParseDiagnostic {
            parser: "tree_sitter_python".to_string(),
            message: "unterminated decorator".to_string(),
            line: Some(1),
            column: Some(0),
            byte_span: Some((0, 3)),
            fallback_used: true,
        };

        // TypeScript file: defines `render`, no xrefs, parses cleanly.
        let ts_content = b"export function render(): void {}";
        let ts_symbol = SymbolRecord {
            name: "render".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (7, 29),
            item_byte_range: Some((0, 33)),
            line_range: (0, 0),
            doc_byte_range: None,
        };

        let mut alias_map = HashMap::new();
        alias_map.insert("Map".to_string(), "HashMap".to_string());

        let rust_file = IndexedFile {
            relative_path: "src/foo.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/foo.rs"),
            content: rust_content.to_vec(),
            symbols: vec![rust_symbol],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: rust_content.len() as u64,
            content_hash: crate::hash::digest_hex(rust_content),
            references: vec![rust_xref],
            alias_map,
            mtime_secs: 0,
        };
        let python_file = IndexedFile {
            relative_path: "src/bar.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("src/bar.py"),
            content: python_content.to_vec(),
            symbols: vec![python_symbol],
            parse_status: ParseStatus::PartialParse {
                warning: python_diagnostic.summary(),
            },
            parse_diagnostic: Some(python_diagnostic),
            byte_len: python_content.len() as u64,
            content_hash: crate::hash::digest_hex(python_content),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let ts_file = IndexedFile {
            relative_path: "src/baz.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/baz.ts"),
            content: ts_content.to_vec(),
            symbols: vec![ts_symbol],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: ts_content.len() as u64,
            content_hash: crate::hash::digest_hex(ts_content),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };

        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert("src/foo.rs".to_string(), Arc::new(rust_file));
        file_map.insert("src/bar.py".to_string(), Arc::new(python_file));
        file_map.insert("src/baz.ts".to_string(), Arc::new(ts_file));

        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut before = LiveIndex {
            files: file_map,
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
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        before.rebuild_reverse_index();
        before.rebuild_path_indices();

        // ── Persist, then restore ─────────────────────────────────────────────
        serialize_index(&before, tmp.path()).expect("serialize should succeed");

        // Tripwire on the serialized version field itself.
        let raw = std::fs::read(tmp.path().join(".symforge").join("index.bin")).unwrap();
        let decoded: IndexSnapshot =
            postcard::from_bytes(&raw).expect("persisted snapshot decodes");
        assert_eq!(
            decoded.version, CURRENT_VERSION,
            "serialized snapshot must carry CURRENT_VERSION"
        );

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let after = snapshot_to_live_index(snapshot);

        // ── Query equivalence ────────────────────────────────────────────────

        // Scalars
        assert_eq!(before.file_count(), after.file_count(), "file_count");
        assert_eq!(before.symbol_count(), after.symbol_count(), "symbol_count");

        // all_files() — sorted equivalence
        let mut before_all: Vec<(String, IndexedFile)> = before
            .all_files()
            .map(|(p, f)| (p.clone(), f.clone()))
            .collect();
        let mut after_all: Vec<(String, IndexedFile)> = after
            .all_files()
            .map(|(p, f)| (p.clone(), f.clone()))
            .collect();
        before_all.sort_by(|a, b| a.0.cmp(&b.0));
        after_all.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(before_all.len(), after_all.len(), "all_files len");
        for ((bp, bf), (ap, af)) in before_all.iter().zip(after_all.iter()) {
            assert_eq!(bp, ap, "all_files path");
            assert_eq!(bf.content, af.content, "content for {bp}");
            assert_eq!(bf.content_hash, af.content_hash, "hash for {bp}");
            assert_eq!(bf.language, af.language, "language for {bp}");
            assert_eq!(bf.symbols, af.symbols, "symbols for {bp}");
            assert_eq!(bf.references, af.references, "references for {bp}");
            assert_eq!(bf.parse_status, af.parse_status, "parse_status for {bp}");
            assert_eq!(
                bf.parse_diagnostic, af.parse_diagnostic,
                "parse_diagnostic for {bp}"
            );
            assert_eq!(bf.alias_map, af.alias_map, "alias_map for {bp}");
            assert_eq!(
                bf.classification, af.classification,
                "classification for {bp}"
            );
        }

        // Per-file: get_file + symbols_for_file
        for path in ["src/foo.rs", "src/bar.py", "src/baz.ts"] {
            let b = before.get_file(path).expect("before file present");
            let a = after.get_file(path).expect("after file present");
            assert_eq!(b.content, a.content, "get_file content {path}");
            assert_eq!(
                before.symbols_for_file(path),
                after.symbols_for_file(path),
                "symbols_for_file {path}"
            );
        }

        // Path indices rebuilt identically
        for basename in ["foo.rs", "bar.py", "baz.ts"] {
            assert_eq!(
                before.find_files_by_basename(basename),
                after.find_files_by_basename(basename),
                "find_files_by_basename {basename}"
            );
        }
        assert_eq!(
            before.find_files_by_dir_component("src"),
            after.find_files_by_dir_component("src"),
            "find_files_by_dir_component src"
        );

        // Cross-reference survives the round-trip (reverse index rebuilt from
        // persisted references).
        let before_refs: Vec<(String, ReferenceRecord)> = before
            .find_references_for_name("other_func", None, true)
            .into_iter()
            .map(|(p, r)| (p.to_string(), r.clone()))
            .collect();
        let after_refs: Vec<(String, ReferenceRecord)> = after
            .find_references_for_name("other_func", None, true)
            .into_iter()
            .map(|(p, r)| (p.to_string(), r.clone()))
            .collect();
        assert_eq!(before_refs.len(), 1, "one xref before round-trip");
        assert_eq!(before_refs, after_refs, "find_references_for_name xref");

        // Health stats: partial/failed breakdown and file/symbol counts.
        let bh = before.health_stats();
        let ah = after.health_stats();
        assert_eq!(bh.file_count, ah.file_count, "health file_count");
        assert_eq!(bh.symbol_count, ah.symbol_count, "health symbol_count");
        assert_eq!(bh.parsed_count, ah.parsed_count, "health parsed_count");
        assert_eq!(
            bh.partial_parse_count, ah.partial_parse_count,
            "health partial_parse_count"
        );
        assert_eq!(bh.failed_count, ah.failed_count, "health failed_count");
        assert_eq!(
            bh.partial_parse_files, ah.partial_parse_files,
            "health partial_parse_files"
        );
        assert_eq!(bh.failed_files, ah.failed_files, "health failed_files");
        assert_eq!(
            bh.partial_parse_count, 1,
            "test setup must include one partial-parse file"
        );

        // Repo outline: same files, languages, symbol counts.
        let bo = before.capture_repo_outline_view();
        let ao = after.capture_repo_outline_view();
        assert_eq!(bo.total_files, ao.total_files, "outline total_files");
        assert_eq!(bo.total_symbols, ao.total_symbols, "outline total_symbols");
        assert_eq!(bo.files, ao.files, "outline files");
    }

    // ── Version mismatch / corrupt data tests ─────────────────────────────────

    #[test]
    fn test_version_mismatch_returns_none() {
        let tmp = TempDir::new().unwrap();

        // Build a snapshot with a wrong version and serialize it manually
        let snapshot = IndexSnapshot {
            version: 999,
            files: HashMap::new(),
        };
        let bytes = postcard::to_stdvec(&snapshot).unwrap();
        let dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.bin"), &bytes).unwrap();

        // load_snapshot must return None, not panic
        let result = load_snapshot(tmp.path());
        assert!(result.is_none(), "version mismatch must return None");
    }

    #[test]
    fn test_corrupt_bytes_returns_none_no_panic() {
        let tmp = TempDir::new().unwrap();

        // Write random garbage
        let dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("index.bin"),
            b"not valid postcard data xyzzy 12345",
        )
        .unwrap();

        let result = load_snapshot(tmp.path());
        assert!(
            result.is_none(),
            "corrupt bytes must return None, not panic"
        );
    }

    #[test]
    fn test_truncated_bytes_returns_none_no_panic() {
        let tmp = TempDir::new().unwrap();

        // Serialize a real snapshot, then truncate it to half
        let index = make_live_index_with_files(vec![("a.rs", b"fn foo() {}")]);
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let bin_path = tmp.path().join(".symforge").join("index.bin");
        let full_bytes = std::fs::read(&bin_path).unwrap();
        let truncated = &full_bytes[..full_bytes.len() / 2];
        std::fs::write(&bin_path, truncated).unwrap();

        let result = load_snapshot(tmp.path());
        assert!(
            result.is_none(),
            "truncated bytes must return None, not panic"
        );
    }

    #[test]
    fn test_missing_file_returns_none() {
        let tmp = TempDir::new().unwrap();
        // No .symforge/index.bin exists
        let result = load_snapshot(tmp.path());
        assert!(result.is_none(), "missing file must return None");
    }

    // ── stat_check_files tests ────────────────────────────────────────────────

    #[test]
    fn test_stat_check_identifies_changed_file_by_size() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("a.rs");
        std::fs::write(&file_path, b"fn foo() {}").unwrap();

        // Build index with wrong byte_len to simulate a changed file
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "a.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "a.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("a.rs"),
                content: b"fn foo() {}".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: 999, // wrong size — simulates change
                content_hash: "old_hash".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
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
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        // mtime from disk
        let mtime = std::fs::metadata(&file_path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut mtimes: HashMap<String, u64> = HashMap::new();
        mtimes.insert("a.rs".to_string(), mtime);

        let result = stat_check_files(&index, &mtimes, tmp.path());
        assert!(
            result.changed.contains(&"a.rs".to_string()),
            "changed by size mismatch"
        );
        assert!(result.deleted.is_empty());
    }

    #[test]
    fn test_stat_check_identifies_deleted_file() {
        let tmp = TempDir::new().unwrap();

        // Index has a file that doesn't exist on disk
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "ghost.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "ghost.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("ghost.rs"),
                content: b"fn ghost() {}".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: 13,
                content_hash: "hash".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
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
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        let result = stat_check_files(&index, &HashMap::new(), tmp.path());
        assert!(
            result.deleted.contains(&"ghost.rs".to_string()),
            "missing file should be in deleted"
        );
    }

    #[test]
    fn test_stat_check_identifies_new_file() {
        let tmp = TempDir::new().unwrap();
        // Write a file on disk that's not in the index
        std::fs::write(tmp.path().join("new.rs"), b"fn new_func() {}").unwrap();

        // Empty index
        let index = make_live_index_with_files(vec![]);

        let result = stat_check_files(&index, &HashMap::new(), tmp.path());
        assert!(
            result.new_files.contains(&"new.rs".to_string()),
            "new file should be detected"
        );
    }

    // ── spot_verify_sample tests ──────────────────────────────────────────────

    #[test]
    fn test_spot_verify_catches_content_hash_mismatch() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("a.rs");
        // On-disk content is different from what's in the index
        std::fs::write(&file_path, b"fn modified() {}").unwrap();

        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "a.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "a.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("a.rs"),
                content: b"fn original() {}".to_vec(), // old content
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: 16,
                content_hash: crate::hash::digest_hex(b"fn original() {}"), // stale hash
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
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
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        // Sample 100% to ensure the file is included
        let mismatches = spot_verify_sample(&index, tmp.path(), 1.0);
        assert!(
            mismatches.contains(&"a.rs".to_string()),
            "hash mismatch should be detected"
        );
    }

    #[test]
    fn test_spot_verify_no_mismatch_when_hashes_match() {
        let tmp = TempDir::new().unwrap();
        let content = b"fn current() {}";
        let file_path = tmp.path().join("a.rs");
        std::fs::write(&file_path, content).unwrap();

        let hash = crate::hash::digest_hex(content);
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "a.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "a.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("a.rs"),
                content: content.to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: content.len() as u64,
                content_hash: hash,
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
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
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        let mismatches = spot_verify_sample(&index, tmp.path(), 1.0);
        assert!(mismatches.is_empty(), "no mismatch when hash is current");
    }

    #[test]
    fn test_spot_verify_empty_index_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![]);
        let mismatches = spot_verify_sample(&index, tmp.path(), 0.10);
        assert!(mismatches.is_empty(), "empty index returns empty vec");
    }

    // ── Snapshot atomicity test ───────────────────────────────────────────────

    #[test]
    fn test_serialize_creates_symforge_dir() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/lib.rs", b"fn lib() {}")]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        assert!(
            tmp.path().join(".symforge").join("index.bin").exists(),
            ".symforge/index.bin should be created"
        );
    }

    #[test]
    fn test_reset_snapshot_state_deletes_only_snapshot_scope() {
        let tmp = TempDir::new().unwrap();
        let symforge_dir = tmp.path().join(".symforge");
        let source_dir = tmp.path().join("src");
        std::fs::create_dir_all(&symforge_dir).expect("create .symforge");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        std::fs::write(source_dir.join("lib.rs"), "fn source_file() {}\n").expect("write source");
        std::fs::write(symforge_dir.join("index.bin"), b"stale snapshot").expect("write snapshot");
        std::fs::write(symforge_dir.join("index.bin.tmp"), b"stale tmp").expect("write tmp");
        std::fs::write(symforge_dir.join("frecency.db"), b"unrelated").expect("write sentinel");

        let report = reset_snapshot_state(tmp.path()).expect("reset snapshot state");

        assert_eq!(report.removed_count(), 2);
        assert!(!symforge_dir.join("index.bin").exists());
        assert!(!symforge_dir.join("index.bin.tmp").exists());
        assert!(
            symforge_dir.join("frecency.db").exists(),
            "reset must preserve unrelated .symforge state"
        );
        assert!(
            source_dir.join("lib.rs").exists(),
            "reset must never delete source files"
        );
    }

    // ── FrecencyStore init hook tests ─────────────────────────────────────────

    use crate::live_index::frecency::{FRECENCY_FLAG_ENV, FrecencyStore};
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    // Serialize tests that mutate FRECENCY_FLAG_ENV so a parallel runner (or a
    // sibling test that forgets to clear) cannot interleave env transitions.
    static FRECENCY_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    #[allow(unsafe_code)] // test-only flag helper runs under FRECENCY_ENV_LOCK.
    fn clear_frecency_flag() {
        // SAFETY: callers hold FRECENCY_ENV_LOCK and tests run with
        // --test-threads=1 per the project test policy.
        unsafe { std::env::remove_var(FRECENCY_FLAG_ENV) };
    }

    /// Commit `count` empty-tree commits to the repo at `root`, parenting each
    /// on the last commit of `HEAD`. Returns the SHA of the final commit.
    fn make_commits(root: &Path, count: usize, base_msg: &str) -> String {
        let repo = git2::Repository::open(root).expect("open test repo");
        let sig = git2::Signature::now("t", "t@x").expect("sig");
        let tree_id = {
            let mut idx = repo.index().expect("index");
            idx.write_tree().expect("write tree")
        };
        let tree = repo.find_tree(tree_id).expect("find tree");
        let mut head = repo
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel head");
        for i in 0..count {
            let oid = git_test_helpers::commit_head_with_retry(
                &repo,
                &sig,
                &sig,
                &format!("{base_msg} {i}"),
                &tree,
                &[&head],
            );
            head = repo.find_commit(oid).expect("find commit");
        }
        head.id().to_string()
    }

    /// Initialize a repo at `root` with one root commit. Returns that SHA.
    fn init_repo_with_root_commit(root: &Path) -> String {
        let repo = git2::Repository::init(root).expect("init");
        let sig = git2::Signature::now("t", "t@x").expect("sig");
        let tree_id = {
            let mut idx = repo.index().expect("index");
            idx.write_tree().expect("write tree")
        };
        let tree = repo.find_tree(tree_id).expect("find tree");
        let oid = git_test_helpers::commit_head_with_retry(&repo, &sig, &sig, "root", &tree, &[]);
        oid.to_string()
    }

    #[test]
    fn init_frecency_store_is_noop_when_flag_unset() {
        let _g = FRECENCY_ENV_LOCK.lock().unwrap();
        clear_frecency_flag();
        let tmp = TempDir::new().unwrap();
        init_frecency_store(tmp.path());
        assert!(
            !tmp.path().join(paths::SYMFORGE_FRECENCY_DB_PATH).exists(),
            "init must not create the frecency database when flag is unset"
        );
        assert!(
            !tmp.path().join(paths::SYMFORGE_DIR_NAME).exists(),
            "init must not create the .symforge directory when flag is unset"
        );
    }

    #[test]
    fn run_frecency_init_is_noop_when_project_root_is_not_a_repo() {
        // A path with no .git ancestry must degrade gracefully: the DB may be
        // opened (migrate is cheap), but no reset policy can apply since there
        // is no HEAD to read. We assert no last_head gets stored.
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("frecency.db");
        run_frecency_init(&db_path, tmp.path()).expect("init returns Ok on missing repo");
        let store = FrecencyStore::open(&db_path).unwrap();
        assert_eq!(
            store.last_head().unwrap(),
            None,
            "no HEAD should be recorded when the project root is not a git repo"
        );
    }

    #[test]
    fn run_frecency_init_records_head_on_first_session() {
        let tmp = TempDir::new().unwrap();
        let sha = init_repo_with_root_commit(tmp.path());
        let db_path = tmp.path().join("frecency.db");
        run_frecency_init(&db_path, tmp.path()).expect("init ok");
        let store = FrecencyStore::open(&db_path).unwrap();
        assert_eq!(store.last_head().unwrap().as_deref(), Some(sha.as_str()));
    }

    #[test]
    fn run_frecency_init_is_noop_when_head_unchanged() {
        let tmp = TempDir::new().unwrap();
        let sha = init_repo_with_root_commit(tmp.path());
        let db_path = tmp.path().join("frecency.db");
        // Seed: stored_head matches current, some bumps already exist.
        {
            let store = FrecencyStore::open(&db_path).unwrap();
            store.bump(&[PathBuf::from("src/a.rs")], 0).unwrap();
            store.bump(&[PathBuf::from("src/a.rs")], 0).unwrap();
            store
                .reset_or_halve_on_head_change(None, &sha, None)
                .unwrap();
        }
        run_frecency_init(&db_path, tmp.path()).expect("init ok");
        let store = FrecencyStore::open(&db_path).unwrap();
        assert_eq!(
            store.score(Path::new("src/a.rs"), 0).unwrap(),
            2.0,
            "same-HEAD init must not reset hit counts"
        );
        assert_eq!(store.last_head().unwrap().as_deref(), Some(sha.as_str()));
    }

    #[test]
    fn run_frecency_init_halves_at_100_commits() {
        let tmp = TempDir::new().unwrap();
        let first = init_repo_with_root_commit(tmp.path());
        let db_path = tmp.path().join("frecency.db");
        {
            let store = FrecencyStore::open(&db_path).unwrap();
            for _ in 0..10 {
                store.bump(&[PathBuf::from("src/a.rs")], 0).unwrap();
            }
            store
                .reset_or_halve_on_head_change(None, &first, None)
                .unwrap();
        }
        let _new_head = make_commits(tmp.path(), 100, "advance");
        run_frecency_init(&db_path, tmp.path()).expect("init ok");
        let store = FrecencyStore::open(&db_path).unwrap();
        assert_eq!(
            store.score(Path::new("src/a.rs"), 0).unwrap(),
            5.0,
            "100 commits falls into the 50..=500 band and must halve"
        );
    }

    #[test]
    fn run_frecency_init_zeros_above_500_commits() {
        let tmp = TempDir::new().unwrap();
        let first = init_repo_with_root_commit(tmp.path());
        let db_path = tmp.path().join("frecency.db");
        {
            let store = FrecencyStore::open(&db_path).unwrap();
            for _ in 0..10 {
                store.bump(&[PathBuf::from("src/a.rs")], 0).unwrap();
            }
            store
                .reset_or_halve_on_head_change(None, &first, None)
                .unwrap();
        }
        let _new_head = make_commits(tmp.path(), 501, "advance");
        run_frecency_init(&db_path, tmp.path()).expect("init ok");
        let store = FrecencyStore::open(&db_path).unwrap();
        assert_eq!(
            store.score(Path::new("src/a.rs"), 0).unwrap(),
            0.0,
            ">500 commits must zero hit counts"
        );
    }

    #[allow(unsafe_code)] // test-only flag mutation runs under FRECENCY_ENV_LOCK.
    #[test]
    fn init_frecency_store_with_flag_on_wires_boot_policy() {
        let _g = FRECENCY_ENV_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let sha = init_repo_with_root_commit(tmp.path());
        // SAFETY: test holds FRECENCY_ENV_LOCK; tests are --test-threads=1.
        unsafe { std::env::set_var(FRECENCY_FLAG_ENV, "1") };
        init_frecency_store(tmp.path());
        clear_frecency_flag();
        let db_path = tmp.path().join(paths::SYMFORGE_FRECENCY_DB_PATH);
        assert!(
            db_path.exists(),
            "flag=1 init must create the frecency database"
        );
        let store = FrecencyStore::open(&db_path).unwrap();
        assert_eq!(
            store.last_head().unwrap().as_deref(),
            Some(sha.as_str()),
            "flag=1 init must record current HEAD"
        );
    }

    #[test]
    fn test_serialize_idempotent() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("a.rs", b"fn a() {}")]);

        // Serialize twice — should succeed both times (no leftover .tmp)
        serialize_index(&index, tmp.path()).expect("first serialize should succeed");
        serialize_index(&index, tmp.path()).expect("second serialize should succeed");

        assert!(tmp.path().join(".symforge").join("index.bin").exists());
        // No tmp file should remain
        assert!(!tmp.path().join(".symforge").join("index.bin.tmp").exists());
    }
}
