#![cfg(feature = "embed")]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::ProcessIndexRuntime;
use symforge::embed::parity::host::{
    HostCheckpointOutcome, HostHealth, HostLimits, HostPhase, HostPromptRequest, HostRefusalKind,
    HostRequest, HostResourceContent, HostResourceRequest, HostResponse, HostRights, HostRoom,
    HostRoomConfig, HostRoomGrant, HostRuntimeOwner, OperationControl, OperationStop,
};
use symforge::embed::parity::read::FileContentRequest;
use symforge::embed::parity::{
    QueryLimits, QueryOperationKind, QueryOutput, QueryRefusalKind, QueryRequest,
};

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        b"pub fn answer() -> u32 { 42 }\r\n",
    )
    .unwrap();
    root
}

fn open_room(
    root: &std::path::Path,
    room_id: &str,
    rights: HostRights,
    limits: HostLimits,
) -> HostRoom {
    let grant = HostRoomGrant::new(
        room_id.to_owned(),
        root.to_path_buf(),
        format!("{room_id}-guest-source"),
        rights,
    )
    .unwrap();
    let room = HostRoom::open(
        ProcessIndexRuntime::acquire().unwrap(),
        grant,
        HostRoomConfig {
            room_id: room_id.to_owned(),
            limits,
        },
    )
    .unwrap();
    let control = room.control().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) = room.dispatch(&HostRequest::Status, &control).unwrap()
        else {
            unreachable!()
        };
        if status.phase == HostPhase::Current {
            return room;
        }
        assert!(Instant::now() < deadline, "source failed to publish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn wire_dispatch_is_source_bound_and_refuses_untrusted_lifecycle_requests() {
    let root = fixture();
    let room = open_room(
        root.path(),
        "room-a",
        HostRights::read_only(),
        HostLimits::default(),
    );
    let control = room.control().unwrap();
    let wire = serde_json::to_vec(&HostRequest::Health).unwrap();
    let response = room.dispatch_wire(&wire, &control).unwrap();
    let HostResponse::Health(HostHealth { status, current }) =
        serde_json::from_slice(&response).unwrap()
    else {
        panic!("health response")
    };
    assert_eq!(status.room_id, "room-a");
    let proof = current.expect("current source proof").proof;
    assert_eq!(
        status.source_capture_publication_identity.as_deref(),
        Some(proof.source_capture_publication_identity.as_str())
    );
    assert!(!proof.source_identity_digest.is_empty());
    assert!(!proof.manifest_digest.is_empty());
    assert_eq!(status.engine.engine, "SymForge");
    assert_eq!(
        status.engine.api_version,
        symforge::embed::parity::API_VERSION
    );
    assert!(!status.engine.instance_identity.is_empty());

    let HostResponse::Query(query) = room
        .dispatch(
            &HostRequest::Query {
                request: QueryRequest::File {
                    path: "src/lib.rs".into(),
                    start_line: None,
                    end_line: None,
                },
                limits: QueryLimits {
                    max_results: 10_000,
                    max_bytes: 1_048_576,
                },
            },
            &control,
        )
        .unwrap()
    else {
        panic!("query response")
    };
    assert_eq!(query.semantic_operation, QueryOperationKind::File);
    assert_eq!(
        query.semantic_operation_schema_version,
        symforge::embed::parity::API_VERSION
    );
    assert_eq!(query.source_capture_kind, "IndexCensus");
    assert!(!query.canonical_argument_hash.is_empty());
    assert!(!query.source_capture_canonical_argument_hash.is_empty());
    assert!(!query.provenance.identity.is_empty());
    assert!(!query.provenance.authorities.is_empty());
    assert!(!query.producing_runtime_identity.is_empty());
    assert!(!query.source_capture_runtime_identity.is_empty());
    assert_eq!(
        query.source_capture_publication_identity,
        proof.source_capture_publication_identity
    );
    assert_ne!(
        query.publication_identity,
        proof.source_capture_publication_identity
    );
    assert_eq!(query.publication_generation, proof.publication_generation);
    assert_eq!(query.content_generation, proof.content_generation);
    assert_eq!(
        query.effective_limits.max_results,
        room.limits().max_query_results
    );
    assert_eq!(
        query.effective_limits.max_bytes,
        room.limits().max_query_bytes
    );
    let refusal = room
        .dispatch(
            &HostRequest::Query {
                request: QueryRequest::File {
                    path: "src/missing.rs".into(),
                    start_line: None,
                    end_line: None,
                },
                limits: QueryLimits::default(),
            },
            &control,
        )
        .unwrap_err();
    assert_eq!(
        refusal.query_kind,
        Some(symforge::embed::parity::QueryRefusalKind::NotFound)
    );
    assert_eq!(refusal.semantic_operation, Some(QueryOperationKind::File));
    assert!(!refusal.operation_identity.unwrap().is_empty());
    let HostResponse::Catalog(catalog) = room.dispatch(&HostRequest::Catalog, &control).unwrap()
    else {
        panic!("catalog response")
    };
    assert!(catalog.query_operations.contains(&"File".to_owned()));
    assert!(
        catalog
            .resources
            .iter()
            .any(|resource| resource.name == "file-content")
    );
    assert_eq!(catalog.prompts.len(), 8);
    let admin = catalog
        .prompts
        .iter()
        .find(|prompt| prompt.name == "symforge-admin")
        .unwrap();
    assert!(admin.requires_dashboard_observation);
    assert!(!admin.dashboard_observation_available);
    let HostResponse::Resource(resource) = room
        .dispatch(
            &HostRequest::Resource(HostResourceRequest::FileContent {
                path: "src/lib.rs".into(),
                start_line: None,
                end_line: None,
            }),
            &control,
        )
        .unwrap()
    else {
        panic!("file resource")
    };
    assert_eq!(resource.uri, "symforge://file/content");
    assert!(matches!(resource.content, HostResourceContent::Query(_)));
    let HostResponse::Prompt(prompt) = room
        .dispatch(
            &HostRequest::Prompt(HostPromptRequest::Review {
                path: Some("src/lib.rs".into()),
                focus: None,
            }),
            &control,
        )
        .unwrap()
    else {
        panic!("review prompt")
    };
    assert_eq!(prompt.name, "symforge-review");
    assert!(
        prompt
            .messages
            .iter()
            .any(|message| message.resource_uri.as_deref() == Some("symforge://repo/health"))
    );

    let checkpoint = HostRequest::Checkpoint {
        verify_after_write: true,
        export_artifact: false,
    };
    assert_eq!(
        room.dispatch(&checkpoint, &control).unwrap_err().kind,
        HostRefusalKind::Denied
    );
    assert_eq!(
        room.dispatch(&HostRequest::Refresh, &control)
            .unwrap_err()
            .kind,
        HostRefusalKind::Denied
    );
    assert_eq!(
        room.dispatch_wire(
            &vec![b' '; room.limits().max_request_bytes as usize + 1],
            &control
        )
        .unwrap_err()
        .kind,
        HostRefusalKind::RequestTooLarge
    );
    assert_eq!(
        room.dispatch_wire(br#"{"root":"other","request":"Health"}"#, &control)
            .unwrap_err()
            .kind,
        HostRefusalKind::InvalidRequest
    );
    let unknown = serde_json::json!({"Resource": {"FileContent": {
        "path": "src/lib.rs", "start_line": null, "end_line": null,
        "unrecognized_option": true
    }}});
    assert_eq!(
        room.dispatch_wire(&serde_json::to_vec(&unknown).unwrap(), &control)
            .unwrap_err()
            .kind,
        HostRefusalKind::InvalidRequest
    );
}

#[test]
fn escaped_utf8_query_obeys_exact_serialized_frame_cap() {
    let root = fixture();
    let escaped = "\\\"\t🙂".repeat(1_000);
    fs::write(root.path().join("src/escaped.txt"), escaped.as_bytes())
        .expect("write escaped UTF-8 fixture");
    let limits = HostLimits {
        max_response_bytes: 4_096,
        max_query_bytes: 32_768,
        ..HostLimits::default()
    };
    let room = open_room(
        root.path(),
        "room-frame-limit",
        HostRights::read_only(),
        limits,
    );
    let control = room.control().expect("room control");
    let request = HostRequest::Query {
        request: QueryRequest::File {
            path: "src/escaped.txt".into(),
            start_line: None,
            end_line: None,
        },
        limits: QueryLimits {
            max_results: 10,
            max_bytes: 32_768,
        },
    };
    assert!(escaped.len() > 4_096);
    let refusal = room
        .dispatch_wire(
            &serde_json::to_vec(&request).expect("wire request"),
            &control,
        )
        .expect_err("complete escaped response must exceed frame cap");
    assert_eq!(refusal.kind, HostRefusalKind::ResponseTooLarge);
}

#[test]
fn room_session_retrieval_is_isolated_and_reset_expires_handles() {
    let root = fixture();
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().unwrap());
    let open = |room_id: &str, scope: &str| {
        HostRoom::open_in_runtime(
            &owner,
            HostRoomGrant::new(
                room_id.to_owned(),
                root.path().to_path_buf(),
                scope.to_owned(),
                HostRights::read_only(),
            )
            .unwrap(),
            HostRoomConfig {
                room_id: room_id.to_owned(),
                limits: HostLimits::default(),
            },
        )
    };
    let room_a = open("room-session-a", "same-admitted-source").unwrap();
    let room_b = open("room-session-b", "same-admitted-source").unwrap();
    assert_eq!(
        open("room-session-foreign-scope", "different-source-scope")
            .err()
            .unwrap()
            .kind,
        HostRefusalKind::Denied
    );
    let control_a = room_a.control().expect("room control");
    let control_b = room_b.control().expect("room control");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) =
            room_a.dispatch(&HostRequest::Status, &control_a).unwrap()
        else {
            panic!("status response")
        };
        if status.phase == HostPhase::Current {
            break;
        }
        assert!(Instant::now() < deadline, "shared source failed to publish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let file = HostRequest::Query {
        request: QueryRequest::File {
            path: "src/lib.rs".into(),
            start_line: None,
            end_line: None,
        },
        limits: QueryLimits::default(),
    };
    let HostResponse::Query(first) = room_a.dispatch(&file, &control_a).expect("served query")
    else {
        panic!("query reply");
    };
    let handle = first.retrieval_handle.expect("session retrieval handle");
    let retrieve = HostRequest::Query {
        request: QueryRequest::Retrieve {
            handle: handle.clone(),
            offset: 0,
        },
        limits: QueryLimits::default(),
    };
    let HostResponse::Query(cached) = room_a
        .dispatch(&retrieve, &control_a)
        .expect("same-room retrieve")
    else {
        panic!("retrieve reply");
    };
    assert!(matches!(cached.output, QueryOutput::RetrievedOutput(_)));
    let foreign = room_b
        .dispatch(&retrieve, &control_b)
        .expect_err("other room cannot retrieve");
    assert!(matches!(
        foreign.query_kind,
        Some(QueryRefusalKind::StaleHandle | QueryRefusalKind::ForeignSession)
    ));
    room_a.reset_query_session().expect("trusted host reset");
    let stale = room_a
        .dispatch(&retrieve, &control_a)
        .expect_err("reset must expire old handle");
    assert_eq!(stale.query_kind, Some(QueryRefusalKind::StaleHandle));
    room_a.close(&control_a).unwrap();
    assert_eq!(
        room_a.dispatch(&file, &control_a).unwrap_err().kind,
        HostRefusalKind::SourceUnavailable
    );
    assert!(matches!(
        room_b.dispatch(&file, &control_b).unwrap(),
        HostResponse::Query(_)
    ));
    room_b.close(&control_b).unwrap();
    assert_eq!(owner.shutdown(&control_b).unwrap().closed_sources, 0);
}

