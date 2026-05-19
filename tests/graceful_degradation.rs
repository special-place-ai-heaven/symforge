use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use symforge::domain::index::HARD_SKIP_BYTES;
use tempfile::TempDir;

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpClient {
    fn spawn(cwd: &Path, home: &Path) -> Self {
        let binary = env!("CARGO_BIN_EXE_symforge");
        let mut child = Command::new(binary)
            .current_dir(cwd)
            .env("RUST_LOG", "error")
            .env("SYMFORGE_AUTO_INDEX", "false")
            .env("SYMFORGE_NO_DAEMON", "1")
            .env("SYMFORGE_RECONCILE_INTERVAL", "0")
            .env("SYMFORGE_HOME", home)
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
        };
        client.initialize();
        client
    }

    fn initialize(&mut self) {
        let id = self.next_request_id();
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {
                    "name": "symforge-graceful-degradation",
                    "version": "0.0.0"
                }
            }
        }));
        let _ = self.read_response(id);
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }));
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> String {
        let id = self.next_request_id();
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }));
        let response = self.read_response(id);
        if let Some(error) = response.get("error") {
            panic!("MCP tool {name} returned JSON-RPC error: {error}");
        }
        let result = response.get("result").expect("tool result");
        assert_ne!(
            result.get("isError").and_then(Value::as_bool),
            Some(true),
            "MCP tool {name} returned error result: {result}"
        );
        result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find_map(|item| item.get("text")))
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("MCP tool {name} returned no text content: {result}"))
            .to_string()
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

    fn read_response(&mut self, expected_id: u64) -> Value {
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "timed out waiting for JSON-RPC response id {expected_id}"
            );

            let mut line = String::new();
            let bytes = self
                .stdout
                .read_line(&mut line)
                .expect("read JSON-RPC response");
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

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_fixture(root: &Path) {
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(
        src.join("lib.rs"),
        "pub fn target() -> usize { 1 }\n\npub fn caller() -> usize { target() }\n",
    )
    .expect("write tier-1 source");

    let models = root.join("models");
    std::fs::create_dir_all(&models).expect("create models dir");
    std::fs::write(models.join("model.ckpt"), b"artifact").expect("write tier-2 artifact");

    let artifacts = root.join("artifacts");
    std::fs::create_dir_all(&artifacts).expect("create artifacts dir");
    let hard_skip = std::fs::File::create(artifacts.join("huge.bin")).expect("create huge file");
    hard_skip
        .set_len(HARD_SKIP_BYTES + 1)
        .expect("size huge file past hard-skip threshold");
}

#[test]
fn symbol_tools_degrade_for_metadata_only_and_hard_skipped_paths() {
    let root = TempDir::new().expect("fixture root");
    let symforge_home = TempDir::new().expect("isolated symforge home");
    write_fixture(root.path());

    let mut client = McpClient::spawn(root.path(), symforge_home.path());
    let indexed = client.call_tool("index_folder", json!({ "path": root.path() }));
    assert!(
        indexed.contains("Indexed"),
        "index_folder output: {indexed}"
    );

    let tier1_context = client.call_tool(
        "get_symbol_context",
        json!({ "name": "target", "path": "src/lib.rs" }),
    );
    assert!(
        tier1_context.contains("pub fn target() -> usize { 1 }"),
        "Tier 1 get_symbol_context should include the symbol definition: {tier1_context}"
    );
    assert!(
        !tier1_context.contains("metadata-only") && !tier1_context.contains("hard-skipped"),
        "Tier 1 get_symbol_context must not use degraded text: {tier1_context}"
    );

    let tier1_refs = client.call_tool(
        "find_references",
        json!({ "name": "target", "path": "src/lib.rs" }),
    );
    assert!(
        tier1_refs.contains("src/lib.rs"),
        "Tier 1 find_references should keep normal reference output: {tier1_refs}"
    );
    assert!(
        !tier1_refs.contains("metadata-only") && !tier1_refs.contains("hard-skipped"),
        "Tier 1 find_references must not use degraded text: {tier1_refs}"
    );

    for tool in ["get_symbol_context", "find_references"] {
        let tier2 = client.call_tool(
            tool,
            json!({ "name": "target", "path": "models/model.ckpt" }),
        );
        assert!(tier2.contains("Tier 2"), "{tool} Tier 2 output: {tier2}");
        assert!(
            tier2.contains("metadata-only") && tier2.contains("degraded"),
            "{tool} Tier 2 output should say degraded metadata-only: {tier2}"
        );
        assert!(
            tier2.contains("models/model.ckpt") && tier2.contains("Extension: ckpt"),
            "{tool} Tier 2 output should include path and extension: {tier2}"
        );
        assert!(
            tier2.contains("No symbol or reference data"),
            "{tool} Tier 2 output must not claim parsed symbol/reference data: {tier2}"
        );

        let tier3 = client.call_tool(
            tool,
            json!({ "name": "target", "path": "artifacts/huge.bin" }),
        );
        assert!(tier3.contains("Tier 3"), "{tool} Tier 3 output: {tier3}");
        assert!(
            tier3.contains("hard-skipped") && tier3.contains("Reason: >100MB"),
            "{tool} Tier 3 output should include hard-skip reason: {tier3}"
        );
        assert!(
            tier3.contains("No symbol or reference data"),
            "{tool} Tier 3 output must not claim parsed symbol/reference data: {tier3}"
        );
    }
}
