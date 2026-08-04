//! Spec 025 (rmcp 3.x migration) — PR-B protocol-enablement battery.
//!
//! Covers, over the REAL `/mcp` Streamable HTTP path (`build_mcp_router` +
//! `apply_bearer_auth`, the same construction `serve::run` uses):
//!
//! * **SC-310** — `initialize` negotiation: 2026-07-28 negotiates 2026-07-28
//!   and 2025-06-18 still negotiates 2025-06-18.
//! * **SC-311** — discover-FIRST lifecycle: an authenticated, version-headered
//!   `server/discover` as literally the first request, then the full service
//!   surface (`tools/list`, `tools/call`, `prompts/list`, `resources/list`,
//!   `resources/read`) on the same connection, with zero handshake. The test
//!   sends no `initialize` and no `notifications/initialized` by construction
//!   and asserts it explicitly from its own request log; `on_initialized`
//!   never ran (the server stays on its startup binding throughout).
//! * **SC-312** — modern workspace binding per rung of the fallback chain
//!   (`SYMFORGE_WORKSPACE_ROOT` > `index_folder` > CWD walk) without any
//!   `notifications/initialized`; plus the structural assertion that
//!   `Peer::list_roots` is solicited only from `on_initialized`.
//! * **SC-313** — SEP-2549 cache hints on all five surfaces (`Public` + 1h on
//!   the four static lists, `Private` + 0 on `resources/read`) and
//!   deterministic `tools/list` ordering.
//! * **SC-314** — strict-metadata mixed endpoint behind bearer auth: modern
//!   negative cases (missing `_meta`, header/`_meta` disagreement,
//!   `Mcp-Method` body disagreement) rejected; a correctly formed modern
//!   request accepted; header-less legacy requests keep HTTP-200 JSON-RPC
//!   semantics (FR-309 posture pinned in `mcp_http.rs`).
//! * **SC-315** — compact-surface gate through the REAL dispatch path
//!   (HTTP `tools/call`), the binding INV-2 tripwire (FR-320).
//! * **SC-316** — FR-319 binding-evidence disclosure: statused tool,
//!   plain-`String` tool (`health`), `resources/read`, the foreign-binding
//!   negative, and the unbound disclosure.
//! * **FR-A6 / owner tests 1-2** — version-aware `resultType`: present for a
//!   modern-headered request, stripped for a header-less legacy request.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::server::{
    AuthConfig, AuthLayerState, ServerRuntime, apply_bearer_auth, mcp_http::build_mcp_router,
};
use symforge::sidecar::governor::RequestGovernor;
use symforge::watcher::WatcherInfo;

