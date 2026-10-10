#![cfg(feature = "embed")]

use std::fs;
use std::path::Path;
#[cfg(feature = "__test-internals")]
use std::process::{Command, Stdio};
#[cfg(feature = "__test-internals")]
use std::time::{Duration, Instant};

use serde_json::json;
use sha2::{Digest, Sha256};
use symforge::embed::parity::replay::{
    LegacyCutoverEvidence, OutcomeKind, ReconciliationEvidence, ReconciliationResolution,
    ReplayError, ReplayKey, ReplayOutcome, ReplayState, ReplayStore, RequestFingerprint,
    ReserveOutcome,
};

fn store(root: &Path, state: &Path) -> ReplayStore {
    ReplayStore::open(root, state, "room-a").unwrap()
}

fn key() -> ReplayKey {
    ReplayKey::new("test-replay-key").unwrap()
}

fn request() -> RequestFingerprint {
    RequestFingerprint::for_json("write", &json!({"a": 1, "b": 2})).unwrap()
}

fn acquired(result: ReserveOutcome) -> symforge::embed::parity::replay::OwnerLease {
    match result {
        ReserveOutcome::Acquired(lease) => lease,
        ReserveOutcome::Existing(_) => panic!("expected acquisition"),
    }
}

#[test]
fn one_owner_has_fenced_transitions_and_verified_replay() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let store = store(dir.path(), &state);
    let key = key();
    let request = request();
    let lease = acquired(store.reserve(&key, &request).unwrap());
    let record = store.mark_started(&lease).unwrap();
    assert_eq!(record.state, ReplayState::Started);
    assert!(matches!(
        store.release_not_started(&lease),
        Err(ReplayError::StaleOwner)
    ));

    let outcome = ReplayOutcome::from_response_and_post_image(
        OutcomeKind::Applied,
        b"SENSITIVE_TEST_RESPONSE_XYZ",
        b"new source bytes",
    )
    .unwrap();
    let completed = store.complete(&lease, &outcome).unwrap();
    assert_eq!(completed.state, ReplayState::Completed);
    assert!(completed.matches_post_image(b"new source bytes"));
    assert!(!completed.matches_post_image(b"later writer bytes"));
    assert!(matches!(
        store.complete(&lease, &outcome),
        Err(ReplayError::StaleOwner)
    ));

    let replay = store.reserve(&key, &request).unwrap();
    assert!(
        matches!(replay, ReserveOutcome::Existing(record) if record.state == ReplayState::Completed)
    );
    let changed = RequestFingerprint::for_json("write", &json!({"a": 9})).unwrap();
    assert!(matches!(
        store.reserve(&key, &changed),
        Err(ReplayError::Conflict)
    ));

    for entry in fs::read_dir(state.join("idempotency")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            assert!(
                !bytes
                    .windows(b"SENSITIVE_TEST_RESPONSE_XYZ".len())
                    .any(|window| window == b"SENSITIVE_TEST_RESPONSE_XYZ")
            );
        }
    }
    assert!(!format!("{:?}", key).contains("test-replay-key"));
}

#[test]
fn release_is_only_available_to_unstarted_owner() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let first = acquired(store.reserve(&key, &request).unwrap());
    assert_eq!(
        store.inspect(&key).unwrap().unwrap().state,
        ReplayState::NotStarted
    );
    store.release_not_started(&first).unwrap();
    let second = acquired(store.reserve(&key, &request).unwrap());
    assert!(matches!(
        store.release_not_started(&first),
        Err(ReplayError::StaleOwner)
    ));
    store.mark_started(&second).unwrap();
}

#[test]
fn uncertain_requires_postcondition_evidence_and_generation_cas() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let lease = acquired(store.reserve(&key, &request).unwrap());
    store.mark_started(&lease).unwrap();
    let uncertain = store.mark_uncertain(&lease).unwrap();
    assert_eq!(uncertain.state, ReplayState::Uncertain);
    assert!(
        matches!(store.reserve(&key, &request).unwrap(), ReserveOutcome::Existing(record) if record.state == ReplayState::Uncertain)
    );
    let outcome =
        ReplayOutcome::from_response_and_post_image(OutcomeKind::Applied, b"ok", b"post image")
            .unwrap();
    let stale =
        ReconciliationEvidence::from_observation(uncertain.generation - 1, b"post image").unwrap();
    assert!(matches!(
        store.reconcile(
            &key,
            &request,
            &stale,
            ReconciliationResolution::Completed,
            &outcome
        ),
        Err(ReplayError::StaleOwner)
    ));
    let mismatch =
        ReconciliationEvidence::from_observation(uncertain.generation, b"other post image")
            .unwrap();
    assert!(matches!(
        store.reconcile(
            &key,
            &request,
            &mismatch,
            ReconciliationResolution::Completed,
            &outcome
        ),
        Err(ReplayError::InvalidEvidence)
    ));
    let evidence =
        ReconciliationEvidence::from_observation(uncertain.generation, b"post image").unwrap();
    let resolved = store
        .reconcile(
            &key,
            &request,
            &evidence,
            ReconciliationResolution::Completed,
            &outcome,
        )
        .unwrap();
    assert_eq!(resolved.state, ReplayState::Completed);
    assert!(resolved.evidence_digest.is_some());
    assert!(resolved.matches_post_image(b"post image"));
    assert!(matches!(
        store.complete(&lease, &outcome),
        Err(ReplayError::StaleOwner)
    ));
}

