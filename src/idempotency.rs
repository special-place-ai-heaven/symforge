use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::domain::{ControlStateDir, ProjectStateDir};
use crate::{hash, paths};

const KEY_HASH_FRAME_PREFIX: &[u8] = b"symforge-idempotency-key-v1\0";
const REQUEST_HASH_FRAME_PREFIX: &[u8] = b"symforge-idempotency-request-v1\0";
const REPLAY_RECORD_SCHEMA_VERSION: u8 = 1;
const RECORD_FILE_NAME: &str = "record.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new(raw: impl Into<String>) -> Result<Self, IdempotencyError> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(IdempotencyError::EmptyKey);
        }
        Ok(Self(raw))
    }

    fn key_hash(&self) -> String {
        let mut frame = Vec::with_capacity(KEY_HASH_FRAME_PREFIX.len() + self.0.len());
        frame.extend_from_slice(KEY_HASH_FRAME_PREFIX);
        frame.extend_from_slice(self.0.as_bytes());
        hash::digest_hex(&frame)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestHash(String);

impl RequestHash {
    pub fn for_tool_request(tool_name: &str, request: &Value) -> Result<Self, IdempotencyError> {
        if tool_name.is_empty() {
            return Err(IdempotencyError::EmptyToolName);
        }

        let canonical = canonical_json_bytes(request)?;
        let mut frame = Vec::with_capacity(
            REQUEST_HASH_FRAME_PREFIX.len() + tool_name.len() + 1 + canonical.len(),
        );
        frame.extend_from_slice(REQUEST_HASH_FRAME_PREFIX);
        frame.extend_from_slice(tool_name.as_bytes());
        frame.push(0);
        frame.extend_from_slice(&canonical);

        Ok(Self(hash::digest_hex(&frame)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RequestHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayStatus {
    Reserved,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayRecord {
    pub schema_version: u8,
    pub key_hash: String,
    pub request_hash: RequestHash,
    pub status: ReplayStatus,
    pub created_unix_millis: u64,
    pub updated_unix_millis: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_text: Option<String>,
    /// Source-bound operation receipt (Feature 020 Slice 4, the
    /// replay-authority fence): the disk bytes the completed operation left
    /// behind, read back at completion time. A stored response may be
    /// replayed ONLY while every target still holds these bytes; a record
    /// without a receipt never replays through the verified lanes. Absent on
    /// v1 records (serde default), which is exactly the fail-closed case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_image: Option<PostImageReceipt>,
}

/// One target the completed operation left on disk. `path` is the ABSOLUTE
/// path as actually written (edits can be rerouted into a worktree, so the
/// request-relative path is not always where the bytes landed). Absolute
/// paths make the record machine-local; the record's whole job is a
/// retry-window guard on this machine's project state, so that is the
/// correct scope. `content_digest: None` records the path as ABSENT (a
/// delete or rename-away), which is verified as absence, not skipped.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PostImageTarget {
    pub path: String,
    pub content_digest: Option<String>,
}

/// The post-image the operation's completion observed: (path → digest) for
/// every file the operation wrote or removed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PostImageReceipt {
    pub targets: Vec<PostImageTarget>,
}

/// Digest a SINGLE target's post-image from bytes already in hand — the
/// caller's own just-written content — rather than reopening the file from
/// disk. T038 round-1 repair: `capture_post_image`'s disk re-read ran after
/// the write's permit was released (reindex, hooks, and formatting all run
/// between write and the original capture point), an unfenced window in
/// which a concurrent writer's bytes could be digested and bound to THIS
/// response's receipt. The single-target edit tools hold the bytes they
/// wrote for the rest of the call; using them here makes the receipt
/// describe exactly what THIS operation committed, with no reopen and no
/// window at all.
pub fn post_image_from_written_bytes(path: &Path, bytes: &[u8]) -> PostImageReceipt {
    PostImageReceipt {
        targets: vec![PostImageTarget {
            path: path.display().to_string(),
            content_digest: Some(crate::hash::digest_hex(bytes)),
        }],
    }
}

/// Read back the current bytes of the written paths and digest them into a
/// receipt. A missing file records absence; any OTHER read error returns
/// `None` — capture failure means NO receipt, and a record without a receipt
/// never replays (fail closed rather than binding bytes nobody read).
///
/// For a single target whose bytes are already known, prefer
/// [`post_image_from_written_bytes`] — this disk re-read exists for the
/// batch executors, which return only written PATHS to `edit_tools.rs`, not
/// their per-file content.
pub fn capture_post_image(written: &[PathBuf]) -> Option<PostImageReceipt> {
    let mut targets = Vec::with_capacity(written.len());
    for path in written {
        let content_digest = match fs::read(path) {
            Ok(bytes) => Some(crate::hash::digest_hex(&bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(_) => return None,
        };
        targets.push(PostImageTarget {
            path: path.display().to_string(),
            content_digest,
        });
    }
    Some(PostImageReceipt { targets })
}

/// True only when every receipt target matches the CURRENT disk state:
/// present targets byte-hash-equal, absent targets still absent. An empty
/// receipt verifies trivially — callers must capture every written path.
pub fn verify_post_image(receipt: &PostImageReceipt) -> bool {
    receipt
        .targets
        .iter()
        .all(|target| match fs::read(Path::new(&target.path)) {
            Ok(bytes) => target
                .content_digest
                .as_deref()
                .is_some_and(|digest| digest == crate::hash::digest_hex(&bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                target.content_digest.is_none()
            }
            Err(_) => false,
        })
}

impl ReplayRecord {
    fn reserved(key_hash: String, request_hash: RequestHash) -> Self {
        let now = unix_millis();
        Self {
            schema_version: REPLAY_RECORD_SCHEMA_VERSION,
            key_hash,
            request_hash,
            status: ReplayStatus::Reserved,
            created_unix_millis: now,
            updated_unix_millis: now,
            response_text: None,
            post_image: None,
        }
    }

    fn with_status_and_response(
        mut self,
        status: ReplayStatus,
        response_text: Option<String>,
    ) -> Self {
        self.status = status;
        self.updated_unix_millis = unix_millis();
        self.response_text = response_text;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayDecision {
    FirstExecution(ReplayRecord),
    Replay(ReplayRecord),
}

#[derive(Debug, Clone)]
pub struct ActiveReplay {
    store: FileReplayStore,
    key: IdempotencyKey,
    request_hash: RequestHash,
}

impl ActiveReplay {
    pub fn complete(
        &self,
        response_text: impl Into<String>,
    ) -> Result<ReplayRecord, IdempotencyError> {
        self.store.update_status_with_response(
            &self.key,
            &self.request_hash,
            ReplayStatus::Completed,
            Some(response_text.into()),
        )
    }

    /// Complete with the source-bound receipt the verified replay lanes
    /// require. A `None` receipt stores a record that will never replay
    /// through those lanes (capture failed — fail closed).
    pub fn complete_with_post_image(
        &self,
        response_text: impl Into<String>,
        post_image: Option<PostImageReceipt>,
    ) -> Result<ReplayRecord, IdempotencyError> {
        let record = self.store.update_status_with_response(
            &self.key,
            &self.request_hash,
            ReplayStatus::Completed,
            Some(response_text.into()),
        )?;
        let mut record = record;
        record.post_image = post_image;
        self.store.write_record_atomic(&record)?;
        Ok(record)
    }

    pub fn fail(&self, response_text: impl Into<String>) -> Result<ReplayRecord, IdempotencyError> {
        self.store.update_status_with_response(
            &self.key,
            &self.request_hash,
            ReplayStatus::Failed,
            Some(response_text.into()),
        )
    }
}

#[derive(Debug, Clone)]
pub enum ReplayStart {
    FirstExecution(ActiveReplay),
    Replay(String),
}

#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("idempotency key cannot be empty")]
    EmptyKey,
    #[error("tool name cannot be empty for idempotency request hashing")]
    EmptyToolName,
    #[error(
        "idempotency conflict for key hash {key_hash}: existing request {existing}, incoming request {incoming}"
    )]
    Conflict {
        key_hash: String,
        existing: RequestHash,
        incoming: RequestHash,
    },
    #[error("idempotency reservation for key hash {key_hash} is incomplete at {path}")]
    IncompleteReservation { key_hash: String, path: PathBuf },
    #[error(
        "idempotency record at {path} is corrupt and was quarantined at {quarantine_path}: {reason}"
    )]
    CorruptRecordQuarantined {
        path: PathBuf,
        quarantine_path: PathBuf,
        reason: String,
    },
    #[error("idempotency record at {path} is corrupt and could not be quarantined: {reason}")]
    CorruptRecord {
        path: PathBuf,
        reason: String,
        quarantine_error: String,
    },
    #[error("idempotency I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("idempotency JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Age past which a supersede marker is an orphan (its owner crashed between
/// two adjacent fs writes) and may be reclaimed. Generous against clock skew;
/// a healthy claim lives for microseconds.
const SUPERSEDE_MARKER_STALE: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct FileReplayStore {
    records_dir: PathBuf,
    quarantine_dir: PathBuf,
}

impl FileReplayStore {
    pub fn open(project_state: &ProjectStateDir) -> Result<Self, IdempotencyError> {
        Self::open_in(paths::project_state_path(
            project_state,
            paths::IDEMPOTENCY_DIR_NAME,
        ))
    }

    pub fn open_control(control_state: &ControlStateDir) -> Result<Self, IdempotencyError> {
        Self::open_in(paths::control_state_path(
            control_state,
            paths::IDEMPOTENCY_DIR_NAME,
        ))
    }

    fn open_in(idempotency_dir: PathBuf) -> Result<Self, IdempotencyError> {
        fs::create_dir_all(&idempotency_dir)?;
        let records_dir = idempotency_dir.join("records");
        let quarantine_dir = idempotency_dir.join("quarantine");
        fs::create_dir_all(&records_dir)?;
        fs::create_dir_all(&quarantine_dir)?;
        Ok(Self {
            records_dir,
            quarantine_dir,
        })
    }

    pub fn check_or_reserve(
        &self,
        key: &IdempotencyKey,
        request_hash: &RequestHash,
    ) -> Result<ReplayDecision, IdempotencyError> {
        let key_hash = key.key_hash();
        let key_dir = self.key_dir_for_hash(&key_hash);

        match fs::create_dir(&key_dir) {
            Ok(()) => {
                let record = ReplayRecord::reserved(key_hash, request_hash.clone());
                self.write_record_atomic(&record)?;
                Ok(ReplayDecision::FirstExecution(record))
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let record = self.load_existing(&key_hash)?;
                self.ensure_same_hash(&record, request_hash)?;
                Ok(ReplayDecision::Replay(record))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(&self.records_dir)?;
                self.check_or_reserve(key, request_hash)
            }
            Err(error) => Err(IdempotencyError::Io(error)),
        }
    }

    pub fn replay_if_present(
        &self,
        key: &IdempotencyKey,
        request_hash: &RequestHash,
    ) -> Result<Option<ReplayRecord>, IdempotencyError> {
        let key_hash = key.key_hash();
        let key_dir = self.key_dir_for_hash(&key_hash);
        if !key_dir.exists() {
            return Ok(None);
        }

        let record = self.load_existing(&key_hash)?;
        self.ensure_same_hash(&record, request_hash)?;
        Ok(Some(record))
    }

    pub fn update_status(
        &self,
        key: &IdempotencyKey,
        request_hash: &RequestHash,
        status: ReplayStatus,
    ) -> Result<ReplayRecord, IdempotencyError> {
        self.update_status_with_response(key, request_hash, status, None)
    }

    pub fn update_status_with_response(
        &self,
        key: &IdempotencyKey,
        request_hash: &RequestHash,
        status: ReplayStatus,
        response_text: Option<String>,
    ) -> Result<ReplayRecord, IdempotencyError> {
        let key_hash = key.key_hash();
        let record = self.load_existing(&key_hash)?;
        self.ensure_same_hash(&record, request_hash)?;
        let updated = record.with_status_and_response(status, response_text);
        self.write_record_atomic(&updated)?;
        Ok(updated)
    }

    pub fn record_path(&self, key: &IdempotencyKey) -> PathBuf {
        self.record_path_for_hash(&key.key_hash())
    }

    /// One-winner supersede claim (T038 round-1): `create_new` is the same
    /// atomic first-claim primitive `check_or_reserve` uses, so exactly one
    /// of N concurrent contenders superseding the same unverified record may
    /// retake its reservation — the losers answer as reserved instead of
    /// double-executing the mutation. A marker orphaned by a crash between
    /// claim and release (two adjacent fs writes) heals by age.
    pub fn try_claim_supersede(&self, key_hash: &str) -> Result<bool, IdempotencyError> {
        let marker = self.supersede_marker_path(key_hash);
        for attempt in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)
            {
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let stale = fs::metadata(&marker)
                        .and_then(|meta| meta.modified())
                        .ok()
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > SUPERSEDE_MARKER_STALE);
                    if attempt == 0 && stale {
                        let _ = fs::remove_file(&marker);
                        continue;
                    }
                    return Ok(false);
                }
                Err(error) => return Err(IdempotencyError::Io(error)),
            }
        }
        Ok(false)
    }

    /// Release a claim taken by [`Self::try_claim_supersede`]. Best-effort:
    /// an unremovable marker degrades to the age-healing path.
    pub fn release_supersede(&self, key_hash: &str) {
        let _ = fs::remove_file(self.supersede_marker_path(key_hash));
    }

    fn supersede_marker_path(&self, key_hash: &str) -> PathBuf {
        self.records_dir.join(format!("{key_hash}.superseding"))
    }

    fn ensure_same_hash(
        &self,
        record: &ReplayRecord,
        incoming: &RequestHash,
    ) -> Result<(), IdempotencyError> {
        if record.request_hash == *incoming {
            return Ok(());
        }
        Err(IdempotencyError::Conflict {
            key_hash: record.key_hash.clone(),
            existing: record.request_hash.clone(),
            incoming: incoming.clone(),
        })
    }

    fn load_existing(&self, key_hash: &str) -> Result<ReplayRecord, IdempotencyError> {
        let path = self.record_path_for_hash(key_hash);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(IdempotencyError::IncompleteReservation {
                    key_hash: key_hash.to_string(),
                    path,
                });
            }
            Err(error) => return Err(IdempotencyError::Io(error)),
        };

        let record: ReplayRecord = match serde_json::from_slice(&bytes) {
            Ok(record) => record,
            Err(error) => return Err(self.quarantine_record(key_hash, &path, error.to_string())),
        };

        if record.schema_version != REPLAY_RECORD_SCHEMA_VERSION {
            return Err(self.quarantine_record(
                key_hash,
                &path,
                format!("unsupported schema version {}", record.schema_version),
            ));
        }
        if record.key_hash != key_hash {
            return Err(self.quarantine_record(
                key_hash,
                &path,
                format!(
                    "record key hash {} does not match path key hash {}",
                    record.key_hash, key_hash
                ),
            ));
        }

        Ok(record)
    }

    fn write_record_atomic(&self, record: &ReplayRecord) -> Result<(), IdempotencyError> {
        let path = self.record_path_for_hash(&record.key_hash);
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("record path has no parent: {}", path.display()),
            )
        })?;
        fs::create_dir_all(parent)?;

        let bytes = serde_json::to_vec_pretty(record)?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(&bytes)?;
        tmp.flush()?;
        tmp.as_file().sync_all()?;
        tmp.persist(&path)
            .map_err(|error| IdempotencyError::Io(error.error))?;
        Ok(())
    }

    fn quarantine_record(&self, key_hash: &str, path: &Path, source: String) -> IdempotencyError {
        let quarantine_path = self.next_quarantine_path(key_hash);
        if let Err(error) = fs::create_dir_all(&self.quarantine_dir) {
            return IdempotencyError::CorruptRecord {
                path: path.to_path_buf(),
                reason: source,
                quarantine_error: error.to_string(),
            };
        }
        match fs::rename(path, &quarantine_path) {
            Ok(()) => IdempotencyError::CorruptRecordQuarantined {
                path: path.to_path_buf(),
                quarantine_path,
                reason: source,
            },
            Err(error) => IdempotencyError::CorruptRecord {
                path: path.to_path_buf(),
                reason: source,
                quarantine_error: error.to_string(),
            },
        }
    }

    fn next_quarantine_path(&self, key_hash: &str) -> PathBuf {
        let stamp = unix_millis();
        for attempt in 0..100 {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let path = self
                .quarantine_dir
                .join(format!("{key_hash}-{stamp}{suffix}.json"));
            if !path.exists() {
                return path;
            }
        }
        self.quarantine_dir
            .join(format!("{key_hash}-{stamp}-overflow.json"))
    }

    fn key_dir_for_hash(&self, key_hash: &str) -> PathBuf {
        self.records_dir.join(key_hash)
    }

    fn record_path_for_hash(&self, key_hash: &str) -> PathBuf {
        self.key_dir_for_hash(key_hash).join(RECORD_FILE_NAME)
    }
}

