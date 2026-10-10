#![cfg(feature = "embed")]

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
    fs::create_dir(root.path().join("src")).unwrap();
    fs::create_dir(root.path().join("tests")).unwrap();
    fs::write(root.path().join("src/lib.rs"),
        "pub struct Widget;\r\npub fn target() -> u32 { 1 }\r\npub fn caller() -> u32 { let _label = \"ž\"; target() }\r\n#[cfg(test)]\r\nmod tests { fn inline_test() { super::target(); } }\r\n").unwrap();
    fs::write(
        root.path().join("src/other.rs"),
        "pub fn target() -> u32 { 2 }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("tests/check.rs"),
        "fn check() { target(); }\n",
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

fn target() -> QuerySymbolSelector {
    QuerySymbolSelector {
        path: "src/lib.rs".into(),
        name: "target".into(),
        kind: Some("function".into()),
        line: None,
    }
}

#[test]
fn parity_decoded_payload_budget_is_distinct_from_escaped_json_frame_size() {
    fn string_bytes(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::String(value) => value.len(),
            serde_json::Value::Array(values) => values.iter().map(string_bytes).sum(),
            serde_json::Value::Object(values) => values.values().map(string_bytes).sum(),
            _ => 0,
        }
    }
    let root = fixture();
    let source = format!(
        "pub fn escaped() {{ let marker = r#\"{}\"#; }}\r\n",
        "ž\\\"\t".repeat(60)
    );
    fs::write(root.path().join("src/escaped.rs"), source.as_bytes()).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let limits = QueryLimits {
        max_results: 20,
        max_bytes: 512,
    };
    let claim = handle
        .query(
            &QueryRequest::SearchText {
                query: "marker".into(),
                path_prefix: Some("src/escaped.rs".into()),
                regex: false,
                case_sensitive: true,
                include_tests: false,
            },
            limits,
        )
        .unwrap();
    let QueryOutput::Text(rows) = claim.value() else {
        panic!("text result")
    };
    assert_eq!(rows.len(), 1);
    assert!(rows[0].preview.contains('ž'));
    assert!(rows[0].preview.contains('\t'));
    assert!(!claim.truncated());
    assert!(
        string_bytes(&serde_json::to_value(claim.value()).unwrap()) <= limits.max_bytes as usize
    );
    assert!(serde_json::to_vec(claim.value()).unwrap().len() > limits.max_bytes as usize);
    let claim = handle
        .query(
            &QueryRequest::File {
                path: "src/escaped.rs".into(),
                start_line: None,
                end_line: None,
            },
            limits,
        )
        .unwrap();
    let QueryOutput::File { file, .. } = claim.value() else {
        panic!("file result")
    };
    let payload = string_bytes(&serde_json::to_value(claim.value()).unwrap())
        + file.source.as_ref().map_or(0, Vec::len);
    assert!(payload <= limits.max_bytes as usize);
    assert_eq!(file.source.as_deref(), Some(source.as_bytes()));
    assert!(serde_json::to_vec(claim.value()).unwrap().len() > limits.max_bytes as usize);
    handle.close().unwrap();
}

#[cfg(feature = "__test-internals")]
#[test]
fn child_graph_publication_process() {
    let Some(mode) = std::env::var_os("SYMFORGE_GRAPH_TEST_MODE") else {
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os("SYMFORGE_GRAPH_TEST_ROOT").unwrap());
    let receipt =
        std::path::PathBuf::from(std::env::var_os("SYMFORGE_GRAPH_TEST_RECEIPT").unwrap());
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, &root);
    let expected = (mode == "resume").then(|| fs::read_to_string(&receipt).unwrap());
    let result = handle.query(
        &QueryRequest::Graph {
            path_prefix: None,
            offset: if expected.is_some() { 1 } else { 0 },
            expected_publication: expected,
        },
        QueryLimits {
            max_results: 1,
            max_bytes: 4096,
        },
    );
    if mode == "resume" {
        assert_eq!(
            result.unwrap_err().kind(),
            QueryRefusalKind::StalePublication
        );
    } else {
        let claim = result.unwrap();
        let QueryOutput::Graph(graph) = claim.value() else {
            panic!("graph result");
        };
        assert_eq!(graph.next_offset, Some(1));
        fs::write(&receipt, claim.publication_identity()).unwrap();
    }
    handle.close().unwrap();
}

