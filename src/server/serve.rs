//! `symforge serve` async entrypoint and socket binding.
//!
//! Phase-2 scope (this file): resolve the API key, compute loopback, enforce the
//! secure-default refuse-to-start rule, and provide [`bind_listener`] (the
//! socket2 + `SO_REUSEADDR` bind, mirroring [`crate::sidecar::server::spawn_sidecar`]).
//!
//! US1/T013-T016 will extend [`run`] to build the [`ServerRuntime`], mount the
//! `/mcp` router + auth layer, print the attach URL, and run with graceful
//! shutdown. For now [`run`] performs the secure-startup checks and returns
//! `Ok(())` with a "not yet fully implemented" notice — the secure-default
//! behavior (refuse-to-start, key resolution, loopback computation) is already
//! real and exercised before any listener is opened.

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;

use super::api_keys::ApiKeyStore;
use super::auth::{AuthConfig, AuthLayerState, OriginLayerState};
use super::{ServerRuntime, admin, mcp_http};
use crate::live_index::{LiveIndex, SharedIndex};
use crate::protocol::SymForgeServer;
use crate::sidecar::governor::RequestGovernor;
use crate::stel::ledger_store::StelLedgerStore;
use crate::watcher::WatcherInfo;

/// The default `--listen` bind address (loopback, fixed product port).
pub const DEFAULT_LISTEN: &str = "127.0.0.1:8787";

/// Resolved inputs for the `serve` subcommand.
#[derive(Debug, Clone)]
pub struct ServeArgs {
    /// `HOST:PORT` to bind. `PORT=0` requests an OS-assigned port.
    pub listen: String,
    /// Inline API key (`--api-key`).
    pub api_key: Option<String>,
    /// Name of an env var holding the API key (`--api-key-env`); used only when
    /// `api_key` is `None`.
    pub api_key_env: Option<String>,
}

impl Default for ServeArgs {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN.to_string(),
            api_key: None,
            api_key_env: None,
        }
    }
}

/// Resolve the effective API key: `--api-key` wins, else `--api-key-env` (read
/// from the environment), else `None`.
pub fn resolve_api_key(api_key: Option<&str>, api_key_env: Option<&str>) -> Option<String> {
    if let Some(key) = api_key
        && !key.is_empty()
    {
        return Some(key.to_string());
    }
    if let Some(var) = api_key_env
        && let Ok(value) = std::env::var(var)
        && !value.is_empty()
    {
        return Some(value);
    }
    None
}

/// Enforce the inline-`--api-key` source policy (P2-E).
///
/// An inline `--api-key` is visible in process listings (`ps` / Windows Task
/// Manager / `/proc/<pid>/cmdline`), so it is a secret-leak vector on a routable
/// bind. This applies two rules, mirroring the secure-default refuse-to-start
/// rule but for the *key source* rather than the key's presence:
///
/// 1. **Warn** whenever a non-empty inline `--api-key` is used (any bind),
///    recommending `--api-key-env <VAR>` which keeps the secret out of argv.
/// 2. **Refuse** an inline `--api-key` on a **non-loopback** (network) bind:
///    a routable bind must source the key from the environment. Loopback binds
///    may still accept an inline key for local convenience.
///
/// `is_loopback` is computed by the caller from the parsed bind address. The
/// warning is emitted via `tracing::warn!` AND to stderr so an operator running
/// without a tracing subscriber still sees it. Returns
/// [`AuthStartupError::InlineKeyOnNonLoopback`] on a refused config so `run`
/// exits before binding.
pub fn enforce_api_key_source_policy(
    api_key: Option<&str>,
    is_loopback: bool,
) -> Result<(), super::auth::AuthStartupError> {
    let inline_present = matches!(api_key, Some(key) if !key.is_empty());
    if !inline_present {
        return Ok(());
    }

    if !is_loopback {
        // Routable bind + inline key: refuse before binding. The operator must
        // pass --api-key-env so the secret never lands in argv.
        return Err(super::auth::AuthStartupError::InlineKeyOnNonLoopback);
    }

    // Loopback + inline key: allowed, but warn — inline keys are visible in
    // process listings even locally; --api-key-env is the recommended path.
    let msg = "WARNING: --api-key was passed inline; it is visible in process listings \
        (ps / Task Manager). Prefer --api-key-env <VAR> so the secret stays out of argv.";
    tracing::warn!("{msg}");
    eprintln!("{msg}");
    Ok(())
}

