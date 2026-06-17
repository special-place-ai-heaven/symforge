//! 006 US1 — `/admin` render verification (T011 / SC-001 / FR-008).
//!
//! ## Render-evidence method: documented reqwest-level fallback (HONEST)
//!
//! The task spec asks for a headless-browser render check that confirms real
//! economics values appear in the DOM. **No Rust headless-browser dependency is
//! present** in this crate (`Cargo.toml` has no `chromiumoxide` /
//! `headless_chrome` / `fantoccini`), and feature 006's research.md committed to
//! **zero new dependencies**. Driving the system Chrome from the Rust test suite
//! would require adding such a crate (heavy, CI-brittle) — out of scope here.
//!
//! Therefore this test uses the **explicitly-sanctioned fallback** from
//! `tasks.md` T011: assert that
//!   1. the served `/admin` HTML and `/admin/app.js` reference every `/api/v1`
//!      endpoint the dashboard fetches and render-binds (so a browser loading
//!      this page WILL request and display them), AND
//!   2. a `reqwest` fetch of `/api/v1/summary` returns the **seeded** economics
//!      values the dashboard binds into its cards.
//!
//! This proves the data is real and the wiring is correct end-to-end at the HTTP
//! layer. It does NOT claim a real browser painted the pixels — a live headless
//! render against the running `serve` (Chrome is installed on the host) is the
//! recommended manual/CI verification and is recorded in
//! `specs/006-v8-admin-gui/validation.md`.
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

fn seeded_ledger(net_each: i32, n: u64) -> StelLedgerStore {
    let store = StelLedgerStore::open_in_memory("render-it").expect("ledger");
    for i in 0..n {
        store.record(&StelLedgerEvent {
            ts_ms: 1_000 + i,
            plan_id: format!("p-{i}"),
            surface: "symforge".into(),
            intent: IntentBucket::Trace,
            decision: AdmissionDecision::Serve,
            tools_called: vec!["find_references".into()],
            predicted_response_tokens: 100,
            actual_response_tokens: 90,
            manual_baseline_tokens: 300,
            net_vs_manual: net_each,
            equivalence: None,
            route_confidence: RouteConfidence::Exact,
            pff_bypass: None,
            cache_hit: None,
            degrade_flags: vec![],
        });
    }
    store
}

fn runtime(ledger: StelLedgerStore) -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "render-it-project".to_string(),
        watcher_info,
        None,
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    ServerRuntime::build_runtime(
        index,
        protocol,
        governor,
        AuthConfig::new(None),
        Some(ledger),
    )
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
    async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.await;
    }
}

async fn start(runtime: ServerRuntime, auth: AuthConfig, is_loopback: bool) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let admin = build_admin_router(&runtime);
    let gated = apply_origin_gate(admin, OriginLayerState::from_bind_addr(addr));
    let app = apply_bearer_auth(gated, AuthLayerState::new(auth, is_loopback));
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

const KEYED_LOOPBACK_KEY: &str = "sf_admin_render_key";

async fn start_keyed_loopback(runtime: ServerRuntime) -> TestServer {
    let auth = AuthConfig::new(Some(KEYED_LOOPBACK_KEY.to_string()));
    start(runtime, auth, true).await
}

#[tokio::test]
async fn admin_page_references_endpoints_and_summary_has_real_values() {
    let store = seeded_ledger(210, 4);
    let server = start(runtime(store), AuthConfig::new(None), true).await;
    let client = reqwest::Client::new();

    // (1) The served HTML references the JS + CSS the browser will load.
    let html = client
        .get(server.url("/admin"))
        .send()
        .await
        .expect("html req")
        .text()
        .await
        .expect("html body");
    assert!(html.contains("/admin/app.js"), "HTML loads app.js");
    assert!(html.contains("/admin/style.css"), "HTML loads style.css");
    // The dashboard view containers the JS binds into must exist.
    assert!(html.contains("id=\"economics\""));
    assert!(html.contains("id=\"surface\""));
    assert!(html.contains("id=\"system\""));

    // (2) The served JS references every /api/v1 endpoint the dashboard fetches
    // and binds into DOM cards (so a browser loading this WILL render them).
    let js = client
        .get(server.url("/admin/app.js"))
        .send()
        .await
        .expect("js req")
        .text()
        .await
        .expect("js body");
    for endpoint in ["/summary", "/surface", "/harness", "/system", "/keys"] {
        assert!(js.contains(endpoint), "app.js fetches {endpoint}");
    }
    // The JS binds the real economics fields into cards (the values rendered).
    assert!(js.contains("total_events"));
    assert!(js.contains("total_net_vs_manual"));

    // (3) The endpoint the dashboard binds returns the SEEDED real values
    // (not placeholders / zeros) — the data a browser would paint.
    let summary = client
        .get(server.url("/api/v1/summary"))
        .send()
        .await
        .expect("summary req")
        .json::<serde_json::Value>()
        .await
        .expect("summary json");
    assert_eq!(summary["available"], true);
    assert_eq!(summary["total_events"], 4);
    assert_eq!(summary["total_net_vs_manual"], 840); // 4 * 210

    server.shutdown().await;
}

#[tokio::test]
async fn keyed_loopback_admin_static_assets_load_without_bearer_api_stays_gated() {
    // P2-1 / P3-10: with --api-key on loopback, HTML/JS load without Bearer; API
    // routes still require the key.
    let store = seeded_ledger(100, 1);
    let server = start_keyed_loopback(runtime(store)).await;
    let client = reqwest::Client::new();

    for path in ["/admin", "/admin/app.js", "/admin/style.css"] {
        let resp = client
            .get(server.url(path))
            .send()
            .await
            .unwrap_or_else(|_| panic!("request {path}"));
        assert!(
            resp.status().is_success(),
            "keyed loopback static {path} must load without Bearer, got {}",
            resp.status()
        );
    }

    let summary_unauth = client
        .get(server.url("/api/v1/summary"))
        .send()
        .await
        .expect("summary without bearer");
    assert_eq!(
        summary_unauth.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "API must stay gated when a key is configured"
    );

    let summary_auth = client
        .get(server.url("/api/v1/summary"))
        .header("Authorization", format!("Bearer {KEYED_LOOPBACK_KEY}"))
        .send()
        .await
        .expect("summary with bearer");
    assert!(
        summary_auth.status().is_success(),
        "authenticated API must succeed"
    );

    server.shutdown().await;
}