#[test]
fn withheld_file_refusal_carries_only_redacted_shared_metadata() {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    let synthetic = ["S3", "cret", "Value", "9xAb"].concat();
    fs::write(
        root.path().join("config.json"),
        format!("{{\n  \"password\": \"{synthetic}\"\n}}\n"),
    )
    .unwrap();
    let room = open_room(
        root.path(),
        "room-withheld",
        HostRights::read_only(),
        HostLimits::default(),
    );
    let refusal = room
        .dispatch(
            &HostRequest::Query {
                request: QueryRequest::FileContent(FileContentRequest {
                    path: "config.json".into(),
                    ..Default::default()
                }),
                limits: QueryLimits::default(),
            },
            &room.control().unwrap(),
        )
        .expect_err("sensitive source must be withheld");
    let metadata = refusal.withheld.as_ref().expect("shared redacted findings");
    assert_eq!(metadata.path, "config.json");
    assert!(!metadata.findings.is_empty());
    assert!(
        !serde_json::to_string(&refusal)
            .unwrap()
            .contains(&synthetic)
    );
}

#[test]
fn checkpoint_returns_verified_current_proof_and_close_is_broker_controlled() {
    let root = fixture();
    let room = open_room(
        root.path(),
        "room-checkpoint",
        HostRights {
            query: true,
            refresh: true,
            checkpoint: true,
            export_artifact: false,
            ..HostRights::read_only()
        },
        HostLimits::default(),
    );
    let control = room.control().unwrap();
    let HostResponse::Checkpoint(receipt) = room
        .dispatch(
            &HostRequest::Checkpoint {
                verify_after_write: true,
                export_artifact: false,
            },
            &control,
        )
        .unwrap()
    else {
        panic!("checkpoint response")
    };
    assert_eq!(receipt.outcome, HostCheckpointOutcome::WrittenCurrent);
    assert_eq!(receipt.written_files, 1);
    let proof = receipt.proof.expect("checkpoint source proof");
    let HostResponse::Health(health) = room.dispatch(&HostRequest::Health, &control).unwrap()
    else {
        panic!("health response")
    };
    assert_eq!(
        health
            .current
            .unwrap()
            .proof
            .source_capture_publication_identity,
        proof.source_capture_publication_identity
    );
    assert!(!room.close(&control).unwrap().already_terminal);
    assert!(room.close(&control).unwrap().already_terminal);
    assert_eq!(room.shutdown(&control).unwrap().closed_sources, 0);
}

