use std::fs;

use symforge::live_index::{LiveIndex, persist};

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

    let report =
        persist::checkpoint_shared_index(&index, temp.path()).expect("checkpoint should succeed");

    assert_eq!(report.files, 1);
    assert!(report.bytes > 0, "checkpoint should report written bytes");
    assert!(
        temp.path().join(".symforge").join("index.bin").exists(),
        "checkpoint should create .symforge/index.bin"
    );
    let snapshot = persist::load_snapshot(temp.path()).expect("snapshot should load");
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
    fs::write(temp.path().join(".symforge"), b"not a directory").expect("block .symforge dir");
    let index = index_for_root(temp.path());

    let error = persist::checkpoint_shared_index(&index, temp.path())
        .expect_err("blocked .symforge path should fail checkpoint");
    let output = error.to_string();

    assert!(
        output.contains("symforge data dir"),
        "checkpoint write failure should be explicit, got:\n{output}"
    );
    assert!(
        !temp.path().join(".symforge").join("index.bin").exists(),
        "failed checkpoint must not create an index snapshot"
    );
}
