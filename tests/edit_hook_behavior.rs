// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Parity tests — locks in current behavior of the 7 edit tools.
//!
//! These tests MUST pass against current `main` *before* the `edit_hooks`
//! refactor lands. After the refactor, they must still pass byte-identically.
//! That invariant is the refactor's acceptance bar (CONTEXT.md §Acceptance bar).
//!
//! The tests dispatch through `SymForgeServer::dispatch_tool_for_tests`, the
//! test-only JSON entry point that mirrors `daemon::execute_tool_call`. They
//! exercise:
//!
//!   - `replace_symbol_body`       — body replace + dry run + not found + indentation
//!   - `insert_symbol`             — after + before positions
//!   - `delete_symbol`             — removal + doc-cleanup
//!   - `edit_within_symbol`        — scoped find/replace + not-found diagnostic
//!   - `batch_edit`                — multi-file structural edit
//!   - `batch_rename`              — def + references across files
//!   - `batch_insert`              — same content inserted at multiple targets
//!
//! Scope: observable outputs only — the formatted return string and the
//! resulting file bytes on disk. The tests deliberately avoid asserting on
//! internal state (reverse index, trigram cache) that the refactor may
//! reshape without changing externally-visible behavior.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

// ─── Fixture helpers ─────────────────────────────────────────────────────────

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    server: SymForgeServer,
}

impl Fixture {
    /// Create a temp project root, write the provided files, load the index,
    /// and build a `SymForgeServer` bound to that root.
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
            "edit_hook_behavior_test".to_string(),
            watcher_info,
            Some(root.clone()),
            None,
        );
        Self {
            _dir: dir,
            root,
            server,
        }
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).expect("read file")
    }
}

async fn call(server: &SymForgeServer, tool: &str, params: Value) -> String {
    server.dispatch_tool_for_tests(tool, params).await
}

fn assert_contains(result: &str, needle: &str) {
    assert!(
        result.contains(needle),
        "expected result to contain `{needle}`; result was:\n{result}"
    );
}

fn assert_not_contains(result: &str, needle: &str) {
    assert!(
        !result.contains(needle),
        "expected result NOT to contain `{needle}`; result was:\n{result}"
    );
}

fn ensure_repo_root(path: &Path) {
    // Make sure the repo_root we passed is absolute so `prepare_exact_path_for_edit`
    // produces a `repository-bound` envelope line.
    assert!(path.is_absolute(), "test root must be absolute: {path:?}");
}

// ─── replace_symbol_body ─────────────────────────────────────────────────────

#[tokio::test]
async fn replace_symbol_body_replaces_and_reindexes() {
    let original =
        "fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);
    ensure_repo_root(&fx.root);

    let result = call(
        &fx.server,
        "replace_symbol_body",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "new_body": "fn hello() {\n    println!(\"HELLO\");\n}",
        }),
    )
    .await;

    assert_contains(&result, "Edit safety: structural-edit-safe");
    assert_contains(&result, "Path authority: repository-bound");
    assert_contains(&result, "Write semantics: atomic write + reindex");
    assert_contains(&result, "Evidence: symbol anchor `src/lib.rs:1`");
    assert_contains(&result, "src/lib.rs — replaced fn `hello`");

    let on_disk = fx.read("src/lib.rs");
    assert!(on_disk.contains("HELLO"), "replacement written: {on_disk}");
    assert!(
        on_disk.contains("fn world()"),
        "sibling untouched: {on_disk}"
    );
}

#[tokio::test]
async fn replace_symbol_body_preserves_indentation() {
    // Symbol is nested inside a module; replacement body is provided unindented
    // and should be re-indented to 4 spaces to match the target.
    let original = "mod outer {\n    fn inner() {\n        old_body();\n    }\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "replace_symbol_body",
        json!({
            "path": "src/lib.rs",
            "name": "inner",
            "new_body": "fn inner() {\n    new_body();\n}",
        }),
    )
    .await;
    assert_contains(&result, "replaced");

    let on_disk = fx.read("src/lib.rs");
    assert!(
        on_disk.contains("    fn inner() {\n        new_body();\n    }"),
        "replacement re-indented: {on_disk}"
    );
}

