//! Protocol lifecycle x per-request `_meta` acceptance matrix, driven against
//! the REAL binary over stdio.
//!
//! Regression for the 2026-09-25 harness failure where Claude Code sessions
//! ended with no tools: a client opened with `server/discover`, timed out,
//! fell back to `initialize`, and every later `tools/list`, `resources/list`,
//! and `prompts/list` without `_meta` was rejected as "request _meta is
//! missing or has malformed required fields". rmcp latches a per-session
//! "request metadata required" flag on a discover opener and never cleared it
//! after a successful `initialize`; its in-loop `initialize` also recorded the
//! REQUESTED protocol version as peer info instead of the negotiated one, so
//! `ping` on a session negotiated down from 2026-07-28 answered
//! method-not-found. Both are patched in `vendor/rmcp` (see
//! `vendor/rmcp/SYMFORGE-PATCH.md`).
//!
//! 54 rows: {init, discover_then_init} x 4 initialize versions x 6 `_meta`
//! shapes, plus discover_only x 6 shapes. Each row asserts four methods.
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

const PV: &str = "io.modelcontextprotocol/protocolVersion";
const CC: &str = "io.modelcontextprotocol/clientCapabilities";
const CI: &str = "io.modelcontextprotocol/clientInfo";
const METHODS: [&str; 4] = ["tools/list", "resources/list", "prompts/list", "ping"];
const INIT_VERSIONS: [&str; 4] = ["2024-11-05", "2025-06-18", "2025-11-25", "2026-07-28"];
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

fn full_modern() -> Value {
    json!({ PV: "2026-07-28", CC: {}, CI: { "name": "matrix", "version": "1" } })
}

fn metas() -> Vec<(&'static str, Option<Value>)> {
    vec![
        ("none", None),
        ("pv_modern", Some(json!({ PV: "2026-07-28" }))),
        ("pv_legacy", Some(json!({ PV: "2025-11-25" }))),
        ("caps_only", Some(json!({ CC: {} }))),
        ("full_modern", Some(full_modern())),
        ("full_legacy", Some(json!({ PV: "2025-11-25", CC: {} }))),
    ]
}

/// The contract after the fix:
/// - a request whose own `_meta` names 2026-07-28 is an inline-lifecycle
///   request and must carry clientCapabilities (spec-correct rejection);
/// - a session whose client completed `initialize` is a session-lifecycle
///   session even if `server/discover` came first, so no per-request metadata
///   is required;
/// - a discover-only session demands version + capabilities on every request;
/// - `ping` exists only on the legacy lifecycle.
fn expected(lifecycle: &str, meta: &str, method: &str) -> &'static str {
    let inline_request =
        lifecycle == "discover_only" || matches!(meta, "pv_modern" | "full_modern");
    if method == "ping" {
        return if inline_request { "ERR" } else { "OK" };
    }
    if lifecycle == "discover_only" {
        return if matches!(meta, "full_modern" | "full_legacy") {
            "OK"
        } else {
            "ERR"
        };
    }
    if meta == "pv_modern" { "ERR" } else { "OK" }
}

struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Value>,
    next_id: u64,
}

impl Session {
    fn spawn(repo: &TempDir, home: &TempDir) -> Self {
        let mut child = symforge::process_util::hidden_command(env!("CARGO_BIN_EXE_symforge"))
            .current_dir(repo.path())
            .env("RUST_LOG", "error")
            .env("SYMFORGE_AUTO_INDEX", "false")
            .env("SYMFORGE_NO_DAEMON", "1")
            .env("SYMFORGE_RECONCILE_INTERVAL", "0")
            .env("SYMFORGE_HOME", home.path())
            .env("SYMFORGE_SURFACE", "full")
            .env_remove("SYMFORGE_WORKSPACE_ROOT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn symforge MCP server");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let (tx, lines) = mpsc::channel();
        // Reader thread: a hung server surfaces as a recv timeout, not a hung test.
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                if let Ok(message) = serde_json::from_str::<Value>(line.trim())
                    && tx.send(message).is_err()
                {
                    return;
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
            next_id: 1,
        }
    }