pub fn begin_index_folder_replay(
    control_state: &ControlStateDir,
    canonical_request_root: &Path,
    raw_key: &str,
    reset_requested: bool,
    allow_protected_root: bool,
    activate: bool,
) -> Result<ReplayStart, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = index_folder_request_hash(
        canonical_request_root,
        reset_requested,
        allow_protected_root,
        activate,
    )?;

    let store = FileReplayStore::open_control(control_state)?;

    match store.check_or_reserve(&key, &request_hash)? {
        ReplayDecision::FirstExecution(_) => Ok(ReplayStart::FirstExecution(ActiveReplay {
            store,
            key,
            request_hash,
        })),
        ReplayDecision::Replay(record) => Ok(ReplayStart::Replay(replay_response(&record))),
    }
}

pub fn begin_tool_replay(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
) -> Result<ReplayStart, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = RequestHash::for_tool_request(tool_name, request)?;
    let store = FileReplayStore::open(project_state)?;

    match store.check_or_reserve(&key, &request_hash)? {
        ReplayDecision::FirstExecution(_) => Ok(ReplayStart::FirstExecution(ActiveReplay {
            store,
            key,
            request_hash,
        })),
        ReplayDecision::Replay(record) => Ok(ReplayStart::Replay(replay_response(&record))),
    }
}