#[tokio::test]
async fn replace_symbol_body_dry_run_skips_write() {
    let original = "fn hello() {\n    old();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "replace_symbol_body",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "new_body": "fn hello() {\n    new();\n}",
            "dry_run": true,
        }),
    )
    .await;

    assert_contains(&result, "Write semantics: dry run (no writes)");
    assert_contains(&result, "[DRY RUN] Would replace `hello`");

    let on_disk = fx.read("src/lib.rs");
    assert_eq!(on_disk, original, "file must be unchanged in dry run");
}

#[tokio::test]
async fn replace_symbol_body_not_found_returns_error() {
    let fx = Fixture::new(&[("src/lib.rs", "fn hello() {}\n")]);

    let result = call(
        &fx.server,
        "replace_symbol_body",
        json!({
            "path": "nonexistent.rs",
            "name": "hello",
            "new_body": "fn hello() {}",
        }),
    )
    .await;

    let lower = result.to_lowercase();
    assert!(
        lower.contains("not found"),
        "expected not-found error; got: {result}"
    );
    assert_not_contains(&result, "atomic write");
}

// ─── insert_symbol ───────────────────────────────────────────────────────────

#[tokio::test]
async fn insert_symbol_after_places_new_symbol_below_anchor() {
    let original = "fn hello() {\n    h();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "insert_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "content": "fn world() {\n    w();\n}",
            "position": "after",
        }),
    )
    .await;

    assert_contains(&result, "Edit safety: structural-edit-safe");
    assert_contains(&result, "Write semantics: atomic write + reindex");
    assert_contains(&result, "src/lib.rs — inserted after `hello`");

    let on_disk = fx.read("src/lib.rs");
    let hello_pos = on_disk.find("hello").expect("hello kept");
    let world_pos = on_disk.find("world").expect("world added");
    assert!(hello_pos < world_pos, "hello precedes world: {on_disk}");
}

#[tokio::test]
async fn insert_symbol_before_places_new_symbol_above_anchor() {
    let original = "fn world() {\n    w();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "insert_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "world",
            "content": "fn hello() {\n    h();\n}",
            "position": "before",
        }),
    )
    .await;

    assert_contains(&result, "inserted before `world`");

    let on_disk = fx.read("src/lib.rs");
    let hello_pos = on_disk.find("hello").expect("hello added");
    let world_pos = on_disk.find("world").expect("world kept");
    assert!(hello_pos < world_pos, "hello precedes world: {on_disk}");
}

#[tokio::test]
async fn insert_symbol_invalid_position_is_rejected() {
    let fx = Fixture::new(&[("src/lib.rs", "fn hello() {}\n")]);

    let result = call(
        &fx.server,
        "insert_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "content": "fn x() {}",
            "position": "sideways",
        }),
    )
    .await;

    assert_contains(&result, "position must be 'before' or 'after'");
}

// ─── delete_symbol ───────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_symbol_removes_target_and_leaves_siblings() {
    let original = "fn hello() {\n    h();\n}\n\nfn world() {\n    w();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "delete_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
        }),
    )
    .await;

    assert_contains(&result, "Edit safety: structural-edit-safe");
    assert_contains(&result, "Write semantics: atomic write + reindex");
    assert_contains(&result, "src/lib.rs — deleted fn `hello`");

    let on_disk = fx.read("src/lib.rs");
    assert!(!on_disk.contains("hello"), "target removed: {on_disk}");
    assert!(on_disk.contains("fn world"), "sibling kept: {on_disk}");
}

#[tokio::test]
async fn delete_symbol_dry_run_does_not_touch_disk() {
    let original = "fn hello() {}\n\nfn world() {}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "delete_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "dry_run": true,
        }),
    )
    .await;

    assert_contains(&result, "Write semantics: dry run (no writes)");
    assert_contains(&result, "[DRY RUN] Would delete `hello`");
    assert_eq!(fx.read("src/lib.rs"), original);
}

