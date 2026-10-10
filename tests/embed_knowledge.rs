#![cfg(feature = "embed")]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::parity::knowledge::{
    KnowledgeSourceScope, ReviewKnowledgeInput, ReviewKnowledgeMode,
};
use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

#[test]
fn embedded_review_uses_current_source_dossier_and_stable_hashes() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("docs")).expect("create docs");
    fs::create_dir_all(repository.path().join("src")).expect("create source");
    fs::write(
        repository.path().join("docs/architecture.md"),
        "# Architecture\nCurrent implementation links `src/lib.rs`.\n",
    )
    .expect("write document");
    fs::write(repository.path().join("src/lib.rs"), "pub fn anchor() {}\n").expect("write code");

    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    let deadline = Instant::now() + Duration::from_secs(15);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < deadline, "embedded source did not publish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let request = ReviewKnowledgeInput {
        mode: ReviewKnowledgeMode::Document,
        path: Some("docs/architecture.md".to_string()),
        path_prefix: None,
        source_scope: Some(KnowledgeSourceScope::Current),
        project: None,
        projects: None,
        limit: Some(10),
        max_tokens: None,
    };
    let first = handle.review_knowledge(&request).expect("review document");
    let second = handle.review_knowledge(&request).expect("repeat review");
    assert!(first.rendered.contains("mode=document"));
    assert!(
        first
            .rendered
            .contains("code_evidence.consistent_rule_ids=")
    );
    assert_eq!(first.review_hash, second.review_hash);
    assert_eq!(first.result_hash, second.result_hash);
    assert_eq!(first.publication_identity, second.publication_identity);
    assert!(!first.summarized);
}
