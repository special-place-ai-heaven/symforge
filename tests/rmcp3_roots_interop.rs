//! SC-316b (spec 025, owner test 17) — legacy stdio roots adapter interop.
//!
//! A legacy-lifecycle stdio client drives the REAL binary end-to-end:
//! `initialize` (declaring the `roots` capability) → `notifications/initialized`
//! → answers the server's `roots/list` request → the workspace is bound from
//! client roots exactly as today (US2 heritage), verified through the FR-319
//! evidence channel on a subsequent `tools/call`.
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

/// Local copy of the crate's pub(crate) root normalizer: dunce-canonicalize
/// with an identity fallback for paths that do not exist.
fn normalize_root(root: &std::path::Path) -> std::path::PathBuf {
    dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

struct RootsClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
    declared_root: String,
    answered_roots_list: bool,
}

impl RootsClient {
    fn spawn(cwd: &Path, home: &Path, declared_root: &Path) -> Self {
        let binary = env!("CARGO_BIN_EXE_symforge");
        let mut child = symforge::process_util::hidden_command(binary)
            .current_dir(cwd)
            .env("RUST_LOG", "error")
            .env("SYMFORGE_AUTO_INDEX", "false")
            .env("SYMFORGE_NO_DAEMON", "1")
            .env("SYMFORGE_RECONCILE_INTERVAL", "0")
            .env("SYMFORGE_HOME", home)
            .env("SYMFORGE_SURFACE", "full")
            .env_remove("SYMFORGE_WORKSPACE_ROOT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn symforge MCP server");

        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let uri = format!(
            "file:///{}",
            declared_root.display().to_string().replace('\\', "/")
        );
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            declared_root: uri,
            answered_roots_list: false,
        }
    }

    fn handshake(&mut self) {
        let id = self.next_request_id();
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": { "roots": {} },
                "clientInfo": { "name": "sc316b-roots-client", "version": "0.0.0" }
            }
        }));
        let _ = self.read_until_response(id);
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }));
    }

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

    /// Read until the response with `expected_id` arrives, ANSWERING any
    /// server-initiated `roots/list` request seen along the way — the whole
    /// point of the legacy adapter.
    fn read_until_response(&mut self, expected_id: u64) -> Value {
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(20),
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

            if message.get("method").and_then(Value::as_str) == Some("roots/list") {
                let request_id = message
                    .get("id")
                    .cloned()
                    .expect("roots/list carries an id");
                let root = self.declared_root.clone();
                self.write_message(json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": { "roots": [{ "uri": root, "name": "declared-root" }] }
                }));
                self.answered_roots_list = true;
                continue;
            }

            if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
                return message;
            }
        }
    }
}

impl Drop for RootsClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn legacy_stdio_client_roots_bind_the_workspace() {
    // The launch CWD is a throwaway dir (a home-CWD launcher stand-in); the
    // client's DECLARED root is a different repo, which per the 012 D4-A
    // precedence (env > roots > CWD walk) must win over the CWD-derived bind.
    let launch_cwd = TempDir::new().expect("launch cwd");
    let home = TempDir::new().expect("isolated symforge home");
    let repo = TempDir::new().expect("declared repo root");
    std::fs::create_dir(repo.path().join(".git")).expect("plant .git");
    std::fs::write(repo.path().join("lib.rs"), "pub fn rooted() {}\n").expect("seed file");
    // dunce, not std: std::fs::canonicalize yields a `\\?\` UNC path on
    // Windows, which would corrupt the `file:///` root URI the client declares.
    let repo_root = dunce::canonicalize(repo.path()).expect("canonical repo root");

    let mut client = RootsClient::spawn(launch_cwd.path(), home.path(), &repo_root);
    client.handshake();

    // The roots exchange is asynchronous to the handshake; poll the FR-319
    // evidence channel until the declared root is the bound workspace.
    let started = Instant::now();
    let bound_root = loop {
        let result = client.call_tool_result("status", json!({}));
        let disclosed = result
            .get("_meta")
            .and_then(|meta| meta.get("symforge/project_evidence"))
            .and_then(|evidence| evidence.get("canonical_root"))
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(root) = &disclosed
            && normalize_root(Path::new(root)) == normalize_root(&repo_root)
        {
            break root.clone();
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "client roots never bound the workspace; last evidence root: {disclosed:?}"
        );
        std::thread::sleep(Duration::from_millis(250));
    };

    assert!(
        client.answered_roots_list,
        "the server must have solicited roots/list after notifications/initialized"
    );
    assert_eq!(
        normalize_root(Path::new(&bound_root)),
        normalize_root(&repo_root),
        "workspace must be bound from the declared client root"
    );
}
