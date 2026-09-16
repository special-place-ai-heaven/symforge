#![cfg(feature = "embed")]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::{
    EmbeddedSourceSpec, KnowledgeSearchRequest, OperationKind, ProcessIndexRuntime, RetryAdvice,
    SourceRefusalKind, SourceRuntimePhase, SymbolSearchRequest, TextSearchRequest,
};

fn wait_for_view(
    handle: &symforge::embed::EmbeddedSourceHandle,
    deadline: Instant,
    mut predicate: impl FnMut(&symforge::embed::SourceRuntimeView) -> bool,
) -> symforge::embed::SourceRuntimeView {
    loop {
        let view = handle.runtime_view();
        if predicate(&view) {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "embedded source did not settle: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn public_embed_handle_indexes_refreshes_queries_and_joins() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize git repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    fs::write(
        repository.path().join("src/lib.rs"),
        b"pub struct Alpha;\npub fn beta() -> u32 { 1 }\n",
    )
    .expect("write initial source");

    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open embedded source");

    let initial = wait_for_view(&handle, Instant::now() + Duration::from_secs(15), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    assert!(initial.source_version > 0);
    assert!(initial.current_publication_identity.is_some());
    assert!(initial.observer_epoch > 0);

    let symbols = handle
        .search_symbols(&SymbolSearchRequest {
            query: None,
            path_prefix: None,
            limit: u32::MAX,
        })
        .expect("query every symbol");
    assert!(!symbols.value().truncated);
    assert!(
        symbols
            .value()
            .matches
            .iter()
            .any(|item| item.name == "Alpha")
    );
    assert!(
        symbols
            .value()
            .matches
            .iter()
            .any(|item| item.name == "beta")
    );

    let text = handle
        .search_text(&TextSearchRequest {
            query: "Alpha".to_string(),
            path_prefix: Some("src".to_string()),
            limit: u32::MAX,
            case_sensitive: true,
        })
        .expect("search indexed text");
    assert!(!text.value().truncated);
    assert_eq!(text.value().matches.len(), 1);
    assert_eq!(text.value().matches[0].path, "src/lib.rs");

    let refresh_ticket = handle.request_refresh().expect("queue explicit refresh");
    assert_eq!(
        refresh_ticket.requested_source_version(),
        initial.source_version
    );
    let manually_refreshed =
        wait_for_view(&handle, Instant::now() + Duration::from_secs(15), |view| {
            view.phase == SourceRuntimePhase::Current
                && view.source_version > initial.source_version
        });

    fs::write(
        repository.path().join("src/added.rs"),
        b"pub fn gamma() -> u32 { 3 }\n",
    )
    .expect("write changed source");

    let mut saw_refreshing = false;
    let refreshed = wait_for_view(&handle, Instant::now() + Duration::from_secs(15), |view| {
        saw_refreshing |= view.phase == SourceRuntimePhase::Refreshing;
        view.phase == SourceRuntimePhase::Current
            && view.source_version > manually_refreshed.source_version
    });
    assert!(
        saw_refreshing,
        "the source never exposed its Refreshing phase"
    );
    assert_ne!(
        refreshed.current_publication_identity,
        initial.current_publication_identity
    );

    let refreshed_symbols = handle
        .search_symbols(&SymbolSearchRequest {
            query: None,
            path_prefix: None,
            limit: u32::MAX,
        })
        .expect("query refreshed symbols");
    assert!(!refreshed_symbols.value().truncated);
    assert!(
        refreshed_symbols
            .value()
            .matches
            .iter()
            .any(|item| item.name == "gamma")
    );

    std::thread::sleep(Duration::from_millis(450));
    assert_eq!(
        handle.runtime_view().source_version,
        refreshed.source_version
    );

    let close = handle.begin_close();
    let report = close
        .wait(Instant::now() + Duration::from_secs(5))
        .expect("wait for source close");
    assert!(!report.already_terminal);
    assert_eq!(report.terminal_source_version, refreshed.source_version);
    assert_eq!(handle.runtime_view().phase, SourceRuntimePhase::Stopped);

    let reopened = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("reopen after explicit close");
    wait_for_view(
        &reopened,
        Instant::now() + Duration::from_secs(15),
        |view| view.phase == SourceRuntimePhase::Current,
    );
    drop(runtime);
    assert_eq!(reopened.runtime_view().phase, SourceRuntimePhase::Stopped);
}

#[test]
fn drop_while_loading_returns_within_one_second() {
    use std::sync::mpsc;

    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize git repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    for index in 0..32 {
        fs::write(
            repository.path().join(format!("src/file_{index}.rs")),
            format!("pub fn function_{index}() -> usize {{ {index} }}\n"),
        )
        .expect("write source fixture");
    }

    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(repository.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open embedded source");
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "initial reload did not reach the root-scoped parse gate"
    );

    let (finished, received) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        let started = Instant::now();
        drop(handle);
        finished
            .send(started.elapsed())
            .expect("report close duration");
    });
    let elapsed = received
        .recv_timeout(Duration::from_secs(1))
        .expect("dropping while loading must not wait for the held reload");
    closer.join().expect("close thread completes");
    assert!(
        elapsed < Duration::from_secs(1),
        "close exceeded the one-second contract: {elapsed:?}"
    );
}

