//! 008 US1/US2 (T009/T013) — `/api/v1/aap` returns correct detection / mode /
//! versions / drift / presets for fixtures, behind the 006 auth + Origin layer.
//!
//! The admin router is built and layered through the **same** path `serve::run`
//! uses (`build_admin_router` + `apply_origin_gate` + `apply_bearer_auth`),
//! bound on `127.0.0.1:0`, and driven with `reqwest` — exactly mirroring
//! `admin_api_v1.rs`.
//!
//! Detection is driven via the `AAP_ROOT` env var pointed at a committed
//! `tests/fixtures/aap/*` directory (fixtures only — never a real AAP checkout).
//! `get_aap` → `AapView::from_runtime` → `AapDetection::resolve()` reads
//! `AAP_ROOT` in-process, so every test that sets it serializes through
//! `ENV_LOCK` and restores the prior value; the suite runs `--test-threads=1`.
//!
//! Coverage:
//! * **drift fixture (SC-001)** — pinned 7.0.0 ≠ running → `detected`, `drift`,
//!   `drifted=true`, mode `both` (serve active in the admin path).
//! * **no-pin fixture** — detected but `pin_unknown`, `drifted=false` (no false
//!   warning).
//! * **not-detected (SC-002)** — absent `AAP_ROOT` + no sibling → clean
//!   `detected=false`, `mode="none"`, empty roots; the rest of `/admin` is
//!   unaffected.
//! * **presets (SC-003 / T013)** — the embed snippet is ALWAYS present for a
//!   detected AAP and is a path dep with `features=["embed"]`, never a
//!   stdio-spawn config; the serve-URL preset is present in the admin path
//!   (serve active) and is an HTTP MCP registration, never a `command` entry.
//! * **auth (SC)** — a keyed non-loopback runtime rejects an unauthenticated
//!   `/api/v1/aap` with 401.
//! * **Origin** — a disallowed browser `Origin` is refused (403).
#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use parking_lot::Mutex;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::server::{
    AuthConfig, AuthLayerState, OriginLayerState, ServerRuntime, admin::build_admin_router,
    apply_bearer_auth, apply_origin_gate,
};
use symforge::sidecar::governor::RequestGovernor;
use symforge::watcher::WatcherInfo;

const TEST_KEY: &str = "sf_aap_admin_key";
const AAP_FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/aap");

/// Serializes `AAP_ROOT` mutation across the env-driven tests below. The suite
/// runs `--test-threads=1`, but the lock keeps the save/restore discipline
/// explicit (and correct if that flag is ever dropped).
static ENV_LOCK: StdMutex<()> = StdMutex::new(());

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(AAP_FIXTURES).join(name)
}

/// Run `body` with `AAP_ROOT` set to `root` (or removed when `None`), restoring
/// the prior value afterwards. Serialized through `ENV_LOCK`.
fn with_aap_root<T>(root: Option<PathBuf>, body: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prior = std::env::var_os("AAP_ROOT");
    #[allow(unsafe_code)] // test-only env mutation under ENV_LOCK + --test-threads=1.
    // SAFETY: serialized by ENV_LOCK; the suite runs single-threaded.
    unsafe {
        match &root {
            Some(p) => std::env::set_var("AAP_ROOT", p),
            None => std::env::remove_var("AAP_ROOT"),
        }
    }
    let out = body();
    #[allow(unsafe_code)] // test-only env restore under ENV_LOCK + --test-threads=1.
    // SAFETY: serialized by ENV_LOCK; the suite runs single-threaded.
    unsafe {
        match prior {
            Some(v) => std::env::set_var("AAP_ROOT", v),
            None => std::env::remove_var("AAP_ROOT"),
        }
    }
    out
}

fn runtime(auth: AuthConfig) -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "aap-admin-it".to_string(),
        watcher_info,
        None,
        None,
    ));
    let governor = Arc::new(RequestGovernor::new());
    ServerRuntime::build_runtime(index, protocol, governor, auth, None)
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

/// Fetch `/api/v1/aap` with `AAP_ROOT` set to the named fixture (or unset for the
/// not-detected case). Synchronous on purpose: the in-process
/// `AapDetection::resolve()` reads `AAP_ROOT`, so the env guard must span the
/// entire request. We hold the guard (sync) and drive the async request through a
/// nested current-thread runtime created inside it; the env is restored when the
/// guard drops. This avoids holding process-env across a multi-threaded
/// `#[tokio::test]` scheduler (which would race other tests).
fn aap_for_fixture(fixture_name: Option<&str>) -> serde_json::Value {
    let root = fixture_name.map(fixture);
    with_aap_root(root, || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("nested runtime");
        rt.block_on(async {
            let server = start(runtime(AuthConfig::new(None)), AuthConfig::new(None), true).await;
            let (status, body) = get_json(&server.url("/api/v1/aap"), None, None).await;
            assert!(
                status.is_success(),
                "aap endpoint should be 200, got {status}"
            );
            server.shutdown().await;
            body
        })
    })
}

// ---------------------------------------------------------------------------
// Detection / drift / mode (SC-001 / SC-002)
// ---------------------------------------------------------------------------