#[cfg(feature = "__test-internals")]
#[test]
fn parity_graph_cursor_refuses_a_different_process_with_reused_counters() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    let receipt = output.path().join("publication");
    let run = |mode| {
        let output = symforge::process_util::hidden_command(std::env::current_exe().unwrap())
            .args(["--exact", "child_graph_publication_process", "--nocapture"])
            .env("SYMFORGE_GRAPH_TEST_MODE", mode)
            .env("SYMFORGE_GRAPH_TEST_ROOT", root.path())
            .env("SYMFORGE_GRAPH_TEST_RECEIPT", &receipt)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "graph child failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
    };
    run("capture");
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn changed() -> u32 { 7 }\n",
    )
    .unwrap();
    run("resume");
}

#[cfg(feature = "__test-internals")]
#[test]
fn child_query_commitments_process() {
    if std::env::var_os("SYMFORGE_COMMITMENT_TEST_CHILD").is_none() {
        return;
    }
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let path = std::path::Path::new("src/lib.rs");
    let admitted_root = fs::canonicalize(root.path()).unwrap();
    let score = || {
        symforge::live_index::frecency::ranking_scores_for_paths(&admitted_root, None, &[path], 0)
            .unwrap()
            .and_then(|snapshot| snapshot.scores.get(path).copied())
            .unwrap_or(0.0)
    };
    assert_eq!(score(), 0.0);
    for request in [
        QueryRequest::SearchSymbols {
            query: Some("target".into()),
            path_prefix: None,
            kind: None,
            include_tests: false,
        },
        QueryRequest::SearchText {
            query: "target".into(),
            path_prefix: None,
            regex: false,
            case_sensitive: true,
            include_tests: false,
        },
        QueryRequest::SearchFiles {
            query: "lib".into(),
            path_prefix: None,
            include_tests: false,
        },
        QueryRequest::Explore(symforge::embed::parity::guidance::ExploreRequest::new(
            "target",
        )),
    ] {
        handle.query(&request, QueryLimits::default()).unwrap();
        assert_eq!(score(), 0.0, "discovery must not create commitment history");
    }
    let request = QueryRequest::File {
        path: "src/lib.rs".into(),
        start_line: None,
        end_line: None,
    };
    let cancelled =
        symforge::embed::parity::host::OperationControl::new(Duration::from_secs(10)).unwrap();
    cancelled.cancel();
    assert_eq!(
        handle
            .query_with_control(&request, QueryLimits::default(), &cancelled)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::Cancelled,
    );
    assert_eq!(score(), 0.0, "cancelled reads must not bump");
    for request in [
        request,
        QueryRequest::Symbol { selector: target() },
        QueryRequest::Context { selector: target() },
    ] {
        let before = score();
        handle.query(&request, QueryLimits::default()).unwrap();
        assert!(
            score() > before,
            "loaded source context must record a commitment"
        );
    }
    assert!(!root.path().join(".symforge/frecency.db").exists());
    handle.close().unwrap();
}

#[cfg(feature = "__test-internals")]
#[test]
fn parity_queries_share_commitment_frecency_without_discovery_feedback() {
    let output = symforge::process_util::hidden_command(std::env::current_exe().unwrap())
        .args(["--exact", "child_query_commitments_process", "--nocapture"])
        .env("SYMFORGE_COMMITMENT_TEST_CHILD", "1")
        .env("SYMFORGE_FRECENCY", "session")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "commitment child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
}

