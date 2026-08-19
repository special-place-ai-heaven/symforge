//! `symforge serve` async entrypoint and socket binding.
//!
//! [`run`] resolves the API key, computes loopback, enforces the secure-default
//! refuse-to-start rule, binds via [`bind_listener`]/[`probe_free_listener`],
//! then builds the [`ServerRuntime`], mounts the `/mcp` + `/admin`/`/api/v1`
//! routers behind Bearer auth and Origin gating, prints the attach URL, and
//! serves until shutdown.

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;

use super::api_keys::ApiKeyStore;
use super::auth::{AuthConfig, AuthLayerState, OriginLayerState};
use super::{ServerRuntime, admin, mcp_http};
use crate::domain::{
    ControlStateDir, RootCandidateSource, RootRequestMode, RootResolution, StatePlacement,
};
use crate::live_index::{LiveIndex, SharedIndex};
use crate::protocol::SymForgeServer;
use crate::sidecar::governor::RequestGovernor;
use crate::stel::ledger_store::StelLedgerStore;
use crate::watcher::WatcherInfo;

/// The default `--listen` bind address (loopback, fixed product port).
pub const DEFAULT_LISTEN: &str = "127.0.0.1:8787";

/// Resolved inputs for the `serve` subcommand.
///
/// Deliberately NOT `Clone`: `bound_addr_tx` is a `sync_channel(1)` rendezvous
/// end. Cloning would let two `run()` calls share one channel — the second
/// `try_send` after the first receiver already consumed its slot would be
/// silently dropped, so the second call's caller would wait out its own
/// deadline for an address that was never coming. One `ServeArgs`, one `run()`.
#[derive(Debug)]
pub struct ServeArgs {
    /// `HOST:PORT` to bind. `PORT=0` requests an OS-assigned port.
    pub listen: String,
    /// Whether the operator explicitly supplied `--listen` (US1, FR-002/003).
    ///
    /// `false` means `listen` is the historical default ([`DEFAULT_LISTEN`]):
    /// serve prefers it but, if occupied, falls back to an OS-assigned free port
    /// rather than failing (no dead second listener). `true` means the operator
    /// chose the address, so it is honored exactly and an occupied port fails
    /// loudly (no silent substitution).
    pub explicit_listen: bool,
    /// Inline API key (`--api-key`).
    pub api_key: Option<String>,
    /// Name of an env var holding the API key (--api-key-env); used only when
    /// `api_key` is `None`.
    pub api_key_env: Option<String>,
    /// Optional one-shot report of the address this run actually bound.
    ///
    /// A background starter otherwise cannot learn the bound address without
    /// pre-binding a port itself, dropping the listener, and asking this run to
    /// re-bind the same number — a gap in which any other process can take it.
    /// Sending the address from inside `run`, once the listener is live, closes
    /// that gap: the caller is TOLD, never guesses. Send failures are ignored
    /// (the caller stopped caring), so this never affects serving.
    ///
    /// `Err` carries a bind failure (rendered, not the typed [`ServeError`] —
    /// keeps this channel `Send`-simple) so a starter with an `explicit_listen`
    /// occupied port learns that immediately instead of waiting out its full
    /// deadline for a report that a bind error already prevented.
    pub bound_addr_tx: Option<std::sync::mpsc::SyncSender<Result<SocketAddr, String>>>,
    /// Explicit workspace root to index and serve, skipping CWD discovery.
    ///
    /// `None` (every production path) keeps [`crate::discovery::find_project_root`]
    /// behaviour: walk up from the process CWD to the containing git root and
    /// index that whole tree. Tests pass a small fixture root here so a bind /
    /// reuse assertion does not pay for a cold scan of the host checkout —
    /// which on a large repository or a slow runner exceeds the start deadline
    /// even though nothing under test involves the index's contents.
    pub workspace_root: Option<std::path::PathBuf>,
}