#[test]
fn unrelated_open_completes_while_another_root_closes() {
    use std::sync::mpsc;

    let closing = tempfile::tempdir().expect("closing repository");
    git2::Repository::init(closing.path()).expect("initialize git repository");
    fs::create_dir_all(closing.path().join("src")).expect("create source directory");
    for index in 0..32 {
        fs::write(
            closing.path().join(format!("src/file_{index}.rs")),
            format!("pub fn function_{index}() -> usize {{ {index} }}\n"),
        )
        .expect("write source fixture");
    }

    let opening = tempfile::tempdir().expect("unrelated repository");
    git2::Repository::init(opening.path()).expect("initialize git repository");
    fs::create_dir_all(opening.path().join("src")).expect("create source directory");
    fs::write(
        opening.path().join("src/lib.rs"),
        b"pub fn ready() -> u32 { 1 }\n",
    )
    .expect("write unrelated source");

    let gate = symforge::live_index::store::hold_reload_through_cancel_for_test(closing.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            closing.path().to_path_buf(),
        ))
        .expect("open closing source");
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "closing reload did not reach the parse gate"
    );

    let join = symforge::live_index::index_lifecycle::embedded::watch_join_entered_for_test(
        closing.path(),
    );
    let (close_done, close_rx) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        drop(handle);
        close_done.send(()).expect("report close");
    });
    assert!(
        join.wait_until_entered(Duration::from_secs(5)),
        "close did not reach worker.join() after dropping the factory mutex"
    );

    let runtime_for_open = runtime.clone();
    let unrelated_root = opening.path().to_path_buf();
    let (open_done, open_rx) = mpsc::channel();
    let opener = std::thread::spawn(move || {
        let started = Instant::now();
        let result = runtime_for_open
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(unrelated_root))
            .map(|_| ())
            .map_err(|error| error.to_string());
        open_done
            .send((started.elapsed(), result))
            .expect("report unrelated open");
    });

    let (elapsed, opened) = open_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("unrelated open must not queue behind the closing root's worker join");
    opener.join().expect("open thread completes");
    opened.unwrap_or_else(|error| panic!("unrelated open refused: {error}"));
    assert!(
        elapsed < Duration::from_secs(1),
        "unrelated open exceeded the one-second contract: {elapsed:?}"
    );

    drop(gate);
    close_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("closing root must finish after the hold releases");
    closer.join().expect("close thread completes");
}

fn rust_repo_with_files(file_count: usize) -> tempfile::TempDir {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize git repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    for index in 0..file_count {
        fs::write(
            repository.path().join(format!("src/file_{index}.rs")),
            format!("pub fn function_{index}() -> usize {{ {index} }}\n"),
        )
        .expect("write source fixture");
    }
    repository
}

fn open_current_worktree(
    runtime: &ProcessIndexRuntime,
    root: &std::path::Path,
) -> symforge::embed::EmbeddedSourceHandle {
    runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .expect("open embedded source")
}

/// Release the parse hold before any handle close so a failed assert cannot join a parked worker.
struct ThroughCancelHold {
    gate: Option<symforge::live_index::store::ReloadGateForTest>,
    handles: Vec<symforge::embed::EmbeddedSourceHandle>,
}

