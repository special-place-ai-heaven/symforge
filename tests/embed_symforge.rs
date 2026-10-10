#![cfg(feature = "embed")]
//! MCP `symforge` and `symforge_edit` parity: the compact facade over an
//! embedded source routes read, search and edit intents through the shared
//! planner and economics to the native lanes. The server lib test
//! `symforge_facade_matches_embed_parity_golden` in `src/protocol/tools.rs`
//! asserts the same goldens against MCP output.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::embed::parity::stel::{StelRequest, SymforgeAnswer};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryPolicy, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source never reached Current");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

fn facade(
    handle: &EmbeddedSourceHandle,
    session: &symforge::embed::parity::session::QuerySession,
    request: StelRequest,
) -> SymforgeAnswer {
    let claim = handle
        .query_with_policy(
            &QueryRequest::Symforge(request),
            QueryLimits::default(),
            QueryPolicy {
                allow_derived_state_preparation: true,
            },
            Some(session),
            None,
        )
        .unwrap();
    match claim.value() {
        QueryOutput::Symforge(answer) => answer.clone(),
        other => panic!("unexpected output: {other:?}"),
    }
}

/// Every case of the golden MCP's `symforge_facade_matches_embed_parity_golden`
/// also asserts: the same text, with the bound root written as `{root}`, and
/// the same outcome class.
#[test]
fn facade_answers_match_the_mcp_golden() {
    let fixture: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stel_facade/parity.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        fixture["lib_rs"].as_str().unwrap(),
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let root_text = dunce::canonicalize(root.path())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    for case in fixture["cases"].as_array().unwrap() {
        let request: StelRequest = serde_json::from_value(case["request"].clone()).unwrap();
        let answer = facade(&handle, &session, request);
        assert_eq!(
            answer.rendered.replace(&root_text, "{root}"),
            case["rendered"].as_str().unwrap(),
            "{}",
            case["request"]
        );
        assert_eq!(
            serde_json::to_value(answer.outcome).unwrap(),
            case["outcome"],
            "{}",
            case["request"]
        );
    }
}

