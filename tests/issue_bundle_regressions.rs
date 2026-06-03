// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::fs;
use std::path::Path;

use symforge::live_index::LiveIndex;
use symforge::protocol::format::{
    context_bundle_result_view, context_bundle_result_view_with_max_tokens,
};

fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn zero_caller_struct_bundle_suggests_impl_blocks() {
    let temp = tempfile::tempdir().unwrap();
    write_file(
        temp.path(),
        "src/actors.rs",
        r#"pub struct MyActor;

impl MyActor {
    pub fn new() -> Self {
        Self
    }
}

impl Actor for MyActor {
    fn handle(&self) {}
}
"#,
    );

    let index = LiveIndex::load(temp.path()).unwrap();
    let guard = index.read();
    let view = guard.capture_context_bundle_view("src/actors.rs", "MyActor", Some("struct"), None);
    let rendered = context_bundle_result_view(&view, "full");

    assert!(
        rendered.contains("0 direct callers"),
        "missing zero-caller hint: {rendered}"
    );
    assert!(
        rendered.contains("impl MyActor (src/actors.rs:3)"),
        "missing inherent impl suggestion: {rendered}"
    );
    assert!(
        rendered.contains("impl Actor for MyActor (src/actors.rs:9)"),
        "missing trait impl suggestion: {rendered}"
    );
}

#[test]
fn bundle_max_tokens_keeps_direct_dependency_and_omits_transitive_dependency() {
    let temp = tempfile::tempdir().unwrap();
    write_file(
        temp.path(),
        "src/lib.rs",
        r#"mod types;

use crate::types::Alpha;

fn plan(alpha: Alpha) {
    let _ = alpha;
}
"#,
    );
    write_file(
        temp.path(),
        "src/types.rs",
        r#"pub struct Alpha {
    gamma: Gamma,
}

pub struct Gamma {
    payload0: [u8; 64],
    payload1: [u8; 64],
    payload2: [u8; 64],
    payload3: [u8; 64],
    payload4: [u8; 64],
    payload5: [u8; 64],
    payload6: [u8; 64],
    payload7: [u8; 64],
    payload8: [u8; 64],
    payload9: [u8; 64],
}
"#,
    );

    let index = LiveIndex::load(temp.path()).unwrap();
    let guard = index.read();
    let view = guard.capture_context_bundle_view("src/lib.rs", "plan", Some("fn"), None);
    let rendered = context_bundle_result_view_with_max_tokens(&view, "full", Some(100));

    assert!(
        rendered.contains("── Alpha [struct, src/types.rs:1-3]"),
        "expected direct dependency to remain visible: {rendered}"
    );
    assert!(
        !rendered.contains("── Gamma [struct, src/types.rs:5-15]"),
        "transitive dependency should be omitted when the budget is exhausted: {rendered}"
    );
    assert!(
        rendered.contains("Truncated at ~100 tokens."),
        "expected truncation footer: {rendered}"
    );
    assert!(
        rendered.contains("1 additional type dependencies not shown."),
        "expected omitted dependency count: {rendered}"
    );
}
