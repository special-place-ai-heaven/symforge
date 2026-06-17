// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Acceptance coverage for the post-edit impact footer (feature 007, US1).
//!
//! Contract: `specs/007-intelligence-pattern-ports/contracts/impact-footer.md`.
//! Every successful structural mutation appends a single trailing line
//! `[impact: N dependents]` (or `… · cochanges: a, b, c` when git temporal data
//! is Ready). Failed/rejected edits append nothing, and the footer is byte
//! identical on the first apply and on an idempotency replay.
//!
//! Co-change rendering needs a `Ready` git temporal index; a fresh tempdir is
//! not a git repo, so temporal degrades to `Unavailable` and the footer renders
//! the no-cochanges form. The `· cochanges:` clause is exercised against the
//! symforge repo's own `Ready` temporal index in the manual quickstart; here we
//! pin the dependents portion and the graceful no-cochanges degradation.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

// ─── Fixture ─────────────────────────────────────────────────────────────────

struct Fixture {
    _dir: TempDir,
    server: SymForgeServer,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        for (rel, content) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            fs::write(&path, content).expect("write fixture file");
        }
        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "impact_footer_test".to_string(),
            watcher_info,
            Some(root),
            None,
        );
        Self { _dir: dir, server }
    }

    /// Distinct dependent FILE count for `path`, computed via the exact query the
    /// footer uses — so footer assertions stay deterministic regardless of the
    /// parser's per-reference edge details.
    fn expected_dependents(&self, path: &str) -> usize {
        self.server
            .index()
            .read()
            .capture_find_dependents_view(path)
            .files
            .len()
    }

    async fn call(&self, tool: &str, params: Value) -> String {
        self.server.dispatch_tool_for_tests(tool, params).await
    }
}

/// A library symbol plus a second file that depends on it via a qualified call,
/// so `lib.rs` has at least one distinct dependent file under the real parser.
fn library_with_one_dependent() -> Vec<(&'static str, &'static str)> {
    vec![
        ("src/lib.rs", "pub mod widget;\n"),
        ("src/widget.rs", "pub fn render() -> u32 {\n    1\n}\n"),
        (
            "src/consumer.rs",
            "use crate::widget;\n\npub fn run() -> u32 {\n    widget::render()\n}\n",
        ),
    ]
}

// ─── (a) Non-zero dependents reported correctly ──────────────────────────────

#[tokio::test]
async fn replace_symbol_body_reports_dependent_count() {
    let fx = Fixture::new(&library_with_one_dependent());
    let expected = fx.expected_dependents("src/widget.rs");
    assert!(
        expected >= 1,
        "fixture must yield at least one dependent of src/widget.rs (got {expected}); \
         a meaningful non-zero footer assertion depends on it"
    );

    let output = fx
        .call(
            "replace_symbol_body",
            json!({
                "path": "src/widget.rs",
                "name": "render",
                "new_body": "pub fn render() -> u32 {\n    2\n}",
            }),
        )
        .await;

    let needle = format!("[impact: {expected} dependents");
    assert!(
        output.contains(&needle),
        "replace_symbol_body success must carry the impact footer with the correct \
         dependent count (expected fragment {needle:?})\nactual:\n{output}"
    );
}

#[tokio::test]
async fn edit_within_symbol_reports_dependent_count() {
    let fx = Fixture::new(&library_with_one_dependent());
    let expected = fx.expected_dependents("src/widget.rs");
    assert!(expected >= 1, "fixture must yield at least one dependent");

    let output = fx
        .call(
            "edit_within_symbol",
            json!({
                "path": "src/widget.rs",
                "name": "render",
                "old_text": "1",
                "new_text": "3",
            }),
        )
        .await;

    let needle = format!("[impact: {expected} dependents");
    assert!(
        output.contains(&needle),
        "edit_within_symbol success must carry the impact footer with the correct \
         dependent count (expected fragment {needle:?})\nactual:\n{output}"
    );
}

// ─── (b) Zero-dependent symbol renders explicit count ────────────────────────

