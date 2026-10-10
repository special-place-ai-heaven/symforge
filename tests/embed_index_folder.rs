#![cfg(feature = "embed")]
//! MCP `index_folder` parity for embedded sources: `allow_protected_root`,
//! the snapshot reset (at open and in place) and `idempotency_key` replay.
//!
//! The reset assertions mirror the MCP golden in
//! `test_index_folder_reset_deletes_snapshot_scope_and_reports_fresh_status`
//! (`src/protocol/tools.rs`): the same scope label, and the same
//! `load_source` / `reset_state` / `index_state` runtime fields rendered by
//! the shared health engine after the reset reload publishes.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::embed::parity::host::{
    HostCheckpointOutcome, HostHealthRequest, HostLimits, HostPhase, HostRefreshReceipt,
    HostRefreshRequest, HostRefusalKind, HostRequest, HostResponse, HostRights, HostRoom,
    HostRoomConfig, HostRoomGrant, HostRuntimeOwner, OperationControl,
};
use symforge::embed::parity::source_options::{EmbeddedOpenOptions, EmbeddedStateSelection};
use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

const RESET_SCOPE: &str = ".symforge/index.bin,.symforge/index.bin.tmp";

fn write_source(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), b"pub fn answer() -> u32 { 42 }\n").unwrap();
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    write_source(root.path());
    root
}

fn rights() -> HostRights {
    HostRights {
        query: true,
        refresh: true,
        checkpoint: true,
        ..HostRights::read_only()
    }
}

fn wait_current(room: &HostRoom, control: &OperationControl) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) = room.dispatch(&HostRequest::Status, control).unwrap()
        else {
            unreachable!()
        };
        if status.phase == HostPhase::Current {
            return;
        }
        assert!(Instant::now() < deadline, "source failed to publish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn room_in(owner: &HostRuntimeOwner, root: &Path, id: &str, rights: HostRights) -> HostRoom {
    let grant = HostRoomGrant::new(
        id.to_owned(),
        root.to_path_buf(),
        "shared-scope".to_owned(),
        rights,
    )
    .unwrap();
    let room = HostRoom::open_in_runtime(
        owner,
        grant,
        HostRoomConfig {
            room_id: id.to_owned(),
            limits: HostLimits::default(),
        },
    )
    .unwrap();
    wait_current(&room, &room.control().unwrap());
    room
}

fn refresh(
    room: &HostRoom,
    request: HostRefreshRequest,
) -> Result<HostRefreshReceipt, HostRefusalKind> {
    let control = room.control().unwrap();
    match room.dispatch(&HostRequest::RefreshWith(request), &control) {
        Ok(HostResponse::Refresh(receipt)) => Ok(receipt),
        Ok(_) => panic!("refresh response"),
        Err(refusal) => Err(refusal.kind),
    }
}

fn checkpoint(room: &HostRoom) {
    let HostResponse::Checkpoint(receipt) = room
        .dispatch(
            &HostRequest::Checkpoint {
                verify_after_write: true,
                export_artifact: false,
            },
            &room.control().unwrap(),
        )
        .unwrap()
    else {
        panic!("checkpoint response")
    };
    assert_eq!(receipt.outcome, HostCheckpointOutcome::WrittenCurrent);
}

fn health_text(room: &HostRoom) -> String {
    let HostResponse::HealthReport(report) = room
        .dispatch(
            &HostRequest::HealthReport(HostHealthRequest::default()),
            &room.control().unwrap(),
        )
        .unwrap()
    else {
        panic!("health response")
    };
    report.report.unwrap()
}

#[test]
fn allow_protected_root_admits_exactly_the_protected_root() {
    let parent = tempfile::tempdir().unwrap();
    // `System32` is a protected root name on every platform.
    let protected = parent.path().join("System32");
    write_source(&protected);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let memory_only = EmbeddedOpenOptions {
        state: EmbeddedStateSelection::MemoryOnly,
        ..Default::default()
    };
    let refused = runtime
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(protected.clone()),
            memory_only.clone(),
        )
        .unwrap_err();
    assert_eq!(refused.kind_name(), "InvalidSelection");

    let handle = runtime
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(protected.clone()),
            EmbeddedOpenOptions {
                allow_protected_root: true,
                ..memory_only
            },
        )
        .expect("explicit protected authority opens the exact root");
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(
            Instant::now() < deadline,
            "protected source failed to publish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(handle.index_progress().files_parsed, 1);
    assert!(!protected.join(".symforge").exists(), "read/index-only");
    assert!(handle.open_reset_receipt().is_none());
}

