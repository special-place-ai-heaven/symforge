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

use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use symforge::live_index::LiveIndex;
use symforge::sidecar::spawn_sidecar;
use tempfile::TempDir;

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .no_proxy()
        .build()
        .expect("build bounded sidecar test HTTP client")
});

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

async fn http_get_with_status(
    port: u16,
    path: &str,
    query: &str,
) -> anyhow::Result<(String, String)> {
    let mut url = format!("http://127.0.0.1:{port}{path}");
    if !query.is_empty() {
        url.push('?');
        url.push_str(query);
    }
    let response = HTTP_CLIENT.get(url).send().await?;
    let status_code = response.status();
    let status = format!(
        "HTTP/1.1 {} {}",
        status_code.as_u16(),
        status_code.canonical_reason().unwrap_or_default()
    );
    let body = response.text().await?;
    Ok((status, body))
}

async fn spawn_repo_sidecar() -> (TempDir, symforge::sidecar::SidecarHandle) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn keep() {}\n").expect("write");
    let index = LiveIndex::load(dir.path()).expect("LiveIndex::load");
    let control_state = symforge::domain::ControlStateDir::new(dir.path().join("control-state"));
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None, Some(control_state))
        .await
        .expect("spawn_sidecar");
    (dir, handle)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mismatched_caller_root_gets_409_not_wrong_project_answer() {
    let (repo, handle) = spawn_repo_sidecar().await;
    let other = tempfile::tempdir().expect("other tempdir");

    let query = format!(
        "path=src/lib.rs&caller_root={}",
        url_encode(&other.path().to_string_lossy())
    );
    let (status, body) = http_get_with_status(handle.port, "/outline", &query)
        .await
        .expect("GET /outline");
    assert!(
        status.contains("409"),
        "a wrong-root caller must get 409, not an answer from another project; got {status}: {body}"
    );
    let repo_text = repo.path().to_string_lossy().to_string();
    let canonical = repo.path().canonicalize().expect("canonical repo");
    assert!(
        body.contains("different project")
            && !body.contains(&repo_text)
            && !body.contains(canonical.to_string_lossy().as_ref()),
        "the 409 body must not disclose the indexed root; got: {body}"
    );
}

async fn get_with_headers(port: u16, path: &str, headers: &[(&str, &str)]) -> anyhow::Result<u16> {
    let mut request = HTTP_CLIENT.get(format!("http://127.0.0.1:{port}{path}"));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    Ok(request.send().await?.status().as_u16())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_sidecar_refuses_foreign_host_and_any_origin() {
    let (_repo, handle) = spawn_repo_sidecar().await;
    let port = handle.port;
    let loopback = format!("127.0.0.1:{port}");
    let localhost = format!("localhost:{port}");
    for path in ["/outline?path=src/lib.rs", "/health", "/stats"] {
        assert_eq!(
            get_with_headers(port, path, &[("host", &format!("attacker.example:{port}"))])
                .await
                .expect("foreign host request"),
            403,
            "{path}: a foreign Host must be refused"
        );
        for origin in ["http://attacker.example", "http://127.0.0.1", "null"] {
            assert_eq!(
                get_with_headers(port, path, &[("host", &loopback), ("origin", origin)])
                    .await
                    .expect("origin request"),
                403,
                "{path}: a request carrying Origin {origin} must be refused"
            );
        }
        for host in [&loopback, &localhost] {
            assert_eq!(
                get_with_headers(port, path, &[("host", host)])
                    .await
                    .expect("loopback host request"),
                200,
                "{path}: Host {host} must pass"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn matching_caller_root_passes_through() {
    let (repo, handle) = spawn_repo_sidecar().await;
    let query = format!(
        "path=src/lib.rs&caller_root={}",
        url_encode(&repo.path().to_string_lossy())
    );
    let (status, _body) = http_get_with_status(handle.port, "/outline", &query)
        .await
        .expect("GET /outline");
    assert!(
        status.contains("200"),
        "the caller's own root must pass the guard; got {status}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absent_caller_root_stays_backward_compatible() {
    let (_repo, handle) = spawn_repo_sidecar().await;
    let (status, _body) = http_get_with_status(handle.port, "/outline", "path=src/lib.rs")
        .await
        .expect("GET /outline");
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
    let (status, _body) = http_get_with_status(handle.port, "/health", &query)
        .await
        .expect("GET /health");
    assert!(
        status.contains("200"),
        "/health must stay root-agnostic (liveness + hook fail-open target); got {status}"
    );
}
