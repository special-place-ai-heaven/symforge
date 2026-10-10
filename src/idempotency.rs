use std::fmt;
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
const MAX_REPLAY_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct IdempotencyKey(String);

impl fmt::Debug for IdempotencyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IdempotencyKey(<redacted>)")
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    Started,
    Uncertain,
    Completed,
    Failed,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl fmt::Debug for ReplayRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReplayRecord")
            .field("schema_version", &self.schema_version)
            .field("key_hash", &self.key_hash)
            .field("request_hash", &self.request_hash)
            .field("status", &self.status)
            .field("created_unix_millis", &self.created_unix_millis)
            .field("updated_unix_millis", &self.updated_unix_millis)
            .field(
                "response_text",
                &self.response_text.as_ref().map(|_| "<redacted>"),
            )
            .field("post_image", &self.post_image)
            .finish()
    }
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
    /// Old receipts lack this field and require explicit reconciliation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PostImageSourceBinding>,
    /// Linked worktrees an operation routed targets into. Each is verified
    /// through its own admitted authority, never through `source`'s root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_sources: Vec<LinkedPostImageSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PostImageSourceBinding {
    pub project_id: String,
    pub physical_root_key: [u8; 16],
}

/// A linked worktree root that holds some of a receipt's targets, bound to the
/// physical root its write was admitted under.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkedPostImageSource {
    pub root: String,
    pub binding: PostImageSourceBinding,
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
        source: None,
        linked_sources: Vec::new(),
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
    Some(PostImageReceipt {
        targets,
        source: None,
        linked_sources: Vec::new(),
    })
}

const MAX_BOUND_REPLAY_TARGETS: usize = 4096;

fn source_binding(
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
) -> Option<PostImageSourceBinding> {
    Some(PostImageSourceBinding {
        project_id: canonical_project_key(source.admitted_root()),
        physical_root_key: source.physical_root_stable_key()?,
    })
}

/// Attach the source admitted for this effect only after checking the exact
/// post-image through its original physical-root anchor.
pub(crate) fn bind_post_image_to_source(
    receipt: PostImageReceipt,
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
) -> Option<PostImageReceipt> {
    bind_post_image_to_sources(receipt, source, &[])
}

/// [`bind_post_image_to_source`] for an operation that also routed targets
/// into linked worktrees: every linked root is bound to its own admitted
/// authority, and the receipt is kept only when each target verifies through
/// the authority whose root holds it.
pub(crate) fn bind_post_image_to_sources(
    mut receipt: PostImageReceipt,
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
    linked: &[std::sync::Arc<crate::index_lifecycle::activation::ProjectSourceAuthority>],
) -> Option<PostImageReceipt> {
    receipt.source = Some(source_binding(source)?);
    receipt.linked_sources = linked
        .iter()
        .map(|linked| {
            Some(LinkedPostImageSource {
                root: linked.admitted_root().display().to_string(),
                binding: source_binding(linked)?,
            })
        })
        .collect::<Option<_>>()?;
    verify_post_image_bound(&receipt, source).then_some(receipt)
}

/// The one per-authority post-image read shared by MCP replay receipts and
/// the embedded routed batch's replay manifest: stream the digest of a target
/// beneath the admitted source whose root holds it, never following a link
/// out of that root, including while its publication refreshes.
pub(crate) fn post_image_digest_beneath(
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
    relative: &Path,
) -> Option<Option<String>> {
    source.digest_regular_beneath_anchor(relative).ok()
}

