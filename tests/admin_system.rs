//! 006 US3 — `/api/v1/system` matches the runtime's real state (T019 / SC-005).
//!
//! Asserts the system snapshot returned over HTTP matches the **actual** running
//! process: PID equals `std::process::id()`, active sessions is the single serve
//! session, and the indexed-project / index-generation fields reflect the live
//! index the runtime owns (not fabricated values).
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
use symforge::watcher::WatcherInfo;

fn runtime(project: &str) -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        project.to_string(),
        watcher_info,
        None,
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    ServerRuntime::build_runtime(index, protocol, governor, AuthConfig::new(None), None)
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

async fn start(runtime: ServerRuntime) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let admin = build_admin_router(&runtime);
    let gated = apply_origin_gate(admin, OriginLayerState::from_bind_addr(addr));
    let app = apply_bearer_auth(gated, AuthLayerState::new(AuthConfig::new(None), true));
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

#[tokio::test]
async fn system_snapshot_matches_real_runtime_state() {
    let server = start(runtime("admin-sys-project")).await;
    let client = reqwest::Client::new();

    let system = client
        .get(server.url("/api/v1/system"))
        .send()
        .await
        .expect("system req")
        .json::<serde_json::Value>()
        .await
        .expect("system json");

    // PID matches the actual test process (the serve runtime runs in-process).
    assert_eq!(
        system["pid"].as_u64(),
        Some(u64::from(std::process::id())),
        "PID must match the real process"
    );
    // Exactly one active serve session.
    assert_eq!(system["active_sessions"], 1);
    // Indexed-project name reflects the runtime's project (non-default name is
    // reported even over an empty index).
    let projects = system["indexed_projects"]
        .as_array()
        .expect("indexed_projects array");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0], "admin-sys-project");
    // Index telemetry fields are present and numeric (empty index → 0 files).
    assert_eq!(system["indexed_file_count"].as_u64(), Some(0));
    assert!(system["index_generation"].as_u64().is_some());
    // Uptime is a real elapsed value (>= 0).
    assert!(system["uptime_secs"].as_u64().is_some());

    server.shutdown().await;
}
