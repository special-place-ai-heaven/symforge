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
/// Attempts to bind `preferred` (when non-zero); on success returns that address,
/// on failure (the port is occupied) binds the first free
/// [`serve::operator_port_candidates`] port (8000-8999 then 5000-5999) — never an
/// OS-assigned ephemeral port, which corporate networks block (the 61850
/// problem). An explicit `:0` `preferred` is honored verbatim. The probe listener
/// is dropped before returning, so the same documented small rebind window as
/// `serve::probe_free_port` applies — closed in practice by the `SO_REUSEADDR`
/// rebind inside `serve::run` plus the step-3 reachability gate. Pub(crate) for
/// setup wizard reuse.
pub(crate) fn select_free_addr_std(preferred: Option<SocketAddr>) -> std::io::Result<SocketAddr> {
    if let Some(addr) = preferred {
        // `:0` requested explicitly → resolve the concrete OS-assigned port.
        if addr.port() == 0 {
            return std::net::TcpListener::bind(addr)?.local_addr();
        }
        if let Ok(listener) = std::net::TcpListener::bind(addr) {
            return listener.local_addr();
        }
    }
    // Preferred occupied / no preference: first free operator port in the
    // corporate-friendly ranges, never an OS ephemeral port.
    for port in crate::server::serve::operator_port_candidates() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        if let Ok(listener) = std::net::TcpListener::bind(addr) {
            return listener.local_addr();
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "no free operator port available in 8000-8999 or 5000-5999",
    ))
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
    use crate::cli::operator_profile::OperatorSetupProfile;

    let project_base = ctx.project_base();
    let existing_profile = OperatorSetupProfile::load(&project_base);

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
            persist_started_port(&project_base, existing_profile.as_ref(), &started);
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
    project_base: &std::path::Path,
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

    if let Err(error) = profile.save(project_base) {
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
        OperatorSetupProfile::new(
            InstallationType::Server,
            running_port,
            AuthPosture::LoopbackNoKey,
            &[],
            1,
        )
        .save(project.path())
        .expect("persist profile");

        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();
        let outcome = run_admin(&AdminCliArgs { no_open: false }, &ctx, &browser)
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
        assert!(OperatorSetupProfile::load(project.path()).is_none());

        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();
        let outcome = run_admin(&AdminCliArgs { no_open: false }, &ctx, &browser)
            .expect("admin should start a server when none runs");

        assert!(!outcome.reused_server, "no server ran; must start one");
        assert!(outcome.session.reachable);
        assert!(
            operator_server_reachable(outcome.session.bound_addr, Duration::from_millis(500)),
            "the reported URL must actually be reachable (FR-020)"
        );
        assert_eq!(browser.opened_urls().len(), 1);

        // The bound port is persisted so the next run reuses it (FR-012/015).
        let profile = OperatorSetupProfile::load(project.path()).expect("port persisted");
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
        OperatorSetupProfile::new(
            InstallationType::Server,
            running.bound_addr.port(),
            AuthPosture::LoopbackNoKey,
            &[],
            1,
        )
        .save(project.path())
        .expect("persist profile");

        let ctx = ctx_over(home.path(), project.path());
        let browser = NoopBrowserOpener::default();
        let outcome = run_admin(&AdminCliArgs { no_open: true }, &ctx, &browser)
            .expect("admin --no-open should report without opening");

        assert!(
            browser.opened_urls().is_empty(),
            "no_open suppresses the open"
        );
        assert_eq!(outcome.browser_outcome, BrowserOpenOutcome::Skipped);
        assert!(outcome.session.dashboard_url.ends_with("/admin"));
    }
}
