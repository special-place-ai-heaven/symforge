//! Feature 036 slice A2: no tool argument is silently ignored (specs/036-tool-reliability,
//! T016-T022, FR-001 to FR-004).
//!
//! Every test drives the production `tools/call` path through a real stdio server, so the
//! refusals under test are the ones a client sees. Every refusal carries an accepting
//! control in the same test: the same call without the bad argument is not refused.
#![cfg(feature = "server")]

mod reliability_support;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use symforge::protocol::SymForgeServer;
use tempfile::TempDir;

const STATUS_KEY: &str = "symforge/result_status";
const NOTICE_KEY: &str = "symforge/repeat_notice";

struct StdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
    _home: TempDir,
}

impl StdioClient {
    fn spawn(workspace: &Path) -> Self {
        let home = TempDir::new().expect("isolated symforge home");
        let mut child = symforge::process_util::hidden_command(env!("CARGO_BIN_EXE_symforge"))
            .current_dir(workspace)
            .env("RUST_LOG", "error")
            .env("SYMFORGE_NO_DAEMON", "1")
            .env("SYMFORGE_RECONCILE_INTERVAL", "0")
            .env("SYMFORGE_HOME", home.path())
            .env("SYMFORGE_SURFACE", "full")
            .env("SYMFORGE_WORKSPACE_ROOT", workspace)
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
                "clientInfo": { "name": "reliability-contract", "version": "0.0.0" }
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

    /// The full `tools/call` result JSON (content, `isError`, `_meta`).
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

fn outcome_class(result: &Value) -> Option<&str> {
    result["_meta"][STATUS_KEY]["outcome_class"].as_str()
}

/// A bound server over a committed copy of the reliability fixture, indexed.
fn ready_server() -> (TempDir, StdioClient) {
    let dir = reliability_support::committed_fixture_copy();
    let root = dunce::canonicalize(dir.path()).expect("canonical fixture copy");
    let mut client = StdioClient::spawn(&root);
    let started = Instant::now();
    loop {
        let reply = client.call("search_symbols", json!({ "query": "target_fn" }));
        if text_of(&reply).contains("target_fn") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "index never served the fixture: {}",
            text_of(&reply)
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    (dir, client)
}

fn evidence(result: &Value) -> &Value {
    &result["_meta"]["symforge/project_evidence"]
}

fn is_bound(evidence: &Value) -> bool {
    evidence.get("bound").is_none()
        && evidence["project_id"]
            .as_str()
            .is_some_and(|id| id != "unbound")
        && evidence["generation"].as_u64().is_some()
}

/// Wait until the index serves a bound, Ready generation whose evidence no longer moves
/// (the startup republish is over), so a repeat notice can be earned (tests/repeat_notice.rs).
fn stabilize(client: &mut StdioClient) {
    let started = Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(40),
            "index never reached a stable bound Ready generation"
        );
        let health = client.call("health", json!({}));
        let ev = evidence(&health);
        let ready = is_bound(ev)
            && ev["index_state"] == json!("Ready")
            && text_of(&health).contains("Watcher: active");
        if ready {
            let a = evidence(&client.call("status", json!({}))).clone();
            std::thread::sleep(Duration::from_millis(500));
            let b = evidence(&client.call("status", json!({}))).clone();
            let x = client.call("search_symbols", json!({ "query": "short_a" }));
            let y = client.call("search_symbols", json!({ "query": "leaf" }));
            if a == b && is_bound(&a) && evidence(&x) == evidence(&y) && evidence(&x) == &a {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// `invalid_request` refusal that names `key`, the tool, and lists valid arguments.
fn assert_unknown_argument_refusal(result: &Value, tool: &str, key: &str, valid_sample: &str) {
    let text = text_of(result);
    assert_eq!(
        result["isError"],
        json!(true),
        "{tool}: not an error: {text}"
    );
    assert_eq!(
        outcome_class(result),
        Some("invalid_request"),
        "{tool}: wrong outcome class: {result}"
    );
    assert!(
        text.starts_with("invalid_request: "),
        "{tool}: refusal text form: {text}"
    );
    assert!(
        result["_meta"].get("symforge/project_evidence").is_some(),
        "{tool}: a refusal names the project like every other result: {result}"
    );
    assert!(
        text.contains(&format!("{tool} does not accept")) && text.contains(key),
        "{tool}: refusal must name the tool and `{key}`: {text}"
    );
    assert!(
        text.contains("Valid arguments:") && text.contains(valid_sample),
        "{tool}: refusal must list valid arguments including `{valid_sample}`: {text}"
    );
}

fn assert_not_refused(result: &Value, label: &str) {
    let text = text_of(result);
    assert!(
        !text.contains("does not accept") && outcome_class(result) != Some("invalid_request"),
        "{label}: an accepted call was refused: {text}"
    );
}

/// Resolve `{placeholder}` string values of a typical call; reports whether any existed.
fn fill_placeholders(value: &mut Value, root: &str) -> bool {
    match value {
        Value::String(s) if s.starts_with('{') && s.ends_with('}') => {
            *s = if s == "{root}" {
                root.to_string()
            } else {
                "x".to_string()
            };
            true
        }
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |any, v| fill_placeholders(v, root) | any),
        Value::Object(map) => map
            .values_mut()
            .fold(false, |any, v| fill_placeholders(v, root) | any),
        _ => false,
    }
}

#[test]
fn every_registered_tool_refuses_an_unknown_argument_and_accepts_the_control() {
    let (dir, mut client) = ready_server();
    let root = dunce::canonicalize(dir.path()).expect("root");
    let root = root.to_string_lossy().into_owned();
    let mut calls = SymForgeServer::typical_calls_for_tests();
    // Calls that rebind or rewrite server state go last so earlier controls see one index.
    calls.sort_by_key(|(tool, _)| matches!(*tool, "index_folder" | "checkpoint_now"));
    assert!(
        calls.len() >= 41,
        "contract table lost tools: {}",
        calls.len()
    );
    for (tool, typical) in calls {
        let mut args: Value = serde_json::from_str(typical).expect("typical call is JSON");
        let placeholders = fill_placeholders(&mut args, &root);

        // A placeholder stands for an earlier call's output, so such a control may be
        // invalid for its own reasons; it must still not be refused for its arguments.
        // The `symforge` facade answers invalid_request on the full surface by design.
        let control = client.call(tool, args.clone());
        if placeholders || tool == "symforge" {
            assert!(
                !text_of(&control).contains("does not accept"),
                "{tool}: control refused for its arguments: {}",
                text_of(&control)
            );
        } else {
            assert_not_refused(&control, &format!("{tool} control"));
        }

        let mut bad = args.clone();
        bad.as_object_mut()
            .expect("typical call is an object")
            .insert("zz_unknown".to_string(), json!(1));
        let refused = client.call(tool, bad);
        let valid_sample = args
            .as_object()
            .and_then(|o| o.keys().next().cloned())
            .unwrap_or_else(|| "Valid arguments:".to_string());
        assert_unknown_argument_refusal(&refused, tool, "zz_unknown", &valid_sample);
    }
}

#[test]
fn a_nested_unknown_key_is_refused_and_the_clean_call_succeeds() {
    let (_dir, mut client) = ready_server();

    let edit = json!({
        "edits": [{
            "path": "src/short.rs", "name": "SHORT_A",
            "operation": { "type": "replace", "new_body": "pub const SHORT_A: u32 = 1;" }
        }],
        "dry_run": true
    });
    let control = client.call("batch_edit", edit.clone());
    assert_not_refused(&control, "batch_edit control");
    assert_eq!(control["isError"], json!(false), "{}", text_of(&control));
    let mut bad = edit;
    bad["edits"][0]["zz_unknown"] = json!(1);
    let refused = client.call("batch_edit", bad);
    assert_unknown_argument_refusal(&refused, "batch_edit", "zz_unknown", "edits");
    assert!(
        text_of(&refused).contains("edits"),
        "nested refusal must locate the key: {}",
        text_of(&refused)
    );

    let targets = json!({ "targets": [{ "path": "src/long.rs", "name": "long_body" }] });
    let control = client.call("get_symbol", targets.clone());
    assert_not_refused(&control, "get_symbol control");
    assert_eq!(control["isError"], json!(false), "{}", text_of(&control));
    let mut bad = targets;
    bad["targets"][0]["zz_unknown"] = json!(1);
    let refused = client.call("get_symbol", bad);
    assert_unknown_argument_refusal(&refused, "get_symbol", "zz_unknown", "targets");
}

#[test]
fn serde_aliases_are_accepted() {
    let (_dir, mut client) = ready_server();
    // `symbol` is an alias of `name`, `body` of `new_body` (src/protocol/edit.rs).
    let by_alias = client.call(
        "replace_symbol_body",
        json!({
            "path": "src/short.rs", "symbol": "SHORT_A",
            "body": "pub const SHORT_A: u32 = 1;", "dry_run": true
        }),
    );
    assert_not_refused(&by_alias, "replace_symbol_body via aliases");
    assert_eq!(by_alias["isError"], json!(false), "{}", text_of(&by_alias));
    let canonical = client.call(
        "replace_symbol_body",
        json!({
            "path": "src/short.rs", "name": "SHORT_A",
            "new_body": "pub const SHORT_A: u32 = 1;", "dry_run": true
        }),
    );
    assert_eq!(text_of(&by_alias), text_of(&canonical));
    // The facade edit tool uses `symbol` and `body` as its own names.
    let facade = client.call(
        "symforge_edit",
        json!({
            "path": "src/short.rs", "symbol": "SHORT_A", "op": "replace",
            "body": "pub const SHORT_A: u32 = 1;"
        }),
    );
    assert_not_refused(&facade, "symforge_edit");
}

#[test]
fn an_argument_the_handler_ignores_is_refused_naming_its_mode() {
    let (_dir, mut client) = ready_server();

    // detail="full" never reads `depth` (tools.rs get_repo_map full branch).
    let refused = client.call("get_repo_map", json!({ "detail": "full", "depth": 1 }));
    let text = text_of(&refused);
    assert_eq!(outcome_class(&refused), Some("invalid_request"), "{text}");
    assert!(
        text.contains("\"depth\"") && text.contains("detail") && text.contains("\"full\""),
        "must name depth, detail and the mode where it is ignored: {text}"
    );
    let control = client.call("get_repo_map", json!({ "detail": "tree", "depth": 1 }));
    assert_not_refused(&control, "get_repo_map tree+depth");

    // A batch `targets` call never reads the top-level name.
    let refused = client.call(
        "get_symbol",
        json!({ "targets": [{ "path": "src/long.rs", "name": "long_body" }], "name": "x" }),
    );
    assert_eq!(outcome_class(&refused), Some("invalid_request"));
    assert!(text_of(&refused).contains("targets"));
    let control = client.call(
        "get_symbol",
        json!({ "targets": [{ "path": "src/long.rs", "name": "long_body" }] }),
    );
    assert_not_refused(&control, "get_symbol targets");
}

#[test]
fn an_empty_sections_list_keeps_its_published_meaning() {
    let (_dir, mut client) = ready_server();
    let with_path = client.call(
        "get_symbol_context",
        json!({ "name": "target_fn", "path": "src/parent/leaf.rs", "sections": [] }),
    );
    assert_not_refused(&with_path, "sections=[] with path");
    assert_eq!(
        with_path["isError"],
        json!(false),
        "{}",
        text_of(&with_path)
    );
    let all = client.call(
        "get_symbol_context",
        json!({ "name": "target_fn", "path": "src/parent/leaf.rs",
                "sections": ["dependents", "siblings", "implementations", "git", "knowledge"] }),
    );
    assert!(
        text_of(&with_path).contains("Callers") || text_of(&all).contains("Callers"),
        "trace mode must name callers: {}",
        text_of(&with_path)
    );

    // Without a path the call is refused as an invalid request that names the argument
    // that is missing AND the mode that needs it, not as an unknown argument.
    let refused = client.call(
        "get_symbol_context",
        json!({ "name": "target_fn", "sections": [] }),
    );
    let text = text_of(&refused);
    assert_eq!(outcome_class(&refused), Some("invalid_request"), "{text}");
    assert!(
        text.contains("path") && text.contains("sections") && text.contains("trace"),
        "must name path, sections and the trace mode: {text}"
    );
    assert!(!text.contains("does not accept"), "{text}");
}

#[test]
fn a_refused_call_does_not_advance_the_repeat_notice_counter() {
    let (_dir, mut client) = ready_server();
    stabilize(&mut client);
    let notice = |result: &Value| result["_meta"].get(NOTICE_KEY).cloned();

    // Control: three plain identical clean calls DO carry the notice, so the absence
    // asserted below is meaningful.
    let control = json!({ "query": "long_body" });
    assert!(notice(&client.call("search_symbols", control.clone())).is_none());
    assert!(notice(&client.call("search_symbols", control.clone())).is_none());
    let third = client.call("search_symbols", control);
    assert_eq!(
        notice(&third).map(|n| n["repeat_count"].clone()),
        Some(json!(3)),
        "control: the third identical clean call must carry the notice: {third}"
    );

    // Two clean calls, a refused call with the same tool and otherwise identical
    // arguments, then the third clean call: the notice appears exactly when it would
    // without the refused call.
    // A query nothing else in this test (or `ready_server`/`stabilize`) has served.
    let good = json!({ "query": "SHORT_C" });
    let bad = json!({ "query": "SHORT_C", "zz_unknown": 1 });
    assert!(notice(&client.call("search_symbols", good.clone())).is_none());
    assert!(notice(&client.call("search_symbols", good.clone())).is_none());
    let refused = client.call("search_symbols", bad.clone());
    assert_unknown_argument_refusal(&refused, "search_symbols", "zz_unknown", "query");
    assert!(notice(&refused).is_none());
    let third = client.call("search_symbols", good);
    assert_eq!(
        notice(&third).map(|n| n["repeat_count"].clone()),
        Some(json!(3)),
        "the refused call must not have disturbed the run: {third}"
    );

    // Repeated refusals never count toward a notice of their own: before the check,
    // the unknown key was ignored and the third identical serve carried one.
    for n in 1..=4 {
        let refused = client.call("search_symbols", bad.clone());
        assert_unknown_argument_refusal(&refused, "search_symbols", "zz_unknown", "query");
        assert!(notice(&refused).is_none(), "refusal {n} carries a notice");
    }
}

fn assert_no_mode_refusal(result: &Value, label: &str) {
    let text = text_of(result);
    assert!(
        !text.contains("applies when")
            && !text.contains("does not apply when")
            && !text.contains("does not accept"),
        "{label}: a valid call was refused: {text}"
    );
}

#[test]
fn calls_the_handlers_read_are_not_refused() {
    let (_dir, mut client) = ready_server();
    // Each of these is read by its handler (or decodes to a valid call): a refusal here is
    // a false refusal. The "0"/"1" strings are lenient booleans and "" is absent (FR-003).
    for (tool, args) in [
        (
            "edit_within_symbol",
            json!({ "path": "src/user.rs", "name": "use_it", "old_text": "target_fn(7)",
                    "new_text": "target_fn(8)", "dry_run": true,
                    "replace_all": "0", "occurrence": 2 }),
        ),
        (
            "edit_within_symbol",
            json!({ "path": "src/user.rs", "name": "use_it", "old_text": "target_fn(7)",
                    "new_text": "target_fn(8)", "dry_run": true,
                    "replace_all": true, "occurrence": 1 }),
        ),
        (
            "search_text",
            json!({ "query": "LONG_FIRST", "follow_refs": "1", "follow_refs_limit": 5 }),
        ),
        (
            "search_text",
            json!({ "query": "LONG_FIRST", "regex": true, "whole_word": true }),
        ),
        (
            "get_symbol_context",
            json!({ "name": "target_fn", "bundle": "0", "file": "src/parent/leaf.rs" }),
        ),
        (
            "get_symbol_context",
            json!({ "name": "target_fn", "path": "src/parent/leaf.rs", "bundle": true,
                    "file": "src/parent/leaf.rs" }),
        ),
        (
            "find_references",
            json!({ "name": "target_fn", "mode": "", "kind": "call" }),
        ),
        (
            "find_references",
            json!({ "name": "target_fn", "mode": "refs", "kind": "call" }),
        ),
        (
            "find_references",
            json!({ "name": "target_fn", "mode": "implementations",
                    "path": "src/parent/leaf.rs", "direction": "auto" }),
        ),
        ("get_repo_map", json!({ "detail": "", "depth": 1 })),
        ("get_repo_map", json!({ "detail": "Compact", "depth": 3 })),
        ("get_repo_map", json!({ "detail": "summary", "depth": 3 })),
        ("get_repo_map", json!({ "max_files": 50, "depth": 3 })),
        (
            "get_symbol",
            json!({ "targets": [], "name": "long_body", "path": "src/long.rs" }),
        ),
        (
            "review_knowledge",
            json!({ "mode": "summary", "path": "docs/notes.md" }),
        ),
        (
            "review_knowledge",
            json!({ "mode": "remediation", "path": "docs/notes.md" }),
        ),
        (
            "what_changed",
            json!({ "since": 1, "uncommitted": true, "include_symbol_diff": true }),
        ),
        (
            "detect_impact",
            json!({ "base_branch": "main", "since": "HEAD" }),
        ),
        (
            "analyze_file_impact",
            json!({ "path": "src/parent/leaf.rs", "co_changes_limit": 3 }),
        ),
        (
            "search_files",
            json!({ "query": "leaf", "anchor_path": "src/parent/leaf.rs" }),
        ),
    ] {
        let reply = client.call(tool, args.clone());
        assert_no_mode_refusal(&reply, &format!("{tool} {args}"));
        // No server-chosen mode, so no note.
        assert!(!text_of(&reply).contains("\n\nMode: `"), "{tool}");
    }
}

#[test]
fn stringified_arrays_and_elements_are_checked_like_native_ones() {
    let (_dir, mut client) = ready_server();
    let edit = json!({
        "path": "src/short.rs", "name": "SHORT_A",
        "operation": { "type": "replace", "new_body": "pub const SHORT_A: u32 = 1;" }
    });
    let mut bad = edit.clone();
    bad["zz"] = json!(1);
    let shorthand = "src/short.rs::SHORT_A => replace pub const SHORT_A: u32 = 1;";

    // Every valid shape stays accepted.
    for (label, edits) in [
        ("native", json!([edit])),
        ("stringified array", json!(json!([edit]).to_string())),
        ("stringified element", json!([edit.to_string()])),
        ("shorthand element", json!([shorthand])),
    ] {
        let reply = client.call("batch_edit", json!({ "edits": edits, "dry_run": true }));
        assert_not_refused(&reply, label);
        assert_eq!(
            reply["isError"],
            json!(false),
            "{label}: {}",
            text_of(&reply)
        );
    }
    // Every invalid shape is refused, naming the key.
    for (label, edits) in [
        ("native", json!([bad])),
        ("stringified array", json!(json!([bad]).to_string())),
        ("stringified element", json!([bad.to_string()])),
    ] {
        let refused = client.call("batch_edit", json!({ "edits": edits, "dry_run": true }));
        assert_unknown_argument_refusal(&refused, "batch_edit", "zz", "edits");
        assert!(
            text_of(&refused).contains("edits.0.zz"),
            "{label}: must locate the key: {}",
            text_of(&refused)
        );
    }
    // A key inside the operation is located too.
    let mut bad_operation = edit.clone();
    bad_operation["operation"]["zz"] = json!(1);
    let refused = client.call(
        "batch_edit",
        json!({ "edits": [bad_operation], "dry_run": true }),
    );
    assert_unknown_argument_refusal(&refused, "batch_edit", "zz", "edits");
    assert!(
        text_of(&refused).contains("edits.0.operation.zz"),
        "{}",
        text_of(&refused)
    );

    // batch_insert targets, stringified.
    let target = json!({ "path": "src/short.rs", "name": "SHORT_A" });
    let mut bad_target = target.clone();
    bad_target["zz"] = json!(1);
    let insert = |targets: Value| json!({ "content": "// x", "position": "before", "targets": targets, "dry_run": true });
    let ok = client.call("batch_insert", insert(json!(json!([target]).to_string())));
    assert_not_refused(&ok, "batch_insert stringified");
    assert_eq!(ok["isError"], json!(false), "{}", text_of(&ok));
    let refused = client.call(
        "batch_insert",
        insert(json!(json!([bad_target]).to_string())),
    );
    assert_unknown_argument_refusal(&refused, "batch_insert", "zz", "targets");
}

#[test]
fn nested_unknown_fields_keep_their_path_and_hint() {
    let (_dir, mut client) = ready_server();
    let refused = client.call(
        "get_symbol",
        json!({ "targets": [{ "path": "src/long.rs", "name": "long_body", "zz": 1 }] }),
    );
    assert_unknown_argument_refusal(&refused, "get_symbol", "zz", "targets");
    let text = text_of(&refused);
    assert!(text.contains("\"targets.0.zz\""), "{text}");
    assert!(text.contains("Check the fields of \"targets\""), "{text}");
}

#[test]
fn the_probe_relay_applies_the_same_check() {
    let (_dir, mut client) = ready_server();
    let relay = |args: Value| json!({ "query": "x", "_probe_legacy_tool": "search_symbols", "_probe_legacy_args": args });
    let ok = client.call("symforge", relay(json!({ "query": "target_fn" })));
    assert!(text_of(&ok).contains("target_fn"), "{}", text_of(&ok));
    let refused = client.call(
        "symforge",
        relay(json!({ "query": "target_fn", "zz_unknown": 1 })),
    );
    assert_unknown_argument_refusal(&refused, "search_symbols", "zz_unknown", "query");
}

#[test]
fn a_mode_refusal_never_echoes_an_argument_value() {
    let (_dir, mut client) = ready_server();
    let value = "SENSITIVE_VALUE_".repeat(10);
    let refused = client.call("get_repo_map", json!({ "detail": "full", "depth": value }));
    let text = text_of(&refused);
    assert_eq!(outcome_class(&refused), Some("invalid_request"), "{text}");
    assert!(!text.contains("SENSITIVE_VALUE"), "value echoed: {text}");
    let refused = client.call(
        "get_symbol",
        json!({ "targets": [{ "path": "src/long.rs", "name": "long_body" }], "name": value }),
    );
    assert!(
        !text_of(&refused).contains("SENSITIVE_VALUE"),
        "{}",
        text_of(&refused)
    );
    // An over-long unknown KEY is cut, and its value is never echoed.
    let long_key = "k".repeat(300);
    let refused = client.call(
        "search_symbols",
        json!({ "query": "x", (long_key.clone()): value }),
    );
    let text = text_of(&refused);
    assert!(
        !text.contains(&"k".repeat(60)) && !text.contains("SENSITIVE_VALUE"),
        "{text}"
    );
}
