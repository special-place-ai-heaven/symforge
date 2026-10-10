#![cfg(feature = "embed")]

use std::{
    fs,
    time::{Duration, Instant},
};
use symforge::embed::parity::symbol_context::{SymbolContextMode, SymbolContextRequest};
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
fn symbol_context_knowledge_only_does_not_return_unrequested_source_sections() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lib.rs"),
        "pub fn selected() { let internal_body = 42; }\n",
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let claim = handle
        .query(
            &QueryRequest::SymbolContext(SymbolContextRequest {
                name: "selected".into(),
                path: Some("lib.rs".into()),
                sections: Some(vec!["knowledge".into()]),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolContext(result) = claim.value() else {
        panic!("knowledge context")
    };
    assert!(result.context.is_none());
    assert!(result.trace.is_none());
    assert!(!result.rendered.contains("internal_body"));
}

#[test]
fn global_symbol_context_applies_kind_before_deciding_path_ambiguity() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("function.rs"), "pub fn selected() {}\n").unwrap();
    fs::write(root.path().join("type.rs"), "pub struct selected;\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let claim = handle
        .query(
            &QueryRequest::SymbolContext(SymbolContextRequest {
                name: "selected".into(),
                symbol_kind: Some("fn".into()),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolContext(result) = claim.value() else {
        panic!("selected context")
    };
    assert_eq!(result.path.as_deref(), Some("function.rs"));
    assert_eq!(result.refusal, None);
    assert!(
        result
            .context
            .as_ref()
            .unwrap()
            .body
            .contains("pub fn selected")
    );
}

#[test]
fn symbol_context_preserves_default_bundle_and_selected_trace_evidence() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub struct Inner { pub value: usize }\npub struct Input { pub inner: Inner }\npub fn helper() {}\npub fn selected(input: Input) -> Inner { helper(); input.inner }\npub fn caller(input: Input) { selected(input); }\npub fn sibling() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let request = SymbolContextRequest {
        name: "selected".into(),
        path: Some("lib.rs".into()),
        ..Default::default()
    };
    let default = handle
        .query(
            &QueryRequest::SymbolContext(request.clone()),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolContext(default) = default.value() else {
        panic!("symbol context")
    };
    assert_eq!(default.mode, SymbolContextMode::Default);
    assert!(default.rendered.contains("pub fn selected"));
    let context = default.context.as_ref().unwrap();
    assert!(
        context
            .callers
            .entries
            .iter()
            .any(|entry| entry.display_name == "selected"
                && entry.path == "lib.rs"
                && entry.line == 5
                && entry.enclosing.as_deref() == Some("in fn caller"))
    );
    assert!(
        context
            .callees
            .entries
            .iter()
            .any(|entry| entry.display_name.contains("helper"))
    );

    let bundle = handle
        .query(
            &QueryRequest::SymbolContext(SymbolContextRequest {
                bundle: Some(true),
                ..request.clone()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolContext(bundle) = bundle.value() else {
        panic!("bundle")
    };
    assert_eq!(bundle.mode, SymbolContextMode::Bundle);
    let context = bundle.context.as_ref().unwrap();
    assert!(context.dependencies.iter().any(|dep| dep.name == "Input"));
    assert!(context.dependencies.iter().any(|dep| dep.name == "Inner"));
    assert!(bundle.rendered.contains("pub struct Input"));

    let trace = handle
        .query(
            &QueryRequest::SymbolContext(SymbolContextRequest {
                sections: Some(vec!["siblings".into()]),
                verbosity: Some("signature".into()),
                ..request
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolContext(trace) = trace.value() else {
        panic!("trace")
    };
    assert_eq!(trace.mode, SymbolContextMode::Trace);
    let evidence = trace.trace.as_ref().unwrap();
    assert!(evidence.siblings.iter().any(|item| item.name == "sibling"));
    assert!(evidence.dependents.is_empty());
    assert!(evidence.git_activity.is_none());
}

#[test]
fn symbol_context_keeps_ambiguity_estimates_and_full_cache_evidence() {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "pub fn selected() {{\n{}\n}}\n",
        "    let value = 123;\n".repeat(80)
    );
    fs::write(root.path().join("lib.rs"), &source).unwrap();
    fs::write(root.path().join("other.rs"), "pub fn selected() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let ambiguous = handle
        .query(
            &QueryRequest::SymbolContext(SymbolContextRequest {
                name: "selected".into(),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolContext(ambiguous) = ambiguous.value() else {
        panic!("ambiguity")
    };
    assert_eq!(ambiguous.refusal, Some(QueryRefusalKind::AmbiguousSymbol));
    assert_eq!(ambiguous.candidates.len(), 2);

    let request = SymbolContextRequest {
        name: "selected".into(),
        path: Some("lib.rs".into()),
        bundle: Some(true),
        max_tokens: Some(30),
        ..Default::default()
    };
    let estimate = handle
        .query(
            &QueryRequest::SymbolContext(SymbolContextRequest {
                estimate: Some(true),
                ..request.clone()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolContext(estimate) = estimate.value() else {
        panic!("estimate")
    };
    assert!(estimate.estimate.as_ref().unwrap().body_tokens > 30);
    assert!(estimate.context.is_none());

    let session = handle.new_query_session().unwrap();
    let bounded = handle
        .query_with_session(
            &QueryRequest::SymbolContext(request),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    assert!(bounded.truncated());
    let cached = handle
        .query_with_session(
            &QueryRequest::Retrieve {
                handle: bounded.retrieve_handle().unwrap().into(),
                offset: 0,
            },
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::RetrievedOutput(cached) = cached.value() else {
        panic!("full cached context")
    };
    let full: serde_json::Value = serde_json::from_slice(&cached.bytes).unwrap();
    assert!(
        full["output"]["SymbolContext"]["rendered"]
            .as_str()
            .unwrap()
            .contains(source.trim_end())
    );
    assert!(
        serde_json::from_value::<SymbolContextRequest>(
            serde_json::json!({"name":"selected","ignored_scope":true})
        )
        .is_err()
    );
    assert_eq!(
        handle
            .query(
                &QueryRequest::SymbolContext(SymbolContextRequest {
                    name: "selected".into(),
                    path: Some("lib.rs".into()),
                    verbosity: Some("typo".into()),
                    ..Default::default()
                }),
                QueryLimits::default()
            )
            .unwrap_err()
            .kind(),
        QueryRefusalKind::UnsupportedOption
    );
}
