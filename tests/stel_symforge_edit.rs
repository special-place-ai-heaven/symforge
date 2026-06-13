//! Compact-surface `symforge_edit` — preview-only structural edit facade.
#![cfg(feature = "server")]
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
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

async fn dispatch_symforge_edit(server: &SymForgeServer, request: &StelEditRequest) -> String {
    let params = serde_json::to_value(request).expect("serialize edit request");
    let result = server
        .dispatch_tool_result_for_tests("symforge_edit", params)
        .await
        .expect("symforge_edit dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize CallToolResult");
    tool_result_text(&serialized).to_string()
}

#[tokio::test]
async fn symforge_edit_rejects_non_compact_surface() {
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
    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
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
    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
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
    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
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