impl Default for ServeArgs {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN.to_string(),
            explicit_listen: false,
            api_key: None,
            api_key_env: None,
            bound_addr_tx: None,
            workspace_root: None,
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
///
/// P3-D (resolved): an IPv4-mapped IPv6 loopback (`[::ffff:127.0.0.1]`) is
/// normalized to its IPv4 form before the check, so it is correctly treated as
/// loopback (matching operator intent) rather than as a routable bind.
pub fn is_loopback_addr(addr: &SocketAddr) -> bool {
    let ip = match addr.ip() {
        std::net::IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(std::net::IpAddr::V4)
            .unwrap_or(std::net::IpAddr::V6(v6)),
        other => other,
    };
    ip.is_loopback()
}

/// Bind a [`tokio::net::TcpListener`] on `addr`.
///
/// Mirrors the socket setup in [`crate::sidecar::server::spawn_sidecar`] except
/// for the platform-specific address-reuse policy: create a `socket2::Socket`,
/// set non-blocking, bind, listen with backlog 1024, then hand the std socket
/// to tokio. `socket2` is kept over `std::net::TcpListener::bind` — which
/// already implements exactly this platform reuse policy — for ONE reason: an
/// explicit 1024 backlog, which std does not expose.
///
/// On LINUX ONLY, `SO_REUSEADDR` is set so a restart on a FIXED serve port is
/// not blocked by a connection the previous process closed first. Measured,
/// that covers `FIN_WAIT_2` (a peer that never closed) as well as `TIME_WAIT`.
/// The flag is NOT self-sufficient: the bind only succeeds when the INCUMBENT
/// socket set it too, so the FIRST restart after upgrading from a build without
/// this flag can still fail `EADDRINUSE`, and only self-heals from the next
/// restart on.
///
/// Linux still refuses a second LIVE listener on the same address — sharing a
/// live port is `SO_REUSEPORT`'s job, not this flag's — so the explicit
/// `--listen` loud-failure contract (FR-002/003) is intact there. Verified for
/// both an identical `127.0.0.1:P` and a `0.0.0.0:P` over a live `127.0.0.1:P`.
///
/// The gate is `target_os = "linux"` and NOT `unix` because macOS/BSD does NOT
/// hold that second guarantee. Measured on `macos-latest` (CI job 89996269731):
/// with the flag set, `bind_listener("0.0.0.0:P")` SUCCEEDS over a live
/// `127.0.0.1:P`, so `explicit_wildcard_over_live_specific_still_fails_loudly`
/// failed there. Classic BSD `SO_REUSEADDR` permits a wildcard/specific overlap
/// even against a `TCP_LISTEN` incumbent. That is the same silent
/// "binds, looks healthy, accepts nothing" shape as the Windows bug below, and
/// it reaches the DEFAULT path — `bind_listener` is also the occupancy probe for
/// [`probe_free_listener`], whose fallback scans common loopback ports.
///
/// Accepted consequence: macOS/BSD keeps the fixed-port restart limitation the
/// flag would have fixed (`EADDRINUSE` until the remnant drains). That is a
/// known gap, not an oversight; it matches `main`'s behavior on every platform
/// before this change.
///
/// On Windows, `SO_REUSEADDR` is deliberately NOT set: there it lets two
/// listening sockets share one address when both sides opt in. That was a real,
/// shipped bug — a second `symforge serve --listen <occupied>` bound
/// successfully, printed a healthy attach URL, and then accepted ZERO
/// connections, with every request still going to the first server and no error
/// anywhere to explain it.
pub fn bind_listener(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let domain = if addr.is_ipv4() {
        socket2::Domain::IPV4
    } else {
        socket2::Domain::IPV6
    };
    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
    #[cfg(target_os = "linux")]
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    let std_listener: std::net::TcpListener = socket.into();
    tokio::net::TcpListener::from_std(std_listener)
}

/// Operator-friendly listen ports, in selection order. Corporate networks
/// routinely permit the 8000/5000 ranges but block ephemeral high ports, so a
/// no-explicit-port serve/admin start MUST resolve here and never drift to an
/// OS-assigned `:0` port (the 61850 problem). A short curated spread comes
/// first (memorable, non-adjacent), then the full 8000-8999 and 5000-5999
/// ranges guarantee a free port if one exists.
/// ponytail: ~2000 candidates; widen the ranges only if both ever fill.
pub(crate) fn operator_port_candidates() -> impl Iterator<Item = u16> {
    const SPREAD: [u16; 6] = [8080, 8088, 8181, 8585, 8686, 8989];
    SPREAD
        .into_iter()
        .chain(8000u16..=8999)
        .chain(5000u16..=5999)
}

/// Bind a verified-free listener, preferring `preferred` (US1, D1).
///
/// The race-free free-port primitive: when `preferred` is `Some` and non-zero,
/// attempt to bind it via [`bind_listener`]; on success return that live
/// listener. On a bind failure (the port is occupied), or when `preferred` is
/// `None`, bind the first free [`operator_port_candidates`] port (8000-8999 then
/// 5000-5999) — never an OS-assigned ephemeral port, which corporate networks
/// routinely block (the 61850 problem). An explicit `:0` `preferred` is honored
/// verbatim (callers/tests that truly want an OS port). Each candidate bind is
/// atomic, so the returned listener is guaranteed-free with no check-then-bind
/// TOCTOU gap; the caller serves directly on it, so reported URL == bound URL
/// (FR-020).
///
/// This is the production path: it never drops and rebinds, so there is no
/// window for another process to steal the chosen port. The thin
/// [`probe_free_port`] wrapper exists for the decision-logic unit tests and
/// callers that only need the resolved address.
pub fn probe_free_listener(
    preferred: Option<SocketAddr>,
) -> std::io::Result<tokio::net::TcpListener> {
    if let Some(addr) = preferred {
        // Explicit ephemeral (`:0`) is honored verbatim.
        if addr.port() == 0 {
            return bind_listener(addr);
        }
        if let Ok(listener) = bind_listener(addr) {
            return Ok(listener);
        }
    }
    // No preference, or the preferred port was occupied: bind the first free
    // operator port in the corporate-friendly ranges. NEVER an OS ephemeral port.
    for port in operator_port_candidates() {
        let addr = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port);
        if let Ok(listener) = bind_listener(addr) {
            return Ok(listener);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "no free operator port available in 8000-8999 or 5000-5999",
    ))
}

