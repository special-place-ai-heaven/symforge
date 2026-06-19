//! Compact-surface `symforge_edit` — preview and guarded apply structural edit facade.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::path::{Path, PathBuf};

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::result_status::RESULT_STATUS_META_KEY;
use symforge::stel::StelEditRequest;

fn tool_result_text(result: &serde_json::Value) -> &str {
    result["content"][0]["text"]
        .as_str()
        .expect("symforge_edit result must contain text content")
}

fn outcome_class(result: &serde_json::Value) -> &str {
    result["_meta"][RESULT_STATUS_META_KEY]["outcome_class"]
        .as_str()
        .expect("symforge_edit result must include result_status outcome_class")
}

fn server_for_repo(root: &Path, project: &str) -> SymForgeServer {
    let shared = LiveIndex::load(root).unwrap_or_else(|error| {
        panic!("index {}: {error}", root.display());
    });
    SymForgeServer::new(
        shared,
        project.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    )
}

fn temp_rust_repo(content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join(".git")).expect("create .git");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src");
    let file = src.join("lib.rs");
    std::fs::write(&file, content).expect("write lib.rs");
    (dir, file)
}

fn temp_markdown_repo(content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join(".git")).expect("create .git");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src");
    let file = src.join("doc.md");
    std::fs::write(&file, content).expect("write doc.md");
    (dir, file)
}

async fn dispatch_symforge_edit_result(
    server: &SymForgeServer,
    request: &StelEditRequest,
) -> serde_json::Value {
    let params = serde_json::to_value(request).expect("serialize edit request");
    let result = server
        .dispatch_tool_result_for_tests("symforge_edit", params)
        .await
        .expect("symforge_edit dispatch");
    serde_json::to_value(&result).expect("serialize CallToolResult")
}

async fn dispatch_symforge_edit(server: &SymForgeServer, request: &StelEditRequest) -> String {
    tool_result_text(&dispatch_symforge_edit_result(server, request).await).to_string()
}

fn ledger_meta_from_output(output: &str) -> symforge::stel::LedgerEnvelopeMeta {
    let ledger_json = output
        .lines()
        .find(|line| line.starts_with("ledger: "))
        .expect("ledger line in envelope")
        .trim_start_matches("ledger: ");
    serde_json::from_str(ledger_json).expect("ledger json")
}

#[tokio::test]
async fn symforge_edit_rejects_non_compact_surface() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("full");

    let (dir, _) = temp_rust_repo("fn foo() {}\n");
    let server = server_for_repo(dir.path(), "edit-non-compact");
    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() { 1 }".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert!(
        output.contains("requires SYMFORGE_SURFACE=compact"),
        "unexpected output:\n{output}"
    );
}

#[tokio::test]
async fn symforge_edit_rejects_unsafe_path() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let (dir, _) = temp_rust_repo("fn foo() {}\n");
    let server = server_for_repo(dir.path(), "edit-unsafe-path");
    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "../outside.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() {}".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert!(
        output.contains("parent traversal"),
        "unexpected output:\n{output}"
    );
}

#[tokio::test]
async fn symforge_edit_rejects_missing_symbol_and_body() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let (dir, _) = temp_rust_repo("fn foo() {}\n");
    let server = server_for_repo(dir.path(), "edit-missing-fields");
    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            ..Default::default()
        },
    )
    .await;
    assert!(
        output.contains("symbol is required"),
        "unexpected output:\n{output}"
    );
}

#[tokio::test]
async fn symforge_edit_preview_includes_envelope_ledger_and_dry_run_without_writes() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "fn foo() { old }\n";
    let (dir, file_path) = temp_rust_repo(original);
    let before = std::fs::read(&file_path).expect("read file before edit");
    let server = server_for_repo(dir.path(), "edit-preview");

    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() { new }".to_string()),
            ..Default::default()
        },
    )
    .await;

    for needle in [
        "── stel ──",
        "decision: serve",
        "ledger:",
        "preview-only (dry_run)",
        "Chosen tool: replace_symbol_body",
        "[DRY RUN]",
        "Write semantics: dry run (no writes)",
    ] {
        assert!(output.contains(needle), "missing `{needle}` in:\n{output}");
    }

    let after = std::fs::read(&file_path).expect("read file after edit");
    assert_eq!(before, after, "preview must not mutate source bytes");
}