/// Local copy of the crate's pub(crate) root normalizer: dunce-canonicalize
/// with an identity fallback for paths that do not exist.
fn normalize_root(root: &std::path::Path) -> std::path::PathBuf {
    dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

const TEST_KEY: &str = "sf_rmcp3_key";
const MODERN_VERSION: &str = "2026-07-28";
const EVIDENCE_KEY: &str = "symforge/project_evidence";

fn test_runtime() -> ServerRuntime {
    let index = LiveIndex::empty();
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let protocol = Arc::new(SymForgeServer::new(
        Arc::clone(&index),
        "rmcp3-protocol-test".to_string(),
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

/// A `/mcp` client that records every JSON-RPC method it sends, so SC-311 can
/// assert from the log (not just by construction) that no handshake happened.
struct McpHttpClient {
    url: String,
    client: reqwest::Client,
    next_id: u32,
    sent_methods: Vec<String>,
}

/// Extra HTTP headers for one request: `(name, value)` pairs.
type Headers<'a> = &'a [(&'a str, &'a str)];

impl McpHttpClient {
    fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
            next_id: 1,
            sent_methods: Vec::new(),
        }
    }

    /// POST one JSON-RPC request; returns `(http_status, body_json)`. Never
    /// asserts success — SC-314's negative cases need the raw outcome.
    async fn call_raw(
        &mut self,
        method: &str,
        params: Value,
        headers: Headers<'_>,
    ) -> (reqwest::StatusCode, Value) {
        let id = self.next_id;
        self.next_id += 1;
        self.sent_methods.push(method.to_string());
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut request = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Authorization", format!("Bearer {TEST_KEY}"))
            .json(&body);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let resp = request.send().await.expect("request sent");
        let status = resp.status();
        let body = resp.json::<Value>().await.expect("JSON-RPC body");
        (status, body)
    }

    /// POST expecting success; returns the JSON-RPC `result`.
    async fn call(&mut self, method: &str, params: Value, headers: Headers<'_>) -> Value {
        let (status, body) = self.call_raw(method, params, headers).await;
        assert!(
            status.is_success(),
            "{method} should succeed, got {status}: {body}"
        );
        assert!(
            body.get("error").is_none(),
            "{method} should not error: {body}"
        );
        body["result"].clone()
    }
}

/// `_meta` carrying the 2026-07-28 required per-request metadata.
fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": MODERN_VERSION,
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

/// Merge the modern `_meta` into `params`.
fn with_modern_meta(mut params: Value) -> Value {
    params["_meta"] = modern_meta();
    params
}

/// A correctly formed modern request needs the version header plus the
/// SEP-2243 standard headers agreeing with the body.
fn modern_headers(method: &str) -> Vec<(&str, &str)> {
    vec![
        ("MCP-Protocol-Version", MODERN_VERSION),
        ("Mcp-Method", method),
    ]
}

fn evidence_of(result: &Value) -> &Value {
    result
        .get("_meta")
        .and_then(|meta| meta.get(EVIDENCE_KEY))
        .unwrap_or_else(|| panic!("result must carry {EVIDENCE_KEY}: {result}"))
}

// ---------------------------------------------------------------------------
// SC-310 — negotiation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn negotiation_modern_and_legacy_versions() {
    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    for version in [MODERN_VERSION, "2025-06-18"] {
        let result = client
            .call(
                "initialize",
                json!({
                    "protocolVersion": version,
                    "capabilities": {},
                    "clientInfo": {"name": "sc310", "version": "0.0.0"},
                }),
                &[],
            )
            .await;
        assert_eq!(
            result["protocolVersion"], *version,
            "requested {version} must negotiate {version}"
        );
    }

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// SC-311 — discover-first full surface, zero handshake
// ---------------------------------------------------------------------------

#[tokio::test]
async fn discover_first_serves_full_surface_without_handshake() {
    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    // Literally the FIRST request on the connection: server/discover,
    // version-headered and carrying the modern request metadata.
    let discover = client
        .call(
            "server/discover",
            json!({"_meta": modern_meta()}),
            &modern_headers("server/discover"),
        )
        .await;
    // The identity travels in `_meta` under the spec'd serverInfo key.
    assert_eq!(
        discover["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "symforge"
    );
    let versions: Vec<&str> = discover["supportedVersions"]
        .as_array()
        .expect("discover lists protocolVersions")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        versions.contains(&MODERN_VERSION),
        "discover must list {MODERN_VERSION}, got {versions:?}"
    );

    // Then the full service surface on the same connection.
    let tools = client
        .call(
            "tools/list",
            with_modern_meta(json!({})),
            &modern_headers("tools/list"),
        )
        .await;
    assert!(!tools["tools"].as_array().expect("tools array").is_empty());

    let mut call_headers = modern_headers("tools/call");
    call_headers.push(("Mcp-Name", "status"));
    let call = client
        .call(
            "tools/call",
            with_modern_meta(json!({"name": "status", "arguments": {}})),
            &call_headers,
        )
        .await;
    assert!(call["content"][0]["text"].is_string());

    let prompts = client
        .call(
            "prompts/list",
            with_modern_meta(json!({})),
            &modern_headers("prompts/list"),
        )
        .await;
    assert!(prompts["prompts"].is_array());

    let resources = client
        .call(
            "resources/list",
            with_modern_meta(json!({})),
            &modern_headers("resources/list"),
        )
        .await;
    assert!(
        !resources["resources"]
            .as_array()
            .expect("resources array")
            .is_empty()
    );

    let mut read_headers = modern_headers("resources/read");
    read_headers.push(("Mcp-Name", "symforge://glossary"));
    let read = client
        .call(
            "resources/read",
            with_modern_meta(json!({"uri": "symforge://glossary"})),
            &read_headers,
        )
        .await;
    assert!(read["contents"][0]["text"].is_string());

    // Explicit round-3 asserts: no initialize, no notifications/initialized —
    // therefore `on_initialized` (the only roots-binding trigger) never ran.
    assert!(
        !client
            .sent_methods
            .iter()
            .any(|m| m == "initialize" || m == "notifications/initialized"),
        "SC-311 must complete with zero handshake, sent: {:?}",
        client.sent_methods
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// SC-312 — modern binding, per rung, no handshake
// ---------------------------------------------------------------------------

/// Rung 1: `SYMFORGE_WORKSPACE_ROOT` wins, validated through the same
/// forbidden-root guard as CWD discovery.
#[tokio::test]
async fn binding_rung_env_override() {
    let _lock = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical tempdir");
    let guard = stel_surface_env::EnvVarGuard::set(
        "SYMFORGE_WORKSPACE_ROOT",
        root.to_str().expect("utf8 root"),
    );
    let resolved = symforge::discovery::find_project_root().expect("env rung binds");
    assert_eq!(normalize_root(&resolved), normalize_root(&root),);
    drop(guard);
}

/// Rung 2: `index_folder` binds an unbound server through the real dispatch
/// path — a modern client that never sent `notifications/initialized`.
#[tokio::test]
async fn binding_rung_index_folder_over_modern_dispatch() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn seeded() {}\n").expect("seed file");

    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    let mut call_headers = modern_headers("tools/call");
    call_headers.push(("Mcp-Name", "index_folder"));
    let path = dir.path().to_str().expect("utf8 tempdir").to_string();
    client
        .call(
            "tools/call",
            with_modern_meta(json!({"name": "index_folder", "arguments": {"path": path}})),
            &call_headers,
        )
        .await;

    // The next call's evidence discloses the bound root (FR-319 channel).
    let mut status_headers = modern_headers("tools/call");
    status_headers.push(("Mcp-Name", "status"));
    let status = client
        .call(
            "tools/call",
            with_modern_meta(json!({"name": "status", "arguments": {}})),
            &status_headers,
        )
        .await;
    let bound_root = evidence_of(&status)["canonical_root"]
        .as_str()
        .expect("bound root disclosed")
        .to_string();
    let expected = dir
        .path()
        .canonicalize()
        .expect("canonical tempdir")
        .to_string_lossy()
        .replace('\\', "/");
    assert!(
        normalize_root(std::path::Path::new(&bound_root))
            == normalize_root(std::path::Path::new(&expected)),
        "index_folder rung must bind {expected}, evidence says {bound_root}"
    );
    assert!(
        !client
            .sent_methods
            .iter()
            .any(|m| m == "initialize" || m == "notifications/initialized"),
        "binding must not require a handshake"
    );

    server.shutdown().await;
}

/// Rung 3: the CWD walk — exercised directly against `find_project_root` with
/// the env override absent (serial suite; CWD is restored on exit).
#[tokio::test]
async fn binding_rung_cwd_walk() {
    let _lock = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _env = stel_surface_env::EnvVarGuard::unset("SYMFORGE_WORKSPACE_ROOT");
    let dir = tempfile::TempDir::new().expect("tempdir");
    std::fs::create_dir(dir.path().join(".git")).expect("plant .git");
    let nested = dir.path().join("src");
    std::fs::create_dir(&nested).expect("nested dir");

    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&nested).expect("enter nested");
    let resolved = symforge::discovery::find_project_root();
    std::env::set_current_dir(&original).expect("restore cwd");

    let resolved = resolved.expect("cwd walk binds the .git ancestor");
    assert_eq!(
        normalize_root(&resolved),
        normalize_root(&dir.path().canonicalize().expect("canonical tempdir")),
    );
}

/// FR-314 structural assertion: the server solicits `list_roots` from exactly
/// one place — `bind_workspace_from_client_roots`, reachable only from
/// `on_initialized` — so a modern (no-handshake) client can never be blocked
/// on a roots round-trip.
#[test]
fn list_roots_is_solicited_only_from_on_initialized() {
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/protocol/mod.rs"))
            .expect("read protocol/mod.rs");
    // Count CODE solicitations only (comments also mention the API).
    let solicitations = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .filter(|line| line.contains(".list_roots("))
        .count();
    assert_eq!(
        solicitations, 1,
        "Peer::list_roots must be solicited only inside bind_workspace_from_client_roots"
    );
    let grep_others = |path: &str| {
        let full = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), path);
        std::fs::read_to_string(&full)
            .map(|text| text.matches(".list_roots(").count())
            .unwrap_or(0)
    };
    assert_eq!(grep_others("src/protocol/tools.rs"), 0);
    assert_eq!(grep_others("src/server/mcp_http.rs"), 0);
}