/// NON-RESERVING, read-only replay probe for a tool request.
///
/// Unlike [`begin_tool_replay`], this NEVER reserves a record and NEVER mutates
/// store state. It answers a single question: "does an identical
/// key+request already have a stored result?"
///
/// - `Ok(Some(response))` — an existing record for this key whose request hash
///   matches; returns the replay response text. The caller may short-circuit
///   and return it without writing any bytes.
/// - `Ok(None)` — no record exists for this key. The caller must fall through
///   to its normal execution path (which reserves via `begin_tool_replay`).
/// - `Err(Conflict)` — a record exists for this key but the incoming request
///   hash differs; the caller must surface the conflict, NOT replay.
///
/// This is the read-only sibling of `check_or_reserve`'s `FirstExecution`-vs
/// `Replay` decision: it observes the `Replay`/`Conflict` outcomes without ever
/// consuming a reservation, so a later `begin_tool_replay` on the miss path
/// still sees a clean slate.
pub fn probe_tool_replay(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
) -> Result<Option<String>, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = RequestHash::for_tool_request(tool_name, request)?;
    let store = FileReplayStore::open(project_state)?;

    match store.replay_if_present(&key, &request_hash)? {
        Some(record) => Ok(Some(replay_response(&record))),
        None => Ok(None),
    }
}