/// The primitive tool's answer inside a `symforge_edit` reply: the text after
/// the routing summary up to the trust envelope's closing rule, with the
/// timestamped tee snapshot path compared as `<tee>`.
fn edit_body_section(text: &str, routing: Option<&str>) -> Option<String> {
    let routing = routing?;
    let start = text.find(routing)? + routing.len();
    let rest = text[start..].trim_start_matches('\n');
    let rest = &rest[..rest.find("\n──").unwrap_or(rest.len())];
    Some(
        rest.lines()
            .map(|line| {
                let trimmed = line.trim_start();
                let indent = &line[..line.len() - trimmed.len()];
                match trimmed
                    .strip_prefix("Tee snapshot: `")
                    .and_then(|tail| tail.split_once('`'))
                {
                    Some((_, tail)) => format!("{indent}Tee snapshot: `<tee>`{tail}"),
                    None => line.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// The `symforge_edit` observation MCP's `symforge_edit_matches_embed_parity_golden`
/// records: the routing summary, the text of an answer with no trust
/// envelope, the outcome, the error flag and the file after.
fn edit_observation(
    answer: &symforge::embed::parity::stel::SymforgeEditAnswer,
    file: &str,
) -> serde_json::Value {
    let text = answer.rendered.as_str();
    let routing = text.find("Mode: ").map(|start| {
        let tail = &text[start..];
        let end = tail
            .find("\nEconomics: ")
            .and_then(|economics| {
                tail[economics + 1..]
                    .find('\n')
                    .map(|line| economics + 1 + line)
            })
            .unwrap_or(tail.len());
        tail[..end].to_string()
    });
    let body = edit_body_section(text, routing.as_deref());
    serde_json::json!({
        "outcome": answer.outcome,
        "is_error": answer.is_error,
        "routing": routing,
        "body": body,
        // A key conflict names each store's own request hashes; only the
        // class of refusal is shared.
        "plain_text": (!text.starts_with("──")).then(|| {
            if text.starts_with("Idempotency conflict:") {
                "Idempotency conflict:".to_string()
            } else {
                text.to_string()
            }
        }),
        "file": file,
    })
}

/// Every `symforge_edit` case of the shared golden: the same routing summary,
/// outcome, error flag and resulting bytes as MCP, including the idempotent
/// replay that writes nothing and the refused replay reporting an error once
/// the file moved past the recorded post-image (the MCP fix of 2d733398).
#[test]
fn symforge_edit_answers_match_the_mcp_golden() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use symforge::embed::parity::edit::EditApplyAuthority;
    use symforge::embed::parity::stel::{StelEditRequest, SymforgeEditAuthority};

    let fixture: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/stel_facade/edit_parity.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    let file_path = root.path().join("src/lib.rs");
    fs::write(&file_path, fixture["lib_rs"].as_str().unwrap()).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(root.path()).unwrap(),
        "edit-parity".into(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        if let Some(tamper) = case["tamper"].as_str() {
            fs::write(&file_path, tamper).unwrap();
        }
        let request: StelEditRequest = serde_json::from_value(case["request"].clone()).unwrap();
        // Writes are published by the worker; wait until the source is
        // Current again so every case reads the bytes the last one left.
        let until = Instant::now() + Duration::from_secs(20);
        let answer = loop {
            match handle.symforge_edit(
                &request,
                Some(&session),
                QueryPolicy {
                    allow_derived_state_preparation: true,
                },
                Some(SymforgeEditAuthority {
                    authority: &authority,
                    admitted: &[],
                }),
            ) {
                Ok(answer) => break answer,
                Err(_) => {
                    assert!(Instant::now() < until, "source never returned to Current");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        };
        assert!(
            !answer.rendered.contains("Idempotency warning"),
            "{}",
            answer.rendered
        );
        let mut observed = edit_observation(&answer, &fs::read_to_string(&file_path).unwrap());
        if case["replay"].as_bool() == Some(true) {
            assert!(answer.replayed, "{}", answer.rendered);
            observed["routing"] = serde_json::Value::Null;
            observed["body"] = serde_json::Value::Null;
            observed["plain_text"] = serde_json::Value::Null;
        }
        for key in [
            "outcome",
            "is_error",
            "routing",
            "body",
            "plain_text",
            "file",
        ] {
            assert_eq!(
                observed[key], case[key],
                "{} / {key}:\n{}",
                case["name"], answer.rendered
            );
        }
    }
}

/// A host room serves the facade as a query and `symforge_edit` as its own
/// request: a read-only room previews but may not apply, and lists both.
#[test]
fn host_room_serves_the_facade_and_gates_symforge_edit_apply() {
    use symforge::embed::parity::host::{
        HostLimits, HostRefusalKind, HostRequest, HostResponse, HostRights, HostRoom,
        HostRoomConfig, HostRoomGrant,
    };
    use symforge::embed::parity::stel::{OutcomeClass, StelEditRequest};

    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn target() -> u32 {\n    1\n}\n",
    )
    .unwrap();
    let grant = HostRoomGrant::new(
        "facade-room".into(),
        fs::canonicalize(root.path()).unwrap(),
        "facade-scope".into(),
        HostRights::read_only(),
    )
    .unwrap();
    let room = HostRoom::open(
        ProcessIndexRuntime::acquire().unwrap(),
        grant,
        HostRoomConfig {
            room_id: "facade-room".into(),
            limits: HostLimits::default(),
        },
    )
    .unwrap();
    let control = room.control().unwrap();
    let catalog = room.catalog();
    assert!(catalog.query_operations.iter().any(|op| op == "Symforge"));

    let until = Instant::now() + Duration::from_secs(20);
    let preview = StelEditRequest {
        path: "src/lib.rs".into(),
        symbol: Some("target".into()),
        body: Some("pub fn target() -> u32 {\n    2\n}".into()),
        ..Default::default()
    };
    let answer = loop {
        match room.dispatch(&HostRequest::SymforgeEdit(preview.clone()), &control) {
            Ok(HostResponse::SymforgeEdit(answer)) => break answer,
            Ok(other) => panic!("unexpected response: {other:?}"),
            Err(_) => {
                assert!(Instant::now() < until, "room never served the preview");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    };
    assert_eq!(answer.outcome, OutcomeClass::Found, "{}", answer.rendered);
    assert!(answer.rendered.contains("Mode: preview-only (dry_run)"));

    let facade = room
        .dispatch(
            &HostRequest::Query {
                request: QueryRequest::Symforge(StelRequest {
                    query: "who calls target".into(),
                    ..Default::default()
                }),
                limits: QueryLimits::default(),
            },
            &control,
        )
        .unwrap();
    let HostResponse::Query(_) = facade else {
        panic!("facade query reply")
    };

    let refused = room
        .dispatch(
            &HostRequest::SymforgeEdit(StelEditRequest {
                apply: Some(true),
                ..preview
            }),
            &control,
        )
        .unwrap_err();
    assert_eq!(refused.kind, HostRefusalKind::Denied);
    assert_eq!(
        fs::read_to_string(root.path().join("src/lib.rs")).unwrap(),
        "pub fn target() -> u32 {\n    1\n}\n"
    );
}
