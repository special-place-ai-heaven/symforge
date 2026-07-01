// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Pins every backward-compat alias in `src/daemon.rs::execute_tool_call`.
//!
//! Aliases are deliberate compatibility routes for clients that learned an old
//! name. Retired aliases must warn explicitly while still routing to the
//! translated destination payload. If the destination handler is renamed, its
//! input shape drifts, or the alias branch is silently dropped, the alias
//! breaks for every agent session that still uses the old name — and nothing
//! in CI notices, because aliases aren't exposed via `tools/list`.
//!
//! Each alias below has one test that:
//!   1. Calls the tool by its **old** name via the MCP dispatch HTTP path
//!      (the same path an MCP client reaches the daemon through).
//!   2. Asserts no "unknown tool" error and no dispatch-layer failure.
//!   3. Asserts the response contains any required deprecation warning and
//!      preserves the **destination** handler payload for the exact input that
//!      the alias branch translates to.
//!
//! Enumerated aliases (source of truth: `src/daemon.rs::execute_tool_call`):
//!   - `trace_symbol` → `get_symbol_context` (sections=None translates to
//!     `Some(vec![])`, which switches `get_symbol_context` into trace mode)
//!     with an explicit deprecation warning.
//!   - `detect_changes` → `detect_impact` (same input shape, D-015-012) with
//!     an explicit deprecation warning.
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

/// Pin the deprecated `trace_symbol` → `get_symbol_context` alias.
///
/// `execute_tool_call` translates `trace_symbol` input into a
/// `GetSymbolContextInput` with `sections = Some(vec![])` (which puts
/// `get_symbol_context` into trace mode). If either side of that translation
/// drifts — e.g., a field is added to `GetSymbolContextInput` without
/// matching the alias branch, or `get_symbol_context` changes how
/// `sections = Some(empty)` behaves — the post-warning payload diverges and
/// this test fails loudly.
#[allow(unsafe_code)] // test-only daemon home override is scoped to this async test.
#[tokio::test]
async fn trace_symbol_alias_routes_to_get_symbol_context() {
    let daemon_home = TempDir::new().expect("daemon home temp dir");
    // This binary's tests run with `--test-threads=1` (see CLAUDE.md), so the
    // env var does not race with sibling tests in the same binary. Integration
    // test binaries run in their own process, so they do not share env state
    // with other test binaries either.
    //
    // The daemon is now fail-closed: it ALWAYS requires an auth token. Pin a
    // known token via the env var so this test (which exercises the real MCP
    // dispatch path) can present it with `.bearer_auth`, mirroring how the real
    // MCP front-end authenticates against the daemon.
    let auth_token = "daemon-aliases-test-token";
    // SAFETY: single-threaded test binary; no concurrent env access.
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
        .bearer_auth(auth_token)
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
        "trace_symbol alias must not return 'unknown tool' while retained for \
         compatibility. Got body: {alias_body}"
    );
    let deprecation_warning = concat!(
        "Deprecation warning: `trace_symbol` is retired; ",
        "use `get_symbol_context` with `sections=[...]` or `find_references` instead. ",
        "Compatibility policy: keep daemon alias through v7.x; planned removal in v8.0."
    );
    assert!(
        alias_body.starts_with(deprecation_warning),
        "trace_symbol alias must emit an explicit deprecation warning. Got body: {alias_body}"
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
        .bearer_auth(auth_token)
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

    // --- Deprecated alias preserves destination payload after warning ------
    let alias_payload = alias_body
        .strip_prefix(deprecation_warning)
        .and_then(|body| body.strip_prefix("\n\n"))
        .expect("trace_symbol alias should return warning plus destination payload");
    assert_eq!(
        alias_payload,
        destination_body.as_str(),
        "trace_symbol alias payload must match get_symbol_context with the \
         translated input after removing the deprecation warning.\n\
         Divergence here means the alias branch in \
         src/daemon.rs::execute_tool_call has drifted from its destination \
         handler.\n\
         alias payload bytes: {}\n\
         destination bytes: {}",
        alias_payload.len(),
        destination_body.len(),
    );

    let _ = handle.shutdown_tx.send(());
    wait_for_shutdown(handle.port).await;
}