// ─── edit_within_symbol ──────────────────────────────────────────────────────

#[tokio::test]
async fn edit_within_symbol_replaces_single_occurrence() {
    let original = "fn hello() {\n    println!(\"hello\");\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "edit_within_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "old_text": "\"hello\"",
            "new_text": "\"HELLO\"",
            "replace_all": false,
        }),
    )
    .await;

    assert_contains(&result, "Edit safety: text-edit-safe");
    assert_contains(&result, "Write semantics: atomic write + reindex");
    assert_contains(&result, "edited within `hello`");
    assert_contains(&result, "(1 replacement(s)");

    let on_disk = fx.read("src/lib.rs");
    assert!(on_disk.contains("\"HELLO\""), "replaced: {on_disk}");
    assert!(!on_disk.contains("\"hello\""), "old text gone: {on_disk}");
}

#[tokio::test]
async fn edit_within_symbol_returns_body_preview_when_old_text_missing() {
    let original = "fn hello() {\n    println!(\"hi\");\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "edit_within_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "old_text": "nonexistent_marker_xyz",
            "new_text": "replacement",
            "replace_all": false,
        }),
    )
    .await;

    assert_contains(&result, "not found within symbol `hello`");
    // Preview should include the real body so the caller can correct the input.
    assert_contains(&result, "println!");
}

// ─── batch_edit ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn batch_edit_applies_edits_across_files_atomically() {
    let fx = Fixture::new(&[
        ("src/a.rs", "fn alpha() {\n    a_old();\n}\n"),
        ("src/b.rs", "fn beta() {\n    b_old();\n}\n"),
    ]);

    let result = call(
        &fx.server,
        "batch_edit",
        json!({
            "edits": [
                {
                    "path": "src/a.rs",
                    "name": "alpha",
                    "operation": {
                        "type": "edit_within",
                        "old_text": "a_old()",
                        "new_text": "a_new()",
                    },
                },
                {
                    "path": "src/b.rs",
                    "name": "beta",
                    "operation": {
                        "type": "edit_within",
                        "old_text": "b_old()",
                        "new_text": "b_new()",
                    },
                },
            ]
        }),
    )
    .await;

    assert_contains(&result, "Edit safety: structural-edit-safe");
    assert_contains(&result, "Match type: exact");
    assert_contains(
        &result,
        "Write semantics: transactional write + rollback + reindex",
    );
    assert_contains(&result, "Evidence: 2 edit target(s) across 2 file(s)");
    assert_contains(&result, "2 edit(s) across 2 file(s)");

    let a = fx.read("src/a.rs");
    let b = fx.read("src/b.rs");
    assert!(a.contains("a_new()"), "a updated: {a}");
    assert!(b.contains("b_new()"), "b updated: {b}");
}

#[tokio::test]
async fn batch_edit_dry_run_does_not_touch_disk() {
    let original_a = "fn alpha() {\n    a_old();\n}\n";
    let original_b = "fn beta() {\n    b_old();\n}\n";
    let fx = Fixture::new(&[("src/a.rs", original_a), ("src/b.rs", original_b)]);

    let result = call(
        &fx.server,
        "batch_edit",
        json!({
            "edits": [
                {
                    "path": "src/a.rs",
                    "name": "alpha",
                    "operation": {
                        "type": "edit_within",
                        "old_text": "a_old()",
                        "new_text": "a_new()",
                    },
                },
                {
                    "path": "src/b.rs",
                    "name": "beta",
                    "operation": {
                        "type": "edit_within",
                        "old_text": "b_old()",
                        "new_text": "b_new()",
                    },
                },
            ],
            "dry_run": true,
        }),
    )
    .await;

    assert_contains(&result, "Write semantics: dry run (no writes)");
    assert_eq!(fx.read("src/a.rs"), original_a);
    assert_eq!(fx.read("src/b.rs"), original_b);
}

