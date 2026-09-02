//! Feature 032 (US1) — repeat-call notice acceptance oracles.
//!
//! Every oracle observes the notice the way a client does: through the
//! serialized `tools/call` response (text content + `_meta`), never through
//! tracker internals. Negative oracles carry their accepting positive control
//! in the same test function (Constitution II).
//!
//! Harness: the FULL-JSON subprocess stdio client pattern from
//! `tests/rmcp3_roots_interop.rs` (`call_tool_result`), plus the in-process
//! HTTP `/mcp` harness from `tests/rmcp3_protocol.rs` for the lane-inertness
//! pin. The stdio server runs with `SYMFORGE_NO_DAEMON=1`, the full surface,
//! and the periodic watcher reconcile disabled, so the only thing that can
//! move the published index between calls is a real file event.
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::server::{
    AuthConfig, AuthLayerState, ServerRuntime, apply_bearer_auth, mcp_http::build_mcp_router,
};
use symforge::sidecar::governor::RequestGovernor;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

const EVIDENCE_KEY: &str = "symforge/project_evidence";
const NOTICE_KEY: &str = "symforge/repeat_notice";
const NOTICE_PREFIX: &str = "Repeat notice:";
const TEST_KEY: &str = "sf_repeat_notice_key";
const MODERN_VERSION: &str = "2026-07-28";

/// Byte-canonical notice text (contracts/repeat-notice.md §2).
fn notice_text(count: u32) -> String {
    format!(
        "Repeat notice: identical request served {count}x with no index change published in between (project evidence unchanged). The result cannot differ until the index changes - change the request instead of retrying."
    )
}

// ---------------------------------------------------------------------------
// Workspace fixture
// ---------------------------------------------------------------------------

struct Workspace {
    _dir: TempDir,
    root: PathBuf,
}

impl Workspace {
    /// A tiny Rust project with distinct symbols so every eligible tool has a
    /// deterministic, non-empty answer.
    fn seed() -> Self {
        let dir = TempDir::new().expect("workspace tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"repeat-notice-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("seed Cargo.toml");
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).expect("seed src dir");
        std::fs::write(src.join("lib.rs"), "pub mod alpha;\npub mod beta;\n").expect("seed lib.rs");
        std::fs::write(
            src.join("alpha.rs"),
            "pub fn alpha_anchor() -> u32 {\n    1\n}\n",
        )
        .expect("seed alpha.rs");
        std::fs::write(
            src.join("beta.rs"),
            "use crate::alpha::alpha_anchor;\n\npub fn beta_anchor() -> u32 {\n    alpha_anchor() + 1\n}\n",
        )
        .expect("seed beta.rs");
        // dunce, not std: std::fs::canonicalize yields a `\\?\` UNC path on
        // Windows, which the env override and evidence root would then carry.
        let root = dunce::canonicalize(dir.path()).expect("canonical workspace root");
        Self { _dir: dir, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

// ---------------------------------------------------------------------------
// Subprocess stdio client (full JSON result access)
// ---------------------------------------------------------------------------

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
        let binary = env!("CARGO_BIN_EXE_symforge");
        let mut child = symforge::process_util::hidden_command(binary)
            .current_dir(workspace)
            .env("RUST_LOG", "error")
            .env("SYMFORGE_NO_DAEMON", "1")
            // Periodic reconcile OFF: only a real file event may republish, so
            // an unchanged index stays unchanged for the whole session.
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
        client.handshake();
        client
    }

    fn handshake(&mut self) {
        let id = self.next_request_id();
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "repeat-notice-client", "version": "0.0.0" }
            }
        }));
        let _ = self.read_until_response(id);
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }));
    }

    /// The FULL `tools/call` result JSON (content, `isError`, `_meta`).
    fn call_tool_result(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_request_id();
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }));
        let response = self.read_until_response(id);
        if let Some(error) = response.get("error") {
            panic!("MCP tool {name} returned JSON-RPC error: {error}");
        }
        response.get("result").expect("tool result").clone()
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn write_message(&mut self, message: Value) {
        serde_json::to_writer(&mut self.stdin, &message).expect("write JSON-RPC message");
        self.stdin.write_all(b"\n").expect("write JSON-RPC newline");
        self.stdin.flush().expect("flush JSON-RPC message");
    }

    fn read_until_response(&mut self, expected_id: u64) -> Value {
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "timed out waiting for JSON-RPC response id {expected_id}"
            );
            let mut line = String::new();
            let bytes = self
                .stdout
                .read_line(&mut line)
                .expect("read JSON-RPC line");
            assert_ne!(bytes, 0, "MCP server stdout closed before response");
            let message: Value = serde_json::from_str(line.trim()).unwrap_or_else(|error| {
                panic!("invalid JSON-RPC line from MCP server: {error}; line={line:?}")
            });
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

