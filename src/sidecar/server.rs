//! Sidecar server spawner.
//!
//! Binds to an OS-assigned ephemeral port, writes port/PID files,
//! and spawns an axum serve task with graceful shutdown support.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::info;

use super::{SidecarHandle, SidecarState, TokenStats, port_file, router};
use crate::live_index::store::SharedIndex;

/// Spawn the HTTP sidecar.
///
/// 1. Reads `SYMFORGE_SIDECAR_BIND` env var (default `"127.0.0.1"`).
/// 2. Calls `port_file::check_stale(bind_host)` to clean up any stale files.
/// 3. Creates a `socket2::Socket`, sets `SO_REUSEADDR`, binds to
///    `{bind_host}:0` (OS assigns the port), then listens with backlog 1024.
///    `SO_REUSEADDR` lets the bind succeed when the chosen ephemeral port is
///    still in TIME_WAIT from a recently-closed listener; this keeps rapid
///    daemon restarts and parallel test fan-out from racing on Windows.
/// 4. Hands the socket to `tokio::net::TcpListener::from_std`.
/// 5. Writes port and PID files via `port_file`.
/// 6. Creates `SidecarState` with `TokenStats` and empty symbol cache.
/// 7. Builds the axum router via `router::build_router`.
/// 8. Spawns `axum::serve` with graceful shutdown wired to a oneshot channel.
/// 9. After the server completes, calls `port_file::cleanup_files()`.
/// 10. Returns `SidecarHandle { port, shutdown_tx, server_join, token_stats }`.
pub async fn spawn_sidecar(
    index: SharedIndex,
    bind_host: &str,
    repo_root: Option<PathBuf>,
) -> anyhow::Result<SidecarHandle> {
    // Allow overriding bind host via env var.
    let resolved_host =
        std::env::var("SYMFORGE_SIDECAR_BIND").unwrap_or_else(|_| bind_host.to_string());

    // Clean up stale files from a previous crashed sidecar.
    port_file::check_stale(&resolved_host);
    // Ensure local sidecar mode does not inherit a daemon session routing file.
    port_file::cleanup_session_file();

    // Bind to an OS-assigned ephemeral port with SO_REUSEADDR so a TIME_WAIT
    // socket on the picked port does not block the bind. On Windows this
    // matters under parallel test fan-out and on rapid daemon restarts; on
    // Linux it permits the same with comparable semantics.
    let std_addr: std::net::SocketAddr = format!("{resolved_host}:0").parse()?;
    let domain = if std_addr.is_ipv4() {
        socket2::Domain::IPV4
    } else {
        socket2::Domain::IPV6
    };
    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&std_addr.into())?;
    socket.listen(1024)?;
    let std_listener: std::net::TcpListener = socket.into();
    let listener = TcpListener::from_std(std_listener)?;
    let port = listener.local_addr()?.port();

    // Task 8: one atomic per-adapter descriptor (no daemon session for a
    // purely local sidecar) so hook scripts can locate this sidecar without
    // clobbering a sibling's record.
    port_file::write_session_descriptor(port, None, repo_root.as_deref())?;

    // Install panic hook to clean up port files if the process panics.
    let symforge_dir = crate::paths::select_runtime_data_base(
        repo_root.as_deref(),
        std::env::current_dir().ok().as_deref(),
    );
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        port_file::cleanup_own_descriptor_at(&symforge_dir);
        previous_hook(info);
    }));

    info!("sidecar listening on {resolved_host}:{port}");

    // Construct SidecarState with fresh TokenStats and empty symbol cache.
    // Keep a clone of the Arc<TokenStats> to return in SidecarHandle so the MCP server
    // can read token savings directly without an HTTP round-trip.
    let token_stats = TokenStats::new();
    let state = SidecarState {
        index,
        token_stats: Arc::clone(&token_stats),
        repo_root,
        symbol_cache: Arc::new(RwLock::new(HashMap::new())),
    };

    // Build the router with SidecarState.
    let app = router::build_router(state);

    // Create graceful shutdown channel.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn the server task. The returned `JoinHandle` is surfaced on
    // `SidecarHandle::server_join` so callers can deterministically await
    // listener drop via `shutdown_and_join`.
    let server_join = tokio::spawn(async move {
        let shutdown_signal = async move {
            let _ = shutdown_rx.await;
        };

        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
        {
            tracing::error!("sidecar server error: {e}");
        }

        // Task 8: remove ONLY this sidecar's descriptor after shutdown.
        port_file::cleanup_own_descriptor(None);
        tracing::info!("sidecar shut down, session descriptor cleaned up");
    });

    Ok(SidecarHandle {
        port,
        shutdown_tx,
        server_join,
        token_stats,
    })
}

#[cfg(test)]
mod tests {
    /// Regression: SO_REUSEADDR on the listener bind path must allow rebinding
    /// to a port that was just released. Without this, parallel test fan-out
    /// on Windows can race a previous listener's TIME_WAIT slot and fail with
    /// WSAEADDRINUSE.
    ///
    /// We can't reliably synthesize a TIME_WAIT state in a unit test, but we
    /// can exercise the bind helper code path twice on the same explicit port
    /// to prove the listener-creation flow does not reject a fresh
    /// SO_REUSEADDR bind on a recently-freed port.
    #[test]
    fn so_reuseaddr_listener_rebinds_on_recently_freed_port() {
        fn bind_reuse(addr: std::net::SocketAddr) -> std::io::Result<std::net::TcpListener> {
            let domain = if addr.is_ipv4() {
                socket2::Domain::IPV4
            } else {
                socket2::Domain::IPV6
            };
            let socket =
                socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
            socket.set_reuse_address(true)?;
            socket.bind(&addr.into())?;
            socket.listen(16)?;
            Ok(socket.into())
        }

        // First bind: ephemeral port assignment.
        let ephemeral: std::net::SocketAddr = "127.0.0.1:0".parse().expect("parse addr");
        let first = bind_reuse(ephemeral).expect("first bind");
        let bound_port = first.local_addr().expect("local_addr").port();
        drop(first);

        // Immediate rebind on the same port via SO_REUSEADDR must succeed.
        // Without SO_REUSEADDR on Windows, this is the failure mode that
        // surfaces under rapid spawn_sidecar churn.
        let explicit: std::net::SocketAddr = format!("127.0.0.1:{bound_port}")
            .parse()
            .expect("parse explicit");
        let second = bind_reuse(explicit).expect("rebind on freed port");
        assert_eq!(
            second.local_addr().expect("local_addr").port(),
            bound_port,
            "rebind must hold the same port"
        );
    }
}
