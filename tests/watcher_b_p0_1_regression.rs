use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;

const FILE_COUNT: usize = 32;
const WATCHER_STARTUP_WAIT: Duration = Duration::from_millis(350);
const STALE_EVENT_WINDOW: Duration = Duration::from_millis(1_500);
const FILE_COUNT_TOLERANCE: usize = 2;

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
                    "name": "symforge-b-p0-1-regression",
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
                started.elapsed() < Duration::from_secs(10),
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

fn write_fixture(root: &Path, marker: &str) {
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    for index in 0..FILE_COUNT {
        let function_name = format!("{marker}_shared_{index:03}");
        std::fs::write(
            src.join(format!("shared_{index:03}.rs")),
            format!("pub fn {function_name}() -> usize {{ {index} }}\n"),
        )
        .expect("write fixture source");
    }
}

fn delete_fixture_sources(root: &Path) {
    for index in 0..FILE_COUNT {
        std::fs::remove_file(root.join("src").join(format!("shared_{index:03}.rs")))
            .expect("remove root A source");
    }
}

fn assert_indexed(output: &str, expected_files: usize) {
    assert!(
        output.starts_with(&format!("Indexed {expected_files} files,")),
        "expected {expected_files} indexed files, got: {output}"
    );
}

fn health_file_count(client: &mut McpClient) -> usize {
    let health = client.call_tool("health_compact", json!({}));
    parse_file_count(&health).unwrap_or_else(|| panic!("health output has no file count: {health}"))
}

fn parse_file_count(health: &str) -> Option<usize> {
    let after_marker = health.split_once("Files: ")?.1;
    after_marker.split_whitespace().next()?.parse().ok()
}

#[test]
fn reload_cross_root_preserves_file_count_public_api() {
    let root_a = TempDir::new().expect("root A tempdir");
    let root_b = TempDir::new().expect("root B tempdir");
    let symforge_home = TempDir::new().expect("isolated symforge home");
    write_fixture(root_a.path(), "a");
    write_fixture(root_b.path(), "b");

    let mut client = McpClient::spawn(root_a.path(), symforge_home.path());

    let index_a = client.call_tool("index_folder", json!({ "path": root_a.path() }));
    assert_indexed(&index_a, FILE_COUNT);
    assert_eq!(health_file_count(&mut client), FILE_COUNT);

    std::thread::sleep(WATCHER_STARTUP_WAIT);

    let index_b = client.call_tool("index_folder", json!({ "path": root_b.path() }));
    assert_indexed(&index_b, FILE_COUNT);
    let b_count_initial = health_file_count(&mut client);
    assert_eq!(b_count_initial, FILE_COUNT);

    delete_fixture_sources(root_a.path());

    let deadline = Instant::now() + STALE_EVENT_WINDOW;
    let mut lowest_count = b_count_initial;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        lowest_count = lowest_count.min(health_file_count(&mut client));
    }

    assert!(
        lowest_count + FILE_COUNT_TOLERANCE >= b_count_initial,
        "stale root A watcher event destroyed root B index: initial={b_count_initial}, lowest={lowest_count}, tolerance={FILE_COUNT_TOLERANCE}"
    );
}