// ─── batch_rename ────────────────────────────────────────────────────────────

#[tokio::test]
async fn batch_rename_updates_definition_and_callers() {
    let lib = "pub fn old_name() {}\n";
    let call_site = "use crate::old_name;\n\nfn caller() {\n    old_name();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", lib), ("src/caller.rs", call_site)]);

    let result = call(
        &fx.server,
        "batch_rename",
        json!({
            "path": "src/lib.rs",
            "name": "old_name",
            "new_name": "new_name",
        }),
    )
    .await;

    assert_contains(&result, "Edit safety: structural-edit-safe");
    assert_contains(&result, "Match type: constrained");
    assert_contains(
        &result,
        "Write semantics: transactional write + rollback + reindex",
    );
    assert_contains(&result, "old_name");
    assert_contains(&result, "new_name");

    let lib_after = fx.read("src/lib.rs");
    let caller_after = fx.read("src/caller.rs");
    assert!(
        lib_after.contains("pub fn new_name()"),
        "definition renamed: {lib_after}"
    );
    assert!(!lib_after.contains("old_name"), "old def gone: {lib_after}");
    assert!(
        caller_after.contains("new_name();"),
        "call site renamed: {caller_after}"
    );
    assert!(
        !caller_after.contains("old_name();"),
        "old call gone: {caller_after}"
    );
}

#[tokio::test]
async fn batch_rename_dry_run_does_not_touch_disk() {
    let lib = "pub fn old_name() {}\n";
    let call_site = "use crate::old_name;\n\nfn caller() {\n    old_name();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", lib), ("src/caller.rs", call_site)]);

    let result = call(
        &fx.server,
        "batch_rename",
        json!({
            "path": "src/lib.rs",
            "name": "old_name",
            "new_name": "new_name",
            "dry_run": true,
        }),
    )
    .await;

    assert_contains(&result, "Write semantics: dry run (no writes)");
    assert_eq!(fx.read("src/lib.rs"), lib);
    assert_eq!(fx.read("src/caller.rs"), call_site);
}

#[tokio::test]
async fn batch_rename_code_only_excludes_docs_from_qualified_usage_scan() {
    let lib = "pub struct Widget;\n\nimpl Widget {\n    pub fn new() -> Self { Widget }\n}\n";
    let caller = "fn build() {\n    let _ = crate::Widget::new();\n}\n";
    let docs = "# Example\n\nCall `Widget::new()` from Rust code.\n";
    let fx = Fixture::new(&[
        ("src/lib.rs", lib),
        ("src/caller.rs", caller),
        ("docs/readme.md", docs),
    ]);

    let broad_result = call(
        &fx.server,
        "batch_rename",
        json!({
            "path": "src/lib.rs",
            "name": "Widget",
            "new_name": "Gadget",
            "dry_run": true,
        }),
    )
    .await;
    assert_contains(&broad_result, "docs/readme.md");

    let code_only_result = call(
        &fx.server,
        "batch_rename",
        json!({
            "path": "src/lib.rs",
            "name": "Widget",
            "new_name": "Gadget",
            "dry_run": true,
            "code_only": true,
        }),
    )
    .await;

    assert_contains(&code_only_result, "src/lib.rs");
    assert_contains(&code_only_result, "src/caller.rs");
    assert_not_contains(&code_only_result, "docs/readme.md");
    assert_eq!(fx.read("docs/readme.md"), docs);
}

// ─── batch_insert ────────────────────────────────────────────────────────────