// ---------------------------------------------------------------------------
// SC-313 — cache hints on all five surfaces + deterministic tools/list
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cache_hints_on_all_five_surfaces() {
    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    let static_lists = [
        ("tools/list", json!({})),
        ("prompts/list", json!({})),
        ("resources/list", json!({})),
        ("resources/templates/list", json!({})),
    ];
    for (method, params) in static_lists {
        let result = client
            .call(method, with_modern_meta(params), &modern_headers(method))
            .await;
        assert_eq!(
            result["ttlMs"], 3_600_000,
            "{method} must carry the FR-311 static-list TTL: {result}"
        );
        assert_eq!(
            result["cacheScope"], "public",
            "{method} must be publicly cacheable: {result}"
        );
    }

    let mut read_headers = modern_headers("resources/read");
    read_headers.push(("Mcp-Name", "symforge://glossary"));
    let read = client
        .call(
            "resources/read",
            with_modern_meta(json!({"uri": "symforge://glossary"})),
            &read_headers,
        )
        .await;
    assert_eq!(read["ttlMs"], 0, "reads are immediately stale (INV-4)");
    assert_eq!(
        read["cacheScope"], "private",
        "reads must never be shared-cacheable (INV-4)"
    );

    // FR-311: a cacheable surface must not shuffle.
    let names = |result: &Value| -> Vec<String> {
        result["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name").to_string())
            .collect()
    };
    let first = client
        .call(
            "tools/list",
            with_modern_meta(json!({})),
            &modern_headers("tools/list"),
        )
        .await;
    let second = client
        .call(
            "tools/list",
            with_modern_meta(json!({})),
            &modern_headers("tools/list"),
        )
        .await;
    assert_eq!(
        names(&first),
        names(&second),
        "tools/list ordering must be deterministic"
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// SC-314 — strict metadata, mixed endpoint, behind bearer auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn strict_metadata_mixed_endpoint() {
    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    // (i) Modern header with MISSING required `_meta` → rejected.
    let (status, body) = client
        .call_raw("tools/list", json!({}), &modern_headers("tools/list"))
        .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("required fields"),
        "missing-_meta rejection must name the gap: {body}"
    );

    // (ii) Header vs `_meta.protocolVersion` disagreement → rejected.
    let (status, body) = client
        .call_raw(
            "tools/list",
            json!({"_meta": {
                "io.modelcontextprotocol/protocolVersion": "2025-11-25",
                "io.modelcontextprotocol/clientCapabilities": {},
            }}),
            &modern_headers("tools/list"),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("does not match"),
        "version disagreement must be named: {body}"
    );

    // (iii) `Mcp-Method` header disagreeing with the body → rejected.
    let (status, body) = client
        .call_raw(
            "tools/list",
            with_modern_meta(json!({})),
            &[
                ("MCP-Protocol-Version", MODERN_VERSION),
                ("Mcp-Method", "prompts/list"),
            ],
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Mcp-Method"),
        "standard-header disagreement must be named: {body}"
    );

    // (iv) Correctly formed modern request behind bearer auth → accepted.
    let result = client
        .call(
            "tools/list",
            with_modern_meta(json!({})),
            &modern_headers("tools/list"),
        )
        .await;
    assert!(!result["tools"].as_array().expect("tools").is_empty());

    // Mixed endpoint, legacy half: a header-less request keeps HTTP-200
    // JSON-RPC semantics (FR-309) — accepted, no transport-level rejection.
    let (status, body) = client.call_raw("tools/list", json!({}), &[]).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert!(body.get("error").is_none(), "legacy accept intact: {body}");

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// SC-315 — compact-surface gate via the real dispatch path (INV-2 tripwire)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compact_gate_rejects_off_surface_calls_via_real_dispatch() {
    let _lock = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _guard = stel_surface_env::set_symforge_surface("compact");

    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    // Off-surface tool through the REAL rmcp dispatch path → InvalidRequest.
    let (status, body) = client
        .call_raw(
            "tools/call",
            json!({"name": "get_symbol", "arguments": {"name": "anything"}}),
            &[],
        )
        .await;
    let error = &body["error"];
    assert!(
        !error.is_null(),
        "off-surface call must be rejected (status {status}): {body}"
    );
    assert_eq!(
        error["code"], -32600,
        "compact gate rejects with InvalidRequest: {body}"
    );

    // On-surface compact tool succeeds on the same path.
    let (status, body) = client
        .call_raw(
            "tools/call",
            json!({"name": "status", "arguments": {}}),
            &[],
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert!(
        body.get("error").is_none(),
        "on-surface compact call must succeed: {body}"
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// SC-316 — FR-319 binding evidence disclosure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evidence_disclosed_on_statused_plain_and_resource_results() {
    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    // (a) A statused tool result carries the evidence key.
    let mut status_headers = modern_headers("tools/call");
    status_headers.push(("Mcp-Name", "status"));
    let statused = client
        .call(
            "tools/call",
            with_modern_meta(json!({"name": "status", "arguments": {}})),
            &status_headers,
        )
        .await;
    let evidence = evidence_of(&statused);
    assert!(evidence["project_id"].is_string(), "{evidence}");

    // (b) A plain-`String` tool (health) — previously meta-less — now carries
    // it via the central seam (parity exception documented in spec 025).
    let mut health_headers = modern_headers("tools/call");
    health_headers.push(("Mcp-Name", "health"));
    let health = client
        .call(
            "tools/call",
            with_modern_meta(json!({"name": "health", "arguments": {}})),
            &health_headers,
        )
        .await;
    let evidence = evidence_of(&health);
    assert!(evidence["project_id"].is_string(), "{evidence}");

    // (c) A resources/read result carries it too.
    let mut read_headers = modern_headers("resources/read");
    read_headers.push(("Mcp-Name", "symforge://glossary"));
    let read = client
        .call(
            "resources/read",
            with_modern_meta(json!({"uri": "symforge://glossary"})),
            &read_headers,
        )
        .await;
    let evidence = evidence_of(&read);
    assert!(evidence["project_id"].is_string(), "{evidence}");

    // (e) Unbound server: the disclosure is the EXPLICIT unbound shape —
    // project_id "unbound", no canonical root — never a silently missing key.
    assert_eq!(evidence["project_id"], "unbound", "{evidence}");
    assert!(evidence["canonical_root"].is_null(), "{evidence}");

    server.shutdown().await;
}

/// (d) The negative case: a server bound to repository A answers a client
/// expecting repository B — the evidence discloses the FOREIGN binding rather
/// than results passing silently.
#[tokio::test]
async fn evidence_discloses_foreign_binding() {
    let repo_a = tempfile::TempDir::new().expect("repo A");
    std::fs::write(repo_a.path().join("a.rs"), "pub fn a() {}\n").expect("seed A");
    let repo_b = tempfile::TempDir::new().expect("repo B");

    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    let mut index_headers = modern_headers("tools/call");
    index_headers.push(("Mcp-Name", "index_folder"));
    let path_a = repo_a.path().to_str().expect("utf8 A").to_string();
    client
        .call(
            "tools/call",
            with_modern_meta(json!({"name": "index_folder", "arguments": {"path": path_a}})),
            &index_headers,
        )
        .await;

    let mut status_headers = modern_headers("tools/call");
    status_headers.push(("Mcp-Name", "status"));
    let statused = client
        .call(
            "tools/call",
            with_modern_meta(json!({"name": "status", "arguments": {}})),
            &status_headers,
        )
        .await;
    let disclosed = evidence_of(&statused)["canonical_root"]
        .as_str()
        .expect("bound root disclosed")
        .to_string();

    let normalize = |p: &std::path::Path| normalize_root(p);
    let root_a = repo_a.path().canonicalize().expect("canonical A");
    let root_b = repo_b.path().canonicalize().expect("canonical B");
    assert_eq!(
        normalize(std::path::Path::new(&disclosed)),
        normalize(&root_a),
        "evidence must name the ACTUAL binding"
    );
    assert_ne!(
        normalize(std::path::Path::new(&disclosed)),
        normalize(&root_b),
        "a client expecting repo B can detect the foreign binding"
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// FR-A6 / owner tests 1-2 — version-aware `resultType` on the wire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn result_type_present_for_modern_stripped_for_legacy() {
    let server = start_server(test_runtime()).await;
    let mut client = McpHttpClient::new(server.mcp_url());

    // Modern-headered peer sees the SEP-2322 discriminator.
    let modern = client
        .call(
            "tools/list",
            with_modern_meta(json!({})),
            &modern_headers("tools/list"),
        )
        .await;
    assert_eq!(
        modern["resultType"], "complete",
        "modern peers must see resultType: {modern}"
    );

    // Header-less legacy peer has the key STRIPPED by the SDK.
    let (status, body) = client.call_raw("tools/list", json!({}), &[]).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(
        body["result"].get("resultType").is_none(),
        "legacy peers must not see resultType: {body}"
    );

    server.shutdown().await;
}
