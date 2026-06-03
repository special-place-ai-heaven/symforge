// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::fs;
use std::path::Path;

use symforge::live_index::LiveIndex;
use symforge::protocol::edit_plan::plan_edit;
use tempfile::TempDir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn build_index(source: &str) -> (TempDir, symforge::live_index::SharedIndex) {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_file(dir.path(), "src/lib.rs", source);
    let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
    (dir, shared)
}

fn canonical_symbol_line(index: &LiveIndex, path: &str, name: &str) -> u32 {
    let detail = index
        .capture_symbol_detail_view(path)
        .expect("fixture file should be indexed");
    let symbol = detail
        .symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("{name} should be indexed"));
    symbol.line_range.0 + 1
}

fn assert_plan_line_matches_selector(index: &LiveIndex, name: &str, selector_line: u32) {
    let plan = plan_edit(index, &format!("src/lib.rs::{name}"));
    let expected = format!("{name} in src/lib.rs (lines {selector_line}-");
    assert!(
        plan.contains(&expected),
        "edit_plan should report the one-based selector line accepted by find_references\n\
         expected fragment: {expected:?}\n\
         actual plan:\n{plan}"
    );

    index
        .capture_find_references_view_for_symbol(
            "src/lib.rs",
            name,
            Some("fn"),
            Some(selector_line),
            Some("call"),
            10,
        )
        .unwrap_or_else(|error| {
            panic!("find_references should accept edit_plan's reported line: {error}")
        });
}

#[test]
fn edit_plan_symbol_lines_match_find_references_selectors() {
    let source = "\
fn caller() {
    documented_target();
    plain_target();
    let worker = Worker;
    worker.nested_target();
}

/// First doc line.
/// Second doc line.
fn documented_target() {}

fn plain_target() {}

struct Worker;

impl Worker {
    /// Method doc.
    fn nested_target(&self) {}
}
";

    let (_dir, shared) = build_index(source);
    let index = shared.read();

    for (name, expected_line) in [
        ("documented_target", 10),
        ("plain_target", 12),
        ("nested_target", 18),
    ] {
        let selector_line = canonical_symbol_line(&index, "src/lib.rs", name);
        assert_eq!(selector_line, expected_line);
        assert_plan_line_matches_selector(&index, name, selector_line);
    }
}
