#![cfg(feature = "embed")]

use std::fs;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use symforge::embed::parity::host::OperationControl;
use symforge::embed::parity::search::SymbolSearchRequest;
use symforge::embed::parity::{
    QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest, QuerySymbolSelector,
};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRefusalKind,
    SourceRuntimePhase, SourceRuntimeView,
};

fn fixture(extra_files: usize) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    let src = root.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("lib.rs"),
        "pub fn target() -> u32 { 1 }\npub fn caller() -> u32 { target() }\n",
    )
    .unwrap();
    for index in 0..extra_files {
        fs::write(
            src.join(format!("part_{index}.rs")),
            format!("pub fn indexed_{index}() -> u32 {{ {index} }}\n"),
        )
        .unwrap();
    }
    root
}

fn wait_for_view(
    handle: &EmbeddedSourceHandle,
    timeout: Duration,
    mut predicate: impl FnMut(&SourceRuntimeView) -> bool,
) -> SourceRuntimeView {
    let deadline = Instant::now() + timeout;
    loop {
        let since = handle.change_version();
        let view = handle.runtime_view();
        if predicate(&view) {
            return view;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "source did not reach the expected view: {view:?}"
        );
        let changed = handle.wait_for_change(since, remaining).unwrap();
        if predicate(&changed) {
            return changed;
        }
    }
}

fn open(runtime: &ProcessIndexRuntime, path: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(path.to_path_buf()))
        .unwrap();
    wait_for_view(&handle, Duration::from_secs(30), |view| {
        view.phase == SourceRuntimePhase::Current
    });
    handle
}

fn wait_for_current_after(
    handle: &EmbeddedSourceHandle,
    after_version: u64,
    timeout: Duration,
    ready_to_wait: mpsc::SyncSender<()>,
) -> SourceRuntimeView {
    let deadline = Instant::now() + timeout;
    let mut ready_to_wait = Some(ready_to_wait);
    loop {
        let since = handle.change_version();
        let view = handle.runtime_view();
        if view.phase == SourceRuntimePhase::Current && view.source_version > after_version {
            return view;
        }
        if let Some(ready) = ready_to_wait.take() {
            ready.send(()).unwrap();
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "source did not publish after refresh: {view:?}"
        );
        let changed = handle.wait_for_change(since, remaining).unwrap();
        if changed.phase == SourceRuntimePhase::Current && changed.source_version > after_version {
            return changed;
        }
    }
}

fn graph(offset: u64) -> QueryRequest {
    QueryRequest::Graph {
        path_prefix: None,
        offset,
        expected_publication: None,
    }
}

