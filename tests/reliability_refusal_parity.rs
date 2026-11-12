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

/// Tools whose embed request declares exactly the MCP tool's arguments: refusals are
/// byte-identical. (`checkpoint_now` too; it runs last.)
const SAME_ARGUMENT_SET: &[&str] = &[
    "health",
    "health_compact",
    "get_repo_map",
    "get_file_context",
    "get_symbol",
    "get_symbol_context",
    "search_knowledge",
    "review_knowledge",
    "find_references",
    "find_dependents",
    "inspect_match",
    "what_changed",
    "diff_symbols",
    "ask",
    "conventions",
    "context_inventory",
    "status",
    "symforge",
    "symforge_edit",
];

/// Tools whose embed request declares other arguments than the MCP tool, and the embed
/// request's fields in declaration order, which its refusal lists.
const DIFFERENT_ARGUMENT_SET: &[(&str, &str)] = &[
    (
        "search_text",
        "query, terms, regex, structural, path_prefix, language, limit, max_per_file, \
         include_generated, include_tests, include_vendor, include_personal_tooling, glob, \
         exclude_glob, context, case_sensitive, whole_word, group_by, include_body, \
         follow_refs, follow_refs_limit, ranked, estimate, max_tokens",
    ),
    (
        "search_symbols",
        "query, kind, path_prefix, language, limit, include_generated, include_tests, \
         include_vendor, include_personal_tooling, estimate, max_tokens",
    ),
    (
        "search_files",
        "query, limit, current_file, changed_with, resolve, estimate, max_tokens, \
         debug_ranking, rank_by, anchor_path, include_vendor, include_personal_tooling, \
         path_prefix",
    ),
    (
        "get_file_content",
        "path, mode, start_line, end_line, chunk_index, max_lines, around_line, around_match, \
         match_occurrence, around_symbol, symbol_line, context_lines, show_line_numbers, \
         header, estimate, offset, limit, max_tokens, force_refresh",
    ),
    ("validate_file_syntax", "path"),
    (
        "analyze_file_impact",
        "path, new_file, include_co_changes, co_changes_limit, estimate",
    ),
    (
        "explore",
        "query, depth, limit, include_noise, include_vendor, include_personal_tooling, \
         language, path_prefix, estimate, max_tokens",
    ),
    ("investigation_suggest", "focus"),
    ("edit_plan", "target"),
    (
        "detect_impact",
        "base_branch, since, depth, scope, include_untracked, include_data, paths",
    ),
];

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

/// Every tool that exists on both lanes: the same call with one unknown top-level key is
/// refused, and without it succeeds, on both lanes. Where the embed request declares the
/// MCP tool's argument set the text is MCP's byte for byte; where it does not
/// (`field_set_differences_are_pinned`), the list is the embed request's own fields in
/// declaration order, so an LLM following it is not refused again.
#[test]
fn an_unknown_top_level_key_is_refused_with_the_mcp_text_on_the_embed_lane() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "parity-room");

    let mut calls: std::collections::BTreeMap<&str, Value> =
        concrete_typical_calls(&root.to_string_lossy())
            .into_iter()
            .collect();
    // The embed `ExploreRequest` and `Checkpoint` require these; MCP accepts them.
    for (key, value) in [
        ("depth", json!(1)),
        ("limit", json!(10)),
        ("include_noise", json!(false)),
        ("include_vendor", json!(false)),
        ("include_personal_tooling", json!(false)),
    ] {
        calls.get_mut("explore").expect("explore call")[key] = value;
    }
    for key in ["verify_after_write", "export_artifact"] {
        calls.get_mut("checkpoint_now").expect("checkpoint call")[key] = json!(false);
    }
    // A checkpoint writes state; it goes last so earlier controls see one index.
    let cases = SAME_ARGUMENT_SET
        .iter()
        .map(|tool| (*tool, None))
        .chain(
            DIFFERENT_ARGUMENT_SET
                .iter()
                .map(|(tool, fields)| (*tool, Some(*fields))),
        )
        .chain([("checkpoint_now", None)]);

    let mut failures = Vec::new();
    for (tool, embed_fields) in cases {
        let args = calls
            .get(tool)
            .unwrap_or_else(|| panic!("{tool}: no concrete typical call"))
            .clone();
        let control_wire = embed_wire(tool, &args).unwrap_or_else(|| panic!("{tool}: no wire"));
        assert_mcp_not_refused(&mcp.call(tool, args.clone()), &format!("{tool} control"));
        if let Err(refusal) = wire(&room, &control_wire) {
            failures.push(format!("{tool}: embed control refused: {refusal:?}"));
            continue;
        }
        let bad = with_bad_key(&args);
        let mcp_text = text_of(&mcp.call(tool, bad.clone()));
        let expected = match embed_fields {
            None => mcp_text.clone(),
            Some(fields) => format!(
                "invalid_request: {tool} does not accept \"zz_unknown\". Valid arguments: \
                 {fields}."
            ),
        };
        match wire(&room, &embed_wire(tool, &bad).expect("same shape")) {
            Ok(_) => failures.push(format!("{tool}: embed accepted the unknown key")),
            Err(refusal) => {
                if refusal.kind != HostRefusalKind::InvalidRequest {
                    failures.push(format!("{tool}: refusal kind {:?}", refusal.kind));
                }
                let embed_text = message_of(&refusal);
                if embed_text.as_deref() != Some(expected.as_str()) {
                    failures.push(format!(
                        "{tool}:\n  mcp:      {mcp_text}\n  expected: {expected}\n  embed:    \
                         {embed_text:?}"
                    ));
                }
            }
        }
    }
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

