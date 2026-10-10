//! Project-scoped durable replay admission for trusted embedded hosts.
//!
//! The old `FileReplayStore` is deliberately not exposed through this API. A
//! host must call `mark_started` before performing an external effect. After a
//! crash, `Started` and `Uncertain` never become executable by age or retry.

use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde_json::Value;

const KEY_FRAME: &[u8] = b"symforge-idempotency-key-v1\0";
const MAX_KEY_BYTES: usize = 512;
const MAX_SCOPE_BYTES: usize = 256;
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS replay_meta (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        project_hash TEXT NOT NULL,
        scope_hash TEXT NOT NULL,
        schema_version INTEGER NOT NULL,
        legacy_cutover_digest TEXT
    );
    CREATE TABLE IF NOT EXISTS replay_entries (
        key_hash TEXT PRIMARY KEY,
        request_hash TEXT NOT NULL,
        state TEXT NOT NULL CHECK (state IN ('not_started', 'started', 'uncertain', 'completed', 'failed')),
        generation INTEGER NOT NULL CHECK (generation > 0),
        owner_token BLOB,
        outcome_kind TEXT,
        outcome_digest TEXT,
        post_image_digest TEXT,
        evidence_digest TEXT,
        CHECK ((state IN ('not_started', 'started')) = (owner_token IS NOT NULL))
    );
    CREATE TABLE IF NOT EXISTS replay_binding (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        root_anchor_key BLOB NOT NULL,
        state_anchor_key BLOB NOT NULL
    );
";

/// A raw host key. Its debug rendering cannot disclose the supplied bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct ReplayKey(String);

impl ReplayKey {
    pub fn new(raw: impl Into<String>) -> Result<Self, ReplayError> {
        let raw = raw.into();
        if raw.is_empty() || raw.len() > MAX_KEY_BYTES {
            return Err(ReplayError::InvalidKey);
        }
        Ok(Self(raw))
    }

    fn hash(&self) -> String {
        let mut frame = Vec::with_capacity(KEY_FRAME.len() + self.0.len());
        frame.extend_from_slice(KEY_FRAME);
        frame.extend_from_slice(self.0.as_bytes());
        crate::hash::digest_hex(&frame)
    }
}

impl std::fmt::Debug for ReplayKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReplayKey(<redacted>)")
    }
}

/// The old canonical JSON framing is retained so migration cannot rebind a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestFingerprint(String);