impl Drop for ThroughCancelHold {
    fn drop(&mut self) {
        self.gate.take();
        self.handles.clear();
    }
}

#[test]
fn reload_hold_on_one_root_leaves_other_roots_running() {
    let held = rust_repo_with_files(32);
    let other = rust_repo_with_files(4);
    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(held.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let mut session = ThroughCancelHold {
        gate: Some(gate),
        handles: vec![open_current_worktree(&runtime, held.path())],
    };
    assert!(
        session
            .gate
            .as_ref()
            .expect("reload gate")
            .wait_until_blocked(Duration::from_secs(5)),
        "held root did not reach the parse gate"
    );

    session
        .handles
        .push(open_current_worktree(&runtime, other.path()));
    let other_view = wait_for_view(
        &session.handles[1],
        Instant::now() + Duration::from_secs(3),
        |view| view.phase == SourceRuntimePhase::Current,
    );
    assert!(other_view.source_version > 0);
    assert_eq!(
        session.handles[0].runtime_view().phase,
        SourceRuntimePhase::Loading,
        "the held root must stay Loading while the other root reaches Current"
    );
}

#[test]
fn close_during_refresh_returns_before_gate_release() {
    use std::sync::mpsc;

    let repository = rust_repo_with_files(32);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(15), |view| {
        view.phase == SourceRuntimePhase::Current
    });

    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(repository.path(), 1);
    handle.request_refresh().expect("queue refresh");
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "refresh reload did not reach the parse gate"
    );

    let (finished, received) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        let started = Instant::now();
        handle.close().expect("close during refresh");
        finished
            .send(started.elapsed())
            .expect("report close duration");
    });
    let elapsed = received
        .recv_timeout(Duration::from_secs(1))
        .expect("closing during refresh must not wait for the held reload");
    closer.join().expect("close thread completes");
    assert!(
        elapsed < Duration::from_secs(1),
        "close exceeded the one-second contract: {elapsed:?}"
    );
}

#[test]
fn cancelled_first_load_publishes_nothing() {
    let repository = rust_repo_with_files(32);
    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(repository.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = open_current_worktree(&runtime, repository.path());
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "initial reload did not reach the parse gate"
    );

    handle.close().expect("close cancelled first load");
    let view = handle.runtime_view();
    assert_eq!(view.phase, SourceRuntimePhase::Stopped);
    assert_eq!(view.source_version, 0);
    assert!(view.current_publication_identity.is_none());
}

#[test]
fn cancelled_refresh_keeps_last_current_publication() {
    let repository = rust_repo_with_files(32);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = open_current_worktree(&runtime, repository.path());
    let current = wait_for_view(&handle, Instant::now() + Duration::from_secs(15), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    assert!(current.source_version > 0);
    let identity = current
        .current_publication_identity
        .clone()
        .expect("Current source has a publication identity");

    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(repository.path(), 1);
    handle.request_refresh().expect("queue refresh");
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "refresh reload did not reach the parse gate"
    );

    handle.close().expect("close cancelled refresh");
    let view = handle.runtime_view();
    assert_eq!(view.phase, SourceRuntimePhase::Stopped);
    assert_eq!(view.source_version, current.source_version);
    assert_eq!(
        view.current_publication_identity.as_deref(),
        Some(identity.as_str())
    );
}

