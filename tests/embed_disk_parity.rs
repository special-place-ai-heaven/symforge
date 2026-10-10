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
use symforge::embed::parity::search::TextSearchRequest;
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

/// Shared with the MCP goldens.
const INDEXED_SYNTAX_GOLDEN: &str = "Syntax validation: src/broken.rs\nLanguage: Rust\nStatus: partial\nDiagnostic: tree-sitter: syntax error near `pub fn broken( {` (line 1, column 1)\nByte span: 0..16\nSymbols extracted: 0";
const UNINDEXED_SYNTAX_GOLDEN: &str = "Syntax validation: ignored/scratch.rs\nLanguage: Rust\nStatus: partial\nDiagnostic: tree-sitter: syntax error near `pub fn scratch( {` (line 1, column 1)\nByte span: 0..17\nSymbols extracted: 0";

/// MCP's sweep diagnostic for one late untracked file.
fn sweep_golden(path: &str) -> String {
    format!(
        "untracked file may match: 1 untracked path(s) are not indexed. To index the first match, call analyze_file_impact(\"{path}\", new_file=true)."
    )
}

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

fn syntax_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("ignored")).unwrap();
    fs::create_dir_all(root.path().join("config")).unwrap();
    fs::write(root.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::write(root.path().join("src/broken.rs"), "pub fn broken( {\n").unwrap();
    fs::write(
        root.path().join("ignored/scratch.rs"),
        "pub fn scratch( {\n",
    )
    .unwrap();
    fs::write(
        root.path().join("config/app.json"),
        "{\n  \"password\": \"S3cretValue9xAb\"\n}\n",
    )
    .unwrap();
    root
}

#[test]
fn syntax_reparses_unindexed_disk_bytes_and_refuses_with_mcp_metadata() {
    let root = syntax_fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());

    let QueryOutput::Syntax(indexed) = query_current(
        &handle,
        &QueryRequest::Syntax {
            path: "src/broken.rs".into(),
        },
    ) else {
        panic!("syntax");
    };
    assert_eq!(indexed.authority, ReadAuthority::PublishedGeneration);
    assert_eq!(indexed.rendered, INDEXED_SYNTAX_GOLDEN);

    let QueryOutput::Syntax(unindexed) = query_current(
        &handle,
        &QueryRequest::Syntax {
            path: "ignored/scratch.rs".into(),
        },
    ) else {
        panic!("syntax");
    };
    assert_eq!(unindexed.authority, ReadAuthority::DiskObservation);
    assert_eq!(unindexed.rendered, UNINDEXED_SYNTAX_GOLDEN);
    assert!(!unindexed.valid);

    let refused = handle
        .query(
            &QueryRequest::Syntax {
                path: "config/app.json".into(),
            },
            QueryLimits::default(),
        )
        .unwrap_err();
    assert_eq!(refused.kind(), QueryRefusalKind::AdmissionUnavailable);
    let withheld = refused.withheld().expect("withheld findings");
    assert_eq!(withheld.path, "config/app.json");
    let lines: Vec<(u32, u32, &str)> = withheld
        .findings
        .iter()
        .map(|finding| {
            (
                finding.line_start,
                finding.line_end,
                finding.rule_id.as_str(),
            )
        })
        .collect();
    assert_eq!(lines, vec![(2, 2, "secret.context-assignment")]);
    handle.close().unwrap();
}

#[test]
fn zero_hit_text_search_sweeps_matching_untracked_files_like_mcp() {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/lib.rs"), "pub fn indexed() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());

    // A file written after capture stays unknown to the publication until the
    // background worker's next poll publishes it, so each attempt uses a new
    // file and is judged only when the search really was zero-hit.
    let mut swept = false;
    for attempt in 0..20 {
        let path = format!("src/late_{attempt}.rs");
        let needle = format!("late_needle_{attempt}");
        fs::write(
            root.path().join(&path),
            format!("fn late() {{ let _ = \"{needle}\"; }}\n"),
        )
        .unwrap();
        let QueryOutput::TextSearch(result) = query_current(
            &handle,
            &QueryRequest::TextSearch(TextSearchRequest {
                query: Some(needle),
                ..Default::default()
            }),
        ) else {
            panic!("text search");
        };
        if result.total_matches > 0 {
            assert!(result.untracked_paths.is_empty());
            assert_eq!(result.untracked_diagnostic, None);
            continue;
        }
        assert_eq!(result.untracked_paths, vec![path.clone()]);
        assert_eq!(result.untracked_diagnostic, Some(sweep_golden(&path)));
        swept = true;
        break;
    }
    assert!(swept, "no attempt observed a zero-hit search");
    handle.close().unwrap();
}
