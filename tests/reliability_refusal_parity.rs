//! Feature 036 T021, embed refusal text parity: a call that carries an argument its
//! tool does not define is refused by the embed lane with the same text, byte for byte,
//! that the MCP lane prints for the same tool and key.
//!
//! The MCP side drives the production `tools/call` path through a real stdio server; the
//! embed side sends the wire form through `HostRoom::dispatch_wire`. Every refusal carries
//! an accepting control: the same call without the bad key succeeds on both lanes. The
//! refusal text is read from the serialized `HostRefusal` so the assertions name the
//! wire field a host forwards.
#![cfg(all(feature = "server", feature = "embed"))]

mod reliability_support;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use symforge::embed::ProcessIndexRuntime;
use symforge::embed::parity::host::{
    HostLimits, HostPhase, HostRefusal, HostRefusalKind, HostRequest, HostResponse, HostRights,
    HostRoom, HostRoomConfig, HostRoomGrant, HostRuntimeOwner, OperationControl,
};
use symforge::embed::parity::source_options::{EmbeddedOpenOptions, ProjectConfigTrustMode};
use symforge::protocol::SymForgeServer;
use tempfile::TempDir;

const BAD_KEY: &str = "zz_unknown";
const TRUST_MODE_ENV: &str = "SYMFORGE_PROJECT_CONFIG_TRUST_MODE";

struct StdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
    _home: TempDir,
}

impl StdioClient {
    /// `state_home` overrides the isolated `SYMFORGE_HOME` (the trust store lives there).
    fn spawn(workspace: &Path, env: &[(&str, &str)], state_home: Option<&Path>) -> Self {
        let home = TempDir::new().expect("isolated symforge home");
        let mut command = symforge::process_util::hidden_command(env!("CARGO_BIN_EXE_symforge"));
        command
            .current_dir(workspace)
            .env("RUST_LOG", "error")
            .env("SYMFORGE_NO_DAEMON", "1")
            .env("SYMFORGE_RECONCILE_INTERVAL", "0")
            .env("SYMFORGE_HOME", state_home.unwrap_or(home.path()))
            .env("SYMFORGE_SURFACE", "full")
            .env("SYMFORGE_WORKSPACE_ROOT", workspace)
            .env_remove(symforge::edit_safety::trust::TRUST_ENV_OVERRIDE)
            .env_remove(TRUST_MODE_ENV);
        for var in symforge::edit_safety::trust::CI_ENV_VARS {
            command.env_remove(var);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn symforge MCP server");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let mut client = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            _home: home,
        };
        let id = client.send(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "refusal-parity", "version": "0.0.0" }
            }),
        );
        let _ = client.read_until_response(id);
        client.write_message(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }));
        client
    }

    fn send(&mut self, method: &str, params: Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }));
        id
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.send(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let response = self.read_until_response(id);
        if let Some(error) = response.get("error") {
            panic!("tool {name} returned a JSON-RPC error instead of a result: {error}");
        }
        response.get("result").expect("tool result").clone()
    }

    fn write_message(&mut self, message: Value) {
        serde_json::to_writer(&mut self.stdin, &message).expect("write message");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush");
    }

    fn read_until_response(&mut self, expected_id: u64) -> Value {
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(90),
                "timed out waiting for response {expected_id}"
            );
            let mut line = String::new();
            let bytes = self.stdout.read_line(&mut line).expect("read line");
            assert_ne!(bytes, 0, "server closed stdout before responding");
            let message: Value = serde_json::from_str(line.trim())
                .unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"));
            if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
                return message;
            }
        }
    }
}