/// Whether a parsed bind address is loopback (`127.0.0.0/8` or `::1`).
// REVIEW P3-D (deferred): `IpAddr::is_loopback()` is `false` for an IPv4-mapped
// IPv6 loopback (`[::ffff:127.0.0.1]`). This is currently safe — with a key it
// binds (fine); without a key it refuses (secure default). Optional future fix:
// normalize an IPv4-mapped loopback to its IPv4 form before the policy check.
pub fn is_loopback_addr(addr: &SocketAddr) -> bool {
    addr.ip().is_loopback()
}

/// Bind a [`tokio::net::TcpListener`] on `addr` with `SO_REUSEADDR`.
///
/// Mirrors the socket setup in [`crate::sidecar::server::spawn_sidecar`]:
/// create a `socket2::Socket`, set `SO_REUSEADDR` (so a TIME_WAIT socket on the
/// chosen port does not block the bind under rapid restarts / parallel test
/// fan-out), set non-blocking, bind, listen with backlog 1024, then hand the
/// std socket to tokio. Unlike the sidecar (which always binds `:0`), this
/// honors the operator-chosen port from `--listen` (and `:0` for tests).
pub fn bind_listener(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let domain = if addr.is_ipv4() {
        socket2::Domain::IPV4
    } else {
        socket2::Domain::IPV6
    };
    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    let std_listener: std::net::TcpListener = socket.into();
    tokio::net::TcpListener::from_std(std_listener)
}

/// Error returned by [`run`] before/while binding the operator server.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// `--listen` could not be parsed as a `HOST:PORT` socket address.
    #[error("invalid --listen address {addr:?}: {source}")]
    InvalidListen {
        addr: String,
        #[source]
        source: std::net::AddrParseError,
    },
    /// Secure-default policy refused the requested bind (non-loopback, no key).
    #[error(transparent)]
    Startup(#[from] super::auth::AuthStartupError),
    /// The socket could not be bound (e.g. address already in use).
    #[error("failed to bind {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    /// The project index could not be loaded for serving.
    #[error("failed to load project index for serving: {source}")]
    IndexLoad {
        #[source]
        source: anyhow::Error,
    },
    /// The axum server returned an error while serving.
    #[error("operator server error: {source}")]
    Serve {
        #[source]
        source: std::io::Error,
    },
}

/// Build the in-process [`SharedIndex`] for serving.
///
/// Resolves the project root (`discovery::find_project_root`) and loads it
/// synchronously (the same `LiveIndex::load` the stdio local path uses). When no
/// safe root is found, serves over an empty index — `tools/list` still responds,
/// and the operator can `index_folder` after attaching. Returns the index and
/// the resolved root (for the STEL ledger store location and the project name).
fn load_serve_index() -> Result<(SharedIndex, Option<std::path::PathBuf>), ServeError> {
    match crate::discovery::find_project_root() {
        Some(root) => {
            let index =
                LiveIndex::load(&root).map_err(|source| ServeError::IndexLoad { source })?;
            Ok((index, Some(root)))
        }
        None => Ok((LiveIndex::empty(), None)),
    }
}