#[test]
fn stopping_is_never_overwritten_by_blocked_or_current() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    let repository = rust_repo_with_files(32);
    let gate =
        symforge::live_index::store::hold_reload_through_cancel_for_test(repository.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = open_current_worktree(&runtime, repository.path());
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "initial reload did not reach the parse gate"
    );

    let phases = Arc::new(Mutex::new(Vec::new()));
    let stop_poll = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop_poll.load(Ordering::Acquire) {
                phases
                    .lock()
                    .expect("phase log")
                    .push(handle.runtime_view().phase);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let closer = scope.spawn(|| handle.close().expect("close while Stopping is observable"));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let phase = handle.runtime_view().phase;
            assert_ne!(
                phase,
                SourceRuntimePhase::Stopped,
                "close reached Stopped before Stopping was observed"
            );
            if phase == SourceRuntimePhase::Stopping {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "close never reached Stopping: {phase:?}"
            );
        }
        gate.release();
        closer.join().expect("close thread completes");
        stop_poll.store(true, Ordering::Release);
    });

    let recorded = phases.lock().expect("phase log");
    let mut saw_stopping = false;
    for phase in recorded.iter() {
        if *phase == SourceRuntimePhase::Stopping {
            saw_stopping = true;
        }
        if saw_stopping {
            assert!(
                *phase != SourceRuntimePhase::Blocked && *phase != SourceRuntimePhase::Current,
                "Stopping was overwritten by {phase:?}"
            );
        }
    }
    assert!(saw_stopping, "close never exposed Stopping");
    let view = handle.runtime_view();
    assert_eq!(view.phase, SourceRuntimePhase::Stopped);
    assert_eq!(view.source_version, 0);
    assert!(view.current_publication_identity.is_none());
}

#[test]
fn close_during_scout_returns_before_scout_completes() {
    use std::sync::mpsc;

    let repository = rust_repo_with_files(32);
    let gate = symforge::discovery::hold_scout_after_files_for_test(repository.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = open_current_worktree(&runtime, repository.path());
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "scout walk did not reach the hold"
    );

    let (finished, received) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        let started = Instant::now();
        drop(handle);
        finished
            .send(started.elapsed())
            .expect("report close duration");
    });
    let elapsed = received
        .recv_timeout(Duration::from_secs(1))
        .expect("closing during scout must not wait for the rest of the walk");
    closer.join().expect("close thread completes");
    assert!(
        elapsed < Duration::from_secs(1),
        "close exceeded the one-second contract: {elapsed:?}"
    );
    assert!(
        gate.files_seen() < 32,
        "scout completed the whole walk before close returned: seen={}",
        gate.files_seen()
    );
}

#[test]
fn close_during_derived_stage_returns_before_release() {
    use std::sync::mpsc;

    let repository = rust_repo_with_files(4);
    let _hold = symforge::live_index::store::hold_derived_stage_for_test();
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(5), |view| {
        view.phase == SourceRuntimePhase::Loading
    });
    let (finished, received) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        let started = Instant::now();
        handle.close().expect("close during derived-index hold");
        finished
            .send(started.elapsed())
            .expect("report close duration");
    });
    let elapsed = received
        .recv_timeout(Duration::from_secs(1))
        .expect("closing during a derived-index hold must not wait for the rebuild");
    closer.join().expect("close thread completes");
    assert!(
        elapsed < Duration::from_secs(1),
        "close exceeded the one-second contract: {elapsed:?}"
    );
}

#[test]
fn close_after_cancelled_reload_does_not_wait_a_poll() {
    use std::sync::mpsc;

    let repository = rust_repo_with_files(32);
    let gate =
        symforge::live_index::store::hold_reload_through_cancel_for_test(repository.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let handle = open_current_worktree(&runtime, repository.path());
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "initial reload did not reach the parse gate"
    );

    let join = symforge::live_index::index_lifecycle::embedded::watch_join_entered_for_test(
        repository.path(),
    );
    let (finished, received) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        handle.close().expect("close after cancelled reload");
        finished.send(()).expect("report close");
    });
    assert!(
        join.wait_until_entered(Duration::from_secs(5)),
        "close did not enter join while the reload was held"
    );
    let released = Instant::now();
    drop(gate);
    received
        .recv_timeout(Duration::from_secs(1))
        .expect("close must return after the cancelled reload without a poll sleep");
    closer.join().expect("close thread completes");
    let elapsed = released.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "close slept a poll after cancel: {elapsed:?}"
    );
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    fs::create_dir_all(to).expect("create copy destination");
    for entry in fs::read_dir(from).expect("read copy source") {
        let entry = entry.expect("copy source entry");
        let name = entry.file_name();
        if matches!(
            name.to_str(),
            Some("target" | ".git" | ".symforge" | "node_modules" | ".worktrees")
        ) {
            continue;
        }
        let from_path = entry.path();
        let to_path = to.join(&name);
        if from_path.is_dir() {
            copy_tree(&from_path, &to_path);
        } else {
            let _ = fs::copy(&from_path, &to_path);
        }
    }
}