#[test]
fn aap_drift_fixture_reports_detected_drift_and_versions() {
    let body = aap_for_fixture(Some("drift"));
    assert_eq!(body["detected"], true, "drift fixture must be detected");
    assert_eq!(body["source"], "env", "AAP_ROOT resolution => source=env");
    assert_eq!(body["pinned_version"], "7.0.0");
    assert!(
        body["running_version"].as_str().is_some(),
        "running version always present"
    );
    assert_ne!(
        body["pinned_version"], body["running_version"],
        "drift fixture pins an old version"
    );
    assert_eq!(body["drift"], "drift");
    assert_eq!(body["drifted"], true, "old pin must flag drift");
    // Serve is active in the admin path → both embed + serve URL.
    assert_eq!(body["mode"], "both");
    // The detected root is surfaced as an indexed root (read-only, no fabrication).
    assert!(
        body["indexed_roots"]
            .as_array()
            .is_some_and(|r| !r.is_empty()),
        "detected root is surfaced as an indexed root"
    );
}

#[test]
fn aap_no_pin_fixture_is_detected_pin_unknown_no_false_drift() {
    let body = aap_for_fixture(Some("no-pin"));
    assert_eq!(body["detected"], true, "no-pin fixture root still detected");
    assert!(
        body["pinned_version"].is_null(),
        "no symforge pin => null pinned version"
    );
    assert_eq!(body["drift"], "pin_unknown");
    assert_eq!(
        body["drifted"], false,
        "no pin must NOT raise a false drift warning"
    );
}

#[test]
fn aap_not_detected_is_clean_empty_state() {
    // No AAP_ROOT and (in the test working dir) no sibling AAP checkout → clean
    // not-detected. SC-002: not an error; mode none; no fabricated roots.
    let body = aap_for_fixture(None);
    // The CI/test working dir has no Agent_Army_Professionals sibling, so this is
    // not-detected. (If a developer runs the suite from a tree that DOES have the
    // sibling, detection would flip; guard the meaningful invariants either way.)
    if body["detected"] == true {
        // Sibling present in the dev tree: still a clean, non-error view.
        assert!(body["root"].as_str().is_some());
        assert_ne!(body["mode"], "none");
    } else {
        assert_eq!(body["detected"], false);
        assert!(body["root"].is_null(), "no root when not detected");
        assert!(body["source"].is_null());
        assert_eq!(body["mode"], "none");
        assert_eq!(body["drift"], "pin_unknown");
        assert_eq!(body["drifted"], false);
        assert!(
            body["indexed_roots"]
                .as_array()
                .is_some_and(|r| r.is_empty()),
            "no fabricated roots when not detected"
        );
    }
    // running_version is always reported, detected or not.
    assert!(body["running_version"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// Presets (SC-003 / T013): embed always; serve-URL when serve active; never stdio
// ---------------------------------------------------------------------------

#[test]
fn aap_presets_embed_always_serve_url_when_active_never_stdio() {
    let body = aap_for_fixture(Some("drift"));
    let presets = &body["presets"];

    // Embed snippet is ALWAYS present for a detected AAP and is a path dep with
    // the embed feature — never a stdio-spawn config.
    let embed = presets["embed_snippet"]
        .as_str()
        .expect("embed snippet always present for detected AAP");
    assert!(
        embed.contains("path = \"../symforge\""),
        "embed snippet is a path dep: {embed}"
    );
    assert!(
        embed.contains("features = [\"embed\"]"),
        "embed snippet carries the embed feature: {embed}"
    );
    assert!(
        !embed.contains("command") && !embed.contains("stdio") && !embed.contains("args"),
        "embed snippet must NEVER be a stdio-spawn config: {embed}"
    );

    // The admin path runs inside an active serve → the serve-URL preset is
    // present and is an HTTP MCP registration, never a `command`/stdio entry.
    let serve = presets["serve_url_snippet"]
        .as_str()
        .expect("serve-URL preset present in the active-serve admin path");
    let parsed: serde_json::Value =
        serde_json::from_str(serve).expect("serve-URL preset is valid JSON");
    assert_eq!(parsed["mcpServers"]["symforge"]["type"], "http");
    assert!(
        parsed["mcpServers"]["symforge"]["command"].is_null(),
        "serve-URL preset must NEVER emit a stdio-spawn command"
    );
}

// ---------------------------------------------------------------------------
// Auth (SC) + Origin
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aap_unauth_keyed_request_is_rejected() {
    // Keyed runtime, non-loopback flag → auth always required. No Bearer → 401.
    // No AAP_ROOT needed: auth is enforced before the handler runs.
    let auth = AuthConfig::new(Some(TEST_KEY.to_string()));
    let rt = runtime(auth.clone());
    let server = start(rt, auth, false).await;

    let (status, _) = get_json(&server.url("/api/v1/aap"), None, None).await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "unauthenticated non-loopback /api/v1/aap must be 401"
    );

    // Correct key → success.
    let (status, _) = get_json(&server.url("/api/v1/aap"), Some(TEST_KEY), None).await;
    assert!(status.is_success(), "authenticated request must succeed");

    server.shutdown().await;
}

#[tokio::test]
async fn aap_disallowed_origin_is_rejected() {
    let rt = runtime(AuthConfig::new(None));
    let server = start(rt, AuthConfig::new(None), true).await;

    let (status, _) = get_json(
        &server.url("/api/v1/aap"),
        None,
        Some("http://evil.example.com"),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::FORBIDDEN,
        "disallowed Origin must be rejected on /api/v1/aap"
    );

    // Same-origin request is allowed.
    let own = server.own_origin();
    let (status, _) = get_json(&server.url("/api/v1/aap"), None, Some(&own)).await;
    assert!(status.is_success(), "same-origin must be allowed");

    server.shutdown().await;
}