/// The texts both lanes give for one call: MCP's result text and the embed refusal text.
fn both(
    mcp: &mut StdioClient,
    room: &HostRoom,
    tool: &str,
    args: &Value,
) -> (String, Option<String>) {
    let mcp_text = text_of(&mcp.call(tool, args.clone()));
    let embed = wire(room, &embed_wire(tool, args).expect("wire"))
        .err()
        .and_then(|refusal| message_of(&refusal));
    (mcp_text, embed)
}

/// `name=value` from a fixed-grammar answer.
fn token(text: &str, name: &str) -> String {
    text.split_whitespace()
        .find_map(|field| field.strip_prefix(&format!("{name}=")))
        .map(|value| value.trim_end_matches([',', ']']).to_string())
        .unwrap_or_else(|| panic!("missing {name}: {text}"))
}

/// The first string member named `key` anywhere in `value`.
fn find_string(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| map.values().find_map(|inner| find_string(inner, key))),
        Value::Array(items) => items.iter().find_map(|inner| find_string(inner, key)),
        _ => None,
    }
}

/// Several unknown keys, in one element or beside it, a long key and a non-ASCII key:
/// both lanes name every key, in the same order, with the same bytes.
#[test]
fn several_unknown_keys_are_named_alike_on_both_lanes() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "several-room");

    let long_key = "k".repeat(50);
    let long_named = format!("\"{}...\"", "k".repeat(40));
    let review = json!({ "mode": "remediation", "source_scope": "current", "limit": 20,
        "zz_one": 1, "zz_two": 2 });
    let cases: Vec<(&str, Value, Vec<&str>)> = vec![
        ("review_knowledge", review, vec!["\"zz_one\"", "\"zz_two\""]),
        (
            "get_symbol",
            json!({ "targets": [{ "path": "src/long.rs", "name": "long_body",
                "zz_a": 1, "zz_b": 2 }] }),
            vec!["\"targets.0.zz_a\"", "\"targets.0.zz_b\""],
        ),
        (
            "get_symbol",
            json!({ "targets": [{ "path": "src/long.rs", "name": "long_body", "zz_a": 1 }],
                "zzz": 1 }),
            vec!["\"targets.0.zz_a\"", "\"zzz\""],
        ),
        (
            "get_repo_map",
            json!({ "zz_one": 1, "zz_two": 2 }),
            vec!["\"zz_one\", \"zz_two\""],
        ),
        (
            "get_repo_map",
            json!({ long_key.as_str(): 1 }),
            vec![long_named.as_str()],
        ),
        ("get_repo_map", json!({ "ключ_ü": 1 }), vec!["\"ключ_ü\""]),
    ];
    let mut failures = Vec::new();
    for (tool, args, named) in cases {
        let (mcp_text, embed) = both(&mut mcp, &room, tool, &args);
        if embed.as_deref() != Some(mcp_text.as_str()) {
            failures.push(format!(
                "{tool} {args}:\n  mcp:   {mcp_text}\n  embed: {embed:?}"
            ));
        }
        for key in named {
            if !mcp_text.contains(key) {
                failures.push(format!("{tool} {args}: MCP text lacks {key}: {mcp_text}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A key inside a nested request object is named by its path with the parent hint, never
/// as a top-level argument; its value is never echoed.
#[test]
fn a_key_inside_a_nested_request_object_is_named_by_its_path() {
    let root = tempfile::tempdir().expect("repository");
    git2::Repository::init(root.path()).expect("git repository");
    std::fs::write(
        root.path().join("lib.rs"),
        b"pub fn target() -> u32 { 1 }\n",
    )
    .expect("source");
    let root = dunce::canonicalize(root.path()).expect("canonical root");
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "nested-object-room");
    let limits = json!({ "max_files": 1, "max_findings": 1, "max_total_bytes": 4096 });
    let inspect = |limits: Value| json!({ "InspectSecrets": { "scope": { "Paths": ["lib.rs"] }, "limits": limits } });
    wire(&room, &inspect(limits.clone())).expect("inspect control");
    let mut bad = limits;
    bad["ignored_future_flag"] = json!("untrusted-field-value-must-not-echo");
    let refusal = wire(&room, &inspect(bad)).expect_err("nested key refused");
    let text = message_of(&refusal).expect("refusal text");
    assert_eq!(
        text,
        "invalid_request: InspectSecrets does not accept \"limits.ignored_future_flag\". Valid \
         arguments: scope, limits. Check the fields of \"limits\" against the tool's input \
         schema."
    );
    assert!(
        !serde_json::to_string(&refusal)
            .unwrap()
            .contains("must-not-echo")
    );
}

/// A wrong type and a missing required argument get the same text on both lanes, and
/// neither lane echoes the offending value.
#[test]
fn decode_failures_carry_the_same_text_without_values() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "decode-room");

    let cases = [
        ("inspect_match", json!({ "line": 1 })),
        ("inspect_match", json!({ "path": 4242, "line": 1 })),
        ("find_dependents", json!({})),
        ("find_dependents", json!({ "path": 4242 })),
        ("get_symbol_context", json!({})),
        ("get_symbol_context", json!({ "name": 4242 })),
    ];
    let mut failures = Vec::new();
    for (tool, args) in cases {
        let (mcp_text, embed) = both(&mut mcp, &room, tool, &args);
        if !mcp_text.starts_with("failed to deserialize parameters: ") || mcp_text.contains("4242")
        {
            failures.push(format!("{tool} {args}: MCP text: {mcp_text}"));
        }
        if embed.as_deref() != Some(mcp_text.as_str()) {
            failures.push(format!(
                "{tool} {args}:\n  mcp:   {mcp_text}\n  embed: {embed:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The embed edit plan request (`Edit.plan`) has `target` only: its refusal lists that,
/// where MCP `edit_plan` also lists `project`.
#[test]
fn the_edit_plan_request_names_its_own_arguments() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "edit-plan-room");
    let plan = |extra: Option<(&str, Value)>| {
        let mut request = json!({ "kind": "plan", "target": {
            "path": "src/short.rs", "name": "SHORT_A", "kind": null, "symbol_line": null
        }});
        if let Some((key, value)) = extra {
            request[key] = value;
        }
        json!({ "Edit": { "request": request, "operation_key": null } })
    };
    wire(&room, &plan(None)).expect("plan control");
    let refusal = wire(&room, &plan(Some((BAD_KEY, json!(1))))).expect_err("unknown key");
    assert_eq!(
        message_of(&refusal).as_deref(),
        Some("invalid_request: edit_plan does not accept \"zz_unknown\". Valid arguments: target.")
    );
    let mcp_text = text_of(&mcp.call("edit_plan", json!({ "target": "SHORT_A", BAD_KEY: 1 })));
    assert_eq!(
        mcp_text,
        "invalid_request: edit_plan does not accept \"zz_unknown\". Valid arguments: project, \
         target."
    );
}

/// What a host shows its LLM for one embed call: the refusal's `message` (or a marker
/// when it has none) or the reply's `rendered` text, and the refusal's kinds.
fn embed_reply(room: &HostRoom, tool: &str, args: &Value) -> (String, Option<String>) {
    match wire(room, &embed_wire(tool, args).expect("wire")) {
        Ok(bytes) => {
            let reply: Value = serde_json::from_slice(&bytes).expect("reply json");
            let text =
                find_string(&reply, "rendered").unwrap_or_else(|| format!("<no rendered> {reply}"));
            (text, None)
        }
        Err(refusal) => {
            let kinds = format!("{:?}/{:?}", refusal.kind, refusal.query_kind);
            let text = message_of(&refusal).unwrap_or_else(|| format!("<no message> {kinds}"));
            (text, Some(kinds))
        }
    }
}

/// How a row of `specs/036-tool-reliability/inputs/embed-refusal-inventory.md` stands.
#[derive(Clone, Copy, Debug)]
enum Row {
    /// Both lanes give the LLM the same bytes.
    Same,
    /// The lanes still differ, for the inventory's stated reason: the embed lane refuses
    /// with exactly `embed` (`kind/query_kind`) and the MCP text starts with `mcp`.
    Differs {
        why: &'static str,
        embed: &'static str,
        mcp: &'static str,
    },
}

/// Runs every case on both lanes and returns one line per row whose standing is not
/// the expected one.
fn probe(mcp: &mut StdioClient, room: &HostRoom, cases: &[(&str, Value, Row)]) -> Vec<String> {
    let mut failures = Vec::new();
    for (tool, args, expected) in cases {
        let mcp_text = text_of(&mcp.call(tool, args.clone()));
        let (embed, kinds) = embed_reply(room, tool, args);
        match expected {
            Row::Same if embed == mcp_text => {}
            Row::Same => failures.push(format!(
                "{tool} {args}:\n  mcp:   {mcp_text:?}\n  embed: {embed:?}"
            )),
            Row::Differs {
                why,
                embed: refusal,
                mcp: prefix,
            } => {
                if embed == mcp_text {
                    failures.push(format!(
                        "{tool} {args}: now equal ({why}); update the inventory and this row"
                    ));
                }
                if kinds.as_deref() != Some(*refusal) || !mcp_text.starts_with(prefix) {
                    failures.push(format!(
                        "{tool} {args} ({why}): expected embed {refusal} and MCP `{prefix}...`\n  \
                         mcp:   {mcp_text:?}\n  embed: {kinds:?} {embed:?}"
                    ));
                }
            }
        }
    }
    failures
}

/// A `Row::Differs` for a call the embed lane refuses although MCP answers it.
const fn stricter(embed: &'static str, mcp: &'static str) -> Row {
    Row::Differs {
        why: "embed refuses a call MCP answers",
        embed,
        mcp,
    }
}

/// Spec 036 embed refusal inventory: a call that leaves the MCP lane with a refusal,
/// not-found, ambiguous or invalid answer gives the embed lane the same bytes, in the
/// refusal `message` or the reply's `rendered` text. Each row is one inventory row.
#[test]
fn query_refusals_carry_the_mcp_text_on_the_embed_lane() {
    use Row::Same;
    const INVALID: &str = "InvalidRequest/Some(InvalidRequest)";
    const UNSUPPORTED: &str = "InvalidRequest/Some(UnsupportedOption)";
    const DECODE: &str = "InvalidRequest/None";
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "query-refusal-room");
    let explore = |query: &str, language: Option<&str>| {
        json!({ "query": query, "language": language, "depth": 1, "limit": 10,
            "include_noise": false, "include_vendor": false, "include_personal_tooling": false })
    };
    let cases: Vec<(&str, Value, Row)> = vec![
        (
            "get_file_context",
            json!({ "path": "src/missing.rs" }),
            Same,
        ),
        (
            "get_file_context",
            json!({ "path": "src/short.rs", "max_tokens": 0 }),
            stricter(INVALID, "Trust: exact"),
        ),
        (
            "get_symbol",
            json!({ "path": "src/missing.rs", "name": "x" }),
            Same,
        ),
        (
            "get_symbol",
            json!({ "path": "src/short.rs", "name": "NOPE" }),
            Same,
        ),
        ("get_symbol", json!({ "name": "NOPE_ANYWHERE" }), Same),
        ("get_symbol", json!({ "name": "target_fn" }), Same),
        (
            "get_symbol",
            json!({ "path": "src/short.rs", "name": "" }),
            stricter(INVALID, "No symbol  in src/short.rs."),
        ),
        (
            "get_symbol",
            json!({ "targets": [{ "path": "src/missing.rs", "name": "x" }] }),
            Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "NOPE_ANYWHERE" }),
            Same,
        ),
        ("get_symbol_context", json!({ "name": "target_fn" }), Same),
        (
            "get_symbol_context",
            json!({ "name": "NOPE", "path": "src/short.rs" }),
            Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "x", "path": "src/missing.rs" }),
            Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "NOPE", "path": "src/short.rs", "bundle": true }),
            Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "NOPE", "path": "src/short.rs", "sections": ["dependents"] }),
            Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "target_fn", "verbosity": "bogus" }),
            stricter(UNSUPPORTED, "Ambiguous symbol selector: 2 symbols"),
        ),
        (
            "get_symbol_context",
            json!({ "name": "target_fn", "bundle": true }),
            Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "target_fn", "symbol_line": 0 }),
            stricter(INVALID, "Ambiguous symbol selector: 2 symbols"),
        ),
        (
            "get_symbol_context",
            json!({ "name": "target_fn", "path": "src/parent/leaf.rs", "sections": ["bogus"] }),
            stricter(UNSUPPORTED, "pub fn target_fn"),
        ),
        ("find_references", json!({ "name": "NOPE_ANYWHERE" }), Same),
        (
            "find_references",
            json!({ "name": "target_fn", "kind": "bogus" }),
            stricter(UNSUPPORTED, "Trust: constrained"),
        ),
        (
            "find_references",
            json!({ "name": "target_fn", "path": "src/missing.rs" }),
            Same,
        ),
        (
            "find_references",
            json!({ "name": "NOPE", "path": "src/short.rs" }),
            Same,
        ),
        (
            "find_references",
            json!({ "name": "" }),
            stricter(INVALID, "No references found for \"\""),
        ),
        (
            "find_references",
            json!({ "name": "target_fn", "max_tokens": 0 }),
            stricter(INVALID, "Trust: constrained"),
        ),
        (
            "find_references",
            json!({ "name": "target_fn", "limit": 0 }),
            stricter(INVALID, "Match type: constrained"),
        ),
        (
            "find_references",
            json!({ "name": "target_fn", "max_per_file": 0 }),
            stricter(INVALID, "Match type: constrained"),
        ),
        ("find_dependents", json!({ "path": "src/missing.rs" }), Same),
        (
            "find_dependents",
            json!({ "path": "src/short.rs", "format": "bogus" }),
            stricter(UNSUPPORTED, "No file-level dependents found"),
        ),
        (
            "find_dependents",
            json!({ "path": "src/parent/leaf.rs", "max_tokens": 0 }),
            stricter(INVALID, "File-level dependency graph:"),
        ),
        (
            "find_dependents",
            json!({ "path": "src/parent/leaf.rs", "limit": 0 }),
            stricter(INVALID, "File-level dependency graph:"),
        ),
        (
            "find_dependents",
            json!({ "path": "src/parent/leaf.rs", "max_per_file": 0 }),
            stricter(INVALID, "File-level dependency graph:"),
        ),
        (
            "inspect_match",
            json!({ "path": "src/short.rs", "line": 999 }),
            Same,
        ),
        (
            "inspect_match",
            json!({ "path": "src/missing.rs", "line": 1 }),
            Same,
        ),
        (
            "inspect_match",
            json!({ "path": "src/short.rs", "line": 0 }),
            Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/missing.rs" }),
            Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/short.rs", "start_line": 5, "end_line": 2 }),
            Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/short.rs", "start_line": 0 }),
            Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/short.rs", "start_line": 999 }),
            Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/short.rs", "mode": "bogus" }),
            stricter(DECODE, "Unknown mode 'bogus'."),
        ),
        (
            "get_file_content",
            json!({ "path": "src/short.rs", "around_symbol": "NOPE" }),
            Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/short.rs", "around_match": "NOPE_TEXT" }),
            Same,
        ),
        (
            "search_symbols",
            json!({ "query": "x", "kind": "bogus" }),
            stricter(UNSUPPORTED, "No symbols matching 'x'."),
        ),
        (
            "search_symbols",
            json!({ "query": "x", "language": "Klingon" }),
            Same,
        ),
        ("search_text", json!({ "query": "" }), Same),
        (
            "search_text",
            json!({ "query": "x", "language": "Klingon" }),
            Same,
        ),
        ("search_text", json!({ "query": "(", "regex": true }), Same),
        (
            "search_text",
            json!({ "query": "x", "group_by": "bogus" }),
            stricter(DECODE, "Match type: constrained"),
        ),
        ("search_files", json!({ "query": "" }), Same),
        (
            "get_repo_map",
            json!({ "max_tokens": 0 }),
            stricter(INVALID, "Index: "),
        ),
        ("get_repo_map", json!({ "path": "nope/dir" }), Same),
        ("get_repo_map", json!({ "detail": "bogus" }), Same),
        (
            "validate_file_syntax",
            json!({ "path": "src/missing.rs" }),
            Same,
        ),
        (
            "analyze_file_impact",
            json!({ "path": "src/missing.rs" }),
            Same,
        ),
        ("explore", explore("x", Some("Klingon")), Same),
        ("explore", explore("", None), Same),
        ("edit_plan", json!({ "target": "NOPE_ANYWHERE" }), Same),
        ("edit_plan", json!({ "target": "target_fn" }), Same),
        ("what_changed", json!({ "git_ref": "no-such-ref" }), Same),
        ("diff_symbols", json!({ "base": "no-such-ref" }), Same),
        ("search_knowledge", json!({ "query": "" }), Same),
        ("ask", json!({ "query": "" }), Same),
    ];
    let failures = probe(&mut mcp, &room, &cases);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The text one call's refusal carries belongs to that refusal: a facade whose step was