/// Construct the [`ServerRuntime`] from a resolved index + auth config.
///
/// Builds the **same** [`SymForgeServer`] the stdio path constructs (one shared
/// dispatcher, no logic fork), a default [`RequestGovernor`], and opens the
/// durable STEL [`StelLedgerStore`] under the project's symforge data dir. If the
/// store cannot open it degrades to [`StelLedgerStore::Disabled`] (serving
/// continues — FR-011). When there is no project root the ledger store is left
/// `None` (no data dir to anchor it).
///
/// US3/T028+T029: the durable store is opened **before** the protocol
/// dispatcher so the same `Arc<StelLedgerStore>` is shared into both the
/// dispatcher (write-through on each `symforge`/`symforge_edit` invocation via
/// `finalize_symforge_with_ledger`) and the [`ServerRuntime`] (read path for
/// the `status` summary). One physical store = one durable ledger path, so no
/// economics row is counted twice across the in-memory and durable sinks.
fn build_serve_runtime(
    index: SharedIndex,
    repo_root: Option<std::path::PathBuf>,
    auth: AuthConfig,
) -> ServerRuntime {
    let project_name = repo_root
        .as_ref()
        .and_then(|root| root.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string();

    // US3: open the durable economics ledger under the project data dir FIRST,
    // so the opened handle can be shared with both the dispatcher and the
    // runtime. A failure here degrades to Disabled inside `StelLedgerStore::open`,
    // so the server still starts (FR-011).
    let ledger_store: Option<Arc<StelLedgerStore>> =
        repo_root
            .as_ref()
            .and_then(|root| match crate::paths::ensure_symforge_dir(root) {
                Ok(dir) => Some(Arc::new(StelLedgerStore::open(
                    &dir,
                    format!("serve-{}", std::process::id()),
                ))),
                Err(error) => {
                    tracing::warn!(
                        root = %root.display(),
                        %error,
                        "could not ensure symforge data dir; STEL ledger will not persist"
                    );
                    None
                }
            });

    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let mut protocol = SymForgeServer::new(
        Arc::clone(&index),
        project_name,
        watcher_info,
        repo_root.clone(),
        None,
    );
    // Share the SAME store allocation with the dispatcher so durable
    // write-through (T028) and the runtime summary read (T029) use one path.
    if let Some(store) = ledger_store.as_ref() {
        protocol = protocol.with_stel_ledger_store(Arc::clone(store));
    }
    let protocol = Arc::new(protocol);
    // P2-F (resolved): the governor is now consulted on the `/mcp` HTTP path.
    // `mcp_http::build_mcp_router` wraps the route with `apply_governor`, which
    // acquires one concurrency permit per request from this shared governor and
    // releases it on completion — bounding concurrent operator clients to
    // `max_concurrency` (queued/shed with 503 beyond that). No longer dead.
    let governor = Arc::new(RequestGovernor::new());

    // The runtime holds a clone of the same underlying store (the `Sqlite`
    // variant shares its `Arc<Mutex<Connection>>`), so `status`'s `summary()`
    // observes exactly the rows the dispatcher wrote — surviving restart.
    let runtime_store = ledger_store.map(|store| (*store).clone());

    // 006 G-039: open the hashed product API-key store under the same data dir.
    // On failure it degrades to `Disabled` (bootstrap --api-key still works).
    // Shared by Arc into both the auth layer (minted keys authenticate at /mcp)
    // and the admin /api/v1/keys handlers.
    let key_store: Option<Arc<ApiKeyStore>> =
        repo_root
            .as_ref()
            .and_then(|root| match crate::paths::ensure_symforge_dir(root) {
                Ok(dir) => Some(Arc::new(ApiKeyStore::open(&dir))),
                Err(error) => {
                    tracing::warn!(
                        root = %root.display(),
                        %error,
                        "could not ensure symforge data dir; API-key store unavailable"
                    );
                    None
                }
            });

    let mut runtime = ServerRuntime::build_runtime(index, protocol, governor, auth, runtime_store);
    if let Some(store) = key_store {
        runtime = runtime.with_key_store(store);
    }
    runtime
}

/// `symforge serve` entrypoint (US1 — `/mcp` over Streamable HTTP).
///
/// Resolves the key, parses `--listen`, computes loopback, and enforces the
/// refuse-to-start rule **before** opening any socket. On a permitted config it
/// loads the project index, builds the [`ServerRuntime`] (the same shared
/// `SymForgeServer` stdio uses — no logic fork), mounts the `/mcp` Streamable
/// HTTP router with the Bearer auth layer in front, prints the attach URL to
/// stdout, and runs one long-lived server until a shutdown signal arrives with
/// graceful drain: SIGINT (Ctrl+C) on all platforms, plus SIGTERM on Unix so the
/// server drains under Docker/K8s/systemd (P2-B).
pub async fn run(args: ServeArgs) -> Result<(), ServeError> {
    let api_key = resolve_api_key(args.api_key.as_deref(), args.api_key_env.as_deref());
    let auth = AuthConfig::new(api_key);

    let addr: SocketAddr = args
        .listen
        .parse()
        .map_err(|source| ServeError::InvalidListen {
            addr: args.listen.clone(),
            source,
        })?;
    let is_loopback = is_loopback_addr(&addr);

    // P2-E: enforce the inline-key source policy before binding — warn on any
    // inline --api-key (recommend --api-key-env), and refuse an inline key on a
    // non-loopback bind (the secret would be visible in process listings).
    enforce_api_key_source_policy(args.api_key.as_deref(), is_loopback)?;

    // Secure default (G-033): refuse a routable bind with no key before binding.
    auth.refuse_to_start(is_loopback)?;

    // Load the shared index, then bind. Load before bind so an index failure does
    // not leave a half-open listener.
    let (index, repo_root) = load_serve_index()?;
    // Keep a copy for the onboarding state path (the original is moved into the
    // runtime builder below).
    let onboarding_root = repo_root.clone();
    let runtime = build_serve_runtime(index, repo_root, auth.clone());

    let listener = bind_listener(addr).map_err(|source| ServeError::Bind { addr, source })?;
    let local_addr = listener
        .local_addr()
        .map_err(|source| ServeError::Bind { addr, source })?;

    // Build the /mcp router plus the /admin + /api/v1 router (006), merge them,
    // and layer Bearer auth + Origin gating in front (one enforcement point —
    // secure-default rule lives on AuthConfig/AuthLayerState; P1-B Origin gating
    // lives on OriginLayerState). Layer order: the Bearer auth runs first
    // (outermost), then the Origin gate, then the routed handler.
    let mcp_router = mcp_http::build_mcp_router(&runtime, local_addr);
    let admin_router = admin::build_admin_router(&runtime);
    let merged = mcp_router.merge(admin_router);

    // Origin gate (P1-B): reject arbitrary cross-origin browser fetches against
    // the browser-facing surface; non-browser API clients send no Origin and are
    // unaffected. Allowed origins are the server's own bound address + loopback
    // aliases.
    let origin_state = OriginLayerState::from_bind_addr(local_addr);
    let gated = super::apply_origin_gate(merged, origin_state);

    // Bearer auth: the bootstrap --api-key OR any active minted key (G-039).
    let mut auth_state = AuthLayerState::new(auth, is_loopback);
    if let Some(store) = runtime.key_store() {
        auth_state = auth_state.with_key_store(Arc::clone(store));
    }
    let app = super::apply_bearer_auth(gated, auth_state);

    // Attach URL to stdout (the operator copies this into a second client).
    let attach_url = format!(
        "http://{host}:{port}{path}",
        host = local_addr.ip(),
        port = local_addr.port(),
        path = mcp_http::MCP_PATH,
    );
    println!("{attach_url}");

    // First-run / post-update onboarding banner (FR-009). Best-effort: a state
    // read/write failure never affects serve. Shows once per build version, and
    // only when anchored to a project data dir (no root => skip silently).
    if let Some(root) = onboarding_root.as_ref() {
        let state_path = crate::cli::onboarding::state_path(root);
        let mut sink = crate::cli::onboarding::StderrSink;
        crate::cli::onboarding::maybe_show_banner(
            &state_path,
            env!("CARGO_PKG_VERSION"),
            &attach_url,
            &mut sink,
        );
    }

    tracing::info!(
        addr = %local_addr,
        auth = if runtime.auth().requires_auth(is_loopback) { "required" } else { "loopback-open" },
        "operator server listening; MCP surface mounted at {}",
        mcp_http::MCP_PATH
    );

    // Graceful shutdown (P2-B). On Unix, drain on either SIGINT (Ctrl+C) or
    // SIGTERM (the signal Docker/K8s/systemd send to stop a container/unit) so
    // the server actually drains under orchestration instead of being killed.
    // On Windows, only Ctrl+C is available.
    let shutdown = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            // If SIGTERM cannot be registered, fall back to Ctrl+C only rather
            // than failing the serve loop.
            let mut sigterm = signal(SignalKind::terminate()).ok();
            match sigterm.as_mut() {
                Some(term) => {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {}
                        _ = term.recv() => {}
                    }
                }
                None => {
                    let _ = tokio::signal::ctrl_c().await;
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        tracing::info!("shutdown signal received, stopping operator server");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|source| ServeError::Serve { source })?;

    tracing::info!("operator server shut down cleanly");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::sync::Mutex;

    /// Serializes process-env mutation across the env-dependent tests in this
    /// module (in addition to the suite-wide `--test-threads=1`).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_api_key_prefers_inline_over_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "SYMFORGE_TEST_SERVE_KEY_PREFER";
        #[allow(unsafe_code)] // test-only env mutation under ENV_LOCK + --test-threads=1.
        // SAFETY: serialized by ENV_LOCK; suite runs single-threaded.
        unsafe {
            std::env::set_var(var, "from_env")
        };
        let resolved = resolve_api_key(Some("from_inline"), Some(var));
        assert_eq!(resolved.as_deref(), Some("from_inline"));
        #[allow(unsafe_code)] // test-only env restore under ENV_LOCK + --test-threads=1.
        unsafe {
            std::env::remove_var(var)
        };
    }

    #[test]
    fn resolve_api_key_falls_back_to_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "SYMFORGE_TEST_SERVE_KEY_FALLBACK";
        #[allow(unsafe_code)] // test-only env mutation under ENV_LOCK + --test-threads=1.
        unsafe {
            std::env::set_var(var, "from_env")
        };
        let resolved = resolve_api_key(None, Some(var));
        assert_eq!(resolved.as_deref(), Some("from_env"));
        #[allow(unsafe_code)] // test-only env restore under ENV_LOCK + --test-threads=1.
        unsafe {
            std::env::remove_var(var)
        };
    }

    #[test]
    fn resolve_api_key_none_when_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        let var = "SYMFORGE_TEST_SERVE_KEY_UNSET";
        #[allow(unsafe_code)] // test-only env restore under ENV_LOCK + --test-threads=1.
        unsafe {
            std::env::remove_var(var)
        };
        assert_eq!(resolve_api_key(None, Some(var)), None);
        assert_eq!(resolve_api_key(None, None), None);
        // Empty inline key is treated as unset.
        assert_eq!(resolve_api_key(Some(""), None), None);
    }

    #[test]
    fn inline_key_on_loopback_is_allowed_with_warning() {
        // P2-E: an inline key on loopback is permitted (warns, does not refuse).
        assert!(enforce_api_key_source_policy(Some("k"), true).is_ok());
    }

    #[test]
    fn inline_key_on_non_loopback_is_refused() {
        // P2-E: an inline key on a routable bind is refused (argv leak vector).
        let err = enforce_api_key_source_policy(Some("k"), false)
            .expect_err("inline key on non-loopback must refuse");
        assert_eq!(
            err,
            super::super::auth::AuthStartupError::InlineKeyOnNonLoopback
        );
    }

    #[test]
    fn no_inline_key_passes_policy_on_any_bind() {
        // No inline key: policy is a no-op regardless of bind (env/none handled
        // by the secure-default refuse-to-start rule, not this source policy).
        assert!(enforce_api_key_source_policy(None, true).is_ok());
        assert!(enforce_api_key_source_policy(None, false).is_ok());
        // Empty inline key is treated as "not provided".
        assert!(enforce_api_key_source_policy(Some(""), false).is_ok());
    }

    #[tokio::test]
    async fn run_refuses_inline_key_on_non_loopback() {
        // P2-E end-to-end: a routable bind WITH an inline key still refuses to
        // start (before binding) because the key would leak via argv.
        let args = ServeArgs {
            listen: "0.0.0.0:8787".to_string(),
            api_key: Some("inline-secret".to_string()),
            api_key_env: None,
        };
        let err = run(args)
            .await
            .expect_err("non-loopback + inline key must refuse");
        assert!(matches!(
            err,
            ServeError::Startup(super::super::auth::AuthStartupError::InlineKeyOnNonLoopback)
        ));
    }

    #[test]
    fn is_loopback_addr_classifies_v4_and_v6() {
        assert!(is_loopback_addr(&SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            8787
        )));
        assert!(is_loopback_addr(&SocketAddr::new(
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            8787
        )));
        assert!(!is_loopback_addr(&SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            8787
        )));
        assert!(!is_loopback_addr(&SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            8787
        )));
    }

    #[tokio::test]
    async fn run_refuses_non_loopback_without_key() {
        let args = ServeArgs {
            listen: "0.0.0.0:8787".to_string(),
            api_key: None,
            api_key_env: None,
        };
        let err = run(args)
            .await
            .expect_err("non-loopback + no key must refuse");
        assert!(matches!(err, ServeError::Startup(_)));
    }

    #[tokio::test]
    async fn run_rejects_unparseable_listen() {
        let args = ServeArgs {
            listen: "not-an-address".to_string(),
            api_key: Some("k".to_string()),
            api_key_env: None,
        };
        let err = run(args).await.expect_err("bad --listen must error");
        assert!(matches!(err, ServeError::InvalidListen { .. }));
    }

    #[tokio::test]
    async fn bind_listener_binds_ephemeral_loopback_port() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = bind_listener(addr).expect("bind ephemeral loopback");
        let local = listener.local_addr().expect("local_addr");
        assert!(local.ip().is_loopback());
        assert_ne!(local.port(), 0, "OS must assign a concrete port");
    }
}
