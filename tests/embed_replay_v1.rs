#![cfg(feature = "embed")]
#![allow(deprecated)]

use std::fs;
use std::path::Path;
#[cfg(feature = "__test-internals")]
use std::process::{Command, Stdio};
#[cfg(feature = "__test-internals")]
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use sha2::{Digest, Sha256};
use symforge::embed::parity::replay::{
    LegacyCutoverEvidence, OutcomeKind, ReconciliationEvidence, ReconciliationResolution,
    ReplayError, ReplayKey, ReplayOutcome, ReplayRecord, ReplayState, ReplayStore,
    RequestFingerprint, ReserveOutcome,
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

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
    assert_eq!(
        room_a.inspect(&key).unwrap().unwrap().state,
        ReplayState::Started
    );
    assert_eq!(
        room_b.inspect(&key).unwrap().unwrap().state,
        ReplayState::NotStarted
    );
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
    let store = {
        let root_path = Path::new(&root);
        let state_path = Path::new(&state);
        let start = Instant::now();
        loop {
            match ReplayStore::open(root_path, state_path, "room-a") {
                Ok(store) => break store,
                Err(ReplayError::WrongPhysicalRoot) if mode == "inspect_binding" => {
                    fs::write(result, b"wrong_physical_root").unwrap();
                    return;
                }
                // Concurrent first-open / schema races surface as Busy after C10.
                Err(ReplayError::Busy) if start.elapsed() < Duration::from_secs(20) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("child replay open refused: {error}"),
            }
        }
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
    let reserved = match std::env::var("SYMFORGE_REPLAY_TEST_LEASE_MS") {
        Ok(raw) => {
            let millis: u64 = raw.parse().expect("lease milliseconds");
            store.reserve_with_lease(&key, &request, Duration::from_millis(millis))
        }
        Err(_) => store.reserve(&key, &request),
    };
    match reserved.unwrap() {
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
    // Create the replay DB before the race so eight first-opens do not contend
    // on schema/bootstrap and stall before writing the ready gate.
    drop(store(dir.path(), &state));
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
            start.elapsed() < Duration::from_secs(30),
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
    assert!(
        child("inspect_binding", &root, &state, &first)
            .status()
            .unwrap()
            .success()
    );
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
    assert!(
        copied_db,
        "the physical-root probe must copy the live replay DB"
    );
    let second = dir.path().join("replacement-root-child");
    assert!(
        child("inspect_binding", &root, &copied_state, &second)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&second).unwrap(), b"wrong_physical_root");
}

#[cfg(feature = "__test-internals")]
fn child_lease(mode: &str, root: &Path, state: &Path, result: &Path, lease_ms: u64) -> Command {
    let mut command = child(mode, root, state, result);
    command.env("SYMFORGE_REPLAY_TEST_LEASE_MS", lease_ms.to_string());
    command
}

#[cfg(feature = "__test-internals")]
#[test]
fn reservation_lease_expiry_allows_exactly_one_takeover() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let setup = store(dir.path(), &state);
    let held = acquired(
        setup
            .reserve_with_lease(&key(), &request(), Duration::from_millis(50))
            .unwrap(),
    );
    assert!(!held.took_over());
    drop(held);
    drop(setup);
    std::thread::sleep(Duration::from_millis(400));

    let mut children = Vec::new();
    for index in 0..8 {
        let result = dir.path().join(format!("result-{index}"));
        children.push((
            child_lease("race", dir.path(), &state, &result, 60_000)
                .spawn()
                .unwrap(),
            result,
        ));
    }
    let start = Instant::now();
    while !children
        .iter()
        .all(|(_, result)| result.with_extension("ready").exists())
    {
        assert!(
            start.elapsed() < Duration::from_secs(30),
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

#[test]
fn stale_owner_token_rejected_after_takeover() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let old = acquired(
        store
            .reserve_with_lease(&key, &request, Duration::from_millis(80))
            .unwrap(),
    );
    std::thread::sleep(Duration::from_millis(300));
    let new = acquired(
        store
            .reserve_with_lease(&key, &request, Duration::from_secs(30))
            .unwrap(),
    );
    assert!(new.took_over());
    let generation = store.inspect(&key).unwrap().unwrap().generation;
    let applied =
        ReplayOutcome::from_response_and_post_image(OutcomeKind::Applied, b"ok", b"post").unwrap();
    let rejected =
        ReplayOutcome::from_response_and_post_image(OutcomeKind::Rejected, b"no", b"post").unwrap();
    assert!(matches!(
        store.mark_started(&old),
        Err(ReplayError::StaleOwner)
    ));
    assert!(matches!(
        store.release_not_started(&old),
        Err(ReplayError::StaleOwner)
    ));
    assert!(matches!(
        store.complete(&old, &applied),
        Err(ReplayError::StaleOwner)
    ));
    assert!(matches!(
        store.fail(&old, &rejected),
        Err(ReplayError::StaleOwner)
    ));
    assert!(matches!(
        store.mark_uncertain(&old),
        Err(ReplayError::StaleOwner)
    ));
    let still = store.inspect(&key).unwrap().unwrap();
    assert_eq!(still.state, ReplayState::NotStarted);
    assert_eq!(still.generation, generation);
    store.mark_started(&new).unwrap();
}

#[cfg(feature = "__test-internals")]
#[test]
fn crash_between_reserve_and_mark_started_recovers_after_lease() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let result = dir.path().join("result");
    assert!(
        child_lease("crash_unstarted", dir.path(), &state, &result, 200)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&result).unwrap(), b"acquired");
    std::thread::sleep(Duration::from_millis(500));
    let store = store(dir.path(), &state);
    let lease = acquired(
        store
            .reserve_with_lease(&key(), &request(), Duration::from_secs(30))
            .unwrap(),
    );
    assert!(lease.took_over());
    store.mark_started(&lease).unwrap();
    let outcome = ReplayOutcome::from_response_and_post_image(
        OutcomeKind::Applied,
        b"ok",
        b"committed bytes",
    )
    .unwrap();
    let completed = store.complete(&lease, &outcome).unwrap();
    assert_eq!(completed.state, ReplayState::Completed);
    assert!(completed.matches_post_image(b"committed bytes"));
}

