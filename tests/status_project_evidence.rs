//! Drive structured status through the real stdio adapter and daemon transport.
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use symforge::process_util::hidden_command;
use tempfile::TempDir;

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn command(root: &Path, home: &Path) -> Command {
    let mut cmd = hidden_command(env!("CARGO_BIN_EXE_symforge"));
    cmd.current_dir(root)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("SYMFORGE_HOME", home)
        .env("SYMFORGE_SURFACE", "full")
        .env_remove("SYMFORGE_TOOLS")
        .env("SYMFORGE_WORKSPACE_ROOT", root)
        .env("SYMFORGE_RECONCILE_INTERVAL", "0")
        .env("RUST_LOG", "error");
    cmd
}

struct Client {
    _process: Process,
    stdin: ChildStdin,
    responses: Receiver<Value>,
    next_id: u64,
}

impl Client {
    fn start(root: &Path, home: &Path, daemon: bool, bind: bool) -> Self {
        let mut process = Process(
            command(root, home)
                .env("SYMFORGE_NO_DAEMON", if daemon { "0" } else { "1" })
                .env("SYMFORGE_AUTO_INDEX", if bind { "true" } else { "false" })
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("start stdio"),
        );
        let stdin = process.0.stdin.take().unwrap();
        let stdout = process.0.stdout.take().unwrap();
        let (tx, responses) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let message = serde_json::from_str(&line.expect("read JSON-RPC line"))
                    .expect("parse JSON-RPC line");
                if tx.send(message).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            _process: process,
            stdin,
            responses,
            next_id: 1,
        };
        let initialized = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "status-project-evidence", "version": "1"}
            }),
        );
        assert_eq!(initialized["protocolVersion"], "2025-06-18");
        client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        if bind {
            // Let the transport bind without making a status call before the
            // requested test order. A zero-limit/empty index is not our subject.
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                let health = client.call("health", json!({}));
                if evidence(&health)["index_state"] == "Ready" {
                    assert_eq!(evidence(&health)["canonical_root"], root_text(root));
                    break;
                }
                assert!(Instant::now() < deadline, "index never ready: {health}");
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        client
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").expect("write JSON-RPC request");
        self.stdin.flush().expect("flush request");
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let reply = self
                .responses
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|e| panic!("waiting for {method} {id}: {e}"));
            if reply["id"] == id {
                return reply
                    .get("result")
                    .unwrap_or_else(|| panic!("{reply}"))
                    .clone();
            }
        }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }

    fn status(&mut self, detail: &str) -> Value {
        let reply = self.call("status", json!({"detail": detail}));
        assert_eq!(reply["isError"], false, "{detail}: {reply}");
        assert!(reply["structuredContent"].is_object(), "{detail}: {reply}");
        assert_eq!(&reply["structuredContent"], evidence(&reply), "{detail}");
        reply
    }
}

fn evidence(result: &Value) -> &Value {
    &result["_meta"]["symforge/project_evidence"]
}

fn root_text(root: &Path) -> String {
    // The tool renders Windows paths with forward slashes.
    if cfg!(windows) {
        root.to_str().unwrap().replace('\\', "/")
    } else {
        root.to_str().unwrap().to_owned()
    }
}

fn project() -> TempDir {
    let root = TempDir::new().unwrap();
    std::fs::write(root.path().join("lib.rs"), "pub fn sample() {}\n").unwrap();
    root
}

#[test]
fn singleton_daemon_inventory_has_the_same_project_evidence_as_full_status() {
    let home = TempDir::new().unwrap();
    let root_dir = project();
    let root = dunce::canonicalize(root_dir.path()).unwrap();
    // Own the daemon process and use its isolated control state. Starting it
    // explicitly prevents a detached subprocess outliving the test.
    let mut daemon = Process(
        command(&root, home.path())
            .arg("daemon")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .expect("start daemon"),
    );
    let port_file = home
        .path()
        .join("daemon")
        .join(format!("daemon.{}.port", std::env::consts::OS));
    let deadline = Instant::now() + Duration::from_secs(30);
    while !port_file.is_file() {
        assert!(
            daemon.0.try_wait().unwrap().is_none(),
            "daemon exited during startup"
        );
        assert!(Instant::now() < deadline, "daemon did not publish its port");
        std::thread::sleep(Duration::from_millis(25));
    }
    for order in [["projects", "full"], ["full", "projects"]] {
        let mut client = Client::start(&root, home.path(), true, true);
        let first = client.status(order[0]);
        let second = client.status(order[1]);
        let inventory = if order[0] == "projects" {
            &first
        } else {
            &second
        };
        let text = inventory["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("daemon sessions:"),
            "not daemon inventory: {text}"
        );
        for result in [&first, &second] {
            assert_eq!(
                evidence(result)["canonical_root"],
                root_text(&root),
                "{result}"
            );
            assert_eq!(evidence(result)["index_state"], "Ready", "{result}");
        }
        assert_eq!(
            evidence(&first)["project_id"],
            evidence(&second)["project_id"]
        );
        let receipt = evidence(inventory);
        assert!(text.contains(&format!(
            "files={} symbols={} index={}",
            receipt["index_files"],
            receipt["index_symbols"],
            receipt["index_state"].as_str().unwrap()
        )));

        let sibling_dir = project();
        let sibling = dunce::canonicalize(sibling_dir.path()).unwrap();
        let opened = client.call("index_folder", json!({"path": sibling}));
        assert_eq!(opened["isError"], false, "{opened}");
        for detail in order {
            let result = client.status(detail);
            if detail == "projects" {
                let text = result["content"][0]["text"].as_str().unwrap();
                assert!(text.contains(&root_text(&root)) && text.contains(&root_text(&sibling)));
                assert_eq!(
                    evidence(&result),
                    &json!({
                        "bound": false, "reason": "project_evidence_unavailable"
                    })
                );
            } else {
                assert_eq!(
                    evidence(&result)["canonical_root"],
                    root_text(&sibling),
                    "{result}"
                );
            }
        }
    }
}

#[test]
fn local_status_call_order_preserves_bound_and_unbound_evidence() {
    let home = TempDir::new().unwrap();
    let root_dir = project();
    let root = dunce::canonicalize(root_dir.path()).unwrap();
    for bind in [false, true] {
        for order in [["projects", "full"], ["full", "projects"]] {
            let mut client = Client::start(&root, home.path(), false, bind);
            let first = client.status(order[0]);
            let second = client.status(order[1]);
            assert_eq!(
                evidence(&first)["project_id"],
                evidence(&second)["project_id"]
            );
            for result in [&first, &second] {
                let receipt = evidence(result);
                if bind {
                    assert_eq!(receipt["canonical_root"], root_text(&root), "{result}");
                } else {
                    assert_eq!(receipt["project_id"], "unbound", "{result}");
                    assert!(receipt["canonical_root"].is_null(), "{result}");
                }
            }
        }
    }
}
