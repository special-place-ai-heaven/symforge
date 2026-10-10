#![cfg(feature = "embed")]

use std::time::{Duration, Instant};

use symforge::embed::parity::host::{
    HostLimits, HostPhase, HostRefusalKind, HostRequest, HostResponse, HostRights, HostRoom,
    HostRoomConfig, HostRoomGrant, HostRuntimeOwner,
};
use symforge::embed::parity::source_options::{EmbeddedOpenOptions, EmbeddedStateSelection};
use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    std::fs::write(root.path().join("lib.rs"), b"pub fn answer() -> u8 { 1 }\n").unwrap();
    root
}

fn wait_current(handle: &symforge::embed::EmbeddedSourceHandle) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if handle.runtime_view().phase == SourceRuntimePhase::Current {
            return;
        }
        assert!(Instant::now() < deadline, "source did not become current");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn memory_only_creates_no_state_and_source_can_reopen_with_automatic_placement() {
    let root = repository();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let source = runtime
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(root.path().to_path_buf()),
            EmbeddedOpenOptions {
                state: EmbeddedStateSelection::MemoryOnly,
            },
        )
        .unwrap();
    wait_current(&source);
    assert!(!root.path().join(".symforge").exists());
    source
        .begin_close()
        .wait(Instant::now() + Duration::from_secs(20))
        .unwrap();
    let reopened = runtime
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(root.path().to_path_buf()),
            EmbeddedOpenOptions {
                state: EmbeddedStateSelection::Automatic,
            },
        )
        .unwrap();
    wait_current(&reopened);
    assert!(root.path().join(".symforge").is_dir());
    reopened
        .begin_close()
        .wait(Instant::now() + Duration::from_secs(20))
        .unwrap();
    runtime
        .begin_shutdown()
        .wait(Instant::now() + Duration::from_secs(20))
        .unwrap();
}

#[test]
fn selected_user_local_base_is_partitioned_by_source_root() {
    let first = repository();
    let second = repository();
    let base = tempfile::tempdir().unwrap();
    let owner = HostRuntimeOwner::new_with_options(
        ProcessIndexRuntime::acquire().unwrap(),
        EmbeddedOpenOptions {
            state: EmbeddedStateSelection::UserLocal {
                base_directory: base.path().to_path_buf(),
            },
        },
    );
    let mut rights = HostRights::read_only();
    rights.checkpoint = true;
    let mut rooms = Vec::new();
    for (id, root) in [("first", &first), ("second", &second)] {
        let room = HostRoom::open_in_runtime(
            &owner,
            HostRoomGrant::new(
                id.to_owned(),
                root.path().to_path_buf(),
                id.to_owned(),
                rights,
            )
            .unwrap(),
            HostRoomConfig {
                room_id: id.to_owned(),
                limits: HostLimits::default(),
            },
        )
        .unwrap();
        let control = room.control().unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let HostResponse::Status(status) =
                room.dispatch(&HostRequest::Status, &control).unwrap()
            else {
                panic!("status reply");
            };
            if status.phase == HostPhase::Current {
                break;
            }
            assert!(Instant::now() < deadline, "source did not become current");
            std::thread::sleep(Duration::from_millis(10));
        }
        room.dispatch(
            &HostRequest::Checkpoint {
                verify_after_write: true,
                export_artifact: false,
            },
            &control,
        )
        .unwrap();
        rooms.push(room);
    }
    let children: Vec<_> = std::fs::read_dir(base.path().join("projects"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(children.len(), 2);
    assert!(
        children
            .iter()
            .all(|child| child.join("index.bin").is_file())
    );
    assert!(!first.path().join(".symforge").exists());
    assert!(!second.path().join(".symforge").exists());
    for room in rooms {
        room.close(&room.control().unwrap()).unwrap();
    }
    owner
        .shutdown(
            &symforge::embed::parity::host::OperationControl::new(Duration::from_secs(20)).unwrap(),
        )
        .unwrap();
}

#[test]
fn memory_only_checkpoint_refuses_durable_persistence() {
    let root = repository();
    let owner = HostRuntimeOwner::new_with_options(
        ProcessIndexRuntime::acquire().unwrap(),
        EmbeddedOpenOptions {
            state: EmbeddedStateSelection::MemoryOnly,
        },
    );
    let mut rights = HostRights::read_only();
    rights.checkpoint = true;
    let room = HostRoom::open_in_runtime(
        &owner,
        HostRoomGrant::new(
            "memory-room".into(),
            root.path().to_path_buf(),
            "memory-source".into(),
            rights,
        )
        .unwrap(),
        HostRoomConfig {
            room_id: "memory-room".into(),
            limits: HostLimits::default(),
        },
    )
    .unwrap();
    let control = room.control().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) = room.dispatch(&HostRequest::Status, &control).unwrap()
        else {
            panic!("status reply");
        };
        if status.phase == HostPhase::Current {
            break;
        }
        assert!(Instant::now() < deadline, "source did not become current");
        std::thread::sleep(Duration::from_millis(10));
    }
    let refusal = room
        .dispatch(
            &HostRequest::Checkpoint {
                verify_after_write: true,
                export_artifact: false,
            },
            &control,
        )
        .expect_err("memory-only source has no durable checkpoint");
    assert_eq!(refusal.kind, HostRefusalKind::PersistenceUnavailable);
    assert!(!root.path().join(".symforge").exists());
    room.close(&control).unwrap();
    owner.shutdown(&control).unwrap();
}
