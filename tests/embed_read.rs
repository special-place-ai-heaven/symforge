#![cfg(feature = "embed")]

#[cfg(windows)]
#[test]
fn disk_fallback_refuses_case_alias_of_an_admitted_file() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Source")).unwrap();
    fs::write(
        root.path().join("Source/Exact.rs"),
        b"pub fn original() {}\n",
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    for path in ["Source/exact.rs", "source/Exact.rs"] {
        let result = handle.query(
            &QueryRequest::FileContent(request(path)),
            QueryLimits::default(),
        );
        assert!(
            result.is_err(),
            "an alias must not bypass indexed path admission"
        );
    }
    handle.close().unwrap();
}

#[test]
fn disk_fallback_refuses_bytes_from_a_replacement_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("source");
    let moved = parent.path().join("original");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("lib.rs"), b"pub fn original() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, &root);
    fs::rename(&root, &moved).unwrap();
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("replacement.txt"),
        b"replacement directory content\n",
    )
    .unwrap();
    let refusal = handle
        .query(
            &QueryRequest::FileContent(request("replacement.txt")),
            QueryLimits::default(),
        )
        .unwrap_err();
    assert!(matches!(
        refusal.kind(),
        QueryRefusalKind::SourceUnavailable
            | QueryRefusalKind::AdmissionUnavailable
            | QueryRefusalKind::StalePublication
    ));
    handle.close().unwrap();
}