/// Pin the `detect_changes` → `detect_impact` alias (D-015-012, CBM migrator
/// ergonomics).
///
/// `execute_tool_call` routes the old CBM tool name `detect_changes` to the
/// real `detect_impact` handler with the SAME input shape, prefixed with an
/// explicit deprecation warning. If the alias branch is dropped, or its input
/// decoding drifts from `detect_impact`'s, this test fails loudly. The fixture
/// project has no `.git`, so both the alias and the destination hit the same
/// "Git unavailable" error path — sufficient to pin routing without needing a
/// bootstrapped git history here.
#[allow(unsafe_code)] // test-only daemon home override is scoped to this async test.
#[tokio::test]
async fn detect_changes_alias_routes_to_detect_impact() {
    let daemon_home = TempDir::new().expect("daemon home temp dir");
    let auth_token = "daemon-aliases-detect-changes-test-token";
    // SAFETY: single-threaded test binary; no concurrent env access.
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
            client_name: "daemon-aliases-detect-changes-test".to_string(),
            pid: Some(9102),
        })
        .send()
        .await
        .expect("open session request")
        .error_for_status()
        .expect("open session status")
        .json()
        .await
        .expect("open session body");

    // --- Call the OLD name: detect_changes ---------------------------------
    let alias_resp = client
        .post(format!(
            "{base_url}/v1/sessions/{}/tools/detect_changes",
            opened.session_id
        ))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("detect_changes alias request");

    assert!(
        alias_resp.status().is_success(),
        "detect_changes alias HTTP status must be 2xx, got {}",
        alias_resp.status(),
    );
    let alias_body = alias_resp.text().await.expect("detect_changes alias body");
    assert!(
        !alias_body.contains("unknown tool"),
        "detect_changes alias must not return 'unknown tool' while retained for \
         compatibility. Got body: {alias_body}"
    );
    let deprecation_warning = concat!(
        "Deprecation warning: `detect_changes` is a CBM-compatibility alias; ",
        "use `detect_impact` instead. Compatibility policy: kept for CBM migrator ",
        "ergonomics (decision-log D-015-012); no removal date set."
    );
    assert!(
        alias_body.starts_with(deprecation_warning),
        "detect_changes alias must emit an explicit deprecation warning. Got body: {alias_body}"
    );

    // --- Call the DESTINATION with the same input ---------------------------
    let destination_resp = client
        .post(format!(
            "{base_url}/v1/sessions/{}/tools/detect_impact",
            opened.session_id
        ))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("detect_impact destination request");

    assert!(
        destination_resp.status().is_success(),
        "detect_impact destination HTTP status must be 2xx, got {}",
        destination_resp.status(),
    );
    let destination_body = destination_resp
        .text()
        .await
        .expect("detect_impact destination body");

    // --- Deprecated alias preserves destination payload after warning ------
    let alias_payload = alias_body
        .strip_prefix(deprecation_warning)
        .and_then(|body| body.strip_prefix("\n\n"))
        .expect("detect_changes alias should return warning plus destination payload");
    assert_eq!(
        alias_payload,
        destination_body.as_str(),
        "detect_changes alias payload must match detect_impact for the same input \
         after removing the deprecation warning.\n\
         Divergence here means the alias branch in \
         src/daemon.rs::execute_tool_call has drifted from its destination handler.\n\
         alias payload bytes: {}\n\
         destination bytes: {}",
        alias_payload.len(),
        destination_body.len(),
    );
    assert!(
        destination_body.contains("Git unavailable"),
        "fixture project has no .git — both alias and destination should surface \
         the git-unavailable error. Got: {destination_body}"
    );

    let _ = handle.shutdown_tx.send(());
    wait_for_shutdown(handle.port).await;
}
