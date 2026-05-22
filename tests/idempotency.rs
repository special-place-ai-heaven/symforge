use std::fs;

use serde_json::json;
use symforge::idempotency::{
    FileReplayStore, IdempotencyError, IdempotencyKey, ReplayDecision, ReplayStatus, RequestHash,
};
use symforge::paths::{
    SYMFORGE_IDEMPOTENCY_QUARANTINE_DIR_PATH, SYMFORGE_IDEMPOTENCY_RECORDS_DIR_PATH,
};
use tempfile::TempDir;

fn expect_first_execution(decision: ReplayDecision) -> symforge::idempotency::ReplayRecord {
    match decision {
        ReplayDecision::FirstExecution(record) => record,
        ReplayDecision::Replay(record) => {
            panic!("expected first execution, got replay: {record:?}")
        }
    }
}

#[test]
fn idempotency_canonical_request_hash_ignores_json_object_key_order() {
    let first = json!({
        "path": "src/lib.rs",
        "options": {
            "force": false,
            "sections": ["outline", "imports"]
        }
    });
    let same_semantics = json!({
        "options": {
            "sections": ["outline", "imports"],
            "force": false
        },
        "path": "src/lib.rs"
    });

    let first_hash = RequestHash::for_tool_request("index_folder", &first).unwrap();
    let same_hash = RequestHash::for_tool_request("index_folder", &same_semantics).unwrap();
    let different_tool_hash = RequestHash::for_tool_request("batch_edit", &same_semantics).unwrap();

    assert_eq!(first_hash, same_hash);
    assert_ne!(first_hash, different_tool_hash);
}

#[test]
fn idempotency_first_execution_reserves_record_without_storing_raw_key_or_request_body() {
    let tmp = TempDir::new().unwrap();
    let store = FileReplayStore::open(tmp.path()).unwrap();
    let key = IdempotencyKey::new("SECRET_IDEMPOTENCY_KEY").unwrap();
    let request = json!({
        "path": "SECRET_REQUEST_PATH",
        "dry_run": false
    });
    let hash = RequestHash::for_tool_request("index_folder", &request).unwrap();

    let record = expect_first_execution(store.check_or_reserve(&key, &hash).unwrap());

    assert_eq!(record.request_hash, hash);
    assert_eq!(record.status, ReplayStatus::Reserved);
    let record_path = store.record_path(&key);
    assert!(record_path.starts_with(tmp.path().join(SYMFORGE_IDEMPOTENCY_RECORDS_DIR_PATH)));

    let raw_record = fs::read_to_string(record_path).unwrap();
    assert!(raw_record.contains(hash.as_str()));
    assert!(!raw_record.contains("SECRET_IDEMPOTENCY_KEY"));
    assert!(!raw_record.contains("SECRET_REQUEST_PATH"));
}

#[test]
fn idempotency_same_key_same_hash_replays_stored_status() {
    let tmp = TempDir::new().unwrap();
    let store = FileReplayStore::open(tmp.path()).unwrap();
    let key = IdempotencyKey::new("replay-key").unwrap();
    let hash = RequestHash::for_tool_request("index_folder", &json!({"path": "src"})).unwrap();

    expect_first_execution(store.check_or_reserve(&key, &hash).unwrap());
    let completed = store
        .update_status(&key, &hash, ReplayStatus::Completed)
        .unwrap();
    assert_eq!(completed.status, ReplayStatus::Completed);

    let replay = store.check_or_reserve(&key, &hash).unwrap();
    match replay {
        ReplayDecision::Replay(record) => {
            assert_eq!(record.request_hash, hash);
            assert_eq!(record.status, ReplayStatus::Completed);
        }
        ReplayDecision::FirstExecution(record) => {
            panic!("same key/hash should replay existing record: {record:?}")
        }
    }
}

#[test]
fn idempotency_same_key_different_hash_returns_deterministic_conflict() {
    let tmp = TempDir::new().unwrap();
    let store = FileReplayStore::open(tmp.path()).unwrap();
    let key = IdempotencyKey::new("conflict-key").unwrap();
    let first_hash =
        RequestHash::for_tool_request("index_folder", &json!({"path": "src"})).unwrap();
    let conflicting_hash =
        RequestHash::for_tool_request("index_folder", &json!({"path": "tests"})).unwrap();

    expect_first_execution(store.check_or_reserve(&key, &first_hash).unwrap());

    let error = store.check_or_reserve(&key, &conflicting_hash).unwrap_err();
    match error {
        IdempotencyError::Conflict {
            existing, incoming, ..
        } => {
            assert_eq!(existing, first_hash);
            assert_eq!(incoming, conflicting_hash);
        }
        other => panic!("expected deterministic conflict, got {other:?}"),
    }
}

#[test]
fn idempotency_corrupt_replay_record_is_quarantined_and_not_served_as_success() {
    let tmp = TempDir::new().unwrap();
    let store = FileReplayStore::open(tmp.path()).unwrap();
    let key = IdempotencyKey::new("corrupt-key").unwrap();
    let hash = RequestHash::for_tool_request("index_folder", &json!({"path": "src"})).unwrap();

    expect_first_execution(store.check_or_reserve(&key, &hash).unwrap());
    let record_path = store.record_path(&key);
    fs::write(&record_path, b"{ this is not valid json").unwrap();

    let error = store.check_or_reserve(&key, &hash).unwrap_err();
    match error {
        IdempotencyError::CorruptRecordQuarantined {
            path,
            quarantine_path,
            ..
        } => {
            assert_eq!(path, record_path);
            assert!(!path.exists(), "corrupt live record should be removed");
            assert!(
                quarantine_path.exists(),
                "corrupt bytes should be preserved"
            );
            assert!(
                quarantine_path
                    .starts_with(tmp.path().join(SYMFORGE_IDEMPOTENCY_QUARANTINE_DIR_PATH))
            );
        }
        other => panic!("expected corrupt record quarantine, got {other:?}"),
    }
}
