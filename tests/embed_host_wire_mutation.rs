#![cfg(feature = "embed")]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::ProcessIndexRuntime;
use symforge::embed::parity::edit::{
    EditTarget, WireBatchEditAction, WireEditGuard, WireEditRequest,
};
use symforge::embed::parity::host::{
    HostEffectCertainty, HostLimits, HostPhase, HostRefusalKind, HostRequest, HostResponse,
    HostRights, HostRoom, HostRoomConfig, HostRoomGrant, HostRuntimeOwner,
};
use symforge::embed::parity::remediation::{SecretRemediationScope, SecretScanLimits};

fn room(root: &std::path::Path, cap: u32, rights: HostRights) -> HostRoom {
    let room_id = "wire-recovery-room";
    let grant = HostRoomGrant::new(
        room_id.to_owned(),
        root.to_path_buf(),
        "stable-guest-source".to_owned(),
        rights,
    )
    .expect("trusted room grant");
    let config = HostRoomConfig {
        room_id: room_id.to_owned(),
        limits: HostLimits {
            max_response_bytes: cap,
            ..HostLimits::default()
        },
    };
    let room = HostRoom::open(
        ProcessIndexRuntime::acquire().expect("runtime"),
        grant,
        config,
    )
    .expect("bound room");
    let control = room.control().expect("room control");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) = room
            .dispatch(&HostRequest::Status, &control)
            .expect("status")
        else {
            panic!("status reply");
        };
        if status.phase == HostPhase::Current {
            return room;
        }
        assert!(Instant::now() < deadline, "room source did not publish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn read_only_room_denies_write_and_rejects_unknown_nested_scan_option() {
    let root = tempfile::tempdir().expect("repository");
    git2::Repository::init(root.path()).expect("git repository");
    fs::write(
        root.path().join("lib.rs"),
        b"pub fn target() -> u32 { 1 }\n",
    )
    .expect("source");
    let room = room(root.path(), 4_096, HostRights::read_only());
    let control = room.control().expect("room control");
    let plan = HostRequest::Edit {
        request: WireEditRequest::Plan {
            target: EditTarget {
                path: "lib.rs".into(),
                name: "target".into(),
                kind: None,
                symbol_line: None,
            },
        },
        operation_key: None,
    };
    assert_eq!(
        room.dispatch(&plan, &control)
            .expect_err("edit right required")
            .kind,
        HostRefusalKind::Denied
    );
    let inspect = HostRequest::InspectSecrets {
        scope: SecretRemediationScope::Paths(vec!["lib.rs".into()]),
        limits: SecretScanLimits::default(),
    };
    assert_eq!(
        room.dispatch(&inspect, &control)
            .expect_err("secret inspection right required")
            .kind,
        HostRefusalKind::Denied
    );
    let malformed = serde_json::json!({"InspectSecrets": {
        "scope": {"Paths": ["lib.rs"]},
        "limits": {"max_files": 1, "max_findings": 1, "max_total_bytes": 4096, "ignored_future_flag": "untrusted-field-value-must-not-echo"}
    }});
    let refusal = room
        .dispatch_wire(&serde_json::to_vec(&malformed).expect("wire"), &control)
        .expect_err("nested option must refuse");
    assert_eq!(refusal.kind, HostRefusalKind::InvalidRequest);
    assert!(
        !serde_json::to_string(&refusal)
            .expect("refusal wire")
            .contains("untrusted-field-value-must-not-echo")
    );
}