fn write_extra_parse_files(root: &std::path::Path, file_count: usize) {
    let extra = root.join("extra_payload");
    fs::create_dir_all(&extra).expect("create extra payload directory");
    for index in 0..file_count {
        let mut body = String::from("pub struct ExtraPayload;\n");
        for function in 0..80 {
            body.push_str(&format!(
                "pub fn extra_{index}_{function}() -> usize {{ {index} + {function} }}\n"
            ));
        }
        fs::write(extra.join(format!("file_{index}.rs")), body).expect("write extra payload");
    }
}

fn measure_full_reload(runtime: &ProcessIndexRuntime, root: &std::path::Path) -> Duration {
    let handle = open_current_worktree(runtime, root);
    let started = Instant::now();
    wait_for_view(&handle, Instant::now() + Duration::from_secs(600), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let elapsed = started.elapsed();
    handle.close().expect("close full-reload measurement");
    elapsed
}

fn enlarge_fixture_until_reload_floor(
    runtime: &ProcessIndexRuntime,
    checkout: &std::path::Path,
    floor: Duration,
) -> (std::path::PathBuf, tempfile::TempDir, Duration) {
    // Never open CARGO_MANIFEST_DIR: embed create_dir_all would write .symforge
    // into the live checkout.
    let fixture = tempfile::tempdir().expect("G16 fixture");
    git2::Repository::init(fixture.path()).expect("initialize G16 fixture");
    copy_tree(checkout, fixture.path());
    let mut extra_files = 0usize;
    loop {
        if extra_files > 0 {
            write_extra_parse_files(fixture.path(), extra_files);
        }
        let reload = measure_full_reload(runtime, fixture.path());
        if reload >= floor {
            return (fixture.path().to_path_buf(), fixture, reload);
        }
        extra_files = if extra_files == 0 {
            250
        } else {
            extra_files
                .checked_mul(2)
                .expect("extra payload count overflow")
        };
        assert!(
            extra_files <= 8_000,
            "could not enlarge this checkout to a {floor:?} debug reload (last={reload:?})"
        );
    }
}

#[test]
#[ignore]
fn close_latency_bound_on_this_checkout() {
    use std::sync::mpsc;

    let checkout = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime = ProcessIndexRuntime::acquire().expect("acquire embedded runtime");
    let floor = Duration::from_secs(10);
    let (fixture, _keep, full_reload) =
        enlarge_fixture_until_reload_floor(&runtime, &checkout, floor);
    assert!(
        full_reload >= floor,
        "full reload must stay at or above ten seconds: {full_reload:?}"
    );

    let unrelated = rust_repo_with_files(1);
    let mut close_durations = Vec::new();
    let mut unrelated_durations = Vec::new();
    for attempt in 0..40 {
        if close_durations.len() >= 20 {
            break;
        }
        let handle = open_current_worktree(&runtime, &fixture);
        let spread = Duration::from_millis((attempt as u64).saturating_mul(20));
        let wait_until = Instant::now() + spread;
        while Instant::now() < wait_until
            && handle.runtime_view().phase == SourceRuntimePhase::Loading
        {
            std::thread::sleep(Duration::from_millis(1));
        }
        if handle.runtime_view().phase != SourceRuntimePhase::Loading {
            handle.close().expect("close trial that left Loading");
            continue;
        }

        let join =
            symforge::live_index::index_lifecycle::embedded::watch_join_entered_for_test(&fixture);
        let (finished, received) = mpsc::channel();
        let closer = std::thread::spawn(move || {
            let started = Instant::now();
            drop(handle);
            finished
                .send(started.elapsed())
                .expect("report close duration");
        });
        assert!(
            join.wait_until_entered(Duration::from_secs(5)),
            "close did not enter join during Loading"
        );
        let open_started = Instant::now();
        let opened = open_current_worktree(&runtime, unrelated.path());
        let unrelated_elapsed = open_started.elapsed();
        opened.close().expect("close unrelated root");
        let close_elapsed = received
            .recv_timeout(Duration::from_secs(5))
            .expect("timed close must return");
        closer.join().expect("close thread completes");
        assert!(
            close_elapsed <= Duration::from_secs(1),
            "close exceeded one second during Loading: {close_elapsed:?}"
        );
        assert!(
            unrelated_elapsed <= Duration::from_secs(1),
            "unrelated open exceeded one second: {unrelated_elapsed:?}"
        );
        close_durations.push(close_elapsed);
        unrelated_durations.push(unrelated_elapsed);
    }

    assert!(
        close_durations.len() >= 20,
        "only {} closes landed during Loading",
        close_durations.len()
    );
    let mut close_ms: Vec<u128> = close_durations.iter().map(Duration::as_millis).collect();
    close_ms.sort_unstable();
    let max_ms = *close_ms.last().expect("close samples");
    let median_ms = close_ms[close_ms.len() / 2];
    let unrelated_open_max_ms = unrelated_durations
        .iter()
        .map(Duration::as_millis)
        .max()
        .expect("unrelated open samples");
    println!(
        "CLOSE-LATENCY bound_ms=1000 closes={} max_ms={max_ms} median_ms={median_ms} unrelated_open_max_ms={unrelated_open_max_ms} full_reload_ms={}",
        close_ms.len(),
        full_reload.as_millis()
    );
}

fn census_repo() -> tempfile::TempDir {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize git repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    fs::write(
        repository.path().join("src/zeta.rs"),
        b"pub fn zeta() -> u32 { 1 }\n",
    )
    .expect("write zeta");
    fs::write(repository.path().join("src/empty.rs"), b"// no symbols\n")
        .expect("write empty parsed file");
    fs::write(
        repository.path().join("src/alpha.rs"),
        b"pub struct Alpha;\npub fn beta() -> u32 { 2 }\n",
    )
    .expect("write alpha");
    repository
}

#[test]
fn index_census_lists_sorted_zero_symbol_parsed_files() {
    let repository = census_repo();
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let census = handle.index_census().expect("census current source");
    let paths: Vec<&str> = census
        .value()
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert!(
        paths.windows(2).all(|pair| pair[0] <= pair[1]),
        "census files must be sorted by path: {paths:?}"
    );
    let empty = census
        .value()
        .files
        .iter()
        .find(|file| file.path == "src/empty.rs")
        .expect("zero-symbol parsed file must appear");
    assert_eq!(empty.symbol_count, 0);
    assert!(census.value().total_files >= 3);
}

#[test]
fn index_census_path_counts_match_the_captured_publication() {
    let repository = census_repo();
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let census = handle.index_census().expect("census");
    let symbols = handle
        .search_symbols(&SymbolSearchRequest {
            query: None,
            path_prefix: None,
            limit: u32::MAX,
        })
        .expect("symbols from the same publication");
    for file in &census.value().files {
        let counted = symbols
            .value()
            .matches
            .iter()
            .filter(|item| item.path == file.path)
            .count();
        assert_eq!(
            counted, file.symbol_count as usize,
            "{} census count must match captured symbol matches",
            file.path
        );
    }
    assert_eq!(
        census.value().total_symbols,
        census
            .value()
            .files
            .iter()
            .map(|file| u64::from(file.symbol_count))
            .sum::<u64>()
    );
}

#[test]
fn index_census_reads_one_generation_across_a_publish() {
    let repository = census_repo();
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let before = handle.index_census().expect("baseline census");
    let empty_before = before
        .value()
        .files
        .iter()
        .find(|file| file.path == "src/empty.rs")
        .expect("empty file")
        .symbol_count;
    assert_eq!(empty_before, 0);
    let initial_version = handle.runtime_view().source_version;

    let gate =
        symforge::live_index::store::hold_census_after_first_read_for_test(repository.path());
    std::thread::scope(|scope| {
        let census_thread =
            scope.spawn(|| handle.index_census().expect("held census").value().clone());
        assert!(
            gate.wait_until_blocked(Duration::from_secs(5)),
            "census did not reach the first-read hold"
        );
        fs::write(
            repository.path().join("src/empty.rs"),
            b"pub fn now_a_symbol() -> u32 { 0 }\n",
        )
        .expect("publish a new symbol on the held path");
        handle.request_refresh().expect("queue publish during hold");
        wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
            view.phase == SourceRuntimePhase::Current && view.source_version > initial_version
        });
        drop(gate);
        let held = census_thread.join().expect("census thread");
        let empty_held = held
            .files
            .iter()
            .find(|file| file.path == "src/empty.rs")
            .expect("empty file still present")
            .symbol_count;
        assert_eq!(
            empty_held, 0,
            "census must keep the generation captured before the publish"
        );
    });
}

