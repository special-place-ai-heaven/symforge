#![cfg(feature = "embed")]

#[test]
fn search_token_caps_keep_full_shared_result_in_session_cache() {
    let root = tempfile::tempdir().unwrap();
    let source = (0..24)
        .map(|i| format!("pub fn selected_{i}() {{}}\n"))
        .collect::<String>();
    std::fs::write(root.path().join("lib.rs"), source).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let result = handle
        .query_with_session(
            &QueryRequest::SymbolSearch(SymbolSearchRequest {
                query: Some("selected".into()),
                limit: Some(24),
                max_tokens: Some(120),
                ..Default::default()
            }),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    assert!(result.truncated());
    let full = handle
        .query_with_session(
            &QueryRequest::Retrieve {
                handle: result.retrieve_handle().unwrap().into(),
                offset: 0,
            },
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::RetrievedOutput(full) = full.value() else {
        panic!("full search cache");
    };
    let output: serde_json::Value = serde_json::from_slice(&full.bytes).unwrap();
    assert_eq!(
        output["output"]["SymbolSearch"]["symbols"]
            .as_array()
            .unwrap()
            .len(),
        24
    );
}

use std::fs;
use std::time::{Duration, Instant};
use symforge::embed::parity::search::{
    SymbolSearchRequest, TextGrouping, TextRows, TextSearchRequest,
};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::create_dir(root.path().join("tests")).unwrap();
    fs::write(root.path().join("src/lib.rs"), b"pub fn target() -> u32 {\n    let color = 1;\n    let colorful = 2;\n    color + colorful\n}\npub fn caller() -> u32 { target() }\n// color comment\n").unwrap();
    fs::write(
        root.path().join("src/omit.rs"),
        b"pub fn omitted() { let color = 1; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/other.py"),
        b"def python_color():\n    color = 1\n",
    )
    .unwrap();
    fs::write(
        root.path().join("tests/check.rs"),
        b"fn check() { let color = 1; }\n",
    )
    .unwrap();
    root
}
fn open(runtime: &ProcessIndexRuntime, root: &std::path::Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}
fn text(
    handle: &EmbeddedSourceHandle,
    request: TextSearchRequest,
) -> symforge::embed::parity::QueryClaim {
    handle
        .query(&QueryRequest::TextSearch(request), QueryLimits::default())
        .unwrap()
}

#[test]
fn rich_text_search_applies_terms_scope_globs_words_and_limits_before_projection() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let claim = text(
        &handle,
        TextSearchRequest {
            terms: Some(vec!["color".into(), "caller".into()]),
            whole_word: Some(true),
            language: Some("Rust".into()),
            glob: Some("**/*.rs".into()),
            exclude_glob: Some("**/omit.rs".into()),
            max_per_file: Some(20),
            limit: Some(20),
            ..Default::default()
        },
    );
    let QueryOutput::TextSearch(report) = claim.value() else {
        panic!()
    };
    let TextRows::Files(files) = &report.rows else {
        panic!()
    };
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "src/lib.rs");
    assert!(files[0].matches.iter().all(|hit| hit.line != 3));
    assert!(files[0].matches.iter().any(|hit| {
        hit.enclosing_symbol
            .as_ref()
            .is_some_and(|symbol| symbol.name == "caller")
    }));
    assert!(report.suppressed_by_noise > 0);
    let included = text(
        &handle,
        TextSearchRequest {
            query: Some("color".into()),
            include_tests: Some(true),
            path_prefix: Some("tests/".into()),
            ..Default::default()
        },
    );
    let QueryOutput::TextSearch(report) = included.value() else {
        panic!()
    };
    let TextRows::Files(files) = &report.rows else {
        panic!()
    };
    assert_eq!(files[0].path, "tests/check.rs");
}

#[test]
fn rich_text_search_preserves_regex_recovery_ast_context_callers_and_grouping() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let found = text(
        &handle,
        TextSearchRequest {
            query: Some(r"let\\s+color\\s*=".into()),
            regex: Some(true),
            path_prefix: Some("src/lib.rs".into()),
            max_per_file: Some(20),
            context: Some(1),
            follow_refs: Some(true),
            ..Default::default()
        },
    );
    let QueryOutput::TextSearch(report) = found.value() else {
        panic!()
    };
    assert!(report.auto_corrected_regex && report.regex);
    let TextRows::Files(files) = &report.rows else {
        panic!()
    };
    assert!(files[0].context.as_ref().unwrap().len() >= 3);
    assert!(
        files[0]
            .callers
            .as_ref()
            .unwrap()
            .iter()
            .any(|caller| caller.symbol == "caller")
    );
    let names = text(
        &handle,
        TextSearchRequest {
            query: Some("color".into()),
            path_prefix: Some("src/lib.rs".into()),
            group_by: Some(TextGrouping::Names),
            ..Default::default()
        },
    );
    let QueryOutput::TextSearch(report) = names.value() else {
        panic!()
    };
    assert_eq!(report.rows, TextRows::Names(vec!["target".into()]));
    let usage = text(
        &handle,
        TextSearchRequest {
            query: Some("color".into()),
            path_prefix: Some("src/lib.rs".into()),
            group_by: Some(TextGrouping::Usage),
            max_per_file: Some(20),
            ..Default::default()
        },
    );
    let QueryOutput::TextSearch(report) = usage.value() else {
        panic!()
    };
    assert_eq!(report.excluded_by_usage, 1);
    let structural = text(
        &handle,
        TextSearchRequest {
            query: Some("target()".into()),
            structural: Some(true),
            language: Some("Rust".into()),
            ..Default::default()
        },
    );
    let QueryOutput::TextSearch(report) = structural.value() else {
        panic!()
    };
    assert!(report.structural && report.total_matches > 0);
    let detected = text(
        &handle,
        TextSearchRequest {
            query: Some(r"let\s+color".into()),
            ..Default::default()
        },
    );
    let QueryOutput::TextSearch(report) = detected.value() else {
        panic!()
    };
    assert!(report.auto_detected_regex);
}

#[test]
fn rich_symbol_search_browses_with_language_kind_and_explicit_invalid_options() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let found = handle
        .query(
            &QueryRequest::SymbolSearch(SymbolSearchRequest {
                language: Some("Rust".into()),
                kind: Some("function".into()),
                path_prefix: Some("src/lib.rs".into()),
                limit: Some(1),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SymbolSearch(report) = found.value() else {
        panic!()
    };
    assert_eq!(report.symbols.len(), 1);
    assert_eq!(report.overflow_count, 1);
    assert!(found.truncated());
    assert!(
        serde_json::from_str::<TextSearchRequest>(r#"{"query":"color","invented":true}"#).is_err()
    );
    assert!(
        serde_json::from_str::<TextSearchRequest>(r#"{"query":"color","group_by":"invented"}"#)
            .is_err()
    );
    let invalid = handle
        .query(
            &QueryRequest::TextSearch(TextSearchRequest {
                query: Some("color".into()),
                regex: Some(true),
                whole_word: Some(true),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(invalid.kind(), QueryRefusalKind::UnsupportedOption);
}
