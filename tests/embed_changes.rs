#![cfg(feature = "embed")]

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use symforge::embed::parity::changes::{ChangeMode, DiffSymbolsRequest, WhatChangedRequest};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
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

fn commit(repo: &git2::Repository, paths: &[&str]) -> git2::Oid {
    let mut index = repo.index().unwrap();
    for path in paths {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    let previous = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<_> = previous.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "fixture",
        &tree,
        &parents,
    )
    .unwrap()
}

#[test]
fn changes_preserve_timestamp_git_scope_worktree_and_symbol_evidence() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join(".gitignore"), ".symforge/\n").unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn old() {}\npub fn changed() -> u8 { 1 }\n",
    )
    .unwrap();
    fs::write(root.path().join("README.md"), "# Before\n").unwrap();
    let repo = git2::Repository::init(root.path()).unwrap();
    let base = commit(&repo, &[".gitignore", "src/lib.rs", "README.md"]);
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn added() {}\npub fn changed() -> u16 { 2 }\n",
    )
    .unwrap();
    fs::write(root.path().join("README.md"), "# After\n").unwrap();
    let target = commit(&repo, &["src/lib.rs", "README.md"]);
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn working() {}\npub fn changed() -> u32 { 3 }\n",
    )
    .unwrap();
    fs::write(root.path().join("notes.json"), "{\"note\":\"fixture\"}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());

    let timestamp = handle
        .query(
            &QueryRequest::WhatChanged(WhatChangedRequest {
                since: Some(0),
                path_prefix: Some("src/".into()),
                language: Some("Rust".into()),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::WhatChanged(timestamp) = timestamp.value() else {
        panic!("timestamp changes")
    };
    assert_eq!(timestamp.mode, ChangeMode::Timestamp { since: 0 });
    assert_eq!(timestamp.paths, ["src/lib.rs"]);

    let worktree = handle
        .query(
            &QueryRequest::WhatChanged(WhatChangedRequest {
                uncommitted: Some(true),
                include_symbol_diff: Some(true),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::WhatChanged(worktree) = worktree.value() else {
        panic!("working changes")
    };
    assert_eq!(worktree.mode, ChangeMode::Uncommitted);
    assert_eq!(worktree.paths, ["src/lib.rs"]);
    let delta = &worktree.symbol_diff.as_ref().unwrap().files[0];
    assert!(delta.added.iter().any(|name| name == "working"));
    assert!(delta.removed.iter().any(|name| name == "added"));
    assert!(delta.modified.iter().any(|name| name == "changed"));

    let committed = handle
        .query(
            &QueryRequest::DiffSymbols(DiffSymbolsRequest {
                base: Some(base.to_string()),
                target: Some(target.to_string()),
                path_prefix: Some("src".into()),
                language: Some("Rust".into()),
                compact: Some(false),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::DiffSymbols(committed) = committed.value() else {
        panic!("committed changes")
    };
    assert_eq!(committed.files.len(), 1);
    assert!(committed.files[0].added.iter().any(|name| name == "added"));
    assert!(committed.files[0].removed.iter().any(|name| name == "old"));
    assert!(!committed.rendered.contains("working"));

    let all = handle
        .query(
            &QueryRequest::WhatChanged(WhatChangedRequest {
                uncommitted: Some(true),
                code_only: Some(false),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::WhatChanged(all) = all.value() else {
        panic!("unfiltered changes")
    };
    assert!(all.paths.iter().any(|path| path == "notes.json"));
}

#[test]
fn changes_bound_rows_preserve_full_cache_and_reject_unknown_filters() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("one.rs"), "pub fn one() {}\n").unwrap();
    fs::write(root.path().join("two.rs"), "pub fn two() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let request = WhatChangedRequest {
        since: Some(0),
        ..Default::default()
    };
    let bounded = handle
        .query(
            &QueryRequest::WhatChanged(request.clone()),
            QueryLimits {
                max_results: 2,
                max_bytes: 65536,
            },
        )
        .unwrap();
    assert!(bounded.usage().rows <= 2);
    assert!(bounded.truncated());
    let session = handle.new_query_session().unwrap();
    let capped = handle
        .query_with_session(
            &QueryRequest::WhatChanged(WhatChangedRequest {
                max_tokens: Some(5),
                ..request.clone()
            }),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    assert!(capped.truncated());
    let restored = handle
        .query_with_session(
            &QueryRequest::Retrieve {
                handle: capped.retrieve_handle().unwrap().into(),
                offset: 0,
            },
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::RetrievedOutput(restored) = restored.value() else {
        panic!("full changes")
    };
    let full: serde_json::Value = serde_json::from_slice(&restored.bytes).unwrap();
    assert!(full["output"]["WhatChanged"]["rendered"]
        .as_str()
        .unwrap()
        .contains("two.rs"));
    let estimate = handle
        .query(
            &QueryRequest::WhatChanged(WhatChangedRequest {
                estimate: Some(true),
                ..request.clone()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::WhatChanged(estimate) = estimate.value() else {
        panic!("estimate")
    };
    assert!(estimate.estimated_tokens.unwrap() > 0);
    let refusal = handle
        .query(
            &QueryRequest::WhatChanged(WhatChangedRequest {
                language: Some("typo-language".into()),
                ..request
            }),
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(refusal.kind(), QueryRefusalKind::UnsupportedOption);
    assert!(serde_json::from_value::<WhatChangedRequest>(
        serde_json::json!({"since":0,"ignore_scope":true})
    )
    .is_err());
}
