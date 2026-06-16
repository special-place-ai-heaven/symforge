//! `/mcp` Streamable HTTP transport (US1/T013).
//!
//! Mounts rmcp's server-side Streamable HTTP transport as an axum route at
//! `POST /mcp`, backed by the **same** [`crate::protocol::SymForgeServer`] the
//! stdio path uses. The transport's `service_factory` clones the shared
//! protocol dispatcher (`SymForgeServer` derives `Clone`; every clone shares the
//! same `Arc` index + session context + STEL ledger), so HTTP and stdio dispatch
//! through one in-process handler — no proxy hop, no logic fork
//! (G-022 / G-034 / GATE-5).
//!
//! ## Transport mode: stateless + JSON response
//!
//! [`StreamableHttpServerConfig`] is configured with `stateful_mode = false` and
//! `json_response = true`:
//!
//! * **Stateless** — each request is served directly (`rmcp` calls
//!   `serve_directly`, which skips the MCP `initialize` handshake enforcement),
//!   so a remote harness can issue `tools/list` / `tools/call` independently. The
//!   request-serving *state* (index, ledger) lives on the shared
//!   [`crate::protocol::SymForgeServer`] `Arc`s, not in per-session MCP state, so
//!   statelessness costs no parity: two requests hit the same index and the same
//!   STEL ledger. This matches how the stdio dispatch is fundamentally one shared
//!   `SymForgeServer` over one index.
//! * **JSON response** — a `tools/call` returns `Content-Type: application/json`
//!   (a single JSON-RPC response) instead of `text/event-stream`, eliminating SSE
//!   framing for simple request/response tools (allowed by the MCP Streamable HTTP
//!   spec, 2025-06-18). This is what a remote MCP client and the integration
//!   tests consume.
//!
//! ## DNS-rebinding host allow-list
//!
//! rmcp defaults `allowed_hosts` to loopback only (DNS-rebinding protection for
//! locally-running servers). [`build_mcp_service`] extends the allow-list with
//! the operator-chosen bind host so a routable bind (always authenticated — see
//! [`super::auth`]) is reachable, while the loopback defaults are preserved.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::routing::any_service;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use super::ServerRuntime;
use crate::protocol::SymForgeServer;

/// The mount path for the MCP Streamable HTTP surface.
pub const MCP_PATH: &str = "/mcp";

/// Build the `StreamableHttpService` that serves the MCP surface.
///
/// The `service_factory` clones the shared [`SymForgeServer`] for each inbound
/// request. `SymForgeServer` derives `Clone`; the clone shares the same `Arc`
/// index, session context, and STEL ledger as the runtime's dispatcher, so every
/// HTTP request dispatches through the identical handler methods stdio invokes.
///
/// `bind_host` is the operator-chosen host (from `--listen`); it is added to the
/// transport's `allowed_hosts` so a routable bind is reachable while the loopback
/// DNS-rebinding defaults remain.
fn build_mcp_service(
    runtime: &ServerRuntime,
    bind_host: &str,
) -> StreamableHttpService<SymForgeServer, LocalSessionManager> {
    // Clone the shared dispatcher Arc into the factory; each call clones the
    // inner SymForgeServer (sharing all Arc state) — one dispatch path.
    let protocol = Arc::clone(runtime.protocol());
    let service_factory = move || Ok((*protocol).clone());

    // `StreamableHttpServerConfig` is `#[non_exhaustive]`; use its builders.
    let config = StreamableHttpServerConfig::default()
        // Stateless: serve each request directly (no MCP session handshake gate).
        .with_stateful_mode(false)
        // Return application/json (single JSON-RPC response), not SSE framing.
        .with_json_response(true)
        // Preserve loopback DNS-rebinding defaults; additionally permit the
        // operator-chosen bind host (a routable bind is always authenticated).
        .with_allowed_hosts(host_allow_list(bind_host));

    StreamableHttpService::new(
        service_factory,
        Arc::new(LocalSessionManager::default()),
        config,
    )
}

/// The `allowed_hosts` list: the rmcp loopback defaults plus the operator bind host.
fn host_allow_list(bind_host: &str) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    let host = bind_host.trim();
    if !host.is_empty() && !hosts.iter().any(|h| h == host) {
        hosts.push(host.to_string());
    }
    hosts
}

/// Build the axum [`Router`] mounting the MCP transport at [`MCP_PATH`].
///
/// `bind_addr` supplies the operator-chosen host for the DNS-rebinding
/// allow-list. The returned router has **no** auth layer; [`super::auth`]'s
/// middleware is layered on by [`super::serve::run`] so the secure-default rule
/// is enforced in one place, in front of `/mcp`.
pub fn build_mcp_router(runtime: &ServerRuntime, bind_addr: SocketAddr) -> Router {
    let bind_host = bind_addr.ip().to_string();
    let service = build_mcp_service(runtime, &bind_host);
    Router::new().route(MCP_PATH, any_service(service))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_allow_list_keeps_loopback_defaults() {
        let hosts = host_allow_list("127.0.0.1");
        assert!(hosts.iter().any(|h| h == "localhost"));
        assert!(hosts.iter().any(|h| h == "127.0.0.1"));
        assert!(hosts.iter().any(|h| h == "::1"));
        // Loopback bind host is already present — no duplicate.
        assert_eq!(
            hosts.iter().filter(|h| h.as_str() == "127.0.0.1").count(),
            1
        );
    }

    #[test]
    fn host_allow_list_adds_routable_bind_host() {
        let hosts = host_allow_list("192.168.1.10");
        assert!(hosts.iter().any(|h| h == "192.168.1.10"));
        // Loopback defaults are preserved alongside the routable host.
        assert!(hosts.iter().any(|h| h == "localhost"));
    }
}
