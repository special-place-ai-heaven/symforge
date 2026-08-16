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
//!   1. `no_sidecar` — sidecar descriptor missing and daemon fallback fails.
//!      Exercises `record_hook_outcome_with_detail(NoSidecar,
//!      reason="sidecar_port_missing")`.
//!   2. `stale_port` — sidecar descriptor present but the listener never accepts,
//!      so the subprocess's 50ms HTTP read times out. Exercises
//!      `record_hook_outcome_with_detail(NoSidecar,
//!      reason="sidecar_port_stale")`.
//!   3. `routed_success` — sidecar descriptor points at a minimal in-test TCP
//!      responder that returns `HTTP/1.1 200 OK`. Exercises the plain
//!      `record_hook_outcome(Routed)` call on the success path.
//!   4. `stale_sidecar_with_live_daemon` — descriptor present but the
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
#![cfg(feature = "server")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Mirrors `ADOPTION_LOG_FILE` in `src/cli/hook.rs`. Intentionally
/// duplicated: if the constant is renamed, the in-crate test
/// `test_record_hook_outcome_writes_to_adoption_log_file_constant`
/// catches it; if the constant's consumer inside `run_hook` drops its
/// call site, these tests catch it. The pair pins the full chain.
const ADOPTION_LOG_FILE: &str = "hook-adoption.log";

fn write_sidecar_descriptor(control_root: &Path, project_root: &Path, port: u16) {
    let control_state = symforge::domain::ControlStateDir::new(control_root.to_path_buf());
    symforge::sidecar::port_file::write_session_descriptor(
        &control_state,
        port,
        None,
        Some(project_root),
        None,
    )
    .expect("write sidecar session descriptor");
}

fn write_daemon_session_descriptor(
    control_root: &Path,
    project_root: &Path,
    port: u16,
    session_id: &str,
) {
    let control_state = symforge::domain::ControlStateDir::new(control_root.to_path_buf());
    symforge::sidecar::port_file::write_session_descriptor(
        &control_state,
        port,
        Some(session_id),
        Some(project_root),
        None,
    )
    .expect("write daemon-backed session descriptor");
}

fn write_daemon_port(control_root: &Path, port: u16) {
    let control_state = symforge::domain::ControlStateDir::new(control_root.to_path_buf());
    let daemon_dir = symforge::paths::control_state_path(&control_state, "daemon");
    std::fs::create_dir_all(&daemon_dir).expect("create daemon control directory");
    std::fs::write(
        daemon_dir.join(symforge::paths::os_tagged_runtime_file_name(
            "daemon", "port",
        )),
        port.to_string(),
    )
    .expect("write daemon port file");
}

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
    let home = TempDir::new().expect("control-state tempdir creation");

    // Bind an ephemeral port and HOLD the listener for the entire test —
    // never accept. Subprocess's TCP connect may succeed (SYN queued) or
    // fail depending on backlog; either way the 50ms read timeout in
    // `sync_http_get_with_timeout` trips, producing an `Err` that drives
    // `run_hook` into the stale-port branch.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stale-port listener");
    let stale_port = listener.local_addr().expect("stale-port local_addr").port();
    write_sidecar_descriptor(home.path(), tmp.path(), stale_port);

    let contents = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    )
    .1;
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

    // Minimal multi-shot HTTP responder. Started BEFORE the subprocess
    // spawns so the accept loop is already waiting when the subprocess
    // connects — the 50ms HTTP_TIMEOUT leaves no room for thread start-up
    // races. GET /health answers a DaemonHealth JSON body (the descriptor
    // boot-epoch probe; no epoch = legacy-compatible accept for a legacy
    // descriptor); any other request gets a fixed 200-OK and drops the
    // stream, which closes the connection and lets the subprocess's
    // `read_to_string` return.
    let mock = thread::spawn(move || {
        for _ in 0..4 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 2048];
            let Ok(read) = stream.read(&mut buf) else {
                return;
            };
            if read == 0 {
                continue;
            }
            if buf.starts_with(b"GET /health ") {
                let body = br#"{"project_count":0,"session_count":0,"daemon_version":"10.1.0","executable_path":"x","auth_required":true,"pid":0}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    String::from_utf8_lossy(body)
                );
                let _ = stream.write_all(response.as_bytes());
            } else {
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                return;
            }
        }
    });

    let tmp = TempDir::new().expect("tempdir creation");
    let home = TempDir::new().expect("control-state tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), port);

    let contents = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    )
    .1;

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