#[test]
fn live_reservation_is_not_taken_over_before_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let lease = acquired(
        store
            .reserve_with_lease(&key, &request, Duration::from_secs(60))
            .unwrap(),
    );
    assert!(!lease.took_over());
    match store
        .reserve_with_lease(&key, &request, Duration::from_secs(60))
        .unwrap()
    {
        ReserveOutcome::Existing(record) => assert_eq!(record.state, ReplayState::NotStarted),
        ReserveOutcome::Acquired(_) => panic!("live reservation was taken over"),
    }
    let remaining = store.lease_remaining(&key).unwrap().expect("live lease");
    assert!(remaining > Duration::ZERO);
}

#[cfg(feature = "__test-internals")]
#[test]
fn started_is_never_taken_over_even_when_expired() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let result = dir.path().join("result");
    assert!(
        child_lease("crash_started", dir.path(), &state, &result, 200)
            .status()
            .unwrap()
            .success()
    );
    std::thread::sleep(Duration::from_millis(500));
    match store(dir.path(), &state)
        .reserve_with_lease(&key(), &request(), Duration::from_secs(30))
        .unwrap()
    {
        ReserveOutcome::Existing(record) => {
            assert!(matches!(
                record.state,
                ReplayState::Started | ReplayState::Uncertain
            ));
        }
        ReserveOutcome::Acquired(_) => panic!("started reservation was taken over"),
    }
}

#[test]
fn release_not_started_with_foreign_token_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let old = acquired(
        store
            .reserve_with_lease(&key, &request, Duration::from_millis(80))
            .unwrap(),
    );
    std::thread::sleep(Duration::from_millis(300));
    let owner = acquired(
        store
            .reserve_with_lease(&key, &request, Duration::from_secs(30))
            .unwrap(),
    );
    assert!(owner.took_over());
    let before = store.inspect(&key).unwrap().unwrap();
    assert!(matches!(
        store.release_not_started(&old),
        Err(ReplayError::StaleOwner)
    ));
    let after = store.inspect(&key).unwrap().unwrap();
    assert_eq!(after.state, ReplayState::NotStarted);
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.request, before.request);
    store.release_not_started(&owner).unwrap();
    assert!(store.inspect(&key).unwrap().is_none());
}

#[test]
fn late_reservation_after_owner_release_is_cleanable_by_token() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let first = acquired(
        store
            .reserve_with_lease(&key, &request, Duration::from_secs(60))
            .unwrap(),
    );
    store.release_not_started(&first).unwrap();
    let second = acquired(
        store
            .reserve_with_lease(&key, &request, Duration::from_secs(60))
            .unwrap(),
    );
    assert!(!second.took_over());
    assert!(matches!(
        store.release_not_started(&first),
        Err(ReplayError::StaleOwner)
    ));
    assert_eq!(
        store.inspect(&key).unwrap().unwrap().state,
        ReplayState::NotStarted
    );
    store.mark_started(&second).unwrap();
    assert_eq!(
        store.inspect(&key).unwrap().unwrap().state,
        ReplayState::Started
    );
}

