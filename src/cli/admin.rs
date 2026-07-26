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
/// **Approach.** `serve::run` blocks in `axum::serve(...).await` until shutdown,
/// so it never returns. This helper:
///
/// 1. Spawns a dedicated **OS thread** owning its own multi-thread tokio runtime,
///    running `serve::run` with a `bound_addr_tx` channel. The thread (and its
///    server) live for the lifetime of the process — this is a start-on-demand
///    helper, not a managed lifecycle (no shutdown handle is returned; stopping
///    is process exit, matching D3's "no OS service unit" scope).
/// 2. Waits for `serve::run` to REPORT the address it bound, then confirms
///    reachability before returning the descriptor (FR-020).
///
/// Selecting the port here — bind it, read the number, drop the listener, and
/// ask serve to re-bind it — was a real defect, not a theoretical window. The
/// gap was wide enough to lose in CI repeatedly, first on OS ephemeral ports
/// and then on `8080`, the first operator candidate that every concurrent
/// starter picks. Letting serve bind once and report back removes the gap
/// rather than narrowing it: there is no interval in which the port is chosen
/// but unowned.
///
/// `preferred` is a **preference**, not a demand: a remembered port to land back
/// on, or the default. When it is occupied, serve falls back to a free operator
/// port rather than failing — the caller wanted *a* server, not that exact port.
///
/// **Limit (documented):** there is no graceful-stop handle — the spawned server
/// runs until the process exits.
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
    // Step 1: let serve pick AND own the port — the caller never touches it, so
    // there is no interval in which it is chosen but unowned.
    //
    // `explicit_listen` stays FALSE here on purpose. It means "the operator
    // named this exact port, fail loudly if it is taken", and nobody on this
    // path did: `preferred` is a *preference* — a remembered port to land back
    // on, or the default — not a demand. Setting it true made an occupied
    // preference a hard failure, which only looked survivable while
    // `SO_REUSEADDR` was silently letting two servers share one address. False
    // routes through `probe_free_listener`, which honors the preference when
    // free and falls back to a genuinely free operator port when not.
    let requested = preferred.map_or_else(
        || crate::server::serve::DEFAULT_LISTEN.to_string(),
        |addr| addr.to_string(),
    );
    let (bound_tx, bound_rx) = std::sync::mpsc::sync_channel(1);
    let serve_args = ServeArgs {
        listen: requested,
        explicit_listen: false,
        api_key,
        api_key_env,
        bound_addr_tx: Some(bound_tx),
    };
    std::thread::Builder::new()
        .name("symforge-serve".to_string())
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

    // Step 2: wait to be TOLD the bound address, then confirm it serves. The
    // report arrives once the listener is live, so the remaining poll only
    // covers router/middleware setup, never a port that might never bind.
    // `serve::run` loads the whole workspace index BEFORE it binds, so the wait
    // for the address covers indexing as well as the bind — on a loaded machine
    // that is the slow part. Give it the full deadline, then give reachability
    // its own: sharing one budget would let a slow index consume the time the
    // router needs to come up, failing a server that was going to work.
    let bound_addr = bound_rx.recv_timeout(deadline).map_err(|_| {
        anyhow::anyhow!(
            "operator server did not report a bound address within {deadline:?} \
             (index load + bind)"
        )
    })?;

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