#[test]
fn legacy_state_preserves_original_bytes_and_requires_cutover() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let store = store(dir.path(), &state);
    let legacy = state.join("idempotency/records");
    fs::create_dir_all(&legacy).unwrap();
    let key = key();
    assert!(matches!(
        store.reserve(&key, &request()),
        Err(ReplayError::LegacyCutoverRequired)
    ));
    store
        .confirm_legacy_cutover(
            &LegacyCutoverEvidence::from_checkpoint(b"old writers stopped").unwrap(),
        )
        .unwrap();

    // The exact old key-hash frame keeps old reservations in the same key lane.
    let mut frame = b"symforge-idempotency-key-v1\0".to_vec();
    frame.extend_from_slice(b"test-replay-key");
    let key_hash: String = Sha256::digest(&frame)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let old_key_dir = legacy.join(key_hash);
    fs::create_dir_all(&old_key_dir).unwrap();
    let old_path = old_key_dir.join("record.json");
    let old_bytes = br#"{"status":"reserved","response_text":"original bytes"}"#;
    fs::write(&old_path, old_bytes).unwrap();
    assert!(matches!(
        store.reserve(&key, &request()),
        Err(ReplayError::LegacyStateRequiresReconciliation)
    ));
    assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
}

#[test]
fn state_directory_fences_project_and_isolates_room_scopes() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let state = first.path().join("state");
    let room_a = store(first.path(), &state);
    assert!(matches!(
        ReplayStore::open(second.path(), &state, "room-a"),
        Err(ReplayError::WrongProject)
    ));
    let room_b = ReplayStore::open(first.path(), &state, "room-b").unwrap();
    let key = key();
    let request = request();
    let first_lease = acquired(room_a.reserve(&key, &request).unwrap());
    let _second_lease = acquired(room_b.reserve(&key, &request).unwrap());
    room_a.mark_started(&first_lease).unwrap();
    assert_eq!(room_a.inspect(&key).unwrap().unwrap().state, ReplayState::Started);
    assert_eq!(room_b.inspect(&key).unwrap().unwrap().state, ReplayState::NotStarted);
}

#[test]
fn request_and_outcome_payloads_are_bounded() {
    assert!(matches!(
        RequestFingerprint::for_json("write", &json!({"payload": "x".repeat(70_000)})),
        Err(ReplayError::InvalidRequest)
    ));
    assert!(matches!(
        ReplayOutcome::from_response_and_post_image(
            OutcomeKind::Applied,
            &vec![b'x'; 70_000],
            b"post image"
        ),
        Err(ReplayError::UnsafeOutcome)
    ));
    assert!(matches!(
        ReplayKey::new("x".repeat(513)),
        Err(ReplayError::InvalidKey)
    ));
}

