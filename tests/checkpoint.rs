use std::fs;

use symforge::live_index::{LiveIndex, persist};

fn state_placement(root: &std::path::Path) -> symforge::domain::StatePlacement {
    let binding = match symforge::discovery::resolve_root_candidate(
        root,
        symforge::domain::RootCandidateSource::LaunchCwd,
        symforge::domain::RootRequestMode::Automatic,
    ) {
        symforge::domain::RootResolution::Bound(binding) => binding,
        resolution => panic!("fixture root should bind: {resolution:?}"),
    };
    symforge::discovery::resolve_state_placement(&binding)
}

fn write_file(root: &std::path::Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, contents).expect("write fixture");
}

fn index_for_root(root: &std::path::Path) -> symforge::live_index::SharedIndex {
    LiveIndex::load(root).expect("load fixture index")
}

#[test]
fn checkpoint_shared_index_writes_current_index_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_file(
        temp.path(),
        "src/lib.rs",
        "pub fn alpha() -> usize {\n    1\n}\n",
    );
    let index = index_for_root(temp.path());
    let placement = state_placement(temp.path());

    let report = persist::checkpoint_shared_index(&index, temp.path(), &placement)
        .expect("checkpoint should succeed");

    assert_eq!(report.files, 1);
    assert!(report.bytes > 0, "checkpoint should report written bytes");
    assert!(
        temp.path().join(".symforge").join("index.bin").exists(),
        "checkpoint should create .symforge/index.bin"
    );
    let snapshot = persist::load_snapshot(temp.path(), &placement).expect("snapshot should load");
    assert_eq!(snapshot.files.len(), 1);
    assert!(
        snapshot.files.contains_key("src/lib.rs"),
        "snapshot should contain indexed source file"
    );
}

#[test]
fn checkpoint_shared_index_reports_write_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_file(
        temp.path(),
        "src/lib.rs",
        "pub fn alpha() -> usize {\n    1\n}\n",
    );
    let index = index_for_root(temp.path());
    let placement = state_placement(temp.path());
    fs::remove_dir_all(temp.path().join(".symforge")).expect("remove resolved state dir");
    fs::write(temp.path().join(".symforge"), b"not a directory").expect("block .symforge dir");

    let error = persist::checkpoint_shared_index(&index, temp.path(), &placement)
        .expect_err("blocked .symforge path should fail checkpoint");
    let output = error.to_string();

    assert!(
        output.contains("project state directory"),
        "checkpoint write failure should be explicit, got:\n{output}"
    );
    assert!(
        !temp.path().join(".symforge").join("index.bin").exists(),
        "failed checkpoint must not create an index snapshot"
    );
}