/// Suggest a currently-free loopback address using only `std` (no tokio
/// reactor), preferring `preferred`. Callable from a synchronous CLI context.
///
/// Attempts to bind `preferred` (when non-zero); on success returns that address,
/// on failure (the port is occupied) returns the first free
/// [`serve::operator_port_candidates`] port (8000-8999 then 5000-5999) — never an
/// OS-assigned ephemeral port, which corporate networks block (the 61850
/// problem). An explicit `:0` `preferred` means "any free port" and is resolved
/// from those same operator ranges.
///
/// **This SUGGESTS a port; it does not reserve one.** The probe listener is
/// dropped before returning, so the address can go stale — that is inherent to
/// suggesting a value for a config file, which is the setup wizard's use. Do NOT
/// use it to pick a port to then start a server on: that reintroduces the
/// drop-then-rebind gap [`start_operator_server`] was fixed to remove. A starter
/// should let `serve::run` bind and report its own address.
///
/// The probe uses a plain `std::net::TcpListener` (no `SO_REUSEADDR`), so an
/// occupied port is detected honestly rather than silently shared.
pub(crate) fn select_free_addr_std(preferred: Option<SocketAddr>) -> std::io::Result<SocketAddr> {
    if let Some(addr) = preferred {
        // `:0` asks for "any free port", answered from the operator ranges
        // rather than the OS ephemeral range — corporate networks routinely
        // block ephemeral high ports (the 61850 problem), and this value is
        // written into a config for later use. `addr` may carry a non-loopback
        // host, so honor its IP while picking the port.
        if addr.port() != 0 {
            if let Ok(listener) = std::net::TcpListener::bind(addr) {
                return listener.local_addr();
            }
        } else if let Some(resolved) = first_free_operator_addr(addr.ip()) {
            return Ok(resolved);
        }
    }
    // Preferred occupied / no preference: first free operator port in the
    // corporate-friendly ranges, never an OS ephemeral port.
    first_free_operator_addr(IpAddr::V4(Ipv4Addr::LOCALHOST)).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "no free operator port available in 8000-8999 or 5000-5999",
        )
    })
}

/// First bindable operator-range address on `host`, or `None` if the ranges are
/// exhausted. Restricted to [`serve::operator_port_candidates`] so the returned
/// port is one the OS does not hand out on its own — the property that makes it
/// survive this selector's drop-then-rebind gap.
fn first_free_operator_addr(host: IpAddr) -> Option<SocketAddr> {
    crate::server::serve::operator_port_candidates().find_map(|port| {
        let addr = SocketAddr::new(host, port);
        std::net::TcpListener::bind(addr)
            .ok()
            .and_then(|listener| listener.local_addr().ok())
    })
}

/// Flags for `symforge admin` (see `contracts/admin-cli.md`).
#[derive(Args, Debug, Clone)]
pub struct AdminCliArgs {
    /// Do not attempt to open a browser; print/return the dashboard URL only.
    #[arg(long)]
    pub no_open: bool,
}

/// The result of one `symforge admin` run, returned for caller messaging and
/// test assertions (mirrors US2's `WizardOutcome`: every effect observable
/// without scraping stderr).
///
/// Tests inspect this to assert the reuse-vs-start decision, the reported URL,
/// and the (no-op) browser open — deterministically, over fixtures, with no real
/// browser (FR-017/018).
#[derive(Debug)]
pub struct AdminOutcome {
    /// The running operator server (reused or started); always reachable
    /// (FR-020).
    pub session: ServerSessionDescriptor,
    /// `true` when an already-reachable server on the remembered port was reused
    /// instead of starting a second one (FR-015 / SC-004).
    pub reused_server: bool,
    /// The browser-open outcome for the dashboard URL.
    pub browser_outcome: crate::cli::browser::BrowserOpenOutcome,
}

/// How long to wait for a reachability probe of the remembered port before
/// deciding "nothing is serving there" and starting a fresh server.
const ADMIN_REACHABILITY_TIMEOUT: Duration = Duration::from_millis(500);

/// How long a fresh serve-start may take to become reachable before the admin
/// verb gives up. Generous on purpose: `serve::run` loads the workspace index on
/// startup, which on a large repo and/or a cold or heavily-loaded machine (e.g. a
/// CI runner) can legitimately exceed 15s — giving up early would be a false
/// "server failed to start" while it was merely still indexing. 60s is a ceiling
/// for pathological cases, not the expected wait (a warm start is seconds). Shared
/// by the setup wizard and the in-lib serve-start tests so there is one source of
/// truth for "how long a real serve may take to come up".
pub(crate) const ADMIN_SERVE_START_DEADLINE: Duration = Duration::from_secs(60);

