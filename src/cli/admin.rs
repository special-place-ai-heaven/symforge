//! `symforge admin` — open (or start + open) the operator dashboard (009 US3).
//!
//! Reuses a running operator server when one is reachable on the remembered port
//! (no duplicate server, FR-015); otherwise starts one on a verified-free port
//! and opens/returns the dashboard URL. This module is the thin admin-verb layer
//! over the shipped 004 serve + 006 admin dashboard.
//!
//! Phase 3 (Foundational, T011) lands the two reusable pieces the wizard (US2) and
//! this admin verb (US3) both consume: [`operator_server_reachable`] (an HTTP
//! reachability probe) and [`start_operator_server`] (a non-blocking serve-start
//! that returns a [`ServerSessionDescriptor`]). The full reachability ->
//! reuse/start -> open flow (`run`) lands in Phase US3 (T023).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use clap::Args;

use crate::server::admin::ADMIN_PATH;
use crate::server::mcp_http::MCP_PATH;
use crate::server::serve::{self, ServeArgs};

/// The running operator server as the wizard / admin verb sees it (E4, transient
/// — never persisted).
///
/// **Invariant (FR-020)**: a descriptor is only returned with `reachable == true`,
/// and every URL it carries names exactly `bound_addr` — no advertised-but-dead
/// URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSessionDescriptor {
    /// The actually-bound address (D1); the source of every reported URL.
    pub bound_addr: SocketAddr,
    /// `http://<bound_addr>/admin` — the dashboard URL.
    pub dashboard_url: String,
    /// `http://<bound_addr>/mcp` — the MCP attach URL on the same address.
    pub attach_url: String,
    /// Whether an HTTP reachability probe of `bound_addr` succeeded within the
    /// deadline. Always `true` for a descriptor returned by
    /// [`start_operator_server`].
    pub reachable: bool,
}

impl ServerSessionDescriptor {
    /// Build the descriptor for a `bound_addr`, deriving the dashboard + attach
    /// URLs from that single address (FR-020).
    pub fn for_addr(bound_addr: SocketAddr, reachable: bool) -> Self {
        Self {
            dashboard_url: format!("http://{bound_addr}{ADMIN_PATH}"),
            attach_url: format!("http://{bound_addr}{MCP_PATH}"),
            bound_addr,
            reachable,
        }
    }
}

/// HTTP reachability probe for an operator server (D6, FR-015/FR-020).
///
/// Sends a `GET http://<addr>/api/v1/summary` with `timeout` as the total budget.
/// Returns `true` when the server **responds at all** — including a `401`
/// (auth-gated `/api/v1/summary` on a keyed server) — because any HTTP response
/// proves a server is listening and answering on that address. Returns `false`
/// on a connection refusal, timeout, or any transport error (no server there).
///
/// This is the `sidecar::port_file::sidecar_port_is_alive` pattern lifted to HTTP:
/// a bare TCP connect would pass for a bound-but-not-serving socket, whereas an
/// HTTP response proves the dashboard router actually answers (FR-020).
///
/// HTTP-client choice: `reqwest` is already a first-class server-feature dep
/// (`dep:reqwest`, used in `cli::version`, `protocol::tools`, etc.), so all
/// `#[cfg(feature = "server")]` code — including this module — can use it. We run
/// it on a private current-thread runtime (mirroring
/// `cli::version::latest_npm_version_with_timeout`) so the probe is callable from
/// a plain synchronous CLI context without requiring an ambient async runtime.
pub fn operator_server_reachable(addr: SocketAddr, timeout: Duration) -> bool {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    else {
        return false;
    };

    runtime.block_on(async move {
        let Ok(client) = reqwest::Client::builder().timeout(timeout).build() else {
            return false;
        };
        let url = format!("http://{addr}/api/v1/summary");
        // Any HTTP response (2xx, 401, anything) means a server is there. Only a
        // transport error (refused / timeout / DNS) means "nothing serving".
        client.get(url).send().await.is_ok()
    })
}