#[test]
fn run_hook_index_not_ready_fails_open_as_sidecar_error() {
    const PARTIAL_MARKER: &str = "partial-index-context-must-not-escape";

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let port = listener
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();
    let mock = thread::spawn(move || {
        serve_mock_http_response(listener, "503 Service Unavailable", PARTIAL_MARKER)
    });

    let tmp = TempDir::new().expect("tempdir creation");
    let home = TempDir::new().expect("control-state tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), port);

    let (stdout, log, stderr) = run_hook_in_tempdir_with_env_and_stderr(
        tmp.path(),
        READ_PAYLOAD,
        &[
            ("SYMFORGE_HOME", home.path().to_string_lossy().as_ref()),
            ("SYMFORGE_HOOK_VERBOSE", "1"),
        ],
    );
    mock.join().expect("mock sidecar thread joins");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid fail-open JSON");
    assert_eq!(parsed["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert_eq!(
        parsed["hookSpecificOutput"]["additionalContext"], "",
        "an index-loading refusal must fail open with empty context"
    );
    assert!(
        !stdout.contains(PARTIAL_MARKER),
        "503 response body must not be injected into hook context; got:\n{stdout}"
    );
    assert!(
        log.contains("\tsource-read\tsidecar-error"),
        "an index-loading refusal must use the honest sidecar-error lane; \
         stderr:\n{stderr}\nlog:\n{log}"
    );
    assert!(
        !log.contains("sidecar_port_stale"),
        "an index-loading refusal must not suggest restarting a live sidecar; got:\n{log}"
    );
    assert!(
        stderr.contains("index not ready"),
        "verbose diagnostics must name the live-but-loading condition; got:\n{stderr}"
    );
    assert!(
        !stderr.contains("sidecar not running"),
        "a live sidecar's 503 must not emit the restart diagnostic; got:\n{stderr}"
    );
    let hint_marker = symforge::paths::control_state_path(
        &symforge::domain::ControlStateDir::new(home.path().to_path_buf()),
        "hook-hint-shown",
    );
    assert!(
        !hint_marker.exists(),
        "an index-loading refusal must not create the sidecar restart-hint marker"
    );
}

#[test]
fn run_hook_local_http_500_fails_open_as_http_failure_without_restart_hint() {
    const ERROR_MARKER: &str = "live-sidecar-500-body-must-not-escape";

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let port = listener
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();
    let mock = thread::spawn(move || {
        serve_mock_http_response(listener, "500 Internal Server Error", ERROR_MARKER)
    });

    let tmp = TempDir::new().expect("tempdir creation");
    let home = TempDir::new().expect("control-state tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), port);

    let (stdout, log, stderr) = run_hook_in_tempdir_with_env_and_stderr(
        tmp.path(),
        READ_PAYLOAD,
        &[
            ("SYMFORGE_HOME", home.path().to_string_lossy().as_ref()),
            ("SYMFORGE_HOOK_VERBOSE", "1"),
        ],
    );
    mock.join().expect("mock sidecar thread joins");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid fail-open JSON");
    assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "");
    assert!(!stdout.contains(ERROR_MARKER));
    assert!(log.contains("\tsource-read\tsidecar-error"));
    assert!(!log.contains("sidecar_port_stale"));
    assert!(stderr.contains("outcome=SidecarError reason=http_failure"));
    assert!(!stderr.contains("sidecar not running"));
    assert!(!stderr.contains("restart_sidecar"));
    let hint_marker = symforge::paths::control_state_path(
        &symforge::domain::ControlStateDir::new(home.path().to_path_buf()),
        "hook-hint-shown",
    );
    assert!(!hint_marker.exists());
}

#[test]
fn run_hook_root_conflict_fails_open_without_stale_sidecar_hint() {
    const CONFLICT_MARKER: &str = "wrong-project-context-must-not-escape";

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let port = listener
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();
    let mock =
        thread::spawn(move || serve_mock_http_response(listener, "409 Conflict", CONFLICT_MARKER));

    let tmp = TempDir::new().expect("tempdir creation");
    let home = TempDir::new().expect("control-state tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), port);
    let (stdout, log, stderr) = run_hook_in_tempdir_with_env_and_stderr(
        tmp.path(),
        READ_PAYLOAD,
        &[
            ("SYMFORGE_HOME", home.path().to_string_lossy().as_ref()),
            ("SYMFORGE_HOOK_VERBOSE", "1"),
        ],
    );
    mock.join().expect("mock sidecar thread joins");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid fail-open JSON");
    assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "");
    assert!(!stdout.contains(CONFLICT_MARKER));
    assert!(log.contains("\tsource-read\tsidecar-error"));
    assert!(!log.contains("sidecar_port_stale"));
    assert!(stderr.contains("root conflict"));
    assert!(!stderr.contains("sidecar not running"));
    let hint_marker = symforge::paths::control_state_path(
        &symforge::domain::ControlStateDir::new(home.path().to_path_buf()),
        "hook-hint-shown",
    );
    assert!(!hint_marker.exists());
}

#[test]
fn run_hook_index_not_ready_uses_ready_daemon_fallback() {
    const PARTIAL_MARKER: &str = "partial-local-sidecar-context-must-not-escape";

    let sidecar = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let sidecar_port = sidecar
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();
    let sidecar_thread = thread::spawn(move || {
        serve_mock_http_response(sidecar, "503 Service Unavailable", PARTIAL_MARKER)
    });

    let daemon = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon listener");
    let daemon_port = daemon.local_addr().expect("daemon local_addr").port();
    let tmp = TempDir::new().expect("tempdir creation");
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());
    let daemon_thread =
        thread::spawn(move || serve_mock_daemon(daemon, &canonical_root, &project_id));

    let home = TempDir::new().expect("control-state tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), sidecar_port);
    write_daemon_port(home.path(), daemon_port);

    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );
    sidecar_thread
        .join()
        .expect("mock loading sidecar thread joins");
    daemon_thread.join().expect("mock daemon thread joins");

    assert!(
        stdout.contains(ENRICHED_MARKER),
        "a ready daemon must enrich when the selected local sidecar is still loading; got:\n{stdout}"
    );
    assert!(
        !stdout.contains(PARTIAL_MARKER),
        "the local 503 body must never be injected; got:\n{stdout}"
    );
    assert!(
        log.contains("mock-session\tsource-read\tdaemon-fallback"),
        "daemon-routed enrichment must be attributed to the daemon session; got:\n{log}"
    );
}

#[test]
fn run_hook_daemon_index_not_ready_uses_daemon_session_error() {
    const LOCAL_PARTIAL: &str = "partial-local-context-must-not-escape";
    const DAEMON_PARTIAL: &str = "partial-daemon-context-must-not-escape";

    let sidecar = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let sidecar_port = sidecar
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();
    let sidecar_thread = thread::spawn(move || {
        serve_mock_http_response(sidecar, "503 Service Unavailable", LOCAL_PARTIAL)
    });

    let daemon = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon listener");
    let daemon_port = daemon.local_addr().expect("daemon local_addr").port();
    let tmp = TempDir::new().expect("tempdir creation");
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());
    let daemon_thread = thread::spawn(move || {
        serve_mock_daemon_with_enrichment(
            daemon,
            &canonical_root,
            &project_id,
            "503 Service Unavailable",
            DAEMON_PARTIAL,
        )
    });

    let home = TempDir::new().expect("control-state tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), sidecar_port);
    write_daemon_port(home.path(), daemon_port);

    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );
    sidecar_thread
        .join()
        .expect("mock loading sidecar thread joins");
    daemon_thread.join().expect("mock daemon thread joins");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid fail-open JSON");
    assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "");
    assert!(!stdout.contains(LOCAL_PARTIAL));
    assert!(!stdout.contains(DAEMON_PARTIAL));
    assert!(
        log.contains("mock-session\tsource-read\tsidecar-error"),
        "the daemon's 503 must be attributed to the daemon session; got:\n{log}"
    );
    assert!(!log.contains("sidecar_port_stale"));
    assert!(!log.contains("restart_sidecar"));
}