impl Drop for StdioClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text_of(result: &Value) -> String {
    result["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn mcp_ready(root: &Path) -> StdioClient {
    let mut client = StdioClient::spawn(root, &[], None);
    let started = Instant::now();
    loop {
        let reply = client.call("search_symbols", json!({ "query": "target_fn" }));
        if text_of(&reply).contains("target_fn") {
            return client;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "index never served the fixture: {}",
            text_of(&reply)
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn all_rights() -> HostRights {
    HostRights {
        query: true,
        refresh: true,
        checkpoint: true,
        export_artifact: true,
        edit: true,
        curate_knowledge: true,
        inspect_secrets: true,
        remediate_secrets: true,
        derived_state_prepare: true,
    }
}

fn room_in(owner: &HostRuntimeOwner, root: &Path, id: &str) -> HostRoom {
    let room = HostRoom::open_in_runtime(
        owner,
        HostRoomGrant::new(
            id.to_owned(),
            root.to_path_buf(),
            format!("{id}-scope"),
            all_rights(),
        )
        .expect("room grant"),
        HostRoomConfig {
            room_id: id.to_owned(),
            limits: HostLimits {
                max_response_bytes: 2_097_152,
                max_query_bytes: 1_048_576,
                ..HostLimits::default()
            },
        },
    )
    .expect("bound room");
    let control = room.control().expect("room control");
    let deadline = Instant::now() + Duration::from_secs(30);
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

fn control_for(room: &HostRoom) -> OperationControl {
    room.control().expect("room control")
}

fn wire(room: &HostRoom, request: &Value) -> Result<Vec<u8>, HostRefusal> {
    room.dispatch_wire(
        &serde_json::to_vec(request).expect("wire bytes"),
        &control_for(room),
    )
}

/// The refusal text a host forwards to its LLM: the serialized `message` field.
fn message_of(refusal: &HostRefusal) -> Option<String> {
    serde_json::to_value(refusal).expect("refusal wire")["message"]
        .as_str()
        .map(str::to_owned)
}

fn with_bad_key(args: &Value) -> Value {
    let mut bad = args.clone();
    bad.as_object_mut()
        .expect("arguments are an object")
        .insert(BAD_KEY.to_string(), json!(1));
    bad
}

const QUERY_LIMITS: fn() -> Value = || json!({ "max_results": 100, "max_bytes": 1 << 20 });

/// `QueryRequest` variant of each tool whose embed request decodes the MCP arguments.
fn query_variant(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "search_text" => "TextSearch",
        "search_symbols" => "SymbolSearch",
        "search_files" => "FileSearch",
        "search_knowledge" => "SearchKnowledge",
        "get_file_content" => "FileContent",
        "get_repo_map" => "RepoMap",
        "get_file_context" => "FileContext",
        "get_symbol" => "SymbolRead",
        "get_symbol_context" => "SymbolContext",
        "inspect_match" => "InspectMatch",
        "find_references" => "ReferenceSearch",
        "find_dependents" => "DependentSearch",
        "analyze_file_impact" => "FileImpact",
        "what_changed" => "WhatChanged",
        "diff_symbols" => "DiffSymbols",
        "detect_impact" => "DetectImpact",
        "ask" => "Ask",
        "symforge" => "Symforge",
        "explore" => "Explore",
        "validate_file_syntax" => "Syntax",
        "investigation_suggest" => "InvestigationSuggest",
        "edit_plan" => "EditPlan",
        _ => return None,
    })
}

/// The embed wire request carrying `args` as `tool`'s arguments, or `None` when the tool's
/// embed request has a different shape.
fn embed_wire(tool: &str, args: &Value) -> Option<Value> {
    if let Some(variant) = query_variant(tool) {
        return Some(json!({ "Query": {
            "request": { variant: args },
            "limits": QUERY_LIMITS(),
        }}));
    }
    let unit_query = |variant: &str| {
        let request = if args.as_object().is_some_and(|o| o.is_empty()) {
            json!(variant)
        } else {
            json!({ variant: args })
        };
        json!({ "Query": { "request": request, "limits": QUERY_LIMITS() } })
    };
    Some(match tool {
        "conventions" => unit_query("Conventions"),
        "context_inventory" => unit_query("ContextInventory"),
        "status" => json!({ "StatusReport": args }),
        "checkpoint_now" => json!({ "Checkpoint": args }),
        "health" => json!({ "HealthReport": args }),
        "symforge_edit" => json!({ "SymforgeEdit": args }),
        "health_compact" => {
            if args.as_object().is_some_and(|o| o.is_empty()) {
                json!("HealthCompact")
            } else {
                json!({ "HealthCompact": args })
            }
        }
        "review_knowledge" => json!({ "Knowledge": {
            "request": { "kind": "review", "input": args },
            "operation_key": null,
        }}),
        _ => return None,
    })
}

/// Typical calls that need no earlier output, with `{root}` resolved.
fn concrete_typical_calls(root: &str) -> Vec<(&'static str, Value)> {
    SymForgeServer::typical_calls_for_tests()
        .into_iter()
        .filter_map(|(tool, typical)| {
            let resolved = typical.replace("{root}", &root.replace('\\', "\\\\"));
            (!resolved.contains("\"{")).then(|| {
                (
                    tool,
                    serde_json::from_str(&resolved).expect("typical call is JSON"),
                )
            })
        })
        .collect()
}

fn assert_mcp_not_refused(result: &Value, label: &str) {
    let text = text_of(result);
    assert!(
        !text.contains("does not accept"),
        "{label}: the MCP control was refused for its arguments: {text}"
    );
}

/// Every tool that exists on both lanes with the same arguments: the same call with one
/// unknown top-level key is refused with byte-identical text; without it, it succeeds.
#[test]
fn an_unknown_top_level_key_is_refused_with_the_mcp_text_on_the_embed_lane() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "parity-room");

    let mut failures = Vec::new();
    let mut compared = Vec::new();
    let mut calls = concrete_typical_calls(&root.to_string_lossy());
    // A checkpoint writes state; it goes last so earlier controls see one index.
    calls.sort_by_key(|(tool, _)| *tool == "checkpoint_now");
    for (tool, mut args) in calls {
        if tool == "explore" {
            // The embed `ExploreRequest` requires these; MCP accepts them.
            args["depth"] = json!(1);
            args["limit"] = json!(10);
            args["include_noise"] = json!(false);
            args["include_vendor"] = json!(false);
            args["include_personal_tooling"] = json!(false);
        }
        if tool == "checkpoint_now" {
            // The embed `Checkpoint` request requires both; MCP accepts them.
            args["verify_after_write"] = json!(false);
            args["export_artifact"] = json!(false);
        }
        let Some(control_wire) = embed_wire(tool, &args) else {
            continue;
        };
        let control = mcp.call(tool, args.clone());
        assert_mcp_not_refused(&control, &format!("{tool} control"));
        if let Err(refusal) = wire(&room, &control_wire) {
            failures.push(format!(
                "{tool}: embed control refused: {refusal:?} for {control_wire}"
            ));
            continue;
        }
        let bad = with_bad_key(&args);
        let mcp_text = text_of(&mcp.call(tool, bad.clone()));
        let embed_wire_bad = embed_wire(tool, &bad).expect("same shape");
        match wire(&room, &embed_wire_bad) {
            Ok(_) => failures.push(format!("{tool}: embed accepted the unknown key")),
            Err(refusal) => {
                if refusal.kind != HostRefusalKind::InvalidRequest {
                    failures.push(format!("{tool}: refusal kind {:?}", refusal.kind));
                }
                let embed_text = message_of(&refusal);
                if embed_text.as_deref() != Some(mcp_text.as_str()) {
                    failures.push(format!(
                        "{tool}:\n  mcp:   {mcp_text}\n  embed: {embed_text:?}"
                    ));
                }
            }
        }
        compared.push(tool);
    }
    assert!(
        compared.len() >= 20,
        "too few tools compared: {compared:?}\n{}",
        failures.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A key the MCP tool accepts but the embed request does not (`project` on the embed
/// text search) is refused under the MCP tool's name, listing the embed request's fields:
/// the MCP list would name the refused key as valid. The MCP lane accepts the same call.
#[test]
fn a_key_only_the_mcp_tool_accepts_names_the_embed_arguments() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "one-side-room");

    let args = json!({ "query": "target_fn" });
    wire(&room, &embed_wire("search_text", &args).unwrap()).expect("embed control");
    let mut with_project = args;
    with_project["project"] = json!("x");
    assert_mcp_not_refused(
        &mcp.call("search_text", with_project.clone()),
        "search_text",
    );
    let refusal = wire(&room, &embed_wire("search_text", &with_project).unwrap())
        .expect_err("embed text search has no project argument");
    let text = message_of(&refusal).expect("refusal text");
    assert!(
        text.starts_with(
            "invalid_request: search_text does not accept \"project\". Valid arguments: query, "
        ),
        "{text}"
    );
    assert!(
        !text.contains("project,") && !text.contains(", project"),
        "{text}"
    );
}

/// A nested unknown key keeps its path and hint, identically on both lanes; the embed
/// batch edit is a different request (`actions` of source guards, no MCP tool takes it)
/// and is named by its label with its own arguments.
#[test]
fn a_nested_unknown_key_keeps_its_path_and_hint_on_the_embed_lane() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "nested-room");

