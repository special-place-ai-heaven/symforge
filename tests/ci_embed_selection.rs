//! The `server-embed-parity` CI job runs the integration tests that need the
//! `embed` feature by name. A new test file gated on `embed` must be selected
//! there, or no CI job ever runs it; a selected name must still exist.

use std::fs;
use std::path::Path;

const STEP_PREFIX: &str = "run: cargo test --features embed,__test-internals --test ";

/// The `--test` selectors of the embed integration step in ci.yml.
fn ci_selectors(workflow: &str) -> Vec<String> {
    let lines: Vec<&str> = workflow
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(STEP_PREFIX))
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "exactly one embed integration step: {lines:?}"
    );
    let command = lines[0]
        .strip_prefix("run: ")
        .unwrap()
        .split(" -- ")
        .next()
        .unwrap();
    let words: Vec<&str> = command.split(' ').collect();
    words
        .windows(2)
        .filter(|pair| pair[0] == "--test")
        .map(|pair| pair[1].trim_matches('\'').to_owned())
        .collect()
}

fn selected(selectors: &[String], stem: &str) -> bool {
    selectors
        .iter()
        .any(|selector| match selector.strip_suffix('*') {
            Some(prefix) => stem.starts_with(prefix),
            None => stem == selector,
        })
}

/// True when a `cfg`/`cfg_attr` attribute (not a comment) names `feature = "embed"`.
fn gated_on_embed(source: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim_start();
        !line.starts_with("//")
            && (line.contains("cfg(") || line.contains("cfg_attr("))
            && line.contains("feature = \"embed\"")
    })
}

#[test]
fn ci_embed_step_selects_every_embed_gated_integration_test() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let selectors = ci_selectors(&workflow);
    let mut stems = Vec::new();
    let mut missing = Vec::new();
    for entry in fs::read_dir(root.join("tests")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        if gated_on_embed(&fs::read_to_string(&path).unwrap()) && !selected(&selectors, &stem) {
            missing.push(stem.clone());
        }
        stems.push(stem);
    }
    missing.sort();
    assert!(
        missing.is_empty(),
        "embed-gated integration tests the CI embed step does not select: {missing:?}"
    );
    let stale: Vec<&String> = selectors
        .iter()
        .filter(|selector| {
            !stems
                .iter()
                .any(|stem| selected(&[(*selector).clone()], stem))
        })
        .collect();
    assert!(
        stale.is_empty(),
        "CI embed step selects no test file: {stale:?}"
    );
}