/// Non-blocking serve-start: start `serve::run` on a background thread bound to a
/// verified-free port, poll reachability, and return the live
/// [`ServerSessionDescriptor`] (D3, E4).
///
/// **Approach + limits.** `serve::run` binds its listener *internally* and then
/// blocks in `axum::serve(...).await` until a shutdown signal, so it neither
/// returns the bound address nor yields control. Rather than fork/reimplement
/// serve, this helper:
///
/// 1. Selects a verified-free address up front via
///    [`serve::probe_free_port`] (preferring `preferred`, else an OS-assigned
///    ephemeral loopback port).
/// 2. Spawns a dedicated **OS thread** that owns its own multi-thread tokio
///    runtime and runs `serve::run` with `explicit_listen = true` bound to that
///    exact address. `serve::run` sets `SO_REUSEADDR`, so the rebind into the
///    just-probed port is robust against the tiny probe-then-bind window. The
///    thread (and its server) live for the lifetime of the process — this is a
///    start-on-demand helper, not a managed lifecycle (no shutdown handle is
///    returned; stopping is process exit, matching D3's "no OS service unit"
///    scope).
/// 3. Polls [`operator_server_reachable`] on the chosen address until it serves
///    or `deadline` elapses, returning the reachable descriptor (FR-020) or an
///    error if it never came up.
///
/// **Limit (documented):** there is no graceful-stop handle — the spawned server
/// runs until the process exits. The probe-then-bind on step 1 has the documented
/// small rebind window of [`serve::probe_free_port`]; `SO_REUSEADDR` plus the
/// immediate rebind makes a steal vanishingly unlikely, and the reachability gate
/// in step 3 means we never report a URL that did not actually come up.
///
/// `api_key` / `api_key_env` are threaded straight into [`ServeArgs`] so a network
/// bind can carry a key sourced from the environment (the wizard never passes an
/// inline key on a routable bind — `serve::run` refuses that anyway).
pub fn start_operator_server(
    preferred: Option<SocketAddr>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    deadline: Duration,
) -> anyhow::Result<ServerSessionDescriptor> {
    // Step 1: pick a verified-free address before spawning, so we know the bound
    // address without needing serve::run to report it back. This runs on the
    // CALLER's thread, which has no ambient tokio reactor, so we cannot use
    // `serve::probe_free_port` here (it builds a `tokio::net::TcpListener`, which
    // panics outside a runtime). Instead select the address with a reactor-free
    // `std` bind — the same probe-then-bind decision logic, the same documented
    // small rebind window; the real serve bind (inside the spawned runtime) still
    // uses `bind_listener` with `SO_REUSEADDR`, which makes the rebind robust.
    let bound_addr = select_free_addr_std(preferred)
        .map_err(|e| anyhow::anyhow!("could not select a free operator-server port: {e}"))?;

    // Step 2: run serve::run on its own thread + runtime, bound to that exact
    // address. `explicit_listen = true` makes serve honor the address exactly
    // (it would otherwise fall back on an occupied default — but we already
    // verified this address is free).
    let serve_args = ServeArgs {
        listen: bound_addr.to_string(),
        explicit_listen: true,
        api_key,
        api_key_env,
    };
    std::thread::Builder::new()
        .name(format!("symforge-serve-{}", bound_addr.port()))
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(error) => {
                    tracing::error!(%error, "operator serve-start: failed to build runtime");
                    return;
                }
            };
            if let Err(error) = runtime.block_on(serve::run(serve_args)) {
                // The reachability poll below is the real success signal; a serve
                // failure simply means the poll times out and the caller errors.
                tracing::error!(%error, "operator serve-start: serve::run exited with error");
            }
        })
        .map_err(|e| anyhow::anyhow!("could not spawn operator-server thread: {e}"))?;

    // Step 3: poll reachability until the server answers or the deadline elapses.
    let start = Instant::now();
    let probe_timeout = Duration::from_millis(250);
    let poll_interval = Duration::from_millis(50);
    while start.elapsed() < deadline {
        if operator_server_reachable(bound_addr, probe_timeout) {
            return Ok(ServerSessionDescriptor::for_addr(bound_addr, true));
        }
        std::thread::sleep(poll_interval);
    }

    anyhow::bail!("operator server did not become reachable on {bound_addr} within {deadline:?}")
}

