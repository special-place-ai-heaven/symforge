use std::collections::HashMap;
use std::fs;

use symforge::domain::{LanguageId, SymbolKind, SymbolRecord};
use symforge::live_index::{IndexState, IndexedFile, LiveIndex, ParseStatus, persist};

const GENUINE_LIB_RS: &str = "pub const GENUINE_CHECKPOINT_MARKER: &str = \"symforge-genuine-artifact-v1\";\n";
const BOOTSTRAP_ONLY_RS: &str =
    "pub const UNVOUCHED_BOOTSTRAP_ATTACK_MARKER: &str = \"symforge-unvouched-bootstrap-v1\";\n";

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

fn index_bin_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".symforge").join("index.bin")
}

fn read_index_bin_bytes(root: &std::path::Path) -> Vec<u8> {
    let bytes = fs::read(index_bin_path(root)).expect("read index.bin");
    assert!(!bytes.is_empty(), "genuine checkpoint artifact must be non-empty");
    bytes
}

fn make_rust_indexed_file(path: &str, content: &str) -> IndexedFile {
    let content_bytes = content.as_bytes().to_vec();
    let byte_len = content_bytes.len() as u64;
    IndexedFile {
        relative_path: path.to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path(path),
        content: content_bytes.clone(),
        symbols: vec![SymbolRecord {
            name: "marker".to_string(),
            kind: SymbolKind::Constant,
            depth: 0,
            sort_order: 0,
            byte_range: (0, byte_len.min(u32::MAX as u64) as u32),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len,
        content_hash: symforge::hash::digest_hex(&content_bytes),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    }
}

fn assert_snapshot_contains_genuine_artifact(
    root: &std::path::Path,
    placement: &symforge::domain::StatePlacement,
) {
    let snapshot = persist::load_snapshot(root, placement).expect("snapshot should load");
    let genuine = snapshot
        .files
        .get("src/lib.rs")
        .expect("snapshot should contain src/lib.rs");
    assert_eq!(
        String::from_utf8_lossy(&genuine.content),
        GENUINE_LIB_RS,
        "snapshot should preserve the genuine indexed file content"
    );
    assert!(
        !snapshot.files.contains_key("src/bootstrap_only.rs"),
        "snapshot must not contain the unvouched bootstrap-only path"
    );
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
        index_bin_path(temp.path()).exists(),
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
        !index_bin_path(temp.path()).exists(),
        "failed checkpoint must not create an index snapshot"
    );
}

fn assert_manifest_withheld(shared: &symforge::live_index::SharedIndex) {
    assert!(
        shared.published_generation().manifest.is_none(),
        "refuse precondition: attacker must publish manifest=None before checkpoint"
    );
}

#[test]
fn unvouched_empty_bootstrap_checkpoint_must_not_overwrite_genuine_artifact() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_file(temp.path(), "src/lib.rs", GENUINE_LIB_RS);

    let rooted = index_for_root(temp.path());
    assert_eq!(
        rooted.read().index_state(),
        IndexState::Ready,
        "rooted fixture index must be Ready before checkpointing"
    );

    let placement = state_placement(temp.path());
    persist::checkpoint_shared_index(&rooted, temp.path(), &placement)
        .expect("genuine rooted checkpoint should succeed");

    let genuine_bytes = read_index_bin_bytes(temp.path());
    assert_snapshot_contains_genuine_artifact(temp.path(), &placement);

    // Scenario: populated EmptyBootstrap/unvouched generation against the same placement.
    let unvouched = LiveIndex::empty();
    unvouched.update_file(
        "src/bootstrap_only.rs".to_string(),
        make_rust_indexed_file("src/bootstrap_only.rs", BOOTSTRAP_ONLY_RS),
    );
    assert_manifest_withheld(&unvouched);

    let unvouched_result =
        persist::checkpoint_shared_index(&unvouched, temp.path(), &placement);

    assert!(
        unvouched_result.is_err(),
        "checkpoint must refuse when published.manifest is None against an existing genuine artifact, got Ok({unvouched_result:?})"
    );

    let after_bytes = read_index_bin_bytes(temp.path());
    assert_eq!(
        after_bytes, genuine_bytes,
        "index.bin bytes must remain identical to the genuine artifact after the refused unvouched checkpoint"
    );
    assert_snapshot_contains_genuine_artifact(temp.path(), &placement);
}

/// Optional ALLOW inversion guard when `RepositoryManifest::new` succeeded
/// (`published.manifest.is_some()`). Do not treat source-Some/manifest-None
/// (Complete-mint hole) as allow.
#[test]
fn manifest_vouched_checkpoint_is_allowed() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_file(temp.path(), "src/lib.rs", GENUINE_LIB_RS);

    let manifest_vouched = LiveIndex::from_indexed_files(
        temp.path(),
        vec![(
            "src/lib.rs".to_string(),
            make_rust_indexed_file("src/lib.rs", GENUINE_LIB_RS),
        )],
    )
    .expect("from_indexed_files should publish a manifest at the fixture root");

    assert!(
        manifest_vouched.published_generation().manifest.is_some(),
        "ALLOW fixture must publish manifest=Some before checkpoint"
    );

    let placement = state_placement(temp.path());
    persist::checkpoint_shared_index(&manifest_vouched, temp.path(), &placement)
        .expect("checkpoint must allow when published.manifest is Some");
}

#[test]
fn rooted_checkpoint_of_changed_content_updates_artifact_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_file(temp.path(), "src/lib.rs", GENUINE_LIB_RS);

    let rooted = index_for_root(temp.path());
    let placement = state_placement(temp.path());

    persist::checkpoint_shared_index(&rooted, temp.path(), &placement)
        .expect("initial rooted checkpoint should succeed");
    let initial_bytes = read_index_bin_bytes(temp.path());

    write_file(
        temp.path(),
        "src/lib.rs",
        "pub const GENUINE_CHECKPOINT_MARKER: &str = \"symforge-genuine-artifact-v2\";\n",
    );
    rooted
        .reload(temp.path())
        .expect("rooted reload after source change should succeed");
    persist::checkpoint_shared_index(&rooted, temp.path(), &placement)
        .expect("second rooted checkpoint should succeed");

    let updated_bytes = read_index_bin_bytes(temp.path());
    assert_ne!(
        updated_bytes, initial_bytes,
        "a rooted checkpoint of changed content must update index.bin bytes"
    );
}