/// [`begin_tool_replay`] with the replay-authority fence (Feature 020
/// Slice 4): a stored result is replayed ONLY when its source-bound
/// post-image receipt verifies against the CURRENT bytes at the recorded
/// written paths. A completed/failed record whose receipt is missing or no
/// longer true is SUPERSEDED — the reservation is retaken and the caller
/// executes fresh, so the record ends up holding the current truth instead
/// of a claim the disk no longer supports. In-flight reservations keep their
/// existing replay-unavailable answer; superseding a live reservation would
/// race the owner.
pub fn begin_tool_replay_verified(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
) -> Result<ReplayStart, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = RequestHash::for_tool_request(tool_name, request)?;
    let store = FileReplayStore::open(project_state)?;

    match store.check_or_reserve(&key, &request_hash)? {
        ReplayDecision::FirstExecution(_) => Ok(ReplayStart::FirstExecution(ActiveReplay {
            store,
            key,
            request_hash,
        })),
        ReplayDecision::Replay(record) => {
            let verified = record.post_image.as_ref().is_some_and(verify_post_image);
            if verified || record.status == ReplayStatus::Reserved {
                return Ok(ReplayStart::Replay(replay_response(&record)));
            }
            // T038 round-1 repair: superseding must have ONE winner. Without
            // the claim, two concurrent identical-key retries could both
            // retake the reservation and both execute the mutation. Losers
            // answer as reserved — transient by construction (the winner's
            // record write and marker release are adjacent, and an orphaned
            // marker heals by age).
            if !store.try_claim_supersede(&key.key_hash())? {
                return Ok(ReplayStart::Replay(replay_response(
                    &ReplayRecord::reserved(key.key_hash(), request_hash.clone()),
                )));
            }
            let superseding = ReplayRecord::reserved(key.key_hash(), request_hash.clone());
            let written = store.write_record_atomic(&superseding);
            store.release_supersede(&key.key_hash());
            written?;
            Ok(ReplayStart::FirstExecution(ActiveReplay {
                store,
                key,
                request_hash,
            }))
        }
    }
}

