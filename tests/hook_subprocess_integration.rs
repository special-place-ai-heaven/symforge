//! Subprocess-level end-to-end tests for `run_hook`'s adoption-log dispatch
//! sites.
//!
//! Follow-up to the in-crate tests added in `src/cli/hook.rs` by the
//! daemon-and-sidecar tentacle (swarm-2). Those tests pin the metric
//! rendering + the counter wire-up from `record_hook_outcome` into
//! `ADOPTION_LOG_FILE` by calling `record_hook_outcome` directly. That
//! leaves the three dispatch sites inside `run_hook` itself
//! code-review-guarded: someone could remove a `record_hook_outcome*` call
//! and the in-crate tests would still pass.
//!
//! These tests spawn the real `symforge` binary in a tempdir and pin each
//! site end-to-end:
//!
//!   1. `no_sidecar` — port file missing and daemon fallback fails.
//!      Exercises `record_hook_outcome_with_detail(NoSidecar,
//!      reason="sidecar_port_missing")`.
//!   2. `stale_port` — port file present but the listener never accepts,
//!      so the subprocess's 50ms HTTP read times out. Exercises
//!      `record_hook_outcome_with_detail(NoSidecar,
//!      reason="sidecar_port_stale")`.
//!   3. `routed_success` — port file points at a minimal in-test TCP
//!      responder that returns `HTTP/1.1 200 OK`. Exercises the plain
//!      `record_hook_outcome(Routed)` call on the success path.
//!   4. `stale_sidecar_with_live_daemon` — port file present but the
//!      sidecar is dead, while a mock daemon is reachable via
//!      `SYMFORGE_HOME`. Pins the stale-sidecar daemon fallback: the hook
//!      must serve the daemon's ENRICHED body and record `DaemonFallback`,
//!      not fail open. This is the reliability-gap regression guard.
//!   5. `stale_sidecar_and_dead_daemon` — both unreachable. Pins the
//!      degrade-to-pass-through guarantee: no hang, no error, recorded as
//!      `no-sidecar`.
//!
//! The adoption-log sites assert against the tab-separated substring format
//! written by `append_hook_adoption_event*`:
//! `<session>\t<workflow>\t<outcome>`. The session id is left unpinned
//! (normalized to `-` when no daemon session file is present), leaving only
//! the `(workflow, outcome)` pair checked.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Mirrors `ADOPTION_LOG_FILE` in `src/cli/hook.rs`. Intentionally
/// duplicated: if the constant is renamed, the in-crate test
/// `test_record_hook_outcome_writes_to_adoption_log_file_constant`
/// catches it; if the constant's consumer inside `run_hook` drops its
/// call site, these tests catch it. The pair pins the full chain.
const ADOPTION_LOG_RELATIVE: &str = ".symforge/hook-adoption.log";

/// Mirrors `PORT_FILE` in `src/cli/hook.rs`. Any rename of either side
/// without updating this constant breaks the stale-port and routed
/// tests loudly.
const PORT_FILE_RELATIVE: &str = ".symforge/sidecar.port";

/// Minimal PostToolUse/Read payload for the stdin-routing path. The
/// `.rs` extension keeps `should_fail_open_read` from downgrading the
/// workflow to PassThrough (which skips `record_hook_outcome` and would
/// turn every test in this file into a no-op).
const READ_PAYLOAD: &str = r#"{"tool_name":"Read","tool_input":{"file_path":"src/foo.rs"}}"#;

/// Pin site 1: no sidecar, no daemon fallback.
#[test]
fn run_hook_no_sidecar_writes_source_read_no_sidecar_event() {
    let tmp = TempDir::new().expect("tempdir creation");
    let contents = run_hook_in_tempdir(tmp.path(), READ_PAYLOAD);
    assert!(
        contents.contains("\tsource-read\tno-sidecar"),
        "log must contain a tab-separated `source-read\\tno-sidecar` entry \
         (regression: record_hook_outcome_with_detail removed from the \
         port-file-missing dispatch branch); got:\n{contents}"
    );
}