#[test]
fn checkpoint_reopen_uses_snapshot_restore_before_background_verification() {
    let root = fixture();
    let room = open_room(
        root.path(),
        "warm-first",
        HostRights {
            checkpoint: true,
            ..HostRights::read_only()
        },
        HostLimits::default(),
    );
    let control = room.control().unwrap();
    let HostResponse::Checkpoint(receipt) = room
        .dispatch(
            &HostRequest::Checkpoint {
                verify_after_write: true,
                export_artifact: false,
            },
            &control,
        )
        .unwrap()
    else {
        panic!("checkpoint response");
    };
    assert_eq!(receipt.outcome, HostCheckpointOutcome::WrittenCurrent);
    room.close(&control).unwrap();
    room.shutdown(&control).unwrap();
    drop(room);

    let reopened = open_room(
        root.path(),
        "warm-second",
        HostRights::read_only(),
        HostLimits::default(),
    );
    let HostResponse::Health(health) = reopened
        .dispatch(&HostRequest::Health, &reopened.control().unwrap())
        .unwrap()
    else {
        panic!("health response");
    };
    let current = health.current.expect("reopened current source proof");
    assert_eq!(current.load_source, "snapshot_restore");
    assert_eq!(current.files, 1);
    assert_eq!(current.parsed, 1);
}

