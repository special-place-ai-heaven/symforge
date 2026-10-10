#![cfg(feature = "embed")]
//! Git temporal data for embedded queries: with derived-state preparation
//! permitted, the source's history is walked by MCP's producer and aggregator,
//! so `edit_plan` reports co-change partners and `search_files(changed_with)`
//! reports coupling. Without permission the state is Unavailable with a
//! reason, never "loading".

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::embed::parity::search::{CoChangeState, FileSearchRequest};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryPolicy, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn commit(repository: &git2::Repository, root: &Path, files: &[(&str, &str)], message: &str) {
    let mut index = repository.index().unwrap();
    for (path, content) in files {
        fs::write(root.join(path), content).unwrap();
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    let parent = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .unwrap();
}

/// `src/lib.rs` and `src/other.rs` change together in three commits;
/// `src/lone.rs` changes alone.
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let repository = git2::Repository::init(root.path()).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    commit(
        &repository,
        root.path(),
        &[
            (".gitignore", ".symforge/\n"),
            ("src/lib.rs", "pub fn number() -> u32 { 1 }\n"),
            ("src/other.rs", "pub fn other() -> u32 { 1 }\n"),
            ("src/lone.rs", "pub fn lone() -> u32 { 1 }\n"),
        ],
        "root",
    );
    for round in 2..=3 {
        commit(
            &repository,
            root.path(),
            &[
                (
                    "src/lib.rs",
                    &format!("pub fn number() -> u32 {{ {round} }}\n"),
                ),
                (
                    "src/other.rs",
                    &format!("pub fn other() -> u32 {{ {round} }}\n"),
                ),
            ],
            "couple lib and other",
        );
    }
    commit(
        &repository,
        root.path(),
        &[("src/lone.rs", "pub fn lone() -> u32 { 2 }\n")],
        "lone",
    );
    root
}

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source never reached Current");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

fn query(handle: &EmbeddedSourceHandle, request: QueryRequest, permit: bool) -> QueryOutput {
    handle
        .query_with_policy(
            &request,
            QueryLimits::default(),
            QueryPolicy {
                allow_derived_state_preparation: permit,
            },
            None,
            None,
        )
        .unwrap()
        .value()
        .clone()
}

#[test]
fn edit_plan_reports_co_change_partners_only_when_history_may_be_walked() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let plan = |permit| {
        let QueryOutput::EditPlan(plan) = query(
            &handle,
            QueryRequest::EditPlan {
                target: "number".into(),
            },
            permit,
        ) else {
            panic!("edit plan");
        };
        plan.rendered
    };
    let permitted = plan(true);
    assert!(
        permitted.contains("Co-change partners: src/other.rs"),
        "{permitted}"
    );
    // MCP's terse plan omits the line when temporal data is not Ready.
    assert!(!plan(false).contains("Co-change partners"));
}

#[test]
fn changed_with_reports_strong_coupling_or_an_honest_unavailable_reason() {
    let root = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let search = |permit| {
        let QueryOutput::FileSearch(report) = query(
            &handle,
            QueryRequest::FileSearch(FileSearchRequest {
                changed_with: Some("src/lib.rs".into()),
                ..Default::default()
            }),
            permit,
        ) else {
            panic!("file search");
        };
        report
    };
    let permitted = search(true);
    let summary = permitted.cochange.as_ref().unwrap();
    assert_eq!(summary.state, CoChangeState::Strong);
    assert_eq!(summary.analyzed_commits, 4);
    assert_eq!(permitted.hits[0].path, "src/other.rs");

    let refused = search(false);
    assert!(matches!(
        &refused.cochange.as_ref().unwrap().state,
        CoChangeState::Unavailable { reason } if reason.contains("derived-state")
    ));
}