    let targets = json!({ "targets": [{ "path": "src/long.rs", "name": "long_body" }] });
    assert_mcp_not_refused(&mcp.call("get_symbol", targets.clone()), "get_symbol");
    wire(&room, &embed_wire("get_symbol", &targets).unwrap()).expect("embed get_symbol control");
    let mut bad = targets;
    bad["targets"][0][BAD_KEY] = json!(1);
    let mcp_text = text_of(&mcp.call("get_symbol", bad.clone()));
    assert!(mcp_text.contains("targets.0.zz_unknown"), "{mcp_text}");
    let refusal = wire(&room, &embed_wire("get_symbol", &bad).unwrap()).expect_err("nested key");
    assert_eq!(message_of(&refusal).as_deref(), Some(mcp_text.as_str()));

    let HostResponse::Edit(plan) = room
        .dispatch(
            &serde_json::from_value(json!({ "Edit": {
                "request": { "kind": "plan", "target": {
                    "path": "src/short.rs", "name": "SHORT_A", "kind": null, "symbol_line": null
                }},
                "operation_key": null,
            }}))
            .expect("plan request"),
            &control_for(&room),
        )
        .expect("plan")
    else {
        panic!("plan reply");
    };
    let action = json!({
        "kind": "replace",
        "guard": plan["result"]["guard"],
        "new_body": "pub const SHORT_A: u32 = 1;",
    });
    let batch = |action: Value| {
        json!({ "Edit": {
            "request": { "kind": "preview_batch_edit", "actions": [action] },
            "operation_key": null,
        }})
    };
    wire(&room, &batch(action.clone())).expect("embed batch edit preview control");
    let mut bad_action = action;
    bad_action[BAD_KEY] = json!(1);
    let refusal = wire(&room, &batch(bad_action)).expect_err("nested batch key");
    assert_eq!(
        message_of(&refusal).as_deref(),
        Some(
            "invalid_request: Edit.preview_batch_edit does not accept \"actions.0.zz_unknown\". \
             Valid arguments: actions. Check the fields of \"actions\" against the tool's \
             input schema."
        )
    );
    assert!(!serde_json::to_string(&refusal).unwrap().contains("SHORT_A"));
}

