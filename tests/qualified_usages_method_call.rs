use std::fs;
use std::path::Path;

use symforge::live_index::{FindReferencesView, LiveIndex};
use tempfile::TempDir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn build_index(files: &[(&str, &str)]) -> (TempDir, symforge::live_index::SharedIndex) {
    let dir = TempDir::new().expect("failed to create tempdir");
    for (name, content) in files {
        write_file(dir.path(), name, content);
    }
    let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
    (dir, shared)
}

fn reference_lines(view: &FindReferencesView) -> Vec<&str> {
    view.files
        .iter()
        .flat_map(|file| file.hits.iter())
        .flat_map(|hit| hit.context_lines.iter())
        .filter(|line| line.is_reference_line)
        .map(|line| line.text.trim())
        .collect()
}

#[test]
fn free_function_references_exclude_method_call_sites() {
    let source = r#"
fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        input.to_string()
    } else {
        input[..max].to_string()
    }
}

fn sanitize_task_description(input: &str) -> String {
    truncate(input, 500)
}

struct MyType;

impl MyType {
    fn truncate(value: &mut String, max: usize) {
        value.truncate(max);
    }
}

fn unrelated_method_calls(mut s: String, mut digest: String) {
    s.truncate(500);
    digest.truncate(64);
    String::truncate(&mut s, 10);
    MyType::truncate(&mut digest, 9);
}

fn utf8_receiver_does_not_match_or_panic() {
    let mut é = String::new();
    é.truncate(0);
}
"#;

    let (_dir, shared) = build_index(&[("src/orchestrator.rs", source)]);
    let index = shared.read();

    let view = index
        .capture_find_references_view_for_symbol(
            "src/orchestrator.rs",
            "truncate",
            Some("fn"),
            Some(2),
            Some("call"),
            50,
        )
        .expect("free function truncate should resolve");

    let lines = reference_lines(&view);
    assert_eq!(
        lines,
        vec!["truncate(input, 500)"],
        "free function references must exclude method-call sites; view: {view:#?}"
    );
}
