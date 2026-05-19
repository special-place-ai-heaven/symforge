//! Pins every backward-compat alias in `src/daemon.rs::execute_tool_call`.
//!
//! ADR 0001 codifies that aliases are permanent. An alias is a promise to
//! every client that learned the old name: "this tool still routes to the
//! same handler". If the destination handler is renamed, its input shape
//! drifts, or the alias branch is silently dropped, the alias breaks for
//! every agent session that still uses the old name — and nothing in CI
//! notices, because aliases aren't exposed via `tools/list`.
//!
//! Each alias below has one test that:
//!   1. Calls the tool by its **old** name via the MCP dispatch HTTP path
//!      (the same path an MCP client reaches the daemon through).
//!   2. Asserts no "unknown tool" error and no dispatch-layer failure.
//!   3. Asserts the response is byte-identical to calling the **destination**
//!      handler with the exact input that the alias branch translates to —
//!      the strongest possible "alias still routes to the same handler"
//!      contract. Any divergence means the alias and its destination have
//!      drifted apart.
//!
//! Enumerated aliases (source of truth: `src/daemon.rs::execute_tool_call`):
//!   - `trace_symbol` → `get_symbol_context` (sections=None translates to
//!     `Some(vec![])`, which switches `get_symbol_context` into trace mode).
//!
//! When a new alias is added or removed in `execute_tool_call`, update this
//! file in the same commit. A new alias without a test here is a silent
//! regression waiting to happen.

use std::time::Duration;

use symforge::daemon::{OpenProjectRequest, OpenProjectResponse, spawn_daemon};
use tempfile::TempDir;

/// Minimal fixture project: one Rust file with one function symbol.
/// Enough for `trace_symbol`/`get_symbol_context` to return a meaningful
/// trace-mode response without depending on external files.
fn write_fixture(project_root: &std::path::Path) {
    let src = project_root.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("write main.rs");
}

/// Poll the daemon port until the TCP listener stops accepting connections,
/// signalling graceful shutdown. Bounded to avoid hanging the test binary
/// if shutdown stalls.
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

/// Pin the `trace_symbol` → `get_symbol_context` alias.
///
/// `execute_tool_call` translates `trace_symbol` input into a
/// `GetSymbolContextInput` with `sections = Some(vec![])` (which puts
/// `get_symbol_context` into trace mode). If either side of that translation
/// drifts — e.g., a field is added to `GetSymbolContextInput` without
/// matching the alias branch, or `get_symbol_context` changes how
/// `sections = Some(empty)` behaves — the two responses diverge and this
/// test fails loudly.
#[allow(unsafe_code)] // test-only daemon home override is scoped to this async test.
#[tokio::test]
async fn trace_symbol_alias_routes_to_get_symbol_context() {
    let daemon_home = TempDir::new().expect("daemon home temp dir");
    // This binary's tests run with `--test-threads=1` (see CLAUDE.md), so the
    // env var does not race with sibling tests in the same binary. Integration
    // test binaries run in their own process, so they do not share env state
    // with other test binaries either.
    // SAFETY: single-threaded test binary; no concurrent env access.
    unsafe {
        std::env::set_var("SYMFORGE_HOME", daemon_home.path());
    }

    let project = TempDir::new().expect("project temp dir");
    write_fixture(project.path());

    let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", handle.port);

    let opened: OpenProjectResponse = client
        .post(format!("{base_url}/v1/sessions/open"))
        .json(&OpenProjectRequest {
            project_root: project.path().display().to_string(),
            client_name: "daemon-aliases-test".to_string(),
            pid: Some(9101),
        })
        .send()
        .await
        .expect("open session request")
        .error_for_status()
        .expect("open session status")
        .json()
        .await
        .expect("open session body");

    // --- Call the OLD name: trace_symbol -----------------------------------
    let alias_resp = client
        .post(format!(
            "{base_url}/v1/sessions/{}/tools/trace_symbol",
            opened.session_id
        ))
        .json(&serde_json::json!({
            "path": "src/main.rs",
            "name": "main",
        }))
        .send()
        .await
        .expect("trace_symbol alias request");

    assert!(
        alias_resp.status().is_success(),
        "trace_symbol alias HTTP status must be 2xx, got {}",
        alias_resp.status(),
    );
    let alias_body = alias_resp.text().await.expect("trace_symbol alias body");
    assert!(
        !alias_body.contains("unknown tool"),
        "trace_symbol alias must not return 'unknown tool' — it is a permanent \
         backward-compat alias (ADR 0001). Got body: {alias_body}"
    );
    assert!(
        !alias_body.starts_with("Error in trace_symbol:"),
        "trace_symbol dispatch layer must not error — the alias branch in \
         execute_tool_call is responsible for translating input. Got body: \
         {alias_body}"
    );
    assert!(
        alias_body.contains("main"),
        "trace_symbol response must mention the traced symbol `main`. Got \
         body: {alias_body}"
    );

    // --- Call the DESTINATION with the translated input --------------------
    //
    // `execute_tool_call`'s `trace_symbol` branch maps `{path, name}` to
    // `GetSymbolContextInput { name, path: Some(path), sections: Some(vec![]),
    // ..Default::default()-equivalent }`. Reproduce that input exactly here.
    // `sections: []` in the JSON deserializes to `Some(vec![])`, which the
    // handler treats as "trace mode, all sections".
    let destination_resp = client
        .post(format!(
            "{base_url}/v1/sessions/{}/tools/get_symbol_context",
            opened.session_id
        ))
        .json(&serde_json::json!({
            "name": "main",
            "path": "src/main.rs",
            "sections": [],
        }))
        .send()
        .await
        .expect("get_symbol_context destination request");

    assert!(
        destination_resp.status().is_success(),
        "get_symbol_context destination HTTP status must be 2xx, got {}",
        destination_resp.status(),
    );
    let destination_body = destination_resp
        .text()
        .await
        .expect("get_symbol_context destination body");

    // --- Byte-identical parity is the alias contract ----------------------
    assert_eq!(
        alias_body,
        destination_body,
        "trace_symbol alias output must be byte-identical to \
         get_symbol_context with the translated input.\n\
         Divergence here means the alias branch in \
         src/daemon.rs::execute_tool_call has drifted from its destination \
         handler — an ADR 0001 violation.\n\
         alias bytes:       {}\n\
         destination bytes: {}",
        alias_body.len(),
        destination_body.len(),
    );

    let _ = handle.shutdown_tx.send(());
    wait_for_shutdown(handle.port).await;
}