#[test]
fn run_hook_daemon_root_conflict_uses_daemon_session_error() {
    const LOCAL_PARTIAL: &str = "partial-local-context-before-daemon-conflict";
    const DAEMON_CONFLICT: &str = "wrong-project-daemon-context-must-not-escape";

    let sidecar = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let sidecar_port = sidecar
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();
    let sidecar_thread = thread::spawn(move || {
        serve_mock_http_response(sidecar, "503 Service Unavailable", LOCAL_PARTIAL)
    });

    let daemon = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon listener");
    let daemon_port = daemon.local_addr().expect("daemon local_addr").port();
    let tmp = TempDir::new().expect("tempdir creation");
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());
    let daemon_thread = thread::spawn(move || {
        serve_mock_daemon_with_enrichment(
            daemon,
            &canonical_root,
            &project_id,
            "409 Conflict",
            DAEMON_CONFLICT,
        )
    });

    let home = TempDir::new().expect("control-state tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), sidecar_port);
    write_daemon_port(home.path(), daemon_port);
    let (stdout, log, stderr) = run_hook_in_tempdir_with_env_and_stderr(
        tmp.path(),
        READ_PAYLOAD,
        &[
            ("SYMFORGE_HOME", home.path().to_string_lossy().as_ref()),
            ("SYMFORGE_HOOK_VERBOSE", "1"),
        ],
    );
    sidecar_thread
        .join()
        .expect("mock loading sidecar thread joins");
    daemon_thread.join().expect("mock daemon thread joins");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid fail-open JSON");
    assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "");
    assert!(!stdout.contains(LOCAL_PARTIAL));
    assert!(!stdout.contains(DAEMON_CONFLICT));
    assert!(log.contains("mock-session\tsource-read\tsidecar-error"));
    assert!(!log.contains("sidecar_port_stale"));
    assert!(stderr.contains("reason=root_conflict"));
    assert!(!stderr.contains("restart_sidecar"));
}

#[test]
fn run_hook_initial_daemon_index_not_ready_fails_open_without_retry() {
    const DAEMON_PARTIAL: &str = "initial-daemon-partial-context-must-not-escape";

    let daemon = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon listener");
    let daemon_port = daemon.local_addr().expect("daemon local_addr").port();
    let tmp = TempDir::new().expect("tempdir creation");
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());
    let expected_project_id = project_id.clone();
    let daemon_thread = thread::spawn(move || {
        serve_initially_discovered_loading_daemon(
            daemon,
            &canonical_root,
            &project_id,
            DAEMON_PARTIAL,
        )
    });

    let home = TempDir::new().expect("control-state tempdir creation");
    write_daemon_port(home.path(), daemon_port);
    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );
    let daemon_requests = daemon_thread.join().expect("mock daemon thread joins");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid fail-open JSON");
    assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "");
    assert!(!stdout.contains(DAEMON_PARTIAL));
    assert!(
        log.contains("mock-session\tsource-read\tsidecar-error"),
        "the initially selected daemon's 503 must use its own session; got:\n{log}"
    );
    assert!(!log.contains("sidecar_port_stale"));
    let project_sessions_route = format!("/v1/projects/{expected_project_id}/sessions");
    assert_request_routes(
        &daemon_requests,
        &[
            "/v1/projects",
            project_sessions_route.as_str(),
            "/v1/sessions/mock-session/sidecar/outline",
        ],
        "initial daemon discovery",
    );
    assert!(
        daemon_requests
            .last()
            .is_some_and(|path| path.contains("caller_root=")),
        "the initially discovered daemon request must retain the root fence; got {daemon_requests:?}"
    );
}