#[test]
fn refresh_opt_in_serves_all_aap_snapshot_queries_from_the_last_publication() {
    let root = fixture(1);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let initial = handle.runtime_view();
    let capture_publication = initial
        .current_publication_identity
        .as_deref()
        .expect("current publication")
        .to_owned();
    let before_refresh = handle.query(&graph(0), QueryLimits::default()).unwrap();
    let publication = before_refresh.publication_identity().to_owned();
    assert_eq!(
        before_refresh.source_capture_publication_identity(),
        capture_publication
    );

    let gate = symforge::live_index::store::hold_reload_after_discovery_for_test(root.path());
    fs::write(
        root.path().join("src/fresh.rs"),
        "pub fn fresh_after_refresh() -> u32 { 7 }\n",
    )
    .unwrap();
    handle.request_refresh().unwrap();
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "reload did not reach its deterministic hold"
    );
    let refreshing = handle.runtime_view();
    assert_eq!(refreshing.phase, SourceRuntimePhase::Refreshing);
    assert_eq!(
        refreshing.current_publication_identity.as_deref(),
        Some(capture_publication.as_str())
    );

    let requests = [
        (graph(0), "graph"),
        (
            QueryRequest::File {
                path: "src/lib.rs".into(),
                start_line: None,
                end_line: None,
            },
            "file",
        ),
        (
            QueryRequest::SymbolSearch(SymbolSearchRequest {
                query: Some("target".into()),
                path_prefix: Some("src".into()),
                include_tests: Some(false),
                ..SymbolSearchRequest::default()
            }),
            "symbol search",
        ),
        (
            QueryRequest::References {
                name: "target".into(),
                target: None,
                kind: None,
                include_tests: false,
            },
            "references",
        ),
    ];

    for (request, label) in requests {
        let claim = handle
            .query_allowing_refresh(&request, QueryLimits::default())
            .unwrap_or_else(|error| panic!("{label} refresh query refused: {error}"));
        assert_eq!(claim.publication_identity(), publication, "{label}");
        assert_eq!(
            claim.source_capture_publication_identity(),
            capture_publication,
            "{label}"
        );
        assert_eq!(
            claim.source_phase(),
            SourceRuntimePhase::Refreshing,
            "{label}"
        );
        assert!(
            claim.is_refreshing(),
            "{label} did not disclose refresh state"
        );
        match (label, claim.value()) {
            ("graph", QueryOutput::Graph(_))
            | ("file", QueryOutput::File { .. })
            | ("symbol search", QueryOutput::SymbolSearch(_))
            | ("references", QueryOutput::References(_)) => {}
            (_, output) => panic!("{label} returned the wrong output: {output:?}"),
        }
    }

    let control = OperationControl::new(Duration::from_secs(5)).unwrap();
    let controlled = handle
        .query_allowing_refresh_with_control(&graph(0), QueryLimits::default(), &control)
        .unwrap();
    assert_eq!(controlled.publication_identity(), publication);
    assert_eq!(
        controlled.source_capture_publication_identity(),
        capture_publication
    );
    assert!(controlled.is_refreshing());

    let cancelled = OperationControl::new(Duration::from_secs(5)).unwrap();
    cancelled.cancel();
    assert_eq!(
        handle
            .query_allowing_refresh_with_control(&graph(0), QueryLimits::default(), &cancelled)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::Cancelled
    );

    assert_eq!(
        handle
            .query(&graph(0), QueryLimits::default())
            .unwrap_err()
            .kind(),
        QueryRefusalKind::SourceUnavailable
    );
    let mixed = QueryRequest::Diff {
        path: "src/lib.rs".into(),
        from_ref: "HEAD".into(),
        to_ref: None,
    };
    assert_eq!(
        handle
            .query_allowing_refresh(&mixed, QueryLimits::default())
            .unwrap_err()
            .kind(),
        QueryRefusalKind::UnsupportedOption
    );
    let after_refusal = handle.runtime_view();
    assert_eq!(after_refusal.phase, SourceRuntimePhase::Refreshing);
    assert_eq!(
        after_refusal.current_publication_identity.as_deref(),
        Some(capture_publication.as_str())
    );

    drop(gate);
    let settled = wait_for_view(&handle, Duration::from_secs(30), |view| {
        view.phase == SourceRuntimePhase::Current && view.source_version > initial.source_version
    });
    assert_ne!(
        settled.current_publication_identity.as_deref(),
        Some(capture_publication.as_str())
    );
}

#[test]
fn source_change_waits_cover_timeout_late_waiter_and_publication() {
    let root = fixture(8);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = Arc::new(open(&runtime, root.path()));
    let initial = handle.runtime_view();

    let timeout = Duration::from_millis(100);
    let started = Instant::now();
    let unchanged = handle
        .wait_for_change(handle.change_version(), timeout)
        .unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed >= timeout.saturating_sub(Duration::from_millis(20)),
        "unchanged wait returned too early: {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_secs(2),
        "unchanged wait exceeded its bounded timeout: {elapsed:?}"
    );
    assert_eq!(unchanged.phase, initial.phase);
    assert_eq!(unchanged.source_version, initial.source_version);
    assert_eq!(
        unchanged.current_publication_identity,
        initial.current_publication_identity
    );

    let since = handle.change_version();
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let waiter_handle = Arc::clone(&handle);
    let after_version = initial.source_version;
    let waiter = thread::spawn(move || {
        wait_for_current_after(
            &waiter_handle,
            after_version,
            Duration::from_secs(30),
            ready_tx,
        )
    });
    // The waiter captured its token and non-current view before this refresh starts.
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    fs::write(root.path().join("src/late.rs"), "pub fn late_change() {}\n").unwrap();
    handle.request_refresh().unwrap();
    // This call starts after the refresh event but uses the earlier token.
    let phase_change = handle
        .wait_for_change(since, Duration::from_secs(10))
        .unwrap();
    assert!(
        phase_change.phase != initial.phase || phase_change.source_version > initial.source_version,
        "wait missed the refresh transition: {phase_change:?}"
    );

    let published = waiter.join().unwrap();
    assert_eq!(published.phase, SourceRuntimePhase::Current);
    assert!(published.source_version > initial.source_version);
}

