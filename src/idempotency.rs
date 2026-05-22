use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

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

#[derive(Debug, Clone)]
pub struct FileReplayStore {
    records_dir: PathBuf,
    quarantine_dir: PathBuf,
}

impl FileReplayStore {
    pub fn open(project_root: &Path) -> Result<Self, IdempotencyError> {
        let idempotency_dir = paths::ensure_idempotency_dir(project_root)?;
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
    store_root: &Path,
    conflict_probe_root: Option<&Path>,
    canonical_request_root: &Path,
    raw_key: &str,
    reset_requested: bool,
) -> Result<ReplayStart, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = index_folder_request_hash(canonical_request_root, reset_requested)?;

    if let Some(probe_root) = conflict_probe_root
        && !same_normalized_path(probe_root, store_root)
    {
        let probe_store = FileReplayStore::open(probe_root)?;
        if let Some(record) = probe_store.replay_if_present(&key, &request_hash)? {
            return Ok(ReplayStart::Replay(replay_response(&record)));
        }
    }

    let store = FileReplayStore::open(store_root)?;

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
    store_root: &Path,
    tool_name: &str,
    raw_key: &str,
    request: &Value,
) -> Result<ReplayStart, IdempotencyError> {
    let key = IdempotencyKey::new(raw_key)?;
    let request_hash = RequestHash::for_tool_request(tool_name, request)?;
    let store = FileReplayStore::open(store_root)?;

    match store.check_or_reserve(&key, &request_hash)? {
        ReplayDecision::FirstExecution(_) => Ok(ReplayStart::FirstExecution(ActiveReplay {
            store,
            key,
            request_hash,
        })),
        ReplayDecision::Replay(record) => Ok(ReplayStart::Replay(replay_response(&record))),
    }
}

fn same_normalized_path(left: &Path, right: &Path) -> bool {
    normalized_path_string(left) == normalized_path_string(right)
}

pub fn index_folder_request_hash(
    canonical_root: &Path,
    reset_requested: bool,
) -> Result<RequestHash, IdempotencyError> {
    RequestHash::for_tool_request(
        "index_folder",
        &json!({
            "path": normalized_path_string(canonical_root),
            "reset": reset_requested,
        }),
    )
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

fn normalized_path_string(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
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
