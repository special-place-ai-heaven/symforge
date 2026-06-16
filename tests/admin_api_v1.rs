//! 006 US1/US3 — `/api/v1/*` returns real data; auth + Origin enforced.
//!
//! The admin router is built and layered through the **same** path
//! `serve::run` uses (`build_admin_router` + `apply_origin_gate` +
//! `apply_bearer_auth`), bound on `127.0.0.1:0`, and driven with `reqwest`:
//!
//! * **Real data (T010 / SC-001 fallback)** — with a seeded in-memory ledger,
//!   `GET /api/v1/summary` returns the seeded economics; `GET /api/v1/surface`
//!   returns the active tool surface; `GET /api/v1/harness` returns the host
//!   scan.
//! * **Auth (SC-002)** — a keyed runtime rejects an unauthenticated request
//!   with 401 (same Bearer contract as `/mcp`).
//! * **Origin (SC-006)** — a request carrying a disallowed browser `Origin` is
//!   refused (403); a same-origin request (or no Origin) is allowed.
//! * **Unavailable state (SC-004)** — a `Disabled` ledger renders
//!   `available = false` with no fabricated numbers.
#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::server::{
    AuthConfig, AuthLayerState, OriginLayerState, ServerRuntime, admin::build_admin_router,
    apply_bearer_auth, apply_origin_gate,
};
use symforge::sidecar::governor::RequestGovernor;
use symforge::stel::ledger_store::StelLedgerStore;
use symforge::stel::types::{AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent};
use symforge::watcher::WatcherInfo;

const TEST_KEY: &str = "sf_admin_key";

fn seeded_ledger() -> StelLedgerStore {
    let store = StelLedgerStore::open_in_memory("admin-it").expect("in-memory ledger");
    for i in 0..3 {
        store.record(&StelLedgerEvent {
            ts_ms: 1_000 + i,
            plan_id: format!("plan-{i}"),
            surface: "symforge".into(),
            intent: IntentBucket::Trace,
            decision: AdmissionDecision::Serve,
            tools_called: vec!["find_references".into()],
            predicted_response_tokens: 100,
            actual_response_tokens: 90,
            manual_baseline_tokens: 300,
            net_vs_manual: 210,
            equivalence: None,
            route_confidence: RouteConfidence::Exact,
            pff_bypass: None,
            cache_hit: None,
            degrade_flags: vec![],
        });
    }
    store
}

fn runtime(auth: AuthConfig, ledger: Option<StelLedgerStore>) -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "admin-it-project".to_string(),
        watcher_info,
        None,
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    ServerRuntime::build_runtime(index, protocol, governor, auth, ledger)
}

struct TestServer {
    addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
}

impl TestServer {
    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
    fn own_origin(&self) -> String {
        format!("http://{}", self.addr)
    }
    async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.await;
    }
}

/// Start the admin router (origin-gated + auth-layered) on `127.0.0.1:0`,
/// exactly mirroring `serve::run`'s layering order.
async fn start(runtime: ServerRuntime, auth: AuthConfig, is_loopback: bool) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback");
    let addr = listener.local_addr().expect("local_addr");

    let admin = build_admin_router(&runtime);
    let origin_state = OriginLayerState::from_bind_addr(addr);
    let gated = apply_origin_gate(admin, origin_state);
    let mut auth_state = AuthLayerState::new(auth, is_loopback);
    if let Some(store) = runtime.key_store() {
        auth_state = auth_state.with_key_store(Arc::clone(store));
    }
    let app = apply_bearer_auth(gated, auth_state);

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

async fn get_json(
    url: &str,
    bearer: Option<&str>,
    origin: Option<&str>,
) -> (reqwest::StatusCode, serde_json::Value) {
    let client = reqwest::Client::new();
    let mut req = client.get(url).header("Accept", "application/json");
    if let Some(key) = bearer {
        req = req.header("Authorization", format!("Bearer {key}"));
    }
    if let Some(o) = origin {
        req = req.header("Origin", o);
    }
    let resp = req.send().await.expect("request sent");
    let status = resp.status();
    let body = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or(serde_json::Value::Null);
    (status, body)
}

// ---------------------------------------------------------------------------
// Real data
// ---------------------------------------------------------------------------

