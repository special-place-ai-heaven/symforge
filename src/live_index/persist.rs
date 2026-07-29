/// LiveIndex persistence: serialize on shutdown, load on startup.
///
/// Uses postcard (compact binary) for fast round-trips.
/// Atomic write (tmp → rename) to prevent corruption on crash.
/// Background verification corrects stale entries after loading a snapshot.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::domain::{
    CatalogEntry, CoverageStatus, FileClassification, FileDisposition, HistoryCoverage,
    HistoryLimit, LanguageId, ManifestResourceUsage, ProjectId, ProjectStateDir, ReferenceRecord,
    RepositoryFingerprint, RepositoryId, RepositoryManifest, SnapshotSourceIdentity, SourceId,
    SourceIdentity, SourceLocation, SourceVersion, StatePlacement, SymbolRecord, WorkingTreeState,
};
use crate::live_index::store::{
    CircuitBreakerState, CodeSignalsSnapshot, IndexLoadSource, IndexedFile, LiveIndex, ParseStatus,
    SnapshotVerifyState, normalize_root,
};
use crate::paths;

// ── Constants ─────────────────────────────────────────────────────────────────

use crate::domain::ParseDiagnostic;

const CURRENT_VERSION: u32 = 7;
const INDEX_FILENAME: &str = "index.bin";
const INDEX_TMP_FILENAME: &str = "index.bin.tmp";
const INDEX_TMP_PREFIX: &str = "index.bin.tmp.";
pub const SNAPSHOT_RESET_SCOPE_LABEL: &str = ".symforge/index.bin,.symforge/index.bin.tmp";
pub const CHECKPOINT_INTERVAL_ENV: &str = "SYMFORGE_CHECKPOINT_INTERVAL_SECS";
pub const MIN_CHECKPOINT_INTERVAL_SECS: u64 = 30;
pub const MAX_CHECKPOINT_INTERVAL_SECS: u64 = 3600;
type SnapshotPathLocks = HashMap<PathBuf, Weak<Mutex<()>>>;
static SNAPSHOT_PATH_LOCKS: OnceLock<Mutex<SnapshotPathLocks>> = OnceLock::new();
static SNAPSHOT_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn snapshot_path_and_lock(
    state_dir: &ProjectStateDir,
) -> anyhow::Result<(PathBuf, Arc<Mutex<()>>)> {
    if !state_dir.as_path().is_absolute() {
        anyhow::bail!(
            "project state directory must be absolute: {}",
            state_dir.as_path().display()
        );
    }
    std::fs::create_dir_all(state_dir.as_path()).map_err(|error| {
        anyhow::anyhow!(
            "creating project state directory {}: {}",
            state_dir.as_path().display(),
            error
        )
    })?;
    let canonical_dir = std::fs::canonicalize(state_dir.as_path()).map_err(|error| {
        anyhow::anyhow!(
            "canonicalizing project state directory {}: {}",
            state_dir.as_path().display(),
            error
        )
    })?;
    let snapshot_path = canonical_dir.join(INDEX_FILENAME);

    let registry = SNAPSHOT_PATH_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = registry
        .lock()
        .map_err(|_| anyhow::anyhow!("snapshot path-lock registry poisoned"))?;
    locks.retain(|_, lock| lock.strong_count() > 0);

    if let Some(lock) = locks.get(&snapshot_path).and_then(Weak::upgrade) {
        return Ok((snapshot_path, lock));
    }

    let lock = Arc::new(Mutex::new(()));
    locks.insert(snapshot_path.clone(), Arc::downgrade(&lock));
    Ok((snapshot_path, lock))
}

fn next_snapshot_temp_path(dir: &Path) -> PathBuf {
    let counter = SNAPSHOT_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(
        "{INDEX_TMP_PREFIX}{}.{counter}",
        std::process::id()
    ))
}

fn is_unique_snapshot_temp_name(name: &std::ffi::OsStr) -> bool {
    let Some(suffix) = name
        .to_str()
        .and_then(|name| name.strip_prefix(INDEX_TMP_PREFIX))
    else {
        return false;
    };
    let mut parts = suffix.split('.');
    let (Some(pid), Some(counter), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };

    !pid.is_empty()
        && !counter.is_empty()
        && pid.bytes().all(|byte| byte.is_ascii_digit())
        && counter.bytes().all(|byte| byte.is_ascii_digit())
}

fn cleanup_snapshot_temp(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!(
            path = %path.display(),
            "failed to clean snapshot temp file after write error: {error}"
        ),
    }
}

fn resolved_snapshot_state<'a>(
    source_root: &Path,
    placement: &'a StatePlacement,
) -> anyhow::Result<(&'a ProjectStateDir, ProjectId)> {
    let canonical_root = dunce::canonicalize(source_root).map_err(|error| {
        anyhow::anyhow!(
            "canonicalizing snapshot source root {}: {}",
            source_root.display(),
            error
        )
    })?;
    let expected_id = crate::discovery::project_id_for_canonical_root(&canonical_root);
    let directory = match placement {
        StatePlacement::ProjectLocal { directory } => directory,
        StatePlacement::UserLocal {
            directory, root_id, ..
        } => {
            if root_id != &expected_id {
                anyhow::bail!(
                    "project state placement identity mismatch: expected {}, got {}",
                    expected_id.0,
                    root_id.0
                );
            }
            directory
        }
        StatePlacement::MemoryOnly { .. } => {
            anyhow::bail!("project state persistence unavailable: memory-only placement")
        }
    };
    if !directory.as_path().is_absolute() {
        anyhow::bail!(
            "project state directory must be absolute: {}",
            directory.as_path().display()
        );
    }
    Ok((directory, expected_id))
}

// ── Snapshot types ────────────────────────────────────────────────────────────

/// Serializable snapshot of all per-file data in a `LiveIndex`.
///
/// Does NOT include non-serializable fields (Instant, AtomicUsize, RwLock).
/// Reverse index and trigram index are rebuilt from snapshot on load.
#[derive(Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub version: u32,
    pub project_id: ProjectId,
    pub source_identity: SnapshotSourceIdentity,
    pub files: HashMap<String, IndexedFileSnapshot>,
    pub manifest: RepositoryManifest,
    pub code_signals: PersistedCodeSignals,
}

/// Serializable provenance for the immutable code-signal slice of a published generation.
#[derive(Clone, Serialize, Deserialize)]
pub struct PersistedCodeSignals {
    pub temporal: super::git_temporal::GitTemporalIndex,
    pub computed_for_content_generation: u64,
    pub computed_for_source_version: SourceVersion,
    pub coverage: HistoryCoverage,
}

impl PersistedCodeSignals {
    fn from_published(code_signals: &CodeSignalsSnapshot) -> Self {
        Self {
            temporal: code_signals.temporal.as_ref().clone(),
            computed_for_content_generation: code_signals.computed_for_content_generation,
            computed_for_source_version: code_signals.computed_for_source_version.clone(),
            coverage: code_signals.coverage.as_ref().clone(),
        }
    }

    fn into_published(self) -> CodeSignalsSnapshot {
        let state = self.temporal.state.clone();
        CodeSignalsSnapshot {
            state,
            temporal: Arc::new(self.temporal),
            computed_for_content_generation: self.computed_for_content_generation,
            computed_for_source_version: self.computed_for_source_version,
            coverage: Arc::new(self.coverage),
        }
    }
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
    /// Used by `stat_check_files_from_view` for mtime comparison.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotWriteReport {
    pub path: PathBuf,
    pub bytes: usize,
    pub files: usize,
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
pub fn serialize_index(
    index: &LiveIndex,
    project_root: &Path,
    state_placement: &StatePlacement,
) -> anyhow::Result<()> {
    let snapshot_input = capture_snapshot_build_input(index);
    serialize_captured_snapshot(snapshot_input, project_root, state_placement).map(|_| ())
}

fn capture_snapshot_build_input(index: &LiveIndex) -> SnapshotBuildInput {
    SnapshotBuildInput {
        files: index.files.clone(),
        manifest_entries: index.manifest_entries.clone(),
        manifest: None,
        code_signals: None,
    }
}

fn indexed_content_digest<'a>(
    files: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> anyhow::Result<String> {
    let mut rows: Vec<(String, u64, String)> = files
        .map(|(path, content)| {
            (
                path.to_string(),
                content.len() as u64,
                crate::hash::digest_hex(content),
            )
        })
        .collect();
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    let mut identity = b"symforge-snapshot-resident-content-v1\0".to_vec();
    identity.extend_from_slice(&postcard::to_stdvec(&rows)?);
    Ok(crate::hash::digest_hex(&identity))
}

#[cfg(unix)]
fn native_path_identity_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn native_path_identity_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(not(any(unix, windows)))]
fn native_path_identity_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

fn digest_identity_parts(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut identity = domain.to_vec();
    for part in parts {
        identity.extend_from_slice(&(part.len() as u64).to_le_bytes());
        identity.extend_from_slice(part);
    }
    crate::hash::digest_hex(&identity)
}

fn safe_git_ref_name(reference: &git2::Reference<'_>) -> String {
    reference
        .name()
        .ok()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("bytes:{}", crate::hash::digest_hex(reference.name_bytes())))
}

fn git_working_tree_state(repository: &git2::Repository) -> WorkingTreeState {
    if repository.is_bare() {
        return WorkingTreeState::NotApplicable;
    }

    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let Ok(statuses) = repository.statuses(Some(&mut options)) else {
        return WorkingTreeState::Unknown;
    };
    let dirty = statuses.iter().any(|entry| {
        let path = entry.path_bytes();
        path != b".symforge" && !path.starts_with(b".symforge/")
    });
    if dirty {
        WorkingTreeState::Dirty
    } else {
        WorkingTreeState::Clean
    }
}

fn reachable_history_fingerprint(
    repository: &git2::Repository,
    tip: Option<git2::Oid>,
) -> anyhow::Result<String> {
    let mut object_ids = Vec::new();
    if let Some(tip) = tip {
        let mut walk = repository.revwalk()?;
        walk.push(tip)?;
        for object_id in walk {
            object_ids.push(object_id?.to_string());
        }
        object_ids.sort();
    }
    let mut identity = b"symforge-git-reachable-history-v1\0".to_vec();
    identity.extend_from_slice(&postcard::to_stdvec(&object_ids)?);
    Ok(crate::hash::digest_hex(&identity))
}

