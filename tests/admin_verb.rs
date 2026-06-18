// 009 US3 (T021): the `symforge admin` verb — reuse-vs-start over fixtures.
//
// (a) A real loopback operator server running on the profile port → `run_admin`
//     REUSES it (no 2nd server; descriptor port == the running one; browser
//     recorded; SC-004).
// (b) No server + no profile → `run_admin` STARTS one on a verified-free port,
//     reports a reachable URL, and persists the bound port to the profile.
//
// Fixtures only (FR-018): every path is a TempDir; the browser is a
// NoopBrowserOpener; no real harness config is touched.
//
// NOTE: this binary binds real loopback listeners. On some Windows hosts a test
// *binary* that opens a listening socket triggers an elevation/firewall prompt
// (the same reason US2's server coverage lives in-lib). The same assertions are
// mirrored into `symforge::cli::admin::tests` so they always run; this binary is
// the integration-level proof on hosts that permit it.
#![cfg(feature = "server")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use symforge::cli::admin::{
    AdminCliArgs, operator_server_reachable, run_admin, start_operator_server,
};
use symforge::cli::browser::{BrowserOpenOutcome, NoopBrowserOpener};
use symforge::cli::operator_profile::{AuthPosture, OperatorSetupProfile};
use symforge::cli::setup::{InstallationType, SetupContext};

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[test]
fn admin_reuses_running_server_on_profile_port() {
    // Start a real loopback server, then write a profile pointing at its port.
    let preferred = loopback(0);
    let running = start_operator_server(Some(preferred), None, None, Duration::from_secs(15))
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

    let ctx = SetupContext {
        home: home.path().to_path_buf(),
        working_dir: project.path().to_path_buf(),
    };
    let browser = NoopBrowserOpener::default();

    let outcome = run_admin(&AdminCliArgs { no_open: false }, &ctx, &browser)
        .expect("admin should reuse the running server");

    // Reused the existing server: same port, no second listener started.
    assert!(
        outcome.reused_server,
        "must reuse, not start a second server"
    );
    assert_eq!(
        outcome.session.bound_addr.port(),
        running_port,
        "reused descriptor names the running server's port"
    );
    assert_eq!(
        outcome.session.dashboard_url,
        format!("http://127.0.0.1:{running_port}/admin")
    );
    // Browser was asked to open exactly the reused dashboard URL.
    assert_eq!(
        browser.opened_urls(),
        vec![outcome.session.dashboard_url.clone()]
    );
    assert_eq!(outcome.browser_outcome, BrowserOpenOutcome::Skipped);
}

#[test]
fn admin_starts_server_when_none_running_and_persists_port() {
    let project = tempfile::tempdir().expect("temp project");
    let home = tempfile::tempdir().expect("temp home");
    assert!(
        OperatorSetupProfile::load(project.path()).is_none(),
        "no profile to start"
    );

    let ctx = SetupContext {
        home: home.path().to_path_buf(),
        working_dir: project.path().to_path_buf(),
    };
    let browser = NoopBrowserOpener::default();

    let outcome = run_admin(&AdminCliArgs { no_open: false }, &ctx, &browser)
        .expect("admin should start a server when none runs");

    // Started fresh (not reused), reachable, and the URL is bound + answers.
    assert!(
        !outcome.reused_server,
        "no server was running; must start one"
    );
    assert!(outcome.session.reachable);
    assert!(outcome.session.bound_addr.ip().is_loopback());
    assert!(
        operator_server_reachable(outcome.session.bound_addr, Duration::from_millis(500)),
        "reported URL must be reachable (FR-020)"
    );
    assert_eq!(browser.opened_urls().len(), 1);

    // The bound port was persisted so the next admin run reuses it (FR-012/015).
    let profile = OperatorSetupProfile::load(project.path()).expect("port persisted");
    assert_eq!(profile.port, outcome.session.bound_addr.port());
}

#[test]
fn admin_no_open_skips_browser_but_still_reports_url() {
    let preferred = loopback(0);
    let running = start_operator_server(Some(preferred), None, None, Duration::from_secs(15))
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

    let ctx = SetupContext {
        home: home.path().to_path_buf(),
        working_dir: project.path().to_path_buf(),
    };
    let browser = NoopBrowserOpener::default();

    let outcome = run_admin(&AdminCliArgs { no_open: true }, &ctx, &browser)
        .expect("admin --no-open should report the URL without opening");

    assert!(outcome.reused_server);
    // --no-open: the browser opener was NOT invoked, the URL is still returned.
    assert!(
        browser.opened_urls().is_empty(),
        "no_open must not open a browser"
    );
    assert_eq!(outcome.browser_outcome, BrowserOpenOutcome::Skipped);
    assert!(outcome.session.dashboard_url.ends_with("/admin"));
}