/// Entry point for `symforge admin`. Wires the live home/cwd context and the OS
/// browser into [`run_admin`] and discards the outcome — the function already
/// printed the dashboard URL to the operator.
pub fn run(args: AdminCliArgs) -> anyhow::Result<()> {
    let ctx = crate::cli::setup::SetupContext::from_env()?;
    let browser = crate::cli::browser::OsBrowserOpener;
    let outcome = run_admin(&args, &ctx, &browser)?;
    if outcome.reused_server {
        eprintln!(
            "Operator dashboard already running — {}",
            outcome.session.dashboard_url
        );
    } else {
        eprintln!(
            "Started operator dashboard — {}",
            outcome.session.dashboard_url
        );
    }
    eprintln!("Attach: {}", outcome.session.attach_url);
    eprintln!(
        "Browser: {:?} — open {}",
        outcome.browser_outcome, outcome.session.dashboard_url
    );

    // D21: a freshly-started operator server runs on a background thread that
    // dies the moment this process exits — so `admin` must stay in the
    // foreground and keep serving until the operator stops it, or the URL we
    // just printed is dead the instant we return. A reused server is owned by
    // another process, so that path returns immediately (exit 0).
    serve_foreground_if_started(&outcome, block_until_ctrl_c)
}

/// Operator-facing notice printed on the fresh-start (non-reused) path right
/// before `symforge admin` blocks in the foreground. Without it a fresh start
/// reads as a frozen terminal (a dogfood defect). Never printed on the reuse
/// path, which returns immediately.
pub(crate) const FOREGROUND_SERVE_NOTICE: &str =
    "Serving the operator dashboard in the foreground — press Ctrl-C to stop.";

/// Keep `symforge admin` in the foreground when it STARTED the operator server,
/// so the printed dashboard URL actually keeps serving (D21). A reused server is
/// owned by another process, so this returns immediately (exit 0); only a server
/// we started ourselves must be kept alive here, blocking in `wait_for_shutdown`
/// until the operator terminates the process (Ctrl-C in production).
///
/// Split from [`run`] so the reuse-vs-block decision is unit-testable with an
/// injected waiter (the real waiter blocks until Ctrl-C, which a test cannot).
fn serve_foreground_if_started(
    outcome: &AdminOutcome,
    wait_for_shutdown: impl FnOnce(&ServerSessionDescriptor) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if outcome.reused_server {
        return Ok(());
    }
    eprintln!("{FOREGROUND_SERVE_NOTICE}");
    wait_for_shutdown(&outcome.session)
}

/// Block the calling thread until the operator asks the process to stop (Ctrl-C
/// on all platforms). Mirrors [`operator_server_reachable`]'s private
/// current-thread runtime so it is callable from the plain synchronous CLI
/// context without an ambient reactor — the operator server runs on its own
/// thread + runtime, so this is a sibling runtime, never a nested one.
///
/// ponytail: on Ctrl-C the process exits promptly and the background serve
/// thread is torn down best-effort — the operator dashboard is read-only (no
/// writes to drain), so an abrupt stop is safe. If graceful drain ever matters,
/// thread the serve thread's `JoinHandle` out of `start_operator_server` and
/// join it here instead.
fn block_until_ctrl_c(_session: &ServerSessionDescriptor) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(tokio::signal::ctrl_c())?;
    Ok(())
}

