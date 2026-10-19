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
use crate::domain::ControlStateDir;
use crate::live_index::store::SharedIndex;

const SIDECAR_BIND_ENV: &str = "SYMFORGE_SIDECAR_BIND";
const SIDECAR_ALLOW_NON_LOOPBACK_ENV: &str = "SYMFORGE_SIDECAR_ALLOW_NON_LOOPBACK";
const SIDECAR_TOKEN_ENV: &str = "SYMFORGE_SIDECAR_TOKEN";

/// Successful bind decision. Refused non-loopback hosts are `Err` from
/// [`resolve_sidecar_bind`] so a listener is unrepresentable on those arms.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SidecarListenerKind {
    Loopback { host: String },
    NonLoopbackAuthenticated { host: String, token: String },
}

impl SidecarListenerKind {
    fn host(&self) -> &str {
        match self {
            Self::Loopback { host } | Self::NonLoopbackAuthenticated { host, .. } => host,
        }
    }

    fn bearer_token(self) -> Option<String> {
        match self {
            Self::Loopback { .. } => None,
            Self::NonLoopbackAuthenticated { token, .. } => Some(token),
        }
    }
}

fn env_var_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Resolve bind host from `SYMFORGE_SIDECAR_BIND` (default `bind_host`).
///
/// Loopback starts unauthenticated. Non-loopback requires
/// `SYMFORGE_SIDECAR_ALLOW_NON_LOOPBACK` and a non-empty
/// `SYMFORGE_SIDECAR_TOKEN`. Does not read `SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK`.
fn resolve_sidecar_bind(default_host: &str) -> anyhow::Result<SidecarListenerKind> {
    let configured = std::env::var(SIDECAR_BIND_ENV).unwrap_or_else(|_| default_host.to_string());
    let host = configured.trim();
    if host.is_empty() {
        anyhow::bail!("{SIDECAR_BIND_ENV} must not be empty");
    }

    if crate::paths::is_loopback_bind_host(host) {
        return Ok(SidecarListenerKind::Loopback {
            host: host.to_string(),
        });
    }

    if !env_var_truthy(SIDECAR_ALLOW_NON_LOOPBACK_ENV) {
        anyhow::bail!(
            "refusing non-loopback sidecar bind '{host}'; set {SIDECAR_ALLOW_NON_LOOPBACK_ENV}=1 to allow it explicitly"
        );
    }

    let token = std::env::var(SIDECAR_TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(token) = token else {
        anyhow::bail!(
            "refusing non-loopback sidecar bind '{host}'; set {SIDECAR_TOKEN_ENV} when {SIDECAR_ALLOW_NON_LOOPBACK_ENV}=1"
        );
    };

    tracing::warn!(
        bind_host = %host,
        allow_env = SIDECAR_ALLOW_NON_LOOPBACK_ENV,
        "sidecar binding to a non-loopback interface after explicit operator opt-in; Bearer required on every route except /health"
    );

    Ok(SidecarListenerKind::NonLoopbackAuthenticated {
        host: host.to_string(),
        token,
    })
}

/// Spawn the HTTP sidecar.
///
/// 1. Resolves `SYMFORGE_SIDECAR_BIND` (default `"127.0.0.1"`) via
///    [`crate::paths::is_loopback_bind_host`] and refuses a non-loopback host
///    unless `SYMFORGE_SIDECAR_ALLOW_NON_LOOPBACK` is truthy and
///    `SYMFORGE_SIDECAR_TOKEN` is set. Refusal happens before listen.
/// 2. Calls `port_file::check_stale(bind_host)` to clean up any stale files.
/// 3. Creates a `socket2::Socket`, sets `SO_REUSEADDR`, binds to
///    `{bind_host}:0` (OS assigns the port), then listens with backlog 1024.
///    `SO_REUSEADDR` lets the bind succeed when the chosen ephemeral port is
///    still in TIME_WAIT from a recently-closed listener; this keeps rapid
///    daemon restarts and parallel test fan-out from racing on Windows.
/// 4. Hands the socket to `tokio::net::TcpListener::from_std`.
/// 5. Writes port and PID files via `port_file`.
/// 6. Creates `SidecarState` with `TokenStats` and empty symbol cache.
/// 7. Builds the axum router via `router::build_router`. Bearer is attached
///    to the whole router only for `NonLoopbackAuthenticated`.
/// 8. Spawns `axum::serve` with graceful shutdown wired to a oneshot channel.
/// 9. After the server completes, calls `port_file::cleanup_files()`.
/// 10. Returns `SidecarHandle { port, shutdown_tx, server_join, token_stats }`.
pub async fn spawn_sidecar(
    index: SharedIndex,
    bind_host: &str,
    repo_root: Option<PathBuf>,
    control_state_dir: Option<ControlStateDir>,
) -> anyhow::Result<SidecarHandle> {
    // Policy first: a refused non-loopback host must never hold a listener.
    let decision = resolve_sidecar_bind(bind_host)?;
    let resolved_host = decision.host().to_string();
    let bearer_token = decision.bearer_token();

    // Clean up stale files from a previous crashed sidecar.
    if let Some(state_dir) = control_state_dir.as_ref() {
        port_file::check_stale(state_dir, &resolved_host, repo_root.as_deref());
        // Ensure local sidecar mode does not inherit a daemon session routing file.
        port_file::cleanup_session_file(state_dir);
    }

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
    if let Some(state_dir) = control_state_dir.as_ref() {
        port_file::write_session_descriptor(state_dir, port, None, repo_root.as_deref(), None)?;
    }

    // Install panic hook to clean up port files if the process panics.
    if let Some(state_dir) = control_state_dir.as_ref() {
        let sidecar_dir = crate::paths::control_state_path(state_dir, "sidecar");
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            port_file::cleanup_own_descriptor_at(&sidecar_dir);
            previous_hook(info);
        }));
    }

    info!("sidecar listening on {resolved_host}:{port}");

    // Construct SidecarState with fresh TokenStats and empty symbol cache.
    // Keep a clone of the Arc<TokenStats> to return in SidecarHandle so the MCP server
    // can read token savings directly without an HTTP round-trip.
    let token_stats = TokenStats::new();
    let state = SidecarState {
        index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(index),
        token_stats: Arc::clone(&token_stats),
        repo_root,
        symbol_cache: Arc::new(RwLock::new(HashMap::new())),
    };

    // Bearer only when the listener is NonLoopbackAuthenticated.
    let app = router::build_router(state, bearer_token);

    // Create graceful shutdown channel.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn the server task. The returned `JoinHandle` is surfaced on
    // `SidecarHandle::server_join` so callers can deterministically await
    // listener drop via `shutdown_and_join`.
    let shutdown_control_state_dir = control_state_dir.clone();
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
        if let Some(state_dir) = shutdown_control_state_dir.as_ref() {
            port_file::cleanup_own_descriptor(state_dir);
        }
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
    use super::*;
    use std::ffi::OsString;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use crate::live_index::store::LiveIndex;

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().await
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    #[allow(unsafe_code)] // test-only env guard serializes process env mutation.
    impl EnvVarGuard {
        fn set_str(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: called only in single-threaded test context; no concurrent env readers.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: called only in single-threaded test context; no concurrent env readers.
            unsafe {
                std::env::remove_var(key);
            }
            Self { key, previous }
        }
    }

    #[allow(unsafe_code)] // test-only env guard restores serialized process env mutation.
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                // SAFETY: called only in single-threaded test context; no concurrent env readers.
                Some(previous) => unsafe {
                    std::env::set_var(self.key, previous);
                },
                // SAFETY: called only in single-threaded test context; no concurrent env readers.
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    struct VecWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for VecWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("warn buffer").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn capture_warns(f: impl FnOnce()) -> String {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let writer_buf = Arc::clone(&buf);
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || VecWriter(Arc::clone(&writer_buf)))
            .with_max_level(tracing::Level::WARN)
            .with_target(false)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        String::from_utf8(buf.lock().expect("warn buffer").clone()).expect("warn utf8")
    }

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

    #[tokio::test]
    async fn test_sidecar_bind_policy_rejects_non_loopback_without_opt_in() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(SIDECAR_BIND_ENV, "0.0.0.0");
        let _allow_guard = EnvVarGuard::unset(SIDECAR_ALLOW_NON_LOOPBACK_ENV);
        let _token_guard = EnvVarGuard::unset(SIDECAR_TOKEN_ENV);

        let error = resolve_sidecar_bind("127.0.0.1")
            .expect_err("non-loopback bind must require explicit opt-in");
        let message = error.to_string();
        assert!(
            message.contains("refusing non-loopback sidecar bind"),
            "unexpected bind rejection: {message}"
        );
        assert!(
            message.contains(SIDECAR_ALLOW_NON_LOOPBACK_ENV),
            "rejection should name the explicit opt-in env var: {message}"
        );

        let spawn_error = match spawn_sidecar(LiveIndex::empty(), "127.0.0.1", None, None).await {
            Err(error) => error,
            Ok(_) => panic!("spawn must fail before listen when non-loopback is refused"),
        };
        let spawn_message = spawn_error.to_string();
        assert!(
            spawn_message.contains("refusing non-loopback sidecar bind"),
            "spawn rejection must match resolve: {spawn_message}"
        );
    }

    #[tokio::test]
    async fn test_sidecar_bind_policy_allows_non_loopback_with_explicit_opt_in() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(SIDECAR_BIND_ENV, "0.0.0.0");
        let _allow_guard = EnvVarGuard::set_str(SIDECAR_ALLOW_NON_LOOPBACK_ENV, "1");
        let _token_guard = EnvVarGuard::set_str(SIDECAR_TOKEN_ENV, "sidecar-test-token");

        let logged = capture_warns(|| {
            let resolved = resolve_sidecar_bind("127.0.0.1")
                .expect("explicit opt-in plus token should allow non-loopback bind");
            match resolved {
                SidecarListenerKind::NonLoopbackAuthenticated { host, token } => {
                    assert_eq!(host, "0.0.0.0");
                    assert_eq!(token, "sidecar-test-token");
                }
                SidecarListenerKind::Loopback { host } => {
                    panic!("expected authenticated non-loopback, got loopback {host}")
                }
            }
        });
        assert!(
            logged.contains("non-loopback"),
            "opt-in plus token must warn that the bind is off-loopback: {logged}"
        );

        let handle = spawn_sidecar(LiveIndex::empty(), "127.0.0.1", None, None)
            .await
            .expect("opt-in plus token must start the sidecar");
        assert!(handle.port > 0, "authenticated non-loopback must bind");
        handle.shutdown_and_join().await;
    }

    #[tokio::test]
    async fn test_sidecar_bind_policy_rejects_non_loopback_opt_in_without_token() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(SIDECAR_BIND_ENV, "0.0.0.0");
        let _allow_guard = EnvVarGuard::set_str(SIDECAR_ALLOW_NON_LOOPBACK_ENV, "1");
        let _token_guard = EnvVarGuard::unset(SIDECAR_TOKEN_ENV);

        let error = resolve_sidecar_bind("127.0.0.1")
            .expect_err("non-loopback opt-in without a token must fail");
        let message = error.to_string();
        assert!(
            message.contains("refusing non-loopback sidecar bind"),
            "unexpected bind rejection: {message}"
        );
        assert!(
            message.contains(SIDECAR_TOKEN_ENV),
            "rejection should name the token env var: {message}"
        );

        if spawn_sidecar(LiveIndex::empty(), "127.0.0.1", None, None)
            .await
            .is_ok()
        {
            panic!("spawn must fail before listen when the token is missing");
        }
    }

    #[tokio::test]
    async fn test_sidecar_bind_policy_rejects_empty_or_whitespace_token() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(SIDECAR_BIND_ENV, "0.0.0.0");
        let _allow_guard = EnvVarGuard::set_str(SIDECAR_ALLOW_NON_LOOPBACK_ENV, "1");
        for empty in ["", "   "] {
            let _token_guard = EnvVarGuard::set_str(SIDECAR_TOKEN_ENV, empty);
            resolve_sidecar_bind("127.0.0.1")
                .expect_err("empty or whitespace sidecar token must count as missing");
            if spawn_sidecar(LiveIndex::empty(), "127.0.0.1", None, None)
                .await
                .is_ok()
            {
                panic!("spawn must fail before listen when the token is empty");
            }
        }
    }

    #[tokio::test]
    async fn test_sidecar_bind_policy_loopback_does_not_require_token() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(SIDECAR_BIND_ENV, "127.0.0.1");
        let _allow_guard = EnvVarGuard::unset(SIDECAR_ALLOW_NON_LOOPBACK_ENV);
        let _token_guard = EnvVarGuard::unset(SIDECAR_TOKEN_ENV);

        let resolved = resolve_sidecar_bind("127.0.0.1")
            .expect("loopback bind must start without a sidecar token");
        match resolved {
            SidecarListenerKind::Loopback { host } => assert_eq!(host, "127.0.0.1"),
            SidecarListenerKind::NonLoopbackAuthenticated { host, .. } => {
                panic!("loopback must not require Bearer, got authenticated {host}")
            }
        }

        let handle = spawn_sidecar(LiveIndex::empty(), "127.0.0.1", None, None)
            .await
            .expect("loopback sidecar must start without SYMFORGE_SIDECAR_TOKEN");
        assert!(handle.port > 0, "loopback spawn must bind a port");
        handle.shutdown_and_join().await;
    }

    #[tokio::test]
    async fn test_sidecar_bind_policy_loopback_with_token_has_no_bearer() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(SIDECAR_BIND_ENV, "127.0.0.1");
        let _allow_guard = EnvVarGuard::unset(SIDECAR_ALLOW_NON_LOOPBACK_ENV);
        let _token_guard = EnvVarGuard::set_str(SIDECAR_TOKEN_ENV, "sidecar-test-token");

        let resolved = resolve_sidecar_bind("127.0.0.1")
            .expect("loopback must stay startable when a token env is set");
        match resolved {
            SidecarListenerKind::Loopback { host } => assert_eq!(host, "127.0.0.1"),
            SidecarListenerKind::NonLoopbackAuthenticated { host, .. } => {
                panic!("token env must not flip loopback to authenticated, got {host}")
            }
        }

        let handle = spawn_sidecar(LiveIndex::empty(), "127.0.0.1", None, None)
            .await
            .expect("loopback sidecar must start when a token env is set");
        let port = handle.port;
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/workflows/source-read"))
            .send()
            .await
            .expect("loopback source-read without Authorization");
        assert_ne!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "loopback must not attach Bearer when SYMFORGE_SIDECAR_TOKEN is set"
        );

        handle.shutdown_and_join().await;
    }
}