/// refused for a missing path, then refused as a whole for its byte budget, does not
/// repeat the step's text; neither does an `ask` whose routed read was refused.
#[test]
fn a_step_refusal_text_does_not_leak_into_a_later_refusal() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "leak-room");
    let step_text = "File not found: src/missing.rs";
    let query = |request: Value, max_bytes: u32| {
        json!({ "Query": { "request": request,
            "limits": { "max_results": 100, "max_bytes": max_bytes } } })
    };
    let facade = json!({ "Symforge": { "query": "outline src/missing.rs" } });
    let mut later_refusals = 0;
    for max_bytes in [1, 8, 32, 64, 128, 256, 512, 1024] {
        if let Err(refusal) = wire(&room, &query(facade.clone(), max_bytes)) {
            later_refusals += 1;
            assert_ne!(
                message_of(&refusal).as_deref(),
                Some(step_text),
                "max_bytes {max_bytes}: {refusal:?}"
            );
        }
    }
    assert!(later_refusals > 0, "no budget refused the facade answer");
    let ask = json!({ "Ask": { "query": "outline src/missing.rs" } });
    // `ask` routes this to a file search, which answers: no refusal, and no step text.
    let answer = wire(&room, &query(ask, 1 << 20)).expect("ask answers");
    let answer: Value = serde_json::from_slice(&answer).expect("ask reply");
    assert_eq!(
        find_string(&answer, "route").as_deref(),
        Some("FindFile"),
        "{answer}"
    );
    assert!(!answer.to_string().contains(step_text), "{answer}");
}