/// Select a verified-free loopback address using only `std` (no tokio reactor),
/// preferring `preferred`. Mirrors the decision logic of `serve::probe_free_port`
/// but with a plain `std::net::TcpListener` so it is callable from a synchronous
/// CLI context (the caller of [`start_operator_server`] has no ambient runtime).
///
/// Attempts to bind `preferred`; on success returns that address, on failure (the
/// port is occupied) binds `127.0.0.1:0` for an OS-assigned ephemeral port. The
/// probe listener is dropped before returning, so the same documented small
/// rebind window as `serve::probe_free_port` applies — closed in practice by the
/// `SO_REUSEADDR` rebind inside `serve::run` plus the step-3 reachability gate.
fn select_free_addr_std(preferred: Option<SocketAddr>) -> std::io::Result<SocketAddr> {
    if let Some(addr) = preferred
        && let Ok(listener) = std::net::TcpListener::bind(addr)
    {
        let local = listener.local_addr()?;
        // `:0` was requested explicitly via `preferred` → resolve the concrete port.
        return Ok(local);
    }
    let ephemeral: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let listener = std::net::TcpListener::bind(ephemeral)?;
    listener.local_addr()
}

/// Flags for `symforge admin` (see `contracts/admin-cli.md`).
#[derive(Args, Debug, Clone)]
pub struct AdminCliArgs {
    /// Do not attempt to open a browser; print/return the dashboard URL only.
    #[arg(long)]
    pub no_open: bool,
}

/// Entry point for `symforge admin`.
///
/// Phase 3 lands the shared reachability + serve-start helpers above; the full
/// reachability -> reuse/start -> open flow lands in Phase US3 (T023). Until then
/// this returns a clear not-yet-implemented error rather than a fake success.
pub fn run(_args: AdminCliArgs) -> anyhow::Result<()> {
    anyhow::bail!(
        "symforge admin: the dashboard admin verb is not yet implemented (009 US3). \
         Start the server with `symforge serve` and open the printed `/admin` URL."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_derives_urls_from_bound_addr() {
        let addr: SocketAddr = "127.0.0.1:8787".parse().unwrap();
        let desc = ServerSessionDescriptor::for_addr(addr, true);
        assert_eq!(desc.dashboard_url, "http://127.0.0.1:8787/admin");
        assert_eq!(desc.attach_url, "http://127.0.0.1:8787/mcp");
        assert!(desc.reachable);
        assert_eq!(desc.bound_addr, addr);
    }

    #[test]
    fn reachable_false_on_dead_port() {
        // Reserve an ephemeral port, then free it: nothing serves there, so the
        // HTTP probe must report not-reachable (connection refused), not hang.
        let scratch = std::net::TcpListener::bind("127.0.0.1:0").expect("scratch bind");
        let dead = scratch.local_addr().expect("local_addr");
        drop(scratch);

        assert!(
            !operator_server_reachable(dead, Duration::from_millis(300)),
            "a freed port has no server; reachability must be false"
        );
    }

    #[test]
    fn reachable_true_against_a_real_serve() {
        // Start a real operator server on a verified-free loopback port (no key =
        // loopback open), then confirm both the start helper's descriptor and a
        // standalone reachability probe agree it is serving.
        let preferred = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let desc = start_operator_server(Some(preferred), None, None, Duration::from_secs(10))
            .expect("operator server should become reachable");

        assert!(desc.reachable);
        assert!(desc.bound_addr.ip().is_loopback());
        assert_ne!(desc.bound_addr.port(), 0, "a concrete port was bound");
        assert_eq!(
            desc.dashboard_url,
            format!("http://{}/admin", desc.bound_addr)
        );

        // A standalone probe of the same address also sees the live server (the
        // reuse path US3 depends on). `/api/v1/summary` is unauth-open on a
        // keyless loopback serve, so this is a 200; either way a response = alive.
        assert!(
            operator_server_reachable(desc.bound_addr, Duration::from_millis(500)),
            "a standalone reachability probe must see the running server"
        );
        // The server thread lives until process exit; the test does not stop it
        // (start-on-demand has no graceful-stop handle by design, D3).
    }
}
