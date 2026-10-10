#![cfg(feature = "embed")]
use std::fs;
use std::time::{Duration, Instant};
use symforge::embed::parity::search::{
    FileRanking, FileResolution, FileSearchRequest, RankingCapability, RankingStatus,
};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryPolicy, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    for dir in ["src/a", "src/b", "vendor"] {
        fs::create_dir_all(root.path().join(dir)).unwrap();
    }
    fs::write(root.path().join("src/a/lib.rs"), b"pub fn first() {}\n").unwrap();
    fs::write(root.path().join("src/b/lib.rs"), b"pub fn second() {}\n").unwrap();
    fs::write(root.path().join("vendor/lib.rs"), b"pub fn external() {}\n").unwrap();
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
fn search(
    handle: &EmbeddedSourceHandle,
    input: FileSearchRequest,
) -> symforge::embed::parity::QueryClaim {
    handle
        .query(&QueryRequest::FileSearch(input), QueryLimits::default())
        .unwrap()
}
#[test]
fn full_file_search_scopes_counts_and_reports_resolution_ambiguity() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let scoped = search(
        &handle,
        FileSearchRequest {
            query: "lib.rs".into(),
            path_prefix: Some("src/a".into()),
            limit: Some(1),
            ..Default::default()
        },
    );
    let QueryOutput::FileSearch(report) = scoped.value() else {
        panic!("file result");
    };
    assert_eq!(report.hits.len(), 1);
    assert_eq!(report.hits[0].path, "src/a/lib.rs");
    assert_eq!((report.total_matches, report.overflow_count), (1, 0));
    let ambiguous = search(
        &handle,
        FileSearchRequest {
            query: "lib.rs".into(),
            resolve: Some(true),
            ..Default::default()
        },
    );
    let QueryOutput::FileSearch(report) = ambiguous.value() else {
        panic!("resolve result");
    };
    let Some(FileResolution::Ambiguous {
        candidates,
        overflow_count,
    }) = &report.resolution
    else {
        panic!("duplicate basename must remain ambiguous");
    };
    assert_eq!(candidates, &["src/a/lib.rs", "src/b/lib.rs"]);
    assert_eq!(*overflow_count, 0);
    let resolved = search(
        &handle,
        FileSearchRequest {
            query: "src/b/lib.rs".into(),
            resolve: Some(true),
            ..Default::default()
        },
    );
    let QueryOutput::FileSearch(report) = resolved.value() else {
        panic!("resolve result");
    };
    assert!(
        matches!(&report.resolution,Some(FileResolution::Resolved{path,..}) if path=="src/b/lib.rs")
    );
}

#[test]
fn optional_file_ranking_reports_evidence_and_query_json_cannot_grant_preparation() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let ranked = handle
        .query_with_policy(
            &QueryRequest::FileSearch(FileSearchRequest {
                query: "lib.rs".into(),
                rank_by: Some(FileRanking::PathCochange),
                anchor_path: Some("src/a/lib.rs".into()),
                debug_ranking: Some(true),
                ..Default::default()
            }),
            QueryLimits::default(),
            QueryPolicy::default(),
            None,
            None,
        )
        .unwrap();
    let QueryOutput::FileSearch(report) = ranked.value() else {
        panic!("ranked result");
    };
    assert_eq!(report.hits.len(), 2);
    let evidence = report
        .ranking
        .iter()
        .find(|row| row.capability == RankingCapability::CoChangeRanking)
        .expect("requested ranker must report actual state");
    assert!(matches!(
        evidence.status,
        RankingStatus::DisabledByPolicy | RankingStatus::Unavailable | RankingStatus::FallbackUsed
    ));
    assert!(
        report
            .ranking_explanation
            .as_ref()
            .is_some_and(|text| text.contains("path"))
    );
    assert!(
        serde_json::from_str::<FileSearchRequest>(
            r#"{"query":"lib","allow_derived_state_preparation":true}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<FileSearchRequest>(r#"{"query":"lib","rank_by":"invented"}"#)
            .is_err()
    );

    let frecency = search(
        &handle,
        FileSearchRequest {
            query: "lib.rs".into(),
            rank_by: Some(FileRanking::Frecency),
            ..Default::default()
        },
    );
    let QueryOutput::FileSearch(report) = frecency.value() else {
        panic!("ranked result");
    };
    assert!(
        report
            .ranking
            .iter()
            .any(|row| row.capability == RankingCapability::FrecencyRanking)
    );

    let temporal = search(
        &handle,
        FileSearchRequest {
            changed_with: Some("src/a/lib.rs".into()),
            ..Default::default()
        },
    );
    let QueryOutput::FileSearch(report) = temporal.value() else {
        panic!("temporal result");
    };
    assert!(
        report
            .cochange
            .as_ref()
            .is_some_and(|state| state.deprecated_changed_with)
    );
}