/// Testable admin-verb core: reuse a running operator server (reachable on the
/// remembered port) or start one on a verified-free port, then open + return the
/// dashboard URL (FR-015, contracts/admin-cli.md, SC-004).
///
/// Mirrors US2's `run_wizard` seam shape: tests call this directly with a
/// TempDir-backed [`crate::cli::setup::SetupContext`] and a
/// [`crate::cli::browser::NoopBrowserOpener`], then assert on the returned
/// [`AdminOutcome`] — no real browser, and (apart from a deliberate loopback
/// bind on the start path) no network beyond the reachability probe (FR-018).
///
/// Flow:
/// 1. Load [`crate::cli::operator_profile::OperatorSetupProfile`] for the project
///    base -> the remembered port.
/// 2. If a port is remembered and [`operator_server_reachable`] confirms a server
///    is up on the loopback address, **reuse it**: build the descriptor for that
///    address and start nothing (SC-004 — never a second server).
/// 3. Otherwise [`start_operator_server`] on a verified-free loopback port (no key
///    this slice), then persist the bound port back to the profile so the next run
///    reuses it.
/// 4. Open the dashboard URL via `browser` (a no-op opener in tests) and return.
pub fn run_admin<B: crate::cli::browser::BrowserOpener + ?Sized>(
    args: &AdminCliArgs,
    ctx: &crate::cli::setup::SetupContext,
    browser: &B,
) -> anyhow::Result<AdminOutcome> {
    let placement = crate::paths::process_control_state_placement();
    run_admin_with_control_state(args, ctx, browser, placement.directory())
}

#[doc(hidden)]
pub fn run_admin_with_control_state<B: crate::cli::browser::BrowserOpener + ?Sized>(
    args: &AdminCliArgs,
    ctx: &crate::cli::setup::SetupContext,
    browser: &B,
    control_state_dir: Option<&crate::domain::ControlStateDir>,
) -> anyhow::Result<AdminOutcome> {
    use crate::cli::operator_profile::OperatorSetupProfile;

    let _ = ctx;
    let existing_profile = control_state_dir.and_then(OperatorSetupProfile::load);

    // Step 1+2: reuse an already-running server on the remembered port (FR-015).
    let mut reused_server = false;
    let mut session: Option<ServerSessionDescriptor> = None;
    if let Some(profile) = existing_profile.as_ref() {
        let addr = loopback_addr_std(profile.port);
        if operator_server_reachable(addr, ADMIN_REACHABILITY_TIMEOUT) {
            session = Some(ServerSessionDescriptor::for_addr(addr, true));
            reused_server = true;
        }
    }

    // Step 3: nothing reachable -> start a fresh server on a verified-free
    // loopback port, then remember it. Prefer the remembered port (so a restart
    // lands back on the same bookmarkable port when it is free); else the
    // historical default, else an OS-assigned ephemeral port — all via
    // `start_operator_server`'s own free-address selection.
    let session = match session {
        Some(s) => s,
        None => {
            let preferred = preferred_start_addr(existing_profile.as_ref());
            let started =
                start_operator_server(Some(preferred), None, None, ADMIN_SERVE_START_DEADLINE)?;
            persist_started_port(control_state_dir, existing_profile.as_ref(), &started);
            started
        }
    };

    // Step 4: open the dashboard (a no-op opener in tests), unless `--no-open`.
    let browser_outcome = if args.no_open {
        crate::cli::browser::BrowserOpenOutcome::Skipped
    } else {
        browser.open_url(&session.dashboard_url)
    };

    Ok(AdminOutcome {
        session,
        reused_server,
        browser_outcome,
    })
}