#[tokio::test]
async fn symforge_edit_explicit_apply_false_matches_preview_no_write() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "fn foo() { old }\n";
    let (dir, file_path) = temp_rust_repo(original);
    let before = std::fs::read(&file_path).expect("read file before edit");
    let server = server_for_repo(dir.path(), "edit-apply-false");

    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() { new }".to_string()),
            apply: Some(false),
            ..Default::default()
        },
    )
    .await;

    assert!(output.contains("[DRY RUN]"), "output:\n{output}");
    let after = std::fs::read(&file_path).expect("read file after edit");
    assert_eq!(before, after, "apply:false must not write");
}

#[tokio::test]
async fn symforge_edit_rejects_missing_symbol_on_apply() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // No FULL-envelope opt-in here: this is a pre-apply reject that emits NO
    // trust envelope at all (asserted below via `!output.contains("── stel ──")`),
    // so the envelope render mode is irrelevant to what this test checks.

    let (dir, file_path) = temp_rust_repo("fn foo() { old }\n");
    let before = std::fs::read(&file_path).expect("read file");
    let server = server_for_repo(dir.path(), "edit-missing-symbol");

    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("missing_symbol".to_string()),
            body: Some("fn missing_symbol() {}".to_string()),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;

    assert!(
        output.contains("symbol not found"),
        "unexpected output:\n{output}"
    );
    assert_eq!(before, std::fs::read(&file_path).unwrap());
    assert!(
        !output.contains("── stel ──"),
        "pre-apply reject must not include envelope"
    );
}

#[tokio::test]
async fn symforge_edit_rejects_if_match_mismatch_on_apply() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "fn foo() { old }\n";
    let (dir, file_path) = temp_rust_repo(original);
    let before = std::fs::read(&file_path).expect("read file");
    let server = server_for_repo(dir.path(), "edit-if-match");

    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() { new }".to_string()),
            apply: Some(true),
            if_match: Some("fn foo() { wrong }".to_string()),
            ..Default::default()
        },
    )
    .await;

    assert!(
        output.contains("if_match does not match"),
        "unexpected output:\n{output}"
    );
    assert_eq!(before, std::fs::read(&file_path).unwrap());
}

#[tokio::test]
async fn symforge_edit_apply_already_applied_is_idempotent_without_rewrite() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "fn foo() { same }\n";
    let (dir, file_path) = temp_rust_repo(original);
    let before = std::fs::read(&file_path).expect("read file");
    let server = server_for_repo(dir.path(), "edit-already-applied");

    let output = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() { same }".to_string()),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;

    for needle in ["already applied", "Write mode: already_applied", "ledger:"] {
        assert!(output.contains(needle), "missing `{needle}` in:\n{output}");
    }
    let meta = ledger_meta_from_output(&output);
    assert!(
        !meta.legacy_executed,
        "already-applied must not count as committed write"
    );
    assert_eq!(before, std::fs::read(&file_path).unwrap());
}

#[tokio::test]
async fn symforge_edit_preview_then_apply_writes_once() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "fn foo() { old }\n";
    let (dir, file_path) = temp_rust_repo(original);
    let server = server_for_repo(dir.path(), "edit-preview-then-apply");

    let preview = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() { new }".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert!(preview.contains("[DRY RUN]"), "preview:\n{preview}");
    assert!(
        std::fs::read_to_string(&file_path).unwrap().contains("old"),
        "preview must not write"
    );

    let apply = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() { new }".to_string()),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;

    for needle in [
        "── stel ──",
        "ledger:",
        "Write mode: committed",
        "Byte range:",
        "Line range:",
        "Write semantics: atomic write + reindex",
    ] {
        assert!(apply.contains(needle), "missing `{needle}` in:\n{apply}");
    }
    let meta = ledger_meta_from_output(&apply);
    assert!(
        meta.legacy_executed,
        "successful apply must record legacy_executed from write-semantics envelope"
    );
    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(on_disk.contains("new"), "disk after apply: {on_disk}");
    assert!(!on_disk.contains("old"), "disk after apply: {on_disk}");
}