#[test]
fn committed_batch_overflow_has_bound_recovery_and_replays_after_rebind() {
    let root = tempfile::tempdir().expect("repository");
    git2::Repository::init(root.path()).expect("git repository");
    fs::create_dir(root.path().join("src")).expect("source directory");
    for index in 0..70 {
        fs::write(
            root.path().join(format!("src/file_{index:02}.rs")),
            b"pub fn target() -> u32 { 1 }\n",
        )
        .expect("source file");
    }
    let rights = HostRights {
        edit: true,
        ..HostRights::read_only()
    };
    let small = room(root.path(), 4_096, rights);
    let control = small.control().expect("room control");
    let mut actions = Vec::new();
    for index in 0..70 {
        let path = format!("src/file_{index:02}.rs");
        let plan = HostRequest::Edit {
            request: WireEditRequest::Plan {
                target: EditTarget {
                    path: path.clone(),
                    name: "target".into(),
                    kind: None,
                    symbol_line: None,
                },
            },
            operation_key: None,
        };
        let HostResponse::Edit(value) = small.dispatch(&plan, &control).expect("plan exact file")
        else {
            panic!("edit plan reply");
        };
        let guard: WireEditGuard = serde_json::from_value(value["result"]["guard"].clone())
            .expect("deserialize source facts");
        actions.push(WireBatchEditAction::Replace {
            guard,
            new_body: "pub fn target() -> u32 { 2 }\n".into(),
        });
    }
    let apply = HostRequest::Edit {
        request: WireEditRequest::ApplyBatchEdit { actions },
        operation_key: Some("fixture-batch-recovery-key".into()),
    };
    let wire = serde_json::to_vec(&apply).expect("wire apply");
    assert!(wire.len() < small.limits().max_request_bytes as usize);
    let before = match small
        .dispatch(&HostRequest::Status, &control)
        .expect("status")
    {
        HostResponse::Status(status) => status.source_version,
        _ => panic!("status reply"),
    };
    let refusal = small
        .dispatch_wire(&wire, &control)
        .expect_err("full applied receipt exceeds 4KiB");
    assert_eq!(refusal.kind, HostRefusalKind::ResponseTooLarge);
    let recovery = refusal.recovery.expect("effect and recovery evidence");
    assert_eq!(recovery.certainty, HostEffectCertainty::Committed);
    assert!(!recovery.canonical_request_hash.is_empty());
    assert!(!recovery.operation_key_digest.is_empty());
    let evidence = serde_json::to_vec(&recovery).expect("recovery wire");
    assert!(evidence.len() < 4_096);
    assert!(!String::from_utf8_lossy(&evidence).contains("fixture-batch-recovery-key"));
    for index in 0..70 {
        assert!(
            fs::read_to_string(root.path().join(format!("src/file_{index:02}.rs")))
                .expect("applied file")
                .contains("{ 2 }")
        );
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) = small
            .dispatch(&HostRequest::Status, &control)
            .expect("status")
        else {
            panic!("status reply");
        };
        if status.phase == HostPhase::Current && status.source_version > before {
            break;
        }
        assert!(Instant::now() < deadline, "refresh did not publish");
        std::thread::sleep(Duration::from_millis(10));
    }
    small
        .close(&control)
        .expect("close old source before rebind");
    let large = room(root.path(), 65_536, rights);
    let large_control = large.control().expect("room control");
    let full = large
        .dispatch_wire(&wire, &large_control)
        .expect("attested completed replay");
    let HostResponse::Edit(value) = serde_json::from_slice(&full).expect("reply wire") else {
        panic!("edit reply");
    };
    assert_eq!(value["result"]["replayed"], true);
    assert_eq!(
        value["result"]["files"]
            .as_array()
            .expect("file receipts")
            .len(),
        70
    );
}

#[test]
fn shared_source_rooms_isolate_replay_keys_and_fence_duplicate_active_ids() {
    let root = tempfile::tempdir().expect("repository");
    git2::Repository::init(root.path()).expect("git repository");
    fs::write(
        root.path().join("lib.rs"),
        b"pub fn target() -> u32 { 1 }\n",
    )
    .expect("source");
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let rights = HostRights {
        edit: true,
        ..HostRights::read_only()
    };
    let open = |id: &str| {
        HostRoom::open_in_runtime(
            &owner,
            HostRoomGrant::new(
                id.to_owned(),
                root.path().to_path_buf(),
                "shared-source-scope".to_owned(),
                rights,
            )
            .expect("grant"),
            HostRoomConfig {
                room_id: id.to_owned(),
                limits: HostLimits::default(),
            },
        )
    };
    let first = open("first-room").expect("first room");
    let second = open("second-room").expect("second room");
    assert_eq!(
        open("first-room")
            .err()
            .expect("duplicate active room refused")
            .kind,
        HostRefusalKind::Denied
    );
    let control = first.control().expect("control");
    let wait_current = |room: &HostRoom| {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let HostResponse::Status(status) = room
                .dispatch(&HostRequest::Status, &control)
                .expect("status")
            else {
                panic!("status reply");
            };
            if status.phase == HostPhase::Current {
                break;
            }
            assert!(Instant::now() < deadline, "source did not publish");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let edit = |room: &HostRoom, value: u32| {
        wait_current(room);
        let HostResponse::Status(before) = room
            .dispatch(&HostRequest::Status, &control)
            .expect("pre-edit status")
        else {
            panic!("status reply");
        };
        let HostResponse::Edit(plan) = room
            .dispatch(
                &HostRequest::Edit {
                    request: WireEditRequest::Plan {
                        target: EditTarget {
                            path: "lib.rs".into(),
                            name: "target".into(),
                            kind: None,
                            symbol_line: None,
                        },
                    },
                    operation_key: None,
                },
                &control,
            )
            .expect("plan")
        else {
            panic!("plan reply");
        };
        let guard: WireEditGuard =
            serde_json::from_value(plan["result"]["guard"].clone()).expect("guard");
        room.dispatch(
            &HostRequest::Edit {
                request: WireEditRequest::ApplyBatchEdit {
                    actions: vec![WireBatchEditAction::Replace {
                        guard,
                        new_body: format!("pub fn target() -> u32 {{ {value} }}\n"),
                    }],
                },
                operation_key: Some("same-key-in-different-rooms".into()),
            },
            &control,
        )
        .expect("room-scoped apply");
        before.source_version
    };
    let first_version = edit(&first, 2);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) = second
            .dispatch(&HostRequest::Status, &control)
            .expect("shared status")
        else {
            panic!("status reply");
        };
        if status.phase == HostPhase::Current && status.source_version > first_version {
            break;
        }
        assert!(Instant::now() < deadline, "first edit did not publish");
        std::thread::sleep(Duration::from_millis(10));
    }
    edit(&second, 3);
    assert!(
        fs::read_to_string(root.path().join("lib.rs"))
            .expect("second source")
            .contains("{ 3 }")
    );
    first.close(&control).expect("first close");
    assert!(open("first-room").is_ok(), "closed room ID may rebind");
    second.close(&control).expect("second close");
}