impl RequestFingerprint {
    pub fn for_json(operation: &str, arguments: &Value) -> Result<Self, ReplayError> {
        if operation.is_empty()
            || operation.len() > 128
            || serde_json::to_vec(arguments)
                .map_err(|_| ReplayError::InvalidRequest)?
                .len()
                > MAX_REQUEST_BYTES
        {
            return Err(ReplayError::InvalidRequest);
        }
        let old = crate::idempotency::RequestHash::for_tool_request(operation, arguments)
            .map_err(|_| ReplayError::InvalidRequest)?;
        Ok(Self(old.as_str().to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayState {
    NotStarted,
    Started,
    Uncertain,
    Completed,
    Failed,
}

impl ReplayState {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Started => "started",
            Self::Uncertain => "uncertain",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "not_started" => Ok(Self::NotStarted),
            "started" => Ok(Self::Started),
            "uncertain" => Ok(Self::Uncertain),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

/// Only a category and digest of the host response are durable. Raw responses,
/// error messages, and payloads are never written to the replay database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    Applied,
    NoChange,
    Rejected,
}

impl OutcomeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::NoChange => "no_change",
            Self::Rejected => "rejected",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "applied" => Ok(Self::Applied),
            "no_change" => Ok(Self::NoChange),
            "rejected" => Ok(Self::Rejected),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayOutcome {
    pub kind: OutcomeKind,
    response_digest: String,
    post_image_digest: String,
}

impl ReplayOutcome {
    pub fn from_response_and_post_image(
        kind: OutcomeKind,
        response: &[u8],
        post_image: &[u8],
    ) -> Result<Self, ReplayError> {
        if response.len() > MAX_RESPONSE_BYTES
            || !matches!(
                crate::knowledge::scan_secret_bytes("replay-response", response),
                crate::knowledge::SecretScan::Clean
            )
            || !matches!(
                crate::knowledge::scan_secret_bytes("replay-post-image", post_image),
                crate::knowledge::SecretScan::Clean
            )
        {
            return Err(ReplayError::UnsafeOutcome);
        }
        Ok(Self {
            kind,
            response_digest: crate::hash::digest_hex(response),
            post_image_digest: crate::hash::digest_hex(post_image),
        })
    }

    pub fn matches_post_image(&self, bytes: &[u8]) -> bool {
        self.post_image_digest == crate::hash::digest_hex(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayRecord {
    pub key_hash: String,
    pub request: RequestFingerprint,
    pub state: ReplayState,
    pub generation: i64,
    pub outcome: Option<ReplayOutcome>,
    pub evidence_digest: Option<String>,
}

impl ReplayRecord {
    pub fn matches_post_image(&self, bytes: &[u8]) -> bool {
        self.state == ReplayState::Completed
            && self
                .outcome
                .as_ref()
                .is_some_and(|outcome| outcome.matches_post_image(bytes))
    }
}

/// This unforgeable lease is returned only to the winner of the SQL insert.
/// It is intentionally not Clone or Debug, and its token is never exposed.
pub struct OwnerLease {
    key_hash: String,
    request_hash: String,
    token: Vec<u8>,
    generation: i64,
}

pub enum ReserveOutcome {
    Acquired(OwnerLease),
    Existing(ReplayRecord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconciliationResolution {
    Completed,
    Failed,
}

/// Trusted-host postcondition evidence. The caller must verify the actual
/// operation's effect before constructing this value; only its digest is stored.
#[derive(Clone, Debug)]
pub struct ReconciliationEvidence {
    observed_generation: i64,
    observation_digest: String,
}

/// Attests that the trusted host has stopped old replay writers before
/// enabling new-key admissions in a state directory with old JSON records.
pub struct LegacyCutoverEvidence(String);

impl LegacyCutoverEvidence {
    pub fn from_checkpoint(checkpoint: &[u8]) -> Result<Self, ReplayError> {
        if checkpoint.is_empty()
            || checkpoint.len() > MAX_EVIDENCE_BYTES
            || !matches!(
                crate::knowledge::scan_secret_bytes("replay-cutover", checkpoint),
                crate::knowledge::SecretScan::Clean
            )
        {
            return Err(ReplayError::UnsafeOutcome);
        }
        Ok(Self(crate::hash::digest_hex(checkpoint)))
    }
}

impl ReconciliationEvidence {
    pub fn from_observation(
        observed_generation: i64,
        observation: &[u8],
    ) -> Result<Self, ReplayError> {
        if observed_generation < 1
            || observation.is_empty()
            || observation.len() > MAX_EVIDENCE_BYTES
            || !matches!(
                crate::knowledge::scan_secret_bytes("replay-evidence", observation),
                crate::knowledge::SecretScan::Clean
            )
        {
            return Err(ReplayError::UnsafeOutcome);
        }
        Ok(Self {
            observed_generation,
            observation_digest: crate::hash::digest_hex(observation),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("invalid replay key")]
    InvalidKey,
    #[error("invalid replay request")]
    InvalidRequest,
    #[error("replay outcome or evidence is unsafe to persist")]
    UnsafeOutcome,
    #[error("replay request conflicts with the stored key binding")]
    Conflict,
    #[error("legacy replay state requires explicit reconciliation")]
    LegacyStateRequiresReconciliation,
    #[error("legacy replay store requires an exclusive migration cutover")]
    LegacyCutoverRequired,
    #[error("replay owner or state changed")]
    StaleOwner,
    #[error("replay postcondition evidence does not match the asserted outcome")]
    InvalidEvidence,
    #[error("replay state cannot make the requested transition")]
    InvalidTransition,
    #[error("project state is bound to a different root")]
    WrongProject,
    #[error("project state is bound to a different physical root")]
    WrongPhysicalRoot,
    #[error("project state is bound to a different physical state directory")]
    WrongStateDirectory,
    #[error("project state is bound to a different host scope")]
    WrongScope,
    #[error("invalid replay host scope")]
    InvalidScope,
    #[error("replay project root must be an existing directory and state path must be absolute")]
    InvalidStatePath,
    #[error("replay state path is a symlink")]
    SymlinkStatePath,
    #[error("replay state I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("replay database: {0}")]
    Database(#[from] rusqlite::Error),
}

/// One durable replay database within a caller-supplied project state directory.
/// The state directory may be user-local, so its canonical project binding is
/// checked in SQLite on every open.
pub struct ReplayStore {
    db_path: PathBuf,
    legacy_records: PathBuf,
    project_root: PathBuf,
    state_dir: PathBuf,
    root_anchor_key: [u8; 16],
    state_anchor_key: [u8; 16],
}

struct AnchoredConnection {
    conn: Connection,
    _root: crate::live_index::index_lifecycle::physical_root::PhysicalRootLease,
    _state: crate::live_index::index_lifecycle::physical_root::PhysicalRootLease,
}

impl Deref for AnchoredConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl DerefMut for AnchoredConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

impl ReplayStore {
    pub fn open(
        project_root: &Path,
        project_state_dir: &Path,
        scope: &str,
    ) -> Result<Self, ReplayError> {
        Self::open_with_anchor(project_root, project_state_dir, scope, None)
    }

    pub(crate) fn open_bound(
        project_root: &Path,
        project_state_dir: &Path,
        scope: &str,
        expected_anchor: [u8; 16],
    ) -> Result<Self, ReplayError> {
        Self::open_with_anchor(project_root, project_state_dir, scope, Some(expected_anchor))
    }

    fn open_with_anchor(
        project_root: &Path,
        project_state_dir: &Path,
        scope: &str,
        expected_anchor: Option<[u8; 16]>,
    ) -> Result<Self, ReplayError> {
        if scope.is_empty() || scope.len() > MAX_SCOPE_BYTES {
            return Err(ReplayError::InvalidScope);
        }
        if !project_root.is_absolute() || !project_state_dir.is_absolute() || !project_root.is_dir()
        {
            return Err(ReplayError::InvalidStatePath);
        }
        let root = fs::canonicalize(project_root)?;
        let root_lease = crate::live_index::index_lifecycle::physical_root::PhysicalRootLease::take(&root);
        let root_anchor_key = root_lease
            .opened_stable_key()
            .ok_or(ReplayError::WrongPhysicalRoot)?;
        if expected_anchor.is_some_and(|expected| expected != root_anchor_key) {
            return Err(ReplayError::WrongPhysicalRoot);
        }
        fs::create_dir_all(project_state_dir)?;
        let state = fs::canonicalize(project_state_dir)?;
        let state_lease = crate::live_index::index_lifecycle::physical_root::PhysicalRootLease::take(&state);
        let state_anchor_key = state_lease
            .opened_stable_key()
            .ok_or(ReplayError::WrongStateDirectory)?;
        let replay_dir = state.join("idempotency");
        if fs::symlink_metadata(&replay_dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(ReplayError::SymlinkStatePath);
        }
        fs::create_dir_all(&replay_dir)?;
        set_private_dir(&replay_dir)?;
        let scope_hash = crate::hash::digest_hex(scope.as_bytes());
        let original_db = replay_dir.join("replay-v1.sqlite3");
        let original_meta = match fs::symlink_metadata(&original_db) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(ReplayError::Io(error)),
        };
        if original_meta.as_ref().is_some_and(|meta| meta.file_type().is_symlink()) {
            return Err(ReplayError::SymlinkStatePath);
        }
        let original_scope = if original_meta.is_some() {
            let conn = Connection::open_with_flags(&original_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            Some(conn.query_row(
                "SELECT scope_hash FROM replay_meta WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )?)
        } else {
            None
        };
        // The original single-scope database stays at its historical path.
        // New scopes receive independent files, while every scope continues to
        // inspect the shared legacy JSON directory before admitting a key.
        let db_path = if original_scope.as_deref() == Some(scope_hash.as_str()) {
            original_db
        } else {
            let scopes_dir = replay_dir.join("scopes");
            if fs::symlink_metadata(&scopes_dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(ReplayError::SymlinkStatePath);
            }
            fs::create_dir_all(&scopes_dir)?;
            set_private_dir(&scopes_dir)?;
            let scoped_dir = scopes_dir.join(&scope_hash);
            if fs::symlink_metadata(&scoped_dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(ReplayError::SymlinkStatePath);
            }
            fs::create_dir_all(&scoped_dir)?;
            set_private_dir(&scoped_dir)?;
            scoped_dir.join("replay-v1.sqlite3")
        };
        if fs::symlink_metadata(&db_path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(ReplayError::SymlinkStatePath);
        }
        let store = Self {
            db_path,
            legacy_records: replay_dir.join("records"),
            project_root: root.clone(),
            state_dir: state,
            root_anchor_key,
            state_anchor_key,
        };
        let mut conn = store.connect()?;
        conn.execute_batch(SCHEMA)?;
        let project_hash = crate::hash::digest_hex(root.as_os_str().as_encoded_bytes());
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO replay_meta(singleton, project_hash, scope_hash, schema_version) VALUES (1, ?1, ?2, 1)",
            params![project_hash, scope_hash],
        )?;
        let bound: (String, String, i64) = tx.query_row(
            "SELECT project_hash, scope_hash, schema_version FROM replay_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if bound.0 != project_hash || bound.2 != 1 {
            return Err(ReplayError::WrongProject);
        }
        if bound.1 != scope_hash {
            return Err(ReplayError::WrongScope);
        }
        match tx
            .query_row(
                "SELECT root_anchor_key, state_anchor_key FROM replay_binding WHERE singleton = 1",
                [],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?
        {
            Some((root_key, state_key)) => {
                if root_key != root_anchor_key {
                    return Err(ReplayError::WrongPhysicalRoot);
                }
                if state_key != state_anchor_key {
                    return Err(ReplayError::WrongStateDirectory);
                }
            }
            None => {
                let entries: i64 =
                    tx.query_row("SELECT COUNT(*) FROM replay_entries", [], |row| row.get(0))?;
                if entries != 0 {
                    return Err(ReplayError::LegacyStateRequiresReconciliation);
                }
                tx.execute(
                    "INSERT INTO replay_binding(singleton, root_anchor_key, state_anchor_key) VALUES (1, ?1, ?2)",
                    params![root_anchor_key.as_slice(), state_anchor_key.as_slice()],
                )?;
            }
        }
        tx.commit()?;
        store.ensure_current_anchors()?;
        set_private_file(&store.db_path)?;
        Ok(store)
    }

    /// The host must first stop old replay writers and verify that they cannot
    /// restart against this state directory. The checkpoint digest is kept for
    /// audit; each old per-key record still refuses automatic replay/admission.
    pub fn confirm_legacy_cutover(
        &self,
        evidence: &LegacyCutoverEvidence,
    ) -> Result<(), ReplayError> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE replay_meta SET legacy_cutover_digest = ?1 WHERE singleton = 1
                 AND (legacy_cutover_digest IS NULL OR legacy_cutover_digest = ?1)",
            params![evidence.0],
        )?;
        let stored: Option<String> = conn.query_row(
            "SELECT legacy_cutover_digest FROM replay_meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        if stored.as_deref() != Some(evidence.0.as_str()) {
            return Err(ReplayError::LegacyCutoverRequired);
        }
        Ok(())
    }

    fn ensure_current_anchors(&self) -> Result<(), ReplayError> {
        use crate::live_index::index_lifecycle::physical_root::PhysicalRootAnchor;
        if PhysicalRootAnchor::observe(&self.project_root).map(PhysicalRootAnchor::stable_key)
            != Some(self.root_anchor_key)
        {
            return Err(ReplayError::WrongPhysicalRoot);
        }
        if PhysicalRootAnchor::observe(&self.state_dir).map(PhysicalRootAnchor::stable_key)
            != Some(self.state_anchor_key)
        {
            return Err(ReplayError::WrongStateDirectory);
        }
        Ok(())
    }

    fn connect(&self) -> Result<AnchoredConnection, ReplayError> {
        use crate::live_index::index_lifecycle::physical_root::PhysicalRootLease;
        let root = PhysicalRootLease::take(&self.project_root);
        if root.opened_stable_key() != Some(self.root_anchor_key) {
            return Err(ReplayError::WrongPhysicalRoot);
        }
        let state = PhysicalRootLease::take(&self.state_dir);
        if state.opened_stable_key() != Some(self.state_anchor_key) {
            return Err(ReplayError::WrongStateDirectory);
        }
        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        self.ensure_current_anchors()?;
        Ok(AnchoredConnection {
            conn,
            _root: root,
            _state: state,
        })
    }

    fn legacy_guard(&self, key_hash: &str) -> Result<bool, ReplayError> {
        let metadata = match fs::symlink_metadata(&self.legacy_records) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(ReplayError::Io(error)),
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ReplayError::LegacyStateRequiresReconciliation);
        }
        match fs::symlink_metadata(self.legacy_records.join(key_hash)) {
            Ok(_) => Err(ReplayError::LegacyStateRequiresReconciliation),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(ReplayError::Io(error)),
        }
    }

    /// Before old and new processes can coexist, the host must enforce an
    /// exclusive cutover. A legacy directory blocks all new reservations.
    /// After cutover, remove or archive that directory outside this API; old
    /// bytes are never modified here. Existing per-key old records still block
    /// if the directory is retained.
    pub fn reserve(
        &self,
        key: &ReplayKey,
        request: &RequestFingerprint,
    ) -> Result<ReserveOutcome, ReplayError> {
        let key_hash = key.hash();
        if self.legacy_guard(&key_hash)? {
            let cutover: Option<String> = self.connect()?.query_row(
                "SELECT legacy_cutover_digest FROM replay_meta WHERE singleton = 1",
                [],
                |row| row.get(0),
            )?;
            if cutover.is_none() {
                return Err(ReplayError::LegacyCutoverRequired);
            }
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO replay_entries(key_hash, request_hash, state, generation, owner_token)
             VALUES (?1, ?2, 'not_started', 1, randomblob(32))",
            params![key_hash, request.as_str()],
        )?;
        if inserted == 1 {
            let token = tx.query_row(
                "SELECT owner_token FROM replay_entries WHERE key_hash = ?1",
                params![key_hash],
                |row| row.get(0),
            )?;
            tx.commit()?;
            return Ok(ReserveOutcome::Acquired(OwnerLease {
                key_hash,
                request_hash: request.as_str().to_owned(),
                token,
                generation: 1,
            }));
        }
        let record = read_record(&tx, &key_hash)?.ok_or(ReplayError::StaleOwner)?;
        tx.commit()?;
        if record.request != *request {
            return Err(ReplayError::Conflict);
        }
        Ok(ReserveOutcome::Existing(record))
    }

    pub fn inspect(&self, key: &ReplayKey) -> Result<Option<ReplayRecord>, ReplayError> {
        let key_hash = key.hash();
        self.legacy_guard(&key_hash)?;
        read_record(&*self.connect()?, &key_hash)
    }

    pub fn mark_started(&self, lease: &OwnerLease) -> Result<ReplayRecord, ReplayError> {
        self.advance_owned(lease, ReplayState::NotStarted, ReplayState::Started, None)
    }

    /// Release is valid only before the host has begun the external effect.
    /// The owner token and generation are compared in the deletion itself.
    pub fn release_not_started(&self, lease: &OwnerLease) -> Result<(), ReplayError> {
        let changed = self.connect()?.execute(
            "DELETE FROM replay_entries WHERE key_hash = ?1 AND request_hash = ?2
             AND state = 'not_started' AND generation = ?3 AND owner_token = ?4",
            params![
                lease.key_hash,
                lease.request_hash,
                lease.generation,
                lease.token
            ],
        )?;
        if changed != 1 {
            return Err(ReplayError::StaleOwner);
        }
        Ok(())
    }

    pub fn complete(
        &self,
        lease: &OwnerLease,
        outcome: &ReplayOutcome,
    ) -> Result<ReplayRecord, ReplayError> {
        self.advance_owned(
            lease,
            ReplayState::Started,
            ReplayState::Completed,
            Some(outcome),
        )
    }

    /// Use only for a known terminal failure. If effect status is ambiguous,
    /// call `mark_uncertain` and require reconciliation instead.
    pub fn fail(
        &self,
        lease: &OwnerLease,
        outcome: &ReplayOutcome,
    ) -> Result<ReplayRecord, ReplayError> {
        if outcome.kind != OutcomeKind::Rejected {
            return Err(ReplayError::InvalidTransition);
        }
        self.advance_owned(
            lease,
            ReplayState::Started,
            ReplayState::Failed,
            Some(outcome),
        )
    }

    pub fn mark_uncertain(&self, lease: &OwnerLease) -> Result<ReplayRecord, ReplayError> {
        self.advance_owned(lease, ReplayState::Started, ReplayState::Uncertain, None)
    }

    fn advance_owned(
        &self,
        lease: &OwnerLease,
        from: ReplayState,
        to: ReplayState,
        outcome: Option<&ReplayOutcome>,
    ) -> Result<ReplayRecord, ReplayError> {
        self.legacy_guard(&lease.key_hash)?;
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let next_generation = lease.generation
            + if from == ReplayState::NotStarted {
                1
            } else {
                2
            };
        let changed = tx.execute(
            "UPDATE replay_entries SET state = ?1, generation = ?2,
                 owner_token = CASE WHEN ?1 = 'started' THEN owner_token ELSE NULL END,
                 outcome_kind = ?3, outcome_digest = ?4, post_image_digest = ?5
             WHERE key_hash = ?6 AND request_hash = ?7 AND state = ?8
                 AND generation = ?9 AND owner_token = ?10",
            params![
                to.as_str(),
                next_generation,
                outcome.map(|value| value.kind.as_str()),
                outcome.map(|value| value.response_digest.as_str()),
                outcome.map(|value| value.post_image_digest.as_str()),
                lease.key_hash,
                lease.request_hash,
                from.as_str(),
                lease.generation
                    + if from == ReplayState::NotStarted {
                        0
                    } else {
                        1
                    },
                lease.token,
            ],
        )?;
        if changed != 1 {
            return Err(ReplayError::StaleOwner);
        }
        let record = read_record(&tx, &lease.key_hash)?.ok_or(ReplayError::StaleOwner)?;
        tx.commit()?;
        Ok(record)
    }

    /// A trusted host may classify an interrupted operation only after it has
    /// verified its postcondition. CAS on the observed generation fences a
    /// concurrent owner completion. This never creates a fresh execution.
    pub fn reconcile(
        &self,
        key: &ReplayKey,
        request: &RequestFingerprint,
        evidence: &ReconciliationEvidence,
        resolution: ReconciliationResolution,
        outcome: &ReplayOutcome,
    ) -> Result<ReplayRecord, ReplayError> {
        if evidence.observation_digest != outcome.post_image_digest {
            return Err(ReplayError::InvalidEvidence);
        }
        let key_hash = key.hash();
        self.legacy_guard(&key_hash)?;
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_record(&tx, &key_hash)?.ok_or(ReplayError::InvalidTransition)?;
        if current.request != *request {
            return Err(ReplayError::Conflict);
        }
        if !matches!(current.state, ReplayState::Started | ReplayState::Uncertain) {
            return Err(ReplayError::InvalidTransition);
        }
        let target = match resolution {
            ReconciliationResolution::Completed => ReplayState::Completed,
            ReconciliationResolution::Failed if outcome.kind == OutcomeKind::Rejected => {
                ReplayState::Failed
            }
            ReconciliationResolution::Failed => return Err(ReplayError::InvalidTransition),
        };
        let changed = tx.execute(
            "UPDATE replay_entries SET state = ?1, generation = generation + 1,
                owner_token = NULL, outcome_kind = ?2, outcome_digest = ?3,
                post_image_digest = ?4, evidence_digest = ?5
             WHERE key_hash = ?6 AND request_hash = ?7 AND generation = ?8
                AND state IN ('started', 'uncertain')",
            params![
                target.as_str(),
                outcome.kind.as_str(),
                outcome.response_digest,
                outcome.post_image_digest,
                evidence.observation_digest,
                key_hash,
                request.as_str(),
                evidence.observed_generation
            ],
        )?;
        if changed != 1 {
            return Err(ReplayError::StaleOwner);
        }
        let record = read_record(&tx, &key_hash)?.ok_or(ReplayError::StaleOwner)?;
        tx.commit()?;
        Ok(record)
    }
}

fn read_record(conn: &Connection, key_hash: &str) -> Result<Option<ReplayRecord>, ReplayError> {
    conn.query_row(
        "SELECT key_hash, request_hash, state, generation, outcome_kind, outcome_digest, post_image_digest, evidence_digest
         FROM replay_entries WHERE key_hash = ?1",
        params![key_hash],
        |row| {
            let state: String = row.get(2)?;
            let kind: Option<String> = row.get(4)?;
            let digest: Option<String> = row.get(5)?;
            let post_image: Option<String> = row.get(6)?;
            Ok(ReplayRecord {
                key_hash: row.get(0)?,
                request: RequestFingerprint(row.get(1)?),
                state: ReplayState::parse(&state)?,
                generation: row.get(3)?,
                outcome: match (kind, digest, post_image) {
                    (Some(kind), Some(digest), Some(post_image_digest)) => Some(ReplayOutcome {
                        kind: OutcomeKind::parse(&kind)?,
                        response_digest: digest,
                        post_image_digest,
                    }),
                    (None, None, None) => None,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                },
                evidence_digest: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(ReplayError::from)
}

#[cfg(unix)]
fn set_private_dir(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}