// ---------------------------------------------------------------------------
// Response readers
// ---------------------------------------------------------------------------

fn evidence(result: &Value) -> &Value {
    result
        .get("_meta")
        .and_then(|meta| meta.get(EVIDENCE_KEY))
        .unwrap_or_else(|| panic!("result must carry {EVIDENCE_KEY}: {result}"))
}

/// Full, bound evidence: a typed `ProjectEvidence` whose project is not the
/// `"unbound"` placeholder (the same rule the tracker applies).
fn is_bound(evidence: &Value) -> bool {
    evidence.get("bound").is_none()
        && evidence
            .get("project_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != "unbound")
        && evidence
            .get("canonical_root")
            .and_then(Value::as_str)
            .is_some()
        && evidence.get("generation").and_then(Value::as_u64).is_some()
}

fn text_of(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn notice_meta(result: &Value) -> Option<&Value> {
    result.get("_meta").and_then(|meta| meta.get(NOTICE_KEY))
}

fn assert_no_notice(result: &Value, label: &str) {
    assert!(
        notice_meta(result).is_none(),
        "{label}: must not carry the {NOTICE_KEY} _meta carrier: {result}"
    );
    assert!(
        !text_of(result).contains(NOTICE_PREFIX),
        "{label}: must not carry the notice text: {}",
        text_of(result)
    );
}

/// Assert BOTH carriers with `repeat_count == count`, and return the response
/// with the notice removed (so the caller can compare it byte-for-byte to an
/// earlier serve — spec SC-004).
fn assert_notice_and_strip(result: &Value, count: u32, tool: &str, label: &str) -> Value {
    let meta =
        notice_meta(result).unwrap_or_else(|| panic!("{label}: must carry {NOTICE_KEY}: {result}"));
    assert_eq!(
        meta["contract_version"],
        json!(1),
        "{label}: contract_version"
    );
    assert_eq!(meta["repeat_count"], json!(count), "{label}: repeat_count");
    assert_eq!(meta["tool"], json!(tool), "{label}: tool");
    let hash = meta["request_hash"]
        .as_str()
        .unwrap_or_else(|| panic!("{label}: request_hash must be a string: {meta}"));
    assert!(
        !hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit()),
        "{label}: request_hash must be hex: {hash:?}"
    );
    assert_eq!(
        meta["evidence_generation"],
        evidence(result)["generation"],
        "{label}: evidence_generation must name the witnessed evidence generation"
    );

    let mut stripped = result.clone();
    stripped["_meta"]
        .as_object_mut()
        .expect("_meta object")
        .remove(NOTICE_KEY);
    let content = stripped["content"].as_array_mut().expect("content array");
    let last_text = content
        .iter_mut()
        .rev()
        .find(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .unwrap_or_else(|| panic!("{label}: a text block must carry the notice"));
    let text = last_text["text"]
        .as_str()
        .expect("text block text")
        .to_string();
    let suffix = format!("\n\n{}", notice_text(count));
    let prefix = text.strip_suffix(&suffix).unwrap_or_else(|| {
        panic!("{label}: final text block must end with the contract notice; got: {text}")
    });
    last_text["text"] = Value::String(prefix.to_string());
    stripped
}

// ---------------------------------------------------------------------------
// Index stabilization (a startup republish must never masquerade as a change)
// ---------------------------------------------------------------------------

/// Wait until the stdio server serves a bound, Ready, source-backed index whose
/// evidence is identical across two DIFFERENT-fingerprint eligible calls.
/// Returns that evidence.
///
/// The watcher runs a mandatory fresh-instance reconciliation right after
/// startup and republishes once (observed as a generation bump plus a
/// freshness flip); its state only becomes `active` AFTER that repair, so the
/// `health` watcher line is the deterministic "startup republish is over"
/// signal, and a quiet-period settle guards the remaining async tail.
fn stabilize(client: &mut StdioClient) -> Value {
    let started = Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "index never reached a stable bound Ready generation"
        );
        let health = client.call_tool_result("health", json!({}));
        let ready = {
            let ev = evidence(&health);
            is_bound(ev)
                && ev["index_state"] == json!("Ready")
                && ev["index_files"].as_u64().unwrap_or(0) > 0
                && text_of(&health).contains("Watcher: active")
        };
        if !ready {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        let _settled = settle(client);
        let a = client.call_tool_result("search_symbols", json!({"query": "alpha_anchor"}));
        let b = client.call_tool_result("search_symbols", json!({"query": "beta_anchor"}));
        let ea = evidence(&a);
        let eb = evidence(&b);
        if ea == eb && is_bound(ea) && text_of(&a).contains("alpha_anchor") {
            return ea.clone();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Evidence stable across a quiet period (two `status` reads 500 ms apart).
fn settle(client: &mut StdioClient) -> Value {
    let started = Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "evidence never settled"
        );
        let a = evidence(&client.call_tool_result("status", json!({}))).clone();
        std::thread::sleep(Duration::from_millis(500));
        let b = evidence(&client.call_tool_result("status", json!({}))).clone();
        if a == b && is_bound(&a) {
            return a;
        }
    }
}

/// Append a new symbol to `src/beta.rs` and wait until the index has
/// PUBLISHED it (evidence moved AND a different-fingerprint search finds it).
/// Falls back to an in-band `analyze_file_impact` re-read if the watcher does
/// not republish within 30 s, and says so on stderr.
fn publish_index_change(client: &mut StdioClient, workspace: &Path, before: &Value) -> Value {
    let beta = workspace.join("src").join("beta.rs");
    let mut source = std::fs::read_to_string(&beta).expect("read beta.rs");
    source.push_str("\npub fn gamma_anchor() -> u32 {\n    3\n}\n");
    std::fs::write(&beta, source).expect("append gamma_anchor");

    let started = Instant::now();
    let mut nudged = false;
    loop {
        let status = client.call_tool_result("status", json!({}));
        let ev = evidence(&status);
        if is_bound(ev) && ev != before {
            break;
        }
        if !nudged && started.elapsed() > Duration::from_secs(30) {
            eprintln!(
                "repeat_notice: watcher did not republish within 30s; nudging via analyze_file_impact"
            );
            let _ = client.call_tool_result("analyze_file_impact", json!({"path": "src/beta.rs"}));
            nudged = true;
        }
        assert!(
            started.elapsed() < Duration::from_secs(45),
            "the index change was never published (evidence still {before})"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    let settled = settle(client);
    let probe = client.call_tool_result("search_symbols", json!({"query": "gamma_anchor"}));
    assert!(
        text_of(&probe).contains("gamma_anchor"),
        "the published index must serve the new symbol: {}",
        text_of(&probe)
    );
    assert_eq!(
        evidence(&probe),
        &settled,
        "probe evidence must match the settled evidence"
    );
    settled
}

// ---------------------------------------------------------------------------
// Oracle 1 — third identical eligible call notices; first two do not (SC-001,
// SC-004 byte-stability, isError untouched)
// ---------------------------------------------------------------------------

#[test]
fn third_identical_eligible_call_carries_notice_and_first_two_do_not() {
    let workspace = Workspace::seed();
    let mut client = StdioClient::spawn(workspace.root());
    let stable = stabilize(&mut client);

    let args = json!({"query": "anchor"});
    let serve1 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve1, "serve 1");
    assert_eq!(evidence(&serve1), &stable, "serve 1 evidence");
    let serve2 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve2, "serve 2");
    // Positive control for the eligibility claim itself: an index-determined
    // tool answers byte-identically on an unchanged index.
    assert_eq!(serve1, serve2, "serve 2 must be byte-identical to serve 1");

    // FR-002 at the seam: interleaved DIFFERENT calls — one eligible with
    // another fingerprint, one ineligible — neither notice nor reset the run.
    let other = client.call_tool_result("search_symbols", json!({"query": "beta_anchor"}));
    assert_no_notice(&other, "interleaved different-fingerprint eligible call");
    assert_eq!(evidence(&other), &stable, "interleaved eligible call evidence");
    let status = client.call_tool_result("status", json!({}));
    assert_no_notice(&status, "interleaved ineligible call");
    assert_eq!(evidence(&status), &stable, "interleaved ineligible call evidence");

    let serve3 = client.call_tool_result("search_symbols", args);
    let stripped = assert_notice_and_strip(&serve3, 3, "search_symbols", "serve 3");
    // SC-004: the notice is a strict suffix — everything else (content bytes,
    // isError, evidence) is identical to serve 1.
    assert_eq!(
        stripped, serve1,
        "serve 3 minus the notice must equal serve 1 byte-for-byte"
    );
    assert_eq!(
        serve3.get("isError"),
        serve1.get("isError"),
        "the notice must never alter isError"
    );
}

// ---------------------------------------------------------------------------
// Oracle 2 — an index change between repeats resets the run; the notice
// returns at the state machine's count 3 (positive control)
// ---------------------------------------------------------------------------

#[test]
fn index_change_between_repeats_resets_run() {
    let workspace = Workspace::seed();
    let mut client = StdioClient::spawn(workspace.root());
    let stable = stabilize(&mut client);

    let args = json!({"query": "anchor"});
    let serve1 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve1, "serve 1");
    let serve2 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve2, "serve 2");
    assert_eq!(evidence(&serve2), &stable);

    let changed = publish_index_change(&mut client, workspace.root(), &stable);
    assert_ne!(changed, stable, "the change must move the evidence");

    // Serve 3 (would have been the notice) — the index changed, so no claim.
    let serve3 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve3, "serve 3 after index change");
    assert_eq!(evidence(&serve3), &changed, "serve 3 evidence");
    assert_ne!(
        text_of(&serve3),
        text_of(&serve1),
        "the new symbol must change the search answer (the change was real)"
    );

    // Positive control: the run restarted at serve 3 (count 1); serve 4 is
    // count 2 (no notice); serve 5 is count 3 (notice, repeat_count 3).
    let serve4 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve4, "serve 4 (count 2 of the new run)");
    let serve5 = client.call_tool_result("search_symbols", args);
    let stripped = assert_notice_and_strip(&serve5, 3, "search_symbols", "serve 5");
    assert_eq!(
        stripped, serve3,
        "serve 5 minus the notice must equal serve 3 byte-for-byte"
    );
}

