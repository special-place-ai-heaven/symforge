//! 006 US2 — hashed API-key store + end-to-end auth at `/mcp` (SC-003).
//!
//! Proves:
//! * **Hash-only persistence** — `mint` returns the raw secret exactly once; the
//!   persisted DB file never contains the raw secret (only its SHA-256 hash);
//!   `list` returns label/fingerprint/timestamps, never the raw secret.
//! * **Minted key authenticates at /mcp** — a runtime wired with the key store
//!   accepts a minted key's Bearer token at `/mcp` (same auth path `serve::run`
//!   uses), and **rejects** the token after the key is revoked.
//! * **Revoked rejected** — both at the store level and end-to-end over HTTP.
#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::server::{
    ApiKeyStore, AuthConfig, AuthLayerState, ServerRuntime, apply_bearer_auth,
    mcp_http::build_mcp_router,
};
use symforge::sidecar::governor::RequestGovernor;
use symforge::watcher::WatcherInfo;

// ---------------------------------------------------------------------------
// Store-level: hash-only, raw-once, list-no-raw, revoke
// ---------------------------------------------------------------------------

#[test]
fn mint_persists_hash_only_raw_shown_once() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ApiKeyStore::open(tmp.path());
    let minted = store.mint("ci").expect("mint");
    let raw = minted.raw_secret.clone();
    assert!(raw.starts_with("sf_"));

    // The persisted DB bytes must NOT contain the raw secret anywhere.
    let db_path = tmp.path().join(".symforge").join("api-keys.db");
    let bytes = std::fs::read(&db_path).expect("read db");
    let needle = raw.as_bytes();
    let contains = bytes.windows(needle.len()).any(|w| w == needle);
    assert!(
        !contains,
        "raw secret must never be persisted (hash-only store)"
    );

    // list() never returns the raw secret.
    let listed = store.list().expect("list");
    assert_eq!(listed.len(), 1);
    let json = serde_json::to_string(&listed).expect("serialize");
    assert!(
        !json.contains(&raw),
        "list output must not contain raw secret"
    );
    assert!(json.contains("fingerprint"));
}

#[test]
fn list_never_returns_raw_after_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let raw = {
        let store = ApiKeyStore::open(tmp.path());
        store.mint("k").expect("mint").raw_secret
    };
    let store2 = ApiKeyStore::open(tmp.path());
    // Verify still works (hash persisted), but list cannot reveal raw.
    assert!(store2.verify(&raw));
    let json = serde_json::to_string(&store2.list().expect("list")).expect("serialize");
    assert!(!json.contains(&raw));
}

// ---------------------------------------------------------------------------
// End-to-end: minted key authenticates at /mcp; revoked rejected
// ---------------------------------------------------------------------------

fn runtime_with_store(store: Arc<ApiKeyStore>) -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "keys-it".to_string(),
        watcher_info,
        None,
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    // Bootstrap key is set (so auth is required on every bind), but the minted
    // key is a SEPARATE credential proven to authenticate via the store.
    ServerRuntime::build_runtime(
        index,
        protocol,
        governor,
        AuthConfig::new(Some("bootstrap-key".to_string())),
        None,
    )
    .with_key_store(store)
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

async fn start_mcp(runtime: ServerRuntime) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let router = build_mcp_router(&runtime, addr);
    let mut auth_state = AuthLayerState::new(runtime.auth().clone(), true);
    if let Some(store) = runtime.key_store() {
        auth_state = auth_state.with_key_store(Arc::clone(store));
    }
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

async fn tools_list_status(url: &str, bearer: &str) -> reqwest::StatusCode {
    let client = reqwest::Client::new();
    client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Authorization", format!("Bearer {bearer}"))
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}
        }))
        .send()
        .await
        .expect("request sent")
        .status()
}

#[tokio::test]
async fn minted_key_authenticates_at_mcp_revoked_rejected() {
    let store = Arc::new(ApiKeyStore::open_in_memory().expect("store"));
    let minted = store.mint("agent").expect("mint");
    let raw = minted.raw_secret.clone();

    let runtime = runtime_with_store(Arc::clone(&store));
    let server = start_mcp(runtime).await;

    // Minted key authenticates at /mcp (not 401).
    let status = tools_list_status(&server.mcp_url(), &raw).await;
    assert_ne!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "minted key must authenticate at /mcp"
    );
    assert!(status.is_success());

    // Bootstrap key also still authenticates.
    let status = tools_list_status(&server.mcp_url(), "bootstrap-key").await;
    assert_ne!(status, reqwest::StatusCode::UNAUTHORIZED);

    // Wrong key is rejected.
    let status = tools_list_status(&server.mcp_url(), "sf_not_a_key").await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);

    // Revoke the minted key → it stops authenticating.
    store.revoke(minted.record.id).expect("revoke");
    let status = tools_list_status(&server.mcp_url(), &raw).await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "revoked key must be rejected at /mcp (SC-003)"
    );

    server.shutdown().await;
}
