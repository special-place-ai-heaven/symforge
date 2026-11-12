//! Shared helpers for the feature 036 reliability tests (specs/036-tool-reliability, T002).
//!
//! Indexes a directory (the fixture crate `tests/fixtures/reliability/` by
//! default) and calls a tool through the same test doors the other integration
//! tests use: `dispatch_tool_for_tests` for text, and
//! `dispatch_tool_result_for_tests` when `_meta` is wanted (statused tools only).
#![allow(dead_code)] // each including test binary uses a subset

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;

/// Text and result metadata of one tool call.
pub struct Reply {
    pub text: String,
    /// `_meta` of the `CallToolResult`; `None` when the tool has no statused path yet.
    pub meta: Option<Value>,
}

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reliability")
}

/// A server whose index was loaded from `root`.
pub fn server_for(root: &Path) -> SymForgeServer {
    server_from_shared(root, LiveIndex::load(root).expect("load index"))
}

/// A server over an index the caller already loaded (and may have adjusted).
pub fn server_from_shared(
    root: &Path,
    shared: symforge::live_index::SharedIndex,
) -> SymForgeServer {
    SymForgeServer::new(
        shared,
        "reliability".to_string(),
        Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    )
}

/// A server over the reliability fixture.
pub fn fixture_server() -> SymForgeServer {
    server_for(&fixture_root())
}

/// A server over this repository (for tests whose failure needs a large tree).
pub fn repo_server() -> SymForgeServer {
    server_for(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// Call `tool` and return its text.
pub async fn call(server: &SymForgeServer, tool: &str, params: Value) -> String {
    server.dispatch_tool_for_tests(tool, params).await
}

/// Call `tool` and return text plus `_meta`, using the statused dispatcher.
pub async fn call_with_meta(server: &SymForgeServer, tool: &str, params: Value) -> Reply {
    let result = server
        .dispatch_tool_result_for_tests(tool, params)
        .await
        .expect("statused dispatch");
    let value = serde_json::to_value(&result).expect("serialize CallToolResult");
    let text = value["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    Reply {
        text,
        meta: value.get("_meta").cloned(),
    }
}

/// Leading line numbers of a numbered listing (`N: text`, `N| text`, `N\ttext`).
pub fn numbered_lines(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
            let rest = &line[digits.len()..];
            if digits.is_empty() || !rest.starts_with([':', '|', '\t']) {
                return None;
            }
            digits.parse().ok()
        })
        .collect()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Run `git` in `dir`, panicking with its stderr on failure; returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = symforge::process_util::hidden_command("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A plain copy of the fixture (no git repository) that a test may modify.
pub fn fixture_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&fixture_root(), dir.path());
    dir
}

/// A copy of the fixture committed on branch `main` in a fresh git repository.
pub fn committed_fixture_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&fixture_root(), dir.path());
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "baseline"]);
    dir
}