#[cfg(feature = "__test-internals")]
#[test]
fn lease_expiry_uses_one_clock_inside_the_cas_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let t0 = UNIX_EPOCH + Duration::from_millis(1_700_000_000_000);
    let lease_for = Duration::from_millis(1_000);
    let first = acquired(
        store
            .reserve_with_lease_at(&key, &request, lease_for, t0)
            .unwrap(),
    );
    assert!(!first.took_over());
    let live_at = t0 + Duration::from_millis(999);
    assert!(matches!(
        store
            .reserve_with_lease_at(&key, &request, lease_for, live_at)
            .unwrap(),
        ReserveOutcome::Existing(_)
    ));
    assert_eq!(
        store.lease_remaining_at(&key, live_at).unwrap().unwrap(),
        Duration::from_millis(1)
    );
    let earlier = t0 - Duration::from_secs(5);
    assert!(matches!(
        store
            .reserve_with_lease_at(&key, &request, lease_for, earlier)
            .unwrap(),
        ReserveOutcome::Existing(_)
    ));
    let expired_at = t0 + lease_for;
    let taken = acquired(
        store
            .reserve_with_lease_at(&key, &request, lease_for, expired_at)
            .unwrap(),
    );
    assert!(taken.took_over());
    assert_eq!(
        store.lease_remaining_at(&key, expired_at).unwrap().unwrap(),
        lease_for
    );
}

fn replay_database(state: &Path) -> std::path::PathBuf {
    let mut found = Vec::new();
    let root = state.join("idempotency");
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "replay-v1.sqlite3" {
            found.push(entry.path());
        }
        if entry.file_name() == "scopes" {
            for scope in fs::read_dir(entry.path()).unwrap() {
                let db = scope.unwrap().path().join("replay-v1.sqlite3");
                if db.is_file() {
                    found.push(db);
                }
            }
        }
    }
    assert_eq!(
        found.len(),
        1,
        "expected one replay database, found {found:?}"
    );
    found.remove(0)
}

#[test]
fn legacy_rows_without_lease_never_expire() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let opened = store(dir.path(), &state);
    let key = key();
    let request = request();
    let lease = acquired(opened.reserve(&key, &request).unwrap());
    assert!(matches!(
        opened
            .reserve_with_lease(&key, &request, Duration::from_secs(60))
            .unwrap(),
        ReserveOutcome::Existing(record) if record.state == ReplayState::NotStarted
    ));
    assert!(opened.lease_remaining(&key).unwrap().is_none());
    drop(lease);
    drop(opened);

    let db = replay_database(&state);
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE);
             CREATE TABLE replay_entries_v1 AS
             SELECT key_hash, request_hash, state, generation, owner_token, outcome_kind,
                    outcome_digest, post_image_digest, evidence_digest
             FROM replay_entries;
             DROP TABLE replay_entries;
             CREATE TABLE replay_entries (
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
             INSERT INTO replay_entries
             SELECT * FROM replay_entries_v1;
             DROP TABLE replay_entries_v1;",
        )
        .unwrap();
    }
    let reopened = store(dir.path(), &state);
    assert!(matches!(
        reopened
            .reserve_with_lease(&key, &request, Duration::from_secs(60))
            .unwrap(),
        ReserveOutcome::Existing(record) if record.state == ReplayState::NotStarted
    ));
    assert!(reopened.lease_remaining(&key).unwrap().is_none());
}

#[cfg(feature = "__test-internals")]
#[test]
fn expired_started_reads_as_uncertain_with_since() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let t0 = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let lease_for = Duration::from_secs(30);
    let lease = acquired(
        store
            .reserve_with_lease_at(&key, &request, lease_for, t0)
            .unwrap(),
    );
    store.mark_started_at(&lease, t0).unwrap();
    let midway = store
        .inspect_at(&key, t0 + Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(midway.state, ReplayState::Started);
    assert!(midway.uncertain_since().is_none());
    let expired = store.inspect_at(&key, t0 + lease_for).unwrap().unwrap();
    assert_eq!(expired.state, ReplayState::Uncertain);
    assert_eq!(expired.uncertain_since(), Some(t0 + lease_for));
    assert!(matches!(
        store
            .reserve_with_lease_at(&key, &request, lease_for, t0 + lease_for)
            .unwrap(),
        ReserveOutcome::Existing(record) if record.state == ReplayState::Uncertain
    ));
}