// ---------------------------------------------------------------------------
// Oracle 4 — ineligible tools never notice (eligible control in the same test)
// ---------------------------------------------------------------------------

#[test]
fn ineligible_tools_never_notice() {
    let workspace = Workspace::seed();
    let mut client = StdioClient::spawn(workspace.root());
    let _stable = stabilize(&mut client);

    let ineligible: [(&str, Value); 3] = [
        ("status", json!({})),
        ("what_changed", json!({"uncommitted": true})),
        (
            "get_symbol",
            json!({"path": "src/alpha.rs", "name": "alpha_anchor"}),
        ),
    ];
    for (tool, args) in &ineligible {
        for serve in 1..=3 {
            let result = client.call_tool_result(tool, args.clone());
            assert_no_notice(&result, &format!("{tool} serve {serve}"));
        }
    }

    // Eligible control: the same 3x pattern on an eligible tool DOES notice.
    // Settle first: an ineligible call may itself trigger a republish (a
    // freshness flip observed during the RED run), which must not land
    // between the control's serves.
    let _settled = settle(&mut client);
    let args = json!({"query": "anchor"});
    let serve1 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve1, "control serve 1");
    let serve2 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve2, "control serve 2");
    let serve3 = client.call_tool_result("search_symbols", args);
    let stripped = assert_notice_and_strip(&serve3, 3, "search_symbols", "control serve 3");
    assert_eq!(stripped, serve1);
}

