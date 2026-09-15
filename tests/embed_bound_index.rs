#![cfg(feature = "embed")]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::{
    EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase, SymbolSearchRequest,
    TextSearchRequest,
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

    let (entered, entered_rx) = mpsc::channel();
    let (close_done, close_rx) = mpsc::channel();
    let closer = std::thread::spawn(move || {
        entered.send(()).expect("close thread started");
        drop(handle);
        close_done.send(()).expect("report close");
    });
    entered_rx.recv().expect("close thread starts");
    // Through-cancel hold parks join; this is only to enter shutdown.
    std::thread::sleep(Duration::from_millis(50));

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
