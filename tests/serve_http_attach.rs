//! US1/T018-T019 — `/mcp` attach surface + dispatch parity + structured BYPASS.
//!
//! ## What is proven end-to-end over HTTP
//!
//! The server is built through the **same** `build_mcp_router` + `apply_bearer_auth`
//! path `serve::run` uses, bound on `127.0.0.1:0` with a Bearer key, and driven with
//! `reqwest` over `/mcp` (stateless + `json_response` transport mode, so each
//! JSON-RPC request returns a single `application/json` response):
//!
//! * **T018 attach** — an authenticated `tools/list` over `/mcp` returns the active
//!   tool surface; the advertised names equal the in-process `list_tools` surface
//!   (FR-005). A representative `tools/call` (`status`) over `/mcp` returns a
//!   success `CallToolResult`.
//! * **T018 parity** — the HTTP `status` `tools/call` result equals
//!   `ServerRuntime::dispatch_tool_call("status", …)` for the **same in-memory
//!   index** (the HTTP transport clones the shared `SymForgeServer`, so both hit one
//!   dispatch path — no logic fork, no economics double-count; FR-005 / SC-006).
//! * **T019 BYPASS** — a P-FF whole-file query routed through the `symforge` facade
//!   returns a **machine-readable** bypass envelope (`--- bypass payload ---` plus the
//!   serialized `StelBypassBody` JSON: `action`/`path`/`reason`), not prose only
//!   (FR-007). Asserted over HTTP and at the dispatch level (same shared state).
//!
//! ## Handshake scope (honest)
//!
//! The transport is stateless (`serve_directly`), so `tools/list` / `tools/call`
//! are served without a prior MCP `initialize` handshake — exercising the real
//! request/response path a remote client uses for each call. A full multi-message
//! `initialize` + session-id handshake is not scripted here; the stateless path is
//! the deliberate transport mode (documented in `mcp_http.rs`) and is what the
//! integration surface depends on.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::server::{
    AuthConfig, AuthLayerState, ServerRuntime, apply_bearer_auth, mcp_http::build_mcp_router,
};
use symforge::sidecar::governor::RequestGovernor;
use symforge::watcher::WatcherInfo;

const TEST_KEY: &str = "sf_attach_key";

fn test_runtime() -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "serve-attach-test".to_string(),
        watcher_info,
        None,
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    ServerRuntime::build_runtime(
        index,
        protocol,
        governor,
        AuthConfig::new(Some(TEST_KEY.to_string())),
        None,
    )
}

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

/// Start the `/mcp` router (auth layered) over `runtime` on `127.0.0.1:0`.
async fn start_server(runtime: ServerRuntime) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback");
    let addr = listener.local_addr().expect("local_addr");

    let router = build_mcp_router(&runtime, addr);
    let auth_state = AuthLayerState::new(runtime.auth().clone(), true);
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

/// POST a JSON-RPC message to `/mcp` with the test Bearer key; parse the JSON-RPC
/// response object. In `json_response` mode the body is a single JSON-RPC object.
async fn mcp_call(url: &str, body: serde_json::Value) -> serde_json::Value {
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Authorization", format!("Bearer {TEST_KEY}"))
        .json(&body)
        .send()
        .await
        .expect("request sent");
    assert!(
        resp.status().is_success(),
        "authenticated /mcp call should succeed, got {}",
        resp.status()
    );
    resp.json::<serde_json::Value>()
        .await
        .expect("response is JSON-RPC json")
}

