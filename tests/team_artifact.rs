//! team artifact export/import — Program 015 S1a (C-S1A-005).
//!
//! contracts/team-artifact.md (frozen 2026-06-30). Exercises the public
//! `persist::export_artifact` / `persist::load_snapshot` surface end to end,
//! mirroring acceptance criteria A-US2-01..04 (acceptance-matrix.md).

use std::fs;
use std::path::Path;

use symforge::live_index::LiveIndex;
use symforge::live_index::persist;

fn write_fixture(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/lib.rs"),
        b"pub fn core() -> u32 { 1 }\n" as &[u8],
    )
    .expect("write src/lib.rs");
    fs::write(
        root.join("src/a.rs"),
        b"use crate::core;\npub fn call_a() -> u32 { core() }\n" as &[u8],
    )
    .expect("write src/a.rs");
}

#[test]
fn team_artifact_zstd_round_trip_preserves_content_hash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write_fixture(root);

    let shared = LiveIndex::load(root).expect("load index");
    let before_hashes: Vec<(String, String)> = {
        let index = shared.read();
        index
            .all_files()
            .map(|(path, file)| (path.clone(), file.content_hash.clone()))
            .collect()
    };
    assert!(!before_hashes.is_empty(), "fixture should index some files");

    let report = {
        let index = shared.read();
        persist::export_artifact(&index, root).expect("export artifact")
    };
    assert_eq!(report.files, before_hashes.len());

    // A-US2-01: export produces the Best-tier artifact on disk.
    let artifact_path = root.join(".symforge").join(persist::ARTIFACT_FILENAME);
    let metadata_path = root
        .join(".symforge")
        .join(persist::ARTIFACT_METADATA_FILENAME);
    assert!(
        artifact_path.exists(),
        ".symforge/index.bin.zst should exist"
    );
    assert!(
        metadata_path.exists(),
        ".symforge/artifact.json should exist"
    );

    // A-US2-04: .gitattributes carries the merge=ours hint.
    let gitattributes =
        fs::read_to_string(root.join(".gitattributes")).expect("read .gitattributes");
    assert!(
        gitattributes
            .lines()
            .any(|line| line.trim() == "*.zst merge=ours"),
        "expected *.zst merge=ours hint, got: {gitattributes:?}"
    );

    // A-US2-02: cold load with only the .zst present (no index.bin) imports
    // the artifact and every per-file content_hash matches.
    assert!(
        !root.join(".symforge").join("index.bin").exists(),
        "this scenario must have no plain index.bin — only the .zst artifact"
    );
    let imported = persist::load_snapshot(root).expect("load_snapshot should import the artifact");
    assert_eq!(imported.files.len(), before_hashes.len());
    for (path, expected_hash) in &before_hashes {
        let imported_file = imported
            .files
            .get(path)
            .unwrap_or_else(|| panic!("imported snapshot missing {path}"));
        assert_eq!(
            &imported_file.content_hash, expected_hash,
            "content_hash mismatch for {path}"
        );
    }
}

#[test]
fn team_artifact_corrupt_quarantines_without_partial_serve() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write_fixture(root);

    let shared = LiveIndex::load(root).expect("load index");
    {
        let index = shared.read();
        persist::export_artifact(&index, root).expect("export artifact");
    }

    // Corrupt the exported artifact in place (simulates a bad transfer/merge).
    let artifact_path = root.join(".symforge").join(persist::ARTIFACT_FILENAME);
    let good = fs::read(&artifact_path).expect("read good artifact");
    fs::write(&artifact_path, &good[..good.len() / 2]).expect("write truncated artifact");

    // A-US2-03: corrupt import falls back to a full rebuild (None), never a
    // partially-loaded index.
    let result = persist::load_snapshot(root);
    assert!(
        result.is_none(),
        "corrupt artifact must fall back to a full re-index, not partial-serve"
    );
    assert!(
        !artifact_path.exists(),
        "corrupt artifact should be removed from the active path after quarantine"
    );

    let quarantine_dir = root.join(".symforge/quarantine/artifacts");
    let quarantined: Vec<_> = fs::read_dir(&quarantine_dir)
        .expect("quarantine dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("zst"))
        .collect();
    assert_eq!(
        quarantined.len(),
        1,
        "corrupt artifact should be quarantined under .symforge/quarantine/artifacts/"
    );
}
