#![cfg(feature = "embed")]

#[test]
fn usage_counts_decoded_strings_and_raw_bytes_without_json_escape_expansion() {
    use symforge::embed::parity::read::{FileContentRequest, SourcePageRequest};
    let root = tempfile::tempdir().unwrap();
    let source = "// é \\\\ \"quoted\"\r\npub fn item() {}\r\n";
    fs::write(root.path().join("unicode.rs"), source).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let read = handle
        .query(
            &QueryRequest::FileContent(FileContentRequest {
                path: "unicode.rs".into(),
                ..Default::default()
            }),
            QueryLimits {
                max_results: 1,
                max_bytes: 512,
            },
        )
        .unwrap();
    let QueryOutput::FileContent(content) = read.value() else {
        panic!("content");
    };
    let expected = content.path.len()
        + content.content_hash.len()
        + content.rendered.len()
        + "PublishedGeneration".len();
    assert_eq!(read.usage().decoded_bytes, expected as u64);
    assert_eq!(read.usage().rows, 1);
    assert!(serde_json::to_vec(read.value()).unwrap().len() > expected);
    let page = handle
        .query(
            &QueryRequest::SourcePage(SourcePageRequest {
                path: "unicode.rs".into(),
                offset: 0,
                expected_publication: None,
            }),
            QueryLimits {
                max_results: 1,
                max_bytes: 512,
            },
        )
        .unwrap();
    let QueryOutput::SourcePage(content) = page.value() else {
        panic!("page");
    };
    assert_eq!(
        page.usage().decoded_bytes,
        (content.path.len() + content.content_hash.len() + content.bytes.len()) as u64
    );
    assert_eq!(content.bytes, source.as_bytes());
}
use std::{
    fs,
    time::{Duration, Instant},
};
use symforge::embed::parity::federation::{
    AdmittedQuerySource, FederatedSourceOutcome, FederationRefusalKind, SourceSelection,
    query_sources,
};
use symforge::embed::parity::search::SymbolSearchRequest;
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryPolicy, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};
fn open(runtime: &ProcessIndexRuntime, path: &std::path::Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(path.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}
#[test]
fn admitted_federation_keeps_source_claims_and_refuses_unknown_alias_before_execution() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    fs::write(a.path().join("lib.rs"), "pub fn widget_alpha() {}\n").unwrap();
    fs::write(b.path().join("lib.rs"), "pub fn widget_beta() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let a = open(&runtime, a.path());
    let b = open(&runtime, b.path());
    let a_session = a.new_query_session().unwrap();
    let b_session = b.new_query_session().unwrap();
    let sources = [
        AdmittedQuerySource {
            label: "beta",
            handle: &b,
            session: Some(&b_session),
            policy: QueryPolicy::default(),
        },
        AdmittedQuerySource {
            label: "alpha",
            handle: &a,
            session: Some(&a_session),
            policy: QueryPolicy::default(),
        },
    ];
    let query = QueryRequest::SymbolSearch(SymbolSearchRequest {
        query: Some("widget".into()),
        ..Default::default()
    });
    let refusal = query_sources(
        &sources,
        &SourceSelection::Many(vec!["alpha".into(), "foreign".into()]),
        &query,
        QueryLimits::default(),
        None,
    )
    .unwrap_err();
    assert_eq!(refusal.kind, FederationRefusalKind::UnknownSource);
    let inventory = a
        .query_with_session(
            &QueryRequest::ContextInventory,
            QueryLimits::default(),
            &a_session,
            None,
        )
        .unwrap();
    let QueryOutput::ContextInventory(inventory) = inventory.value() else {
        panic!("inventory");
    };
    assert!(inventory.listed_symbols.is_empty());
    let result = query_sources(
        &sources,
        &SourceSelection::All,
        &query,
        QueryLimits::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        result
            .results
            .iter()
            .map(|r| r.source.label.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(result.resolved_sources.len(), 2);
    let mut publications = Vec::new();
    for row in &result.results {
        let FederatedSourceOutcome::Claim(claim) = &row.outcome else {
            panic!("source claim");
        };
        let QueryOutput::SymbolSearch(found) = claim.value() else {
            panic!("symbols");
        };
        assert_eq!(found.symbols.len(), 1);
        assert!(found.symbols[0].symbol.name.ends_with(&row.source.label));
        publications.push(claim.publication_identity());
    }
    assert_ne!(publications[0], publications[1]);
    assert!(result.usage.rows <= 100);
    assert!(result.usage.decoded_bytes <= 65_536);
}
#[test]
fn federation_aggregate_cap_and_duplicate_alias_are_deterministic() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn widget() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let sources = [AdmittedQuerySource {
        label: "one",
        handle: &handle,
        session: None,
        policy: QueryPolicy::default(),
    }];
    let query = QueryRequest::SymbolSearch(SymbolSearchRequest {
        query: Some("widget".into()),
        ..Default::default()
    });
    let result = query_sources(
        &sources,
        &SourceSelection::Many(vec!["one".into(), "one".into()]),
        &query,
        QueryLimits {
            max_results: 2,
            max_bytes: 2000,
        },
        None,
    )
    .unwrap();
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.resolved_sources.len(), 1);
    assert!(result.usage.rows <= 2);
    assert!(result.usage.decoded_bytes <= 2000);
}
