//! 009 US1 / SC-003 — collision-free serve port (the real bug).
//!
//! ## What is proven
//!
//! The pre-009 serve path bound the fixed `DEFAULT_LISTEN` (`127.0.0.1:8787`)
//! directly and FAILED when that port was occupied — leaving an operator on a
//! dead "not found" dashboard. US1 replaces the no-explicit-address bind with the
//! race-free `probe_free_listener` / `probe_free_port` pattern: prefer the
//! requested port, else bind `127.0.0.1:0` (OS-assigned, atomic) so the chosen
//! port is always verified-free, and the reported URL == the bound, *reachable*
//! URL (FR-001/020).
//!
//! ## Coverage map (honest)
//!
//! * `default_occupied_falls_back_to_a_reachable_port` — the regression: a
//!   preferred port is OCCUPIED, the probe selects a DIFFERENT port, a real HTTP
//!   server is started on the returned listener, and a real GET to the reported
//!   URL returns 200 (reachable, no dead listener — SC-003 / FR-020). This drives
//!   `probe_free_listener`, the exact unit `serve::run` uses for the default path.
//!   It FAILS against the pre-fix fixed-bind behavior (there was no probe; a
//!   direct bind of the occupied port errored).
//! * `default_free_is_honored_exactly` — control: a verified-free preferred port
//!   is returned unchanged (no needless ephemeral substitution).
//! * `explicit_occupied_fails_loudly` — an EXPLICIT address (the `bind_listener`
//!   path `serve::run` uses when `explicit_listen == true`) returns an `Err` on
//!   an occupied port: no silent substitution (FR-002/003).
//! * `default_listen_constant_is_loopback_8787` — pins the historical default the
//!   no-address path prefers.
//!
//! ## Needs live-verify (not unit-covered here)
//!
//! The full `serve::run` startup (index load + `/mcp` + `/admin` mount + graceful
//! shutdown) is not spawned here — it runs until a shutdown signal and loads the
//! real project index, which is out of scope for a deterministic unit. The
//! port-*selection* unit it depends on (`probe_free_listener`) is fully covered
//! above with a real bound+reachable server; an end-to-end `symforge serve` with
//! 8787 occupied is the live-dogfood step (tasks T028).
#![cfg(feature = "server")]

use std::net::SocketAddr;

use symforge::server::serve::{
    DEFAULT_LISTEN, bind_listener, probe_free_listener, probe_free_port,
};

/// Occupy a loopback port with an **exclusive** listener (plain `std` bind, no
/// `SO_REUSEADDR`) — the honest reproduction of a real squatter (`wslrelay` /
/// another service). A `bind_listener` (which sets `SO_REUSEADDR`) on the same
/// port then fails, so the probe actually falls back. A `bind_listener` occupier
/// would (wrongly) let the probe share the port and never fall back: on Windows
/// two sockets share a port only if both set `SO_REUSEADDR`, and on Linux
/// `SO_REUSEADDR` does not let a second socket bind an active listening port.
fn occupy_a_port() -> (std::net::TcpListener, SocketAddr) {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("exclusive occupy a loopback port");
    let addr = listener.local_addr().expect("local_addr");
    (listener, addr)
}

/// Serve a trivial `200 OK` router on `listener` until the returned sender fires.
fn serve_until_shutdown(
    listener: tokio::net::TcpListener,
) -> (
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let addr = listener.local_addr().expect("local_addr");
    let app = axum::Router::new().route("/", axum::routing::get(|| async { "ok" }));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let shutdown = async {
            let _ = rx.await;
        };
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
    });
    (addr, tx, join)
}

#[tokio::test]
async fn default_occupied_falls_back_to_a_reachable_port() {
    // Occupy the "preferred" port (stand-in for an already-squatted 8787), then
    // ask the probe to prefer it. The bug fix: instead of failing, the probe
    // returns a live listener on a DIFFERENT, OS-assigned free port.
    let (occupier, occupied) = occupy_a_port();

    let listener = probe_free_listener(Some(occupied)).expect("probe must fall back, not fail");
    let chosen = listener.local_addr().expect("local_addr");
    assert_ne!(
        chosen.port(),
        occupied.port(),
        "must NOT bind the occupied port (the pre-fix bug)"
    );
    assert_ne!(chosen.port(), 0, "must resolve a concrete OS-assigned port");
    assert!(chosen.ip().is_loopback(), "fallback stays on loopback");

    // The reported URL must actually serve (FR-020 / SC-003: no dead listener).
    let (addr, shutdown, join) = serve_until_shutdown(listener);
    let reported_url = format!("http://{addr}/");
    let status = reqwest::Client::new()
        .get(&reported_url)
        .send()
        .await
        .expect("GET the reported fallback URL")
        .status();
    assert!(
        status.is_success(),
        "the reported fallback URL must be reachable, got {status}"
    );

    let _ = shutdown.send(());
    let _ = join.await;
    drop(occupier);
}

#[tokio::test]
async fn default_free_is_honored_exactly() {
    // Control: a verified-free preferred port is honored exactly — no needless
    // ephemeral substitution.
    let (scratch, free_addr) = occupy_a_port();
    drop(scratch); // free it so `free_addr` is now bindable

    let chosen = probe_free_port(Some(free_addr)).expect("probe a free preferred port");
    assert_eq!(
        chosen, free_addr,
        "a free preferred port is returned unchanged"
    );
}

#[tokio::test]
async fn explicit_occupied_fails_loudly() {
    // The EXPLICIT-address path `serve::run` uses (`explicit_listen == true`)
    // calls `bind_listener` directly: an occupied explicit port must error, never
    // silently substitute (FR-002/003).
    let (occupier, occupied) = occupy_a_port();

    let result = bind_listener(occupied);
    assert!(
        result.is_err(),
        "an explicit occupied address must fail loudly (no substitution)"
    );

    drop(occupier);
}

#[test]
fn default_listen_constant_is_loopback_8787() {
    // Pin the historical default the no-explicit-address path prefers.
    let addr: SocketAddr = DEFAULT_LISTEN.parse().expect("DEFAULT_LISTEN parses");
    assert!(addr.ip().is_loopback(), "default bind is loopback");
    assert_eq!(addr.port(), 8787, "historical default product port");
}
