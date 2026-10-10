#![cfg(feature = "embed")]

use std::{
    fs,
    time::{Duration, Instant},
};
use symforge::embed::parity::reference::{DependentSearchRequest, ReferenceSearchRequest};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

#[test]
fn reference_query_row_budget_includes_nested_callsite_evidence() {
    let root = tempfile::tempdir().unwrap();
    let calls = (0..24).map(|_| "    external_call();\n").collect::<String>();
    fs::write(root.path().join("lib.rs"), format!("pub fn caller() {{\n{calls}}}\n")).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let claim = handle.query(
        &QueryRequest::ReferenceSearch(ReferenceSearchRequest {
            name: "external_call".into(), kind: Some("call".into()),
            max_per_file: Some(50), ..Default::default()
        }),
        QueryLimits {max_results:3,max_bytes:65536},
    ).unwrap();
    let QueryOutput::ReferenceSearch(result)=claim.value() else {panic!("references")};
    let records=result.files.iter().map(|file| file.hits.len()+file.caller_declarations.len()).sum::<usize>()
        + result.target_candidates.len()+result.implementations.len();
    assert!(records <= 3, "nested evidence records exceed the aggregate row bound");
    assert!(claim.truncated());
    assert!(claim.usage().rows <= 3);
}

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
fn implementation_estimate_does_not_spend_budget_on_unreturned_evidence() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub trait Contract {}\npub struct Item;\nimpl Contract for Item {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let claim = handle.query(
        &QueryRequest::ReferenceSearch(ReferenceSearchRequest {
            name: "Contract".into(), mode: Some("implementations".into()),
            estimate: Some(true), ..Default::default()
        }),
        QueryLimits { max_results: 1, max_bytes: 4096 },
    ).unwrap();
    let QueryOutput::ReferenceSearch(result) = claim.value() else { panic!("estimate") };
    assert!(result.estimated_tokens.unwrap() > 0);
    assert!(result.implementations.is_empty());
    assert_eq!(claim.usage().rows, 1);
    assert!(!claim.truncated());
}


#[test]
fn reference_search_keeps_caller_declarations_scoping_limits_and_implementations() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn target() {}\npub fn caller() { target(); target(); }\npub trait Contract { fn work(&self); }\npub struct Item;\nimpl Contract for Item { fn work(&self) {} }\n").unwrap();
    fs::write(
        root.path().join("other.rs"),
        "pub fn target() {}\npub fn unrelated() { target(); }\n",
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let result = handle
        .query(
            &QueryRequest::ReferenceSearch(ReferenceSearchRequest {
                name: "target".into(),
                path: Some("lib.rs".into()),
                symbol_kind: Some("fn".into()),
                symbol_line: Some(1),
                kind: Some("call".into()),
                max_per_file: Some(1),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::ReferenceSearch(result) = result.value() else {
        panic!("references")
    };
    assert!(result.files.iter().any(|file| file.path == "lib.rs"));
    assert!(!result.files.iter().any(|file| file.path == "other.rs"));
    assert!(result.files.iter().all(|file| file.hits.len() <= 1));
    assert!(
        result
            .files
            .iter()
            .flat_map(|file| &file.caller_declarations)
            .any(|declaration| declaration.text.contains("caller"))
    );
    let implementations = handle
        .query(
            &QueryRequest::ReferenceSearch(ReferenceSearchRequest {
                name: "Contract".into(),
                mode: Some("implementations".into()),
                direction: Some("trait".into()),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::ReferenceSearch(implementations) = implementations.value() else {
        panic!("implementations")
    };
    assert!(
        implementations
            .implementations
            .iter()
            .any(|entry| entry.implementor == "Item" && entry.trait_name == "Contract")
    );
    let invalid = handle
        .query(
            &QueryRequest::ReferenceSearch(ReferenceSearchRequest {
                name: "target".into(),
                mode: Some("unsupported".into()),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(invalid.kind(), QueryRefusalKind::UnsupportedOption);
}

#[test]
fn dependent_search_preserves_file_graph_formats_and_symbol_redirect() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("provider.rs"),
        "pub struct Target;\npub fn target() {}\n",
    )
    .unwrap();
    fs::write(root.path().join("consumer.rs"), "use crate::provider::Target;\nuse crate::provider::target;\npub fn caller() { target(); }\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    for format in ["text", "mermaid", "dot"] {
        let result = handle
            .query(
                &QueryRequest::DependentSearch(DependentSearchRequest {
                    path: "provider.rs".into(),
                    format: Some(format.into()),
                    ..Default::default()
                }),
                QueryLimits::default(),
            )
            .unwrap();
        let QueryOutput::DependentSearch(result) = result.value() else {
            panic!("dependents")
        };
        assert!(result.files.iter().any(|file| file.path == "consumer.rs"));
        match format {
            "mermaid" => {
                assert!(result.rendered.contains("graph") || result.rendered.contains("flowchart"))
            }
            "dot" => assert!(result.rendered.contains("digraph")),
            _ => assert!(result.rendered.contains("consumer.rs")),
        }
    }
    let redirected = handle
        .query(
            &QueryRequest::DependentSearch(DependentSearchRequest {
                path: "provider.rs".into(),
                name: Some("target".into()),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::DependentSearch(redirected) = redirected.value() else {
        panic!("redirect")
    };
    assert!(redirected.references.is_some());
    assert!(
        serde_json::from_str::<ReferenceSearchRequest>(r#"{"name":"target","ignored":true}"#)
            .is_err()
    );
}