/// A `symforge` facade step refused for a missing path prints the step tool's own text,
/// as the compact-surface MCP facade does.
#[test]
fn a_facade_step_refusal_prints_the_step_text() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = StdioClient::spawn(&root, &[("SYMFORGE_SURFACE", "compact")], None);
    let ready = json!({ "query": "outline src/short.rs" });
    let started = Instant::now();
    loop {
        let served = text_of(&mcp.call("symforge", ready.clone()));
        if served.contains("SHORT_A") || served.contains("Host read: `src/short.rs`") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "index never served: {served}"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "facade-step-room");
    let cases: Vec<(&str, Value, Row)> = vec![(
        "symforge",
        json!({ "query": "outline src/missing.rs" }),
        Row::Same,
    )];
    let failures = probe(&mut mcp, &room, &cases);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A path selector that matches two same-named symbols in one file: every tool that
/// takes one answers with the same text on both lanes, in every symbol-context mode.
#[test]
fn ambiguous_path_selectors_carry_the_mcp_text_on_the_embed_lane() {
    let dir = reliability_support::committed_fixture_copy();
    std::fs::write(
        dir.path().join("src/dup.rs"),
        "pub struct A;\npub struct B;\nimpl A {\n    pub fn dup(&self) -> u32 {\n        1\n    }\n}\n\
         impl B {\n    pub fn dup(&self) -> u32 {\n        2\n    }\n}\n",
    )
    .expect("duplicate-name source");
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "ambiguous-selector-room");
    let cases: Vec<(&str, Value, Row)> = vec![
        (
            "get_symbol",
            json!({ "path": "src/dup.rs", "name": "dup" }),
            Row::Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "dup", "path": "src/dup.rs" }),
            Row::Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "dup", "path": "src/dup.rs", "bundle": true }),
            Row::Same,
        ),
        (
            "get_symbol_context",
            json!({ "name": "dup", "path": "src/dup.rs", "sections": ["dependents"] }),
            Row::Same,
        ),
        (
            "find_references",
            json!({ "name": "dup", "path": "src/dup.rs" }),
            Row::Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/dup.rs", "around_symbol": "dup" }),
            Row::Same,
        ),
    ];
    let failures = probe(&mut mcp, &room, &cases);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Answers whose bytes depend on more than one indexed file: similar-path suggestions