/// The facade, `status` and `symforge_edit` requests used to drop unknown keys on embed.
#[test]
fn facade_status_and_compact_edit_refuse_unknown_keys_like_mcp() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "facade-room");

    let cases = [
        ("status", json!({})),
        ("symforge", json!({ "query": "where is target_fn" })),
        (
            "symforge_edit",
            json!({
                "path": "src/short.rs",
                "symbol": "SHORT_A",
                "body": "pub const SHORT_A: u32 = 1;"
            }),
        ),
    ];
    for (tool, args) in cases {
        assert_mcp_not_refused(&mcp.call(tool, args.clone()), tool);
        wire(&room, &embed_wire(tool, &args).unwrap())
            .unwrap_or_else(|refusal| panic!("{tool}: embed control refused: {refusal:?}"));
        let bad = with_bad_key(&args);
        let mcp_text = text_of(&mcp.call(tool, bad.clone()));
        assert!(
            mcp_text.contains("does not accept \"zz_unknown\""),
            "{mcp_text}"
        );
        let refusal = wire(&room, &embed_wire(tool, &bad).unwrap())
            .err()
            .unwrap_or_else(|| panic!("{tool}: embed accepted the unknown key"));
        assert_eq!(refusal.kind, HostRefusalKind::InvalidRequest, "{tool}");
        assert_eq!(
            message_of(&refusal).as_deref(),
            Some(mcp_text.as_str()),
            "{tool}"
        );
    }
}

