// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Sidecar caller-root guard (dogfood #6 / spec 012 FR-006b, hook half).
//!
//! When another agent's `index_folder` retargets the shared session, the
//! sidecar's index no longer belongs to the caller's repo. A hook request
//! pinned with `caller_root` must get a 409 (so the hook falls back to the
//! daemon, which resolves the project BY ROOT) — never a false "not found"
//! report from the wrong project.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use symforge::live_index::LiveIndex;
use symforge::sidecar::spawn_sidecar;
use tempfile::TempDir;

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn raw_http_get_with_status(
    port: u16,
    path: &str,
    query: &str,
) -> anyhow::Result<(String, String)> {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let timeout = Duration::from_millis(1000);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let request_path = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };
    let request = format!(
        "GET {request_path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let status_line = response.lines().next().unwrap_or("").to_string();
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("")
        .to_string();
    Ok((status_line, body))
}

async fn spawn_repo_sidecar() -> (TempDir, symforge::sidecar::SidecarHandle) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn keep() {}\n").expect("write");
    let index = LiveIndex::load(dir.path()).expect("LiveIndex::load");
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar");
    tokio::time::sleep(Duration::from_millis(20)).await;
    (dir, handle)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mismatched_caller_root_gets_409_not_wrong_project_answer() {
    let (_repo, handle) = spawn_repo_sidecar().await;
    let other = tempfile::tempdir().expect("other tempdir");

    let query = format!(
        "path=src/lib.rs&caller_root={}",
        url_encode(&other.path().to_string_lossy())
    );
    let (status, body) =
        raw_http_get_with_status(handle.port, "/outline", &query).expect("GET /outline");
    assert!(
        status.contains("409"),
        "a wrong-root caller must get 409, not an answer from another project; got {status}: {body}"
    );
    assert!(
        body.contains("rooted at"),
        "the 409 body must name both roots; got: {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn matching_caller_root_passes_through() {
    let (repo, handle) = spawn_repo_sidecar().await;
    let query = format!(
        "path=src/lib.rs&caller_root={}",
        url_encode(&repo.path().to_string_lossy())
    );
    let (status, _body) =
        raw_http_get_with_status(handle.port, "/outline", &query).expect("GET /outline");
    assert!(
        status.contains("200"),
        "the caller's own root must pass the guard; got {status}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absent_caller_root_stays_backward_compatible() {
    let (_repo, handle) = spawn_repo_sidecar().await;
    let (status, _body) =
        raw_http_get_with_status(handle.port, "/outline", "path=src/lib.rs").expect("GET /outline");
    assert!(
        status.contains("200"),
        "requests without caller_root must behave as before; got {status}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_is_exempt_from_root_guard() {
    let (_repo, handle) = spawn_repo_sidecar().await;
    let other = tempfile::tempdir().expect("other tempdir");
    let query = format!(
        "caller_root={}",
        url_encode(&other.path().to_string_lossy())
    );
    let (status, _body) =
        raw_http_get_with_status(handle.port, "/health", &query).expect("GET /health");
    assert!(
        status.contains("200"),
        "/health must stay root-agnostic (liveness + hook fail-open target); got {status}"
    );
}