/// (more than five candidates), a bare file target two files end with, and a range
/// refusal for a file holding NUL bytes, with and without an explicit `max_tokens`.
/// Each lane gives the same, process-independent bytes.
#[test]
fn multi_file_answers_are_ordered_and_finished_alike() {
    let dir = reliability_support::committed_fixture_copy();
    std::fs::create_dir_all(dir.path().join("src/sugg")).expect("suggestion dir");
    for suffix in ["h", "g", "f", "e", "d", "c", "b", "a"] {
        std::fs::write(
            dir.path().join(format!("src/sugg/shorty_{suffix}.rs")),
            format!("pub fn shorty_{suffix}() {{}}\n"),
        )
        .expect("suggestion source");
    }
    for name in ["fox", "box"] {
        std::fs::write(
            dir.path().join(format!("src/{name}.rs")),
            format!("pub fn {name}() {{}}\n"),
        )
        .expect("suffix source");
    }
    for side in ["a", "b", "c", "d", "e", "f", "g"] {
        std::fs::create_dir_all(dir.path().join(format!("tools/{side}"))).expect("tools dir");
        std::fs::write(
            dir.path().join(format!("tools/{side}/Makefile")),
            "all:\n\techo ok\n",
        )
        .expect("makefile");
    }
    // A NUL byte past the binary sniff window: indexed, with MCP's NUL warning.
    let mut nul = b"pub const NUL_A: u32 = 1;\n".to_vec();
    nul.extend(b"// padding past the binary sniff window\n".repeat(250));
    nul.extend(b"// a \0 byte\npub const NUL_B: u32 = 2;\n");
    std::fs::write(dir.path().join("src/nulbyte.rs"), nul).expect("nul source");
    // A NUL byte inside the sniff window: a metadata-only binary file.
    std::fs::write(
        dir.path().join("src/bin.rs"),
        b"pub const BIN_A: u32 = 1;\n// a \0 byte\n",
    )
    .expect("binary source");
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "multi-file-room");

    let mut failures = Vec::new();
    let suggested = text_of(&mcp.call(
        "get_symbol",
        json!({ "path": "lib/shorty.py", "name": "x" }),
    ));
    if suggested
        != "File not found: lib/shorty.py. Did you mean: src/sugg/shorty_a.rs, \
            src/sugg/shorty_b.rs, src/sugg/shorty_c.rs, src/sugg/shorty_d.rs, \
            src/sugg/shorty_e.rs?"
    {
        failures.push(format!(
            "suggestions not the sorted first five: {suggested}"
        ));
    }
    let planned = text_of(&mcp.call("edit_plan", json!({ "target": "Makefile" })));
    if !(planned.contains("Found file: tools/a/Makefile")
        && planned.contains(
            "matched 7 paths (tools/a/Makefile, tools/b/Makefile, tools/c/Makefile, \
             tools/d/Makefile, tools/e/Makefile and 2 more)",
        ))
    {
        failures.push(format!(
            "edit_plan does not list five Makefiles and the rest: {planned}"
        ));
    }
    // A file target matches at a path separator: `x.rs` is not `fox.rs` or `box.rs`.
    let suffix = text_of(&mcp.call("edit_plan", json!({ "target": "x.rs" })));
    if !suffix.contains("Target 'x.rs' not found.") {
        failures.push(format!(
            "edit_plan matched x.rs inside a file name: {suffix}"
        ));
    }
    // An invalid regex echoes a long pattern once, capped, before the regex error.
    let pattern = format!("({}", "a".repeat(300));
    let regex = text_of(&mcp.call("search_text", json!({ "query": pattern, "regex": true })));
    if regex.starts_with(&format!("Invalid regex '{pattern}'")) {
        failures.push(format!(
            "the regex refusal echoes the whole pattern twice: {regex}"
        ));
    }
    let cases: Vec<(&str, Value, Row)> = vec![
        (
            "get_symbol",
            json!({ "path": "lib/shorty.py", "name": "x" }),
            Row::Same,
        ),
        ("edit_plan", json!({ "target": "Makefile" }), Row::Same),
        ("edit_plan", json!({ "target": "x.rs" }), Row::Same),
        (
            "search_text",
            json!({ "query": format!("({}", "a".repeat(300)), "regex": true }),
            Row::Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/bin.rs", "start_line": 999 }),
            Row::Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/bin.rs", "start_line": 999, "max_tokens": 5 }),
            Row::Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/short.rs", "start_line": 999, "max_tokens": 5 }),
            Row::Same,
        ),
    ];
    failures.extend(probe(&mut mcp, &room, &cases));
    // Inventory rows 1a, 61 and 62: a metadata-only (binary) file, and an indexed file
    // whose NUL byte lies past the binary sniff window.
    let binary: Vec<(&str, Value, Row)> = vec![
        (
            "get_file_context",
            json!({ "path": "src/bin.rs" }),
            Row::Differs {
                why: "admission-tier text not carried",
                embed: "SourceUnavailable/Some(NotFound)",
                mcp: "Not indexed: src/bin.rs is Tier 2",
            },
        ),
        (
            "get_file_content",
            json!({ "path": "src/bin.rs" }),
            Row::Same,
        ),
        // The stem must not be `nul`: on Windows that is the reserved NUL device name.
        (
            "get_file_content",
            json!({ "path": "src/nulbyte.rs" }),
            Row::Same,
        ),
        (
            "get_file_content",
            json!({ "path": "src/nulbyte.rs", "start_line": 999 }),
            Row::Same,
        ),
    ];
    failures.extend(probe(&mut mcp, &room, &binary));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `curate_knowledge` with two unknown keys: the same text on both lanes, after a review