/// Pin site 2: port file present, HTTP read times out.
#[test]
fn run_hook_stale_port_writes_source_read_no_sidecar_event() {
    let tmp = TempDir::new().expect("tempdir creation");
    std::fs::create_dir_all(tmp.path().join(".symforge")).expect("create .symforge dir");

    // Bind an ephemeral port and HOLD the listener for the entire test —
    // never accept. Subprocess's TCP connect may succeed (SYN queued) or
    // fail depending on backlog; either way the 50ms read timeout in
    // `sync_http_get_with_timeout` trips, producing an `Err` that drives
    // `run_hook` into the stale-port branch.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stale-port listener");
    let stale_port = listener.local_addr().expect("stale-port local_addr").port();
    std::fs::write(tmp.path().join(PORT_FILE_RELATIVE), stale_port.to_string())
        .expect("write stale port file");

    let contents = run_hook_in_tempdir(tmp.path(), READ_PAYLOAD);
    drop(listener);

    assert!(
        contents.contains("\tsource-read\tno-sidecar"),
        "log must contain a tab-separated `source-read\\tno-sidecar` entry \
         (regression: record_hook_outcome_with_detail removed from the \
         stale-port dispatch branch); got:\n{contents}"
    );
}

/// Pin site 3: port file points at a responder; HTTP call succeeds.
#[test]
fn run_hook_routed_success_writes_source_read_routed_event() {
    // Bind first so the port is known before the subprocess launches.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let port = listener
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();

    // Minimal single-shot HTTP responder. Started BEFORE the subprocess
    // spawns so the accept loop is already waiting when the subprocess
    // connects — the 50ms HTTP_TIMEOUT leaves no room for thread start-up
    // races. Writes a fixed 200-OK response and drops the stream, which
    // closes the connection and lets the subprocess's `read_to_string`
    // return.
    let mock = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        }
    });

    let tmp = TempDir::new().expect("tempdir creation");
    std::fs::create_dir_all(tmp.path().join(".symforge")).expect("create .symforge dir");
    std::fs::write(tmp.path().join(PORT_FILE_RELATIVE), port.to_string())
        .expect("write mock port file");

    let contents = run_hook_in_tempdir(tmp.path(), READ_PAYLOAD);

    // Best-effort join: if the subprocess served successfully, the mock
    // has already exited. If it failed early, the accept thread may still
    // block; we don't want to hang the test runner, so the JoinHandle is
    // consumed with a non-blocking check and otherwise detached — the
    // thread dies when the test binary process exits.
    drop(mock);

    assert!(
        contents.contains("\tsource-read\trouted"),
        "log must contain a tab-separated `source-read\\trouted` entry \
         (regression: record_hook_outcome removed from the success \
         dispatch branch); got:\n{contents}"
    );
}