#[tokio::test]
async fn batch_insert_adds_same_content_to_multiple_files() {
    let a = "fn alpha() {\n    a();\n}\n";
    let b = "fn beta() {\n    b();\n}\n";
    let fx = Fixture::new(&[("src/a.rs", a), ("src/b.rs", b)]);

    let result = call(
        &fx.server,
        "batch_insert",
        json!({
            "content": "fn shared() {\n    s();\n}\n",
            "position": "after",
            "targets": [
                { "path": "src/a.rs", "name": "alpha" },
                { "path": "src/b.rs", "name": "beta" },
            ],
        }),
    )
    .await;

    assert_contains(&result, "Edit safety: structural-edit-safe");
    assert_contains(&result, "Match type: exact");
    assert_contains(
        &result,
        "Write semantics: transactional write + rollback + reindex",
    );
    assert_contains(&result, "Evidence: 2 target(s) across 2 file(s)");
    assert_contains(&result, "2 edit(s) across 2 file(s)");

    let a_after = fx.read("src/a.rs");
    let b_after = fx.read("src/b.rs");
    assert!(a_after.contains("fn shared()"), "a got insert: {a_after}");
    assert!(b_after.contains("fn shared()"), "b got insert: {b_after}");
    assert!(a_after.contains("fn alpha()"), "a anchor kept: {a_after}");
    assert!(b_after.contains("fn beta()"), "b anchor kept: {b_after}");
}

#[tokio::test]
async fn batch_insert_dry_run_does_not_touch_disk() {
    let a = "fn alpha() {\n    a();\n}\n";
    let b = "fn beta() {\n    b();\n}\n";
    let fx = Fixture::new(&[("src/a.rs", a), ("src/b.rs", b)]);

    let result = call(
        &fx.server,
        "batch_insert",
        json!({
            "content": "fn shared() {}\n",
            "position": "after",
            "targets": [
                { "path": "src/a.rs", "name": "alpha" },
                { "path": "src/b.rs", "name": "beta" },
            ],
            "dry_run": true,
        }),
    )
    .await;

    assert_contains(&result, "Write semantics: dry run (no writes)");
    assert_eq!(fx.read("src/a.rs"), a);
    assert_eq!(fx.read("src/b.rs"), b);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────
// Named in CONTEXT.md/todo.md: symbol at file end, empty file, byte-range
// collision. The refactor must preserve these behaviors byte-identically.

#[tokio::test]
async fn replace_symbol_body_handles_symbol_at_file_end_without_trailing_newline() {
    // Final symbol in the file, no trailing newline — exercises the byte-range
    // slicer at the EOF boundary.
    let original = "fn keeper() {\n    k();\n}\n\nfn tail() {\n    old();\n}";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "replace_symbol_body",
        json!({
            "path": "src/lib.rs",
            "name": "tail",
            "new_body": "fn tail() {\n    new();\n}",
        }),
    )
    .await;

    assert_contains(&result, "Write semantics: atomic write + reindex");
    assert_contains(&result, "replaced fn `tail`");

    let on_disk = fx.read("src/lib.rs");
    assert!(
        on_disk.contains("new();"),
        "replaced body at EOF: {on_disk}"
    );
    assert!(on_disk.contains("fn keeper()"), "sibling kept: {on_disk}");
    assert!(!on_disk.contains("old();"), "old body removed: {on_disk}");
}

#[tokio::test]
async fn delete_symbol_at_file_end_leaves_predecessor_intact() {
    // Deleting the final symbol should not corrupt the preceding one.
    let original = "fn keeper() {\n    k();\n}\n\nfn goner() {\n    g();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "delete_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "goner",
        }),
    )
    .await;

    assert_contains(&result, "deleted fn `goner`");

    let on_disk = fx.read("src/lib.rs");
    assert!(!on_disk.contains("goner"), "target removed: {on_disk}");
    assert!(
        on_disk.contains("fn keeper()"),
        "predecessor intact: {on_disk}"
    );
    assert!(
        on_disk.contains("k();"),
        "predecessor body intact: {on_disk}"
    );
}

