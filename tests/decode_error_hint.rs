//! A tool-argument decode failure must tell the caller which fields the tool
//! requires, not only what serde tripped over. Driven through the real binary so
//! the assertion covers the `call_tool` seam, not just the helper.

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

fn send(stdin: &mut std::process::ChildStdin, message: Value) {
    serde_json::to_writer(&mut *stdin, &message).expect("write JSON-RPC message");
    stdin.write_all(b"\n").expect("write newline");
    stdin.flush().expect("flush");
}

fn read_response(stdout: &mut impl BufRead, id: u64) -> Value {
    let started = Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "timed out waiting for response id {id}"
        );
        let mut line = String::new();
        let bytes = stdout.read_line(&mut line).expect("read response");
        assert_ne!(bytes, 0, "server closed stdout before response id {id}");
        let message: Value = serde_json::from_str(line.trim())
            .unwrap_or_else(|error| panic!("invalid JSON-RPC line {line:?}: {error}"));
        if message.get("id").and_then(Value::as_u64) == Some(id) {
            return message;
        }
    }
}

#[test]
fn missing_required_field_names_the_required_fields() {
    let cwd = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let mut child = symforge::process_util::hidden_command(env!("CARGO_BIN_EXE_symforge"))
        .current_dir(cwd.path())
        .env("RUST_LOG", "error")
        .env("SYMFORGE_AUTO_INDEX", "false")
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_RECONCILE_INTERVAL", "0")
        .env("SYMFORGE_HOME", home.path())
        .env("SYMFORGE_SURFACE", "full")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn symforge MCP server");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "decode-error-hint", "version": "0.0.0"}
        }}),
    );
    let _ = read_response(&mut stdout, 1);
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}),
    );
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "find_references", "arguments": {}
        }}),
    );
    let response = read_response(&mut stdout, 2);
    let _ = child.kill();
    let _ = child.wait();

    let result = response
        .get("result")
        .expect("tool result, not a JSON-RPC error");
    assert_eq!(
        result["isError"],
        json!(true),
        "decode failure is an error result: {result}"
    );
    let text = result["content"][0]["text"].as_str().expect("text content");
    assert!(
        text.starts_with("failed to deserialize parameters:") && text.contains("missing field"),
        "serde text must stay first: {text}"
    );
    assert!(
        text.contains("Required fields for `find_references`: name."),
        "the required field names must be appended: {text}"
    );
}
