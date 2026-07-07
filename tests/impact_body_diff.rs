// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! End-to-end regression tests for terminal-commander findings (2026-07-07):
//!
//! - `analyze_file_impact` must not mark every symbol `[Changed]` when only a
//!   prefix comment/leading doc shifts byte offsets (TC finding #4/#5).
//! - Real symbol body edits must still surface as `[Changed]` (sanity).
//! - `edit_plan` must resolve `path::Type::method` selectors (TC finding #3).
//!
//! Dispatches through `SymForgeServer::dispatch_tool_for_tests` — the same handler
//! path the MCP tool surface uses (minus daemon proxy transport).

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::sidecar::spawn_sidecar;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;
use tokio::sync::Mutex as AsyncMutex;

static CWD_LOCK: Lazy<AsyncMutex<()>> = Lazy::new(|| AsyncMutex::new(()));

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    server: SymForgeServer,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        for (rel, content) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            fs::write(&path, content).expect("write fixture file");
        }
        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "impact_body_diff_test".to_string(),
            watcher_info,
            Some(root.clone()),
            None,
        );
        Self {
            _dir: dir,
            root,
            server,
        }
    }

    fn rewrite(&self, rel: &str, content: &str) {
        fs::write(self.root.join(rel), content).expect("rewrite fixture file");
    }

    async fn impact(&self, path: &str) -> String {
        self.server
            .dispatch_tool_for_tests(
                "analyze_file_impact",
                json!({ "path": path, "include_co_changes": false }),
            )
            .await
    }

    async fn plan(&self, target: &str) -> String {
        self.server
            .dispatch_tool_for_tests("edit_plan", json!({ "target": target }))
            .await
    }
}

fn assert_no_changed_symbols(result: &str) {
    assert!(
        !result.contains("[Changed]"),
        "prefix-only edits must not mark symbols changed; got:\n{result}"
    );
    assert!(
        result.contains("indexed and unchanged")
            || result.contains("Status: indexed and unchanged"),
        "expected unchanged status; got:\n{result}"
    );
}

// ─── analyze_file_impact: prefix comment drift (TC #4) ───────────────────────

#[tokio::test]
async fn impact_prefix_module_comment_does_not_mark_symbols_changed() {
    let before = "\
fn shell_exec_allowed_on_default_profile_e2e() {}

fn helper_setup() {}
";
    let after = "\
//! Default-allow: on the default profile shell is permitted.

fn shell_exec_allowed_on_default_profile_e2e() {}

fn helper_setup() {}
";
    let fx = Fixture::new(&[("crates/mcp/tests/shell_live_e2e.rs", before)]);
    fx.rewrite("crates/mcp/tests/shell_live_e2e.rs", after);

    let result = fx.impact("crates/mcp/tests/shell_live_e2e.rs").await;
    assert_no_changed_symbols(&result);
}

#[tokio::test]
async fn impact_prefix_doc_comment_before_test_fn_does_not_mark_following_tests_changed() {
    let before = "\
#[test]
fn shell_exec_allowed_on_default_profile_returns_combed_start() {}

#[test]
fn shell_exec_denied_maps_to_policy_denied() {}
";
    let after = "\
/// Shell exec is allowed on the default profile in this harness.
#[test]
fn shell_exec_allowed_on_default_profile_returns_combed_start() {}

#[test]
fn shell_exec_denied_maps_to_policy_denied() {}
";
    let fx = Fixture::new(&[("crates/daemon/tests/ipc_command.rs", before)]);
    fx.rewrite("crates/daemon/tests/ipc_command.rs", after);

    let result = fx.impact("crates/daemon/tests/ipc_command.rs").await;
    assert_no_changed_symbols(&result);
}

// ─── analyze_file_impact: JSON fixture drift (TC #5) ────────────────────────

#[tokio::test]
async fn impact_json_single_invariant_edit_does_not_mark_unrelated_keys_changed() {
    let before = r#"{
  "tool": "shell_exec",
  "version": "1",
  "group": "shell",
  "_meta": { "note": "baseline" },
  "invariants": ["allow_shell off"],
  "params": { "command": { "type": "string" } }
}
"#;
    let after = r#"{
  "tool": "shell_exec",
  "version": "1",
  "group": "shell",
  "_meta": { "note": "baseline" },
  "invariants": ["allow_shell on"],
  "params": { "command": { "type": "string" } }
}
"#;
    let fx = Fixture::new(&[(
        "tests/fixtures/contracts/mcp-tools/shell_exec.v1.json",
        before,
    )]);
    fx.rewrite(
        "tests/fixtures/contracts/mcp-tools/shell_exec.v1.json",
        after,
    );

    let result = fx
        .impact("tests/fixtures/contracts/mcp-tools/shell_exec.v1.json")
        .await;

    assert!(
        !result.contains("[Changed] version"),
        "unchanged JSON keys must not be marked changed; got:\n{result}"
    );
    assert!(
        !result.contains("[Changed] tool"),
        "unchanged JSON keys must not be marked changed; got:\n{result}"
    );
    assert!(
        !result.contains("[Changed] params"),
        "unchanged JSON keys must not be marked changed; got:\n{result}"
    );
    let changed_lines: Vec<&str> = result
        .lines()
        .filter(|line| line.contains("[Changed]"))
        .collect();
    assert!(
        changed_lines.len() <= 2,
        "expect at most the edited invariant key as changed, not a broad cascade; \
         changed lines: {changed_lines:?}\nfull output:\n{result}"
    );
}