/// that gives each lane its own hashes; without the keys neither lane refuses the call for
/// its arguments.
#[test]
fn curate_knowledge_refuses_unknown_keys_like_mcp() {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture");
    let mut mcp = mcp_ready(&root);
    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().expect("runtime"));
    let room = room_in(&owner, &root, "curate-room");
    let review = json!({ "mode": "remediation", "source_scope": "current", "limit": 20 });

    let reviewed = text_of(&mcp.call("review_knowledge", review.clone()));
    let mcp_input = json!({
        "actions": [],
        "if_source_review_hash": token(&reviewed, "review_hash"),
        "if_manifest_digest": token(&reviewed, "manifest_digest"),
        "if_policy_digest": token(&reviewed, "policy_digest"),
    });
    let embed_review =
        wire(&room, &embed_wire("review_knowledge", &review).unwrap()).expect("embed review");
    let embed_review: Value = serde_json::from_slice(&embed_review).expect("review reply");
    let rendered = find_string(&embed_review, "rendered").expect("rendered review");
    let serving = find_string(&embed_review, "serving_publication_identity").expect("serving");
    let embed_input = json!({
        "actions": [],
        "if_source_review_hash": token(&rendered, "review_hash"),
        "if_manifest_digest": token(&rendered, "manifest_digest"),
        "if_policy_digest": token(&rendered, "policy_digest"),
    });
    let curate = |input: Value| {
        json!({ "Knowledge": {
            "request": {
                "kind": "curate_preview",
                "input": input,
                "if_serving_publication_identity": serving,
            },
            "operation_key": null,
        }})
    };

    assert_mcp_not_refused(
        &mcp.call("curate_knowledge", mcp_input.clone()),
        "curate control",
    );
    if let Err(refusal) = wire(&room, &curate(embed_input.clone())) {
        assert!(
            !message_of(&refusal).is_some_and(|text| text.contains("does not accept")),
            "embed curate control refused for its arguments: {refusal:?}"
        );
    }
    let with_keys = |mut input: Value| {
        input["zz_one"] = json!(1);
        input["zz_two"] = json!(2);
        input
    };
    let mcp_text = text_of(&mcp.call("curate_knowledge", with_keys(mcp_input)));
    let refusal = wire(&room, &curate(with_keys(embed_input))).expect_err("unknown keys");
    assert!(mcp_text.contains("\"zz_one\", \"zz_two\""), "{mcp_text}");
    assert_eq!(message_of(&refusal).as_deref(), Some(mcp_text.as_str()));
}
