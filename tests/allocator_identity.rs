//! Issue #876: `health` names the allocator serving the shipped binary, and
//! only what it observed. The binary installs jemalloc on Linux glibc and the
//! system allocator elsewhere; the health line must say jemalloc exactly where
//! a probe allocation went through it, and say "not identified" everywhere
//! else rather than guess.
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

fn send(stdin: &mut impl Write, message: Value) {
    serde_json::to_writer(&mut *stdin, &message).expect("write JSON-RPC message");
    stdin.write_all(b"\n").expect("write newline");
    stdin.flush().expect("flush");
}

fn response(stdout: &mut impl BufRead, id: u64) -> Value {
    let started = Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "no response {id}"
        );
        let mut line = String::new();
        assert_ne!(
            stdout.read_line(&mut line).expect("read"),
            0,
            "stdout closed"
        );
        let message: Value = serde_json::from_str(line.trim()).expect("JSON-RPC line");
        if message.get("id").and_then(Value::as_u64) == Some(id) {
            return message;
        }
    }
}

#[test]
fn health_reports_the_allocator_it_observed() {
    let workspace = tempfile::tempdir().expect("workspace");
    std::fs::write(workspace.path().join("lib.rs"), "pub fn anchor() {}\n").expect("seed");
    let home = tempfile::tempdir().expect("home");
    let mut child = symforge::process_util::hidden_command(env!("CARGO_BIN_EXE_symforge"))
        .current_dir(workspace.path())
        .env("RUST_LOG", "error")
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", home.path())
        .env("SYMFORGE_SURFACE", "full")
        .env("SYMFORGE_WORKSPACE_ROOT", workspace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn symforge");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "allocator-identity", "version": "0"}}}),
    );
    response(&mut stdout, 1);
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}),
    );
    // Early calls may land while the index is still loading and answer with a
    // loading banner instead of the report; ask again until it is ready.
    let started = Instant::now();
    let mut id = 2;
    let text = loop {
        send(
            &mut stdin,
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": {"name": "health", "arguments": {}}}),
        );
        let reply = response(&mut stdout, id);
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("health text: {reply}"))
            .to_string();
        if text.contains("process_resident_bytes") || started.elapsed() > Duration::from_secs(60) {
            break text;
        }
        id += 1;
        std::thread::sleep(Duration::from_millis(250));
    };
    let _ = child.kill();
    let _ = child.wait();

    let line = text
        .lines()
        .find(|line| line.starts_with("allocator"))
        .unwrap_or_else(|| panic!("no allocator line in health:\n{text}"));
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
        assert!(
            line.starts_with("allocator=jemalloc version=5.")
                && line.ends_with(" background_thread=true"),
            "{line}"
        );
    } else {
        assert_eq!(line, "allocator: not identified (only jemalloc is probed)");
    }
}