/// [`probe_tool_replay`] with the replay-authority fence: non-reserving, so
/// an unverified record simply answers `None` and the caller falls through
/// to its normal (reserving) execution path, which supersedes it there.
pub fn probe_tool_replay_verified(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
) -> Result<Option<String>, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = RequestHash::for_tool_request(tool_name, request)?;
    let store = FileReplayStore::open(project_state)?;

    match store.replay_if_present(&key, &request_hash)? {
        Some(record) => {
            let verified = record.post_image.as_ref().is_some_and(verify_post_image);
            Ok(verified.then(|| replay_response(&record)))
        }
        None => Ok(None),
    }
}

pub fn index_folder_request_hash(
    canonical_root: &Path,
    reset_requested: bool,
    allow_protected_root: bool,
    activate: bool,
) -> Result<RequestHash, IdempotencyError> {
    RequestHash::for_tool_request(
        "index_folder",
        &json!({
            // The project key hashes the canonical native path bytes. Never
            // use a lossy UTF-8 rendering here: two distinct Unix roots must
            // not share one replay identity.
            "project_id": canonical_project_key(canonical_root),
            "reset": reset_requested,
            "allow_protected_root": allow_protected_root,
            // The `add` spelling changes observable side effects (per-session
            // activation), so it MUST distinguish the canonical request:
            // replaying an `add:true` record for a default call would skip
            // activation, and vice versa.
            "activate": activate,
        }),
    )
}