#[tokio::test]
async fn symforge_edit_apply_idempotency_key_replays_without_double_write() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "fn foo() { old }\n";
    let (dir, file_path) = temp_rust_repo(original);
    let server = server_for_repo(dir.path(), "edit-idempotency");

    let request = StelEditRequest {
        path: "src/lib.rs".to_string(),
        symbol: Some("foo".to_string()),
        body: Some("fn foo() { new }".to_string()),
        apply: Some(true),
        idempotency_key: Some("stel-edit-replay-key".to_string()),
        ..Default::default()
    };

    let first = dispatch_symforge_edit(&server, &request).await;
    assert!(first.contains("replaced"), "first apply:\n{first}");
    let after_first = std::fs::read(&file_path).unwrap();

    let second = dispatch_symforge_edit(&server, &request).await;
    assert!(
        second.contains("replaced"),
        "second apply should replay stored result without rewriting:\n{second}"
    );
    assert_eq!(after_first, std::fs::read(&file_path).unwrap());
}

#[tokio::test]
async fn symforge_edit_rejects_absolute_and_scheme_paths() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let (dir, file_path) = temp_rust_repo("fn foo() {}\n");
    let before = std::fs::read(&file_path).expect("read file");
    let server = server_for_repo(dir.path(), "edit-absolute-paths");

    let cases = [
        ("/tmp/x.rs", "not absolute"),
        (r"C:\temp\x.rs", "drive or scheme"),
        ("file:///tmp/x.rs", "drive or scheme"),
    ];

    for (path, needle) in cases {
        let output = dispatch_symforge_edit(
            &server,
            &StelEditRequest {
                path: path.to_string(),
                symbol: Some("foo".to_string()),
                body: Some("fn foo() {}".to_string()),
                apply: Some(true),
                ..Default::default()
            },
        )
        .await;
        assert!(
            output.contains(needle),
            "path `{path}` should be rejected ({needle}):\n{output}"
        );
        assert_eq!(
            before,
            std::fs::read(&file_path).unwrap(),
            "path `{path}` must not write"
        );
    }
}

#[tokio::test]
async fn symforge_edit_insert_after_preview_then_apply_adds_new_symbol() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "fn anchor() { 1 }\n";
    let (dir, file_path) = temp_rust_repo(original);
    let server = server_for_repo(dir.path(), "edit-insert-after");

    // Preview routes to insert_symbol and writes nothing.
    let preview = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("anchor".to_string()),
            body: Some("fn added() { 2 }".to_string()),
            op: Some(symforge::stel::StelEditOp::InsertAfter),
            ..Default::default()
        },
    )
    .await;
    assert!(
        preview.contains("Chosen tool: insert_symbol"),
        "insert preview must route to insert_symbol:\n{preview}"
    );
    assert!(preview.contains("[DRY RUN]"), "preview:\n{preview}");
    assert!(
        !std::fs::read_to_string(&file_path)
            .unwrap()
            .contains("added"),
        "preview must not write"
    );

    // Apply commits the insert through the facade — no native file tool.
    let apply = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("anchor".to_string()),
            body: Some("fn added() { 2 }".to_string()),
            op: Some(symforge::stel::StelEditOp::InsertAfter),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;
    assert!(
        apply.contains("Chosen tool: insert_symbol"),
        "insert apply must route to insert_symbol:\n{apply}"
    );
    let meta = ledger_meta_from_output(&apply);
    assert!(meta.legacy_executed, "insert apply must commit:\n{apply}");
    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(on_disk.contains("fn anchor()"), "disk: {on_disk}");
    assert!(on_disk.contains("fn added()"), "disk: {on_disk}");
}

#[tokio::test]
async fn symforge_edit_edit_within_amends_import_inside_module() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    // A file-level `use` is NOT an indexed symbol in Rust, but a `use` inside a
    // `mod` block IS reachable via edit_within scoped to the enclosing module.
    let original = "mod inner {\n    use a::b;\n    pub fn helper() {}\n}\n";
    let (dir, file_path) = temp_rust_repo(original);
    let server = server_for_repo(dir.path(), "edit-within-import");

    let apply = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("inner".to_string()),
            old_text: Some("use a::b;".to_string()),
            new_text: Some("use a::{b, c};".to_string()),
            op: Some(symforge::stel::StelEditOp::EditWithin),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;
    assert!(
        apply.contains("Chosen tool: edit_within_symbol"),
        "within apply must route to edit_within_symbol:\n{apply}"
    );
    let meta = ledger_meta_from_output(&apply);
    assert!(meta.legacy_executed, "within apply must commit:\n{apply}");
    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(
        on_disk.contains("use a::{b, c};"),
        "import not amended:\n{on_disk}"
    );
    assert!(!on_disk.contains("use a::b;"), "stale import:\n{on_disk}");
}