#[test]
fn limited_read_retains_whole_formatted_output_for_retrieval() {
    let root = tempfile::tempdir().unwrap();
    let source = (0..30)
        .map(|i| format!("pub fn item_{i}() {{}}\n"))
        .collect::<String>();
    fs::write(root.path().join("lib.rs"), source).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let claim = handle
        .query_with_session(
            &QueryRequest::FileContent(FileContentRequest {
                max_tokens: Some(2000),
                ..request("lib.rs")
            }),
            QueryLimits {
                max_results: 1,
                max_bytes: 130,
            },
            &session,
            None,
        )
        .unwrap();
    assert!(claim.truncated());
    let QueryOutput::FileContent(read) = claim.value() else {
        panic!("file content");
    };
    assert!(!read.rendered.contains("item_29"));
    let retrieved = handle
        .query_with_session(
            &QueryRequest::Retrieve {
                handle: claim.retrieve_handle().unwrap().to_owned(),
                offset: 0,
            },
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::RetrievedOutput(output) = retrieved.value() else {
        panic!("retrieved");
    };
    assert!(output.next_offset.is_none());
    let record: serde_json::Value = serde_json::from_slice(&output.bytes).unwrap();
    assert!(
        record["output"]["FileContent"]["rendered"]
            .as_str()
            .unwrap()
            .contains("item_29")
    );
    assert_eq!(record["truncated"], false);
}
#[test]
fn token_limited_read_retains_pre_token_cap_content_for_retrieval() {
    let root = tempfile::tempdir().unwrap();
    let source = (0..60)
        .map(|i| format!("pub fn item_{i}() {{}}\n"))
        .collect::<String>();
    fs::write(root.path().join("lib.rs"), source).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let bounded = handle
        .query_with_session(
            &QueryRequest::FileContent(FileContentRequest {
                max_tokens: Some(30),
                ..request("lib.rs")
            }),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    assert!(bounded.truncated());
    let QueryOutput::FileContent(output) = bounded.value() else {
        panic!("file content");
    };
    assert!(!output.rendered.contains("item_59"));
    let retrieved = handle
        .query_with_session(
            &QueryRequest::Retrieve {
                handle: bounded.retrieve_handle().unwrap().into(),
                offset: 0,
            },
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::RetrievedOutput(output) = retrieved.value() else {
        panic!("cached output");
    };
    let record: serde_json::Value = serde_json::from_slice(&output.bytes).unwrap();
    assert!(
        record["output"]["FileContent"]["rendered"]
            .as_str()
            .unwrap()
            .contains("item_59")
    );
    assert_eq!(record["truncated"], false);
}

use std::{
    fs,
    time::{Duration, Instant},
};
use symforge::embed::parity::read::{FileContentRequest, ReadMode, SourcePageRequest};
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
fn request(path: &str) -> FileContentRequest {
    FileContentRequest {
        path: path.into(),
        ..Default::default()
    }
}
#[test]
fn full_read_selectors_keep_mode_alias_and_occurrence_semantics() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"),b"pub fn target() {\r\n let first = \"needle\";\r\n let second = \"needle\";\r\n}\r\npub fn tail() {}\r\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let claim = handle
        .query(
            &QueryRequest::FileContent(FileContentRequest {
                offset: Some(1),
                limit: Some(2),
                ..request("lib.rs")
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileContent(read) = claim.value() else {
        panic!("content");
    };
    assert_eq!(
        read.rendered,
        " let first = \"needle\";\n let second = \"needle\";"
    );
    let selected = handle
        .query(
            &QueryRequest::FileContent(FileContentRequest {
                mode: Some(ReadMode::Match),
                around_match: Some("NEEDLE".into()),
                match_occurrence: Some(2),
                context_lines: Some(0),
                ..request("lib.rs")
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileContent(read) = selected.value() else {
        panic!("match");
    };
    assert!(read.rendered.contains("3:  let second"));
    assert!(!read.rendered.contains("first"));
    let chunk = handle
        .query(
            &QueryRequest::FileContent(FileContentRequest {
                mode: Some(ReadMode::Chunk),
                chunk_index: Some(2),
                max_lines: Some(2),
                ..request("lib.rs")
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileContent(read) = chunk.value() else {
        panic!("chunk");
    };
    assert!(read.rendered.contains("chunk 2/3, lines 3-4"));
    let symbol = handle
        .query(
            &QueryRequest::FileContent(FileContentRequest {
                mode: Some(ReadMode::Symbol),
                around_symbol: Some("tail".into()),
                ..request("lib.rs")
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileContent(read) = symbol.value() else {
        panic!("symbol");
    };
    assert!(read.rendered.contains("5: pub fn tail()"));
    let conflict = handle
        .query(
            &QueryRequest::FileContent(FileContentRequest {
                offset: Some(1),
                start_line: Some(2),
                ..request("lib.rs")
            }),
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(conflict.kind(), QueryRefusalKind::InvalidRequest);
}
#[test]
fn exact_source_pages_reconstruct_a_large_single_line_and_refuse_stale_resume() {
    let root = tempfile::tempdir().unwrap();
    let source = format!("// {}\r\npub fn target() {{}}\r\n", "é".repeat(600_000)).into_bytes();
    fs::write(root.path().join("large.rs"), &source).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let limits = QueryLimits {
        max_results: 10,
        max_bytes: 16_000,
    };
    let mut reconstructed = Vec::new();
    let mut offset = 0;
    let mut publication = None;
    loop {
        let claim = handle
            .query(
                &QueryRequest::SourcePage(SourcePageRequest {
                    path: "large.rs".into(),
                    offset,
                    expected_publication: publication.clone(),
                }),
                limits,
            )
            .unwrap();
        if publication.is_none() {
            publication = Some(claim.publication_identity().to_owned());
        }
        let QueryOutput::SourcePage(page) = claim.value() else {
            panic!("source page");
        };
        assert_eq!(page.byte_start, offset);
        assert_eq!(page.total_bytes, source.len() as u64);
        assert!(!page.bytes.is_empty());
        reconstructed.extend_from_slice(&page.bytes);
        let Some(next) = page.next_offset else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    assert_eq!(reconstructed, source);
    fs::write(root.path().join("large.rs"), b"pub fn changed() {}\n").unwrap();
    handle.request_refresh().unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if handle.runtime_view().phase == SourceRuntimePhase::Current {
            if let Ok(claim) = handle.query(
                &QueryRequest::SourcePage(SourcePageRequest {
                    path: "large.rs".into(),
                    offset: 0,
                    expected_publication: None,
                }),
                limits,
            ) {
                if claim.publication_identity() != publication.as_deref().unwrap() {
                    break;
                }
            }
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let stale = handle
        .query(
            &QueryRequest::SourcePage(SourcePageRequest {
                path: "large.rs".into(),
                offset: 1,
                expected_publication: publication,
            }),
            limits,
        )
        .unwrap_err();
    assert_eq!(stale.kind(), QueryRefusalKind::StalePublication);
}
#[test]
fn repeat_read_cache_reuses_source_bound_handle_and_force_refresh_loads_again() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), b"pub fn loaded() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let query = QueryRequest::FileContent(request("lib.rs"));
    let first = handle
        .query_with_session(&query, QueryLimits::default(), &session, None)
        .unwrap();
    let second = handle
        .query_with_session(&query, QueryLimits::default(), &session, None)
        .unwrap();
    let QueryOutput::FileContent(cached) = second.value() else {
        panic!("cached read");
    };
    assert!(cached.cache_hit);
    assert_eq!(first.retrieve_handle(), second.retrieve_handle());
    let refreshed = handle
        .query_with_session(
            &QueryRequest::FileContent(FileContentRequest {
                force_refresh: Some(true),
                ..request("lib.rs")
            }),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    let QueryOutput::FileContent(read) = refreshed.value() else {
        panic!("fresh read");
    };
    assert!(!read.cache_hit);
    assert!(read.rendered.contains("pub fn loaded"));
}

#[test]
fn exact_path_context_never_returns_old_source_after_a_completed_disk_write() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn before() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();
    let request = QueryRequest::FileContent(FileContentRequest {
        path: "lib.rs".into(),
        ..Default::default()
    });
    handle
        .query_with_session(&request, QueryLimits::default(), &session, None)
        .unwrap();
    fs::write(
        root.path().join("lib.rs"),
        "pub fn after_write_completed() {}\n",
    )
    .unwrap();
    match handle.query_with_session(&request, QueryLimits::default(), &session, None) {
        Ok(claim) => {
            let QueryOutput::FileContent(content) = claim.value() else {
                panic!("file content")
            };
            assert!(
                !content.cache_hit,
                "a completed disk write must invalidate old context before reuse"
            );
            assert!(content.rendered.contains("after_write_completed"));
            assert!(!content.rendered.contains("fn before"));
        }
        Err(refusal) => assert!(matches!(
            refusal.kind(),
            QueryRefusalKind::SourceUnavailable | QueryRefusalKind::StalePublication
        )),
    }
}