/// Pin the stale-sidecar daemon-fallback path: the sidecar port file points
/// at a dead listener (HTTP times out), but a live mock daemon is reachable
/// via `SYMFORGE_HOME`. The hook must route the SAME enrichment request
/// through the daemon and emit ENRICHED output — never a bare pass-through.
///
/// Regression guard for the asymmetry where `run_hook` only attempted the
/// daemon fallback on a MISSING port file, silently failing open whenever the
/// port file existed but the sidecar was dead.
#[test]
fn run_hook_stale_sidecar_with_live_daemon_routes_via_daemon_fallback() {
    // 1. Dead sidecar: bind-and-hold a port that never accepts, so the
    //    subprocess's 50ms HTTP read trips into the stale-sidecar branch.
    let dead_sidecar = TcpListener::bind("127.0.0.1:0").expect("bind dead sidecar listener");
    let stale_port = dead_sidecar
        .local_addr()
        .expect("dead sidecar local_addr")
        .port();

    // 2. Live mock daemon serving the three fallback endpoints + enrichment.
    let daemon = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon listener");
    let daemon_port = daemon.local_addr().expect("daemon local_addr").port();

    // The repo cwd whose canonical root the daemon must advertise.
    let tmp = TempDir::new().expect("tempdir creation");
    std::fs::create_dir_all(tmp.path().join(".symforge")).expect("create .symforge dir");
    std::fs::write(tmp.path().join(PORT_FILE_RELATIVE), stale_port.to_string())
        .expect("write stale port file");

    // The daemon process matches projects by canonical root (same
    // canonicalization + normalization the hook applies), so advertise the
    // canonicalized tempdir.
    let canonical_root = std::fs::canonicalize(tmp.path())
        .unwrap_or_else(|_| tmp.path().to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");

    // SYMFORGE_HOME hosts the daemon port file the hook's daemon fallback reads.
    let home = TempDir::new().expect("home tempdir creation");
    let daemon_port_file = home
        .path()
        .join(format!("daemon.{}.port", std::env::consts::OS));
    std::fs::write(&daemon_port_file, daemon_port.to_string()).expect("write daemon port file");

    let daemon_thread = thread::spawn(move || serve_mock_daemon(daemon, &canonical_root));

    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );

    drop(dead_sidecar);
    let _ = daemon_thread.join();

    // The enriched marker body served by the mock daemon must reach stdout —
    // proves the hook served the daemon's enriched result, not a fail-open.
    assert!(
        stdout.contains(ENRICHED_MARKER),
        "stdout must contain the daemon-served enriched body marker \
         `{ENRICHED_MARKER}` (regression: stale-sidecar path failed open \
         instead of routing through the daemon); got:\n{stdout}"
    );
    // The adoption log must record the degraded-but-routed state honestly.
    assert!(
        log.contains("\tsource-read\tdaemon-fallback"),
        "log must contain a tab-separated `source-read\\tdaemon-fallback` \
         entry (regression: stale sidecar served via daemon must record \
         DaemonFallback, not no-sidecar); got:\n{log}"
    );
    assert!(
        !log.contains("\tsource-read\tno-sidecar"),
        "stale sidecar with a live daemon must NOT record no-sidecar; got:\n{log}"
    );
}

/// Pin the degrade-to-pass-through guarantee: when BOTH the sidecar and the
/// daemon are unreachable, the hook must still fail open cleanly (no hang, no
/// error) and record `no-sidecar` with the stale reason.
#[test]
fn run_hook_stale_sidecar_and_dead_daemon_degrades_to_pass_through() {
    // Dead sidecar (HTTP times out).
    let dead_sidecar = TcpListener::bind("127.0.0.1:0").expect("bind dead sidecar listener");
    let stale_port = dead_sidecar
        .local_addr()
        .expect("dead sidecar local_addr")
        .port();

    // Dead daemon: bind-and-hold a port pointed at by the daemon port file but
    // never accept, so the daemon fallback's first HTTP round-trip times out.
    let dead_daemon = TcpListener::bind("127.0.0.1:0").expect("bind dead daemon listener");
    let dead_daemon_port = dead_daemon
        .local_addr()
        .expect("dead daemon local_addr")
        .port();

    let tmp = TempDir::new().expect("tempdir creation");
    std::fs::create_dir_all(tmp.path().join(".symforge")).expect("create .symforge dir");
    std::fs::write(tmp.path().join(PORT_FILE_RELATIVE), stale_port.to_string())
        .expect("write stale port file");

    let home = TempDir::new().expect("home tempdir creation");
    std::fs::write(
        home.path()
            .join(format!("daemon.{}.port", std::env::consts::OS)),
        dead_daemon_port.to_string(),
    )
    .expect("write daemon port file");

    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );

    drop(dead_sidecar);
    drop(dead_daemon);

    // Must degrade to a valid fail-open JSON pass-through, never the enriched
    // marker (no daemon served it) and never a crash.
    assert!(
        !stdout.contains(ENRICHED_MARKER),
        "no enrichment source is reachable, so stdout must not contain the \
         daemon marker; got:\n{stdout}"
    );
    assert!(
        log.contains("\tsource-read\tno-sidecar"),
        "both sidecar and daemon unreachable must degrade to no-sidecar; \
         got:\n{log}"
    );
}