fn jsonrpc(id: u32, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

/// Extract the first text content block from a `CallToolResult` JSON value.
fn result_text(call_result: &serde_json::Value) -> String {
    call_result["content"][0]["text"]
        .as_str()
        .expect("CallToolResult must carry text content")
        .to_string()
}

// ---------------------------------------------------------------------------
// T018: attach + tools/list surface
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tools_list_over_http_advertises_active_surface() {
    let runtime = test_runtime();
    let server = start_server(runtime).await;

    let response = mcp_call(
        &server.mcp_url(),
        jsonrpc(1, "tools/list", serde_json::json!({})),
    )
    .await;
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools/list result has a tools array");
    assert!(!tools.is_empty(), "advertised surface must be non-empty");

    // The HTTP surface equals the in-process `list_tools` surface (same code path).
    let http_names: std::collections::BTreeSet<String> = tools
        .iter()
        .filter_map(|t| t["name"].as_str().map(str::to_string))
        .collect();
    let expected_names: std::collections::BTreeSet<String> =
        symforge::protocol::surface_probe::list_tools_for_profile(
            symforge::protocol::surface_probe::surface_profile_from_env(),
        )
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    assert_eq!(
        http_names, expected_names,
        "HTTP tools/list surface must equal the in-process list_tools surface"
    );
    // `status` is in every non-empty surface profile.
    assert!(
        http_names.contains("status"),
        "status tool must be advertised"
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// T018: tools/call parity (HTTP result == in-process dispatch result)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_tools_call_parity_http_vs_dispatch() {
    let runtime = test_runtime();

    // In-process dispatch result for `status` on the SAME runtime/index.
    let dispatch_result = runtime
        .dispatch_tool_call("status", serde_json::json!({}))
        .await
        .expect("in-process status dispatch");
    let dispatch_json = serde_json::to_value(&dispatch_result).expect("serialize CallToolResult");
    let dispatch_text = result_text(&dispatch_json);

    let server = start_server(runtime).await;

    // HTTP `tools/call` result for `status`.
    let response = mcp_call(
        &server.mcp_url(),
        jsonrpc(
            2,
            "tools/call",
            serde_json::json!({ "name": "status", "arguments": {} }),
        ),
    )
    .await;
    let http_call_result = &response["result"];
    let http_text = result_text(http_call_result);

    // Parity: the HTTP path and the in-process dispatch hit the same shared
    // SymForgeServer over the same index, so the status body matches.
    assert_eq!(
        http_text, dispatch_text,
        "HTTP tools/call(status) must equal in-process dispatch(status) — one dispatch path"
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// T019: structured BYPASS over /mcp (FR-007)
// ---------------------------------------------------------------------------

/// A P-FF whole-file query (triggers `detect_pff_bypass`) does NOT need a corpus:
/// the controller recognizes "entire/whole/complete <path-with-dot>" and bypasses.
fn pff_symforge_args() -> serde_json::Value {
    serde_json::json!({
        // "entire " + a token containing '.' (src/main.rs) → P-FF bypass.
        "query": "read the entire src/main.rs file line by line"
    })
}

/// The machine-readable bypass envelope markers we require (not prose-only).
fn assert_structured_bypass(text: &str) {
    assert!(
        text.contains("--- bypass payload ---"),
        "bypass output must carry the machine-readable payload marker, got:\n{text}"
    );
    // The serialized StelBypassBody JSON fields must be present.
    assert!(
        text.contains("\"action\":") && text.contains("\"path\":") && text.contains("\"reason\":"),
        "bypass payload must contain structured StelBypassBody fields, got:\n{text}"
    );
    assert!(
        text.contains("host_read"),
        "P-FF bypass action should be host_read, got:\n{text}"
    );
    // Parse the JSON block to prove it is real machine-readable JSON, not prose.
    // The payload is the pretty-printed StelBypassBody after the marker; lines may
    // follow it (e.g. a `ledger:` envelope line), so read exactly one JSON value
    // and tolerate trailing content via a streaming deserializer.
    let marker = "--- bypass payload ---";
    let after_marker = &text[text.find(marker).expect("payload marker present") + marker.len()..];
    let json_start = after_marker
        .find('{')
        .expect("bypass payload has a JSON object");
    let mut stream = serde_json::Deserializer::from_str(&after_marker[json_start..])
        .into_iter::<serde_json::Value>();
    let parsed: serde_json::Value = stream
        .next()
        .expect("a JSON value follows the marker")
        .expect("bypass payload is valid JSON");
    assert_eq!(parsed["action"], "host_read");
    assert!(
        parsed["reason"]
            .as_str()
            .is_some_and(|r| r.contains("policy=P-FF")),
        "reason must name policy=P-FF"
    );
}

#[tokio::test]
async fn bypass_over_http_returns_structured_envelope() {
    // The `symforge` STEL facade is only active on the compact surface.
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let runtime = test_runtime();
    let server = start_server(runtime).await;

    let response = mcp_call(
        &server.mcp_url(),
        jsonrpc(
            3,
            "tools/call",
            serde_json::json!({ "name": "symforge", "arguments": pff_symforge_args() }),
        ),
    )
    .await;
    let text = result_text(&response["result"]);
    assert_structured_bypass(&text);

    server.shutdown().await;
}

#[tokio::test]
async fn bypass_at_dispatch_returns_structured_envelope() {
    // Same assertion at the dispatch level (corpus-free, deterministic) — proves
    // the structured signal is produced by the shared handler, not the transport.
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let runtime = test_runtime();
    let result = runtime
        .dispatch_tool_call("symforge", pff_symforge_args())
        .await
        .expect("symforge dispatch");
    let json = serde_json::to_value(&result).expect("serialize CallToolResult");
    let text = result_text(&json);
    assert_structured_bypass(&text);
}

// ---------------------------------------------------------------------------
// C-stopgap (D16): /mcp loudly REFUSES cross-project targeting
// ---------------------------------------------------------------------------

/// The cross-project working set lives only on the stdio daemon. On the `/mcp`
/// HTTP transport there is NO daemon behind the shared `SymForgeServer`
/// (`test_runtime` builds it via `SymForgeServer::new`, so `daemon_client` is
/// `None`, and `proxy_tool_call` short-circuits to `None`). The three
/// cross-project-capable tools (`search_symbols`/`search_text`/`find_references`,
/// whose inputs carry `project`/`projects`) must therefore LOUDLY REFUSE a
/// cross-project target over `/mcp` instead of silently dropping it and serving
/// the single bound index — D16's silent-wrong half, contained by
/// `local_cross_project_refusal`.
///
/// The complementary stdio+daemon HONOR path is proven by
/// `daemon::tests::test_cross_project_query_returns_attributed_hits_from_both_projects`
/// (attributed cross-project hits; a no-params call stays single-project). This
/// test locks the `/mcp` refusal end-to-end over the real HTTP transport.
#[tokio::test]
async fn cross_project_targeting_is_refused_over_http() {
    // The individual cross-project-capable tools are only advertised + dispatched
    // on the FULL surface; the compact surface exposes only the `symforge` facade
    // (which refuses cross-project via its own guard, D9). The compact-surface
    // `/mcp` path is thus covered transitively by that facade refusal, which has
    // its own test (`tools::tests::symforge_facade_rejects_cross_project_targeting`).
    // Hold the process-global surface lock so the env opt-in is deterministic.
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("full");

    let runtime = test_runtime(); // SymForgeServer::new -> daemon_client = None (the /mcp shape)
    let server = start_server(runtime).await;
    let url = server.mcp_url();

    // The refusal prefix emitted by `local_cross_project_refusal`.
    const REFUSAL_PREFIX: &str = "Cross-project queries (project/projects) require";

    let cross_project_calls = [
        (
            10u32,
            "search_symbols",
            serde_json::json!({ "query": "thing", "projects": ["*"] }),
        ),
        (
            11,
            "search_text",
            serde_json::json!({ "query": "thing", "project": "other-proj" }),
        ),
        (
            12,
            "find_references",
            serde_json::json!({ "name": "thing", "projects": ["a", "b"] }),
        ),
    ];

    for (id, name, arguments) in cross_project_calls {
        let response = mcp_call(
            &url,
            jsonrpc(
                id,
                "tools/call",
                serde_json::json!({ "name": name, "arguments": arguments }),
            ),
        )
        .await;
        let text = result_text(&response["result"]);
        assert!(
            text.contains(REFUSAL_PREFIX),
            "{name} over /mcp must LOUDLY refuse cross-project targeting (no silent \
             single-project drop), got:\n{text}"
        );
        // The refusal must name the /mcp transport so it is honest for the caller.
        assert!(
            text.contains("/mcp HTTP transport"),
            "{name} refusal should name the /mcp transport, got:\n{text}"
        );
    }

    // Control: the SAME tool with NO project/projects must NOT trip the refusal —
    // it serves the single bound index on the normal path (no false refusal on the
    // default single-project route).
    let response = mcp_call(
        &url,
        jsonrpc(
            13,
            "tools/call",
            serde_json::json!({ "name": "search_symbols", "arguments": { "query": "thing" } }),
        ),
    )
    .await;
    let text = result_text(&response["result"]);
    assert!(
        !text.contains(REFUSAL_PREFIX),
        "a single-project search_symbols (no project/projects) must NOT trip the \
         cross-project refusal, got:\n{text}"
    );

    server.shutdown().await;
}