/// Verify recorded bytes only through retained admitted source anchors: the
/// indexed root's, and for a target routed into a linked worktree, that
/// worktree's own authority, whose physical root must still be the one the
/// write was bound to.
pub(crate) fn verify_post_image_bound(
    receipt: &PostImageReceipt,
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
) -> bool {
    let Some(binding) = &receipt.source else {
        return false;
    };
    if receipt.targets.is_empty()
        || receipt.targets.len() > MAX_BOUND_REPLAY_TARGETS
        || source_binding(source).as_ref() != Some(binding)
    {
        return false;
    }
    let mut linked = Vec::with_capacity(receipt.linked_sources.len());
    for recorded in &receipt.linked_sources {
        let authority =
            crate::index_lifecycle::activation::project_source_authority(Path::new(&recorded.root));
        if source_binding(&authority).as_ref() != Some(&recorded.binding) {
            return false;
        }
        linked.push(authority);
    }
    let sources: Vec<&crate::index_lifecycle::activation::ProjectSourceAuthority> =
        std::iter::once(source)
            .chain(linked.iter().map(|authority| authority.as_ref()))
            .collect();
    receipt.targets.iter().all(|target| {
        // Windows canonicalization can add a verbatim-path prefix to an
        // authority root while a tool's absolute target retains its plain
        // spelling. Simplify only that syntax; the anchored read below still
        // enforces the original physical directory and rejects links. The
        // deepest holding root wins, so a worktree nested beneath the indexed
        // root is read through its own authority.
        let target_path = dunce::simplified(Path::new(&target.path));
        let Some((owner, relative)) = sources
            .iter()
            .filter_map(|owner| {
                let root = dunce::simplified(owner.admitted_root());
                target_path
                    .strip_prefix(root)
                    .ok()
                    .map(|relative| (owner, relative, root.components().count()))
            })
            .max_by_key(|(_, _, depth)| *depth)
            .map(|(owner, relative, _)| (owner, relative))
        else {
            return false;
        };
        post_image_digest_beneath(owner, relative)
            .is_some_and(|observed| observed == target.content_digest)
    })
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
    /// Persist that the external effect may have begun before invoking it.
    pub fn mark_started(&self) -> Result<ReplayRecord, IdempotencyError> {
        self.store
            .update_status(&self.key, &self.request_hash, ReplayStatus::Started)
    }

    /// Persist that the effect cannot be safely classified or retried.
    pub fn mark_uncertain(&self) -> Result<ReplayRecord, IdempotencyError> {
        self.store
            .update_status(&self.key, &self.request_hash, ReplayStatus::Uncertain)
    }

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

    /// Publish the result and its source-bound receipt in one atomic record.
    /// An absent receipt leaves the started operation requiring reconciliation.
    pub fn complete_with_post_image(
        &self,
        response_text: impl Into<String>,
        post_image: Option<PostImageReceipt>,
    ) -> Result<ReplayRecord, IdempotencyError> {
        let Some(post_image) = post_image.filter(|receipt| !receipt.targets.is_empty()) else {
            return Err(IdempotencyError::ReceiptRequired);
        };
        self.store.update_status_with_receipt(
            &self.key,
            &self.request_hash,
            response_text.into(),
            post_image,
        )
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
    #[error("idempotency replay response exceeds the 1 MiB persistence limit")]
    ResponseTooLarge,
    #[error("idempotency replay response is sensitive or could not be safely scanned")]
    UnsafeResponse,
    #[error("verified replay completion requires a non-empty post-image receipt")]
    ReceiptRequired,
    #[error("idempotency record state transition requires reconciliation")]
    InvalidTransition,
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
        "idempotency reservation for key hash {key_hash} is publishing; retry without executing"
    )]
    Publishing { key_hash: String },
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
        self.check_or_reserve_with_hooks(key, request_hash, || {}, || {})
    }

    fn check_or_reserve_with_hooks(
        &self,
        key: &IdempotencyKey,
        request_hash: &RequestHash,
        before_publish: impl FnOnce(),
        after_claim: impl FnOnce(),
    ) -> Result<ReplayDecision, IdempotencyError> {
        let key_hash = key.key_hash();
        let key_dir = self.key_dir_for_hash(&key_hash);
        if key_dir.exists() {
            let record = self.load_existing(&key_hash)?;
            self.ensure_same_hash(&record, request_hash)?;
            return Ok(ReplayDecision::Replay(record));
        }
        fs::create_dir_all(&self.records_dir)?;
        let staged = tempfile::Builder::new()
            .prefix(".pending-reservation-")
            .tempdir_in(&self.records_dir)?;
        let record = ReplayRecord::reserved(key_hash.clone(), request_hash.clone());
        let bytes = serde_json::to_vec_pretty(&record)?;
        let mut staged_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staged.path().join(RECORD_FILE_NAME))?;
        staged_file.write_all(&bytes)?;
        staged_file.sync_all()?;
        drop(staged_file);

        // The callback is a deterministic test seam for the publish boundary.
        before_publish();
        let marker = self.records_dir.join(format!("{key_hash}.claim"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(mut claim) => {
                claim.write_all(request_hash.0.as_bytes())?;
                claim.sync_all()?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return self.wait_for_publication(&key_hash, request_hash);
            }
            Err(error) => return Err(IdempotencyError::Io(error)),
        }
        after_claim();
        let result = if key_dir.exists() {
            self.replay_or_incomplete(&key_hash, request_hash)
        } else {
            match fs::rename(staged.path(), &key_dir) {
                Ok(()) => Ok(ReplayDecision::FirstExecution(record)),
                Err(_) if key_dir.exists() => self.replay_or_incomplete(&key_hash, request_hash),
                Err(error) => Err(IdempotencyError::Io(error)),
            }
        };
        // A crash before this remove leaves an explicit fail-closed claim.
        let _ = fs::remove_file(marker);
        result
    }

    fn replay_or_incomplete(
        &self,
        key_hash: &str,
        request_hash: &RequestHash,
    ) -> Result<ReplayDecision, IdempotencyError> {
        let record = self.load_existing(key_hash)?;
        self.ensure_same_hash(&record, request_hash)?;
        Ok(ReplayDecision::Replay(record))
    }

    fn wait_for_publication(
        &self,
        key_hash: &str,
        request_hash: &RequestHash,
    ) -> Result<ReplayDecision, IdempotencyError> {
        let marker = self.records_dir.join(format!("{key_hash}.claim"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if self.key_dir_for_hash(key_hash).exists() {
                return self.replay_or_incomplete(key_hash, request_hash);
            }
            if !marker.exists() || std::time::Instant::now() >= deadline {
                return Err(IdempotencyError::Publishing {
                    key_hash: key_hash.to_owned(),
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
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
            let marker = self.records_dir.join(format!("{key_hash}.claim"));
            if marker.exists() {
                return Err(IdempotencyError::Publishing { key_hash });
            }
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
        if let Some(response) = &response_text {
            validate_replay_response(response)?;
        }
        let key_hash = key.key_hash();
        let record = self.load_existing(&key_hash)?;
        self.ensure_same_hash(&record, request_hash)?;
        let valid = matches!(
            (record.status, status),
            (
                ReplayStatus::Reserved,
                ReplayStatus::Started | ReplayStatus::Completed | ReplayStatus::Failed
            ) | (
                ReplayStatus::Started,
                ReplayStatus::Completed | ReplayStatus::Uncertain
            )
        );
        if !valid {
            return Err(IdempotencyError::InvalidTransition);
        }
        let updated = record.with_status_and_response(status, response_text);
        self.write_record_atomic(&updated)?;
        Ok(updated)
    }

    fn update_status_with_receipt(
        &self,
        key: &IdempotencyKey,
        request_hash: &RequestHash,
        response_text: String,
        post_image: PostImageReceipt,
    ) -> Result<ReplayRecord, IdempotencyError> {
        validate_replay_response(&response_text)?;
        let key_hash = key.key_hash();
        let record = self.load_existing(&key_hash)?;
        self.ensure_same_hash(&record, request_hash)?;
        if !matches!(
            record.status,
            ReplayStatus::Reserved | ReplayStatus::Started
        ) {
            return Err(IdempotencyError::InvalidTransition);
        }
        let mut updated =
            record.with_status_and_response(ReplayStatus::Completed, Some(response_text));
        updated.post_image = Some(post_image);
        self.write_record_atomic(&updated)?;
        Ok(updated)
    }

    pub fn record_path(&self, key: &IdempotencyKey) -> PathBuf {
        self.record_path_for_hash(&key.key_hash())
    }

    /// One-winner supersede claim (T038 round-1; heal narrowed in round-2):
    /// `create_new` is the same atomic first-claim primitive
    /// `check_or_reserve` uses, so of N concurrent contenders superseding
    /// the same unverified record, one retakes its reservation and the
    /// losers answer as reserved instead of double-executing the mutation.
    ///
    /// A marker orphaned by a crash between claim and release (two adjacent
    /// fs writes) heals by age — but healing NEVER claims in the same call:
    /// round-2 review showed delete-then-claim lets a second healer delete
    /// the first healer's FRESH marker by name and mint two winners. The
    /// healer removes the orphan, answers "not claimed" (one extra reserved
    /// response after a crash), and the NEXT retry claims the clean slot
    /// through the ordinary `create_new` path.
    ///
    /// Recorded residual, stated exactly: a contender whose staleness
    /// judgment predates another healer's removal can still delete a fresh
    /// marker created in between. That interleave needs a crash-orphan plus
    /// three parties racing inside the winner's two-fs-write claim window
    /// (microseconds); its degradation equals the pre-claim behavior, and
    /// no name-based marker scheme can close it without an
    /// identity-compare-and-delete primitive the filesystem does not offer.
    pub fn try_claim_supersede(&self, key_hash: &str) -> Result<bool, IdempotencyError> {
        let marker = self.supersede_marker_path(key_hash);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&marker)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|age| age > SUPERSEDE_MARKER_STALE);
                if stale {
                    let _ = fs::remove_file(&marker);
                }
                Ok(false)
            }
            Err(error) => Err(IdempotencyError::Io(error)),
        }
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
        #[cfg(test)]
        if std::env::var_os("SYMFORGE_REPLAY_COMPLETE_CRASH").is_some()
            && record.status == ReplayStatus::Completed
            && record.post_image.is_none()
        {
            std::process::exit(73);
        }
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

/// Legacy entrypoint without an admitted source. It may reserve a new key,
/// but every existing completed record requires reconciliation. Callers with
/// a retained source authority use `begin_tool_replay_verified_bound`.
pub fn begin_tool_replay_verified(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
) -> Result<ReplayStart, IdempotencyError> {
    begin_tool_replay_verified_with(project_state, tool_name, raw_key, request, |_| false)
}

pub(crate) fn begin_tool_replay_verified_bound(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
) -> Result<ReplayStart, IdempotencyError> {
    begin_tool_replay_verified_with(project_state, tool_name, raw_key, request, |receipt| {
        verify_post_image_bound(receipt, source)
    })
}

fn begin_tool_replay_verified_with(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
    verify: impl Fn(&PostImageReceipt) -> bool,
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
            let verified = record.post_image.as_ref().is_some_and(verify);
            if record.status == ReplayStatus::Completed && verified {
                Ok(ReplayStart::Replay(replay_response(&record)))
            } else {
                Ok(ReplayStart::Replay(
                    "Error: Idempotency replay unavailable: stored operation requires reconciliation."
                        .to_owned(),
                ))
            }
        }
    }
}