    fn send(&mut self, message: &Value) -> bool {
        serde_json::to_writer(&mut self.stdin, message).is_ok()
            && self.stdin.write_all(b"\n").is_ok()
            && self.stdin.flush().is_ok()
    }

    /// One-line outcome: `OK ...`, `ERR <message>`, `TIMEOUT`, or `DEAD`.
    fn call(&mut self, method: &str, params: Value) -> String {
        let id = self.next_id;
        self.next_id += 1;
        if !self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })) {
            return "DEAD(stdin)".to_string();
        }
        loop {
            let message = match self.lines.recv_timeout(CALL_TIMEOUT) {
                Ok(message) => message,
                Err(mpsc::RecvTimeoutError::Timeout) => return "TIMEOUT".to_string(),
                Err(mpsc::RecvTimeoutError::Disconnected) => return "DEAD(eof)".to_string(),
            };
            if message.get("method").and_then(Value::as_str) == Some("roots/list")
                && let Some(request_id) = message.get("id").cloned()
            {
                self.send(
                    &json!({ "jsonrpc": "2.0", "id": request_id, "result": { "roots": [] } }),
                );
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return format!(
                    "ERR {}",
                    error.get("message").and_then(Value::as_str).unwrap_or("")
                );
            }
            let result = message.get("result").cloned().unwrap_or(Value::Null);
            return match result.get("protocolVersion").and_then(Value::as_str) {
                Some(version) => format!("OK pv={version}"),
                None => "OK".to_string(),
            };
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn lifecycle_and_request_meta_matrix_matches_contract() {
    let repo = TempDir::new().expect("fixture repo");
    std::fs::create_dir(repo.path().join(".git")).expect("plant .git");
    std::fs::write(repo.path().join("lib.rs"), "pub fn matrix() {}\n").expect("seed file");
    let home = TempDir::new().expect("isolated symforge home");

    let mut rows = 0;
    let mut red = Vec::new();
    for lifecycle in ["init", "discover_then_init", "discover_only"] {
        let versions: &[&str] = if lifecycle == "discover_only" {
            &["-"]
        } else {
            &INIT_VERSIONS
        };
        for &init_version in versions {
            for (meta_name, meta) in metas() {
                rows += 1;
                let mut session = Session::spawn(&repo, &home);
                let mut steps = Vec::new();
                if lifecycle.starts_with("discover") {
                    let outcome =
                        session.call("server/discover", json!({ "_meta": full_modern() }));
                    steps.push(format!("discover:{outcome}"));
                }
                if lifecycle != "discover_only" {
                    let outcome = session.call(
                        "initialize",
                        json!({
                            "protocolVersion": init_version,
                            "capabilities": {},
                            "clientInfo": { "name": "matrix", "version": "1" }
                        }),
                    );
                    steps.push(format!("init:{outcome}"));
                    session
                        .send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
                }
                let params = meta
                    .as_ref()
                    .map_or_else(|| json!({}), |meta| json!({ "_meta": meta }));
                let got: Vec<String> = METHODS
                    .iter()
                    .map(|method| session.call(method, params.clone()))
                    .collect();
                let row_red = METHODS.iter().zip(&got).any(|(method, outcome)| {
                    !outcome.starts_with(expected(lifecycle, meta_name, method))
                });
                if row_red {
                    red.push(format!(
                        "{lifecycle} init={init_version} meta={meta_name} [{}] -> {}",
                        steps.join(" | "),
                        METHODS
                            .iter()
                            .zip(&got)
                            .map(|(method, outcome)| format!("{method}:{outcome}"))
                            .collect::<Vec<_>>()
                            .join(" ; ")
                    ));
                }
            }
        }
    }

    assert_eq!(rows, 54, "the matrix must cover all 54 rows");
    assert!(
        red.is_empty(),
        "{} of 54 rows violate the lifecycle contract:\n{}",
        red.len(),
        red.join("\n")
    );
}