pub(crate) fn capture_history_coverage(project_root: &Path) -> HistoryCoverage {
    let Ok(repository) = git2::Repository::open(project_root) else {
        return HistoryCoverage {
            complete_to_root: false,
            limitations: vec![HistoryLimit::WorkingTreeOnly],
        };
    };
    if repository.is_shallow() {
        HistoryCoverage {
            complete_to_root: false,
            limitations: vec![HistoryLimit::Shallow],
        }
    } else if repository.head().is_ok() {
        HistoryCoverage {
            complete_to_root: true,
            limitations: Vec::new(),
        }
    } else {
        HistoryCoverage {
            complete_to_root: false,
            limitations: vec![HistoryLimit::Unavailable],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CapturedRepositorySource {
    pub source: SourceIdentity,
    pub source_version: SourceVersion,
    pub git_fingerprint: Option<RepositoryFingerprint>,
}

pub(crate) fn capture_repository_source(
    project_root: &Path,
    project_id: &ProjectId,
) -> anyhow::Result<CapturedRepositorySource> {
    let canonical_root = dunce::canonicalize(project_root)?;
    let Ok(repository) = git2::Repository::open(&canonical_root) else {
        let repository_id = RepositoryId::new(project_id.0.clone());
        let worktree_id = digest_identity_parts(
            b"symforge-non-git-worktree-id-v1\0",
            &[repository_id.as_str().as_bytes()],
        );
        let source_id = SourceId::new(digest_identity_parts(
            b"symforge-non-git-source-id-v1\0",
            &[repository_id.as_str().as_bytes(), worktree_id.as_bytes()],
        ));
        return Ok(CapturedRepositorySource {
            source: SourceIdentity {
                repository_id,
                source_id,
                location: SourceLocation::WorkingTree { worktree_id },
            },
            source_version: SourceVersion {
                branch: None,
                commit: None,
                working_tree: WorkingTreeState::NotApplicable,
            },
            git_fingerprint: None,
        });
    };

    let common_dir = dunce::canonicalize(repository.commondir())?;
    let repository_id = RepositoryId::new(digest_identity_parts(
        b"symforge-git-repository-id-v1\0",
        &[&native_path_identity_bytes(&common_dir)],
    ));
    let worktree_id = digest_identity_parts(
        b"symforge-git-worktree-id-v1\0",
        &[
            repository_id.as_str().as_bytes(),
            &native_path_identity_bytes(&canonical_root),
        ],
    );
    let source_id = SourceId::new(digest_identity_parts(
        b"symforge-git-source-id-v1\0",
        &[repository_id.as_str().as_bytes(), worktree_id.as_bytes()],
    ));

    let head = repository.head().ok();
    let tip = head.as_ref().and_then(git2::Reference::target);
    let selected_ref_or_head = head
        .as_ref()
        .map(safe_git_ref_name)
        .unwrap_or_else(|| "HEAD:unborn".to_string());
    let branch = head
        .as_ref()
        .filter(|reference| reference.is_branch())
        .map(safe_git_ref_name);
    let commit = tip.map(|object_id| object_id.to_string());
    let object_format = match commit.as_deref().map(str::len) {
        Some(64) => "sha256",
        _ => "sha1",
    }
    .to_string();

    Ok(CapturedRepositorySource {
        source: SourceIdentity {
            repository_id,
            source_id,
            location: SourceLocation::WorkingTree { worktree_id },
        },
        source_version: SourceVersion {
            branch,
            commit: commit.clone(),
            working_tree: git_working_tree_state(&repository),
        },
        git_fingerprint: Some(RepositoryFingerprint::Git {
            object_format,
            selected_ref_or_head,
            tip_object_id: commit.unwrap_or_default(),
            reachable_history_fingerprint: reachable_history_fingerprint(&repository, tip)?,
        }),
    })
}

fn capture_snapshot_source_identity(
    project_root: &Path,
    project_id: ProjectId,
    manifest_digest: String,
    indexed_content_digest: String,
) -> anyhow::Result<SnapshotSourceIdentity> {
    let captured = capture_repository_source(project_root, &project_id)?;
    Ok(snapshot_source_identity_from_captured(
        project_id,
        captured,
        manifest_digest,
        indexed_content_digest,
    ))
}

fn snapshot_source_identity_from_captured(
    project_id: ProjectId,
    captured: CapturedRepositorySource,
    manifest_digest: String,
    indexed_content_digest: String,
) -> SnapshotSourceIdentity {
    SnapshotSourceIdentity {
        project_id,
        repository_id: captured.source.repository_id,
        source_id: captured.source.source_id,
        source_version: captured.source_version,
        repository_fingerprint: captured.git_fingerprint.unwrap_or_else(|| {
            RepositoryFingerprint::NonGit {
                catalog_identity_digest: manifest_digest.clone(),
            }
        }),
        manifest_digest,
        indexed_content_digest,
    }
}

fn verify_snapshot_source_identity(
    snapshot: &IndexSnapshot,
    project_root: &Path,
) -> anyhow::Result<()> {
    if snapshot.source_identity.project_id != snapshot.project_id {
        anyhow::bail!("snapshot header project identity disagrees with its placement identity");
    }

    let rebuilt_manifest = RepositoryManifest::new(
        snapshot.manifest.schema_version,
        snapshot.manifest.policy_version,
        snapshot.manifest.secret_policy_version,
        snapshot.manifest.source.clone(),
        snapshot.manifest.source_version.clone(),
        snapshot.manifest.coverage,
        snapshot.manifest.entries.clone(),
        snapshot.manifest.issues.clone(),
        snapshot.manifest.usage,
    )?;
    if rebuilt_manifest.digest != snapshot.manifest.digest {
        anyhow::bail!("snapshot manifest digest failed canonical reconstruction");
    }
    let manifest_digest = snapshot.manifest.digest.clone();
    let content_digest = indexed_content_digest(
        snapshot
            .files
            .iter()
            .map(|(path, file)| (path.as_str(), file.content.as_slice())),
    )?;
    let current = capture_snapshot_source_identity(
        project_root,
        snapshot.project_id.clone(),
        manifest_digest,
        content_digest,
    )?;
    if current != snapshot.source_identity {
        anyhow::bail!(
            "snapshot source identity, source version, or repository fingerprint no longer matches"
        );
    }
    Ok(())
}

fn verify_snapshot_lineage_continuity(
    existing: &IndexSnapshot,
    current: &SnapshotSourceIdentity,
    project_root: &Path,
) -> anyhow::Result<()> {
    if existing.source_identity.repository_id != current.repository_id
        || existing.source_identity.source_id != current.source_id
    {
        anyhow::bail!("snapshot repository or stable source identity changed");
    }
    match (
        &existing.source_identity.repository_fingerprint,
        &current.repository_fingerprint,
    ) {
        (
            RepositoryFingerprint::Git {
                object_format: existing_format,
                tip_object_id,
                ..
            },
            RepositoryFingerprint::Git {
                object_format: current_format,
                ..
            },
        ) => {
            if existing_format != current_format {
                anyhow::bail!("Git object format changed");
            }
            if tip_object_id.is_empty() {
                if matches!(
                    &current.repository_fingerprint,
                    RepositoryFingerprint::Git { tip_object_id, .. } if tip_object_id.is_empty()
                ) {
                    return Ok(());
                }
                anyhow::bail!("unborn Git source changed before snapshot overwrite");
            }
            let repository = git2::Repository::open(project_root)?;
            let anchor = git2::Oid::from_str(tip_object_id)
                .map_err(|_| anyhow::anyhow!("stored Git anchor tip is invalid"))?;
            repository.find_commit(anchor).map_err(|_| {
                anyhow::anyhow!("stored Git anchor tip is absent from the live object database")
            })?;
            Ok(())
        }
        (RepositoryFingerprint::NonGit { .. }, RepositoryFingerprint::NonGit { .. }) => Ok(()),
        _ => anyhow::bail!("repository kind changed"),
    }
}

fn serialize_captured_snapshot(
    snapshot_input: SnapshotBuildInput,
    project_root: &Path,
    state_placement: &StatePlacement,
) -> anyhow::Result<SnapshotWriteReport> {
    let (state_dir, project_id) = resolved_snapshot_state(project_root, state_placement)?;
    let snapshot = build_snapshot(snapshot_input, project_root, project_id)?;
    write_snapshot(snapshot, state_dir, project_root)
}

pub fn serialize_shared_index(
    shared: &crate::live_index::store::SharedIndex,
    project_root: &Path,
    state_placement: &StatePlacement,
) -> anyhow::Result<()> {
    checkpoint_shared_index(shared, project_root, state_placement).map(|_| ())
}

pub fn checkpoint_shared_index(
    shared: &crate::live_index::store::SharedIndex,
    project_root: &Path,
    state_placement: &StatePlacement,
) -> anyhow::Result<SnapshotWriteReport> {
    let snapshot_input = {
        let published = shared.published_generation();
        SnapshotBuildInput {
            files: published.live.files.clone(),
            manifest_entries: published.live.manifest_entries.clone(),
            manifest: published.manifest.as_deref().cloned(),
            code_signals: Some(PersistedCodeSignals::from_published(
                published.code_signals.as_ref(),
            )),
        }
    };
    serialize_captured_snapshot(snapshot_input, project_root, state_placement)
}

pub fn checkpoint_interval_from_value(value: Option<&str>) -> Option<Duration> {
    let raw = value?.trim();
    if raw.is_empty()
        || raw == "0"
        || raw.eq_ignore_ascii_case("false")
        || raw.eq_ignore_ascii_case("off")
        || raw.eq_ignore_ascii_case("disabled")
    {
        return None;
    }

    let seconds = raw.parse::<u64>().ok()?;
    if seconds == 0 {
        return None;
    }

    Some(Duration::from_secs(seconds.clamp(
        MIN_CHECKPOINT_INTERVAL_SECS,
        MAX_CHECKPOINT_INTERVAL_SECS,
    )))
}

pub fn checkpoint_interval_from_env() -> Option<Duration> {
    checkpoint_interval_from_value(std::env::var(CHECKPOINT_INTERVAL_ENV).ok().as_deref())
}

pub fn reset_snapshot_state(
    project_root: &Path,
    state_placement: &StatePlacement,
) -> anyhow::Result<SnapshotResetReport> {
    let (state_dir, expected_project_id) = resolved_snapshot_state(project_root, state_placement)?;
    let (snapshot_path, snapshot_lock) = snapshot_path_and_lock(state_dir)?;
    let _reset_guard = snapshot_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("snapshot path lock poisoned"))?;

    if let Ok(existing_bytes) = std::fs::read(&snapshot_path) {
        let mismatch = match postcard::from_bytes::<IndexSnapshot>(&existing_bytes) {
            Ok(existing) if existing.version != CURRENT_VERSION => Some((
                "version-mismatch",
                format!(
                    "snapshot version {}, expected {}",
                    existing.version, CURRENT_VERSION
                ),
            )),
            Ok(existing)
                if existing.manifest.secret_policy_version
                    != crate::knowledge::SECRET_POLICY_VERSION =>
            {
                Some((
                    "secret-policy-mismatch",
                    format!(
                        "snapshot secret policy {}, expected {}",
                        existing.manifest.secret_policy_version,
                        crate::knowledge::SECRET_POLICY_VERSION
                    ),
                ))
            }
            Ok(existing) if existing.project_id != expected_project_id => Some((
                "project-id-mismatch",
                format!(
                    "snapshot project {}, expected {}",
                    existing.project_id.0, expected_project_id.0
                ),
            )),
            Ok(existing) => {
                let current = capture_snapshot_source_identity(
                    project_root,
                    expected_project_id.clone(),
                    existing.source_identity.manifest_digest.clone(),
                    existing.source_identity.indexed_content_digest.clone(),
                );
                match current.and_then(|current| {
                    verify_snapshot_lineage_continuity(&existing, &current, project_root)
                }) {
                    Ok(()) => None,
                    Err(error) => Some(("source-lineage-mismatch", error.to_string())),
                }
            }
            Err(error) => Some(("deserialize-error", error.to_string())),
        };
        if let Some((reason, detail)) = mismatch {
            let quarantine_path = quarantine_bad_snapshot_locked(
                state_dir,
                &snapshot_path,
                &existing_bytes,
                reason,
                detail,
            )?;
            anyhow::bail!(
                "refusing to reset foreign or invalid snapshot; preserved at {}",
                quarantine_path.display()
            );
        }
    }
    let dir = snapshot_path
        .parent()
        .expect("snapshot path must have a parent directory")
        .to_path_buf();
    let mut targets = vec![snapshot_path, dir.join(INDEX_TMP_FILENAME)];
    let entries = std::fs::read_dir(&dir).map_err(|error| {
        anyhow::anyhow!(
            "listing snapshot reset directory {}: {}",
            dir.display(),
            error
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            anyhow::anyhow!(
                "reading snapshot reset directory {}: {}",
                dir.display(),
                error
            )
        })?;
        if is_unique_snapshot_temp_name(&entry.file_name()) {
            targets.push(entry.path());
        }
    }
    targets.sort();
    targets.dedup();
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

// ── Team artifact (Program 015 S1a, C-S1A-005) — index.bin.zst export/import ──
//
// contracts/team-artifact.md (frozen 2026-06-30). Promoted in place from the
// SP-0B spike (former `spike_build_compressed` / `spike_compress_snapshot` /
// `spike_import_compressed` / `SpikeArtifactReport`, previously gated behind
// `cbm-spike` — see research.md § SP-0B): one implementation, no parallel
// spike-only path.
//
// R-14 (no secret leak): every function below only (de)serializes the
// snapshot already captured from `LiveIndex` — the same in-memory data
// `serialize_index` writes to `index.bin`. Nothing here re-walks the
// filesystem, so the artifact can never contain a path the discovery walk
// (`src/discovery/mod.rs`, `.gitignore` + hidden-file aware) had excluded.

/// Bare filename of the compressed team-artifact snapshot under `.symforge/`.
pub const ARTIFACT_FILENAME: &str = "index.bin.zst";
/// Bare filename of the artifact's sidecar metadata (content_hash etc.).
pub const ARTIFACT_METADATA_FILENAME: &str = "artifact.json";
const ARTIFACT_TMP_FILENAME: &str = "index.bin.zst.tmp";
/// Compression level for `checkpoint_now(export_artifact=true)` — the only
/// export path wired this sprint (contracts/team-artifact.md § Tiers, Best).
/// The Fast tier (level 3, watcher/periodic checkpoint) is part of the frozen
/// contract's tier vocabulary but has no caller yet — no sprint task wires a
/// background artifact export, so adding it now would be untested, unreachable
/// code. Add a `Fast` level constant + call site when that lands.
const ARTIFACT_BEST_ZSTD_LEVEL: i32 = 9;

/// Report of a completed team-artifact export ([`export_artifact`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactExportReport {
    pub path: PathBuf,
    pub metadata_path: PathBuf,
    pub files: usize,
    pub raw_bytes: usize,
    pub compressed_bytes: usize,
    pub content_hash: String,
    pub git_visibility: ArtifactGitVisibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactGitVisibility {
    AlreadyTracked,
    UntrackedVisible,
    IgnoredForceAddRequired,
    GitVisibilityUnavailable,
}

impl ArtifactGitVisibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyTracked => "already_tracked",
            Self::UntrackedVisible => "untracked_visible",
            Self::IgnoredForceAddRequired => "ignored_force_add_required",
            Self::GitVisibilityUnavailable => "git_visibility_unavailable",
        }
    }
}

fn artifact_git_visibility(project_root: &Path) -> ArtifactGitVisibility {
    let relative_path = Path::new(".symforge/index.bin.zst");
    let Ok(repository) = git2::Repository::open(project_root) else {
        return ArtifactGitVisibility::GitVisibilityUnavailable;
    };
    let Ok(index) = repository.index() else {
        return ArtifactGitVisibility::GitVisibilityUnavailable;
    };
    if index.get_path(relative_path, 0).is_some() {
        return ArtifactGitVisibility::AlreadyTracked;
    }
    match repository.status_should_ignore(relative_path) {
        Ok(true) => ArtifactGitVisibility::IgnoredForceAddRequired,
        Ok(false) => ArtifactGitVisibility::UntrackedVisible,
        Err(_) => ArtifactGitVisibility::GitVisibilityUnavailable,
    }
}

/// Per-file `content_hash` verification report for one in-memory zstd
/// export/import round trip (no disk I/O). Used by real-repo-scale regression
/// coverage (`tests/team_artifact_calibration.rs`) that must not touch a live
/// project's own `.symforge/` directory.
pub struct ArtifactRoundTripReport {
    pub files: usize,
    pub matched: usize,
    /// `"<path>: <before> != <after>"` (or `"<path>: missing"`) per failure.
    pub mismatches: Vec<String>,
    pub raw_bytes: usize,
    pub compressed_bytes: usize,
}

/// Build the snapshot, postcard-serialize, and zstd-compress it in memory.
/// Returns `(snapshot, raw_bytes, compressed_bytes)`.
fn build_compressed_snapshot(
    index: &LiveIndex,
    project_root: &Path,
) -> anyhow::Result<(IndexSnapshot, Vec<u8>, Vec<u8>)> {
    let canonical_root = dunce::canonicalize(project_root)?;
    let project_id = crate::discovery::project_id_for_canonical_root(&canonical_root);
    let snapshot = build_snapshot(
        capture_snapshot_build_input(index),
        project_root,
        project_id,
    )?;
    let raw = postcard::to_stdvec(&snapshot)?;
    let compressed = zstd::encode_all(raw.as_slice(), ARTIFACT_BEST_ZSTD_LEVEL)?;
    Ok((snapshot, raw, compressed))
}

/// Compress the current snapshot to in-memory `index.bin.zst` bytes (no disk
/// I/O). Building block for [`export_artifact`] and round-trip regression
/// coverage.
pub fn compress_snapshot(index: &LiveIndex, project_root: &Path) -> anyhow::Result<Vec<u8>> {
    Ok(build_compressed_snapshot(index, project_root)?.2)
}

/// Decompress zstd-compressed artifact bytes to raw postcard bytes (no
/// postcard decode, no disk I/O). Kept separate from postcard decode because
/// the Import flow (contracts/team-artifact.md) verifies `content_hash`
/// against these raw bytes *before* deserializing.
pub fn decompress_artifact_bytes(compressed: &[u8]) -> anyhow::Result<Vec<u8>> {
    zstd::decode_all(compressed).map_err(|e| anyhow::anyhow!("zstd decode failed: {e}"))
}

/// Full in-memory round trip: build -> zstd compress -> decompress ->
/// deserialize, then verify every per-file `content_hash` survived
/// byte-exact. No disk I/O — safe to run against a live project's own index
/// (see `tests/team_artifact_calibration.rs`).
pub fn artifact_round_trip_report(
    index: &LiveIndex,
    project_root: &Path,
) -> anyhow::Result<ArtifactRoundTripReport> {
    let (before, raw, compressed) = build_compressed_snapshot(index, project_root)?;
    let decompressed = decompress_artifact_bytes(&compressed)?;
    let after: IndexSnapshot = postcard::from_bytes(&decompressed)?;

    let mut matched = 0usize;
    let mut mismatches = Vec::new();
    for (path, bf) in &before.files {
        match after.files.get(path) {
            Some(af) if af.content_hash == bf.content_hash => matched += 1,
            Some(af) => mismatches.push(format!(
                "{path}: {} != {}",
                bf.content_hash, af.content_hash
            )),
            None => mismatches.push(format!("{path}: missing after round-trip")),
        }
    }

    Ok(ArtifactRoundTripReport {
        files: before.files.len(),
        matched,
        mismatches,
        raw_bytes: raw.len(),
        compressed_bytes: compressed.len(),
    })
}

/// Export the current index as the Best-tier `.symforge/index.bin.zst` team
/// artifact plus its `artifact.json` sidecar, and ensure the `.gitattributes`
/// `*.zst merge=ours` hint (A-US2-04) exists at `project_root`. Atomic write
/// (tmp -> rename), mirroring [`write_snapshot`].
pub fn export_artifact(
    index: &LiveIndex,
    project_root: &Path,
    access_mode: crate::domain::SourceAccessMode,
    state_placement: &crate::domain::StatePlacement,
    capability: &crate::domain::CapabilityStatus,
) -> anyhow::Result<ArtifactExportReport> {
    if !project_root.is_absolute() || !project_root.is_dir() {
        anyhow::bail!("team_artifact_export_refused: invalid_project_root");
    }
    if access_mode != crate::domain::SourceAccessMode::NormalProject {
        anyhow::bail!("team_artifact_export_refused: explicit_protected_source");
    }
    let crate::domain::StatePlacement::ProjectLocal { directory } = state_placement else {
        anyhow::bail!("team_artifact_export_refused: non_project_local_placement");
    };
    if directory.as_path() != paths::resolve_symforge_dir(project_root) {
        anyhow::bail!("team_artifact_export_refused: project_state_directory_mismatch");
    }
    if let crate::domain::CapabilityStatus::Unavailable { reason } = capability {
        anyhow::bail!("team_artifact_export_refused: capability_unavailable:{reason:?}");
    }

    let git_visibility = artifact_git_visibility(project_root);
    let (snapshot, raw, compressed) = build_compressed_snapshot(index, project_root)?;
    let file_count = snapshot.files.len();
    let content_hash = crate::hash::digest_hex(&raw);

    let dir = paths::ensure_symforge_dir(project_root)?;
    let final_path = dir.join(ARTIFACT_FILENAME);
    let tmp_path = dir.join(ARTIFACT_TMP_FILENAME);
    std::fs::write(&tmp_path, &compressed).map_err(|e| {
        anyhow::anyhow!("writing team artifact tmp at {}: {}", tmp_path.display(), e)
    })?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| {
        anyhow::anyhow!(
            "renaming team artifact {} -> {}: {}",
            tmp_path.display(),
            final_path.display(),
            e
        )
    })?;

    let metadata_path = dir.join(ARTIFACT_METADATA_FILENAME);
    let metadata = serde_json::json!({
        "content_hash": content_hash,
        "raw_bytes": raw.len(),
        "compressed_bytes": compressed.len(),
        "files": file_count,
    });
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&metadata)?).map_err(|e| {
        anyhow::anyhow!(
            "writing team artifact metadata at {}: {}",
            metadata_path.display(),
            e
        )
    })?;

    ensure_gitattributes_merge_hint(project_root)?;

    info!(
        bytes = compressed.len(),
        files = file_count,
        path = %final_path.display(),
        "team artifact exported"
    );

    Ok(ArtifactExportReport {
        path: final_path,
        metadata_path,
        files: file_count,
        raw_bytes: raw.len(),
        compressed_bytes: compressed.len(),
        content_hash,
        git_visibility,
    })
}