// A real test-process entry point: parent tests invoke this executable rather
// than using threads or simulated stores.
#[cfg(feature = "__test-internals")]
#[test]
fn child_replay_process() {
    let Ok(mode) = std::env::var("SYMFORGE_REPLAY_TEST_CHILD_MODE") else {
        return;
    };
    let root = std::env::var_os("SYMFORGE_REPLAY_TEST_ROOT").unwrap();
    let state = std::env::var_os("SYMFORGE_REPLAY_TEST_STATE").unwrap();
    let result = std::env::var_os("SYMFORGE_REPLAY_TEST_RESULT").unwrap();
    let store = match ReplayStore::open(Path::new(&root), Path::new(&state), "room-a") {
        Ok(store) => store,
        Err(ReplayError::WrongPhysicalRoot) if mode == "inspect_binding" => {
            fs::write(result, b"wrong_physical_root").unwrap();
            return;
        }
        Err(error) => panic!("child replay open refused: {error}"),
    };
    if mode == "inspect_binding" {
        let state = store.inspect(&key()).unwrap().unwrap().state;
        let label: &[u8] = if state == ReplayState::Completed {
            b"completed"
        } else {
            b"not_completed"
        };
        fs::write(result, label).unwrap();
        return;
    }
    if mode == "race" {
        fs::write(Path::new(&result).with_extension("ready"), b"ready").unwrap();
        let start = Instant::now();
        while !Path::new(&root).join("go").exists() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "race gate timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let key = key();
    let request = request();
    match store.reserve(&key, &request).unwrap() {
        ReserveOutcome::Acquired(lease) => {
            fs::write(result, b"acquired").unwrap();
            if mode == "crash_started" {
                store.mark_started(&lease).unwrap();
                std::process::exit(0);
            }
            if mode == "complete_then_exit" {
                store.mark_started(&lease).unwrap();
                let outcome = ReplayOutcome::from_response_and_post_image(
                    OutcomeKind::Applied,
                    b"ok",
                    b"committed bytes",
                )
                .unwrap();
                store.complete(&lease, &outcome).unwrap();
                std::process::exit(0);
            }
            if mode == "crash_unstarted" {
                std::process::exit(0);
            }
        }
        ReserveOutcome::Existing(_) => fs::write(result, b"existing").unwrap(),
    }
}

#[cfg(feature = "__test-internals")]
fn child(mode: &str, root: &Path, state: &Path, result: &Path) -> Command {
    let mut command = symforge::process_util::hidden_command(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg("child_replay_process")
        .env("SYMFORGE_REPLAY_TEST_CHILD_MODE", mode)
        .env("SYMFORGE_REPLAY_TEST_ROOT", root)
        .env("SYMFORGE_REPLAY_TEST_STATE", state)
        .env("SYMFORGE_REPLAY_TEST_RESULT", result)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(feature = "__test-internals")]
#[test]
fn separate_process_reservation_has_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let mut children = Vec::new();
    for index in 0..8 {
        let result = dir.path().join(format!("result-{index}"));
        children.push((
            child("race", dir.path(), &state, &result).spawn().unwrap(),
            result,
        ));
    }
    let start = Instant::now();
    while !children
        .iter()
        .all(|(_, result)| result.with_extension("ready").exists())
    {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "children did not reach race gate"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::write(dir.path().join("go"), b"go").unwrap();
    let mut winners = 0;
    for (mut process, result) in children {
        assert!(process.wait().unwrap().success());
        if fs::read(&result).unwrap() == b"acquired" {
            winners += 1;
        }
    }
    assert_eq!(winners, 1);
    assert_eq!(
        store(dir.path(), &state)
            .inspect(&key())
            .unwrap()
            .unwrap()
            .state,
        ReplayState::NotStarted
    );
}

#[cfg(feature = "__test-internals")]
#[test]
fn crashed_process_never_releases_started_or_unstarted_without_owner() {
    for (mode, expected) in [
        ("crash_unstarted", ReplayState::NotStarted),
        ("crash_started", ReplayState::Started),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let result = dir.path().join("result");
        assert!(
            child(mode, dir.path(), &state, &result)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(fs::read(&result).unwrap(), b"acquired");
        let store = store(dir.path(), &state);
        assert_eq!(store.inspect(&key()).unwrap().unwrap().state, expected);
        assert!(
            matches!(store.reserve(&key(), &request()).unwrap(), ReserveOutcome::Existing(record) if record.state == expected)
        );
    }
}

#[cfg(feature = "__test-internals")]
#[test]
fn completed_process_exit_replays_only_with_matching_post_image() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let result = dir.path().join("result");
    assert!(
        child("complete_then_exit", dir.path(), &state, &result)
            .status()
            .unwrap()
            .success()
    );
    let store = store(dir.path(), &state);
    let record = match store.reserve(&key(), &request()).unwrap() {
        ReserveOutcome::Existing(record) => record,
        ReserveOutcome::Acquired(_) => panic!("completed process was admitted twice"),
    };
    assert_eq!(record.state, ReplayState::Completed);
    assert!(record.matches_post_image(b"committed bytes"));
    assert!(!record.matches_post_image(b"different bytes"));
}

#[cfg(feature = "__test-internals")]
#[test]
fn copied_replay_state_cannot_rebind_to_replaced_physical_root_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let state = root.join(".symforge");
    let store = store(&root, &state);
    let lease = acquired(store.reserve(&key(), &request()).unwrap());
    store.mark_started(&lease).unwrap();
    let outcome = ReplayOutcome::from_response_and_post_image(
        OutcomeKind::Applied,
        b"completed",
        b"same postimage",
    )
    .unwrap();
    store.complete(&lease, &outcome).unwrap();
    drop(store);

    let first = dir.path().join("same-root-child");
    assert!(child("inspect_binding", &root, &state, &first)
        .status()
        .unwrap()
        .success());
    assert_eq!(fs::read(&first).unwrap(), b"completed");

    let old_root = dir.path().join("original-physical-root");
    fs::rename(&root, &old_root).unwrap();
    fs::create_dir(&root).unwrap();
    let copied_state = root.join(".symforge");
    let mut directories = vec![(
        old_root.join(".symforge/idempotency"),
        copied_state.join("idempotency"),
    )];
    let mut copied_db = false;
    while let Some((source, destination)) = directories.pop() {
        fs::create_dir_all(&destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                directories.push((entry.path(), target));
            } else {
                assert!(entry.file_type().unwrap().is_file());
                copied_db |= entry.file_name().to_string_lossy() == "replay-v1.sqlite3";
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    assert!(copied_db, "the physical-root probe must copy the live replay DB");
    let second = dir.path().join("replacement-root-child");
    assert!(child("inspect_binding", &root, &copied_state, &second)
        .status()
        .unwrap()
        .success());
    assert_eq!(fs::read(&second).unwrap(), b"wrong_physical_root");
}