#[tokio::test]
async fn summary_returns_seeded_economics() {
    // Loopback + no key → auth open; seeded ledger → real values.
    let rt = runtime(AuthConfig::new(None), Some(seeded_ledger()));
    let server = start(rt, AuthConfig::new(None), true).await;

    let (status, body) = get_json(&server.url("/api/v1/summary"), None, None).await;
    assert!(status.is_success(), "summary should be 200, got {status}");
    assert_eq!(body["available"], true);
    assert_eq!(body["total_events"], 3);
    assert_eq!(body["total_net_vs_manual"], 630); // 3 * 210

    server.shutdown().await;
}

#[tokio::test]
async fn surface_and_harness_return_structured_data() {
    let rt = runtime(AuthConfig::new(None), Some(seeded_ledger()));
    let server = start(rt, AuthConfig::new(None), true).await;

    let (status, surface) = get_json(&server.url("/api/v1/surface"), None, None).await;
    assert!(status.is_success());
    assert!(surface["tools"].as_array().is_some_and(|t| !t.is_empty()));
    assert!(surface["tool_count"].as_u64().is_some());

    let (status, harness) = get_json(&server.url("/api/v1/harness"), None, None).await;
    assert!(status.is_success());
    // The registry resolves on any host; `available` is a bool and `entries` an array.
    assert!(harness["available"].is_boolean());
    assert!(harness["entries"].is_array());

    server.shutdown().await;
}

#[tokio::test]
async fn system_returns_real_pid() {
    let rt = runtime(AuthConfig::new(None), None);
    let server = start(rt, AuthConfig::new(None), true).await;

    let (status, system) = get_json(&server.url("/api/v1/system"), None, None).await;
    assert!(status.is_success());
    assert_eq!(system["pid"].as_u64(), Some(u64::from(std::process::id())));
    assert_eq!(system["active_sessions"], 1);

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// Unavailable / empty states (SC-004)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disabled_ledger_renders_unavailable_not_fake_zeros() {
    let rt = runtime(AuthConfig::new(None), Some(StelLedgerStore::Disabled));
    let server = start(rt, AuthConfig::new(None), true).await;

    let (status, body) = get_json(&server.url("/api/v1/summary"), None, None).await;
    assert!(status.is_success());
    assert_eq!(body["available"], false);
    assert!(body["total_events"].is_null(), "no fabricated zeros");
    assert!(body["total_net_vs_manual"].is_null());

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// Auth (SC-002)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unauth_keyed_request_is_rejected() {
    // Keyed runtime, loopback flag false to simulate a non-loopback (auth always
    // required). No Bearer → 401.
    let auth = AuthConfig::new(Some(TEST_KEY.to_string()));
    let rt = runtime(auth.clone(), Some(seeded_ledger()));
    let server = start(rt, auth, false).await;

    let (status, _) = get_json(&server.url("/api/v1/summary"), None, None).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);

    // Correct key → success.
    let (status, body) = get_json(&server.url("/api/v1/summary"), Some(TEST_KEY), None).await;
    assert!(status.is_success());
    assert_eq!(body["available"], true);

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// Origin gating (SC-006 / P1-B)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disallowed_origin_is_rejected() {
    let rt = runtime(AuthConfig::new(None), Some(seeded_ledger()));
    let server = start(rt, AuthConfig::new(None), true).await;

    // A cross-origin browser fetch (attacker page) is refused.
    let (status, _) = get_json(
        &server.url("/api/v1/summary"),
        None,
        Some("http://evil.example.com"),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::FORBIDDEN,
        "disallowed Origin must be rejected (P1-B)"
    );

    // Same-origin request is allowed.
    let own = server.own_origin();
    let (status, body) = get_json(&server.url("/api/v1/summary"), None, Some(&own)).await;
    assert!(status.is_success(), "same-origin must be allowed");
    assert_eq!(body["available"], true);

    server.shutdown().await;
}

#[tokio::test]
async fn admin_html_is_served() {
    let rt = runtime(AuthConfig::new(None), None);
    let server = start(rt, AuthConfig::new(None), true).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(server.url("/admin"))
        .send()
        .await
        .expect("request sent");
    assert!(resp.status().is_success());
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(ct.contains("text/html"), "admin index is HTML, got {ct}");
    let html = resp.text().await.expect("html body");
    assert!(html.contains("/admin/app.js"));
    assert!(html.contains("SymForge Admin"));

    server.shutdown().await;
}