#[test]
fn room_grant_and_resource_policy_are_checked_before_open_or_write() {
    let root = fixture();
    let grant = HostRoomGrant::new(
        "room-a".into(),
        root.path().to_path_buf(),
        "guest-source".into(),
        HostRights::read_only(),
    )
    .unwrap();
    let mismatch = HostRoom::open(
        ProcessIndexRuntime::acquire().unwrap(),
        grant,
        HostRoomConfig {
            room_id: "room-b".into(),
            limits: HostLimits::default(),
        },
    );
    assert_eq!(mismatch.err().unwrap().kind, HostRefusalKind::Denied);

    let mut limits = HostLimits::default();
    limits.max_indexed_bytes = 1;
    let room = open_room(
        root.path(),
        "room-a",
        HostRights {
            query: true,
            refresh: false,
            checkpoint: true,
            export_artifact: false,
            ..HostRights::read_only()
        },
        limits,
    );
    let control = room.control().unwrap();
    assert_eq!(
        room.dispatch(
            &HostRequest::Checkpoint {
                verify_after_write: true,
                export_artifact: false
            },
            &control,
        )
        .unwrap_err()
        .kind,
        HostRefusalKind::ResourceLimit
    );
}

#[test]
fn cancellation_and_host_deadline_stop_before_dispatch() {
    let root = fixture();
    let room = open_room(
        root.path(),
        "room-cancel",
        HostRights::read_only(),
        HostLimits::default(),
    );
    let cancelled = room.control().unwrap();
    cancelled.cancel();
    assert_eq!(cancelled.check(), Err(OperationStop::Cancelled));
    assert_eq!(
        room.dispatch(&HostRequest::Status, &cancelled)
            .unwrap_err()
            .kind,
        HostRefusalKind::Cancelled
    );

    let expired = OperationControl::new(Duration::from_nanos(1)).unwrap();
    std::thread::sleep(Duration::from_millis(1));
    assert_eq!(
        room.dispatch(&HostRequest::Status, &expired)
            .unwrap_err()
            .kind,
        HostRefusalKind::DeadlineExceeded
    );
}

