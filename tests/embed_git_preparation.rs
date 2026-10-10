#![cfg(feature = "embed")]

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use symforge::embed::parity::host::OperationControl;
use symforge::embed::parity::source_options::{
    EmbeddedOpenOptions, EmbeddedStateSelection, GitPreparationOptions,
};
use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

fn commit(root: &Path, body: &str) {
    let repo = git2::Repository::open(root).unwrap();
    fs::write(root.join("lib.rs"), body).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("lib.rs")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<_> = parent.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "fixture",
        &tree,
        &parents,
    )
    .unwrap();
}

#[test]
fn memory_only_git_preparation_uses_only_explicit_bounded_scratch_and_reuses_objects() {
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    commit(root.path(), "pub fn first() {}\n");
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let source = runtime
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(root.path().to_path_buf()),
            EmbeddedOpenOptions {
                state: EmbeddedStateSelection::MemoryOnly,
                ..Default::default()
            },
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while source.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let control = OperationControl::new(Duration::from_secs(20)).unwrap();
    let options = GitPreparationOptions::new(scratch.path().to_path_buf(), 8 * 1024 * 1024, 1000);
    let prepared = source.prepare_git_view(&options, &control).unwrap();
    assert!(prepared.copied_bytes() > 0);
    assert!(prepared.copied_files() > 0);
    assert!(!prepared.view_identity().is_empty());
    assert!(!root.path().join(".symforge").exists());
    let reused = source.prepare_git_view(&options, &control).unwrap();
    assert_eq!(prepared.view_identity(), reused.view_identity());
    assert_eq!(reused.copied_bytes(), 0);
    assert!(reused.reused_files() > 0);
    drop(source);
    runtime.begin_shutdown();
    assert_eq!(fs::read_dir(scratch.path()).unwrap().count(), 0);
}

#[test]
fn git_preparation_obeys_cancellation_and_capacity_before_publishing_a_view() {
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    commit(root.path(), "pub fn bounded() {}\n");
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let source = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            root.path().to_path_buf(),
        ))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while source.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let control = OperationControl::new(Duration::from_secs(20)).unwrap();
    let options = GitPreparationOptions::new(scratch.path().to_path_buf(), 1, 1);
    let refusal = source.prepare_git_view(&options, &control).unwrap_err();
    assert_eq!(refusal.kind_name(), "CapacityExceeded");
    assert_eq!(fs::read_dir(scratch.path()).unwrap().count(), 0);
    control.cancel();
    let refusal = source.prepare_git_view(&options, &control).unwrap_err();
    assert_eq!(refusal.kind_name(), "Cancelled");
    assert_eq!(fs::read_dir(scratch.path()).unwrap().count(), 0);
}