#[tokio::test]
async fn zero_dependent_symbol_renders_zero_no_cochanges() {
    // A standalone symbol nothing references → 0 dependents. The tempdir is not a
    // git repo, so temporal is not Ready → no `· cochanges:` clause.
    let fx = Fixture::new(&[("src/solo.rs", "pub fn alone() -> u32 {\n    1\n}\n")]);
    assert_eq!(
        fx.expected_dependents("src/solo.rs"),
        0,
        "solo symbol must have zero dependents for this assertion to be meaningful"
    );

    let output = fx
        .call(
            "replace_symbol_body",
            json!({
                "path": "src/solo.rs",
                "name": "alone",
                "new_body": "pub fn alone() -> u32 {\n    2\n}",
            }),
        )
        .await;

    assert!(
        output.contains("[impact: 0 dependents]"),
        "zero-dependent edit must render `[impact: 0 dependents]` (explicit, no \
         cochanges clause)\nactual:\n{output}"
    );
}

// ─── (c) Failed / rejected edit appends no footer ────────────────────────────

#[tokio::test]
async fn rejected_edit_has_no_impact_footer() {
    let fx = Fixture::new(&[("src/solo.rs", "pub fn alone() -> u32 {\n    1\n}\n")]);

    // Target a symbol that does not exist → the handler early-returns before the
    // success tail, so no footer is appended.
    let output = fx
        .call(
            "replace_symbol_body",
            json!({
                "path": "src/solo.rs",
                "name": "does_not_exist",
                "new_body": "pub fn does_not_exist() {}",
            }),
        )
        .await;

    assert!(
        !output.contains("[impact:"),
        "a rejected edit must NOT carry an impact footer\nactual:\n{output}"
    );
}

#[tokio::test]
async fn batch_edit_error_arm_has_no_impact_footer() {
    let fx = Fixture::new(&[("src/solo.rs", "pub fn alone() -> u32 {\n    1\n}\n")]);

    // One valid + one invalid target → the whole batch fails (transactional),
    // landing in the Err arm, which appends no footer.
    let output = fx
        .call(
            "batch_edit",
            json!({
                "edits": [
                    {
                        "path": "src/solo.rs",
                        "name": "alone",
                        "operation": {"type": "edit_within", "old_text": "1", "new_text": "2"}
                    },
                    {
                        "path": "src/solo.rs",
                        "name": "missing_symbol",
                        "operation": {"type": "edit_within", "old_text": "x", "new_text": "y"}
                    }
                ]
            }),
        )
        .await;

    assert!(
        !output.contains("[impact:"),
        "a failed batch_edit (Err arm) must NOT carry an impact footer\nactual:\n{output}"
    );
}

// ─── (d) Footer identical on first apply and idempotency replay ──────────────

#[tokio::test]
async fn footer_identical_on_first_apply_and_replay() {
    let fx = Fixture::new(&library_with_one_dependent());
    let key = "impact-footer-replay-key";

    let first = fx
        .call(
            "replace_symbol_body",
            json!({
                "path": "src/widget.rs",
                "name": "render",
                "new_body": "pub fn render() -> u32 {\n    7\n}",
                "idempotency_key": key,
            }),
        )
        .await;

    let replay = fx
        .call(
            "replace_symbol_body",
            json!({
                "path": "src/widget.rs",
                "name": "render",
                "new_body": "pub fn render() -> u32 {\n    7\n}",
                "idempotency_key": key,
            }),
        )
        .await;

    let footer_of = |body: &str| -> String {
        body.lines()
            .find(|line| line.starts_with("[impact:"))
            .unwrap_or_else(|| panic!("response must contain an impact footer line\nbody:\n{body}"))
            .to_string()
    };

    assert_eq!(
        footer_of(&first),
        footer_of(&replay),
        "impact footer must be byte identical on first apply vs idempotency replay\n\
         first:\n{first}\nreplay:\n{replay}"
    );
}

// ─── Forbidden-content guard (contract §Forbidden content) ───────────────────

#[tokio::test]
async fn footer_contains_no_classifier_sentinels() {
    let fx = Fixture::new(&library_with_one_dependent());

    let output = fx
        .call(
            "replace_symbol_body",
            json!({
                "path": "src/widget.rs",
                "name": "render",
                "new_body": "pub fn render() -> u32 {\n    9\n}",
            }),
        )
        .await;

    let footer = output
        .lines()
        .find(|line| line.starts_with("[impact:"))
        .map(PathBuf::from)
        .map(|p| p.to_string_lossy().into_owned())
        .expect("response must contain an impact footer line");

    for sentinel in [
        "Error",
        "unavailable",
        "byte range",
        "Write failed",
        "[DRY RUN]",
        "Write semantics:",
        "Ambiguous:",
        "Symbol not found:",
    ] {
        assert!(
            !footer.contains(sentinel),
            "impact footer must not contain classifier sentinel {sentinel:?}; footer: {footer:?}"
        );
    }
}