/// Resolve a verified-free [`SocketAddr`], preferring `preferred` (US1, D1).
///
/// Wraps [`probe_free_listener`]: tries `preferred` via a real bind, else falls
/// back to the first free operator-range port (never an OS ephemeral port —
/// see [`operator_port_candidates`]), and returns the listener's
/// `local_addr()`. The probe listener is dropped before returning, leaving a
/// small rebind window — a second process could, in principle, grab the freed
/// port between this call and the caller's rebind. Production serve uses
/// [`probe_free_listener`] (which threads the live listener through, eliminating
/// the window); this address-returning form is for callers that only need the
/// decision (e.g. a "suggested free port" in the setup wizard) and the unit
/// tests asserting the selection logic without holding a listener.
///
/// Signature per `contracts/free-port.md` / T005.
pub fn probe_free_port(preferred: Option<SocketAddr>) -> std::io::Result<SocketAddr> {
    let listener = probe_free_listener(preferred)?;
    listener.local_addr()
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
/// Resolves the project root (`discovery::find_project_root`) and loads it —
/// snapshot-first, exactly like the stdio local path (spec 026): a fresh
/// `.symforge/index.bin` rehydrates in well under a second (no re-parsing),
/// with the SAME staleness gate `persist::load_snapshot` applies everywhere
/// and a background verification pass reconciling against current disk state.
/// Only when no usable snapshot exists does serve pay the synchronous cold
/// `LiveIndex::load` it always paid. When no safe root is found, serves over
/// an empty index — `tools/list` still responds, and the operator can
/// `index_folder` after attaching.
///
/// The fourth tuple element carries the snapshot's mtime map when the fast
/// path was taken; the caller spawns `persist::background_verify` with it
/// (this function is sync — it cannot spawn).
type LoadedServeIndex = (
    SharedIndex,
    Option<std::path::PathBuf>,
    Option<StatePlacement>,
    Option<std::collections::HashMap<String, u64>>,
);

fn load_serve_index(
    workspace_root: Option<&std::path::Path>,
) -> Result<LoadedServeIndex, ServeError> {
    // An explicit root answers the same question CWD discovery does — "which
    // tree does this launch serve?" — so it is resolved through the SAME guard
    // and binding path, never trusted blindly.
    let discovered = match workspace_root {
        Some(root) => Some(root.to_path_buf()),
        None => crate::discovery::find_project_root(),
    };
    match discovered {
        Some(root) => {
            let RootResolution::Bound(binding) = crate::discovery::resolve_root_candidate(
                &root,
                RootCandidateSource::LaunchCwd,
                RootRequestMode::Automatic,
            ) else {
                return Ok((LiveIndex::empty(), None, None, None));
            };
            let state_placement = crate::discovery::resolve_state_placement(&binding);
            let root = binding.canonical_root;

            // Fast path: rehydrate the persisted snapshot (staleness-gated
            // inside `load_snapshot`) instead of re-parsing the tree.
            if let Some(snapshot) =
                crate::live_index::persist::load_snapshot(&root, &state_placement)
            {
                let file_count = snapshot.files.len();
                let snapshot_mtimes: std::collections::HashMap<String, u64> = snapshot
                    .files
                    .iter()
                    .map(|(path, file)| (path.clone(), file.mtime_secs))
                    .collect();
                let (live, code_signals) =
                    crate::live_index::persist::snapshot_to_live_index_with_code_signals(
                        snapshot, &root,
                    );
                tracing::info!(
                    files = file_count,
                    load_source = ?live.load_source(),
                    snapshot_verify_state = ?live.snapshot_verify_state(),
                    "serve: loaded serialized index from .symforge/index.bin"
                );
                let shared = crate::live_index::SharedIndexHandle::shared_for_state_placement_with_code_signals(
                    live,
                    &root,
                    &state_placement,
                    code_signals,
                );
                return Ok((
                    shared,
                    Some(root),
                    Some(state_placement),
                    Some(snapshot_mtimes),
                ));
            }

            let index = LiveIndex::load_for_state_placement(&root, &state_placement)
                .map_err(|source| ServeError::IndexLoad { source })?;
            Ok((index, Some(root), Some(state_placement), None))
        }
        None => Ok((LiveIndex::empty(), None, None, None)),
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
    state_placement: Option<StatePlacement>,
    auth: AuthConfig,
) -> ServerRuntime {
    let project_name = repo_root
        .as_ref()
        .and_then(|root| root.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string();

    // US3: open the durable economics ledger under the resolved project state
    // owner FIRST,
    // so the opened handle can be shared with both the dispatcher and the
    // runtime. A failure here degrades to Disabled inside `StelLedgerStore::open`,
    // so the server still starts (FR-011).
    // A dir/open failure degrades to `Disabled` inside `open` (logged, never
    // panics), so the server still starts (FR-011).
    let project_state_dir = state_placement
        .as_ref()
        .and_then(|placement| placement.directory());
    // Sub-phase timings. `serve: runtime built in` measures 2.8s on a
    // genuinely-cold index but only ~22ms when the index came from a snapshot
    // restore — a ~126x gap for what should be identical work. Two plausible
    // causes were measured and REFUTED: it is not a concurrent snapshot write
    // (the cold path spawns no background_verify at all), and it is not
    // first-time sqlite creation (a second cold start with the databases
    // already present still took 2.798s). Schema generation cannot explain it
    // either, being byte-identical on both paths. Name each sub-phase so the
    // next measurement points at the culprit instead of guessing again.
    let phase = std::time::Instant::now();
    let ledger_store: Option<Arc<StelLedgerStore>> = project_state_dir.map(|state_dir| {
        Arc::new(StelLedgerStore::open(
            state_dir,
            format!("serve-{}", std::process::id()),
        ))
    });
    tracing::info!("serve: runtime/ledger store in {:?}", phase.elapsed());

    // 006 G-039: the hashed product API-key store shares the exact same typed
    // project state owner. It cannot reconstruct placement from the source root.
    let phase = std::time::Instant::now();
    let key_store: Option<Arc<ApiKeyStore>> =
        project_state_dir.map(|state_dir| Arc::new(ApiKeyStore::open(state_dir)));
    tracing::info!("serve: runtime/key store in {:?}", phase.elapsed());

    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let phase = std::time::Instant::now();
    let mut protocol = SymForgeServer::new_with_state_placement(
        Arc::clone(&index),
        project_name,
        watcher_info,
        repo_root.clone(),
        state_placement,
        None,
    );
    tracing::info!("serve: runtime/protocol server in {:?}", phase.elapsed());
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

    let phase = std::time::Instant::now();
    let mut runtime = ServerRuntime::build_runtime(index, protocol, governor, auth, runtime_store);
    if let Some(store) = key_store {
        runtime = runtime.with_key_store(store);
    }
    tracing::info!("serve: runtime/server runtime in {:?}", phase.elapsed());
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
    // not leave a half-open listener. Both startup phases log their span: the
    // gap between "LiveIndex loaded" and "listening" was measured at more than
    // the load itself on this repo, and an unattributed slow phase is exactly
    // what let the load-duration under-report go unnoticed.
    let phase = std::time::Instant::now();
    let (index, repo_root, state_placement, snapshot_mtimes) =
        load_serve_index(args.workspace_root.as_deref())?;
    tracing::info!("serve: index ready in {:?}", phase.elapsed());
    // Spec 026: a snapshot-restored index reconciles against current disk
    // state in the background (same verify pass the stdio path spawns); tools
    // serve immediately with the honest SnapshotRestore/Pending trust labels.
    if let (Some(mtimes), Some(root)) = (snapshot_mtimes, repo_root.clone()) {
        let bg_index = index.clone();
        // V11 callbacks census (C3b): carry the observer incarnation current
        // at spawn; a later watcher registration makes it stale and the lane
        // refuses its observations.
        let observer =
            crate::live_index::index_lifecycle::activation::project_source_authority(&root)
                .active_observer();
        tokio::spawn(async move {
            crate::live_index::persist::background_verify(bg_index, root, mtimes, observer).await;
        });
    }
    let control_state_dir: Option<ControlStateDir> =
        crate::paths::process_control_state_placement()
            .directory()
            .cloned();
    let phase = std::time::Instant::now();
    let runtime = build_serve_runtime(index, repo_root, state_placement, auth.clone());
    tracing::info!("serve: runtime built in {:?}", phase.elapsed());

    // US1 (FR-001/002/003): an explicit operator-chosen `--listen` is honored
    // exactly — an occupied port fails loudly (no substitution). The default
    // address (no `--listen`) prefers `DEFAULT_LISTEN` but, if occupied, falls
    // back to an OS-assigned free port via `probe_free_listener` (which threads
    // the live listener through — no rebind window, no dead second listener).
    let bind_result = if args.explicit_listen {
        bind_listener(addr)
    } else {
        probe_free_listener(Some(addr))
    }
    .and_then(|listener| {
        listener
            .local_addr()
            .map(|local_addr| (listener, local_addr))
    });

    // Report the outcome to a background starter, if one asked, BEFORE
    // propagating a bind error with `?`. Without this, a starter waiting on
    // `bound_addr_tx` for an occupied explicit `--listen` would see no send at
    // all and sit out its full deadline, then report a misleading "did not
    // report a bound address" timeout instead of the real "port occupied"
    // error. A closed receiver (no starter waiting) is not an error either way.
    let (listener, local_addr) = match bind_result {
        Ok(pair) => pair,
        Err(source) => {
            if let Some(tx) = args.bound_addr_tx.as_ref() {
                let _ = tx.try_send(Err(format!("failed to bind {addr}: {source}")));
            }
            return Err(ServeError::Bind { addr, source });
        }
    };
    if let Some(tx) = args.bound_addr_tx.as_ref() {
        let _ = tx.try_send(Ok(local_addr));
    }

    // Build the /mcp router plus the /admin + /api/v1 router (006), merge them,
    // and layer Bearer auth + Origin gating in front (secure-default rule on
    // AuthConfig/AuthLayerState; P1-B Origin on OriginLayerState). Bearer auth
    // skips read-only admin static assets so the GUI loads when a key is set
    // (P2-1); `/api/v1/*` and `/mcp` remain gated. Layer order: Bearer outermost,
    // then Origin, then the handler.
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
    //
    // When a sibling AAP checkout is detected (008 US3 / FR-006), the banner also
    // surfaces the operator `/admin` panel URL and the AAP embed path dependency
    // (the AAP-native integration route). Detection is read-only.
    if let Some(control_state_dir) = control_state_dir.as_ref() {
        let state_path = crate::cli::onboarding::state_path(control_state_dir);
        let mut sink = crate::cli::onboarding::StderrSink;
        let detection = crate::server::aap::AapDetection::resolve();
        let aap_banner = detection.detected.then(|| {
            let admin_url = format!(
                "http://{host}:{port}{path}",
                host = local_addr.ip(),
                port = local_addr.port(),
                path = crate::server::admin::ADMIN_PATH,
            );
            crate::cli::onboarding::AapBanner {
                admin_url,
                embed_path_dep: crate::server::aap::embed_cargo_snippet(),
            }
        });
        crate::cli::onboarding::maybe_show_banner_with_aap(
            &state_path,
            env!("CARGO_PKG_VERSION"),
            &attach_url,
            aap_banner.as_ref(),
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

    // P2-3: after the HTTP server drains, wait (bounded) for any durable ledger
    // writes scheduled via `spawn_blocking` just before shutdown to finish, so
    // the economics ledger does not lose events accepted at the very end. A
    // stuck DB cannot hang shutdown — the drain times out and logs the residual.
    runtime
        .protocol()
        .ledger_write_tracker()
        .drain(std::time::Duration::from_secs(5))
        .await;

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

    /// Spec 026: serve restores from a fresh `.symforge/index.bin` instead of
    /// re-parsing — the fast path returns a SnapshotRestore-labeled index plus
    /// the mtime map the caller hands to `background_verify`. Without a
    /// snapshot the cold path still loads (and returns no mtime map).
    #[test]
    fn load_serve_index_restores_snapshot_when_present() {
        let dir = tempfile::TempDir::new().expect("fixture root");
        std::fs::create_dir(dir.path().join(".git")).expect("plant .git");
        std::fs::write(dir.path().join("lib.rs"), "pub fn seeded() {}\n").expect("seed file");

        // Cold path first: no snapshot on disk yet.
        let (cold, root, placement, mtimes) =
            load_serve_index(Some(dir.path())).expect("cold load");
        assert!(mtimes.is_none(), "no snapshot => no verify map");
        let root = root.expect("root bound");
        let placement = placement.expect("state placement resolved");
        assert_eq!(cold.read().file_count(), 1);

        // Persist the checkpoint the daemon/stdio paths write.
        crate::live_index::persist::checkpoint_shared_index(&cold, &root, &placement)
            .expect("write snapshot");

        // Warm path: the snapshot must be restored, not re-parsed.
        let (warm, _, _, mtimes) = load_serve_index(Some(dir.path())).expect("warm load");
        assert_eq!(
            warm.read().load_source(),
            crate::live_index::store::IndexLoadSource::SnapshotRestore,
            "serve must rehydrate a fresh snapshot"
        );
        assert_eq!(warm.read().file_count(), 1);
        let mtimes = mtimes.expect("snapshot restore returns the verify map");
        assert!(mtimes.contains_key("lib.rs"), "mtime map keys: {mtimes:?}");
    }

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
            explicit_listen: true,
            api_key: Some("inline-secret".to_string()),
            api_key_env: None,
            bound_addr_tx: None,
            workspace_root: None,
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
        // P3-D: an IPv4-mapped IPv6 loopback is normalized and treated as loopback.
        assert!(is_loopback_addr(
            &"[::ffff:127.0.0.1]:8787"
                .parse()
                .expect("v4-mapped loopback parses")
        ));
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
            explicit_listen: true,
            api_key: None,
            api_key_env: None,
            bound_addr_tx: None,
            workspace_root: None,
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
            explicit_listen: true,
            api_key: Some("k".to_string()),
            api_key_env: None,
            bound_addr_tx: None,
            workspace_root: None,
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

    #[tokio::test]
    async fn probe_free_port_uses_preferred_when_free() {
        // Reserve an OS-assigned port, then free it, so we have a known-free
        // address to prefer. probe_free_port must return exactly that port.
        let scratch = bind_listener("127.0.0.1:0".parse().unwrap()).expect("scratch bind");
        let preferred = scratch.local_addr().expect("local_addr");
        drop(scratch);

        let chosen = probe_free_port(Some(preferred)).expect("probe a free preferred port");
        assert_eq!(
            chosen, preferred,
            "a free preferred port is honored exactly"
        );
    }

    /// Occupy a loopback port with an **exclusive** listener (plain `std` bind)
    /// — the honest reproduction of a real squatter (`wslrelay`/another
    /// service). What makes it exclusive is that it is actively LISTENING, not
    /// its reuse flag: on Unix `std::net::TcpListener::bind` sets
    /// `SO_REUSEADDR` itself, and a second bind of a live listening address is
    /// refused there regardless (sharing a live port is `SO_REUSEPORT`'s job);
    /// on Windows two sockets share a port only if BOTH set `SO_REUSEADDR`, and
    /// [`bind_listener`] does not set it there. Either way the probe must fall
    /// back rather than share.
    fn occupy_exclusive() -> (std::net::TcpListener, SocketAddr) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("exclusive occupy a loopback port");
        let addr = listener.local_addr().expect("local_addr");
        (listener, addr)
    }

    /// True when `port` is in the operator-friendly ranges (8000-8999 /
    /// 5000-5999) that `operator_port_candidates` scans — never an OS ephemeral
    /// high port that a corporate network would block (the 61850 problem).
    fn in_operator_range(port: u16) -> bool {
        (8000..=8999).contains(&port) || (5000..=5999).contains(&port)
    }

    #[tokio::test]
    async fn probe_free_port_falls_back_to_operator_range_when_preferred_occupied() {
        // Occupy a port exclusively, then prefer it: probe must fall back to a
        // DIFFERENT free loopback port IN THE OPERATOR RANGES (never the occupied
        // port, never an OS ephemeral high port).
        let (occupied_listener, occupied) = occupy_exclusive();

        let chosen = probe_free_port(Some(occupied)).expect("probe falls back to a free port");
        assert_ne!(
            chosen.port(),
            occupied.port(),
            "must not pick the occupied port"
        );
        assert!(chosen.ip().is_loopback(), "fallback stays on loopback");
        assert!(
            in_operator_range(chosen.port()),
            "fallback must be an operator-range port (8000-8999/5000-5999), not ephemeral: {}",
            chosen.port()
        );

        // The fallback address is genuinely bindable (verified-free).
        let rebind = bind_listener(chosen).expect("the chosen fallback port is free");
        drop(rebind);
        drop(occupied_listener);
    }

    #[tokio::test]
    async fn probe_free_listener_threads_a_live_listener() {
        // The production primitive returns a live, serving-capable listener with
        // no rebind window. Occupy the preferred port exclusively and confirm the
        // returned listener is on a different, concrete port.
        let (occupied_listener, occupied) = occupy_exclusive();

        let listener = probe_free_listener(Some(occupied)).expect("probe a live fallback listener");
        let local = listener.local_addr().expect("local_addr");
        assert_ne!(
            local.port(),
            occupied.port(),
            "fell back off the occupied port"
        );
        assert!(
            in_operator_range(local.port()),
            "fallback must be an operator-range port, not ephemeral: {}",
            local.port()
        );
        drop(listener);
        drop(occupied_listener);
    }

    #[tokio::test]
    async fn probe_free_port_none_preference_uses_operator_range() {
        // No preference: bind the first free OPERATOR-RANGE loopback port
        // (8000-8999/5000-5999), never an OS ephemeral high port that corporate
        // networks block (the 61850 problem).
        let chosen = probe_free_port(None).expect("probe with no preference");
        assert!(chosen.ip().is_loopback());
        assert!(
            in_operator_range(chosen.port()),
            "no-preference bind must be in the operator ranges, not ephemeral: {}",
            chosen.port()
        );
    }
}