#[test]
fn closing_a_source_wakes_a_waiter() {
    let root = fixture(1);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = Arc::new(open(&runtime, root.path()));
    let since = handle.change_version();
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let waiter_handle = Arc::clone(&handle);
    let waiter = thread::spawn(move || {
        ready_tx.send(()).unwrap();
        let result = waiter_handle.wait_for_change(since, Duration::from_secs(30));
        result_tx.send(result).unwrap();
    });

    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    handle.begin_close();
    let view = result_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("closing the source did not wake the change waiter")
        .unwrap();
    waiter.join().unwrap();
    assert!(matches!(
        view.phase,
        SourceRuntimePhase::Stopping | SourceRuntimePhase::Stopped
    ));
}

#[test]
fn graph_publication_lease_keeps_pages_on_the_original_content() {
    let root = fixture(8);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let initial = handle.runtime_view();
    let lease = handle
        .begin_graph_publication_lease(Duration::from_secs(20))
        .unwrap();
    let limits = QueryLimits {
        max_results: 2,
        max_bytes: 16 * 1024,
    };
    let first = handle
        .query_with_graph_lease(&graph(0), limits, &lease)
        .unwrap();
    let publication = first.publication_identity().to_owned();
    let QueryOutput::Graph(first_graph) = first.value() else {
        panic!("graph lease returned a non-graph result")
    };
    let mut offset = first_graph.next_offset.expect("first page must continue");
    let control = OperationControl::new(Duration::from_secs(10)).unwrap();
    let controlled = handle
        .query_with_graph_lease_and_control(&graph(0), limits, &lease, &control)
        .unwrap();
    assert_eq!(controlled.publication_identity(), publication);

    fs::write(
        root.path().join("src/fresh.rs"),
        "pub fn fresh_after_refresh() -> u32 { 7 }\n",
    )
    .unwrap();
    handle.request_refresh().unwrap();
    wait_for_view(&handle, Duration::from_secs(30), |view| {
        view.phase == SourceRuntimePhase::Current && view.source_version > initial.source_version
    });

    loop {
        let request = QueryRequest::Graph {
            path_prefix: None,
            offset,
            expected_publication: Some(publication.clone()),
        };
        let page = handle
            .query_with_graph_lease(&request, limits, &lease)
            .unwrap();
        assert_eq!(page.publication_identity(), publication);
        let QueryOutput::Graph(page_graph) = page.value() else {
            panic!("graph lease returned a non-graph result")
        };
        assert!(
            !page_graph
                .files
                .iter()
                .any(|file| file.path == "src/fresh.rs")
        );
        assert!(
            !page_graph
                .symbols
                .iter()
                .any(|symbol| symbol.name == "fresh_after_refresh")
        );
        let Some(next) = page_graph.next_offset else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }

    let fresh = handle.query(&graph(0), QueryLimits::default()).unwrap();
    let QueryOutput::Graph(fresh_graph) = fresh.value() else {
        panic!("unleased graph returned a non-graph result")
    };
    assert_ne!(fresh.publication_identity(), publication);
    assert!(
        fresh_graph
            .files
            .iter()
            .any(|file| file.path == "src/fresh.rs")
    );
    assert!(
        fresh_graph
            .symbols
            .iter()
            .any(|symbol| symbol.name == "fresh_after_refresh")
    );
}

