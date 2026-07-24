// SF-AAP-002 regression: `analyze_file_impact` on a newly-reconciled NON-PARSER
// file (a type with no code parser — a binary/artifact reconciled at Tier-2)
// must report truthful existence + generation/Tier-2 evidence and a typed
// unsupported-analysis outcome — NEVER false absence ("File not found" /
// "Not indexed").
#![cfg(feature = "server")]

use std::fs;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

fn build_server() -> (TempDir, SymForgeServer) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("src")).expect("src dir");
    // A parser file so the index is non-empty and reconciliation runs.
    fs::write(root.join("src/lib.rs"), "pub fn anchor() {}\n").expect("rs fixture");
    // Genuine non-parser file: a binary artifact reconciled at Tier-2 (no code parser).
    fs::write(root.join("blob.bin"), [0u8, 159, 146, 150, 0, 1, 2, 3]).expect("bin fixture");
    // Text-parser files (Tier-1, 0 symbols) — regression guard that the fix does
    // not disturb the truthful indexed-file path.
    fs::write(root.join("notes.txt"), "plain text notes, no code\n").expect("txt fixture");

    let index = LiveIndex::load(&root).expect("LiveIndex::load non-parser fixture");
    let server = SymForgeServer::new(
        index,
        "afi_non_parser_test".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root),
        None,
    );
    (dir, server)
}

#[tokio::test]
async fn non_parser_file_reports_existence_and_typed_unsupported_not_false_absence() {
    let (_dir, server) = build_server();

    // Both the auto-index (new_file=None) and explicit (new_file=true) entry
    // points must be truthful for the reconciled non-parser file.
    for new_file in [None, Some(true)] {
        let mut req = json!({ "path": "blob.bin" });
        if let Some(nf) = new_file {
            req["new_file"] = json!(nf);
        }
        let out = server
            .dispatch_tool_for_tests("analyze_file_impact", req)
            .await;
        let lower = out.to_lowercase();

        // Never a false absence.
        assert!(
            !lower.contains("not found"),
            "non-parser file exists; must not report 'not found' (new_file={new_file:?}):\n{out}"
        );
        assert!(
            !lower.contains("not indexed"),
            "reconciled Tier-2 file must not be framed as 'Not indexed' \
             (false-absence smell) (new_file={new_file:?}):\n{out}"
        );

        // Affirmative existence.
        assert!(
            lower.contains("exists: true"),
            "must affirmatively report existence (new_file={new_file:?}):\n{out}"
        );
        // Tier-2 / generation evidence.
        assert!(
            out.contains("Tier 2"),
            "must surface Tier-2 evidence (new_file={new_file:?}):\n{out}"
        );
        assert!(
            lower.contains("generation"),
            "must surface generation evidence (new_file={new_file:?}):\n{out}"
        );
        // Typed unsupported-analysis outcome, decoupled from parser support.
        assert!(
            lower.contains("unsupported"),
            "must report a typed unsupported-analysis outcome (new_file={new_file:?}):\n{out}"
        );
    }
}

/// Regression guard: a Text-parser file (Tier-1, 0 symbols) must still report a
/// truthful indexed status — the SF-AAP-002 fix targets non-parser Tier-2 files
/// only and must not disturb this path.
#[tokio::test]
async fn text_parser_file_still_reports_truthful_indexed_status() {
    let (_dir, server) = build_server();
    let out = server
        .dispatch_tool_for_tests("analyze_file_impact", json!({ "path": "notes.txt" }))
        .await;
    let lower = out.to_lowercase();
    assert!(
        !lower.contains("not found") && !lower.contains("not indexed"),
        "an indexed Text file must not be reported absent:\n{out}"
    );
    assert!(
        out.contains("indexed and unchanged"),
        "an unchanged indexed Text file should report 'indexed and unchanged':\n{out}"
    );
}
