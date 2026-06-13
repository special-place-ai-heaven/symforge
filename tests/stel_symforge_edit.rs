//! Compact-surface `symforge_edit` — preview and guarded apply structural edit facade.
#![cfg(feature = "server")]
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::result_status::RESULT_STATUS_META_KEY;
use symforge::stel::StelEditRequest;

static COMPACT_ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => unsafe {
                std::env::set_var(self.key, previous);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

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

#[tokio::test]
async fn symforge_edit_rejects_non_compact_surface() {
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "full");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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

    for needle in [
        "── stel ──",
        "already applied",
        "Write mode: already_applied",
        "ledger:",
    ] {
        assert!(output.contains(needle), "missing `{needle}` in:\n{output}");
    }
    assert_eq!(before, std::fs::read(&file_path).unwrap());
}

#[tokio::test]
async fn symforge_edit_preview_then_apply_writes_once() {
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
        "replaced",
        "Write semantics: atomic write + reindex",
    ] {
        assert!(apply.contains(needle), "missing `{needle}` in:\n{apply}");
    }
    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(on_disk.contains("new"), "disk after apply: {on_disk}");
    assert!(!on_disk.contains("old"), "disk after apply: {on_disk}");
}

#[tokio::test]
async fn symforge_edit_apply_idempotency_key_replays_without_double_write() {
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
async fn symforge_edit_failed_guarded_apply_is_not_classified_as_found() {
    let _guard = COMPACT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

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