#[test]
fn run_hook_daemon_descriptor_index_not_ready_is_not_retried() {
    const DAEMON_PARTIAL: &str = "descriptor-daemon-partial-context-must-not-escape";

    let daemon = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon listener");
    let daemon_port = daemon.local_addr().expect("daemon local_addr").port();
    let tmp = TempDir::new().expect("tempdir creation");
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());
    let daemon_thread = thread::spawn(move || {
        serve_descriptor_selected_loading_daemon(
            daemon,
            &canonical_root,
            &project_id,
            DAEMON_PARTIAL,
        )
    });

    let home = TempDir::new().expect("control-state tempdir creation");
    write_daemon_session_descriptor(home.path(), tmp.path(), daemon_port, "mock-session");
    // Keep daemon discovery available so the regression would make a second
    // enrichment request instead of merely failing to find a fallback port.
    write_daemon_port(home.path(), daemon_port);

    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );
    let enrichment_requests = daemon_thread.join().expect("mock daemon thread joins");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("hook output must be valid fail-open JSON");
    assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "");
    assert!(!stdout.contains(DAEMON_PARTIAL));
    assert_eq!(
        enrichment_requests, 1,
        "a descriptor-selected daemon must not be rediscovered and called twice after 503"
    );
    assert!(log.contains("mock-session\tsource-read\tsidecar-error"));
    assert!(!log.contains("sidecar_port_stale"));
}

#[test]
fn run_hook_empty_descriptor_session_503_routes_locally_then_falls_back() {
    assert_blank_descriptor_session_routes_locally_then_falls_back(
        "",
        "503 Service Unavailable",
        "empty-session-local-503-must-not-escape",
    );
}

#[test]
fn run_hook_whitespace_descriptor_session_409_routes_locally_then_falls_back() {
    assert_blank_descriptor_session_routes_locally_then_falls_back(
        " \t ",
        "409 Conflict",
        "whitespace-session-local-409-must-not-escape",
    );
}

fn assert_blank_descriptor_session_routes_locally_then_falls_back(
    descriptor_session_id: &str,
    initial_status: &'static str,
    initial_body: &'static str,
) {
    let initial = TcpListener::bind("127.0.0.1:0").expect("bind descriptor endpoint");
    let initial_port = initial.local_addr().expect("descriptor local_addr").port();
    let initial_thread = thread::spawn(move || {
        serve_recording_descriptor_endpoint(initial, initial_status, initial_body)
    });

    let fallback = TcpListener::bind("127.0.0.1:0").expect("bind fallback daemon");
    let fallback_port = fallback.local_addr().expect("fallback local_addr").port();
    let tmp = TempDir::new().expect("tempdir creation");
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());
    let expected_project_id = project_id.clone();
    let sessions_json = serde_json::json!([
        {"session_id":"fresh-session","project_id":project_id.as_str(),"last_seen_at_unix_secs":1}
    ])
    .to_string();
    let fallback_thread = thread::spawn(move || {
        serve_recording_daemon(
            fallback,
            &canonical_root,
            &project_id,
            &sessions_json,
            "fresh-session",
        )
    });

    let home = TempDir::new().expect("control-state tempdir creation");
    write_daemon_session_descriptor(home.path(), tmp.path(), initial_port, descriptor_session_id);
    write_daemon_port(home.path(), fallback_port);

    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );
    let initial_requests = initial_thread
        .join()
        .expect("descriptor endpoint thread joins");
    let fallback_requests = fallback_thread
        .join()
        .expect("fallback daemon thread joins");

    assert!(
        stdout.contains(ENRICHED_MARKER),
        "a blank descriptor session id must remain local and permit one daemon fallback; got:\n{stdout}"
    );
    assert!(
        !stdout.contains(initial_body),
        "the initial refusal body must never escape into hook context; got:\n{stdout}"
    );
    assert!(
        log.contains("fresh-session\tsource-read\tdaemon-fallback"),
        "the successful fallback must be attributed to the discovered daemon session; got:\n{log}"
    );

    assert_request_routes(
        &initial_requests,
        &["/health", "/outline"],
        "blank-session descriptor endpoint",
    );
    let local_request = initial_requests
        .last()
        .expect("local enrichment request must exist");
    assert!(
        local_request.contains("path=") && local_request.contains("caller_root="),
        "the local request must preserve both source path and root fence; got {local_request:?}"
    );
    let project_sessions_route = format!("/v1/projects/{expected_project_id}/sessions");
    assert_request_routes(
        &fallback_requests,
        &[
            "/v1/projects",
            project_sessions_route.as_str(),
            "/v1/sessions/fresh-session/sidecar/outline",
        ],
        "blank-session daemon fallback",
    );
    assert!(
        fallback_requests
            .last()
            .is_some_and(|path| path.contains("caller_root=")),
        "the fallback enrichment request must retain the root fence; got {fallback_requests:?}"
    );
}

