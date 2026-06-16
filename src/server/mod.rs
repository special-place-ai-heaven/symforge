//! Transport-agnostic operator server spine (v8, `symforge serve`).
//!
//! [`ServerRuntime`] is the single in-process owner of request-serving state:
//! the shared index, the existing protocol dispatcher
//! ([`crate::protocol::SymForgeServer`] — the same handle the stdio path uses),
//! the [`RequestGovernor`], the [`AuthConfig`], and an optional durable STEL
//! [`StelLedgerStore`]. Both stdio and the future `/mcp` transport dispatch
//! through this one runtime — no proxy hop, no logic fork (G-022/G-034/GATE-5).
//!
//! Phase-2 scope: own the state and expose [`ServerRuntime::dispatch_tool_call`],
//! which delegates to the protocol dispatcher's statused tool-dispatch (the same
//! handler methods stdio invokes). Mounting `/mcp` over Streamable HTTP is
//! US1/T013-T016 ([`mcp_http`]); the full live `tool_router` parity battery is
//! finalized in US1/T018.

pub mod admin;
pub mod api_keys;
pub mod auth;
pub mod mcp_http;
pub mod serve;

use std::sync::Arc;

use crate::live_index::SharedIndex;
use crate::protocol::SymForgeServer;
use crate::sidecar::governor::RequestGovernor;
use crate::stel::ledger_store::StelLedgerStore;

pub use api_keys::{ApiKeyRecord, ApiKeyStore, MintedKey};
pub use auth::{
    AuthConfig, AuthLayerState, AuthStartupError, OriginLayerState, apply_bearer_auth,
    apply_origin_gate,
};

/// The single transport-agnostic owner of request-serving state.
///
/// Cloning is cheap: the heavy state ([`SymForgeServer`], [`RequestGovernor`],
/// [`StelLedgerStore`]) is behind `Arc`, so every transport adapter shares one
/// index + dispatcher + governor + ledger.
#[derive(Clone)]
pub struct ServerRuntime {
    /// The shared live index (one per server session; shared with the dispatcher).
    index: SharedIndex,
    /// The existing in-process protocol dispatcher — same handle stdio uses.
    protocol: Arc<SymForgeServer>,
    /// Concurrency / timeout governor, reused from the sidecar.
    governor: Arc<RequestGovernor>,
    /// Bearer auth policy (secure-by-default).
    auth: AuthConfig,
    /// Durable STEL economics ledger; `None` until opened by `serve::run`
    /// (US3/T029). When `Some`, it may itself be [`StelLedgerStore::Disabled`]
    /// if the DB could not open (FR-011) — serving continues regardless.
    ledger_store: Option<StelLedgerStore>,
    /// Hashed product API-key store (G-039), shared with the auth layer and the
    /// admin `/api/v1/keys` handlers. `None` until opened by `serve::run`; when
    /// `Some` it may be [`ApiKeyStore::Disabled`] (DB unavailable) and the
    /// bootstrap `--api-key` still authenticates.
    key_store: Option<Arc<ApiKeyStore>>,
    /// Process-start instant for uptime telemetry (`/api/v1/system`).
    started_at: std::time::Instant,
    /// Human-readable project name for telemetry / dashboard headers.
    project_name: String,
}

impl ServerRuntime {
    /// Build a runtime from an already-constructed protocol dispatcher.
    ///
    /// The `protocol` server must have been built over the same `index` (as the
    /// production `SymForgeServer::new` wiring does) so the index the governor
    /// guards and the index the dispatcher reads are one and the same.
    pub fn build_runtime(
        index: SharedIndex,
        protocol: Arc<SymForgeServer>,
        governor: Arc<RequestGovernor>,
        auth: AuthConfig,
        ledger_store: Option<StelLedgerStore>,
    ) -> Self {
        let project_name = protocol.project_name.clone();
        Self {
            index,
            protocol,
            governor,
            auth,
            ledger_store,
            key_store: None,
            started_at: std::time::Instant::now(),
            project_name,
        }
    }

    /// Attach a hashed product API-key store (G-039). Shared (by `Arc`) with the
    /// auth layer (so minted keys authenticate at `/mcp`) and the admin
    /// `/api/v1/keys` handlers. Consumes and returns `self` (builder style).
    pub fn with_key_store(mut self, key_store: Arc<ApiKeyStore>) -> Self {
        self.key_store = Some(key_store);
        self
    }