/// Loopback `SocketAddr` for `port`, built with only `std` (no tokio reactor) so
/// it is callable from the plain synchronous admin-verb context.
fn loopback_addr_std(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

/// The preferred start address for a fresh admin serve-start: the remembered
/// port if any (so a restart reuses the bookmarkable port when free), else the
/// historical default `8787`, else (if even that fails to parse) an ephemeral
/// `:0`. [`start_operator_server`] verifies the address is actually free and
/// falls back to an OS-assigned port when it is occupied.
fn preferred_start_addr(
    existing_profile: Option<&crate::cli::operator_profile::OperatorSetupProfile>,
) -> SocketAddr {
    if let Some(profile) = existing_profile {
        return loopback_addr_std(profile.port);
    }
    crate::server::serve::DEFAULT_LISTEN
        .parse()
        .unwrap_or_else(|_| loopback_addr_std(0))
}

/// Persist the just-bound port back to the operator profile so the next admin /
/// setup run reuses this server (FR-012/015). Preserves the prior profile's
/// installation type / harness list when one exists; on a first-ever admin start
/// (no profile yet) records a minimal server-mode profile. A persist failure is
/// non-fatal — the server is already up and reported — so it is logged as a
/// warning, never an error that masks a running dashboard.
fn persist_started_port(
    control_state_dir: Option<&crate::domain::ControlStateDir>,
    existing_profile: Option<&crate::cli::operator_profile::OperatorSetupProfile>,
    started: &ServerSessionDescriptor,
) {
    use crate::cli::operator_profile::{AuthPosture, OperatorSetupProfile};

    let port = started.bound_addr.port();
    let updated_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let profile = match existing_profile {
        Some(prior) => OperatorSetupProfile {
            port,
            updated_ms,
            ..prior.clone()
        },
        None => OperatorSetupProfile {
            installation_type: crate::cli::setup::InstallationType::Server,
            port,
            auth_posture: AuthPosture::LoopbackNoKey,
            harnesses: Vec::new(),
            updated_ms,
        },
    };

    let Some(control_state_dir) = control_state_dir else {
        tracing::warn!("admin: process control state unavailable; started port not persisted");
        return;
    };
    if let Err(error) = profile.save(control_state_dir) {
        tracing::warn!(%error, "admin: could not persist the started operator-server port");
    }
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

    /// The suggested port is written into an operator config and used later, so
    /// it must come from the ranges corporate networks permit — never an OS
    /// ephemeral high port (the 61850 problem).
    #[test]
    fn select_free_addr_std_never_returns_an_ephemeral_port() {
        fn in_operator_range(port: u16) -> bool {
            (8000..=8999).contains(&port) || (5000..=5999).contains(&port)
        }

        let explicit_any = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        for preferred in [None, Some(explicit_any)] {
            let addr = select_free_addr_std(preferred).expect("an operator port is free");
            assert!(
                in_operator_range(addr.port()),
                "selected port must be operator-range, not ephemeral: {addr}"
            );
        }
    }

    /// Two starters racing with no preference must both come up. Pre-selecting
    /// the port here (bind, read, drop, ask serve to re-bind) made this fail:
    /// both picked `8080`, the first operator candidate, and one lost the gap.
    /// Serve binding once and reporting back leaves no gap to lose.
    #[test]
    fn concurrent_starts_with_no_preference_both_come_up() {
        let first = start_operator_server(None, None, None, ADMIN_SERVE_START_DEADLINE)
            .expect("first operator server should come up");
        let second = start_operator_server(None, None, None, ADMIN_SERVE_START_DEADLINE)
            .expect("second operator server should come up");
        assert_ne!(
            first.bound_addr, second.bound_addr,
            "each start must own a distinct port"
        );
    }

    /// A `preferred` address is a PREFERENCE (a remembered port to land back on,
    /// or the default) — never an operator demand. An occupied one must fall
    /// back to a free port, not fail the start. This only looked fine while
    /// `SO_REUSEADDR` let two servers silently share one address.
    #[test]
    fn occupied_preference_falls_back_instead_of_failing() {
        let first = start_operator_server(None, None, None, ADMIN_SERVE_START_DEADLINE)
            .expect("first operator server should come up");
        let second = start_operator_server(
            Some(first.bound_addr),
            None,
            None,
            ADMIN_SERVE_START_DEADLINE,
        )
        .expect("an occupied preference must fall back, not fail");
        assert_ne!(
            first.bound_addr, second.bound_addr,
            "the second start must land on a different, genuinely free port"
        );
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
        let desc = start_operator_server(Some(preferred), None, None, ADMIN_SERVE_START_DEADLINE)
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

    // --- T021/T023 run_admin reuse-vs-start (mirrored from tests/admin_verb.rs
    // so the coverage runs in-lib regardless of the Windows test-binary elevation
    // prompt that blocks server-binding integration *binaries*) ----------------

    use crate::cli::browser::{BrowserOpenOutcome, NoopBrowserOpener};
    use crate::cli::operator_profile::{AuthPosture, OperatorSetupProfile};
    use crate::cli::setup::{InstallationType, SetupContext};

    fn ctx_over(home: &std::path::Path, project: &std::path::Path) -> SetupContext {
        SetupContext {
            home: home.to_path_buf(),
            working_dir: project.to_path_buf(),
        }
    }

    fn control_over(home: &std::path::Path) -> crate::domain::ControlStateDir {
        crate::domain::ControlStateDir::new(home.join("control"))
    }

    #[test]
    fn run_admin_reuses_running_server_on_profile_port() {
        // A real server is running; the profile points at its port.
        let preferred = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let running =
            start_operator_server(Some(preferred), None, None, ADMIN_SERVE_START_DEADLINE)
                .expect("operator server should come up");
        let running_port = running.bound_addr.port();

        let project = tempfile::tempdir().expect("temp project");
        let home = tempfile::tempdir().expect("temp home");
        let control = control_over(home.path());
        OperatorSetupProfile::new(
            InstallationType::Server,
            running_port,
            AuthPosture::LoopbackNoKey,
            &[],
            1,
        )
        .save(&control)
        .expect("persist profile");

        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();
        let outcome = run_admin_with_control_state(
            &AdminCliArgs { no_open: false },
            &ctx,
            &browser,
            Some(&control),
        )
        .expect("admin should reuse the running server");

        assert!(
            outcome.reused_server,
            "must reuse, not start a second server"
        );
        assert_eq!(
            outcome.session.bound_addr.port(),
            running_port,
            "reused descriptor names the running server's port (SC-004)"
        );
        assert_eq!(browser.opened_urls().len(), 1);
        assert_eq!(
            browser.opened_urls()[0],
            outcome.session.dashboard_url,
            "the reused dashboard URL is the one opened"
        );
    }

    #[test]
    fn run_admin_starts_and_persists_when_none_running() {
        let project = tempfile::tempdir().expect("temp project");
        let home = tempfile::tempdir().expect("temp home");
        let control = control_over(home.path());
        assert!(OperatorSetupProfile::load(&control).is_none());

        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();
        let outcome = run_admin_with_control_state(
            &AdminCliArgs { no_open: false },
            &ctx,
            &browser,
            Some(&control),
        )
        .expect("admin should start a server when none runs");

        assert!(!outcome.reused_server, "no server ran; must start one");
        assert!(outcome.session.reachable);
        assert!(
            operator_server_reachable(outcome.session.bound_addr, Duration::from_millis(500)),
            "the reported URL must actually be reachable (FR-020)"
        );
        assert_eq!(browser.opened_urls().len(), 1);

        // The bound port is persisted so the next run reuses it (FR-012/015).
        let profile = OperatorSetupProfile::load(&control).expect("port persisted");
        assert_eq!(profile.port, outcome.session.bound_addr.port());
    }

    #[test]
    fn run_admin_no_open_does_not_open_browser() {
        let preferred = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let running =
            start_operator_server(Some(preferred), None, None, ADMIN_SERVE_START_DEADLINE)
                .expect("operator server should come up");

        let project = tempfile::tempdir().expect("temp project");
        let home = tempfile::tempdir().expect("temp home");
        let control = control_over(home.path());
        OperatorSetupProfile::new(
            InstallationType::Server,
            running.bound_addr.port(),
            AuthPosture::LoopbackNoKey,
            &[],
            1,
        )
        .save(&control)
        .expect("persist profile");

        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();
        let outcome = run_admin_with_control_state(
            &AdminCliArgs { no_open: true },
            &ctx,
            &browser,
            Some(&control),
        )
        .expect("admin --no-open should report without opening");

        assert!(
            browser.opened_urls().is_empty(),
            "no_open suppresses the open"
        );
        assert_eq!(outcome.browser_outcome, BrowserOpenOutcome::Skipped);
        assert!(outcome.session.dashboard_url.ends_with("/admin"));
    }

    // --- D21: `admin` must keep serving after a fresh start (foreground) --------

    #[test]
    fn admin_keeps_serving_after_fresh_start_until_shutdown() {
        // No server running -> run_admin starts one on a background thread.
        // serve_foreground_if_started must then KEEP admin in the foreground (invoke
        // the waiter), and while "blocked" the freshly-started server must be
        // reachable AND STAY reachable. D21: pre-fix `run` returned immediately, so
        // the serve thread died with the process and the printed URL was refused.
        let project = tempfile::tempdir().expect("temp project");
        let home = tempfile::tempdir().expect("temp home");
        let control = control_over(home.path());
        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();

        let outcome = run_admin_with_control_state(
            &AdminCliArgs { no_open: true },
            &ctx,
            &browser,
            Some(&control),
        )
        .expect("admin should start a server when none runs");
        assert!(!outcome.reused_server, "no server ran; must start one");

        let waited = std::cell::Cell::new(false);
        serve_foreground_if_started(&outcome, |session| {
            waited.set(true);
            // Probe twice with a gap: the started server must be serving now AND
            // still serving a moment later — not fire-and-return (D21).
            assert!(
                operator_server_reachable(session.bound_addr, Duration::from_millis(500)),
                "freshly-started server must be reachable while admin holds the foreground"
            );
            std::thread::sleep(Duration::from_millis(300));
            assert!(
                operator_server_reachable(session.bound_addr, Duration::from_millis(500)),
                "the started server must STAY reachable (D21: it must not die after start)"
            );
            Ok(())
        })
        .expect("foreground wait returns cleanly on shutdown");

        assert!(
            waited.get(),
            "a freshly-started server must hold the foreground (D21 fix); pre-fix `run` returned immediately"
        );

        // Fix 1: the fresh branch prints `FOREGROUND_SERVE_NOTICE` unconditionally
        // right before the waiter runs, so a fresh start is not a silent block.
        // stderr capture is impractical in-lib, so pin the notice wording here; the
        // `waited.get()` assertion above proves the fresh branch (which emits it)
        // executed, and `admin_does_not_hold_foreground_when_reusing` proves the
        // reuse branch returns before reaching the notice.
        assert!(
            FOREGROUND_SERVE_NOTICE.contains("Ctrl-C"),
            "the foreground notice must tell the operator how to stop it"
        );
    }

    #[test]
    fn admin_does_not_hold_foreground_when_reusing() {
        // A server already runs and the profile points at it -> reuse. The reuse
        // path must NOT block: another process owns the server, so admin exits 0
        // after opening the dashboard (required behavior #1).
        let preferred = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let running =
            start_operator_server(Some(preferred), None, None, ADMIN_SERVE_START_DEADLINE)
                .expect("operator server should come up");

        let project = tempfile::tempdir().expect("temp project");
        let home = tempfile::tempdir().expect("temp home");
        let control = control_over(home.path());
        OperatorSetupProfile::new(
            InstallationType::Server,
            running.bound_addr.port(),
            AuthPosture::LoopbackNoKey,
            &[],
            1,
        )
        .save(&control)
        .expect("persist profile");

        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();
        let outcome = run_admin_with_control_state(
            &AdminCliArgs { no_open: true },
            &ctx,
            &browser,
            Some(&control),
        )
        .expect("admin should reuse the running server");
        assert!(outcome.reused_server, "must reuse the running server");

        let waited = std::cell::Cell::new(false);
        serve_foreground_if_started(&outcome, |_session| {
            waited.set(true);
            Ok(())
        })
        .expect("reuse path returns cleanly");
        assert!(
            !waited.get(),
            "reuse path must NOT hold the foreground — another process owns the server"
        );
    }
}