#[test]
fn parity_reads_searches_and_context_preserve_bytes_filters_and_authority() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let read = handle
        .query(
            &QueryRequest::File {
                path: "src/lib.rs".into(),
                start_line: Some(2),
                end_line: Some(3),
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::File { file, symbols, .. } = read.value() else {
        panic!("file result")
    };
    assert_eq!(file.source.as_deref().unwrap(), "pub fn target() -> u32 { 1 }\r\npub fn caller() -> u32 { let _label = \"ž\"; target() }\r\n".as_bytes());
    assert!(symbols.iter().any(|symbol| symbol.name == "Widget"));
    assert_eq!(read.provenance().authority_count(), 2);
    assert_eq!(
        read.operation_receipt().operation_kind(),
        symforge::embed::parity::QueryOperationKind::File
    );
    assert_eq!(
        read.operation_receipt().identity(),
        read.operation_identity()
    );
    assert_eq!(
        read.operation_receipt().canonical_argument_hash(),
        read.canonical_argument_hash()
    );
    assert_eq!(read.canonical_argument_hash().len(), 64);
    assert_ne!(
        read.canonical_argument_hash(),
        read.source_capture_canonical_argument_hash()
    );
    assert_ne!(
        read.publication_identity(),
        read.source_capture_publication_identity()
    );
    assert_ne!(
        read.producing_runtime_identity(),
        read.source_capture_runtime_identity()
    );
    assert_eq!(
        read.operation_receipt().schema_version(),
        symforge::embed::parity::API_VERSION
    );
    assert_eq!(
        read.source_capture_receipt_kind(),
        symforge::embed::OperationKind::IndexCensus
    );
    assert!(!read.truncated());

    let symbols = handle
        .query(
            &QueryRequest::SearchSymbols {
                query: None,
                path_prefix: Some("src".into()),
                kind: Some("struct".into()),
                include_tests: false,
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Symbols(symbols) = symbols.value() else {
        panic!("symbols result")
    };
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Widget");

    let text = handle
        .query(
            &QueryRequest::SearchText {
                query: "TARGET\\s*\\(".into(),
                path_prefix: None,
                regex: true,
                case_sensitive: false,
                include_tests: false,
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Text(hits) = text.value() else {
        panic!("text result")
    };
    assert!(hits.iter().any(|hit| {
        hit.enclosing_symbol
            .as_ref()
            .is_some_and(|s| s.name == "caller")
    }));
    assert!(
        hits.iter()
            .all(|hit| !hit.path.starts_with("tests/") && hit.line != 5)
    );
    let with_tests = handle
        .query(
            &QueryRequest::SearchText {
                query: "target".into(),
                path_prefix: None,
                regex: false,
                case_sensitive: true,
                include_tests: true,
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Text(all_hits) = with_tests.value() else {
        panic!("text result")
    };
    assert!(all_hits.len() > hits.len());

    let context = handle
        .query(
            &QueryRequest::Context { selector: target() },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Context(context) = context.value() else {
        panic!("context result")
    };
    assert_eq!(context.symbol.name, "target");
    assert!(
        context
            .callers
            .iter()
            .any(|reference| reference.path == "src/lib.rs")
    );
    assert!(
        context
            .symbol
            .source
            .as_deref()
            .unwrap()
            .starts_with(b"pub fn target")
    );

    let invalid = handle
        .query(
            &QueryRequest::SearchText {
                query: "[".into(),
                path_prefix: None,
                regex: true,
                case_sensitive: true,
                include_tests: false,
            },
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(invalid.kind(), QueryRefusalKind::InvalidRequest);
    let invalid = handle
        .query(
            &QueryRequest::SearchSymbols {
                query: None,
                path_prefix: None,
                kind: Some("invented-kind".into()),
                include_tests: true,
            },
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(invalid.kind(), QueryRefusalKind::UnsupportedOption);
    let traversal = handle
        .query(
            &QueryRequest::File {
                path: "../outside.rs".into(),
                start_line: None,
                end_line: None,
            },
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(traversal.kind(), QueryRefusalKind::InvalidRequest);
    handle.begin_close();
    let stopped = handle
        .query(
            &QueryRequest::Symbol { selector: target() },
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(stopped.kind(), QueryRefusalKind::SourceUnavailable);
}

#[test]
fn parity_graph_pages_are_lossless_stable_and_refuse_cross_generation_resume() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let request = QueryRequest::Graph {
        path_prefix: None,
        offset: 0,
        expected_publication: None,
    };
    let full = handle.query(&request, QueryLimits::default()).unwrap();
    let QueryOutput::Graph(full_graph) = full.value() else {
        panic!("graph result")
    };
    assert!(
        full_graph
            .references
            .iter()
            .any(|reference| reference.enclosing_symbol_index.is_some())
    );
    let duplicate_targets: Vec<_> = full_graph
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "target")
        .collect();
    assert_eq!(duplicate_targets.len(), 2);
    assert_ne!(duplicate_targets[0].identity, duplicate_targets[1].identity);
    let mut files = Vec::new();
    let mut symbols = Vec::new();
    let mut references = Vec::new();
    let mut offset = 0;
    loop {
        let page = handle
            .query(
                &QueryRequest::Graph {
                    path_prefix: None,
                    offset,
                    expected_publication: Some(full.publication_identity().into()),
                },
                QueryLimits {
                    max_results: 2,
                    max_bytes: 4096,
                },
            )
            .unwrap();
        assert_eq!(page.publication_identity(), full.publication_identity());
        let QueryOutput::Graph(graph) = page.value() else {
            panic!("graph result")
        };
        files.extend(graph.files.iter().cloned());
        symbols.extend(graph.symbols.iter().cloned());
        references.extend(graph.references.iter().cloned());
        let Some(next) = graph.next_offset else { break };
        assert!(next > offset);
        assert!(page.truncated());
        offset = next;
    }
    assert_eq!(files, full_graph.files);
    assert_eq!(symbols, full_graph.symbols);
    assert_eq!(references, full_graph.references);
    let version = handle.runtime_view().source_version;
    fs::remove_file(root.path().join("src/other.rs")).unwrap();
    handle.request_refresh().unwrap();
    settle(&handle, version);
    let stale = handle
        .query(
            &QueryRequest::Graph {
                path_prefix: None,
                offset: 2,
                expected_publication: Some(full.publication_identity().into()),
            },
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(stale.kind(), QueryRefusalKind::StalePublication);
    let changed = handle.query(&request, QueryLimits::default()).unwrap();
    let QueryOutput::Graph(changed_graph) = changed.value() else {
        panic!("graph result")
    };
    assert!(
        !changed_graph
            .files
            .iter()
            .any(|file| file.path == "src/other.rs")
    );
    let stable_ids: Vec<_> = changed_graph
        .symbols
        .iter()
        .map(|symbol| symbol.identity.clone())
        .collect();
    handle.begin_close();
    let reopened = open(&runtime, root.path());
    let restart = reopened.query(&request, QueryLimits::default()).unwrap();
    let QueryOutput::Graph(restart_graph) = restart.value() else {
        panic!("graph result")
    };
    assert_eq!(
        stable_ids,
        restart_graph
            .symbols
            .iter()
            .map(|symbol| symbol.identity.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn parity_bounds_and_partial_syntax_are_explicit() {
    let root = fixture();
    fs::write(root.path().join("src/broken.rs"), "pub fn broken( {\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let syntax = handle
        .query(
            &QueryRequest::Syntax {
                path: "src/broken.rs".into(),
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Syntax(syntax) = syntax.value() else {
        panic!("syntax result")
    };
    assert!(!syntax.valid);
    let limited = handle
        .query(
            &QueryRequest::SearchSymbols {
                query: None,
                path_prefix: None,
                kind: None,
                include_tests: true,
            },
            QueryLimits {
                max_results: 1,
                max_bytes: 4096,
            },
        )
        .unwrap();
    let QueryOutput::Symbols(symbols) = limited.value() else {
        panic!("symbols result")
    };
    assert_eq!(symbols.len(), 1);
    assert!(limited.truncated());
    assert!(limited.partial_files() > 0);
}

#[test]
fn parity_git_diff_and_impact_bind_local_commit_and_admitted_current_bytes() {
    let root = fixture();
    let repo = git2::Repository::open(root.path()).unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["src", "tests"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    let commit = repo
        .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
        .unwrap();
    fs::write(
        root.path().join("src/other.rs"),
        "pub fn target() -> u32 { 3 }\npub fn added() {}\n",
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let result = handle
        .query(
            &QueryRequest::Diff {
                path: "src/other.rs".into(),
                from_ref: "HEAD".into(),
                to_ref: None,
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Diff(diff) = result.value() else {
        panic!("diff result")
    };
    assert_eq!(diff.from_commit, commit.to_string());
    assert!(
        diff.changes
            .iter()
            .any(|change| change.name == "target" && change.change == "modified")
    );
    assert!(
        diff.changes
            .iter()
            .any(|change| change.name == "added" && change.change == "added")
    );
    assert!(!result.observations().is_empty());
    let impact = handle
        .query(
            &QueryRequest::Impact {
                path: "src/lib.rs".into(),
                base_ref: "HEAD".into(),
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Impact { diff, .. } = impact.value() else {
        panic!("impact result")
    };
    assert!(diff.changes.is_empty());
    let unknown = handle
        .query(
            &QueryRequest::Diff {
                path: "src/lib.rs".into(),
                from_ref: "missing-reference".into(),
                to_ref: None,
            },
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(unknown.kind(), QueryRefusalKind::GitUnavailable);
}

#[test]
fn parity_file_search_filters_tests_before_limit_and_rejects_bad_ranges() {
    let root = fixture();
    for index in 0..55 {
        fs::write(
            root.path().join(format!("tests/needle_{index:02}.rs")),
            "fn fixture() {}\n",
        )
        .unwrap();
    }
    fs::write(root.path().join("src/needle_zz.rs"), "fn production() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let found = handle
        .query(
            &QueryRequest::SearchFiles {
                query: "needle".into(),
                path_prefix: None,
                include_tests: false,
            },
            QueryLimits {
                max_results: 1,
                max_bytes: 4096,
            },
        )
        .unwrap();
    let QueryOutput::Files(files) = found.value() else {
        panic!("file search")
    };
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "src/needle_zz.rs");
    assert!(!found.truncated());
    for (start_line, end_line) in [(Some(0), None), (Some(3), Some(2)), (Some(999), None)] {
        let error = handle
            .query(
                &QueryRequest::File {
                    path: "src/lib.rs".into(),
                    start_line,
                    end_line,
                },
                QueryLimits::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), QueryRefusalKind::InvalidRequest);
    }
    let mut selector = target();
    selector.line = Some(2);
    let symbol = handle
        .query(&QueryRequest::Symbol { selector }, QueryLimits::default())
        .unwrap();
    assert!(matches!(symbol.value(), QueryOutput::Symbol(symbol) if symbol.start_line == 2));
}

#[test]
fn parity_symbol_selectors_share_search_kind_aliases_and_disambiguate_lines() {
    let root = fixture();
    fs::write(root.path().join("src/aliases.rs"), "pub struct A;\npub struct B;\nimpl A { pub fn same(&self) {} }\nimpl B { pub fn same(&self) {} }\npub fn same() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let mut selector = QuerySymbolSelector {
        path: "src/aliases.rs".into(),
        name: "same".into(),
        kind: Some("function".into()),
        line: None,
    };
    let function = handle
        .query(
            &QueryRequest::Symbol {
                selector: selector.clone(),
            },
            QueryLimits::default(),
        )
        .unwrap();
    assert!(
        matches!(function.value(), QueryOutput::Symbol(symbol) if symbol.start_line == 5 && symbol.kind == "function")
    );
    selector.kind = Some("method".into());
    assert_eq!(
        handle
            .query(
                &QueryRequest::Symbol {
                    selector: selector.clone()
                },
                QueryLimits::default()
            )
            .unwrap_err()
            .kind(),
        QueryRefusalKind::AmbiguousSymbol
    );
    selector.line = Some(4);
    let method = handle
        .query(&QueryRequest::Symbol { selector }, QueryLimits::default())
        .unwrap();
    assert!(
        matches!(method.value(), QueryOutput::Symbol(symbol) if symbol.start_line == 4 && symbol.kind == "method")
    );
    let methods = handle
        .query(
            &QueryRequest::SearchSymbols {
                query: Some("same".into()),
                path_prefix: Some("src/aliases.rs".into()),
                kind: Some("method".into()),
                include_tests: true,
            },
            QueryLimits::default(),
        )
        .unwrap();
    assert!(matches!(methods.value(), QueryOutput::Symbols(symbols) if symbols.len() == 2));
}

#[test]
fn parity_graph_too_small_to_progress_refuses_and_same_line_symbols_keep_identity() {
    let root = fixture();
    fs::write(
        root.path().join("src/same_line.rs"),
        "struct A; struct B; impl A { fn same(&self) {} } impl B { fn same(&self) {} }\n",
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let error = handle
        .query(
            &QueryRequest::Graph {
                path_prefix: None,
                offset: 0,
                expected_publication: None,
            },
            QueryLimits {
                max_results: 1,
                max_bytes: 1,
            },
        )
        .unwrap_err();
    assert_eq!(error.kind(), QueryRefusalKind::BudgetTooSmall);
    let symbols = handle
        .query(
            &QueryRequest::SearchSymbols {
                query: Some("same".into()),
                path_prefix: Some("src/same_line.rs".into()),
                kind: Some("method".into()),
                include_tests: true,
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Symbols(symbols) = symbols.value() else {
        panic!("symbols")
    };
    assert_eq!(symbols.len(), 2);
    assert_ne!(symbols[0].identity, symbols[1].identity);
    assert_ne!(symbols[0].symbol_index, symbols[1].symbol_index);
}

#[test]
fn parity_query_control_refuses_cancelled_and_expired_work_without_partial_success() {
    use symforge::embed::parity::host::OperationControl;

    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let request = QueryRequest::Graph {
        path_prefix: None,
        offset: 0,
        expected_publication: None,
    };
    let control = OperationControl::new(Duration::from_secs(1)).unwrap();
    control.clone().cancel();
    assert_eq!(
        handle
            .query_with_control(&request, QueryLimits::default(), &control)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::Cancelled
    );
    let expired = OperationControl::new(Duration::from_nanos(1)).unwrap();
    std::thread::sleep(Duration::from_millis(1));
    assert_eq!(
        handle
            .query_with_control(&request, QueryLimits::default(), &expired)
            .unwrap_err()
            .kind(),
        QueryRefusalKind::DeadlineExceeded
    );
    let available = OperationControl::new(Duration::from_secs(60)).unwrap();
    let claim = handle
        .query_with_control(&request, QueryLimits::default(), &available)
        .unwrap();
    assert!(matches!(claim.value(), QueryOutput::Graph(graph) if !graph.files.is_empty()));
}

#[test]
fn parity_guidance_preserves_shared_exploration_depth_filters_and_conventions() {
    use symforge::embed::parity::guidance::ExploreRequest;
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let mut request = ExploreRequest::new("target");
    request.depth = 3;
    request.path_prefix = Some("src/".into());
    request.language = Some("rust".into());
    let explored = handle
        .query(
            &QueryRequest::Explore(request.clone()),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Exploration(exploration) = explored.value() else {
        panic!("exploration")
    };
    assert_eq!(exploration.depth, 3);
    assert!(
        exploration
            .symbols
            .iter()
            .any(|symbol| symbol.name == "target" && symbol.signature.is_some())
    );
    assert!(
        exploration
            .symbols
            .iter()
            .all(|symbol| symbol.path.starts_with("src/") && symbol.score_millionths <= 1_000_000)
    );
    let conventions = handle
        .query(&QueryRequest::Conventions, QueryLimits::default())
        .unwrap();
    let QueryOutput::Conventions(conventions) = conventions.value() else {
        panic!("conventions")
    };
    assert!(conventions.language.contains("Rust"));
    assert!(!conventions.naming.is_empty());
    request.language = Some("not-a-language".into());
    assert_eq!(
        handle
            .query(&QueryRequest::Explore(request), QueryLimits::default())
            .unwrap_err()
            .kind(),
        QueryRefusalKind::UnsupportedOption
    );
}

#[test]
fn parity_explore_supports_estimate_and_token_capped_retrieval() {
    use symforge::embed::parity::guidance::ExploreRequest;
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let mut request = ExploreRequest::new("target");
    request.depth = 2;
    request.estimate = Some(true);
    let estimate = handle
        .query(
            &QueryRequest::Explore(request.clone()),
            QueryLimits::default(),
        )
        .unwrap();
    assert!(
        matches!(estimate.value(), QueryOutput::SearchEstimate(value) if value.approximate_tokens == 1_500)
    );

    request.estimate = None;
    request.max_tokens = Some(40);
    let capped = handle
        .query(&QueryRequest::Explore(request), QueryLimits::default())
        .unwrap();
    assert!(capped.truncated());
}

#[test]
fn parity_request_deserialization_refuses_unknown_options() {
    assert!(serde_json::from_str::<QueryRequest>(r#"{"SearchText":{"query":"target","path_prefix":null,"regex":false,"case_sensitive":false,"include_tests":false,"whole_word":true}}"#).is_err());
    assert!(
        serde_json::from_str::<QueryLimits>(
            r#"{"max_results":100,"max_bytes":4096,"ignore_bounds":true}"#
        )
        .is_err()
    );
}