#[test]
fn mark_uncertain_records_since() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let before = SystemTime::now();
    let lease = acquired(store.reserve(&key(), &request()).unwrap());
    store.mark_started(&lease).unwrap();
    let uncertain = store.mark_uncertain(&lease).unwrap();
    let since = uncertain.uncertain_since().expect("since");
    assert!(since >= before);
    assert!(since <= SystemTime::now());
    assert_eq!(uncertain.state, ReplayState::Uncertain);
}

#[test]
fn reconcile_clears_uncertain_since() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let lease = acquired(store.reserve(&key, &request).unwrap());
    store.mark_started(&lease).unwrap();
    let uncertain = store.mark_uncertain(&lease).unwrap();
    assert!(uncertain.uncertain_since().is_some());
    let outcome =
        ReplayOutcome::from_response_and_post_image(OutcomeKind::Applied, b"ok", b"post image")
            .unwrap();
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
    assert!(resolved.uncertain_since().is_none());
}

#[test]
fn response_digest_accessor_verifies_embedder_bytes() {
    let response = b"embedder-owned-response";
    let other = b"different-embedder-response";
    let outcome =
        ReplayOutcome::from_response_and_post_image(OutcomeKind::Applied, response, b"post image")
            .unwrap();
    let digest = |bytes: &[u8]| -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    assert_eq!(outcome.response_digest(), digest(response));
    assert_ne!(outcome.response_digest(), digest(other));
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let lease = acquired(store.reserve(&key(), &request()).unwrap());
    store.mark_started(&lease).unwrap();
    let stored = store.complete(&lease, &outcome).unwrap();
    assert_eq!(
        stored.outcome.as_ref().unwrap().response_digest(),
        digest(response)
    );
}

#[cfg(feature = "__test-internals")]
#[test]
fn expired_reservation_cannot_mark_started() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let lease_for = Duration::from_millis(100);
    let t0 = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let lease = acquired(
        store
            .reserve_with_lease_at(&key, &request, lease_for, t0)
            .unwrap(),
    );
    assert!(matches!(
        store.mark_started_at(&lease, t0 + Duration::from_millis(200)),
        Err(ReplayError::StaleOwner)
    ));
    let record = store.inspect(&key).unwrap().unwrap();
    assert_eq!(record.state, ReplayState::NotStarted);
}

#[cfg(feature = "__test-internals")]
#[test]
fn mark_started_with_lease_sets_started_remaining() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let key = key();
    let request = request();
    let t0 = UNIX_EPOCH + Duration::from_secs(1_700_000_100);
    let lease = acquired(
        store
            .reserve_with_lease_at(&key, &request, Duration::from_secs(30), t0)
            .unwrap(),
    );
    let started_for = Duration::from_secs(5);
    store
        .mark_started_with_lease_at(&lease, started_for, t0)
        .unwrap();
    let remaining = store
        .lease_remaining_at(&key, t0)
        .unwrap()
        .expect("started lease");
    assert_eq!(remaining, started_for);
}

#[cfg(feature = "__test-internals")]
#[test]
fn lock_contention_surfaces_as_busy() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let state = root.join("state");
    let holder =
        ReplayStore::open_with_busy_timeout(root, &state, "room-a", Duration::from_millis(50))
            .unwrap();
    let contender =
        ReplayStore::open_with_busy_timeout(root, &state, "room-a", Duration::from_millis(50))
            .unwrap();
    let _hold = holder.begin_immediate_hold().unwrap();
    let err = match contender.reserve_with_lease(&key(), &request(), Duration::from_secs(30)) {
        Err(err) => err,
        Ok(_) => panic!("expected Busy, got Ok"),
    };
    assert!(matches!(err, ReplayError::Busy), "got {err:?}");
}