#[test]
fn graph_leases_refuse_expiry_foreign_sources_close_and_non_graph_queries() {
    let first_root = fixture(4);
    let second_root = fixture(4);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let first = open(&runtime, first_root.path());
    let second = open(&runtime, second_root.path());

    let expired = first.begin_graph_publication_lease(Duration::ZERO).unwrap();
    assert_eq!(
        first
            .query_with_graph_lease(&graph(0), QueryLimits::default(), &expired)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::GraphPublicationLeaseExpired
    );

    let lease = first
        .begin_graph_publication_lease(Duration::from_secs(20))
        .unwrap();
    let symbol_request = QueryRequest::Symbol {
        selector: QuerySymbolSelector {
            path: "src/lib.rs".into(),
            name: "target".into(),
            kind: Some("function".into()),
            line: None,
        },
    };
    assert_eq!(
        first
            .query_with_graph_lease(&symbol_request, QueryLimits::default(), &lease)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::UnsupportedOption
    );

    let cancelled = OperationControl::new(Duration::from_secs(5)).unwrap();
    cancelled.cancel();
    assert_eq!(
        first
            .query_with_graph_lease_and_control(
                &graph(0),
                QueryLimits::default(),
                &lease,
                &cancelled,
            )
            .unwrap_err()
            .kind(),
        QueryRefusalKind::Cancelled
    );

    let foreign = second
        .query_with_graph_lease(&graph(0), QueryLimits::default(), &lease)
        .unwrap_err();
    assert_eq!(foreign.kind(), QueryRefusalKind::StaleHandle);

    first.begin_close();
    let closed = first
        .query_with_graph_lease(&graph(0), QueryLimits::default(), &lease)
        .unwrap_err();
    assert_eq!(closed.kind(), QueryRefusalKind::SourceUnavailable);
}