// ---------------------------------------------------------------------------
// Oracle 9 — a set-valued `projects` request never accumulates (single-project
// control in the same test)
// ---------------------------------------------------------------------------

#[test]
fn projects_argument_never_accumulates() {
    let workspace = Workspace::seed();
    let mut client = StdioClient::spawn(workspace.root());
    let _stable = stabilize(&mut client);

    let fan_out = json!({"query": "anchor", "projects": ["*"]});
    for serve in 1..=3 {
        let result = client.call_tool_result("search_symbols", fan_out.clone());
        assert_no_notice(&result, &format!("projects serve {serve}"));
        // For the record: what evidence does this lane actually carry? On the
        // local (no-daemon) lane the adapter withholds its seed for a
        // set-valued selector, so the seam discloses the unavailable marker.
        let ev = evidence(&result);
        assert!(
            !is_bound(ev),
            "a projects fan-out must not carry bound single-project evidence: {ev}"
        );
        assert_eq!(
            ev.get("bound"),
            Some(&json!(false)),
            "projects serve {serve} evidence: {ev}"
        );
    }

    // Single-project control.
    let _settled = settle(&mut client);
    let args = json!({"query": "anchor"});
    let serve1 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve1, "control serve 1");
    let serve2 = client.call_tool_result("search_symbols", args.clone());
    assert_no_notice(&serve2, "control serve 2");
    let serve3 = client.call_tool_result("search_symbols", args);
    let stripped = assert_notice_and_strip(&serve3, 3, "search_symbols", "control serve 3");
    assert_eq!(stripped, serve1);
}