#[test]
fn index_census_refuses_before_current() {
    let repository = rust_repo_with_files(8);
    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(repository.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "reload did not reach the parse hold"
    );
    let refusal = handle
        .index_census()
        .expect_err("census before Current must refuse");
    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);
    assert_eq!(refusal.retry(), RetryAdvice::OnEvent);
    assert_eq!(
        refusal.operation().operation_kind(),
        OperationKind::IndexCensus
    );
    drop(gate);
}

#[test]
fn index_census_refuses_after_admission_revocation() {
    let repository = census_repo();
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    handle.revoke_admission_for_test();
    let refusal = handle
        .index_census()
        .expect_err("revoked admission must refuse census");
    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);
}

#[test]
fn index_progress_rises_only_during_a_reload() {
    let repository = rust_repo_with_files(1);
    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(repository.path(), 1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "reload did not reach parse 1"
    );
    let held = handle.index_progress();
    assert_eq!(held.files_parsed, 1);
    assert!(held.files_discovered >= 1);
    drop(gate);
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let current = handle.index_progress();
    assert!(current.files_parsed >= 1);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        handle.index_progress(),
        current,
        "progress must not move once Current"
    );
}

#[test]
fn index_progress_resets_before_each_reload() {
    let repository = rust_repo_with_files(6);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let first = handle.index_progress();
    assert!(first.files_parsed >= 6);
    let gate = symforge::live_index::store::hold_reload_after_parses_for_test(repository.path(), 1);
    fs::write(
        repository.path().join("src/file_extra.rs"),
        b"pub fn extra() -> u32 { 9 }\n",
    )
    .expect("force a second reload");
    handle.request_refresh().expect("refresh");
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "second reload did not hold"
    );
    let second = handle.index_progress();
    assert!(
        second.files_parsed < first.files_parsed && second.files_parsed >= 1,
        "a new reload must reset parsed before it rises again, got {second:?} after first {first:?}"
    );
    drop(gate);
}

