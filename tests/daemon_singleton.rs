//! Task 9: daemon uniqueness — the guarded-start seam must never bind a
//! second daemon (or overwrite a live runtime record) for the same
//! `SYMFORGE_HOME`.
//!
//! `ensure_daemon_running` (auto-spawn) already serializes spawns behind the
//! start lock, but the foreground `symforge daemon` path used to bind and
//! clobber the port/token files unconditionally. Both paths now converge on
//! `guarded_daemon_start`: acquire the start lock, re-check for a live
//! compatible daemon, stop an incompatible record, and only then bind in the
//! current process — all under the lock.

use symforge::daemon::{GuardedStart, guarded_daemon_start, spawn_daemon};
use tempfile::TempDir;

fn read_port_file(home: &std::path::Path) -> u16 {
    let os = std::env::consts::OS;
    let contents = std::fs::read_to_string(home.join(format!("daemon.{os}.port")))
        .expect("daemon port file should exist");
    contents.trim().parse().expect("port file should be a u16")
}

/// A guarded start against a home that already has a live compatible daemon
/// must return `AlreadyRunning` with that daemon's port and leave its runtime
/// record untouched.
#[allow(unsafe_code)] // test-only daemon home override; single-threaded binary.
#[tokio::test]
async fn test_guarded_start_refuses_to_replace_live_daemon() {
    let daemon_home = TempDir::new().expect("daemon home temp dir");
    // SAFETY: integration test binary runs with --test-threads=1; no
    // concurrent env access.
    unsafe {
        std::env::set_var("SYMFORGE_HOME", daemon_home.path());
        std::env::set_var("SYMFORGE_DAEMON_AUTH_TOKEN", "daemon-singleton-test-token");
    }

    let first = spawn_daemon("127.0.0.1").await.expect("first daemon");
    let recorded_port = read_port_file(daemon_home.path());
    assert_eq!(recorded_port, first.port, "record belongs to first daemon");

    match guarded_daemon_start("127.0.0.1")
        .await
        .expect("guarded start should not error")
    {
        GuardedStart::AlreadyRunning { port } => {
            assert_eq!(port, first.port, "guard must report the LIVE daemon");
        }
        GuardedStart::Started(_) => {
            panic!("guarded start bound a SECOND daemon over a live one")
        }
    }

    assert_eq!(
        read_port_file(daemon_home.path()),
        first.port,
        "live daemon's runtime record must never be overwritten"
    );
}

/// Two guarded starts racing the same `SYMFORGE_HOME` must yield exactly ONE
/// bound daemon; the loser observes the winner instead of clobbering it.
#[allow(unsafe_code)] // test-only daemon home override; single-threaded binary.
#[tokio::test]
async fn test_concurrent_guarded_starts_yield_one_daemon() {
    let daemon_home = TempDir::new().expect("daemon home temp dir");
    // SAFETY: integration test binary runs with --test-threads=1; no
    // concurrent env access.
    unsafe {
        std::env::set_var("SYMFORGE_HOME", daemon_home.path());
        std::env::set_var("SYMFORGE_DAEMON_AUTH_TOKEN", "daemon-singleton-test-token");
    }

    let (a, b) = tokio::join!(
        guarded_daemon_start("127.0.0.1"),
        guarded_daemon_start("127.0.0.1"),
    );
    let outcomes = [a.expect("racer a"), b.expect("racer b")];

    let started: Vec<u16> = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            GuardedStart::Started(handle) => Some(handle.port),
            GuardedStart::AlreadyRunning { .. } => None,
        })
        .collect();
    let deferred: Vec<u16> = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            GuardedStart::AlreadyRunning { port } => Some(*port),
            GuardedStart::Started(_) => None,
        })
        .collect();

    assert_eq!(started.len(), 1, "exactly one racer may bind a daemon");
    assert_eq!(
        deferred.len(),
        1,
        "the other racer must defer to the winner"
    );
    assert_eq!(
        deferred[0], started[0],
        "loser must report the winner's port"
    );
    assert_eq!(
        read_port_file(daemon_home.path()),
        started[0],
        "the single runtime record must belong to the winner"
    );
}