#[test]
fn in_place_reset_and_idempotent_replay_follow_index_folder_semantics() {
    let root = fixture();
    let control_dir = tempfile::tempdir().unwrap();
    let owner = HostRuntimeOwner::new_with_options(
        ProcessIndexRuntime::acquire().unwrap(),
        EmbeddedOpenOptions {
            state: EmbeddedStateSelection::ProjectLocal,
            replay_control_directory: Some(control_dir.path().to_path_buf()),
            ..Default::default()
        },
    );
    let room = room_in(&owner, root.path(), "reset-room", rights());
    let snapshot = root.path().join(".symforge/index.bin");
    checkpoint(&room);
    assert!(snapshot.exists());

    let request = HostRefreshRequest {
        reset: true,
        idempotency_key: Some("reset-once".to_owned()),
    };
    let first = refresh(&room, request.clone()).unwrap();
    let reset = first.reset.clone().expect("reset receipt");
    assert_eq!(reset.scope, RESET_SCOPE);
    assert_eq!((reset.removed, reset.missing), (1, 1));
    assert!(!first.replayed);
    assert_eq!(first.replay_recorded, Some(true));
    assert!(!snapshot.exists(), "reset deletes the snapshot scope");

    // The reset generation is recorded once the reload publishes.
    let deadline = Instant::now() + Duration::from_secs(20);
    let text = loop {
        wait_current(&room, &room.control().unwrap());
        let text = health_text(&room);
        if text.contains("index_state=index_folder_reset") {
            break text;
        }
        assert!(Instant::now() < deadline, "reset never recorded:\n{text}");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(text.contains("load_source=fresh_load"), "{text}");
    assert!(text.contains("reset_state=current_project:p"), "{text}");
    assert!(text.contains("index_id=index-"), "{text}");

    // Same key, same request: the original receipt; the reset is not repeated.
    checkpoint(&room);
    assert!(snapshot.exists());
    let replayed = refresh(&room, request).unwrap();
    assert!(replayed.replayed);
    assert_eq!(replayed.ticket_identity, first.ticket_identity);
    assert_eq!(replayed.reset, first.reset);
    assert!(snapshot.exists(), "a replay never repeats the reset");

    // Same key, different request: refused, as MCP reports a conflict.
    assert_eq!(
        refresh(
            &room,
            HostRefreshRequest {
                reset: false,
                idempotency_key: Some("reset-once".to_owned()),
            },
        )
        .unwrap_err(),
        HostRefusalKind::InvalidRequest
    );

    // No key: a plain in-place reset.
    let plain = refresh(
        &room,
        HostRefreshRequest {
            reset: true,
            idempotency_key: None,
        },
    )
    .unwrap();
    assert_eq!(plain.replay_recorded, None);
    assert_eq!(plain.reset.unwrap().removed, 1);

    // Reset deletes persisted state, so it also needs the checkpoint right.
    let refresh_only = room_in(
        &owner,
        root.path(),
        "refresh-only-room",
        HostRights {
            checkpoint: false,
            ..rights()
        },
    );
    assert_eq!(
        refresh(
            &refresh_only,
            HostRefreshRequest {
                reset: true,
                idempotency_key: None,
            },
        )
        .unwrap_err(),
        HostRefusalKind::Denied
    );
}

#[test]
fn idempotency_without_a_host_control_directory_is_persistence_unavailable() {
    let root = fixture();
    let owner = HostRuntimeOwner::new_with_options(
        ProcessIndexRuntime::acquire().unwrap(),
        EmbeddedOpenOptions {
            state: EmbeddedStateSelection::MemoryOnly,
            ..Default::default()
        },
    );
    let room = room_in(&owner, root.path(), "no-control-room", rights());
    assert_eq!(
        refresh(
            &room,
            HostRefreshRequest {
                reset: false,
                idempotency_key: Some("k".to_owned()),
            },
        )
        .unwrap_err(),
        HostRefusalKind::PersistenceUnavailable
    );
    // MCP refuses a reset it cannot perform; MemoryOnly has no snapshot scope.
    assert_eq!(
        refresh(
            &room,
            HostRefreshRequest {
                reset: true,
                idempotency_key: None,
            },
        )
        .unwrap_err(),
        HostRefusalKind::PersistenceUnavailable
    );
    let relative = ProcessIndexRuntime::acquire()
        .unwrap()
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(root.path().to_path_buf()),
            EmbeddedOpenOptions {
                replay_control_directory: Some("relative-control".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert_eq!(relative.kind_name(), "InvalidSelection");
}

#[test]
fn reset_at_open_deletes_the_snapshot_before_restore() {
    let root = fixture();
    let options = EmbeddedOpenOptions {
        state: EmbeddedStateSelection::ProjectLocal,
        ..Default::default()
    };
    {
        let owner = HostRuntimeOwner::new_with_options(
            ProcessIndexRuntime::acquire().unwrap(),
            options.clone(),
        );
        let room = room_in(&owner, root.path(), "seed-room", rights());
        checkpoint(&room);
        owner.shutdown(&room.control().unwrap()).unwrap();
    }
    let snapshot = root.path().join(".symforge/index.bin");
    assert!(snapshot.exists());

    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(root.path().to_path_buf()),
            EmbeddedOpenOptions {
                reset_snapshot_state: true,
                ..options
            },
        )
        .unwrap();
    let reset = handle.open_reset_receipt().expect("open reset receipt");
    assert_eq!(reset.scope, RESET_SCOPE);
    assert_eq!((reset.removed, reset.missing), (1, 1));
    assert!(!snapshot.exists());
}