    /// Access the hashed API-key store, if one was opened.
    pub fn key_store(&self) -> Option<&Arc<ApiKeyStore>> {
        self.key_store.as_ref()
    }

    /// Process uptime since the runtime was built (for `/api/v1/system`).
    pub fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Human-readable project name for telemetry / dashboard headers.
    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    /// Access the shared index.
    pub fn index(&self) -> &SharedIndex {
        &self.index
    }

    /// Access the protocol dispatcher (the shared stdio/HTTP handle).
    pub fn protocol(&self) -> &Arc<SymForgeServer> {
        &self.protocol
    }

    /// Access the request governor.
    pub fn governor(&self) -> &Arc<RequestGovernor> {
        &self.governor
    }

    /// Access the auth policy.
    pub fn auth(&self) -> &AuthConfig {
        &self.auth
    }

    /// Access the durable ledger store, if one was opened.
    pub fn ledger_store(&self) -> Option<&StelLedgerStore> {
        self.ledger_store.as_ref()
    }

    /// Dispatch a single `tools/call` through the shared protocol dispatcher.
    ///
    /// Delegates to [`SymForgeServer::dispatch_tool_result_for_tests`] — despite
    /// the `_for_tests` name, this is the statused dispatch entry that routes to
    /// the **same handler methods** the live `tool_router` invokes
    /// (`symforge_facade_tool`, `symforge_edit_facade_tool`, `status_stel_tool`,
    /// the read/search/edit tools), returning the identical
    /// [`rmcp::model::CallToolResult`] shape stdio produces. No logic fork.
    ///
    /// US1/T018 finalizes the full live-`tool_router` parity battery over `/mcp`;
    /// this Phase-2 helper proves the delegation compiles and returns the parity
    /// result type for an in-memory index.
    pub async fn dispatch_tool_call(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        self.protocol
            .dispatch_tool_result_for_tests(tool_name, params)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_index::LiveIndex;
    use crate::watcher::WatcherInfo;
    use parking_lot::Mutex;

    /// Build a minimal in-process runtime over an empty in-memory index,
    /// mirroring the local-stdio `SymForgeServer::new` wiring.
    fn build_test_runtime(auth: AuthConfig) -> ServerRuntime {
        let index = LiveIndex::empty();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let protocol = Arc::new(SymForgeServer::new(
            Arc::clone(&index),
            "test_project".to_string(),
            watcher_info,
            None,
            None,
        ));
        let governor = Arc::new(RequestGovernor::new());
        ServerRuntime::build_runtime(index, protocol, governor, auth, None)
    }

    #[test]
    fn build_runtime_owns_state() {
        let runtime = build_test_runtime(AuthConfig::new(Some("k".to_string())));
        assert!(runtime.auth().requires_auth(true));
        assert!(runtime.ledger_store().is_none());
        // Governor exposes its default concurrency.
        assert!(runtime.governor().max_concurrency() > 0);
        // Index handle is live.
        let _ = runtime.index().published_state();
    }

    #[tokio::test]
    async fn dispatch_tool_call_returns_parity_result_shape() {
        // T010: assert delegation returns Ok with the same CallToolResult shape
        // stdio produces, for a trivial tool on an in-memory index.
        let runtime = build_test_runtime(AuthConfig::new(None));
        let result = runtime
            .dispatch_tool_call("status", serde_json::json!({}))
            .await
            .expect("status dispatch returns a CallToolResult");
        // A CallToolResult always carries content; the status tool emits text.
        assert!(
            !result.content.is_empty(),
            "status result should carry content (parity with stdio dispatch)"
        );
        // Full live-tool_router parity battery is completed in US1/T018.
    }

    #[tokio::test]
    async fn dispatch_unknown_tool_returns_structured_invalid_request() {
        // The shared dispatcher returns a statused InvalidRequest result (Ok),
        // not an error — same as the stdio conformance path.
        let runtime = build_test_runtime(AuthConfig::new(None));
        let result = runtime
            .dispatch_tool_call("definitely_not_a_tool", serde_json::json!({}))
            .await
            .expect("unknown tool yields a statused result, not an Err");
        assert!(!result.content.is_empty());
    }
}