#[test]
fn index_progress_keeps_last_values_after_cancel() {
    let repository = rust_repo_with_files(1);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let held = ThroughCancelHold {
        gate: Some(
            symforge::live_index::store::hold_reload_through_cancel_for_test(repository.path(), 1),
        ),
        handles: vec![open_current_worktree(&runtime, repository.path())],
    };
    assert!(
        held.gate
            .as_ref()
            .expect("gate")
            .wait_until_blocked(Duration::from_secs(5)),
        "reload did not reach parse 1"
    );
    let progress = held.handles[0].index_progress();
    assert_eq!(progress.files_parsed, 1);
    held.handles[0].cancel_reload_for_test();
    assert_eq!(
        held.handles[0].index_progress(),
        progress,
        "cancel must keep the last observed progress values"
    );
}

#[test]
fn idle_tick_scout_leaves_progress_untouched() {
    let repository = rust_repo_with_files(3);
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let baseline = handle.index_progress();
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        handle.index_progress(),
        baseline,
        "idle tick scouts must not move progress"
    );
}

#[test]
fn embed_path_prefix_src_excludes_srcx() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize git repository");
    fs::create_dir_all(repository.path().join("src")).expect("src");
    fs::create_dir_all(repository.path().join("srcx")).expect("srcx");
    fs::write(
        repository.path().join("src/lib.rs"),
        b"pub fn inside() {}\n",
    )
    .expect("src file");
    fs::write(
        repository.path().join("srcx/lib.rs"),
        b"pub fn outside() {}\n",
    )
    .expect("srcx file");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let symbols = handle
        .search_symbols(&SymbolSearchRequest {
            query: None,
            path_prefix: Some("src/".to_string()),
            limit: u32::MAX,
        })
        .expect("prefix symbols");
    assert!(
        symbols
            .value()
            .matches
            .iter()
            .any(|item| item.path == "src/lib.rs" && item.name == "inside")
    );
    assert!(
        symbols
            .value()
            .matches
            .iter()
            .all(|item| !item.path.starts_with("srcx/")),
        "src/ must not match srcx/: {:?}",
        symbols.value().matches
    );
    let text = handle
        .search_text(&TextSearchRequest {
            query: "fn".to_string(),
            path_prefix: Some("src/".to_string()),
            limit: u32::MAX,
            case_sensitive: true,
        })
        .expect("prefix text");
    assert!(
        text.value()
            .matches
            .iter()
            .all(|item| !item.path.starts_with("srcx/")),
        "src/ text must not match srcx/: {:?}",
        text.value().matches
    );
}