#[test]
fn graph_lease_capacity_releases_on_drop() {
    let root = fixture(4);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = Arc::new(open(&runtime, root.path()));
    let mut leases = Vec::new();
    for _ in 0..8 {
        leases.push(
            handle
                .begin_graph_publication_lease(Duration::from_secs(20))
                .unwrap(),
        );
    }

    let before_drop = handle.runtime_view();
    let since_drop = handle.change_version();
    let refused = handle
        .begin_graph_publication_lease(Duration::from_secs(20))
        .err()
        .expect("the ninth live lease must be refused");
    assert_eq!(refused.kind(), SourceRefusalKind::AdmissionUnavailable);
    assert_eq!(refused.retry(), symforge::embed::RetryAdvice::OnEvent);

    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let (result_tx, result_rx) = mpsc::sync_channel(2);
    let waiter_handle = Arc::clone(&handle);
    let waiter = thread::spawn(move || {
        ready_tx.send(()).unwrap();
        let changed = waiter_handle.wait_for_change(since_drop, Duration::from_secs(10));
        result_tx.send(changed).unwrap();
        // This second wait reuses the version from before the observed Drop.
        let late = waiter_handle.wait_for_change(since_drop, Duration::from_secs(10));
        result_tx.send(late).unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(
        result_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    drop(leases.pop());
    assert_ne!(handle.change_version(), since_drop);
    let after_drop = result_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("dropping a graph lease did not wake the capacity waiter")
        .unwrap();
    let late = result_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("a pre-Drop version missed the event when waiting after Drop")
        .unwrap();
    waiter.join().unwrap();
    for view in [after_drop, late] {
        assert_eq!(view.phase, before_drop.phase);
        assert_eq!(view.source_version, before_drop.source_version);
        assert_eq!(
            view.current_publication_identity,
            before_drop.current_publication_identity
        );
    }

    let replacement = handle
        .begin_graph_publication_lease(Duration::from_secs(20))
        .unwrap();
    drop(replacement);
}

#[test]
fn graph_lease_capacity_expiry_wakes_while_refresh_is_held() {
    let root = fixture(4);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = Arc::new(open(&runtime, root.path()));
    let gate = symforge::live_index::store::hold_reload_after_discovery_for_test(root.path());
    handle.request_refresh().unwrap();
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "reload did not reach its deterministic hold"
    );
    let leases = (0..8)
        .map(|_| {
            handle
                .begin_graph_publication_lease(Duration::from_secs(1))
                .unwrap()
        })
        .collect::<Vec<_>>();

    let before_expiry = handle.runtime_view();
    assert_eq!(before_expiry.phase, SourceRuntimePhase::Refreshing);
    let since_expiry = handle.change_version();
    let refused = handle
        .begin_graph_publication_lease(Duration::from_secs(20))
        .err()
        .expect("all eight leases must still be live before waiting for expiry");
    assert_eq!(refused.kind(), SourceRefusalKind::AdmissionUnavailable);
    assert_eq!(refused.retry(), symforge::embed::RetryAdvice::OnEvent);

    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let waiter_handle = Arc::clone(&handle);
    let waiter = thread::spawn(move || {
        let result = waiter_handle.wait_for_change(since_expiry, Duration::from_secs(10));
        result_tx.send(result).unwrap();
    });
    let after_expiry = result_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("lease expiry did not wake the capacity waiter while refresh was held")
        .unwrap();
    waiter.join().unwrap();
    assert_ne!(handle.change_version(), since_expiry);
    assert_eq!(after_expiry.phase, before_expiry.phase);
    assert_eq!(after_expiry.source_version, before_expiry.source_version);
    assert_eq!(
        after_expiry.current_publication_identity,
        before_expiry.current_publication_identity
    );
    assert_eq!(
        handle
            .query_with_graph_lease(&graph(0), QueryLimits::default(), &leases[0])
            .unwrap_err()
            .kind(),
        QueryRefusalKind::GraphPublicationLeaseExpired
    );
    let replacement = handle
        .begin_graph_publication_lease(Duration::from_secs(20))
        .unwrap();
    drop(replacement);
    drop(gate);
}

#[test]
fn graph_lease_refuses_after_admission_revocation() {
    let root = fixture(1);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let lease = handle
        .begin_graph_publication_lease(Duration::from_secs(20))
        .unwrap();

    handle.revoke_admission_for_test();

    let leased_refusal = handle
        .query_with_graph_lease(&graph(0), QueryLimits::default(), &lease)
        .unwrap_err();
    assert_eq!(
        leased_refusal.kind(),
        QueryRefusalKind::AdmissionUnavailable
    );
    assert_eq!(leased_refusal.retry(), symforge::embed::RetryAdvice::Never);
    let strict_refusal = handle.query(&graph(0), QueryLimits::default()).unwrap_err();
    assert_eq!(
        strict_refusal.kind(),
        QueryRefusalKind::AdmissionUnavailable
    );
}

#[test]
fn refresh_opt_in_refuses_after_admission_revocation() {
    let root = fixture(1);
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let gate = symforge::live_index::store::hold_reload_after_discovery_for_test(root.path());

    fs::write(
        root.path().join("src/refresh.rs"),
        "pub fn refresh_target() {}\n",
    )
    .unwrap();
    handle.request_refresh().unwrap();
    assert!(
        gate.wait_until_blocked(Duration::from_secs(5)),
        "reload did not reach its deterministic hold"
    );
    assert_eq!(handle.runtime_view().phase, SourceRuntimePhase::Refreshing);

    let first = handle
        .query_allowing_refresh(&graph(0), QueryLimits::default())
        .unwrap();
    assert!(first.is_refreshing());

    handle.revoke_admission_for_test();

    let refusal = handle
        .query_allowing_refresh(&graph(0), QueryLimits::default())
        .unwrap_err();
    assert_eq!(refusal.kind(), QueryRefusalKind::AdmissionUnavailable);
    assert_eq!(refusal.retry(), symforge::embed::RetryAdvice::Never);
    drop(gate);
}
