//! Generation-fence tests for `SharedIndexHandle` git temporal publication.

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use git2::{IndexAddOption, Repository, Signature};
use symforge::domain::HistoryLimit;
use symforge::live_index::LiveIndex;
use symforge::live_index::git_temporal::{GitTemporalIndex, GitTemporalState};
use tempfile::tempdir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("SymForge test", "symforge@example.invalid").unwrap();
    let parent = repo
        .head()
        .ok()
        .and_then(|head| head.target())
        .map(|oid| repo.find_commit(oid).unwrap());
    let parents = parent.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .unwrap();
}

#[test]
fn stale_temporal_publication_rejected() {
    let dir_a = tempdir().unwrap();
    write_file(dir_a.path(), "a/file.rs", "pub fn from_a() {}\n");
    let shared = LiveIndex::load(dir_a.path()).unwrap();
    let fence_a = shared.git_temporal_publication_fence();

    let (release_tx, release_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let stale_shared = shared.clone();
    let stale_worker = thread::spawn(move || {
        release_rx.recv().unwrap();
        let published = stale_shared.update_git_temporal_at_fence(
            GitTemporalIndex::unavailable("root-a".to_string()),
            &fence_a,
        );
        result_tx.send(published).unwrap();
    });

    let dir_b = tempdir().unwrap();
    write_file(dir_b.path(), "b/file.rs", "pub fn from_b() {}\n");
    shared.reload(dir_b.path()).unwrap();

    release_tx.send(()).unwrap();
    stale_worker.join().unwrap();

    assert!(
        !result_rx.recv().unwrap(),
        "stale git temporal publication must be rejected"
    );
    assert_ne!(
        shared.git_temporal().state,
        GitTemporalState::Unavailable("root-a".to_string()),
        "stale A-era temporal data must not replace B-era state"
    );
}

#[test]
fn current_temporal_publication_allowed() {
    let dir_a = tempdir().unwrap();
    write_file(dir_a.path(), "a/file.rs", "pub fn from_a() {}\n");
    let shared = LiveIndex::load(dir_a.path()).unwrap();

    let dir_b = tempdir().unwrap();
    write_file(dir_b.path(), "b/file.rs", "pub fn from_b() {}\n");
    shared.reload(dir_b.path()).unwrap();
    let fence_b = shared.git_temporal_publication_fence();

    let published = shared.update_git_temporal_at_fence(
        GitTemporalIndex::unavailable("root-b".to_string()),
        &fence_b,
    );

    assert!(
        published,
        "current generation must allow git temporal publication"
    );
    assert_eq!(
        shared.git_temporal().state,
        GitTemporalState::Unavailable("root-b".to_string())
    );
}

#[test]
fn content_generation_change_rejects_temporal_result_from_same_project() {
    let dir = tempdir().unwrap();
    write_file(dir.path(), "src/lib.rs", "pub fn original() {}\n");
    let shared = LiveIndex::load(dir.path()).unwrap();
    let stale_fence = shared.git_temporal_publication_fence();

    shared.remove_file("src/lib.rs");

    assert!(!shared.update_git_temporal_at_fence(
        GitTemporalIndex::unavailable("stale-content".to_string()),
        &stale_fence,
    ));
    assert_ne!(
        shared.git_temporal().state,
        GitTemporalState::Unavailable("stale-content".to_string())
    );
}

#[test]
fn bytes_identical_commit_rejects_old_source_version_and_publishes_new_tip() {
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    write_file(dir.path(), "src/lib.rs", "pub fn stable() {}\n");
    commit_all(&repo, "initial");
    let shared = LiveIndex::load(dir.path()).unwrap();
    let stale_fence = shared.git_temporal_publication_fence();
    let old_tip = stale_fence.source_version.as_ref().unwrap().commit.clone();

    commit_all(&repo, "bytes-identical successor");

    assert!(!shared.update_git_temporal_at_fence(
        GitTemporalIndex::unavailable("stale-tip".to_string()),
        &stale_fence,
    ));
    let current = shared.published_generation();
    assert_ne!(
        current.source_version.as_ref().unwrap().commit,
        old_tip,
        "publication must converge on the accepted bytes-identical successor tip"
    );
    assert_eq!(
        current.content_generation, stale_fence.content_generation,
        "a bytes-identical commit must not mint a content generation"
    );
}

#[test]
fn accepted_temporal_index_reports_window_and_rename_coverage_limits() {
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    write_file(dir.path(), "src/lib.rs", "pub fn history() {}\n");
    commit_all(&repo, "initial");
    let shared = LiveIndex::load(dir.path()).unwrap();

    shared.update_git_temporal(GitTemporalIndex::compute(dir.path()));

    let coverage = &shared.published_generation().code_signals.coverage;
    assert!(!coverage.complete_to_root);
    assert!(coverage.limitations.contains(&HistoryLimit::WindowLimited));
    assert!(
        coverage
            .limitations
            .contains(&HistoryLimit::RenameFollowLimited)
    );
}