#[tokio::test]
async fn symforge_edit_completes_full_refactor_through_facade_only() {
    // KEYSTONE: a realistic refactor completed ENTIRELY through symforge_edit —
    // insert a new method after an anchor, amend an import via a within-symbol
    // edit, and replace a body — each routing to the right internal tool and
    // committing, with NO native file-tool fallback.
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "\
mod imports {
    use crate::a;
    pub fn touch() {}
}

fn anchor() { 1 }
";
    let (dir, file_path) = temp_rust_repo(original);
    let server = server_for_repo(dir.path(), "edit-full-refactor");

    // Step 1: insert a NEW method after the `anchor` symbol.
    let insert = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("anchor".to_string()),
            body: Some("fn appended() { 99 }".to_string()),
            op: Some(symforge::stel::StelEditOp::InsertAfter),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;
    assert!(
        insert.contains("Chosen tool: insert_symbol"),
        "step 1 routing:\n{insert}"
    );
    assert!(
        ledger_meta_from_output(&insert).legacy_executed,
        "step 1 must commit:\n{insert}"
    );

    // Step 2: amend the import inside `mod imports` via a within-symbol edit.
    let within = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("imports".to_string()),
            old_text: Some("use crate::a;".to_string()),
            new_text: Some("use crate::{a, b};".to_string()),
            op: Some(symforge::stel::StelEditOp::EditWithin),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;
    assert!(
        within.contains("Chosen tool: edit_within_symbol"),
        "step 2 routing:\n{within}"
    );
    assert!(
        ledger_meta_from_output(&within).legacy_executed,
        "step 2 must commit:\n{within}"
    );

    // Step 3: replace the body of the `anchor` symbol (default op).
    let replace = dispatch_symforge_edit(
        &server,
        &StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("anchor".to_string()),
            body: Some("fn anchor() { 2 }".to_string()),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;
    assert!(
        replace.contains("Chosen tool: replace_symbol_body"),
        "step 3 routing:\n{replace}"
    );
    assert!(
        ledger_meta_from_output(&replace).legacy_executed,
        "step 3 must commit:\n{replace}"
    );

    // All three edits landed on disk via the facade only.
    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(
        on_disk.contains("fn appended() { 99 }"),
        "insert:\n{on_disk}"
    );
    assert!(on_disk.contains("use crate::{a, b};"), "within:\n{on_disk}");
    assert!(on_disk.contains("fn anchor() { 2 }"), "replace:\n{on_disk}");
    assert!(!on_disk.contains("fn anchor() { 1 }"), "stale:\n{on_disk}");
    assert!(!on_disk.contains("use crate::a;"), "stale:\n{on_disk}");
}

#[tokio::test]
async fn symforge_edit_failed_guarded_apply_is_not_classified_as_found() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: these apply/preview tests assert the
    // `── stel ──` header and parse the `ledger:` line, both full-block only.
    let _full = stel_surface_env::force_full_stel_envelope();

    let original = "# foo\n\nOld section body.\n";
    let (dir, file_path) = temp_markdown_repo(original);
    let before = std::fs::read(&file_path).expect("read file");
    let server = server_for_repo(dir.path(), "edit-failed-apply");

    let result = dispatch_symforge_edit_result(
        &server,
        &StelEditRequest {
            path: "src/doc.md".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("# foo\n\nNew section body.\n".to_string()),
            apply: Some(true),
            ..Default::default()
        },
    )
    .await;
    let output = tool_result_text(&result);
    assert!(
        output.contains("Write mode: failed") || output.contains("edit safety blocked"),
        "expected failed guarded apply output:\n{output}"
    );
    assert_ne!(
        outcome_class(&result),
        "found",
        "failed guarded apply must not classify as Found:\n{output}"
    );
    assert_eq!(before, std::fs::read(&file_path).unwrap());
}
