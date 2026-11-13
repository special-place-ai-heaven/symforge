//! US1/T017 — Bearer auth contract for `symforge serve` over `/mcp`.
//!
//! Proves the secure-by-default rule end-to-end against a live HTTP server:
//! (a) a non-loopback bind with no key **refuses to start** (precondition,
//!     before any socket opens);
//! (b) key configured + **missing** Bearer → HTTP 401;
//! (c) key configured + **wrong** Bearer → HTTP 401;
//! (d) key configured + **correct** Bearer → not 401 (the request reaches the
//!     MCP transport; a tool actually executes).
//!
//! The server is built through the **same** `build_mcp_router` + `apply_bearer_auth`
//! path `serve::run` uses (one auth-enforcement point, one dispatch path), bound on
//! `127.0.0.1:0`, and probed with `reqwest`. Auth is the contract under test, so the
//! assertions key on 401-vs-not-401 of the response status.
#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::server::serve::{ServeArgs, ServeError, run};
use symforge::server::{
    AuthConfig, AuthLayerState, ServerRuntime, apply_bearer_auth, mcp_http::build_mcp_router,
};
use symforge::sidecar::governor::RequestGovernor;
use symforge::watcher::WatcherInfo;

/// Build a minimal runtime over an empty in-memory index with the given auth.
fn test_runtime(auth: AuthConfig) -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "serve-auth-test".to_string(),
        watcher_info,
        None,
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    ServerRuntime::build_runtime(index, protocol, governor, auth, None)
}

/// A running test server: the bound address plus a shutdown signal.
struct TestServer {
    addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
}

impl TestServer {
    fn mcp_url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.await;
    }
}

/// Start the `/mcp` router (auth layered) on `127.0.0.1:0` in a background task.
///
/// `is_loopback` is `true` here (loopback bind), so with no key auth is skipped;
/// with a key it is enforced — matching `serve::run`'s wiring.
async fn start_server(auth: AuthConfig) -> TestServer {
    let runtime = test_runtime(auth.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback");
    let addr = listener.local_addr().expect("local_addr");

    let router = build_mcp_router(&runtime, addr);
    let auth_state = AuthLayerState::new(auth, true);
    let app = apply_bearer_auth(router, auth_state);

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let shutdown = async {
            let _ = rx.await;
        };
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
    });

    TestServer {
        addr,
        shutdown: Some(tx),
        join,
    }
}

/// A well-formed MCP `tools/list` JSON-RPC request body.
fn tools_list_body() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    })
}

/// POST a `tools/list` to `/mcp` with optional Bearer header; return the status.
async fn post_tools_list(url: &str, bearer: Option<&str>) -> reqwest::StatusCode {
    let client = reqwest::Client::new();
    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&tools_list_body());
    if let Some(key) = bearer {
        req = req.header("Authorization", format!("Bearer {key}"));
    }
    req.send().await.expect("request sent").status()
}

// (a) Refuse-to-start: non-loopback bind + no key → ServeError::Startup, before bind.
#[tokio::test]
async fn non_loopback_without_key_refuses_to_start() {
    let args = ServeArgs {
        listen: "0.0.0.0:8787".to_string(),
        explicit_listen: true,
        api_key: None,
        api_key_env: None,
        bound_addr_tx: None,
        workspace_root: None,
        ..ServeArgs::default()
    };
    let err = run(args)
        .await
        .expect_err("non-loopback + no key must refuse to start");
    assert!(
        matches!(err, ServeError::Startup(_)),
        "expected refuse-to-start, got {err:?}"
    );
}

// (b) Key configured + missing Bearer → 401.
#[tokio::test]
async fn missing_bearer_with_key_is_unauthorized() {
    let server = start_server(AuthConfig::new(Some("sf_test_key".to_string()))).await;
    let status = post_tools_list(&server.mcp_url(), None).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
    server.shutdown().await;
}

// (c) Key configured + wrong Bearer → 401.
#[tokio::test]
async fn wrong_bearer_with_key_is_unauthorized() {
    let server = start_server(AuthConfig::new(Some("sf_test_key".to_string()))).await;
    let status = post_tools_list(&server.mcp_url(), Some("sf_wrong_key")).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
    server.shutdown().await;
}

// (d) Key configured + correct Bearer → NOT 401 (request reaches the transport).
#[tokio::test]
async fn correct_bearer_with_key_is_authorized() {
    let server = start_server(AuthConfig::new(Some("sf_test_key".to_string()))).await;
    let status = post_tools_list(&server.mcp_url(), Some("sf_test_key")).await;
    assert_ne!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "correct Bearer must not be rejected by the auth layer"
    );
    assert!(
        status.is_success(),
        "authorized tools/list should succeed (json_response mode), got {status}"
    );
    server.shutdown().await;
}

// No key + loopback bind → auth skipped (request reaches the transport).
#[tokio::test]
async fn no_key_loopback_skips_auth() {
    let server = start_server(AuthConfig::new(None)).await;
    let status = post_tools_list(&server.mcp_url(), None).await;
    assert_ne!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "no key + loopback must not require auth"
    );
    server.shutdown().await;
}

// (e) #874: a bad tool allow-list refuses `serve` before any bind (exit 2).
// Each case runs the real binary on loopback, so the `--tools` flag (the input
// that wins over `SYMFORGE_TOOLS`) is exercised end to end.
fn serve_refusal(args: &[&str], tools_env: Option<std::ffi::OsString>) -> (Option<i32>, String) {
    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");
    let mut command = symforge::process_util::hidden_command(env!("CARGO_BIN_EXE_symforge"));
    command
        .current_dir(cwd.path())
        .env("SYMFORGE_HOME", home.path())
        .env("SYMFORGE_NO_DAEMON", "1")
        .env_remove("SYMFORGE_SURFACE")
        .env_remove("SYMFORGE_TOOLS")
        .args(["serve", "--listen", "127.0.0.1:0"])
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    if let Some(value) = tools_env {
        command.env("SYMFORGE_TOOLS", value);
    }
    // Bounded: a refusal is immediate, so a child still running past the
    // deadline means startup did NOT refuse; kill it and fail loudly.
    const REFUSAL_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);
    let mut child = command.spawn().expect("spawn symforge serve");
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll symforge serve") {
            break status;
        }
        if started.elapsed() > REFUSAL_DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("symforge serve {args:?} did not refuse within {REFUSAL_DEADLINE:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().expect("piped stderr"), &mut stderr)
        .expect("read stderr");
    (status.code(), stderr)
}

#[test]
fn serve_tools_flag_with_unknown_name_refuses_to_start() {
    let (code, stderr) = serve_refusal(&["--tools", "bogus"], None);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("bogus"), "{stderr}");
}

#[test]
fn serve_tools_flag_blank_refuses_instead_of_serving_everything() {
    // A restrictive env list must not be hidden by a blank flag.
    let (code, stderr) = serve_refusal(&["--tools", " "], Some("status".into()));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("blank"), "{stderr}");
}

#[test]
fn serve_refuses_a_tools_env_that_is_not_utf8() {
    #[cfg(windows)]
    let invalid = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0xD800])
    };
    #[cfg(unix)]
    let invalid = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![0xff])
    };
    let (code, stderr) = serve_refusal(&[], Some(invalid));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("not valid UTF-8"), "{stderr}");
}