#[test]
fn run_hook_daemon_descriptor_404_rediscovers_different_session() {
    assert_descriptor_daemon_failure_rediscovers_different_session(
        "404 Not Found",
        "closed-descriptor-session-must-not-escape",
    );
}

#[test]
fn run_hook_daemon_descriptor_409_rediscovers_different_session() {
    assert_descriptor_daemon_failure_rediscovers_different_session(
        "409 Conflict",
        "conflicted-descriptor-session-must-not-escape",
    );
}

fn assert_descriptor_daemon_failure_rediscovers_different_session(
    initial_status: &'static str,
    initial_body: &'static str,
) {
    let initial = TcpListener::bind("127.0.0.1:0").expect("bind descriptor daemon");
    let initial_port = initial
        .local_addr()
        .expect("descriptor daemon local_addr")
        .port();
    let initial_thread = thread::spawn(move || {
        serve_recording_descriptor_endpoint(initial, initial_status, initial_body)
    });

    let fallback = TcpListener::bind("127.0.0.1:0").expect("bind alternate daemon");
    let fallback_port = fallback.local_addr().expect("alternate local_addr").port();
    let tmp = TempDir::new().expect("tempdir creation");
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());
    let expected_project_id = project_id.clone();
    let sessions_json = serde_json::json!([
        {"session_id":"stale-session","project_id":project_id.as_str(),"last_seen_at_unix_secs":2},
        {"session_id":"fresh-session","project_id":project_id.as_str(),"last_seen_at_unix_secs":1}
    ])
    .to_string();
    let fallback_thread = thread::spawn(move || {
        serve_recording_daemon(
            fallback,
            &canonical_root,
            &project_id,
            &sessions_json,
            "fresh-session",
        )
    });

    let home = TempDir::new().expect("control-state tempdir creation");
    write_daemon_session_descriptor(home.path(), tmp.path(), initial_port, "stale-session");
    write_daemon_port(home.path(), fallback_port);

    let (stdout, log) = run_hook_in_tempdir_with_env(
        tmp.path(),
        READ_PAYLOAD,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    );
    let initial_requests = initial_thread
        .join()
        .expect("descriptor daemon thread joins");
    let fallback_requests = fallback_thread
        .join()
        .expect("alternate daemon thread joins");

    assert!(
        stdout.contains(ENRICHED_MARKER),
        "a closed or conflicted descriptor session must recover through a different active session; got:\n{stdout}"
    );
    assert!(
        !stdout.contains(initial_body),
        "the failed descriptor session body must never escape into hook context; got:\n{stdout}"
    );
    assert!(
        log.contains("fresh-session\tsource-read\tdaemon-fallback"),
        "the recovered request must be attributed to the alternate session; got:\n{log}"
    );

    assert_request_routes(
        &initial_requests,
        &["/health", "/v1/sessions/stale-session/sidecar/outline"],
        "descriptor-selected daemon",
    );
    assert!(
        initial_requests
            .last()
            .is_some_and(|path| path.contains("caller_root=")),
        "the failed descriptor request must be root-fenced; got {initial_requests:?}"
    );
    let project_sessions_route = format!("/v1/projects/{expected_project_id}/sessions");
    assert_request_routes(
        &fallback_requests,
        &[
            "/v1/projects",
            project_sessions_route.as_str(),
            "/v1/sessions/fresh-session/sidecar/outline",
        ],
        "alternate daemon discovery",
    );
    assert!(
        fallback_requests
            .last()
            .is_some_and(|path| path.contains("caller_root=")),
        "the alternate daemon request must retain the root fence; got {fallback_requests:?}"
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

    // The daemon process matches projects by canonical root (same
    // canonicalization + normalization the hook applies), so advertise the
    // canonicalized tempdir.
    let canonical_root = canonical_root_for_mock(tmp.path());
    let project_id = project_id_for_mock(tmp.path());

    // SYMFORGE_HOME hosts the daemon port file the hook's daemon fallback reads.
    let home = TempDir::new().expect("home tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), stale_port);
    write_daemon_port(home.path(), daemon_port);

    let daemon_thread =
        thread::spawn(move || serve_mock_daemon(daemon, &canonical_root, &project_id));

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
        log.contains("mock-session\tsource-read\tdaemon-fallback"),
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

    let home = TempDir::new().expect("home tempdir creation");
    write_sidecar_descriptor(home.path(), tmp.path(), stale_port);
    write_daemon_port(home.path(), dead_daemon_port);

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

/// Regression guard: a hook whose stdin is held open without data must exit
/// fail-open instead of hanging.
///
/// `parse_stdin_input` reads stdin on a bounded helper thread and gives up
/// after `STDIN_READ_TIMEOUT_MS`, treating the payload as empty. Before that
/// bound existed, the read blocked until EOF — forever, when the spawning
/// environment kept the inherited pipe open with no writer. The same unbounded
/// read also wedged `sidecar_integration` whenever its harness was launched
/// with an open stdin (the recurring 0-CPU test stall).
#[test]
fn run_hook_stdin_held_open_exits_fail_open_within_deadline() {
    let tmp = TempDir::new().expect("tempdir creation");
    let bin = env!("CARGO_BIN_EXE_symforge");
    let mut child = symforge::process_util::hidden_command(bin)
        .arg("hook")
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("symforge binary should spawn");

    // Deliberately neither write to nor close the child's stdin: the pipe
    // stays open with no writer for the child's whole lifetime. The 5s
    // deadline is generous headroom over the 250ms stdin bound; the kill
    // inside wait_with_timeout keeps a regression from leaking the child.
    let status = wait_with_timeout(&mut child, Duration::from_secs(5))
        .expect("wait on hook subprocess")
        .expect("hook must exit fail-open despite stdin held open, not hang");
    assert!(
        status.success(),
        "hook with held-open stdin must exit zero (fail-open): {status:?}"
    );

    drop(child.stdin.take());
}

/// Marker body returned by the mock daemon's enrichment endpoint. Distinct
/// from any fail-open output so the test can prove enrichment was served.
const ENRICHED_MARKER: &str = "MOCK_DAEMON_ENRICHED_OUTLINE";

fn canonical_root_for_mock(root: &Path) -> String {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let exact = dunce::simplified(&canonical).to_string_lossy();
    if cfg!(windows) {
        exact.replace('\\', "/")
    } else {
        exact.into_owned()
    }
}

/// Obtain the same native-safe project ID production writes into a session
/// descriptor. Mock daemons must not invent an arbitrary ID: hook discovery
/// now requires the advertised ID and canonical root to describe one project.
fn project_id_for_mock(root: &Path) -> String {
    let scratch = TempDir::new().expect("project-id scratch tempdir");
    let control = symforge::domain::ControlStateDir::new(scratch.path().to_path_buf());
    symforge::sidecar::port_file::write_session_descriptor(&control, 0, None, Some(root), None)
        .expect("write project-id probe descriptor");
    let symforge_dir = symforge::sidecar::port_file::ensure_symforge_dir(&control)
        .expect("resolve project-id probe directory");
    let descriptor_path = std::fs::read_dir(symforge_dir.join("sessions"))
        .expect("read project-id probe descriptors")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .expect("project-id probe descriptor exists");
    let descriptor: serde_json::Value = serde_json::from_slice(
        &std::fs::read(descriptor_path).expect("read project-id probe descriptor"),
    )
    .expect("parse project-id probe descriptor");
    descriptor["project_id"]
        .as_str()
        .expect("descriptor carries project_id")
        .to_string()
}

fn assert_request_routes(requests: &[String], expected: &[&str], label: &str) {
    let actual: Vec<_> = requests
        .iter()
        .map(|path| path.split('?').next().unwrap_or(path))
        .collect();
    assert_eq!(
        actual.as_slice(),
        expected,
        "{label} must make exactly the expected requests; full paths: {requests:?}"
    );
}

/// Records the semantic HTTP requests made to a descriptor endpoint. The
/// descriptor scan may also open a bare TCP liveness connection; empty reads
/// are intentionally ignored because they are not hook HTTP requests.
fn serve_recording_descriptor_endpoint(
    listener: TcpListener,
    enrichment_status: &str,
    enrichment_body: &str,
) -> Vec<String> {
    listener
        .set_nonblocking(true)
        .expect("set descriptor endpoint non-blocking");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut requests = Vec::new();

    while Instant::now() < deadline {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => panic!("descriptor endpoint accept failed: {error}"),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let request = read_http_request(&mut stream);
        if request.is_empty() {
            continue;
        }
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("")
            .to_string();
        let route = path.split('?').next().unwrap_or(&path);
        requests.push(path.clone());

        if route == "/health" {
            let health = r#"{"project_count":0,"session_count":1,"daemon_version":"10.0.3","executable_path":"mock","auth_required":true,"pid":0}"#;
            write_http_ok(&mut stream, health);
            continue;
        }

        write_http_response(&mut stream, enrichment_status, enrichment_body);
        return requests;
    }

    panic!("hook never sent enrichment to descriptor endpoint; requests={requests:?}")
}

/// Records one complete daemon rediscovery: root lookup, active-session lookup,
/// then the root-fenced enrichment request through `expected_session_id`.
fn serve_recording_daemon(
    listener: TcpListener,
    canonical_root: &str,
    project_id: &str,
    sessions_json: &str,
    expected_session_id: &str,
) -> Vec<String> {
    listener
        .set_nonblocking(true)
        .expect("set recording daemon non-blocking");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut requests = Vec::new();
    let expected_enrichment_route = format!("/v1/sessions/{expected_session_id}/sidecar/outline");

    while Instant::now() < deadline {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => panic!("recording daemon accept failed: {error}"),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let request = read_http_request(&mut stream);
        if request.is_empty() {
            continue;
        }
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("")
            .to_string();
        let route = path.split('?').next().unwrap_or(&path);
        requests.push(path.clone());

        if route == "/v1/projects" {
            let body = format!(
                r#"[{{"project_id":"{}","canonical_root":"{}","session_count":2}}]"#,
                project_id,
                canonical_root.replace('"', "\\\"")
            );
            write_http_ok(&mut stream, &body);
        } else if route == format!("/v1/projects/{project_id}/sessions") {
            write_http_ok(&mut stream, sessions_json);
        } else if route == expected_enrichment_route {
            let body = format!(r#"{{"enriched":"{ENRICHED_MARKER}"}}"#);
            write_http_ok(&mut stream, &body);
            return requests;
        } else {
            write_http_response(
                &mut stream,
                "404 Not Found",
                "unexpected recording-daemon route",
            );
            return requests;
        }
    }

    panic!("hook did not complete daemon rediscovery; requests={requests:?}")
}

/// Single-purpose mock daemon HTTP server for the fallback test. Serves each
/// `Connection: close` request the hook makes — `/v1/projects`, the project's
/// `/sessions` list, then the `/v1/sessions/{id}/sidecar/outline` enrichment —
/// routing by request-line path. Loops until the enrichment request is served
/// or the listener is dropped.
fn serve_mock_daemon(listener: TcpListener, canonical_root: &str, project_id: &str) {
    let body = format!("{{\"enriched\":\"{ENRICHED_MARKER}\"}}");
    serve_mock_daemon_with_enrichment(listener, canonical_root, project_id, "200 OK", &body);
}

fn serve_mock_daemon_with_enrichment(
    listener: TcpListener,
    canonical_root: &str,
    project_id: &str,
    enrichment_status: &str,
    enrichment_body: &str,
) {
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
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");
        let route = path.split('?').next().unwrap_or(path);

        let body: String = if route.starts_with("/v1/projects/") && route.contains("/sessions") {
            // Include a newer session whose active project no longer matches.
            // The hook must filter it out and route through `mock-session`.
            serde_json::json!([
                {"session_id":"wrong-session","project_id":"other-project","last_seen_at_unix_secs":2},
                {"session_id":"mock-session","project_id":project_id,"last_seen_at_unix_secs":1}
            ])
            .to_string()
        } else if route == "/v1/projects" {
            // Projects list — advertise our canonical root.
            format!(
                r#"[{{"project_id":"{}","canonical_root":"{}","session_count":1}}]"#,
                project_id,
                canonical_root.replace('"', "\\\"")
            )
        } else if route == "/v1/sessions/mock-session/sidecar/outline"
            && path.contains("caller_root=")
        {
            write_http_response(&mut stream, enrichment_status, enrichment_body);
            return;
        } else {
            write_http_response(&mut stream, "404 Not Found", "unexpected mock-daemon route");
            continue;
        };

        write_http_ok(&mut stream, &body);
    }
}

/// Record an initially discovered daemon's complete request sequence and keep
/// listening briefly after its first 503. An erroneous fallback retry then
/// reaches the same live mock and becomes an observable duplicate discovery.
fn serve_initially_discovered_loading_daemon(
    listener: TcpListener,
    canonical_root: &str,
    project_id: &str,
    enrichment_body: &str,
) -> Vec<String> {
    listener
        .set_nonblocking(true)
        .expect("set initially discovered daemon listener non-blocking");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut first_enrichment_at: Option<Instant> = None;
    let mut requests = Vec::new();

    while Instant::now() < deadline {
        if first_enrichment_at.is_some_and(|at| at.elapsed() >= Duration::from_millis(750)) {
            return requests;
        }
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => panic!("initially discovered daemon accept failed: {error}"),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let request = read_http_request(&mut stream);
        if request.is_empty() {
            continue;
        }
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("")
            .to_string();
        let route = path.split('?').next().unwrap_or(&path);
        requests.push(path.clone());

        if route == "/v1/projects" {
            let body = format!(
                r#"[{{"project_id":"{}","canonical_root":"{}","session_count":1}}]"#,
                project_id,
                canonical_root.replace('"', "\\\"")
            );
            write_http_ok(&mut stream, &body);
        } else if route == format!("/v1/projects/{project_id}/sessions") {
            let body = serde_json::json!([
                {"session_id":"mock-session","project_id":project_id,"last_seen_at_unix_secs":1}
            ])
            .to_string();
            write_http_ok(&mut stream, &body);
        } else if route == "/v1/sessions/mock-session/sidecar/outline"
            && path.contains("caller_root=")
        {
            first_enrichment_at.get_or_insert_with(Instant::now);
            write_http_response(&mut stream, "503 Service Unavailable", enrichment_body);
        } else {
            write_http_response(&mut stream, "404 Not Found", "unexpected mock-daemon route");
        }
    }

    panic!("hook never sent enrichment to the initially discovered daemon; requests={requests:?}")
}

/// Serve a daemon selected directly from a session descriptor and keep it
/// alive briefly after the first 503. If the hook incorrectly rediscovers
/// the same daemon, this mock serves the discovery endpoints and counts the
/// second enrichment request deterministically.
fn serve_descriptor_selected_loading_daemon(
    listener: TcpListener,
    canonical_root: &str,
    project_id: &str,
    enrichment_body: &str,
) -> usize {
    listener
        .set_nonblocking(true)
        .expect("set descriptor daemon listener non-blocking");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut first_enrichment_at: Option<Instant> = None;
    let mut enrichment_requests = 0usize;

    while Instant::now() < deadline {
        if first_enrichment_at.is_some_and(|at| at.elapsed() >= Duration::from_millis(750)) {
            return enrichment_requests;
        }
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => panic!("descriptor daemon accept failed: {error}"),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let request = read_http_request(&mut stream);
        if request.is_empty() {
            continue;
        }
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");
        let route = path.split('?').next().unwrap_or(path);

        if route == "/health" {
            let health = r#"{"project_count":0,"session_count":1,"daemon_version":"10.0.3","executable_path":"mock","auth_required":true,"pid":0}"#;
            write_http_ok(&mut stream, health);
        } else if route == "/v1/projects" {
            let body = format!(
                r#"[{{"project_id":"{}","canonical_root":"{}","session_count":1}}]"#,
                project_id,
                canonical_root.replace('"', "\\\"")
            );
            write_http_ok(&mut stream, &body);
        } else if route.starts_with("/v1/projects/") && route.contains("/sessions") {
            let body = serde_json::json!([
                {"session_id":"mock-session","project_id":project_id,"last_seen_at_unix_secs":1}
            ])
            .to_string();
            write_http_ok(&mut stream, &body);
        } else if route == "/v1/sessions/mock-session/sidecar/outline"
            && path.contains("caller_root=")
        {
            enrichment_requests += 1;
            first_enrichment_at.get_or_insert_with(Instant::now);
            write_http_response(&mut stream, "503 Service Unavailable", enrichment_body);
            if enrichment_requests > 1 {
                return enrichment_requests;
            }
        } else {
            write_http_response(&mut stream, "404 Not Found", "unexpected mock-daemon route");
        }
    }

    panic!("hook never sent an enrichment request to the descriptor-selected daemon")
}

/// Serve one real HTTP response while tolerating descriptor-liveness probes
/// that connect without sending a request. The non-blocking deadline keeps a
/// failed hook connection from stranding the test on `join()`.
fn serve_mock_http_response(listener: TcpListener, status: &str, body: &str) {
    listener
        .set_nonblocking(true)
        .expect("set mock sidecar listener non-blocking");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => panic!("mock sidecar accept failed: {error}"),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let request = read_http_request(&mut stream);
        if request.is_empty() {
            continue;
        }
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");
        if path.split('?').next().unwrap_or(path) == "/health" {
            let health = r#"{"project_count":0,"session_count":0,"daemon_version":"10.0.3","executable_path":"mock","auth_required":true,"pid":0}"#;
            write_http_ok(&mut stream, health);
            continue;
        }

        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write mock HTTP response");
        stream.flush().expect("flush mock HTTP response");
        return;
    }
    panic!("hook never sent an HTTP request to the mock sidecar before the deadline");
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    const MAX_REQUEST_BYTES: usize = 16 * 1024;
    let mut request = Vec::new();
    while request.len() < MAX_REQUEST_BYTES {
        let mut chunk = [0u8; 1024];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

/// Write a minimal `200 OK` HTTP response with the given body and close.
fn write_http_ok(stream: &mut std::net::TcpStream, body: &str) {
    write_http_response(stream, "200 OK", body);
}

fn write_http_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let home = TempDir::new().expect("control-state tempdir creation");
    run_hook_in_tempdir_with_env(
        cwd,
        payload,
        &[("SYMFORGE_HOME", home.path().to_string_lossy().as_ref())],
    )
    .1
}

/// Like `run_hook_in_tempdir` but allows injecting extra environment variables
/// and returns `(stdout, adoption_log_contents)` so callers can assert on the
/// enriched body emitted to stdout as well as the recorded outcome.
fn run_hook_in_tempdir_with_env(
    cwd: &Path,
    payload: &str,
    extra_env: &[(&str, &str)],
) -> (String, String) {
    let (stdout, log, _) = run_hook_in_tempdir_with_env_and_stderr(cwd, payload, extra_env);
    (stdout, log)
}

fn run_hook_in_tempdir_with_env_and_stderr(
    cwd: &Path,
    payload: &str,
    extra_env: &[(&str, &str)],
) -> (String, String, String) {
    let control_root = extra_env
        .iter()
        .rev()
        .find_map(|(key, value)| (*key == "SYMFORGE_HOME").then_some(Path::new(*value)))
        .expect("hook subprocess tests must inject an isolated SYMFORGE_HOME");
    let bin = env!("CARGO_BIN_EXE_symforge");
    let mut command = symforge::process_util::hidden_command(bin);
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
    let mut stderr = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }

    let log_path = control_root.join(ADOPTION_LOG_FILE);
    assert!(
        log_path.exists(),
        "run_hook must append to {ADOPTION_LOG_FILE} under the injected control-state root; \
         missing at {}. This usually means a record_hook_outcome* call was \
         removed from the run_hook dispatch branch being exercised.",
        log_path.display()
    );

    let log = std::fs::read_to_string(&log_path).expect("log readable");
    (stdout, log, stderr)
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
