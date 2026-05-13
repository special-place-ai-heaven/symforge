//! Generation-fence tests for `SharedIndexHandle` git temporal publication.

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use symforge::live_index::git_temporal::{GitTemporalIndex, GitTemporalState};
use symforge::live_index::LiveIndex;
use tempfile::tempdir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn stale_temporal_publication_rejected() {
    let dir_a = tempdir().unwrap();
    write_file(dir_a.path(), "a/file.rs", "pub fn from_a() {}\n");
    let shared = LiveIndex::load(dir_a.path()).unwrap();
    let gen_a = shared.current_project_generation();

    let (release_tx, release_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let stale_shared = shared.clone();
    let stale_worker = thread::spawn(move || {
        release_rx.recv().unwrap();
        let published = stale_shared.update_git_temporal_at_generation(
            GitTemporalIndex::unavailable("root-a".to_string()),
            gen_a,
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
    let gen_b = shared.current_project_generation();

    let published = shared.update_git_temporal_at_generation(
        GitTemporalIndex::unavailable("root-b".to_string()),
        gen_b,
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