/// Ensure `project_root/.gitattributes` carries the `*.zst merge=ours` hint
/// (contracts/team-artifact.md § Paths) so merging the shared artifact always
/// keeps the current branch's copy instead of a line-diff conflict on binary
/// content. Idempotent: appends only if the line is not already present;
/// preserves any existing file content byte-for-byte otherwise.
fn ensure_gitattributes_merge_hint(project_root: &Path) -> anyhow::Result<()> {
    const HINT_LINE: &str = "*.zst merge=ours";
    let path = project_root.join(".gitattributes");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == HINT_LINE) {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(HINT_LINE);
    updated.push('\n');
    std::fs::write(&path, updated)
        .map_err(|e| anyhow::anyhow!("writing .gitattributes at {}: {}", path.display(), e))?;
    Ok(())
}

/// Read the `content_hash` field out of an `artifact.json` sidecar, if it
/// parses. Returns `None` when the sidecar is missing, is invalid JSON, or
/// lacks the field. Per contracts/team-artifact.md § Integrity failure, the
/// caller ([`import_artifact`]) treats `None` as an integrity failure: the
/// artifact's `content_hash` cannot be verified, so it is quarantined and the
/// daemon falls back to a full index build rather than trusting an
/// unverifiable payload.
fn read_artifact_content_hash(metadata_path: &Path) -> Option<String> {
    let bytes = std::fs::read(metadata_path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value.get("content_hash")?.as_str().map(str::to_string)
}

/// Import the team artifact (`.symforge/index.bin.zst`) per
/// contracts/team-artifact.md § Import flow: decompress, verify
/// `content_hash` against the `artifact.json` sidecar, then deserialize. The
/// returned snapshot rehydrates via the same `snapshot_to_live_index` +
/// stat-check path an `index.bin` load already uses (see `load_snapshot`).
///
/// Any integrity failure (corrupt zstd frame, hash mismatch, corrupt
/// postcard, version mismatch) quarantines the artifact under
/// `.symforge/quarantine/artifacts/` and returns `None`, so the caller falls
/// back to a full cold re-index (contract § Integrity failure).
fn import_artifact(project_root: &Path, expected_project_id: &ProjectId) -> Option<IndexSnapshot> {
    let dir = paths::resolve_symforge_dir(project_root);
    let artifact_path = dir.join(ARTIFACT_FILENAME);
    let metadata_path = dir.join(ARTIFACT_METADATA_FILENAME);

    let compressed = std::fs::read(&artifact_path).ok()?;

    let raw = match decompress_artifact_bytes(&compressed) {
        Ok(raw) => raw,
        Err(e) => {
            warn!("failed to decompress team artifact (corrupt zst?): {e}");
            try_quarantine_bad_artifact(
                project_root,
                &artifact_path,
                &metadata_path,
                &compressed,
                "zstd-decode-error",
                e.to_string(),
            );
            return None;
        }
    };

    // contracts/team-artifact.md § Import flow requires "verify content_hash"
    // BEFORE loading. A missing/unparseable sidecar means the hash cannot be
    // verified — reachable when a crash (or partial checkout) lands between
    // `export_artifact`'s artifact rename and its separate, non-atomic sidecar
    // write. Do NOT silently trust an unverifiable payload: treat it as an
    // integrity failure (§ Integrity failure), quarantine, and fall back to a
    // full index build.
    let Some(expected_hash) = read_artifact_content_hash(&metadata_path) else {
        warn!(
            path = %artifact_path.display(),
            metadata_path = %metadata_path.display(),
            "team artifact sidecar missing or unparseable — content_hash unverifiable; quarantining and falling back to a full index build"
        );
        try_quarantine_bad_artifact(
            project_root,
            &artifact_path,
            &metadata_path,
            &compressed,
            "missing-sidecar",
            format!(
                "artifact.json missing or unparseable at {}",
                metadata_path.display()
            ),
        );
        return None;
    };

    let actual_hash = crate::hash::digest_hex(&raw);
    if actual_hash != expected_hash {
        warn!(
            "team artifact content_hash mismatch (expected {expected_hash}, got {actual_hash}) — quarantining"
        );
        try_quarantine_bad_artifact(
            project_root,
            &artifact_path,
            &metadata_path,
            &compressed,
            "content-hash-mismatch",
            format!("expected {expected_hash}, got {actual_hash}"),
        );
        return None;
    }

    match postcard::from_bytes::<IndexSnapshot>(&raw) {
        Ok(snapshot)
            if snapshot.version == CURRENT_VERSION
                && snapshot.manifest.secret_policy_version
                    != crate::knowledge::SECRET_POLICY_VERSION =>
        {
            warn!(
                "team artifact secret policy mismatch: got {}, expected {} — will cold re-index",
                snapshot.manifest.secret_policy_version,
                crate::knowledge::SECRET_POLICY_VERSION
            );
            try_quarantine_bad_artifact(
                project_root,
                &artifact_path,
                &metadata_path,
                &compressed,
                "secret-policy-mismatch",
                format!(
                    "snapshot secret policy {}, expected {}",
                    snapshot.manifest.secret_policy_version,
                    crate::knowledge::SECRET_POLICY_VERSION
                ),
            );
            None
        }
        Ok(snapshot) if snapshot.version == CURRENT_VERSION => {
            let identity_result = if &snapshot.project_id != expected_project_id {
                Err(anyhow::anyhow!(
                    "team artifact project {}, expected {}",
                    snapshot.project_id.0,
                    expected_project_id.0
                ))
            } else {
                verify_snapshot_source_identity(&snapshot, project_root)
            };
            if let Err(error) = identity_result {
                warn!(
                    detail = %error,
                    "team artifact source identity mismatch; quarantining foreign or stale state"
                );
                try_quarantine_bad_artifact(
                    project_root,
                    &artifact_path,
                    &metadata_path,
                    &compressed,
                    "source-identity-mismatch",
                    error.to_string(),
                );
                return None;
            }
            Some(snapshot)
        }
        Ok(snapshot) => {
            warn!(
                "team artifact snapshot version mismatch: got {}, expected {} — will cold re-index",
                snapshot.version, CURRENT_VERSION
            );
            try_quarantine_bad_artifact(
                project_root,
                &artifact_path,
                &metadata_path,
                &compressed,
                "version-mismatch",
                format!(
                    "snapshot version {}, expected {}",
                    snapshot.version, CURRENT_VERSION
                ),
            );
            None
        }
        Err(e) => {
            warn!("failed to deserialize team artifact (corrupt postcard?): {e}");
            try_quarantine_bad_artifact(
                project_root,
                &artifact_path,
                &metadata_path,
                &compressed,
                "postcard-decode-error",
                e.to_string(),
            );
            None
        }
    }
}

fn quarantine_bad_artifact(
    project_root: &Path,
    artifact_path: &Path,
    metadata_path: &Path,
    compressed_bytes: &[u8],
    reason: &str,
    detail: String,
) -> anyhow::Result<PathBuf> {
    let dir = paths::ensure_artifact_quarantine_dir(project_root)?;
    let hash = crate::hash::digest_hex(compressed_bytes);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let name = format!(
        "{}-{:09}-{}-{}",
        now.as_secs(),
        now.subsec_nanos(),
        &hash[..16],
        reason
    );
    let quarantine_path = dir.join(format!("{name}.zst"));
    let metadata_out_path = dir.join(format!("{name}.json"));

    std::fs::write(&quarantine_path, compressed_bytes).map_err(|e| {
        anyhow::anyhow!(
            "writing quarantined team artifact at {}: {}",
            quarantine_path.display(),
            e
        )
    })?;

    let quarantine_metadata = serde_json::json!({
        "source_path": artifact_path.to_string_lossy(),
        "quarantine_path": quarantine_path.to_string_lossy(),
        "reason": reason,
        "detail": detail,
        "sha256": hash,
        "bytes": compressed_bytes.len(),
        "quarantined_at_unix_seconds": now.as_secs(),
        "quarantined_at_unix_nanos": now.subsec_nanos(),
    });
    let metadata_bytes = serde_json::to_vec_pretty(&quarantine_metadata)?;
    std::fs::write(&metadata_out_path, metadata_bytes).map_err(|e| {
        anyhow::anyhow!(
            "writing team artifact quarantine metadata at {}: {}",
            metadata_out_path.display(),
            e
        )
    })?;

    // Remove the bad artifact (+ its sidecar) so a later cold start does not
    // retry the same corrupt artifact forever.
    match std::fs::remove_file(artifact_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!(
            path = %artifact_path.display(),
            "failed to remove bad team artifact after quarantine: {error}"
        ),
    }
    let _ = std::fs::remove_file(metadata_path);

    Ok(quarantine_path)
}

fn try_quarantine_bad_artifact(
    project_root: &Path,
    artifact_path: &Path,
    metadata_path: &Path,
    compressed_bytes: &[u8],
    reason: &str,
    detail: String,
) {
    match quarantine_bad_artifact(
        project_root,
        artifact_path,
        metadata_path,
        compressed_bytes,
        reason,
        detail,
    ) {
        Ok(quarantine_path) => warn!(
            path = %artifact_path.display(),
            quarantine_path = %quarantine_path.display(),
            reason = reason,
            "bad team artifact quarantined"
        ),
        Err(error) => warn!(
            path = %artifact_path.display(),
            reason = reason,
            "failed to quarantine bad team artifact: {error}"
        ),
    }
}

fn write_snapshot(
    snapshot: IndexSnapshot,
    state_dir: &ProjectStateDir,
    project_root: &Path,
) -> anyhow::Result<SnapshotWriteReport> {
    // Serialize with postcard
    let bytes = postcard::to_stdvec(&snapshot)?;
    let file_count = snapshot.files.len();

    let (final_path, write_lock) = snapshot_path_and_lock(state_dir)?;
    let _write_guard = write_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("snapshot path lock poisoned"))?;

    match std::fs::read(&final_path) {
        Ok(existing_bytes) => {
            let mismatch = match postcard::from_bytes::<IndexSnapshot>(&existing_bytes) {
                Ok(existing) => {
                    if existing.version != CURRENT_VERSION {
                        Some((
                            "version-mismatch",
                            format!(
                                "snapshot version {}, expected {}",
                                existing.version, CURRENT_VERSION
                            ),
                        ))
                    } else if existing.project_id != snapshot.project_id {
                        Some((
                            "project-id-mismatch",
                            format!(
                                "snapshot project {}, expected {}",
                                existing.project_id.0, snapshot.project_id.0
                            ),
                        ))
                    } else {
                        verify_snapshot_lineage_continuity(
                            &existing,
                            &snapshot.source_identity,
                            project_root,
                        )
                        .err()
                        .map(|error| ("source-identity-mismatch", error.to_string()))
                    }
                }
                Err(error) => Some(("deserialize-error", error.to_string())),
            };
            if let Some((reason, detail)) = mismatch {
                let quarantine_path = quarantine_bad_snapshot_locked(
                    state_dir,
                    &final_path,
                    &existing_bytes,
                    reason,
                    detail,
                )?;
                anyhow::bail!(
                    "refusing to overwrite foreign or invalid snapshot; preserved at {}",
                    quarantine_path.display()
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(anyhow::anyhow!(
                "reading existing snapshot identity at {}: {}",
                final_path.display(),
                error
            ));
        }
    }

    let dir = final_path
        .parent()
        .expect("snapshot path must have a parent directory");

    // Atomic write: tmp file then rename
    let tmp_path = next_snapshot_temp_path(dir);

    if let Err(error) = std::fs::write(&tmp_path, &bytes) {
        cleanup_snapshot_temp(&tmp_path);
        return Err(anyhow::anyhow!(
            "writing index snapshot tmp at {}: {}",
            tmp_path.display(),
            error
        ));
    }
    if let Err(error) = std::fs::rename(&tmp_path, &final_path) {
        cleanup_snapshot_temp(&tmp_path);
        return Err(anyhow::anyhow!(
            "renaming index snapshot {} -> {}: {}",
            tmp_path.display(),
            final_path.display(),
            error
        ));
    }

    info!(
        bytes = bytes.len(),
        files = file_count,
        path = %final_path.display(),
        "index serialized to project data dir"
    );

    Ok(SnapshotWriteReport {
        path: final_path,
        bytes: bytes.len(),
        files: file_count,
    })
}

fn quarantine_bad_snapshot(
    state_dir: &ProjectStateDir,
    snapshot_path: &Path,
    bytes: &[u8],
    reason: &str,
    detail: String,
) -> anyhow::Result<PathBuf> {
    // Recovered finding #10: quarantine removes the ACTIVE snapshot below, so
    // it must hold the same per-path lock as `write_snapshot` /
    // `reset_snapshot_state`; without it, a concurrent atomic publish can have
    // its freshly renamed `index.bin` deleted out from under it. No caller
    // holds this lock when quarantining (load/verify paths are lock-free), so
    // this cannot self-deadlock.
    let (_canonical_snapshot_path, write_lock) = snapshot_path_and_lock(state_dir)?;
    let _write_guard = write_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("snapshot path lock poisoned"))?;
    quarantine_bad_snapshot_locked(state_dir, snapshot_path, bytes, reason, detail)
}

fn quarantine_bad_snapshot_locked(
    state_dir: &ProjectStateDir,
    snapshot_path: &Path,
    bytes: &[u8],
    reason: &str,
    detail: String,
) -> anyhow::Result<PathBuf> {
    let dir = state_dir
        .as_path()
        .join("quarantine")
        .join("index-snapshots");
    std::fs::create_dir_all(&dir).map_err(|error| {
        anyhow::anyhow!(
            "creating snapshot quarantine directory {}: {}",
            dir.display(),
            error
        )
    })?;
    let hash = crate::hash::digest_hex(bytes);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let name = format!(
        "{}-{:09}-{}-{}",
        now.as_secs(),
        now.subsec_nanos(),
        &hash[..16],
        reason
    );
    let quarantine_path = dir.join(format!("{name}.bin"));
    let metadata_path = dir.join(format!("{name}.json"));

    std::fs::write(&quarantine_path, bytes).map_err(|e| {
        anyhow::anyhow!(
            "writing quarantined index snapshot at {}: {}",
            quarantine_path.display(),
            e
        )
    })?;

    let metadata = serde_json::json!({
        "source_path": snapshot_path.to_string_lossy(),
        "quarantine_path": quarantine_path.to_string_lossy(),
        "reason": reason,
        "detail": detail,
        "sha256": hash,
        "bytes": bytes.len(),
        "quarantined_at_unix_seconds": now.as_secs(),
        "quarantined_at_unix_nanos": now.subsec_nanos(),
    });
    let metadata_bytes = serde_json::to_vec_pretty(&metadata)?;
    std::fs::write(&metadata_path, metadata_bytes).map_err(|e| {
        anyhow::anyhow!(
            "writing index snapshot quarantine metadata at {}: {}",
            metadata_path.display(),
            e
        )
    })?;

    match std::fs::read(snapshot_path) {
        Ok(current) if crate::hash::digest_hex(&current) == hash => {
            if let Err(error) = std::fs::remove_file(snapshot_path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                warn!(
                    path = %snapshot_path.display(),
                    quarantine_path = %quarantine_path.display(),
                    "failed to remove active bad index snapshot after quarantine: {error}"
                );
            }
        }
        Ok(_) => warn!(
            path = %snapshot_path.display(),
            quarantine_path = %quarantine_path.display(),
            "active snapshot changed during quarantine; preserved the newer active bytes"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!(
            path = %snapshot_path.display(),
            quarantine_path = %quarantine_path.display(),
            "failed to re-read active bad snapshot after quarantine: {error}"
        ),
    }

    Ok(quarantine_path)
}

fn try_quarantine_bad_snapshot(
    state_dir: &ProjectStateDir,
    snapshot_path: &Path,
    bytes: &[u8],
    reason: &str,
    detail: String,
) {
    match quarantine_bad_snapshot(state_dir, snapshot_path, bytes, reason, detail) {
        Ok(quarantine_path) => warn!(
            path = %snapshot_path.display(),
            quarantine_path = %quarantine_path.display(),
            reason = reason,
            "bad index snapshot quarantined"
        ),
        Err(error) => warn!(
            path = %snapshot_path.display(),
            reason = reason,
            "failed to quarantine bad index snapshot: {error}"
        ),
    }
}

/// Load an `IndexSnapshot` from the project's data directory.
///
/// Returns `None` (not panic) on:
/// - file not found (first run or crash)
/// - version mismatch (schema upgrade)
/// - corrupt / truncated bytes
pub fn load_snapshot(
    project_root: &Path,
    state_placement: &StatePlacement,
) -> Option<IndexSnapshot> {
    let (state_dir, expected_project_id) =
        resolved_snapshot_state(project_root, state_placement).ok()?;
    let path = state_dir.as_path().join(INDEX_FILENAME);

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // contracts/team-artifact.md § Import flow: index.bin missing but a
            // team-shared index.bin.zst artifact may be present — import it
            // instead of dropping straight to a full cold re-index. Absent
            // artifact too (the common first-run case) falls through to `None`.
            let snapshot = match state_placement {
                StatePlacement::ProjectLocal { .. } => {
                    import_artifact(project_root, &expected_project_id)
                }
                StatePlacement::UserLocal { .. } | StatePlacement::MemoryOnly { .. } => None,
            }?;
            return Some(snapshot);
        }
        Err(_) => {
            // Any other read failure (permissions, etc.) is the pre-existing
            // "give up, caller re-indexes" behavior.
            return None;
        }
    };

    let snapshot: IndexSnapshot = match postcard::from_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            warn!("failed to deserialize index snapshot (corrupt?): {e}");
            try_quarantine_bad_snapshot(
                state_dir,
                &path,
                &bytes,
                "deserialize-error",
                e.to_string(),
            );
            return None;
        }
    };

    if snapshot.version != CURRENT_VERSION {
        warn!(
            "index snapshot version mismatch: got {}, expected {} — will re-index",
            snapshot.version, CURRENT_VERSION
        );
        try_quarantine_bad_snapshot(
            state_dir,
            &path,
            &bytes,
            "version-mismatch",
            format!(
                "snapshot version {}, expected {}",
                snapshot.version, CURRENT_VERSION
            ),
        );
        return None;
    }

    if snapshot.manifest.secret_policy_version != crate::knowledge::SECRET_POLICY_VERSION {
        warn!(
            "index snapshot secret policy mismatch: got {}, expected {} — will re-scout",
            snapshot.manifest.secret_policy_version,
            crate::knowledge::SECRET_POLICY_VERSION
        );
        try_quarantine_bad_snapshot(
            state_dir,
            &path,
            &bytes,
            "secret-policy-mismatch",
            format!(
                "snapshot secret policy {}, expected {}",
                snapshot.manifest.secret_policy_version,
                crate::knowledge::SECRET_POLICY_VERSION
            ),
        );
        return None;
    }

    if snapshot.project_id != expected_project_id {
        warn!(
            expected_project_id = %expected_project_id.0,
            actual_project_id = %snapshot.project_id.0,
            "index snapshot project identity mismatch — refusing foreign state"
        );
        try_quarantine_bad_snapshot(
            state_dir,
            &path,
            &bytes,
            "project-id-mismatch",
            format!(
                "snapshot project {}, expected {}",
                snapshot.project_id.0, expected_project_id.0
            ),
        );
        return None;
    }

    if let Err(error) = verify_snapshot_source_identity(&snapshot, project_root) {
        warn!(
            detail = %error,
            "index snapshot source identity mismatch — refusing foreign or stale state"
        );
        try_quarantine_bad_snapshot(
            state_dir,
            &path,
            &bytes,
            "source-identity-mismatch",
            error.to_string(),
        );
        return None;
    }

    Some(snapshot)
}

