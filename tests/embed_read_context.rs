#![cfg(feature = "embed")]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::parity::read_context::{FileContextRequest, RepoMapRequest};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn open(runtime: &ProcessIndexRuntime, root: &std::path::Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

#[test]
fn repo_map_preserves_compact_full_tree_estimate_and_project_scope() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/lib.rs"), b"pub struct Entry;\n").unwrap();
    fs::write(root.path().join("src/engine.rs"), b"pub fn run() {}\n").unwrap();
    fs::write(root.path().join("README.md"), b"# Project\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());

    let compact = handle
        .query(
            &QueryRequest::RepoMap(RepoMapRequest::default()),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::RepoMap(compact) = compact.value() else {
        panic!("repo map output");
    };
    assert_eq!(compact.detail, "compact");
    assert_eq!(compact.shown_files, 0);
    assert!(compact.rendered.contains("Index:"));
    assert!(compact.rendered.contains("src/"));

    let full = handle
        .query(
            &QueryRequest::RepoMap(RepoMapRequest {
                detail: Some("full".into()),
                max_files: Some(1),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::RepoMap(full) = full.value() else {
        panic!("full repo map output");
    };
    assert_eq!(full.detail, "full");
    assert_eq!(full.shown_files, 1);
    assert!(full.rendered.contains("more files"));

    let tree = handle
        .query(
            &QueryRequest::RepoMap(RepoMapRequest {
                detail: Some("tree".into()),
                path: Some("src".into()),
                depth: Some(2),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::RepoMap(tree) = tree.value() else {
        panic!("tree repo map output");
    };
    assert_eq!(tree.detail, "tree");
    assert_eq!(tree.shown_files, 2);
    assert!(tree.rendered.contains("engine.rs"));
    assert!(!tree.rendered.contains("README.md"));

    let capped = handle
        .query(
            &QueryRequest::RepoMap(RepoMapRequest {
                detail: Some("full".into()),
                ..Default::default()
            }),
            QueryLimits {
                max_results: 2,
                ..QueryLimits::default()
            },
        )
        .unwrap();
    let QueryOutput::RepoMap(capped_map) = capped.value() else {
        panic!("capped repo map output");
    };
    assert_eq!(capped_map.rendered.lines().count(), 2);
    assert_eq!(capped_map.shown_files, 1);

    let estimate = handle
        .query(
            &QueryRequest::RepoMap(RepoMapRequest {
                estimate: true,
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::RepoMap(estimate) = estimate.value() else {
        panic!("estimate repo map output");
    };
    assert!(estimate.estimated_tokens.is_some());

    let foreign = handle
        .query(
            &QueryRequest::RepoMap(RepoMapRequest {
                project: Some("another-project".into()),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(foreign.kind(), QueryRefusalKind::AdmissionUnavailable);
    handle.close().unwrap();
}

#[test]
fn file_context_preserves_section_filtering_and_estimate() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lib.rs"),
        b"use std::path::Path;\npub fn target(_path: &Path) -> bool { true }\n",
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());

    let outline = handle
        .query(
            &QueryRequest::FileContext(FileContextRequest {
                path: "lib.rs".into(),
                sections: Some(vec!["outline".into()]),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileContext(outline) = outline.value() else {
        panic!("file context output");
    };
    assert!(outline.rendered.contains("target"));
    assert!(!outline.rendered.contains("Imports from"));
    assert!(!outline.rendered.contains("Used by"));

    let imports = handle
        .query(
            &QueryRequest::FileContext(FileContextRequest {
                path: "lib.rs".into(),
                sections: Some(vec!["imports".into()]),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileContext(imports) = imports.value() else {
        panic!("imports file context output");
    };
    assert!(imports.rendered.contains("Imports from"));
    assert!(!imports.rendered.contains("target  L"));

    let estimate = handle
        .query(
            &QueryRequest::FileContext(FileContextRequest {
                path: "lib.rs".into(),
                estimate: true,
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileContext(estimate) = estimate.value() else {
        panic!("estimate file context output");
    };
    assert!(estimate.estimated_tokens.is_some());
    handle.close().unwrap();
}
