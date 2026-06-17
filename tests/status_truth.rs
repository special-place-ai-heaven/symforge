// Server-only integration test: depends on `#[cfg(feature = "server")]`
// daemon machinery. Gating the whole file keeps
// `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! T016 / TR-01 / FR-006 / FR-007 (SC-002) — status truth over the daemon path.
//!
//! Pins the daemon-side `status` dispatch arm added in
//! `src/daemon.rs::execute_tool_call`. Before the fix the daemon had NO
//! `status` arm at all — a `status` call to the daemon returned
//! `unknown tool 'status'` — and the front-end `status` read its own empty
//! index, so the readout reported a working system as broken.
//!
//! This test drives the REAL daemon HTTP path an MCP client reaches the daemon
//! through (the same transport `daemon_aliases.rs` exercises):
//!   1. spawn a daemon, open a session;
//!   2. `index_folder` over HTTP so the DAEMON's index is warm;
//!   3. `search_symbols` over HTTP to confirm a query serves the indexed symbol;
//!   4. `status` over HTTP and assert the readout reports `index_ready: true`
//!      with a non-zero file count.
//!
//! Coverage limits (honest): this exercises the daemon's per-session tool
//! dispatch over loopback HTTP — the production data path between the
//! front-end proxy and the warm daemon. The complementary in-crate test
//! `src/daemon.rs::test_status_index_matches_daemon_proxy_after_symforge_serve`
//! additionally drives the front-end `new_daemon_proxy` server (the part that
//! decides to proxy `status` instead of reading its empty `self.index`). What
//! NEITHER test covers — and what must be live-verified against a built 8.0.0
//! binary — is the OS desktop launcher / CWD project discovery (TR-03) and
//! cross-binary version skew.

use std::time::Duration;

use symforge::daemon::{OpenProjectRequest, OpenProjectResponse, spawn_daemon};
use tempfile::TempDir;

fn write_fixture(project_root: &std::path::Path) {
    let src = project_root.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(src.join("lib.rs"), "pub fn served_symbol() -> u32 { 42 }\n")
        .expect("write lib.rs");
}

async fn wait_for_shutdown(port: u16) {
    for _ in 0..40 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[allow(unsafe_code)] // test-only daemon home / auth override scoped to this async test.
#[tokio::test]
async fn status_index_matches_daemon_after_index_over_http() {
    let daemon_home = TempDir::new().expect("daemon home temp dir");
    let auth_token = "status-truth-test-token";
    // SAFETY: integration test binaries run single-threaded
    // (`--test-threads=1`) and in their own process; no concurrent env access.
    unsafe {
        std::env::set_var("SYMFORGE_HOME", daemon_home.path());
        std::env::set_var("SYMFORGE_DAEMON_AUTH_TOKEN", auth_token);
    }

    let project = TempDir::new().expect("project temp dir");
    write_fixture(project.path());

    let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", handle.port);

    let opened: OpenProjectResponse = client
        .post(format!("{base_url}/v1/sessions/open"))
        .bearer_auth(auth_token)
        .json(&OpenProjectRequest {
            project_root: project.path().display().to_string(),
            client_name: "status-truth-test".to_string(),
            pid: Some(9201),
        })
        .send()
        .await
        .expect("open session request")
        .error_for_status()
        .expect("open session status")
        .json()
        .await
        .expect("open session body");

    // 2. Index the project on the daemon (warm the served index).
    let index_body = client
        .post(format!(
            "{base_url}/v1/sessions/{}/tools/index_folder",
            opened.session_id
        ))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({ "path": project.path().display().to_string() }))
        .send()
        .await
        .expect("index_folder request")
        .error_for_status()
        .expect("index_folder status")
        .text()
        .await
        .expect("index_folder body");
    assert!(
        index_body.starts_with("Indexed "),
        "daemon index_folder must succeed, got: {index_body}"
    );

    // 3. A query must serve the indexed symbol from the warm index.
    let query_body = client
        .post(format!(
            "{base_url}/v1/sessions/{}/tools/search_symbols",
            opened.session_id
        ))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({ "query": "served_symbol" }))
        .send()
        .await
        .expect("search_symbols request")
        .error_for_status()
        .expect("search_symbols status")
        .text()
        .await
        .expect("search_symbols body");
    assert!(
        query_body.contains("served_symbol"),
        "query must serve the indexed symbol from the warm daemon, got: {query_body}"
    );

    // 4. status over the daemon HTTP path must report the served index.
    //    Pre-fix this returned `unknown tool 'status'`.
    let status_body = client
        .post(format!(
            "{base_url}/v1/sessions/{}/tools/status",
            opened.session_id
        ))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("status request")
        .error_for_status()
        .expect("status status")
        .text()
        .await
        .expect("status body");

    assert!(
        !status_body.contains("unknown tool"),
        "daemon must have a `status` dispatch arm (TR-01), got: {status_body}"
    );
    assert!(
        status_body.contains("index_ready: true"),
        "status must report the served daemon index as ready (FR-006), got:\n{status_body}"
    );
    assert!(
        !status_body.contains("index_files: 0"),
        "status must report a non-zero file count for the served index (SC-002), got:\n{status_body}"
    );
    assert!(
        status_body.contains("index_files: 1\n"),
        "status must report exactly the single indexed file from the daemon, got:\n{status_body}"
    );

    let _ = handle.shutdown_tx.send(());
    wait_for_shutdown(handle.port).await;
}