/// Rehydrate a `LiveIndex` from a persisted snapshot.
///
/// `project_root` is the filesystem root the snapshot was taken from; it is
/// recorded as the index's normalized `indexed_root` so a later project switch
/// triggers a root-mismatch reload (see `SymForgeServer::ensure_local_index`).
/// The snapshot wire format itself does not carry the root, so the caller
/// supplies it — the same root passed to [`load_snapshot`].
pub fn snapshot_to_live_index(snapshot: IndexSnapshot, project_root: &Path) -> LiveIndex {
    snapshot_to_live_index_with_code_signals(snapshot, project_root).0
}

pub fn snapshot_to_live_index_with_code_signals(
    snapshot: IndexSnapshot,
    project_root: &Path,
) -> (LiveIndex, CodeSignalsSnapshot) {
    let IndexSnapshot {
        files: snapshot_files,
        manifest,
        code_signals,
        ..
    } = snapshot;
    let manifest_entries = manifest.entries;
    let mut files: HashMap<String, Arc<IndexedFile>> = HashMap::with_capacity(snapshot_files.len());

    for (path, snap_file) in snapshot_files {
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
        manifest_entries,
        coupling_store: None,
        local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
        // A snapshot-restored index serves a real project; record its root so a
        // project switch invalidates it the same way a freshly loaded one does.
        indexed_root: Some(normalize_root(project_root)),
    };
    index.rebuild_reverse_index();
    index.rebuild_path_indices();
    (index, code_signals.into_published())
}

