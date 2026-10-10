#![cfg(feature = "embed")]

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use symforge::embed::parity::ask::{AskRequest, AskRoute};
use symforge::embed::parity::{QueryLimits, QueryOperationKind, QueryOutput, QueryRequest};
use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

#[test]
fn ask_executes_all_shared_routes_under_one_source_and_one_outer_session_operation() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub trait Service { fn serve(&self); }\npub struct Concrete;\nimpl Service for Concrete { fn serve(&self) {} }\npub fn selected_task() {}\npub fn caller() { selected_task(); }\n").unwrap();
    fs::write(
        root.path().join("README.md"),
        "# Architecture\nThe service provides fixture handling.\n",
    )
    .unwrap();
    fs::write(root.path().join(".gitignore"), ".symforge/\n").unwrap();
    let repo = git2::Repository::init(root.path()).unwrap();
    let mut index = repo.index().unwrap();
    for path in ["lib.rs", "README.md", ".gitignore"] {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
        .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            root.path().to_path_buf(),
        ))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let session = handle.new_query_session().unwrap();
    let routes = [
        ("who calls selected_task", AskRoute::FindCallers),
        ("find symbol selected_task", AskRoute::FindSymbol),
        ("find file lib.rs", AskRoute::FindFile),
        ("what changed", AskRoute::FindChanges),
        ("how does unrelated pipeline work", AskRoute::Understand),
        ("how does selected_task work", AskRoute::UnderstandSymbol),
        (
            "explain the Service implementations",
            AskRoute::UnderstandImplementations,
        ),
        ("grep selected_task", AskRoute::SearchCode),
        (
            "search repository knowledge for architecture",
            AskRoute::SearchKnowledge,
        ),
        (
            "orient me in this repository",
            AskRoute::RepositoryOrientation,
        ),
        ("what depends on lib.rs", AskRoute::FindDependents),
        ("implementations of Service", AskRoute::FindImplementations),
        (
            "which tool should I use for impact analysis",
            AskRoute::ToolHelp,
        ),
        ("error handling patterns", AskRoute::Explore),
    ];
    for (index, (question, expected)) in routes.into_iter().enumerate() {
        let claim = handle
            .query_with_session(
                &QueryRequest::Ask(AskRequest {
                    query: question.into(),
                    ..Default::default()
                }),
                QueryLimits::default(),
                &session,
                None,
            )
            .unwrap();
        assert_eq!(claim.operation(), QueryOperationKind::Ask);
        let QueryOutput::Ask(result) = claim.value() else {
            panic!("ask output")
        };
        assert_eq!(result.route, expected, "{question}");
        assert!(!result.invocation.is_empty());
        assert!(!result.rationale.is_empty());
        assert!(result.rendered.contains("Route confidence:"));
        if expected == AskRoute::ToolHelp {
            assert!(result.output.is_none());
            assert!(
                result.rendered.contains("detect_impact")
                    || result.rendered.contains("analyze_file_impact")
            );
        } else {
            assert!(
                result.output.is_some(),
                "{question} must execute its selected query"
            );
            assert!(result.routed_request.is_some());
        }
        assert_eq!(
            claim.session_evidence().unwrap().revision_before,
            index as u64
        );
        assert_eq!(
            claim.session_evidence().unwrap().revision_after,
            index as u64 + 1
        );
    }
    let inventory = handle
        .query_with_session(
            &QueryRequest::ContextInventory,
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::ContextInventory(inventory) = inventory.value() else {
        panic!("inventory")
    };
    assert!(
        inventory.fetched_symbols.iter().any(|symbol| symbol.name == "selected_task"),
        "Ask records context actually returned by its routed query"
    );
    assert!(serde_json::from_value::<AskRequest>(
        serde_json::json!({"query":"what changed","ambient_projects":true})
    )
    .is_err());
}
