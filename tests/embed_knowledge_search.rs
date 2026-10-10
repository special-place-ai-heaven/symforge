#![cfg(feature = "embed")]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::parity::knowledge::KnowledgeSourceScope;
use symforge::embed::parity::knowledge::search::SearchKnowledgeRequest;
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

fn fixture() -> tempfile::TempDir {
    let repository = tempfile::tempdir().unwrap();
    git2::Repository::init(repository.path()).unwrap();
    fs::create_dir(repository.path().join("docs")).unwrap();
    fs::write(
        repository.path().join("docs/architecture.md"),
        "# Architecture\nThe byte exact source is the persistence boundary.\n",
    )
    .unwrap();
    fs::write(
        repository.path().join("docs/other.md"),
        "# Other\nAnother unrelated design note.\n",
    )
    .unwrap();
    repository
}

fn request(query: &str) -> SearchKnowledgeRequest {
    SearchKnowledgeRequest {
        query: query.to_string(),
        path_prefix: Some("docs/architecture.md".to_string()),
        source_scope: Some(KnowledgeSourceScope::Current),
        authority_scope: None,
        project: None,
        projects: None,
        limit: Some(10),
        max_tokens: None,
    }
}

#[test]
fn embedded_knowledge_search_keeps_shared_evidence_and_typed_provenance() {
    let repository = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < deadline, "source did not publish");
        std::thread::sleep(Duration::from_millis(5));
    }
    let claim = handle
        .query(
            &QueryRequest::SearchKnowledge(request("persistence boundary")),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::SearchKnowledge(result) = claim.value() else {
        panic!("wrong query projection")
    };
    assert_eq!(result.source_scope, KnowledgeSourceScope::Current);
    assert_eq!(result.hit_count, 1);
    assert_eq!(result.rendered_hit_count, 1);
    assert_eq!(result.sources.len(), 1);
    assert!(result.sources[0].envelope.is_some());
    assert!(result.rendered.contains("docs/architecture.md"));
    assert!(result.rendered.contains("persistence boundary"));
    assert!(!result.rendered.contains("docs/other.md"));
    assert!(!result.truncated);

    let mut foreign = request("persistence boundary");
    foreign.project = Some("another-room".to_string());
    let refusal = handle
        .query(
            &QueryRequest::SearchKnowledge(foreign),
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(refusal.kind(), QueryRefusalKind::UnsupportedOption);
}