#[test]
fn from_embedder_digest_accepts_oversized_and_secret_looking_response() {
    let response = vec![b'a'; 1024 * 1024];
    let digest = sha256_hex(&response);
    // Assembled at runtime so the source file does not contain a contiguous
    // private-key envelope (repository_source_is_clean_under_its_own_detector).
    let secretish = [
        b"-----BEGIN RSA ".as_slice(),
        b"PRIVATE KEY-----\nMIIEowIBAAKCAQEA0Z3VS5JJcds3xfn/FL+/oEaL\n-----END RSA ",
        b"PRIVATE KEY-----\n",
    ]
    .concat();
    assert!(matches!(
        ReplayOutcome::from_response_and_post_image(OutcomeKind::Applied, &secretish, b"post"),
        Err(ReplayError::UnsafeOutcome)
    ));
    let outcome = ReplayOutcome::from_embedder_digest(
        OutcomeKind::Applied,
        response.len(),
        &digest,
        b"post image",
    )
    .unwrap();
    assert_eq!(outcome.response_digest(), digest);
    let secret_digest = sha256_hex(&secretish);
    let secret_outcome = ReplayOutcome::from_embedder_digest(
        OutcomeKind::Applied,
        secretish.len(),
        &secret_digest,
        b"post image",
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), &dir.path().join("state"));
    let lease = acquired(
        store
            .reserve_with_lease(&key(), &request(), Duration::from_secs(30))
            .unwrap(),
    );
    store
        .mark_started_with_lease(&lease, Duration::from_secs(30))
        .unwrap();
    let stored = store.complete(&lease, &secret_outcome).unwrap();
    assert_eq!(
        stored.outcome.as_ref().unwrap().response_digest(),
        secret_digest
    );
}

#[test]
fn legacy_records_block_until_cutover_then_new_keys_admit() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let store = store(dir.path(), &state);
    let legacy = state.join("idempotency/records");
    fs::create_dir_all(&legacy).unwrap();
    let fresh = ReplayKey::new("fresh-key").unwrap();
    assert!(matches!(
        store.reserve(&fresh, &request()),
        Err(ReplayError::LegacyCutoverRequired)
    ));
    store
        .confirm_legacy_cutover(
            &LegacyCutoverEvidence::from_checkpoint(b"old writers stopped").unwrap(),
        )
        .unwrap();
    let lease = acquired(store.reserve(&fresh, &request()).unwrap());
    store.release_not_started(&lease).unwrap();

    let mut frame = b"symforge-idempotency-key-v1\0".to_vec();
    frame.extend_from_slice(b"blocked-key");
    let blocked_hash: String = Sha256::digest(&frame)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    fs::create_dir_all(legacy.join(&blocked_hash)).unwrap();
    fs::write(legacy.join(&blocked_hash).join("record.json"), b"{}").unwrap();
    let blocked = ReplayKey::new("blocked-key").unwrap();
    assert!(matches!(
        store.reserve(&blocked, &request()),
        Err(ReplayError::LegacyStateRequiresReconciliation)
    ));
    fs::remove_dir_all(&legacy).unwrap();
    assert!(matches!(
        store.reserve(&blocked, &request()).unwrap(),
        ReserveOutcome::Acquired(_)
    ));
}

#[cfg(feature = "__test-internals")]
#[test]
fn legacy_key_hash_matches_replay_key_hash() {
    let raw = "shared-replay-key";
    let old = symforge::idempotency::IdempotencyKey::new(raw)
        .unwrap()
        .hash_hex();
    let mut frame = b"symforge-idempotency-key-v1\0".to_vec();
    frame.extend_from_slice(raw.as_bytes());
    let framed: String = Sha256::digest(&frame)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(old, framed);

    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let store = store(dir.path(), &state);
    let legacy = state.join("idempotency/records");
    fs::create_dir_all(legacy.join(&old)).unwrap();
    fs::write(legacy.join(&old).join("record.json"), b"{}").unwrap();
    store
        .confirm_legacy_cutover(&LegacyCutoverEvidence::from_checkpoint(b"cutover").unwrap())
        .unwrap();
    let key = ReplayKey::new(raw).unwrap();
    assert!(matches!(
        store.reserve(&key, &request()),
        Err(ReplayError::LegacyStateRequiresReconciliation)
    ));
    let unframed: String = Sha256::digest(raw.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_ne!(unframed, old);
    fs::rename(legacy.join(&old), legacy.join(&unframed)).unwrap();
    assert!(matches!(
        store.reserve(&key, &request()).unwrap(),
        ReserveOutcome::Acquired(_)
    ));
}

#[test]
fn replay_record_struct_literal_stays_constructible() {
    let request = request();
    let record = ReplayRecord {
        key_hash: "literal".to_owned(),
        request: request.clone(),
        state: ReplayState::Uncertain,
        generation: 1,
        outcome: None,
        evidence_digest: None,
    };
    assert_eq!(record.request, request);
    assert_eq!(record.uncertain_since(), None);
}