/// An enforced project-config trust refusal carries MCP's text on the embed lane.
#[test]
fn an_enforced_trust_refusal_carries_the_mcp_text() {
    let dir = reliability_support::committed_fixture_copy();
    std::fs::create_dir_all(dir.path().join(".symforge")).expect("config dir");
    std::fs::write(dir.path().join(".symforge/config.toml"), "[index]\n").expect("config");
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    // One trust store for both lanes, so the refusal names the same store path: the
    // embed lane keeps it under `<control>/embed-host`, MCP under `SYMFORGE_HOME`.
    let control_dir = TempDir::new().expect("host control directory");
    let control = dunce::canonicalize(control_dir.path()).expect("canonical control directory");
    let state_home = control.join("embed-host");
    std::fs::create_dir_all(&state_home).expect("state home");
    // The embed lane reads the trust overrides from this process's environment.
    // SAFETY: the variables are removed before any thread of this test reads them;
    // nextest runs each test in its own process and the cargo gate uses one thread.
    #[allow(unsafe_code)]
    unsafe {
        std::env::remove_var(symforge::edit_safety::trust::TRUST_ENV_OVERRIDE);
        for var in symforge::edit_safety::trust::CI_ENV_VARS {
            std::env::remove_var(var);
        }
    }

    let mut mcp = StdioClient::spawn(&root, &[(TRUST_MODE_ENV, "enforce")], Some(&state_home));
    let started = Instant::now();
    while !text_of(&mcp.call("search_symbols", json!({ "query": "target_fn" })))
        .contains("target_fn")
    {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "index never served"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    let owner = HostRuntimeOwner::new_with_options(
        ProcessIndexRuntime::acquire().expect("runtime"),
        EmbeddedOpenOptions {
            project_config_trust_mode: ProjectConfigTrustMode::Enforce,
            replay_control_directory: Some(control.clone()),
            ..Default::default()
        },
    );
    let room = room_in(&owner, &root, "trust-room");

    let mcp_text = text_of(&mcp.call(
        "replace_symbol_body",
        json!({
            "path": "src/short.rs",
            "name": "SHORT_A",
            "new_body": "pub const SHORT_A: u32 = 1;"
        }),
    ));
    assert!(
        mcp_text.starts_with("ProjectConfigTrustEnforced: "),
        "{mcp_text}"
    );

    let HostResponse::Edit(plan) = room
        .dispatch(
            &serde_json::from_value(json!({ "Edit": {
                "request": { "kind": "plan", "target": {
                    "path": "src/short.rs", "name": "SHORT_A", "kind": null, "symbol_line": null
                }},
                "operation_key": null,
            }}))
            .expect("plan request"),
            &control_for(&room),
        )
        .expect("plan")
    else {
        panic!("plan reply");
    };
    let refusal = wire(
        &room,
        &json!({ "Edit": {
            "request": {
                "kind": "apply_replace",
                "guard": plan["result"]["guard"],
                "new_body": "pub const SHORT_A: u32 = 1;",
            },
            "operation_key": "trust-parity-1",
        }}),
    )
    .expect_err("enforced trust refuses the edit");
    assert_eq!(refusal.edit_kind.as_deref(), Some("EditRefused"));
    assert_eq!(message_of(&refusal).as_deref(), Some(mcp_text.as_str()));
}