fn knowledge_repo() -> tempfile::TempDir {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize git repository");
    fs::create_dir_all(repository.path().join("src")).expect("src");
    fs::write(
        repository.path().join("src/lib.rs"),
        b"pub struct NeedleType;\n",
    )
    .expect("code");
    fs::write(
        repository.path().join("guide.md"),
        b"# NeedleType\n\n`NeedleType` is the documented type. See [lib](src/lib.rs).\n",
    )
    .expect("markdown");
    fs::write(
        repository.path().join("notes.txt"),
        b"NeedleType appears in plain text notes.\n",
    )
    .expect("text");
    fs::write(
        repository.path().join("config.toml"),
        b"name = \"NeedleType\"\n",
    )
    .expect("config");
    repository
}

#[test]
fn embedded_search_knowledge_returns_markdown_text_and_config_with_provenance() {
    let repository = knowledge_repo();
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let result = handle
        .search_knowledge(&KnowledgeSearchRequest {
            query: "NeedleType".to_string(),
            path_prefix: None,
            limit: 32,
        })
        .expect("embedded knowledge search");
    let paths: Vec<&str> = result
        .value()
        .matches
        .iter()
        .map(|item| item.path.as_str())
        .collect();
    assert!(
        paths.iter().any(|path| path.ends_with("guide.md")),
        "markdown hit missing: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with("notes.txt")),
        "text hit missing: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with("config.toml")),
        "config hit missing: {paths:?}"
    );
    for item in &result.value().matches {
        assert!(
            !item.content_hash.is_empty(),
            "{} missing content hash",
            item.path
        );
        assert!(
            !item.provenance_ids.is_empty(),
            "{} missing provenance ids",
            item.path
        );
    }
}

#[test]
fn embedded_search_knowledge_reports_relationships_authority_and_coverage() {
    let repository = knowledge_repo();
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let result = handle
        .search_knowledge(&KnowledgeSearchRequest {
            query: "NeedleType".to_string(),
            path_prefix: None,
            limit: 32,
        })
        .expect("embedded knowledge search");
    let markdown = result
        .value()
        .matches
        .iter()
        .find(|item| item.path.ends_with("guide.md"))
        .expect("markdown hit");
    assert!(
        !markdown.relationship_evidence.is_empty(),
        "document that names code must carry relationship evidence"
    );
    assert!(!markdown.authority.is_empty());
    assert!(!markdown.coverage.is_empty());
}

#[test]
fn embedded_search_knowledge_reports_withheld_evidence() {
    let repository = knowledge_repo();
    fs::write(
        repository.path().join(".env"),
        b"SECRET_KEY=NeedleType-should-be-withheld\n",
    )
    .expect("policy-withheld knowledge file");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire");
    let handle = open_current_worktree(&runtime, repository.path());
    wait_for_view(&handle, Instant::now() + Duration::from_secs(20), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    let result = handle
        .search_knowledge(&KnowledgeSearchRequest {
            query: "NeedleType".to_string(),
            path_prefix: None,
            limit: 32,
        })
        .expect("embedded knowledge search");
    assert!(
        result.value().withheld_count > 0,
        "withheld files in scope must be reported"
    );
    assert!(
        result
            .value()
            .withheld_reasons
            .iter()
            .all(|reason| reason == "policy_withheld"),
        "reasons must stay neutral: {:?}",
        result.value().withheld_reasons
    );
}