#[tokio::test]
async fn insert_symbol_after_last_symbol_appends_at_eof() {
    // `after` on the final symbol must append at EOF without mangling the
    // anchor's trailing bytes.
    let original = "fn anchor() {\n    a();\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "insert_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "anchor",
            "content": "fn appended() {\n    x();\n}",
            "position": "after",
        }),
    )
    .await;

    assert_contains(&result, "inserted after `anchor`");

    let on_disk = fx.read("src/lib.rs");
    let anchor_pos = on_disk.find("anchor").expect("anchor kept");
    let appended_pos = on_disk.find("appended").expect("appended added");
    assert!(anchor_pos < appended_pos, "order preserved: {on_disk}");
    assert!(on_disk.contains("a();"), "anchor body intact: {on_disk}");
    assert!(on_disk.contains("x();"), "new body present: {on_disk}");
}

#[tokio::test]
async fn edit_within_symbol_on_empty_file_returns_not_found() {
    // Empty file indexes no symbols — the resolver reports the miss. Lock in
    // the not-found shape; the refactor must keep this diagnostic.
    let fx = Fixture::new(&[("src/empty.rs", "")]);

    let result = call(
        &fx.server,
        "edit_within_symbol",
        json!({
            "path": "src/empty.rs",
            "name": "nothing_here",
            "old_text": "x",
            "new_text": "y",
            "replace_all": false,
        }),
    )
    .await;

    assert!(
        result.to_lowercase().contains("not found"),
        "expected not-found error; got: {result}"
    );
    assert_not_contains(&result, "atomic write");
    // File remains empty — no write happened on a lookup miss.
    assert_eq!(fx.read("src/empty.rs"), "");
}

#[tokio::test]
async fn replace_symbol_body_on_empty_file_returns_not_found() {
    let fx = Fixture::new(&[("src/empty.rs", "")]);

    let result = call(
        &fx.server,
        "replace_symbol_body",
        json!({
            "path": "src/empty.rs",
            "name": "missing",
            "new_body": "fn missing() {}",
        }),
    )
    .await;

    assert!(
        result.to_lowercase().contains("not found"),
        "expected not-found error; got: {result}"
    );
    assert_eq!(fx.read("src/empty.rs"), "");
}

#[tokio::test]
async fn batch_edit_rejects_overlapping_ranges_on_same_symbol() {
    // Two operations targeting the same symbol produce overlapping byte
    // ranges. `batch_edit` must reject transactionally — no partial write.
    let original = "fn foo() {\n    f();\n}\n";
    let fx = Fixture::new(&[("src/a.rs", original)]);

    let result = call(
        &fx.server,
        "batch_edit",
        json!({
            "edits": [
                {
                    "path": "src/a.rs",
                    "name": "foo",
                    "operation": { "type": "delete" },
                },
                {
                    "path": "src/a.rs",
                    "name": "foo",
                    "operation": { "type": "delete" },
                },
            ]
        }),
    )
    .await;

    assert!(
        result.to_lowercase().contains("overlapping"),
        "expected overlapping-range rejection; got: {result}"
    );
    assert_not_contains(&result, "atomic write");
    // Transactional semantics: no file write on range collision.
    assert_eq!(fx.read("src/a.rs"), original);
}

#[tokio::test]
async fn edit_within_symbol_replace_all_replaces_every_occurrence() {
    // Multi-occurrence edit_within — exercises the replace_all=true branch
    // alongside the single-replacement case already covered above.
    let original = "fn target() {\n    log(\"x\");\n    log(\"x\");\n    log(\"x\");\n}\n";
    let fx = Fixture::new(&[("src/lib.rs", original)]);

    let result = call(
        &fx.server,
        "edit_within_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "target",
            "old_text": "log(\"x\")",
            "new_text": "log(\"y\")",
            "replace_all": true,
        }),
    )
    .await;

    assert_contains(&result, "edited within `target`");
    assert_contains(&result, "(3 replacement(s)");

    let on_disk = fx.read("src/lib.rs");
    assert_eq!(
        on_disk.matches("log(\"y\")").count(),
        3,
        "all replaced: {on_disk}"
    );
    assert!(!on_disk.contains("log(\"x\")"), "no residue: {on_disk}");
}