fn canonical_project_key(canonical_root: &Path) -> String {
    crate::discovery::project_id_for_canonical_root(canonical_root).0
}

pub fn replay_response(record: &ReplayRecord) -> String {
    match (record.status, record.response_text.as_ref()) {
        (ReplayStatus::Completed | ReplayStatus::Failed, Some(response_text)) => {
            response_text.clone()
        }
        (ReplayStatus::Reserved, _) => format!(
            "Idempotency replay unavailable: request for key hash {} is still reserved.",
            record.key_hash
        ),
        (status, None) => format!(
            "Idempotency replay unavailable: record for key hash {} has status {:?} but no stored response.",
            record.key_hash, status
        ),
    }
}

pub fn format_tool_error(error: &IdempotencyError) -> String {
    match error {
        IdempotencyError::Conflict { .. } => format!("Idempotency conflict: {error}"),
        _ => format!("Idempotency error: {error}"),
    }
}

pub fn format_live_postcondition_unavailable(
    historical_receipt: &str,
    error: impl std::fmt::Display,
) -> String {
    format!(
        "applied=false outcome=live_postcondition_unavailable\n\
         historical_receipt_begin\n{historical_receipt}\n\
         historical_receipt_end\n\
         live_postcondition_error={error}"
    )
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&canonicalize_value(value))
}