/// Non-reserving legacy probe without an admitted source. Existing records
/// fail closed; use `probe_tool_replay_verified_bound` for attested replay.
pub fn probe_tool_replay_verified(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
) -> Result<Option<String>, IdempotencyError> {
    probe_tool_replay_verified_with(project_state, tool_name, raw_key, request, |_| false)
}

pub(crate) fn probe_tool_replay_verified_bound(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
) -> Result<Option<String>, IdempotencyError> {
    probe_tool_replay_verified_with(project_state, tool_name, raw_key, request, |receipt| {
        verify_post_image_bound(receipt, source)
    })
}

fn probe_tool_replay_verified_with(
    project_state: &ProjectStateDir,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
    verify: impl Fn(&PostImageReceipt) -> bool,
) -> Result<Option<String>, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = RequestHash::for_tool_request(tool_name, request)?;
    let store = FileReplayStore::open(project_state)?;

    match store.replay_if_present(&key, &request_hash)? {
        Some(record)
            if record.status == ReplayStatus::Completed
                && record.post_image.as_ref().is_some_and(verify) =>
        {
            Ok(Some(replay_response(&record)))
        }
        Some(_) => Ok(Some(
            "Error: Idempotency replay unavailable: stored operation requires reconciliation."
                .to_owned(),
        )),
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
            if validate_replay_response(response_text).is_ok() {
                response_text.clone()
            } else {
                "Error: Idempotency replay unavailable: persisted response requires reconciliation."
                    .to_owned()
            }
        }
        (ReplayStatus::Reserved, _) => format!(
            "Error: Idempotency replay unavailable: request for key hash {} is still reserved.",
            record.key_hash
        ),
        (ReplayStatus::Started | ReplayStatus::Uncertain, _) => format!(
            "Error: Idempotency replay unavailable: request for key hash {} requires reconciliation.",
            record.key_hash
        ),
        (status, None) => format!(
            "Error: Idempotency replay unavailable: record for key hash {} has status {:?} but no stored response.",
            record.key_hash, status
        ),
    }
}