// ---------------------------------------------------------------------------
// Oracle 8 — lane inertness: the shared HTTP `/mcp` lane has no observable
// per-session identity, so it NEVER notices (stdio positive control in the
// same test)
// ---------------------------------------------------------------------------

/// An in-process runtime over a REAL indexed workspace, bound to it, so the
/// HTTP responses carry full, equal `ProjectEvidence` — the only condition
/// under which "no notice" is a meaningful negative.
fn indexed_runtime(root: &Path) -> ServerRuntime {
    let index = LiveIndex::load(root).expect("index the workspace");
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "repeat-notice-http".to_string(),
        watcher_info,
        Some(root.to_path_buf()),
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    ServerRuntime::build_runtime(
        index,
        protocol,
        governor,
        AuthConfig::new(Some(TEST_KEY.to_string())),
        None,
    )
}

struct HttpServer {
    url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
}

async fn start_http_server(runtime: ServerRuntime) -> HttpServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback");
    let addr = listener.local_addr().expect("local_addr");
    let router = build_mcp_router(&runtime, addr);
    let auth_state = AuthLayerState::new(runtime.auth().clone(), true);
    let app = apply_bearer_auth(router, auth_state);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let shutdown = async {
            let _ = rx.await;
        };
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
    });
    HttpServer {
        url: format!("http://{addr}/mcp"),
        shutdown: Some(tx),
        join,
    }
}

impl HttpServer {
    async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.await;
    }
}

/// One modern (2026-07-28) `tools/call` over `/mcp`; returns the result JSON.
async fn http_call_tool(
    client: &reqwest::Client,
    url: &str,
    id: u32,
    name: &str,
    arguments: Value,
) -> Value {
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments,
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": MODERN_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {},
            }
        },
    });
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Authorization", format!("Bearer {TEST_KEY}"))
        .header("MCP-Protocol-Version", MODERN_VERSION)
        .header("Mcp-Method", "tools/call")
        .header("Mcp-Name", name)
        .json(&body)
        .send()
        .await
        .expect("request sent");
    let status = resp.status();
    let body = resp.json::<Value>().await.expect("JSON-RPC body");
    assert!(
        status.is_success(),
        "tools/call {name} got {status}: {body}"
    );
    assert!(
        body.get("error").is_none(),
        "tools/call {name} errored: {body}"
    );
    body["result"].clone()
}

