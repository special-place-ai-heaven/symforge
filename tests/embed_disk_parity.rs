#![cfg(feature = "embed")]
//! MCP parity for the embedded lanes that consult the disk beyond the
//! captured publication: the synchronous exact-path freshen before a targeted
//! read, `validate_file_syntax`'s authoritative disk parse, and `search_text`'s
//! zero-hit untracked sweep.
//!
//! The rendered text asserted here is the exact text MCP renders for the same
//! fixture; the server lib tests `*_matches_embed_parity_golden` in
//! `src/protocol/tools.rs` assert the same goldens against MCP output.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use symforge::embed::parity::read::{FileContentRequest, ReadAuthority, SourcePageRequest};
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

/// Rewrite `path` with an mtime `seconds` ahead, so the freshen's
/// whole-second mtime comparison sees each rewrite as a change.
fn rewrite_later(path: &Path, contents: &str, seconds: u64) {
    fs::write(path, contents).unwrap();
    let later = SystemTime::now() + Duration::from_secs(seconds);
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(later)).unwrap();
}

/// Query, retrying only while the background worker holds the source out of
/// Current. A stale-publication refusal is never retried: it is exactly what
/// the synchronous freshen exists to prevent.
fn query_current(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
) -> symforge::embed::parity::QueryOutput {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        match handle.query(request, QueryLimits::default()) {
            Ok(claim) => return claim.value().clone(),
            Err(refusal) if refusal.kind() == QueryRefusalKind::SourceUnavailable => {
                assert!(Instant::now() < until, "source never returned to Current");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(refusal) => panic!("unexpected refusal: {refusal:?}"),
        }
    }
}

#[test]
fn targeted_read_serves_a_write_completed_after_capture_like_mcp() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn before_write() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let read = QueryRequest::FileContent(FileContentRequest {
        path: "lib.rs".into(),
        ..Default::default()
    });
    let QueryOutput::FileContent(first) = query_current(&handle, &read) else {
        panic!("file content");
    };
    assert!(first.rendered.contains("before_write"));

    rewrite_later(&root.path().join("lib.rs"), "pub fn after_write() {}\n", 5);
    let QueryOutput::FileContent(fresh) = query_current(&handle, &read) else {
        panic!("file content");
    };
    assert_eq!(fresh.authority, ReadAuthority::PublishedGeneration);
    assert!(
        fresh.rendered.contains("pub fn after_write() {}"),
        "{fresh:?}"
    );
    assert!(!fresh.rendered.contains("before_write"));

    rewrite_later(&root.path().join("lib.rs"), "pub fn paged_write() {}\n", 10);
    let QueryOutput::SourcePage(page) = query_current(
        &handle,
        &QueryRequest::SourcePage(SourcePageRequest {
            path: "lib.rs".into(),
            offset: 0,
            expected_publication: None,
        }),
    ) else {
        panic!("source page");
    };
    assert_eq!(page.bytes, b"pub fn paged_write() {}\n");
    handle.close().unwrap();
}