fn validate_replay_response(response: &str) -> Result<(), IdempotencyError> {
    if response.len() > MAX_REPLAY_RESPONSE_BYTES {
        return Err(IdempotencyError::ResponseTooLarge);
    }
    if !matches!(
        crate::knowledge::scan_secret_bytes("replay-response", response.as_bytes()),
        crate::knowledge::SecretScan::Clean
    ) {
        return Err(IdempotencyError::UnsafeResponse);
    }
    Ok(())
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

    #[test]
    fn started_and_uncertain_are_durable_no_retry_states() {
        let dir = tempfile::tempdir().unwrap();
        let state = ProjectStateDir::new(dir.path().join("state"));
        std::fs::create_dir_all(state.as_path()).unwrap();
        let request = json!({ "path": "src/example.rs" });
        let ReplayStart::FirstExecution(active) =
            begin_tool_replay_verified(&state, "secret_remediate", "started-key", &request)
                .unwrap()
        else {
            panic!("new key must execute once");
        };
        active.mark_started().unwrap();
        assert!(matches!(
            begin_tool_replay_verified(&state, "secret_remediate", "started-key", &request)
                .unwrap(),
            ReplayStart::Replay(_)
        ));
        active.mark_uncertain().unwrap();
        assert!(matches!(
            begin_tool_replay_verified(&state, "secret_remediate", "started-key", &request)
                .unwrap(),
            ReplayStart::Replay(_)
        ));
        let store = FileReplayStore::open(&state).unwrap();
        let key = IdempotencyKey::new("started-key").unwrap();
        let hash = RequestHash::for_tool_request("secret_remediate", &request).unwrap();
        assert_eq!(
            store
                .replay_if_present(&key, &hash)
                .unwrap()
                .unwrap()
                .status,
            ReplayStatus::Uncertain
        );
    }

    #[test]
    fn completed_receipt_crash_child() {
        let Some(state_path) = std::env::var_os("SYMFORGE_REPLAY_CRASH_STATE") else {
            return;
        };
        let state = ProjectStateDir::new(PathBuf::from(state_path));
        let request = json!({});
        let ReplayStart::FirstExecution(active) =
            begin_tool_replay_verified(&state, "secret_remediate", "completed-crash", &request)
                .unwrap()
        else {
            panic!("new key must execute once");
        };
        let source = state.as_path().join("source.txt");
        std::fs::write(&source, b"committed").unwrap();
        active
            .complete_with_post_image(
                "applied",
                Some(post_image_from_written_bytes(&source, b"committed")),
            )
            .unwrap();
    }

    #[test]
    fn completed_receipt_publishes_atomically_across_process_crash_point() {
        use std::process::Stdio;
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        let mut child = crate::process_util::hidden_command(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "internals::idempotency::tests::completed_receipt_crash_child",
            ])
            .env("SYMFORGE_REPLAY_CRASH_STATE", &state)
            .env("SYMFORGE_REPLAY_COMPLETE_CRASH", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert_eq!(child.status().unwrap().code(), Some(0));
        let project_state = ProjectStateDir::new(state);
        let store = FileReplayStore::open(&project_state).unwrap();
        let key = IdempotencyKey::new("completed-crash").unwrap();
        let hash = RequestHash::for_tool_request("secret_remediate", &json!({})).unwrap();
        let record = store.replay_if_present(&key, &hash).unwrap().unwrap();
        assert_eq!(record.status, ReplayStatus::Completed);
        assert!(record.post_image.as_ref().is_some_and(verify_post_image));
    }

    #[test]
    fn completed_replay_refuses_replaced_physical_root_with_matching_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let displaced = dir.path().join("displaced");
        let state = ProjectStateDir::new(dir.path().join("state"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(state.as_path()).unwrap();
        let source = root.join("source.txt");
        std::fs::write(&source, b"committed").unwrap();
        let request = json!({ "path": "source.txt" });
        let ReplayStart::FirstExecution(active) = begin_tool_replay_verified(
            &state,
            "replace_symbol_body",
            "physical-root-key",
            &request,
        )
        .unwrap() else {
            panic!("first use must reserve");
        };
        active.mark_started().unwrap();
        active
            .complete_with_post_image(
                "applied",
                Some(post_image_from_written_bytes(&source, b"committed")),
            )
            .unwrap();
        std::fs::rename(&root, &displaced).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&source, b"committed").unwrap();
        let ReplayStart::Replay(response) = begin_tool_replay_verified(
            &state,
            "replace_symbol_body",
            "physical-root-key",
            &request,
        )
        .unwrap() else {
            panic!("same key must not execute again");
        };
        assert!(response.contains("requires reconciliation"));
    }

    #[test]
    fn bound_receipt_replays_on_original_root_and_refuses_replacement() {
        let dir = tempfile::tempdir().unwrap();
        // Production targets are joined from the canonical bound root; a TEMP
        // spelled with 8.3 short names would not prefix the authority's root.
        let base = dunce::canonicalize(dir.path()).unwrap();
        let root = base.join("project");
        let displaced = base.join("displaced");
        let state = ProjectStateDir::new(dir.path().join("state"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(state.as_path()).unwrap();
        let source = root.join("source.txt");
        std::fs::write(&source, b"committed").unwrap();
        let authority = crate::index_lifecycle::activation::project_source_authority(&root);
        let receipt = bind_post_image_to_source(
            post_image_from_written_bytes(&source, b"committed"),
            &authority,
        )
        .expect("original source postimage");
        let request = json!({ "path": "source.txt" });
        let ReplayStart::FirstExecution(active) = begin_tool_replay_verified_bound(
            &state,
            "replace_symbol_body",
            "bound-physical-root-key",
            &request,
            &authority,
        )
        .unwrap() else {
            panic!("first use must reserve");
        };
        active.mark_started().unwrap();
        active
            .complete_with_post_image("applied", Some(receipt))
            .unwrap();
        assert!(matches!(
            begin_tool_replay_verified_bound(
                &state,
                "replace_symbol_body",
                "bound-physical-root-key",
                &request,
                &authority,
            )
            .unwrap(),
            ReplayStart::Replay(response) if response == "applied"
        ));
        std::fs::rename(&root, &displaced).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&source, b"committed").unwrap();
        let replacement = crate::index_lifecycle::activation::project_source_authority(&root);
        for source_authority in [&authority, &replacement] {
            let ReplayStart::Replay(response) = begin_tool_replay_verified_bound(
                &state,
                "replace_symbol_body",
                "bound-physical-root-key",
                &request,
                source_authority,
            )
            .unwrap() else {
                panic!("same key must never execute again");
            };
            assert!(response.contains("requires reconciliation"));
        }
    }

    #[test]
    fn key_debug_never_contains_caller_bytes() {
        let key = IdempotencyKey::new("caller-private-marker").unwrap();
        let diagnostic = format!("{key:?}");
        assert!(diagnostic.contains("IdempotencyKey"));
        assert!(!diagnostic.contains("caller-private-marker"));
    }

    #[test]
    fn staged_reservation_is_invisible_until_fully_published() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileReplayStore::open_in(dir.path().join("idempotency")).unwrap();
        let key = IdempotencyKey::new("atomic-publication").unwrap();
        let request = RequestHash::for_tool_request("checkpoint_now", &json!({})).unwrap();

        let first = store
            .check_or_reserve_with_hooks(
                &key,
                &request,
                || {
                    assert!(store.replay_if_present(&key, &request).unwrap().is_none());
                    assert!(matches!(
                        store.check_or_reserve(&key, &request).unwrap(),
                        ReplayDecision::FirstExecution(_)
                    ));
                },
                || {},
            )
            .unwrap();
        assert!(matches!(first, ReplayDecision::Replay(_)));
        assert!(store.replay_if_present(&key, &request).unwrap().is_some());
    }

    #[test]
    fn reservation_process_child() {
        let Ok(action) = std::env::var("SYMFORGE_RESERVATION_PROCESS_ACTION") else {
            return;
        };
        let state = std::path::PathBuf::from(
            std::env::var_os("SYMFORGE_RESERVATION_PROCESS_STATE").unwrap(),
        );
        let store = FileReplayStore::open_in(state.clone()).unwrap();
        let request = RequestHash::for_tool_request("checkpoint_now", &json!({})).unwrap();
        let key = IdempotencyKey::new(if action == "crash" {
            "crash-key"
        } else {
            "race-key"
        })
        .unwrap();
        if action == "crash" {
            let _ =
                store.check_or_reserve_with_hooks(&key, &request, || {}, || std::process::exit(73));
            unreachable!("crash hook must exit after the claim");
        }
        let barrier = state.join("start-race");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !barrier.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "race barrier timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let result = match store.check_or_reserve(&key, &request).unwrap() {
            ReplayDecision::FirstExecution(_) => "first",
            ReplayDecision::Replay(_) => "replay",
        };
        let result_path = std::env::var_os("SYMFORGE_RESERVATION_PROCESS_RESULT").unwrap();
        fs::write(result_path, result).unwrap();
    }

    #[test]
    fn real_process_race_and_crashed_claim_fail_closed() {
        use std::process::Stdio;

        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("idempotency");
        let store = FileReplayStore::open_in(state.clone()).unwrap();
        let mut children = Vec::new();
        let mut results = Vec::new();
        for index in 0..8 {
            let result = dir.path().join(format!("result-{index}"));
            let mut command = crate::process_util::hidden_command(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "internals::idempotency::tests::reservation_process_child",
                ])
                .env("SYMFORGE_RESERVATION_PROCESS_ACTION", "race")
                .env("SYMFORGE_RESERVATION_PROCESS_STATE", &state)
                .env("SYMFORGE_RESERVATION_PROCESS_RESULT", &result)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            children.push(command.spawn().unwrap());
            results.push(result);
        }
        fs::write(state.join("start-race"), b"go").unwrap();
        for mut child in children {
            assert!(child.wait().unwrap().success(), "reservation child failed");
        }
        let outcomes: Vec<String> = results
            .iter()
            .map(|path| fs::read_to_string(path).unwrap())
            .collect();
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| result.as_str() == "first")
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| result.as_str() == "replay")
                .count(),
            7
        );

        let mut crash = crate::process_util::hidden_command(std::env::current_exe().unwrap());
        crash
            .args([
                "--exact",
                "internals::idempotency::tests::reservation_process_child",
            ])
            .env("SYMFORGE_RESERVATION_PROCESS_ACTION", "crash")
            .env("SYMFORGE_RESERVATION_PROCESS_STATE", &state)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert_eq!(crash.status().unwrap().code(), Some(73));
        let key = IdempotencyKey::new("crash-key").unwrap();
        let request = RequestHash::for_tool_request("checkpoint_now", &json!({})).unwrap();
        assert!(matches!(
            store.replay_if_present(&key, &request),
            Err(IdempotencyError::Publishing { .. })
        ));
        assert!(matches!(
            store.check_or_reserve(&key, &request),
            Err(IdempotencyError::Publishing { .. })
        ));

        let old_key = IdempotencyKey::new("old-orphan").unwrap();
        fs::create_dir(store.key_dir_for_hash(&old_key.key_hash())).unwrap();
        assert!(matches!(
            store.check_or_reserve(&old_key, &request),
            Err(IdempotencyError::IncompleteReservation { .. })
        ));
    }

    #[test]
    fn persisted_outcomes_are_bounded_and_sensitive_text_is_never_replayed() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileReplayStore::open_in(dir.path().join("idempotency")).unwrap();
        let key = IdempotencyKey::new("response-privacy").unwrap();
        let request = RequestHash::for_tool_request("checkpoint_now", &json!({})).unwrap();
        assert!(matches!(
            store.check_or_reserve(&key, &request).unwrap(),
            ReplayDecision::FirstExecution(_)
        ));

        let safe = "checkpoint complete".to_owned();
        let completed = store
            .update_status_with_response(
                &key,
                &request,
                ReplayStatus::Completed,
                Some(safe.clone()),
            )
            .unwrap();
        assert_eq!(replay_response(&completed), safe);
        assert_eq!(
            replay_response(&store.replay_if_present(&key, &request).unwrap().unwrap()),
            safe
        );

        let oversized = "x".repeat(MAX_REPLAY_RESPONSE_BYTES + 1);
        assert!(matches!(
            store.update_status_with_response(
                &key,
                &request,
                ReplayStatus::Completed,
                Some(oversized)
            ),
            Err(IdempotencyError::ResponseTooLarge)
        ));
        let synthetic = ["-----B", "EGIN P", "RIVATE", " KEY--", "---\n"].concat();
        assert!(!matches!(
            crate::knowledge::scan_secret_bytes("replay-response", synthetic.as_bytes()),
            crate::knowledge::SecretScan::Clean
        ));
        assert!(matches!(
            store.update_status_with_response(
                &key,
                &request,
                ReplayStatus::Completed,
                Some(synthetic.clone())
            ),
            Err(IdempotencyError::UnsafeResponse)
        ));
        assert_eq!(
            store
                .replay_if_present(&key, &request)
                .unwrap()
                .unwrap()
                .response_text
                .as_deref(),
            Some(safe.as_str())
        );

        let mut legacy = completed;
        legacy.response_text = Some(synthetic.to_owned());
        assert!(!replay_response(&legacy).contains(&synthetic));
        assert!(!format!("{legacy:?}").contains(&synthetic));
    }

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

        // Crash-orphan healing: backdate the marker past the staleness bound.
        // Healing must NOT claim in the same call (round-2: delete-then-claim
        // re-opens the two-winner race) — it removes the orphan and answers
        // "not claimed"; the NEXT claim owns the clean slot.
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
            !store.try_claim_supersede("k1").expect("stale heal"),
            "healing an orphaned marker must not claim in the same call"
        );
        assert!(
            !marker.exists(),
            "the orphaned marker must be removed by the heal"
        );
        assert!(
            store.try_claim_supersede("k1").expect("post-heal claim"),
            "the claim after the heal owns the clean slot"
        );
    }

    /// A legacy Completed record without a source receipt remains inspectable
    /// but cannot authorize another execution with the same key.
    #[test]
    fn legacy_completed_without_receipt_requires_reconciliation() {
        let dir = tempfile::tempdir().unwrap();
        let project_state = ProjectStateDir::new(dir.path().join("state"));
        std::fs::create_dir_all(project_state.as_path()).unwrap();
        let request = json!({ "path": "src/x.rs" });
        let ReplayStart::FirstExecution(active) =
            begin_tool_replay_verified(&project_state, "t", "race-key", &request).unwrap()
        else {
            panic!("fresh key must be a first execution");
        };
        active.complete("first result").unwrap();
        let ReplayStart::Replay(response) =
            begin_tool_replay_verified(&project_state, "t", "race-key", &request).unwrap()
        else {
            panic!("unverified legacy record must not re-execute");
        };
        assert!(response.contains("requires reconciliation"));
        let key = IdempotencyKey::new("race-key").unwrap();
        let hash = RequestHash::for_tool_request("t", &request).unwrap();
        let record = FileReplayStore::open(&project_state)
            .unwrap()
            .replay_if_present(&key, &hash)
            .unwrap()
            .unwrap();
        assert_eq!(record.status, ReplayStatus::Completed);
        assert!(record.post_image.is_none());
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