#[tokio::test]
async fn sessions_never_share_runs() {
    // Positive control FIRST (blocking subprocess I/O, before any server task
    // shares this runtime): the identical 3x sequence over stdio notices.
    let stdio_workspace = Workspace::seed();
    {
        let mut client = StdioClient::spawn(stdio_workspace.root());
        let _stable = stabilize(&mut client);
        let args = json!({"query": "anchor"});
        let serve1 = client.call_tool_result("search_symbols", args.clone());
        assert_no_notice(&serve1, "stdio control serve 1");
        let serve2 = client.call_tool_result("search_symbols", args.clone());
        assert_no_notice(&serve2, "stdio control serve 2");
        let serve3 = client.call_tool_result("search_symbols", args);
        let stripped =
            assert_notice_and_strip(&serve3, 3, "search_symbols", "stdio control serve 3");
        assert_eq!(stripped, serve1);
    }

    // The shared HTTP lane: one server, bound to a real indexed workspace.
    let http_workspace = Workspace::seed();
    let server = start_http_server(indexed_runtime(http_workspace.root())).await;
    let client = reqwest::Client::new();
    let args = json!({"query": "anchor"});
    let mut serves = Vec::new();
    for id in 1..=3u32 {
        let result = http_call_tool(&client, &server.url, id, "search_symbols", args.clone()).await;
        serves.push(result);
    }
    server.shutdown().await;

    // The negative is only meaningful with FULL, equal, bound evidence — the
    // exact input the tracker would have accumulated on stdio.
    for (index, serve) in serves.iter().enumerate() {
        let ev = evidence(serve);
        assert!(
            is_bound(ev),
            "http serve {} must carry bound evidence: {ev}",
            index + 1
        );
        assert_eq!(
            ev,
            evidence(&serves[0]),
            "http serve {} evidence must equal serve 1",
            index + 1
        );
        assert!(
            text_of(serve).contains("anchor"),
            "http serve {} must answer from the indexed workspace: {}",
            index + 1,
            text_of(serve)
        );
        assert_no_notice(serve, &format!("http serve {}", index + 1));
    }
    assert_eq!(serves[0], serves[1]);
    assert_eq!(serves[1], serves[2]);
}

// ---------------------------------------------------------------------------
// Property pin — every eligible tool is byte-stable on an unchanged index and
// notices on its third identical serve. A RED state is not constructible on
// this code (the list is already exactly these five and each is
// index-determined), the same posture as the `mcp_http` config pin: it exists
// so a future widening of `REPEAT_ELIGIBLE_TOOLS` or a rendering drift in one
// of the five fails a test instead of shipping a false "cannot differ".
// ---------------------------------------------------------------------------

#[test]
fn every_eligible_tool_is_byte_stable_and_notices_on_third_serve() {
    let workspace = Workspace::seed();
    let mut client = StdioClient::spawn(workspace.root());
    let stable = stabilize(&mut client);

    // One distinct fingerprint per tool (and distinct from stabilize()'s
    // probes) so the five runs are independent; every argument set hits real
    // content in the seeded workspace, pinned by `expected`.
    let cases: [(&str, Value, &str); 5] = [
        (
            "search_symbols",
            json!({"query": "anchor", "limit": 20}),
            "fn alpha_anchor",
        ),
        (
            "search_text",
            json!({"query": "alpha_anchor"}),
            "alpha_anchor",
        ),
        (
            "get_repo_map",
            json!({"detail": "tree", "depth": 2}),
            "alpha.rs",
        ),
        (
            "find_references",
            json!({"name": "alpha_anchor"}),
            "beta.rs",
        ),
        (
            "find_dependents",
            json!({"path": "src/alpha.rs"}),
            "beta.rs",
        ),
    ];
    for (tool, args, expected) in cases {
        let serve1 = client.call_tool_result(tool, args.clone());
        assert_no_notice(&serve1, &format!("{tool} serve 1"));
        assert_ne!(
            serve1.get("isError"),
            Some(&json!(true)),
            "{tool} serve 1 must not be an error: {serve1}"
        );
        assert!(
            text_of(&serve1).contains(expected),
            "{tool} serve 1 must hit real workspace content ({expected:?}): {}",
            text_of(&serve1)
        );
        assert_eq!(evidence(&serve1), &stable, "{tool} serve 1 evidence");

        let serve2 = client.call_tool_result(tool, args.clone());
        assert_no_notice(&serve2, &format!("{tool} serve 2"));
        assert_eq!(
            serve2, serve1,
            "{tool}: serve 2 must be byte-identical to serve 1 (content, isError, _meta)"
        );

        let serve3 = client.call_tool_result(tool, args);
        let stripped = assert_notice_and_strip(&serve3, 3, tool, &format!("{tool} serve 3"));
        assert_eq!(
            stripped, serve1,
            "{tool}: serve 3 minus the notice must equal serve 1 byte-for-byte"
        );
        assert_eq!(
            evidence(&serve3)["generation"],
            evidence(&serve1)["generation"],
            "{tool}: evidence generation must be equal across the run"
        );
        assert_eq!(
            serve3.get("isError"),
            serve1.get("isError"),
            "{tool}: the notice must never alter isError"
        );
    }
}