#[tokio::test]
async fn impact_json_meta_note_edit_does_not_mark_schema_keys_changed() {
    let before = r#"{
  "tool": "shell_exec",
  "_meta": { "note": "old note" },
  "params": { "command": { "type": "string" } }
}
"#;
    let after = r#"{
  "tool": "shell_exec",
  "_meta": { "note": "new note" },
  "params": { "command": { "type": "string" } }
}
"#;
    let fx = Fixture::new(&[(
        "tests/fixtures/contracts/mcp-tools/shell_exec.v1.json",
        before,
    )]);
    fx.rewrite(
        "tests/fixtures/contracts/mcp-tools/shell_exec.v1.json",
        after,
    );

    let result = fx
        .impact("tests/fixtures/contracts/mcp-tools/shell_exec.v1.json")
        .await;

    assert!(
        !result.contains("[Changed] params"),
        "schema keys with unchanged body must not be marked changed; got:\n{result}"
    );
    assert!(
        !result.contains("[Changed] tool"),
        "unchanged tool key must not be marked changed; got:\n{result}"
    );
}

// ─── analyze_file_impact: real edits still report changed (sanity) ───────────

#[tokio::test]
async fn impact_real_function_body_edit_still_reports_changed() {
    let before = "fn alpha() { 1 }\nfn beta() { 2 }\n";
    let after = "fn alpha() { 99 }\nfn beta() { 2 }\n";
    let fx = Fixture::new(&[("src/lib.rs", before)]);
    fx.rewrite("src/lib.rs", after);

    let result = fx.impact("src/lib.rs").await;

    assert!(
        result.contains("[Changed]") && result.contains("alpha"),
        "real body edits must still surface as changed; got:\n{result}"
    );
    assert!(
        !result.contains("[Changed]") || !result.contains("beta"),
        "unchanged sibling symbols must not be marked changed; got:\n{result}"
    );
}

// ─── edit_plan: qualified impl method selector (TC #3) ──────────────────────

#[tokio::test]
async fn edit_plan_resolves_qualified_impl_method_via_mcp_handler() {
    let source = "\
struct PolicyEngine;

impl PolicyEngine {
    pub fn new() -> Self {
        PolicyEngine
    }
}
";
    let fx = Fixture::new(&[("crates/daemon/src/policy.rs", source)]);

    let plan = fx
        .plan("crates/daemon/src/policy.rs::PolicyEngine::new")
        .await;

    assert!(
        !plan.contains("not found"),
        "qualified impl method selector should resolve via MCP handler; got:\n{plan}"
    );
    assert!(
        plan.contains("new in crates/daemon/src/policy.rs"),
        "plan should name the resolved bare method; got:\n{plan}"
    );
    assert!(
        plan.contains("Suggested tool sequence"),
        "resolved target should produce an edit plan; got:\n{plan}"
    );
}

// ─── Sidecar /impact hook parity (HOOK-05 path) ─────────────────────────────

fn raw_http_get(port: u16, path: &str, query: &str) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))?;
    let request =
        format!("GET {path}?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sidecar_impact_prefix_comment_matches_mcp_handler() {
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().expect("cwd");
    let dir = TempDir::new().expect("tempdir");
    std::env::set_current_dir(dir.path()).expect("set cwd");

    let before = "fn alpha() {}\nfn beta() {}\n";
    let after = "//! header only\nfn alpha() {}\nfn beta() {}\n";
    let rel = "src/hook_parity.rs";
    let abs = dir.path().join(rel);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).expect("create src dir");
    }
    fs::write(&abs, before).expect("write before");
    let shared = LiveIndex::load(dir.path()).expect("load index");
    fs::write(dir.path().join(rel), after).expect("write after");

    let handle = spawn_sidecar(
        Arc::clone(&shared),
        "127.0.0.1",
        Some(dir.path().to_path_buf()),
    )
    .await
    .expect("spawn sidecar");
    tokio::time::sleep(Duration::from_millis(30)).await;

    let body = raw_http_get(handle.port, "/impact", &format!("path={rel}"))
        .expect("GET /impact must succeed");

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).expect("restore cwd");

    assert_no_changed_symbols(&body);
}