fn canonicalize_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_value).collect()),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            let mut canonical = Map::new();
            for key in keys {
                if let Some(value) = map.get(key) {
                    canonical.insert(key.clone(), canonicalize_value(value));
                }
            }
            Value::Object(canonical)
        }
        other => other.clone(),
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T038 round-1 (replay supersede atomicity): the supersede claim has
    /// exactly one winner, releases cleanly, and heals a crash-orphaned
    /// marker by age. A deterministic RED for the underlying race is not
    /// constructible; this pins the primitive the race fix rests on.
    #[test]
    fn supersede_claim_is_one_winner_releases_and_heals_orphans() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = FileReplayStore::open_in(dir.path().join("idempotency")).expect("store");

        assert!(store.try_claim_supersede("k1").expect("first claim"));
        assert!(
            !store.try_claim_supersede("k1").expect("second claim"),
            "a live claim must have exactly one winner"
        );
        store.release_supersede("k1");
        assert!(
            store.try_claim_supersede("k1").expect("post-release claim"),
            "a released claim is reclaimable"
        );

        // Crash-orphan healing: backdate the marker past the staleness bound;
        // the next claim reclaims it instead of wedging the key forever.
        let marker = store.supersede_marker_path("k1");
        std::fs::File::options()
            .write(true)
            .open(&marker)
            .expect("open marker")
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::now() - (SUPERSEDE_MARKER_STALE * 2)),
            )
            .expect("backdate marker");
        assert!(
            store.try_claim_supersede("k1").expect("stale reclaim"),
            "an orphaned marker past the staleness bound must be reclaimable"
        );
    }

    #[test]
    fn index_folder_identity_uses_native_project_identity() {
        let literal = Path::new("/work/a\\b");
        let nested = Path::new("/work/a/b");

        if cfg!(windows) {
            assert_eq!(
                canonical_project_key(literal),
                canonical_project_key(nested),
                "Windows separator compatibility must remain intact"
            );
        } else {
            assert_ne!(
                canonical_project_key(literal),
                canonical_project_key(nested),
                "distinct Unix roots must produce distinct idempotency request identities"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn index_folder_identity_preserves_non_utf8_native_bytes() {
        use std::os::unix::ffi::OsStringExt;

        let native = PathBuf::from(std::ffi::OsString::from_vec(vec![b'a', 0xff, b'b']));
        let lossy_collision = PathBuf::from("a\u{fffd}b");
        assert_eq!(native.to_string_lossy(), lossy_collision.to_string_lossy());
        assert_ne!(
            canonical_project_key(&native),
            canonical_project_key(&lossy_collision)
        );
    }
}