// ---------------------------------------------------------------------------
// F1 — the witness must observe the RESULT, not only the evidence. On every
// zero-hit result `search_text` appends an "untracked file may match"
// diagnostic computed at query time from live `git status` plus raw worktree
// content — an input the index never publishes. An untracked `.symforge/tee/*.rs`
// edit snapshot is exactly such a file: `.symforge/` is hard-scope-excluded from
// the walker AND the watcher, the gitignore hygiene never creates a root
// `.gitignore`, and the sweep classifies every untracked path as code. So the
// body changes while the evidence compares equal — and a notice here would be
// a false "cannot differ".
// ---------------------------------------------------------------------------

fn run_git(cwd: &Path, args: &[&str]) {
    let out = symforge::process_util::hidden_command("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git spawn failed for {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} in {cwd:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// The untracked sweep needs a git worktree; commit the seed so the only
/// untracked path is the one the test plants.
fn git_init_with_initial_commit(root: &Path) {
    run_git(root, &["init", "-b", "main"]);
    run_git(root, &["config", "user.email", "test@example.com"]);
    run_git(root, &["config", "user.name", "repeat-notice-test"]);
    run_git(root, &["add", "-A"]);
    run_git(root, &["commit", "-q", "-m", "initial"]);
}

#[test]
fn untracked_file_diagnostic_never_earns_a_notice() {
    let workspace = Workspace::seed();
    git_init_with_initial_commit(workspace.root());
    let mut client = StdioClient::spawn(workspace.root());
    let stable = stabilize(&mut client);

    // Two zero-hit serves: identical, no diagnostic, no notice.
    let args = json!({"query": "needle-zz"});
    let serve1 = client.call_tool_result("search_text", args.clone());
    assert_no_notice(&serve1, "serve 1");
    assert!(
        !text_of(&serve1).contains("untracked file may match"),
        "serve 1 must carry no untracked diagnostic yet: {}",
        text_of(&serve1)
    );
    assert_eq!(evidence(&serve1), &stable, "serve 1 evidence");
    let serve2 = client.call_tool_result("search_text", args.clone());
    assert_no_notice(&serve2, "serve 2");
    assert_eq!(serve2, serve1, "serve 2 must be byte-identical to serve 1");

    // An untracked file the index will never admit, containing the needle.
    let tee = workspace.root().join(".symforge").join("tee");
    std::fs::create_dir_all(&tee).expect("tee dir");
    std::fs::write(
        tee.join("snapshot.rs"),
        "// needle-zz lives here, outside the index\n",
    )
    .expect("plant untracked snapshot");

    // Serve 3: the body DIFFERS (the diagnostic appeared) while the evidence is
    // still equal — the seam must not claim "cannot differ".
    let serve3 = client.call_tool_result("search_text", args);
    assert_eq!(
        evidence(&serve3),
        &stable,
        "the planted file must not move the published evidence (it is outside the index)"
    );
    assert!(
        text_of(&serve3).contains("untracked file may match"),
        "serve 3 must render the untracked diagnostic: {}",
        text_of(&serve3)
    );
    assert_ne!(
        text_of(&serve3),
        text_of(&serve1),
        "serve 3 body must differ from serve 1"
    );
    assert_no_notice(&serve3, "serve 3 (body changed under equal evidence)");

    // Positive control: a query with hits (no sweep, no untracked
    // interference) notices on its third serve in the same session.
    let control = json!({"query": "alpha_anchor"});
    let c1 = client.call_tool_result("search_text", control.clone());
    assert_no_notice(&c1, "control serve 1");
    assert!(text_of(&c1).contains("alpha_anchor"));
    let c2 = client.call_tool_result("search_text", control.clone());
    assert_no_notice(&c2, "control serve 2");
    assert_eq!(c2, c1);
    let c3 = client.call_tool_result("search_text", control);
    let stripped = assert_notice_and_strip(&c3, 3, "search_text", "control serve 3");
    assert_eq!(stripped, c1);
}