/// Stat-check all files in the index against disk to find changed/deleted/new files.
///
/// Compares `byte_len` and `mtime_secs` stored in the snapshot against current
/// filesystem metadata. Files with differing size or mtime are in `changed`.
/// Files with `ENOENT` go to `deleted`. Files on disk not in the index go to `new_files`.
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
    let step = total.checked_div(sample_size).unwrap_or(1);
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
pub fn init_frecency_store(project_root: &Path, project_state: &crate::domain::ProjectStateDir) {
    // Hook registration is unconditional — the hook body resolves collection
    // policy at call time, so a test that flips `SYMFORGE_FRECENCY` after boot
    // still sees edits follow the current policy.
    crate::live_index::frecency::ensure_bump_hook_registered();
    if crate::live_index::frecency::collection_policy_from_env()
        != crate::capability::FrecencyCollectionPolicy::Persistent
    {
        return;
    }
    let db_path = crate::live_index::frecency::frecency_db_path(project_state);
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
    manifest_entries: Vec<CatalogEntry>,
    manifest: Option<RepositoryManifest>,
    code_signals: Option<PersistedCodeSignals>,
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
fn build_snapshot(
    snapshot_input: SnapshotBuildInput,
    project_root: &Path,
    project_id: ProjectId,
) -> anyhow::Result<IndexSnapshot> {
    let SnapshotBuildInput {
        files,
        manifest_entries,
        manifest,
        code_signals,
    } = snapshot_input;
    let mut snap_files = HashMap::with_capacity(files.len());

    for (path, file) in files {
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

    let captured_source = capture_repository_source(project_root, &project_id)?;
    let manifest = if let Some(manifest) = manifest {
        if manifest.entries != manifest_entries {
            anyhow::bail!("published manifest entries disagree with the captured live generation");
        }
        if manifest.source != captured_source.source
            || manifest.source_version != captured_source.source_version
        {
            anyhow::bail!(
                "source identity or version advanced while the published generation was checkpointed"
            );
        }
        let rebuilt = RepositoryManifest::new(
            manifest.schema_version,
            manifest.policy_version,
            manifest.secret_policy_version,
            manifest.source.clone(),
            manifest.source_version.clone(),
            manifest.coverage,
            manifest.entries.clone(),
            manifest.issues.clone(),
            manifest.usage,
        )?;
        if rebuilt.digest != manifest.digest {
            anyhow::bail!("published manifest digest failed canonical reconstruction");
        }
        manifest
    } else {
        let coverage = if manifest_entries.iter().any(|entry| {
            matches!(
                entry.disposition,
                FileDisposition::Unreadable { .. }
                    | FileDisposition::UnstableDuringRead
                    | FileDisposition::AbortedCircuitBreaker
            )
        }) {
            CoverageStatus::Degraded
        } else {
            CoverageStatus::Complete
        };
        let usage = ManifestResourceUsage {
            catalog_entries: manifest_entries.len() as u64,
            catalog_metadata_bytes: serde_json::to_vec(&manifest_entries)?.len() as u64,
            admitted_content_bytes: snap_files.values().map(|file| file.byte_len).sum(),
        };
        RepositoryManifest::new(
            1,
            1,
            crate::knowledge::SECRET_POLICY_VERSION,
            captured_source.source.clone(),
            captured_source.source_version.clone(),
            coverage,
            manifest_entries,
            Vec::new(),
            usage,
        )?
    };
    let content_digest = indexed_content_digest(
        snap_files
            .iter()
            .map(|(path, file)| (path.as_str(), file.content.as_slice())),
    )?;
    let source_identity = snapshot_source_identity_from_captured(
        project_id.clone(),
        captured_source,
        manifest.digest.clone(),
        content_digest,
    );

    let code_signals = code_signals.unwrap_or_else(|| PersistedCodeSignals {
        temporal: super::git_temporal::GitTemporalIndex::pending(),
        computed_for_content_generation: 0,
        computed_for_source_version: manifest.source_version.clone(),
        coverage: capture_history_coverage(project_root),
    });

    Ok(IndexSnapshot {
        version: CURRENT_VERSION,
        project_id,
        source_identity,
        files: snap_files,
        manifest,
        code_signals,
    })
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
    background_verify_with_hook(index, root, snapshot_mtimes, || {}).await;
}

async fn background_verify_with_hook<F>(
    index: crate::live_index::store::SharedIndex,
    root: std::path::PathBuf,
    snapshot_mtimes: HashMap<String, u64>,
    after_fence: F,
) where
    F: FnOnce(),
{
    let captured_base = index.publication_fence();
    after_fence();
    let Some(mut commit_fence) = index.mark_snapshot_verify_running_at_fence(captured_base) else {
        return;
    };
    #[cfg(feature = "server")]
    let expected_gen = commit_fence.project_generation;

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
            if !index.remove_file_at_publication_fence(path, commit_fence) {
                return;
            }
            commit_fence = index.publication_fence();
        }
    }

    // 3. Re-parse changed files. Reindexing routes through the watcher's
    //    admission path; embed has no watcher, so changed/new files are
    //    detected but not re-parsed here (reconciliation is server-only).
    #[cfg(feature = "server")]
    {
        let to_reparse: Vec<String> = stat_result
            .changed
            .into_iter()
            .chain(stat_result.new_files)
            .collect();

        for rel_path in &to_reparse {
            if !index.matches_publication_fence(commit_fence) {
                return;
            }
            let abs_path = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let _ = crate::watcher::admit_and_index_single_path(
                rel_path,
                &abs_path,
                &index,
                expected_gen,
            );
            if index.current_project_generation() != expected_gen {
                return;
            }
            commit_fence = index.publication_fence();
        }
    }

    // 4. Spot-verify sample (10%) for content hash mismatches
    let verify_view = {
        let guard = index.read();
        capture_verify_view(&guard)
    };
    let spot_mismatches = spot_verify_sample_from_view(&verify_view, &root, 0.10);

    let spot_count = spot_mismatches.len();

    // Re-parse spot-check mismatches (server-only; see step 3 — embed reports
    // detected mismatches but has no watcher to re-parse them).
    #[cfg(feature = "server")]
    {
        for rel_path in &spot_mismatches {
            if !index.matches_publication_fence(commit_fence) {
                return;
            }
            let abs_path = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let _ = crate::watcher::admit_and_index_single_path(
                rel_path,
                &abs_path,
                &index,
                expected_gen,
            );
            if index.current_project_generation() != expected_gen {
                return;
            }
            commit_fence = index.publication_fence();
        }
    }

    // Under `embed` there is no watcher to re-parse the changed/new files that
    // the stat-check detected in step 1, so fold them into the reported mismatch
    // set. Freshness then resolves to `Degraded` (SnapshotVerificationFailed)
    // rather than mislabeling unreconciled changes as `Current`. Under `server`
    // those files were re-parsed above, so they are correctly not mismatches.
    #[cfg(not(feature = "server"))]
    let spot_mismatches = {
        let mut mismatches = spot_mismatches;
        mismatches.extend(stat_result.changed);
        mismatches.extend(stat_result.new_files);
        mismatches.sort();
        mismatches.dedup();
        mismatches
    };

    if !index.mark_snapshot_verify_completed_at_fence(commit_fence, spot_mismatches) {
        return;
    }

    info!(
        "background verify complete: {} changed, {} deleted, {} new, {} spot-check mismatches",
        changed_count, deleted_count, new_count, spot_count
    );
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AccessErrorKind, CapabilityStatus, CapabilityUnavailableReason, CatalogEntry, CatalogPath,
        FileDisposition, HardSkipReason, IndexTargets, LanguageId, MetadataOnlyReason,
        ProjectStateDir, ReferenceKind, ReferenceRecord, RootBinding, RootCandidateSource,
        SourceAccessMode, StatePlacement, SymbolKind, SymbolRecord, UserLocalPlacementReason,
    };
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

    fn project_local_placement(project_root: &Path) -> StatePlacement {
        StatePlacement::ProjectLocal {
            directory: ProjectStateDir::new(project_root.join(".symforge")),
        }
    }

    fn artifact_binding(project_root: &Path, access_mode: SourceAccessMode) -> RootBinding {
        let canonical_root = dunce::canonicalize(project_root).expect("canonical project root");
        RootBinding {
            source: RootCandidateSource::ExplicitIndexFolder,
            root_id: crate::discovery::project_id_for_canonical_root(&canonical_root),
            canonical_root,
            access_mode,
        }
    }

    fn export_artifact_legacy(
        index: &LiveIndex,
        project_root: &Path,
    ) -> anyhow::Result<ArtifactExportReport> {
        let binding = artifact_binding(project_root, SourceAccessMode::NormalProject);
        super::export_artifact(
            index,
            &binding.canonical_root,
            binding.access_mode,
            &project_local_placement(&binding.canonical_root),
            &CapabilityStatus::Available,
        )
    }

    fn assert_no_team_artifact_mutation(project_root: &Path) {
        assert!(!project_root.join(".symforge/index.bin.zst").exists());
        assert!(!project_root.join(".symforge/artifact.json").exists());
        assert!(!project_root.join(".gitattributes").exists());
    }

    // Existing snapshot unit cases intentionally exercise the project-local
    // variant. Keep their concise call shape test-only while production APIs
    // require an explicit typed placement.
    fn serialize_index(index: &LiveIndex, project_root: &Path) -> anyhow::Result<()> {
        super::serialize_index(index, project_root, &project_local_placement(project_root))
    }

    fn load_snapshot(project_root: &Path) -> Option<IndexSnapshot> {
        super::load_snapshot(project_root, &project_local_placement(project_root))
    }

    fn reset_snapshot_state(project_root: &Path) -> anyhow::Result<SnapshotResetReport> {
        super::reset_snapshot_state(project_root, &project_local_placement(project_root))
    }

    fn snapshot_path_and_lock(project_root: &Path) -> anyhow::Result<(PathBuf, Arc<Mutex<()>>)> {
        super::snapshot_path_and_lock(match &project_local_placement(project_root) {
            StatePlacement::ProjectLocal { directory } => directory,
            _ => unreachable!(),
        })
    }

    fn quarantine_bad_snapshot(
        project_root: &Path,
        snapshot_path: &Path,
        bytes: &[u8],
        reason: &str,
        detail: String,
    ) -> anyhow::Result<PathBuf> {
        let placement = project_local_placement(project_root);
        let StatePlacement::ProjectLocal { directory } = &placement else {
            unreachable!()
        };
        super::quarantine_bad_snapshot(directory, snapshot_path, bytes, reason, detail)
    }

    fn build_snapshot(snapshot_input: SnapshotBuildInput, project_root: &Path) -> IndexSnapshot {
        let canonical_root = project_root.canonicalize().unwrap();
        let project_id = crate::discovery::project_id_for_canonical_root(&canonical_root);
        super::build_snapshot(snapshot_input, project_root, project_id).unwrap()
    }

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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
    }

    fn user_local_placement(source_root: &Path, directory: &Path) -> StatePlacement {
        let canonical_root = dunce::canonicalize(source_root).unwrap();
        StatePlacement::UserLocal {
            directory: ProjectStateDir::new(directory.to_path_buf()),
            root_id: crate::discovery::project_id_for_canonical_root(&canonical_root),
            reason: UserLocalPlacementReason::ProjectLocalUnavailable {
                safe_reason: AccessErrorKind::PermissionDenied,
            },
        }
    }

    fn routed_quarantine_files(state_dir: &Path, extension: &str) -> Vec<PathBuf> {
        let directory = state_dir.join("quarantine").join("index-snapshots");
        let mut files = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    #[test]
    fn global_state_identity_mismatch_is_never_loaded_or_overwritten() {
        let source_a = TempDir::new().unwrap();
        let source_b = TempDir::new().unwrap();
        let global_state = TempDir::new().unwrap();
        let shared_state_dir = global_state
            .path()
            .join("projects")
            .join("forced-collision");
        std::fs::create_dir_all(&shared_state_dir).unwrap();
        let placement_a = user_local_placement(source_a.path(), &shared_state_dir);
        let placement_b = user_local_placement(source_b.path(), &shared_state_dir);
        let index_a = make_live_index_with_files(vec![("src/a.rs", b"fn a() {}\n")]);
        let index_b = make_live_index_with_files(vec![("src/b.rs", b"fn b() {}\n")]);

        super::serialize_index(&index_a, source_a.path(), &placement_a).unwrap();
        let active_snapshot = shared_state_dir.join(INDEX_FILENAME);
        let foreign_bytes = std::fs::read(&active_snapshot).unwrap();

        let overwrite = super::serialize_index(&index_b, source_b.path(), &placement_b);
        assert!(
            overwrite.is_err(),
            "a different source identity sharing a user-local placement must not overwrite foreign state"
        );
        assert!(
            !active_snapshot.exists(),
            "foreign state must be removed from the active slot after quarantine"
        );
        assert!(
            routed_quarantine_files(&shared_state_dir, "bin")
                .iter()
                .any(|path| std::fs::read(path).unwrap() == foreign_bytes),
            "the refused foreign bytes must be preserved exactly in quarantine"
        );

        std::fs::write(&active_snapshot, &foreign_bytes).unwrap();
        assert!(
            super::load_snapshot(source_b.path(), &placement_b).is_none(),
            "foreign user-local state must never be loaded"
        );
        assert!(
            !active_snapshot.exists(),
            "foreign load candidate must leave the active slot quarantined"
        );
        assert!(
            routed_quarantine_files(&shared_state_dir, "bin")
                .iter()
                .filter(|path| std::fs::read(path).unwrap() == foreign_bytes)
                .count()
                >= 2,
            "both the refused overwrite and refused load must preserve the foreign bytes"
        );
    }

    #[test]
    fn snapshot_lifecycle_uses_resolved_state_placement_without_source_fallback() {
        let source = TempDir::new().unwrap();
        let global_state = TempDir::new().unwrap();
        let state_dir = global_state.path().join("projects").join("project-v1-test");
        std::fs::create_dir_all(&state_dir).unwrap();
        let placement = user_local_placement(source.path(), &state_dir);
        let source_state = source.path().join(".symforge");
        std::fs::create_dir_all(&source_state).unwrap();
        std::fs::write(source_state.join("sentinel"), b"source-owned\n").unwrap();

        let shared = crate::live_index::SharedIndexHandle::shared(make_live_index_with_files(
            vec![("src/lib.rs", b"pub fn routed() {}\n")],
        ));
        super::checkpoint_shared_index(&shared, source.path(), &placement).unwrap();
        let routed_snapshot = state_dir.join(INDEX_FILENAME);
        assert!(routed_snapshot.is_file());
        assert!(!source_state.join(INDEX_FILENAME).exists());

        std::fs::write(&routed_snapshot, b"corrupt routed snapshot").unwrap();
        assert!(super::load_snapshot(source.path(), &placement).is_none());
        assert_eq!(routed_quarantine_files(&state_dir, "bin").len(), 1);
        assert!(
            !source_state.join("quarantine").exists(),
            "quarantine must remain under the selected ProjectStateDir"
        );

        super::checkpoint_shared_index(&shared, source.path(), &placement).unwrap();
        std::fs::write(state_dir.join(INDEX_TMP_FILENAME), b"stale temp").unwrap();
        let report = super::reset_snapshot_state(source.path(), &placement).unwrap();
        assert!(report.removed_count() >= 2);
        assert!(!routed_snapshot.exists());
        assert_eq!(
            std::fs::read(source_state.join("sentinel")).unwrap(),
            b"source-owned\n"
        );
        assert_eq!(
            std::fs::read_dir(&source_state).unwrap().count(),
            1,
            "snapshot/reset/quarantine/checkpoint must not reconstruct source-local state"
        );
    }

    // ── Round-trip tests ──────────────────────────────────────────────────────

    #[test]
    fn snapshot_round_trip_preserves_target_enum_and_catalog_dispositions() {
        let tmp = TempDir::new().unwrap();
        let content = b"fn main() {}\n";
        let mut index = make_live_index_with_files(vec![("src/main.rs", content)]);
        let entry = |path: &str, size: u64, disposition: FileDisposition| CatalogEntry {
            path: CatalogPath {
                public_id: path.to_string(),
                normalized_utf8: Some(path.to_string()),
            },
            size,
            language: LanguageId::from_extension(
                Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default(),
            ),
            classification: crate::domain::FileClassification::for_code_path(path),
            disposition,
            content_hash: None,
        };
        let expected = vec![
            entry(
                "docs/legacy.txt",
                17,
                FileDisposition::MetadataOnly {
                    reason: MetadataOnlyReason::UnsupportedTextEncoding,
                },
            ),
            entry(
                "fixtures/oversized.bin",
                50_000_000,
                FileDisposition::HardSkip {
                    reason: HardSkipReason::PerFileCeiling,
                },
            ),
            entry(
                "src/main.rs",
                content.len() as u64,
                FileDisposition::Indexed {
                    targets: IndexTargets::CodeAndKnowledge,
                    parse_status: crate::domain::index::ParseStatus::Parsed,
                },
            ),
        ];
        index.manifest_entries = expected.clone();

        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("fresh snapshot should load");
        let loaded = snapshot_to_live_index(snapshot, tmp.path());

        assert_eq!(loaded.manifest_entries, expected);
    }

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
        let loaded = snapshot_to_live_index(snapshot, tmp.path());

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
    fn test_round_trip_preserves_crlf_bytes_and_hash() {
        let tmp = TempDir::new().unwrap();
        let content = b"fn my_func() {\r\n    other_func();\r\n}\r\n";
        let index =
            make_live_index_with_files(vec![("tests/generated/main.generated.rs", content)]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot, tmp.path());
        let file = loaded
            .files
            .get("tests/generated/main.generated.rs")
            .expect("file should be present");

        assert_eq!(file.content, content);
        assert_eq!(file.byte_len, content.len() as u64);
        assert_eq!(file.content_hash, crate::hash::digest_hex(content));
    }

    #[test]
    fn test_round_trip_empty_index() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![]);

        serialize_index(&index, tmp.path()).expect("serialize empty index should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot, tmp.path());

        assert_eq!(loaded.files.len(), 0);
    }

    #[test]
    fn test_snapshot_to_live_index_marks_snapshot_restore_pending_verify() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}")]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot, tmp.path());

        assert_eq!(loaded.load_source(), IndexLoadSource::SnapshotRestore);
        assert_eq!(loaded.snapshot_verify_state(), SnapshotVerifyState::Pending);
    }

    #[test]
    fn verifying_snapshot_is_not_query_ready() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let shared = crate::live_index::SharedIndexHandle::shared(snapshot_to_live_index(
            snapshot,
            tmp.path(),
        ));

        assert_eq!(
            shared.read().snapshot_verify_state(),
            SnapshotVerifyState::Pending
        );
        assert!(
            !shared.read().is_ready(),
            "an unverified snapshot candidate must not be query-ready"
        );
        assert_ne!(
            shared.published_state().status,
            crate::live_index::store::PublishedIndexStatus::Ready
        );
        assert_eq!(
            &*shared.freshness_status(),
            &crate::domain::FreshnessStatus::Verifying
        );
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
        let loaded = snapshot_to_live_index(snapshot, tmp.path());
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
            SnapshotVerifyState::completed_without_mismatches()
        );
        drop(guard);

        let published = shared.published_state();
        assert_eq!(
            published.snapshot_verify_state,
            SnapshotVerifyState::completed_without_mismatches()
        );
        assert!(
            published.generation >= 2,
            "expected published generation to advance through verify transitions"
        );
        assert_eq!(published.file_count, before.file_count);
        assert_eq!(published.partial_parse_count, before.partial_parse_count);
        assert_eq!(published.failed_count, before.failed_count);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn snapshot_restore_rebuilds_current_authority_versions_before_ready() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), b"# Restored\nbody\n").unwrap();
        let shared = LiveIndex::load(tmp.path()).unwrap();
        checkpoint_shared_index(&shared, tmp.path(), &project_local_placement(tmp.path())).unwrap();

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let snapshot_mtimes = snapshot
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.mtime_secs))
            .collect::<HashMap<_, _>>();
        let (live, code_signals) = snapshot_to_live_index_with_code_signals(snapshot, tmp.path());
        let restored =
            crate::live_index::SharedIndexHandle::shared_with_code_signals(live, code_signals);

        assert!(!restored.read().is_ready());
        let before_ready = restored.published_generation();
        assert_eq!(
            before_ready.authority.versions.authority_rule_version,
            crate::live_index::knowledge_authority::AUTHORITY_RULE_VERSION
        );
        assert_eq!(
            before_ready.authority.versions.policy_version,
            crate::live_index::knowledge_authority::KNOWLEDGE_POLICY_VERSION
        );
        assert_eq!(
            before_ready.authority.versions.secret_policy_version,
            crate::knowledge::SECRET_POLICY_VERSION
        );
        assert!(!before_ready.authority.records.is_empty());

        background_verify(restored.clone(), tmp.path().to_path_buf(), snapshot_mtimes).await;

        assert!(restored.read().is_ready());
        let ready = restored.published_generation();
        assert_eq!(ready.authority.versions, before_ready.authority.versions);
        assert_eq!(ready.authority.records, before_ready.authority.records);
    }

    // Server-only: exercises the watcher admission reparse path, which the embed
    // build (no watcher) does not run.
    #[cfg(feature = "server")]
    #[tokio::test]
    async fn background_verify_uses_shared_admission_for_large_new_file() {
        let tmp = TempDir::new().unwrap();
        let relative_path = "src/oversized.rs";
        let file_path = tmp.path().join("src").join("oversized.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let file = std::fs::File::create(&file_path).unwrap();
        file.set_len(crate::domain::index::METADATA_ONLY_CODE_BYTES + 1)
            .unwrap();
        drop(file);

        let shared =
            crate::live_index::SharedIndexHandle::shared(make_live_index_with_files(Vec::new()));
        background_verify(shared.clone(), tmp.path().to_path_buf(), HashMap::new()).await;

        let guard = shared.read();
        assert!(
            !guard.files.contains_key(relative_path),
            "metadata-terminal files must never enter resident content"
        );
        let disposition = guard
            .manifest_entries
            .iter()
            .find(|entry| entry.path.normalized_utf8.as_deref() == Some(relative_path))
            .map(|entry| &entry.disposition);
        assert!(
            matches!(
                disposition,
                Some(FileDisposition::MetadataOnly {
                    reason: MetadataOnlyReason::OversizedData
                })
            ),
            "background verification must publish the shared metadata-first admission outcome"
        );
    }

    // Server-only: drives the watcher admission + reconcile paths directly, which
    // the embed build (no watcher) does not compile.
    #[cfg(feature = "server")]
    #[tokio::test]
    async fn cold_watch_reconcile_and_background_verify_have_identical_knowledge_units_and_dispositions()
     {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), b"# Root\n## Child\nbody\n").unwrap();
        std::fs::write(tmp.path().join("guide.rst"), b"Guide\n=====\nbody\n").unwrap();
        std::fs::write(tmp.path().join("settings.toml"), b"enabled = true\n").unwrap();
        std::fs::write(tmp.path().join(".env"), b"placeholder=true\n").unwrap();
        std::fs::write(tmp.path().join("invalid.txt"), [0xff, 0xfe, b'x']).unwrap();
        std::fs::write(
            tmp.path().join("pointer.md"),
            b"version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n",
        )
        .unwrap();
        let detector_payload = format!(
            "{}={}\n",
            ["to", "ken"].concat(),
            ["runtime", "-", "canary", "-", "parity"].concat()
        );
        std::fs::write(tmp.path().join(".env.example"), detector_payload.as_bytes()).unwrap();

        let source = SourceIdentity {
            repository_id: RepositoryId::new("repository-fixture"),
            source_id: SourceId::new("source-fixture"),
            location: crate::domain::SourceLocation::WorkingTree {
                worktree_id: "worktree-fixture".to_string(),
            },
        };
        let make_empty_shared = || {
            let mut index = make_live_index_with_files(Vec::new());
            index.indexed_root = Some(tmp.path().to_path_buf());
            crate::live_index::SharedIndexHandle::shared(index)
        };
        let capture = |shared: &crate::live_index::store::SharedIndex| {
            let index = shared.read();
            let mut manifest = index.manifest_entries.clone();
            manifest.sort_by_key(|entry| {
                entry
                    .path
                    .normalized_utf8
                    .clone()
                    .unwrap_or_else(|| entry.path.public_id.clone())
            });
            let mut units = index
                .files
                .values()
                .filter(|file| file.language == LanguageId::Markdown)
                .flat_map(|file| {
                    crate::knowledge::project_markdown_sections(
                        &source,
                        &file.relative_path,
                        &file.content_hash,
                        &file.symbols,
                    )
                })
                .collect::<Vec<_>>();
            units.sort_by_key(|unit| (unit.path.clone(), unit.byte_range.start));
            let authority = shared.published_generation().authority.clone();
            let mut authority_records = authority.records.clone();
            for record in &mut authority_records {
                // The four paths publish at intentionally different generation
                // counters. H-R08 compares semantic derivation, not the receipt
                // number carried by otherwise identical anchors.
                record.unit.content_generation = 0;
                if let crate::live_index::knowledge_authority::LifecycleEvidence::DeclaredSpan(
                    anchor,
                ) = &mut record.lifecycle_evidence
                {
                    anchor.content_generation = 0;
                }
                match &mut record.authority_domain_evidence {
                    crate::live_index::knowledge_authority::AuthorityDomainEvidence::DeclaredSpan(
                        anchor,
                    )
                    | crate::live_index::knowledge_authority::AuthorityDomainEvidence::RoleRule {
                        anchor,
                        ..
                    } => anchor.content_generation = 0,
                    _ => {}
                }
                if let Some(successor) = &mut record.successor {
                    successor.content_generation = 0;
                }
            }
            (
                manifest,
                units,
                authority_records,
                authority.policy_status.clone(),
                authority.coverage.clone(),
            )
        };

        let cold = LiveIndex::load(tmp.path()).unwrap();

        let watch = make_empty_shared();
        let expected_gen = watch.current_project_generation();
        for relative_path in [
            ".env",
            ".env.example",
            "README.md",
            "guide.rst",
            "invalid.txt",
            "pointer.md",
            "settings.toml",
        ] {
            let absolute_path = tmp.path().join(relative_path);
            let _ = crate::watcher::admit_and_index_single_path(
                relative_path,
                &absolute_path,
                &watch,
                expected_gen,
            );
        }

        let reconcile = make_empty_shared();
        crate::watcher::reconcile_stale_files(tmp.path(), &reconcile);

        let background = make_empty_shared();
        background_verify(background.clone(), tmp.path().to_path_buf(), HashMap::new()).await;

        let expected = capture(&cold);
        assert_eq!(capture(&watch), expected);
        assert_eq!(capture(&reconcile), expected);
        assert_eq!(capture(&background), expected);
    }

    #[tokio::test]
    async fn background_verify_cannot_mutate_after_project_retarget() {
        let tmp = TempDir::new().unwrap();
        let new_file = tmp.path().join("src").join("new.rs");
        std::fs::create_dir_all(new_file.parent().unwrap()).unwrap();
        std::fs::write(&new_file, b"fn new_project() {}\n").unwrap();

        let shared = crate::live_index::SharedIndexHandle::shared(make_live_index_with_files(
            vec![("src/old.rs", b"fn old_project() {}\n")],
        ));
        let initial_project_generation = shared.current_project_generation();
        let reset_publication_generation = Arc::new(std::sync::atomic::AtomicU64::new(0));

        background_verify_with_hook(shared.clone(), tmp.path().to_path_buf(), HashMap::new(), {
            let shared = shared.clone();
            let reset_publication_generation = reset_publication_generation.clone();
            move || {
                shared.reset_to_empty();
                reset_publication_generation.store(
                    shared.published_state().generation,
                    std::sync::atomic::Ordering::SeqCst,
                );
            }
        })
        .await;

        assert_eq!(
            shared.current_project_generation(),
            initial_project_generation + 1
        );
        let published = shared.published_state();
        assert_eq!(
            published.generation,
            reset_publication_generation.load(std::sync::atomic::Ordering::SeqCst),
            "a verifier captured for the prior project must not publish into the replacement"
        );
        assert_eq!(
            published.snapshot_verify_state,
            SnapshotVerifyState::NotNeeded,
            "the replacement project must retain its own verification state"
        );
        assert_eq!(published.file_count, 0);
    }

    #[tokio::test]
    async fn background_verify_racing_watcher_update_rebases_or_aborts() {
        let tmp = TempDir::new().unwrap();
        let shared = crate::live_index::SharedIndexHandle::shared(make_live_index_with_files(
            vec![("src/base.rs", b"fn base() {}\n")],
        ));
        let base = shared.published_generation();
        let base_project_generation = base.project_generation;
        let watcher_publication = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let watcher_content = Arc::new(std::sync::atomic::AtomicU64::new(0));

        background_verify_with_hook(shared.clone(), tmp.path().to_path_buf(), HashMap::new(), {
            let shared = shared.clone();
            let watcher_publication = watcher_publication.clone();
            let watcher_content = watcher_content.clone();
            move || {
                let watcher_index =
                    make_live_index_with_files(vec![("src/watcher.rs", b"fn watcher_won() {}\n")]);
                let indexed = watcher_index
                    .files
                    .get("src/watcher.rs")
                    .expect("watcher fixture")
                    .as_ref()
                    .clone();
                assert!(shared.update_file_at_generation(
                    "src/watcher.rs",
                    indexed,
                    base_project_generation,
                ));
                let watcher = shared.published_generation();
                watcher_publication.store(
                    watcher.publication_generation,
                    std::sync::atomic::Ordering::SeqCst,
                );
                watcher_content.store(
                    watcher.content_generation,
                    std::sync::atomic::Ordering::SeqCst,
                );
            }
        })
        .await;

        let current = shared.published_generation();
        assert_eq!(current.project_generation, base_project_generation);
        assert_eq!(
            current.publication_generation,
            watcher_publication.load(std::sync::atomic::Ordering::SeqCst),
            "a verifier fenced to an older publication must not publish after the watcher"
        );
        assert_eq!(
            current.content_generation,
            watcher_content.load(std::sync::atomic::Ordering::SeqCst),
            "a verifier must not relabel or replace the watcher's newer content generation"
        );
        assert!(
            current.live.files.contains_key("src/watcher.rs"),
            "the watcher winner must remain in the captured publication root"
        );
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
        let loaded = snapshot_to_live_index(snapshot, tmp.path());
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
            SnapshotVerifyState::completed_without_mismatches()
        );
        assert_eq!(published.file_count, 0);
        assert_eq!(published.parsed_count, 0);
        assert_eq!(published.partial_parse_count, 0);
        assert_eq!(published.failed_count, 0);
    }

    #[tokio::test]
    async fn test_background_verify_records_spot_verify_mismatch_paths() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn alt1() {}\n").unwrap();
        let disk_mtime = std::fs::metadata(&file_path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        let mut index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        index.files.insert(
            "src/main.rs".to_string(),
            Arc::new(make_indexed_file("src/main.rs", b"fn main() {}\n").with_mtime(disk_mtime)),
        );
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let snapshot_mtimes = snapshot
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.mtime_secs))
            .collect::<HashMap<_, _>>();
        let loaded = snapshot_to_live_index(snapshot, tmp.path());
        let shared = crate::live_index::SharedIndexHandle::shared(loaded);

        background_verify(shared.clone(), tmp.path().to_path_buf(), snapshot_mtimes).await;

        let published = shared.published_state();
        match &published.snapshot_verify_state {
            SnapshotVerifyState::Completed(report) => {
                assert_eq!(report.mismatch_count, 1);
                assert_eq!(report.mismatched_paths, vec!["src/main.rs".to_string()]);
                assert_eq!(report.omitted_path_count(), 0);
            }
            other => panic!("expected completed snapshot verify report, got {other:?}"),
        }
    }

    #[cfg(not(feature = "server"))]
    #[tokio::test]
    async fn test_background_verify_embed_folds_stat_changed_into_mismatches() {
        // Embed contract (no watcher): a file the stat-check flags as changed must
        // degrade freshness even when the 10% content-hash spot sample would clear
        // it. Isolate the stat-only path — identical content on disk and in the
        // snapshot (so spot_verify sees no mismatch), but the recorded snapshot
        // mtime is older than the on-disk mtime (so stat_check flags it changed).
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}\n").unwrap();
        let disk_mtime = std::fs::metadata(&file_path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        let mut index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        index.files.insert(
            "src/main.rs".to_string(),
            Arc::new(make_indexed_file("src/main.rs", b"fn main() {}\n")),
        );
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let mut snapshot_mtimes = snapshot
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.mtime_secs))
            .collect::<HashMap<_, _>>();
        // `build_snapshot` re-stats the disk mtime, so the recorded mtime currently
        // equals the on-disk mtime. Force it OLDER so `stat_check_files_from_view`
        // reports the file as changed while the byte-identical content keeps the
        // spot sample clean — isolating the embed-only fold-in path.
        snapshot_mtimes.insert("src/main.rs".to_string(), disk_mtime.saturating_sub(1_000));

        let loaded = snapshot_to_live_index(snapshot, tmp.path());
        let shared = crate::live_index::SharedIndexHandle::shared(loaded);

        background_verify(shared.clone(), tmp.path().to_path_buf(), snapshot_mtimes).await;

        let published = shared.published_state();
        match &published.snapshot_verify_state {
            SnapshotVerifyState::Completed(report) => {
                assert!(
                    report.mismatched_paths.contains(&"src/main.rs".to_string()),
                    "stat-changed file must be folded into mismatches under embed, got {:?}",
                    report.mismatched_paths
                );
            }
            other => panic!("expected completed snapshot verify report, got {other:?}"),
        }

        match &*shared.freshness_status() {
            crate::domain::FreshnessStatus::Degraded { reason_codes, .. } => {
                assert!(
                    reason_codes
                        .contains(&crate::domain::FreshnessReason::SnapshotVerificationFailed),
                    "expected SnapshotVerificationFailed, got {reason_codes:?}"
                );
            }
            other => panic!("expected Degraded freshness under embed, got {other:?}"),
        }
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
        let loaded = snapshot_to_live_index(snapshot, tmp.path());

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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("load should succeed");
        let loaded = snapshot_to_live_index(snapshot, tmp.path());

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
            CURRENT_VERSION, 7,
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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
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
        let after = snapshot_to_live_index(snapshot, tmp.path());

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

    fn quarantine_files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
        let dir = paths::resolve_index_snapshot_quarantine_dir(root);
        let mut files = match std::fs::read_dir(&dir) {
            Ok(entries) => entries
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(extension))
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        files.sort();
        files
    }

    #[test]
    fn test_version_mismatch_quarantines_snapshot_and_returns_none() {
        let tmp = TempDir::new().unwrap();

        // Build a snapshot with a wrong version and serialize it manually
        let mut snapshot = build_snapshot(
            super::capture_snapshot_build_input(&make_live_index_with_files(Vec::new())),
            tmp.path(),
        );
        snapshot.version = 999;
        let bytes = postcard::to_stdvec(&snapshot).unwrap();
        let dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.bin"), &bytes).unwrap();

        // load_snapshot must return None, not panic
        let result = load_snapshot(tmp.path());
        assert!(result.is_none(), "version mismatch must return None");

        let quarantine_bins = quarantine_files_with_extension(tmp.path(), "bin");
        assert_eq!(
            quarantine_bins.len(),
            1,
            "version mismatch should preserve the snapshot in quarantine"
        );
        assert_eq!(
            std::fs::read(&quarantine_bins[0]).unwrap(),
            bytes,
            "quarantine must preserve original version-mismatched bytes"
        );
        assert!(
            !dir.join("index.bin").exists(),
            "version-mismatched active snapshot should be removed after quarantine"
        );
        let quarantine_metadata = quarantine_files_with_extension(tmp.path(), "json");
        assert_eq!(quarantine_metadata.len(), 1);
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&quarantine_metadata[0]).unwrap()).unwrap();
        assert_eq!(metadata["reason"], "version-mismatch");
        assert_eq!(metadata["sha256"], crate::hash::digest_hex(&bytes));
    }

    #[test]
    fn secret_policy_mismatch_forces_rescout_before_snapshot_ready() {
        let tmp = TempDir::new().unwrap();
        let mut snapshot = build_snapshot(
            super::capture_snapshot_build_input(&make_live_index_with_files(Vec::new())),
            tmp.path(),
        );
        let mismatched_manifest = RepositoryManifest::new(
            snapshot.manifest.schema_version,
            snapshot.manifest.policy_version,
            crate::knowledge::SECRET_POLICY_VERSION + 1,
            snapshot.manifest.source.clone(),
            snapshot.manifest.source_version.clone(),
            snapshot.manifest.coverage,
            snapshot.manifest.entries.clone(),
            snapshot.manifest.issues.clone(),
            snapshot.manifest.usage,
        )
        .unwrap();
        snapshot.manifest = mismatched_manifest;
        snapshot.source_identity = super::capture_snapshot_source_identity(
            tmp.path(),
            snapshot.project_id.clone(),
            snapshot.manifest.digest.clone(),
            snapshot.source_identity.indexed_content_digest.clone(),
        )
        .unwrap();

        let bytes = postcard::to_stdvec(&snapshot).unwrap();
        let dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.bin"), &bytes).unwrap();

        assert!(
            load_snapshot(tmp.path()).is_none(),
            "a changed detector policy must force a cold re-scout"
        );
        let quarantine_metadata = quarantine_files_with_extension(tmp.path(), "json");
        assert_eq!(quarantine_metadata.len(), 1);
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&quarantine_metadata[0]).unwrap()).unwrap();
        assert_eq!(metadata["reason"], "secret-policy-mismatch");
    }

    /// Guards the v1 → v2 BUMP itself, which the sibling test above cannot:
    /// it builds its mismatch symbolically as `SECRET_POLICY_VERSION + 1`, so
    /// it passes at every constant value and says nothing about which values
    /// are stale. This one names the LITERAL version shipped detectors wrote.
    ///
    /// Reverting the bump makes a v1 snapshot match the current policy again,
    /// and manifests carrying verdicts from before the bounded right-hand-side
    /// walk, the embedded-literal tightening and whole-buffer encoding
    /// validation would be trusted — a stale `Indexed` disposition authorizing
    /// bytes the current detector calls sensitive.
    #[test]
    fn snapshots_written_under_the_previous_secret_policy_are_refused() {
        // Deliberately a LITERAL, not `SECRET_POLICY_VERSION - 1`: the guard is
        // that THIS value is stale. Revert the bump and this snapshot matches
        // the current policy, loads, and the assertion below fails.
        const SUPERSEDED_SECRET_POLICY_VERSION: u32 = 1;

        let tmp = TempDir::new().unwrap();
        let mut snapshot = build_snapshot(
            super::capture_snapshot_build_input(&make_live_index_with_files(Vec::new())),
            tmp.path(),
        );
        snapshot.manifest = RepositoryManifest::new(
            snapshot.manifest.schema_version,
            snapshot.manifest.policy_version,
            SUPERSEDED_SECRET_POLICY_VERSION,
            snapshot.manifest.source.clone(),
            snapshot.manifest.source_version.clone(),
            snapshot.manifest.coverage,
            snapshot.manifest.entries.clone(),
            snapshot.manifest.issues.clone(),
            snapshot.manifest.usage,
        )
        .unwrap();
        snapshot.source_identity = super::capture_snapshot_source_identity(
            tmp.path(),
            snapshot.project_id.clone(),
            snapshot.manifest.digest.clone(),
            snapshot.source_identity.indexed_content_digest.clone(),
        )
        .unwrap();

        let bytes = postcard::to_stdvec(&snapshot).unwrap();
        let dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.bin"), &bytes).unwrap();

        assert!(
            load_snapshot(tmp.path()).is_none(),
            "a snapshot carrying the superseded detector policy must force a \
             cold re-scout rather than be trusted"
        );
    }

    #[test]
    fn runtime_canary_is_absent_from_serialized_snapshot() {
        let tmp = TempDir::new().unwrap();
        let canary = ["runtime", "-", "canary", "-", "snapshot"].concat();
        let payload = format!("{}={}\n", ["to", "ken"].concat(), canary);
        std::fs::write(tmp.path().join("notes.txt"), payload.as_bytes()).unwrap();

        let shared = LiveIndex::load(tmp.path()).unwrap();
        {
            let index = shared.read();
            assert!(index.get_file("notes.txt").is_none());
            serialize_index(&index, tmp.path()).unwrap();
        }

        let bytes = std::fs::read(tmp.path().join(".symforge").join("index.bin")).unwrap();
        assert!(
            !bytes
                .windows(canary.len())
                .any(|window| window == canary.as_bytes())
        );
        let snapshot = load_snapshot(tmp.path()).expect("safe snapshot must remain loadable");
        assert!(!snapshot.files.contains_key("notes.txt"));
        assert!(snapshot.manifest.entries.iter().any(|entry| matches!(
            entry.disposition,
            FileDisposition::MetadataOnly {
                reason: MetadataOnlyReason::SensitiveContent { .. }
            }
        )));
    }

    #[test]
    fn same_path_repository_replacement_never_inherits_snapshot_or_temporal_state() {
        let tmp = TempDir::new().unwrap();
        let original_root = init_repo_with_root_commit(tmp.path());
        let index = make_live_index_with_files(vec![("src/lib.rs", b"pub fn original() {}\n")]);
        serialize_index(&index, tmp.path()).unwrap();

        std::fs::remove_dir_all(tmp.path().join(".git")).unwrap();
        let replacement = git2::Repository::init(tmp.path()).expect("replacement init");
        let sig = git2::Signature::now("t", "t@x").expect("replacement signature");
        let tree_id = {
            let mut git_index = replacement.index().expect("replacement index");
            git_index.write_tree().expect("replacement tree")
        };
        let tree = replacement
            .find_tree(tree_id)
            .expect("replacement root tree");
        let replacement_root = git_test_helpers::commit_head_with_retry(
            &replacement,
            &sig,
            &sig,
            "replacement-root",
            &tree,
            &[],
        )
        .to_string();
        assert_ne!(
            original_root, replacement_root,
            "the fixture must replace repository lineage while retaining the same canonical path"
        );

        assert!(
            load_snapshot(tmp.path()).is_none(),
            "same-path repository replacement must reject the prior snapshot and its temporal state"
        );
    }

    #[test]
    fn same_path_repository_replacement_cannot_overwrite_foreign_snapshot() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let original = make_live_index_with_files(vec![("src/original.rs", b"fn original() {}\n")]);
        serialize_index(&original, tmp.path()).unwrap();

        std::fs::remove_dir_all(tmp.path().join(".git")).unwrap();
        let replacement = git2::Repository::init(tmp.path()).expect("replacement init");
        let sig = git2::Signature::now("t", "t@x").expect("replacement signature");
        let tree_id = {
            let mut git_index = replacement.index().expect("replacement index");
            git_index.write_tree().expect("replacement tree")
        };
        let tree = replacement.find_tree(tree_id).expect("replacement tree");
        git_test_helpers::commit_head_with_retry(
            &replacement,
            &sig,
            &sig,
            "foreign-root",
            &tree,
            &[],
        );

        let foreign = make_live_index_with_files(vec![("src/foreign.rs", b"fn foreign() {}\n")]);
        let result = serialize_index(&foreign, tmp.path());
        assert!(
            result.is_err(),
            "a replacement repository must not overwrite state owned by the prior lineage"
        );
        assert_eq!(quarantine_files_with_extension(tmp.path(), "bin").len(), 1);
        assert!(
            !tmp.path().join(".symforge/index.bin").exists(),
            "foreign state must be quarantined before a replacement snapshot may be written"
        );
    }

    #[test]
    fn same_path_repository_replacement_cannot_reset_foreign_snapshot() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let original = make_live_index_with_files(vec![("src/original.rs", b"fn original() {}\n")]);
        serialize_index(&original, tmp.path()).unwrap();

        std::fs::remove_dir_all(tmp.path().join(".git")).unwrap();
        let replacement = git2::Repository::init(tmp.path()).expect("replacement init");
        let sig = git2::Signature::now("t", "t@x").expect("replacement signature");
        let tree_id = {
            let mut git_index = replacement.index().expect("replacement index");
            git_index.write_tree().expect("replacement tree")
        };
        let tree = replacement.find_tree(tree_id).expect("replacement tree");
        git_test_helpers::commit_head_with_retry(
            &replacement,
            &sig,
            &sig,
            "foreign-root",
            &tree,
            &[],
        );

        let result = reset_snapshot_state(tmp.path());
        assert!(
            result.is_err(),
            "a replacement repository must not delete state owned by the prior lineage"
        );
        assert_eq!(quarantine_files_with_extension(tmp.path(), "bin").len(), 1);
        assert!(
            !tmp.path().join(".symforge/index.bin").exists(),
            "foreign state must be quarantined instead of reset in place"
        );
    }

    #[test]
    fn manifest_publication_snapshot_and_response_envelope_preserve_source_version() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("src/lib.rs"),
            b"pub fn dirty_worktree() {}\n",
        )
        .unwrap();

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let published = shared.published_generation();
        let manifest = published
            .manifest
            .as_ref()
            .expect("a bound published generation must carry its canonical manifest");
        let captured_version = manifest.source_version.clone();
        assert_eq!(
            captured_version.working_tree,
            WorkingTreeState::Dirty,
            "the fixture must exercise the closed dirty working-tree state"
        );

        let envelope = published
            .source_response_envelope()
            .expect("a bound generation must format one source envelope");
        checkpoint_shared_index(&shared, tmp.path(), &project_local_placement(tmp.path())).unwrap();
        let snapshot = load_snapshot(tmp.path()).expect("snapshot remains bound to this source");

        assert_eq!(manifest.source_version, captured_version);
        assert_eq!(snapshot.source_identity.source_version, captured_version);
        assert_eq!(envelope.source_version, captured_version);
        assert_eq!(snapshot.source_identity.manifest_digest, manifest.digest);
        assert_eq!(envelope.manifest_digest, manifest.digest);
    }

    #[test]
    fn all_working_tree_states_round_trip_without_digest_substitution() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/lib.rs", b"pub fn source() {}\n")]);

        for working_tree in [
            WorkingTreeState::Clean,
            WorkingTreeState::Dirty,
            WorkingTreeState::NotApplicable,
            WorkingTreeState::Unknown,
        ] {
            let mut snapshot = build_snapshot(capture_snapshot_build_input(&index), tmp.path());
            let version = SourceVersion {
                branch: Some("fixture-branch".to_string()),
                commit: Some("fixture-commit".to_string()),
                working_tree,
            };
            snapshot.manifest.source_version = version.clone();
            snapshot.source_identity.source_version = version.clone();
            snapshot.code_signals.computed_for_source_version = version.clone();
            let manifest_digest = snapshot.manifest.digest.clone();
            let header_manifest_digest = snapshot.source_identity.manifest_digest.clone();
            let resident_content_digest = snapshot.source_identity.indexed_content_digest.clone();

            let bytes = postcard::to_stdvec(&snapshot).unwrap();
            let restored: IndexSnapshot = postcard::from_bytes(&bytes).unwrap();

            assert_eq!(restored.manifest.source_version, version);
            assert_eq!(restored.source_identity.source_version, version);
            assert_eq!(restored.code_signals.computed_for_source_version, version);
            assert_eq!(restored.manifest.digest, manifest_digest);
            assert_eq!(
                restored.source_identity.manifest_digest,
                header_manifest_digest
            );
            assert_eq!(
                restored.source_identity.indexed_content_digest,
                resident_content_digest
            );
        }
    }

    #[test]
    fn snapshot_round_trip_restores_code_signals_into_published_generation() {
        let tmp = TempDir::new().unwrap();
        let shared = crate::live_index::SharedIndexHandle::shared(make_live_index_with_files(
            vec![("src/lib.rs", b"pub fn temporal() {}\n")],
        ));
        shared.remove_file("src/lib.rs");
        shared.update_git_temporal(
            crate::live_index::git_temporal::GitTemporalIndex::unavailable(
                "fixture-history-unavailable".to_string(),
            ),
        );
        let expected = shared.published_generation().code_signals.clone();
        assert!(
            expected.computed_for_content_generation > 0,
            "the fixture must prove provenance survives rather than coincidentally restoring generation zero"
        );
        checkpoint_shared_index(&shared, tmp.path(), &project_local_placement(tmp.path())).unwrap();

        let snapshot = load_snapshot(tmp.path()).expect("snapshot");
        let (live, code_signals) = snapshot_to_live_index_with_code_signals(snapshot, tmp.path());
        let restored =
            crate::live_index::SharedIndexHandle::shared_with_code_signals(live, code_signals);

        assert_eq!(
            restored.published_generation().code_signals.state,
            crate::live_index::git_temporal::GitTemporalState::Unavailable(
                "fixture-history-unavailable".to_string()
            )
        );
        let restored = restored.published_generation();
        assert_eq!(
            restored.code_signals.computed_for_content_generation,
            expected.computed_for_content_generation
        );
        assert_eq!(
            restored.code_signals.computed_for_source_version,
            expected.computed_for_source_version
        );
        assert_eq!(restored.code_signals.coverage, expected.coverage);
    }

    #[test]
    fn test_corrupt_bytes_quarantined_and_returns_none_no_panic() {
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

        let quarantine_bins = quarantine_files_with_extension(tmp.path(), "bin");
        assert_eq!(
            quarantine_bins.len(),
            1,
            "corrupt bytes should be preserved in quarantine"
        );
        assert_eq!(
            std::fs::read(&quarantine_bins[0]).unwrap(),
            b"not valid postcard data xyzzy 12345",
            "quarantine must preserve original corrupt bytes"
        );
        assert!(
            !dir.join("index.bin").exists(),
            "corrupt active snapshot should be removed after quarantine"
        );
        let quarantine_metadata = quarantine_files_with_extension(tmp.path(), "json");
        assert_eq!(quarantine_metadata.len(), 1);
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&quarantine_metadata[0]).unwrap()).unwrap();
        assert_eq!(metadata["reason"], "deserialize-error");
    }

    #[test]
    fn test_truncated_bytes_quarantined_and_returns_none_no_panic() {
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

        let quarantine_bins = quarantine_files_with_extension(tmp.path(), "bin");
        assert_eq!(
            quarantine_bins.len(),
            1,
            "truncated bytes should be preserved in quarantine"
        );
        assert_eq!(std::fs::read(&quarantine_bins[0]).unwrap(), truncated);
    }

    #[test]
    fn test_missing_file_returns_none() {
        let tmp = TempDir::new().unwrap();
        // No .symforge/index.bin exists
        let result = load_snapshot(tmp.path());
        assert!(result.is_none(), "missing file must return None");
        assert!(
            quarantine_files_with_extension(tmp.path(), "bin").is_empty(),
            "missing first-run snapshot should not create quarantine artifacts"
        );
    }

    // ── stat_check_files_from_view tests ──────────────────────────────────────

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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
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

        let result = stat_check_files_from_view(&capture_verify_view(&index), &mtimes, tmp.path());
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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        let result =
            stat_check_files_from_view(&capture_verify_view(&index), &HashMap::new(), tmp.path());
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

        let result =
            stat_check_files_from_view(&capture_verify_view(&index), &HashMap::new(), tmp.path());
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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        // Sample 100% to ensure the file is included
        let mismatches =
            spot_verify_sample_from_view(&capture_verify_view(&index), tmp.path(), 1.0);
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
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        let mismatches =
            spot_verify_sample_from_view(&capture_verify_view(&index), tmp.path(), 1.0);
        assert!(mismatches.is_empty(), "no mismatch when hash is current");
    }

    #[test]
    fn test_spot_verify_empty_index_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![]);
        let mismatches =
            spot_verify_sample_from_view(&capture_verify_view(&index), tmp.path(), 0.10);
        assert!(mismatches.is_empty(), "empty index returns empty vec");
    }

    // ── Snapshot atomicity test ───────────────────────────────────────────────

    #[test]
    fn test_snapshot_path_locks_isolate_distinct_projects() {
        let first = TempDir::new().expect("create first project");
        let second = TempDir::new().expect("create second project");

        let (_, first_lock) =
            snapshot_path_and_lock(first.path()).expect("resolve first snapshot lock");
        let (_, same_lock) = snapshot_path_and_lock(&first.path().join("."))
            .expect("resolve canonical-equivalent snapshot lock");
        let (_, second_lock) =
            snapshot_path_and_lock(second.path()).expect("resolve second snapshot lock");

        assert!(
            Arc::ptr_eq(&first_lock, &same_lock),
            "canonical-equivalent snapshot paths must share one lock"
        );
        assert!(
            !Arc::ptr_eq(&first_lock, &second_lock),
            "different snapshot paths must not share a lock"
        );

        let first_guard = first_lock.lock().expect("lock first snapshot path");
        assert!(
            same_lock.try_lock().is_err(),
            "same-path writes must serialize"
        );
        assert!(
            second_lock.try_lock().is_ok(),
            "different snapshot paths must not block each other"
        );
        drop(first_guard);
    }

    #[test]
    fn test_reset_snapshot_state_waits_for_snapshot_path_lock() {
        let tmp = TempDir::new().expect("create project");
        let symforge_dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&symforge_dir).expect("create .symforge");
        let snapshot_path = symforge_dir.join(INDEX_FILENAME);
        serialize_index(
            &make_live_index_with_files(vec![("src/lib.rs", b"fn source_file() {}\n")]),
            tmp.path(),
        )
        .expect("write valid owned snapshot");

        let (_, snapshot_lock) = snapshot_path_and_lock(tmp.path()).expect("resolve snapshot lock");
        let write_guard = snapshot_lock.lock().expect("hold snapshot write lock");
        let project_root = tmp.path().to_path_buf();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let reset_thread = std::thread::spawn(move || {
            started_tx.send(()).expect("signal reset start");
            let report = reset_snapshot_state(&project_root);
            done_tx.send(report).expect("send reset result");
        });

        started_rx.recv().expect("reset thread started");
        let early_result = done_rx.recv_timeout(Duration::from_millis(200)).ok();
        let snapshot_survived_while_locked = snapshot_path.exists();
        drop(write_guard);

        let completed_early = early_result.is_some();
        let report = match early_result {
            Some(report) => report,
            None => done_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("reset should finish after releasing lock"),
        }
        .expect("reset snapshot state");
        reset_thread.join().expect("join reset thread");

        assert!(
            !completed_early,
            "reset must block while a snapshot write owns the path lock"
        );
        assert!(
            snapshot_survived_while_locked,
            "reset must not delete the published snapshot during a write"
        );
        assert_eq!(report.removed_count(), 1);
        assert!(!snapshot_path.exists());
    }

    /// Recovered finding #10: quarantine removes the ACTIVE snapshot, so it must
    /// serialize on the same per-path lock as `write_snapshot` /
    /// `reset_snapshot_state` — otherwise it can delete `index.bin` in the
    /// middle of an atomic publish.
    #[test]
    fn test_quarantine_waits_for_snapshot_path_lock() {
        let tmp = TempDir::new().expect("create project");
        let symforge_dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&symforge_dir).expect("create .symforge");
        let snapshot_path = symforge_dir.join(INDEX_FILENAME);
        std::fs::write(&snapshot_path, b"corrupt snapshot bytes").expect("write snapshot");

        let (_, snapshot_lock) = snapshot_path_and_lock(tmp.path()).expect("resolve snapshot lock");
        let write_guard = snapshot_lock.lock().expect("hold snapshot write lock");
        let project_root = tmp.path().to_path_buf();
        let thread_snapshot_path = snapshot_path.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let quarantine_thread = std::thread::spawn(move || {
            started_tx.send(()).expect("signal quarantine start");
            let result = quarantine_bad_snapshot(
                &project_root,
                &thread_snapshot_path,
                b"corrupt snapshot bytes",
                "test-reason",
                "test detail".to_string(),
            );
            done_tx.send(result).expect("send quarantine result");
        });

        started_rx.recv().expect("quarantine thread started");
        let early_result = done_rx.recv_timeout(Duration::from_millis(200)).ok();
        let snapshot_survived_while_locked = snapshot_path.exists();
        drop(write_guard);

        let completed_early = early_result.is_some();
        let result = match early_result {
            Some(result) => result,
            None => done_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("quarantine should finish after releasing lock"),
        }
        .expect("quarantine bad snapshot");
        quarantine_thread.join().expect("join quarantine thread");

        assert!(
            !completed_early,
            "quarantine must block while a snapshot write owns the path lock"
        );
        assert!(
            snapshot_survived_while_locked,
            "quarantine must not delete the published snapshot during a write"
        );
        assert!(result.exists(), "quarantine artifact written");
        assert!(!snapshot_path.exists(), "bad snapshot removed after lock");
    }

    #[test]
    fn test_snapshot_temp_paths_are_unique_and_process_scoped() {
        let tmp = TempDir::new().expect("create project");
        let symforge_dir = paths::ensure_symforge_dir(tmp.path()).expect("create .symforge");

        let first = next_snapshot_temp_path(&symforge_dir);
        let second = next_snapshot_temp_path(&symforge_dir);
        let process_prefix = format!("{INDEX_TMP_FILENAME}.{}.", std::process::id());

        assert_ne!(
            first, second,
            "each snapshot write needs a unique temp path"
        );
        for path in [first, second] {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("temp path has a UTF-8 file name");
            assert!(
                name.starts_with(&process_prefix),
                "temp name must include the writer PID and counter: {name}"
            );
        }
    }

    #[test]
    fn test_snapshot_write_error_removes_owned_temp_file() {
        let tmp = TempDir::new().expect("create project");
        let symforge_dir = paths::ensure_symforge_dir(tmp.path()).expect("create .symforge");
        let blocking_final_path = symforge_dir.join(INDEX_FILENAME);
        std::fs::create_dir(&blocking_final_path).expect("block snapshot rename with directory");
        std::fs::write(
            blocking_final_path.join("sentinel"),
            b"keep directory non-empty",
        )
        .expect("write blocking sentinel");
        let index = make_live_index_with_files(vec![("src/lib.rs", b"fn lib() {}")]);

        serialize_index(&index, tmp.path()).expect_err("snapshot rename should fail");

        let leftover_temps: Vec<_> = std::fs::read_dir(&symforge_dir)
            .expect("list .symforge")
            .map(|entry| entry.expect("read .symforge entry"))
            .filter(|entry| is_unique_snapshot_temp_name(&entry.file_name()))
            .map(|entry| entry.path())
            .collect();
        assert!(
            leftover_temps.is_empty(),
            "failed writes must remove their own temp file: {leftover_temps:?}"
        );
    }

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
        serialize_index(
            &make_live_index_with_files(vec![("src/lib.rs", b"fn source_file() {}\n")]),
            tmp.path(),
        )
        .expect("write valid owned snapshot");
        std::fs::write(symforge_dir.join("index.bin.tmp"), b"stale tmp").expect("write tmp");
        let stale_unique_tmp = symforge_dir.join("index.bin.tmp.4242.7");
        let unrelated_similar_name = symforge_dir.join("index.bin.tmp.not-owned");
        std::fs::write(&stale_unique_tmp, b"stale unique tmp").expect("write unique tmp");
        std::fs::write(&unrelated_similar_name, b"unrelated").expect("write similar sentinel");
        std::fs::write(symforge_dir.join("frecency.db"), b"unrelated").expect("write sentinel");

        let report = reset_snapshot_state(tmp.path()).expect("reset snapshot state");

        assert_eq!(report.removed_count(), 3);
        assert!(!symforge_dir.join("index.bin").exists());
        assert!(!symforge_dir.join("index.bin.tmp").exists());
        assert!(!stale_unique_tmp.exists());
        assert!(
            unrelated_similar_name.exists(),
            "reset must only remove PID-and-counter temp names"
        );
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
        let project_state =
            crate::domain::ProjectStateDir::new(tmp.path().join(paths::SYMFORGE_DIR_NAME));
        init_frecency_store(tmp.path(), &project_state);
        assert!(
            !crate::live_index::frecency::frecency_db_path(&project_state).exists(),
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
        let project_state =
            crate::domain::ProjectStateDir::new(tmp.path().join(paths::SYMFORGE_DIR_NAME));
        init_frecency_store(tmp.path(), &project_state);
        clear_frecency_flag();
        let db_path = crate::live_index::frecency::frecency_db_path(&project_state);
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

    // ── Team artifact tests (Program 015 S1a, C-S1A-005) ──────────────────────

    #[test]
    fn team_artifact_export_refuses_non_project_local_or_protected_binding() {
        let index = make_live_index_with_files(vec![("src/lib.rs", b"pub fn indexed() {}")]);
        let available = CapabilityStatus::Available;

        let protected = TempDir::new().unwrap();
        let protected_binding =
            artifact_binding(protected.path(), SourceAccessMode::ExplicitProtected);
        export_artifact(
            &index,
            &protected_binding.canonical_root,
            protected_binding.access_mode,
            &project_local_placement(&protected_binding.canonical_root),
            &available,
        )
        .expect_err("protected bindings must refuse team-artifact export");
        assert_no_team_artifact_mutation(&protected_binding.canonical_root);

        let relocated = TempDir::new().unwrap();
        let relocated_binding = artifact_binding(relocated.path(), SourceAccessMode::NormalProject);
        let global_state = TempDir::new().unwrap();
        let relocated_placement = StatePlacement::UserLocal {
            directory: ProjectStateDir::new(global_state.path().join("project-state")),
            root_id: relocated_binding.root_id.clone(),
            reason: UserLocalPlacementReason::ProjectLocalUnavailable {
                safe_reason: AccessErrorKind::PermissionDenied,
            },
        };
        export_artifact(
            &index,
            &relocated_binding.canonical_root,
            relocated_binding.access_mode,
            &relocated_placement,
            &available,
        )
        .expect_err("non-project-local bindings must refuse team-artifact export");
        assert_no_team_artifact_mutation(&relocated_binding.canonical_root);
        assert!(!global_state.path().join("project-state").exists());

        let read_only = TempDir::new().unwrap();
        let read_only_binding = artifact_binding(read_only.path(), SourceAccessMode::NormalProject);
        export_artifact(
            &index,
            &read_only_binding.canonical_root,
            read_only_binding.access_mode,
            &project_local_placement(&read_only_binding.canonical_root),
            &CapabilityStatus::Unavailable {
                reason: CapabilityUnavailableReason::SourceReadOnly,
            },
        )
        .expect_err("source-read-only capability must refuse before mutation");
        assert_no_team_artifact_mutation(&read_only_binding.canonical_root);

        let tracked = TempDir::new().unwrap();
        let tracked_repo = git2::Repository::init(tracked.path()).expect("init tracked repo");
        std::fs::create_dir_all(tracked.path().join(".symforge")).unwrap();
        std::fs::write(tracked.path().join(".symforge/index.bin.zst"), b"tracked").unwrap();
        let mut tracked_index = tracked_repo.index().unwrap();
        tracked_index
            .add_path(Path::new(".symforge/index.bin.zst"))
            .unwrap();
        tracked_index.write().unwrap();
        let tracked_binding = artifact_binding(tracked.path(), SourceAccessMode::NormalProject);
        let tracked_report = export_artifact(
            &index,
            &tracked_binding.canonical_root,
            tracked_binding.access_mode,
            &project_local_placement(&tracked_binding.canonical_root),
            &available,
        )
        .expect("tracked export");
        assert_eq!(tracked_report.git_visibility.as_str(), "already_tracked");

        let visible = TempDir::new().unwrap();
        git2::Repository::init(visible.path()).expect("init visible repo");
        let visible_binding = artifact_binding(visible.path(), SourceAccessMode::NormalProject);
        let visible_report = export_artifact(
            &index,
            &visible_binding.canonical_root,
            visible_binding.access_mode,
            &project_local_placement(&visible_binding.canonical_root),
            &available,
        )
        .expect("visible export");
        assert_eq!(visible_report.git_visibility.as_str(), "untracked_visible");

        let ignored = TempDir::new().unwrap();
        git2::Repository::init(ignored.path()).expect("init ignored repo");
        std::fs::write(ignored.path().join(".gitignore"), "/.symforge/\n").unwrap();
        let ignored_binding = artifact_binding(ignored.path(), SourceAccessMode::NormalProject);
        let ignored_report = export_artifact(
            &index,
            &ignored_binding.canonical_root,
            ignored_binding.access_mode,
            &project_local_placement(&ignored_binding.canonical_root),
            &available,
        )
        .expect("ignored export");
        assert_eq!(
            ignored_report.git_visibility.as_str(),
            "ignored_force_add_required"
        );

        let unavailable = TempDir::new().unwrap();
        let unavailable_binding =
            artifact_binding(unavailable.path(), SourceAccessMode::NormalProject);
        let unavailable_report = export_artifact(
            &index,
            &unavailable_binding.canonical_root,
            unavailable_binding.access_mode,
            &project_local_placement(&unavailable_binding.canonical_root),
            &available,
        )
        .expect("non-Git export remains available with an honest receipt");
        assert_eq!(
            unavailable_report.git_visibility.as_str(),
            "git_visibility_unavailable"
        );
    }

    #[test]
    fn test_export_artifact_writes_metadata_and_gitattributes_hint() {
        let tmp = TempDir::new().unwrap();
        let index =
            make_live_index_with_files(vec![("a.rs", b"fn a() {}"), ("b.rs", b"fn b() {}")]);

        let report = export_artifact_legacy(&index, tmp.path()).expect("export should succeed");
        assert_eq!(report.files, 2);
        assert!(report.path.exists(), "index.bin.zst should exist");
        assert!(report.metadata_path.exists(), "artifact.json should exist");
        assert_eq!(report.path.file_name().unwrap(), ARTIFACT_FILENAME);

        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report.metadata_path).unwrap()).unwrap();
        assert_eq!(metadata["content_hash"], report.content_hash);
        assert_eq!(metadata["files"], 2);

        // A-US2-04: .gitattributes carries the merge=ours hint.
        let gitattributes = std::fs::read_to_string(tmp.path().join(".gitattributes")).unwrap();
        assert!(
            gitattributes
                .lines()
                .any(|line| line.trim() == "*.zst merge=ours"),
            "expected *.zst merge=ours hint, got: {gitattributes:?}"
        );

        // Idempotent: exporting again must not duplicate the gitattributes hint.
        export_artifact_legacy(&index, tmp.path()).expect("second export should succeed");
        let gitattributes_again =
            std::fs::read_to_string(tmp.path().join(".gitattributes")).unwrap();
        assert_eq!(
            gitattributes_again.matches("merge=ours").count(),
            1,
            "repeat export must not duplicate the gitattributes hint"
        );
    }

    #[test]
    fn test_artifact_round_trip_is_byte_exact_and_preserves_content_hash() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![
            ("src/foo.rs", b"fn foo() { bar(); }"),
            ("src/bar.py", b"def bar():\n    pass\n"),
        ]);

        let (before, raw, compressed) =
            build_compressed_snapshot(&index, tmp.path()).expect("build compressed snapshot");
        let decompressed = decompress_artifact_bytes(&compressed).expect("decompress");
        let after: IndexSnapshot = postcard::from_bytes(&decompressed).expect("decode postcard");

        // SP-0B caveat (a), adapted: `IndexSnapshot.files` is a `HashMap`, so
        // re-serializing the *deserialized* snapshot is not guaranteed to
        // reproduce the original bytes (HashMap iteration order is not part
        // of its API contract, even for identical logical content). The
        // property that IS deterministic — and is the real "whole-snapshot
        // byte-exact" guarantee this artifact needs — is the zstd round trip
        // itself: decompression must reproduce the exact pre-compression
        // postcard bytes.
        assert_eq!(
            decompressed, raw,
            "zstd decompress must reproduce the pre-compression postcard bytes exactly"
        );

        for (path, before_file) in &before.files {
            let after_file = after
                .files
                .get(path)
                .expect("file present after round trip");
            assert_eq!(after_file.content_hash, before_file.content_hash);
        }
    }

    #[test]
    fn test_load_snapshot_imports_artifact_when_bin_missing() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}")]);

        export_artifact_legacy(&index, tmp.path()).expect("export should succeed");
        assert!(
            !tmp.path().join(".symforge").join("index.bin").exists(),
            "only the .zst artifact should exist for this scenario"
        );

        let snapshot = load_snapshot(tmp.path()).expect("load_snapshot should import the artifact");
        let file = snapshot
            .files
            .get("src/main.rs")
            .expect("imported snapshot should contain the file");
        assert_eq!(file.content_hash, crate::hash::digest_hex(b"fn main() {}"));
    }

    #[test]
    fn same_path_repository_replacement_quarantines_foreign_team_artifact() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn original() {}")]);
        export_artifact_legacy(&index, tmp.path()).expect("export should succeed");

        std::fs::remove_dir_all(tmp.path().join(".git")).unwrap();
        let replacement = git2::Repository::init(tmp.path()).expect("replacement init");
        let sig = git2::Signature::now("t", "t@x").expect("replacement signature");
        let tree_id = {
            let mut git_index = replacement.index().expect("replacement index");
            git_index.write_tree().expect("replacement tree")
        };
        let tree = replacement.find_tree(tree_id).expect("replacement tree");
        git_test_helpers::commit_head_with_retry(
            &replacement,
            &sig,
            &sig,
            "foreign-root",
            &tree,
            &[],
        );

        assert!(
            load_snapshot(tmp.path()).is_none(),
            "foreign team artifacts must never hydrate a same-path replacement"
        );
        let artifact_path = tmp.path().join(".symforge").join(ARTIFACT_FILENAME);
        assert!(
            !artifact_path.exists(),
            "foreign team artifact must leave the active import path"
        );
        let quarantined: Vec<_> =
            std::fs::read_dir(paths::resolve_artifact_quarantine_dir(tmp.path()))
                .expect("foreign artifact quarantine must exist")
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().extension().and_then(|ext| ext.to_str()) == Some("zst")
                })
                .collect();
        assert_eq!(quarantined.len(), 1);
    }

    #[test]
    fn test_load_snapshot_quarantines_corrupt_artifact_and_returns_none() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}")]);
        export_artifact_legacy(&index, tmp.path()).expect("export should succeed");

        let artifact_path = tmp.path().join(".symforge").join(ARTIFACT_FILENAME);
        let good = std::fs::read(&artifact_path).unwrap();
        std::fs::write(&artifact_path, &good[..good.len() / 2]).unwrap();

        let result = load_snapshot(tmp.path());
        assert!(
            result.is_none(),
            "corrupt artifact must fall back to a full re-index, not partial-serve"
        );
        assert!(
            !artifact_path.exists(),
            "corrupt artifact should be removed from the active path after quarantine"
        );

        let quarantine_dir = paths::resolve_artifact_quarantine_dir(tmp.path());
        let quarantined: Vec<_> = std::fs::read_dir(&quarantine_dir)
            .expect("quarantine dir should exist")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("zst"))
            .collect();
        assert_eq!(
            quarantined.len(),
            1,
            "corrupt artifact should be quarantined under .symforge/quarantine/artifacts/"
        );
    }

    #[test]
    fn test_load_snapshot_quarantines_artifact_with_missing_sidecar() {
        // The artifact and its sidecar are written non-atomically
        // (export_artifact: rename the .zst, then a separate std::fs::write of
        // artifact.json). A crash between them — or a partial checkout that
        // picked up the .zst but not the .json — leaves a VALID compressed
        // artifact with NO sidecar. content_hash is then unverifiable, so the
        // import path must quarantine and fall back, not silently trust it.
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}")]);
        export_artifact_legacy(&index, tmp.path()).expect("export should succeed");

        let artifact_path = tmp.path().join(".symforge").join(ARTIFACT_FILENAME);
        let metadata_path = tmp
            .path()
            .join(".symforge")
            .join(ARTIFACT_METADATA_FILENAME);
        std::fs::remove_file(&metadata_path).expect("remove sidecar");
        assert!(
            artifact_path.exists(),
            "artifact bytes remain valid — only the sidecar is gone"
        );

        let result = load_snapshot(tmp.path());
        assert!(
            result.is_none(),
            "a valid artifact with a missing sidecar must NOT be silently trusted; \
             content_hash is unverifiable, so import must quarantine and fall back"
        );
        assert!(
            !artifact_path.exists(),
            "unverifiable artifact should be removed from the active path after quarantine"
        );

        let quarantine_dir = paths::resolve_artifact_quarantine_dir(tmp.path());
        let quarantined: Vec<_> = std::fs::read_dir(&quarantine_dir)
            .expect("quarantine dir should exist")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("zst"))
            .collect();
        assert_eq!(
            quarantined.len(),
            1,
            "missing-sidecar artifact should be quarantined under .symforge/quarantine/artifacts/"
        );

        let quarantine_json: Vec<_> = std::fs::read_dir(&quarantine_dir)
            .expect("quarantine dir should exist")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();
        assert_eq!(quarantine_json.len(), 1);
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(quarantine_json[0].path()).unwrap()).unwrap();
        assert_eq!(
            metadata["reason"], "missing-sidecar",
            "quarantine metadata must record the missing-sidecar integrity failure"
        );
    }
}