#[test]
fn shared_runtime_serves_separate_rooms_and_only_owner_can_shutdown() {
    let first_root = fixture();
    let second_root = fixture();
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().unwrap());
    let open = |root: &std::path::Path, room_id: &str| {
        HostRoom::open_in_runtime(
            &owner,
            HostRoomGrant::new(
                room_id.to_owned(),
                root.to_path_buf(),
                format!("{room_id}-guest-source"),
                HostRights::read_only(),
            )
            .unwrap(),
            HostRoomConfig {
                room_id: room_id.to_owned(),
                limits: HostLimits::default(),
            },
        )
        .unwrap()
    };
    let first = open(first_root.path(), "first");
    let second = open(second_root.path(), "second");
    assert_eq!(
        first.engine_identity().instance_identity,
        second.engine_identity().instance_identity
    );
    let control = OperationControl::new(Duration::from_secs(20)).unwrap();
    for room in [&first, &second] {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let HostResponse::Status(status) =
                room.dispatch(&HostRequest::Status, &control).unwrap()
            else {
                panic!("status response")
            };
            if status.phase == HostPhase::Current {
                break;
            }
            assert!(Instant::now() < deadline, "source failed to publish");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert_eq!(
        first.shutdown(&control).unwrap_err().kind,
        HostRefusalKind::Denied
    );
    first.close(&control).unwrap();
    let HostResponse::Status(second_status) =
        second.dispatch(&HostRequest::Status, &control).unwrap()
    else {
        panic!("second room status response")
    };
    assert_eq!(second_status.phase, HostPhase::Current);
    assert_eq!(owner.shutdown(&control).unwrap().closed_sources, 1);
    assert_eq!(
        HostRoom::open_in_runtime(
            &owner,
            HostRoomGrant::new(
                "third".into(),
                first_root.path().to_path_buf(),
                "third-guest-source".into(),
                HostRights::read_only(),
            )
            .unwrap(),
            HostRoomConfig {
                room_id: "third".into(),
                limits: HostLimits::default(),
            },
        )
        .err()
        .unwrap()
        .kind,
        HostRefusalKind::Denied
    );
}
