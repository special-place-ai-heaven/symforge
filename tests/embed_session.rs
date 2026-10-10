#![cfg(feature = "embed")]

#[test]
fn reset_expires_old_handles_even_after_identical_content_is_loaded_again() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn content() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let query = QueryRequest::File {
        path: "lib.rs".into(),
        start_line: None,
        end_line: None,
    };
    let first = handle
        .query_with_session(&query, QueryLimits::default(), &session, None)
        .unwrap();
    let old_handle = first.retrieve_handle().unwrap().to_owned();
    session.reset();
    let second = handle
        .query_with_session(&query, QueryLimits::default(), &session, None)
        .unwrap();
    let result = handle.query_with_session(
        &QueryRequest::Retrieve {
            handle: old_handle.clone(),
            offset: 0,
        },
        QueryLimits::default(),
        &session,
        None,
    );
    assert_eq!(result.unwrap_err().kind(), QueryRefusalKind::StaleHandle);
    assert_ne!(second.retrieve_handle(), Some(old_handle.as_str()));
}

use std::fs;
use std::time::{Duration, Instant};
use symforge::embed::parity::{
    QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest, QuerySymbolSelector,
};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn settle(handle: &EmbeddedSourceHandle, after: u64) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let view = handle.runtime_view();
        if view.phase == SourceRuntimePhase::Current && view.source_version > after {
            return;
        }
        assert!(Instant::now() < deadline, "source did not settle: {view:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::write(
        root.path().join("lib.rs"),
        b"pub fn callee() -> u32 { 7 }\npub fn caller() -> u32 { callee() }\n",
    )
    .unwrap();
    root
}

fn open(runtime: &ProcessIndexRuntime, root: &std::path::Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    settle(&handle, 0);
    handle
}

#[test]
fn session_inventory_and_investigation_share_loaded_context_and_refuse_foreign_sources() {
    let root = fixture();
    let other_root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let other = open(&runtime, other_root.path());
    let session = handle.new_query_session().unwrap();
    let limits = QueryLimits::default();
    assert_eq!(
        handle
            .query(&QueryRequest::ContextInventory, limits)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::SessionRequired
    );
    handle
        .query_with_session(
            &QueryRequest::SearchSymbols {
                query: Some("callee".into()),
                path_prefix: None,
                kind: None,
                include_tests: false,
            },
            limits,
            &session,
            None,
        )
        .unwrap();
    handle
        .query_with_session(
            &QueryRequest::Symbol {
                selector: QuerySymbolSelector {
                    path: "lib.rs".into(),
                    name: "caller".into(),
                    kind: None,
                    line: None,
                },
            },
            limits,
            &session,
            None,
        )
        .unwrap();
    let inventory = handle
        .query_with_session(&QueryRequest::ContextInventory, limits, &session, None)
        .unwrap();
    let QueryOutput::ContextInventory(inventory) = inventory.value() else {
        panic!("inventory")
    };
    assert!(
        inventory
            .fetched_symbols
            .iter()
            .any(|symbol| symbol.name == "caller")
    );
    assert!(
        inventory
            .listed_symbols
            .iter()
            .any(|symbol| symbol.name == "callee")
    );
    assert!(
        !inventory
            .fetched_symbols
            .iter()
            .any(|symbol| symbol.name == "callee")
    );
    let guidance = handle
        .query_with_session(
            &QueryRequest::InvestigationSuggest { focus: None },
            limits,
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::InvestigationSuggestion(guidance) = guidance.value() else {
        panic!("guidance")
    };
    assert!(guidance.text.contains("callee"), "{}", guidance.text);
    assert_eq!(
        other
            .query_with_session(&QueryRequest::ContextInventory, limits, &session, None)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::ForeignSession
    );
    let isolated = handle.new_query_session().unwrap();
    let inventory = handle
        .query_with_session(&QueryRequest::ContextInventory, limits, &isolated, None)
        .unwrap();
    let QueryOutput::ContextInventory(inventory) = inventory.value() else {
        panic!("inventory")
    };
    assert!(inventory.fetched_symbols.is_empty());
    other.close().unwrap();
    handle.close().unwrap();
}

#[test]
fn session_retrieval_preserves_served_bytes_across_refresh_and_reset_expires_handles() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let served = handle
        .query_with_session(
            &QueryRequest::File {
                path: "lib.rs".into(),
                start_line: None,
                end_line: None,
            },
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let cached = served.retrieve_handle().unwrap().to_owned();
    let old_version = handle.runtime_view().source_version;
    fs::write(root.path().join("lib.rs"), b"pub fn changed() {}\n").unwrap();
    handle.request_refresh().unwrap();
    settle(&handle, old_version);
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let result = handle
            .query_with_session(
                &QueryRequest::Retrieve {
                    handle: cached.clone(),
                    offset,
                },
                QueryLimits {
                    max_results: 10,
                    max_bytes: 128,
                },
                &session,
                None,
            )
            .unwrap();
        let QueryOutput::RetrievedOutput(retrieved) = result.value() else {
            panic!("retrieval")
        };
        assert!(retrieved.superseded);
        assert!(retrieved.content_generation < retrieved.current_content_generation);
        assert_eq!(retrieved.byte_start, offset);
        bytes.extend_from_slice(&retrieved.bytes);
        match retrieved.next_offset {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => break,
        }
    }
    let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        record["output"],
        serde_json::to_value(served.value()).unwrap()
    );
    assert_eq!(
        record["serving_publication_identity"],
        served.publication_identity()
    );
    session.reset();
    assert_eq!(
        handle
            .query_with_session(
                &QueryRequest::Retrieve {
                    handle: cached,
                    offset: 0
                },
                QueryLimits::default(),
                &session,
                None
            )
            .unwrap_err()
            .kind(),
        QueryRefusalKind::StaleHandle
    );
    handle.close().unwrap();
}