/// Marker body returned by the mock daemon's enrichment endpoint. Distinct
/// from any fail-open output so the test can prove enrichment was served.
const ENRICHED_MARKER: &str = "MOCK_DAEMON_ENRICHED_OUTLINE";

/// Single-purpose mock daemon HTTP server for the fallback test. Serves each
/// `Connection: close` request the hook makes — `/v1/projects`, the project's
/// `/sessions` list, then the `/v1/sessions/{id}/sidecar/outline` enrichment —
/// routing by request-line path. Loops until the enrichment request is served
/// or the listener is dropped.
fn serve_mock_daemon(listener: TcpListener, canonical_root: &str) {
    // Non-blocking accept so a missing enrichment request can never hang the
    // join() on the test thread — the deadline always wins.
    listener
        .set_nonblocking(true)
        .expect("set mock daemon listener non-blocking");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(_) => return,
        };
        // The accepted stream inherits non-blocking; restore blocking + a read
        // timeout so the request read below behaves like a normal server.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");

        let body: String = if path.starts_with("/v1/projects/") && path.contains("/sessions") {
            // Sessions list for the matched project.
            r#"[{"session_id":"mock-session","last_seen_at_unix_secs":1}]"#.to_string()
        } else if path.starts_with("/v1/projects") {
            // Projects list — advertise our canonical root.
            format!(
                r#"[{{"project_id":"mock-project","canonical_root":"{}","session_count":1}}]"#,
                canonical_root.replace('"', "\\\"")
            )
        } else {
            // Enrichment endpoint (/v1/sessions/.../sidecar/outline).
            let last = path.contains("sidecar");
            let out = format!("{{\"enriched\":\"{ENRICHED_MARKER}\"}}");
            write_http_ok(&mut stream, &out);
            if last {
                return;
            }
            continue;
        };

        write_http_ok(&mut stream, &body);
    }
}

/// Write a minimal `200 OK` HTTP response with the given body and close.
fn write_http_ok(stream: &mut std::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Spawn `symforge hook` in `cwd`, pipe `payload` on stdin, wait for exit,
/// and return the adoption log contents. Panics with a clear message if
/// the subprocess doesn't exit, exits non-zero, or doesn't create the
/// log file. Shared across all three site tests.
fn run_hook_in_tempdir(cwd: &Path, payload: &str) -> String {
    run_hook_in_tempdir_with_env(cwd, payload, &[]).1
}

/// Like `run_hook_in_tempdir` but allows injecting extra environment variables
/// and returns `(stdout, adoption_log_contents)` so callers can assert on the
/// enriched body emitted to stdout as well as the recorded outcome.
fn run_hook_in_tempdir_with_env(
    cwd: &Path,
    payload: &str,
    extra_env: &[(&str, &str)],
) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_symforge");
    let mut command = Command::new(bin);
    command
        .arg("hook")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("symforge binary should spawn");

    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(payload.as_bytes())
        .expect("write hook payload to child stdin");
    drop(child.stdin.take());

    let status = wait_with_timeout(&mut child, Duration::from_secs(15))
        .expect("hook subprocess should exit within 15s")
        .expect("hook subprocess status readable");
    assert!(
        status.success(),
        "symforge hook exited non-zero: {status:?}"
    );

    // Capture stdout after exit. The hook emits a single short JSON line, far
    // below the pipe buffer size, so reading post-exit cannot deadlock.
    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }

    let log_path = cwd.join(ADOPTION_LOG_RELATIVE);
    assert!(
        log_path.exists(),
        "run_hook must append to {ADOPTION_LOG_RELATIVE} under the child's cwd; \
         missing at {}. This usually means a record_hook_outcome* call was \
         removed from the run_hook dispatch branch being exercised.",
        log_path.display()
    );

    let log = std::fs::read_to_string(&log_path).expect("log readable");
    (stdout, log)
}

/// Poll the child for exit with a timeout. `Ok(Some)` on clean exit,
/// `Ok(None)` on timeout (after killing the child), `Err` on wait
/// failure. Local to avoid pulling in an async runtime just for this.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(Some(status)),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
}
