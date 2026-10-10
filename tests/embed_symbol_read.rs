#![cfg(feature = "embed")]

use std::{
    fs,
    time::{Duration, Instant},
};
use symforge::embed::parity::symbol::{InspectMatchRequest, SymbolReadRequest, SymbolTarget};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn open(runtime: &ProcessIndexRuntime, root: &std::path::Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

#[test]
fn symbol_read_supports_global_resolution_batch_exact_bytes_and_ambiguity() {
    let root = tempfile::tempdir().unwrap();
    let source = "// é\r\npub struct Item;\r\nimpl Item { pub fn run(&self) -> usize { 7 } }\r\npub fn selected() -> usize { 9 }\r\n";
    fs::write(root.path().join("lib.rs"), source).unwrap();
    fs::write(root.path().join("other.rs"), "pub fn run() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let global = handle
        .query(
            &QueryRequest::SymbolRead(SymbolReadRequest {
                name: "selected".into(),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolRead(global) = global.value() else {
        panic!("symbol read")
    };
    assert_eq!(global.entries[0].path, "lib.rs");
    assert!(global.rendered.contains("pub fn selected() -> usize { 9 }"));
    let ambiguous = handle
            .query(
                &QueryRequest::SymbolRead(SymbolReadRequest {
                    name: "run".into(),
                    ..Default::default()
                }),
                QueryLimits::default()
            )
            .unwrap();
    let QueryOutput::SymbolRead(ambiguous) = ambiguous.value() else { panic!("ambiguity evidence") };
    assert_eq!(ambiguous.entries[0].refusal, Some(QueryRefusalKind::AmbiguousSymbol));
    assert_eq!(ambiguous.entries[0].candidates.len(), 2);
    assert!(ambiguous.entries[0].candidates.iter().any(|symbol| symbol.path == "other.rs"));
    assert!(ambiguous.entries[0].candidates.iter().any(|symbol| symbol.path == "lib.rs"));
    let batch = handle
        .query(
            &QueryRequest::SymbolRead(SymbolReadRequest {
                targets: Some(vec![
                    SymbolTarget {
                        path: "lib.rs".into(),
                        name: Some("run".into()),
                        kind: Some("method".into()),
                        symbol_line: Some(3),
                        start_byte: None,
                        end_byte: None,
                    },
                    SymbolTarget {
                        path: "lib.rs".into(),
                        name: None,
                        kind: None,
                        symbol_line: None,
                        start_byte: Some(0),
                        end_byte: Some("// é\r\n".len() as u32),
                    },
                    SymbolTarget {
                        path: "absent.rs".into(),
                        name: Some("missing".into()),
                        kind: None,
                        symbol_line: None,
                        start_byte: None,
                        end_byte: None,
                    },
                ]),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolRead(batch) = batch.value() else {
        panic!("batch read")
    };
    assert_eq!(batch.entries.len(), 3);
    assert_eq!(batch.entries[0].symbol.as_ref().unwrap().start_line, 3);
    assert_eq!(
        batch.entries[1].source.as_deref(),
        Some("// é\r\n".as_bytes())
    );
    assert_eq!(batch.entries[2].refusal, Some(QueryRefusalKind::NotFound));
    assert!(
        handle
            .query(
                &QueryRequest::SymbolRead(SymbolReadRequest {
                    targets: Some(vec![SymbolTarget {
                        path: "lib.rs".into(),
                        name: None,
                        kind: None,
                        symbol_line: None,
                        start_byte: Some(9),
                        end_byte: Some(2)
                    }]),
                    ..Default::default()
                }),
                QueryLimits::default()
            )
            .is_err()
    );
}

#[test]
fn symbol_read_estimates_cache_bypass_and_full_overflow_retrieval_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "pub fn lengthy() {{\n{}\n}}\n",
        "    let value = 123;\n".repeat(80)
    );
    fs::write(root.path().join("lib.rs"), &source).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let request = SymbolReadRequest {
        path: "lib.rs".into(),
        name: "lengthy".into(),
        max_tokens: Some(20),
        ..Default::default()
    };
    let estimate = handle
        .query(
            &QueryRequest::SymbolRead(SymbolReadRequest {
                estimate: Some(true),
                ..request.clone()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolRead(estimate) = estimate.value() else {
        panic!("estimate")
    };
    assert!(estimate.estimated_tokens.unwrap() > 20);
    assert!(estimate.entries.iter().all(|entry| entry.source.is_none()));
    let first = handle
        .query_with_session(
            &QueryRequest::SymbolRead(request.clone()),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    assert!(first.truncated());
    let retrieve_handle = first.retrieve_handle().unwrap().to_owned();
    let repeated = handle
        .query_with_session(
            &QueryRequest::SymbolRead(request.clone()),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::SymbolRead(repeated) = repeated.value() else {
        panic!("cache read")
    };
    assert!(repeated.cache_hit);
    let full = handle
        .query_with_session(
            &QueryRequest::Retrieve {
                handle: retrieve_handle,
                offset: 0,
            },
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::RetrievedOutput(full) = full.value() else {
        panic!("cached output")
    };
    let cached: serde_json::Value = serde_json::from_slice(&full.bytes).unwrap();
    assert!(
        cached["output"]["SymbolRead"]["rendered"]
            .as_str()
            .unwrap()
            .contains(&source)
    );
    let forced = handle
        .query_with_session(
            &QueryRequest::SymbolRead(SymbolReadRequest {
                force_refresh: Some(true),
                ..request
            }),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::SymbolRead(forced) = forced.value() else {
        panic!("fresh read")
    };
    assert!(!forced.cache_hit);
}

#[test]
fn inspect_match_preserves_nested_scope_siblings_estimate_and_strict_input() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("nested.py"), "class Worker:\n    def run(self):\n        value = 1\n        return value\n    def stop(self):\n        return None\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let request = InspectMatchRequest {
        path: "nested.py".into(),
        line: 3,
        context: Some(1),
        sibling_limit: Some(0),
        estimate: None,
        max_tokens: None,
    };
    let found = handle
        .query(
            &QueryRequest::InspectMatch(request.clone()),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::InspectMatch(found) = found.value() else {
        panic!("inspection")
    };
    assert_eq!(found.enclosing.as_ref().unwrap().name, "run");
    assert!(
        found
            .parent_chain
            .iter()
            .any(|symbol| symbol.name == "Worker")
    );
    assert!(found.siblings.is_empty());
    assert!(found.excerpt.contains("3:         value = 1"));
    let estimate = handle
        .query(
            &QueryRequest::InspectMatch(InspectMatchRequest {
                estimate: Some(true),
                ..request
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::InspectMatch(estimate) = estimate.value() else {
        panic!("estimate")
    };
    assert!(estimate.estimated_tokens.unwrap() > 0);
    assert!(
        serde_json::from_str::<InspectMatchRequest>(
            r#"{"path":"nested.py","line":3,"ignored":true}"#
        )
        .is_err()
    );
}
