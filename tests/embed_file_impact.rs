#![cfg(feature = "embed")]
//! MCP parity for `analyze_file_impact`: the embedded
//! `QueryRequest::FileImpact` re-admits the file from disk and renders the
//! shared engine's text for a modified file with co-changes, the estimate, a
//! missing new file and a new file. The server lib test
//! `analyze_file_impact_matches_embed_parity_golden` in `src/protocol/tools.rs`
//! asserts the same goldens against MCP output.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::embed::parity::file_impact::FileImpactRequest;
use symforge::embed::parity::{
    QueryLimits, QueryOutput, QueryPolicy, QueryRefusalKind, QueryRequest,
};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

/// Shared with the MCP golden. The edit keeps the byte length so that, with
/// its mtime restored, the worker's scout fingerprint does not move and only
/// the impact re-admission observes it.
const LIB_BEFORE: &str =
    "// round 3\npub fn alpha() -> u32 {\n    1\n}\n\npub fn beta() -> u32 {\n    2\n}\n";
const LIB_AFTER: &str =
    "// round 3\npub fn alpha() -> u32 {\n    7\n}\n\npub fn zeta() -> u32 {\n    2\n}\n";
const EDIT_GOLDEN: &str = "── Impact: src/lib.rs ──\nStatus: changed on disk since last index\n  [Added]   fn zeta\n  [Changed] fn alpha\n  [Removed] fn beta\n\nCallers to review:\n  Callers of alpha():\n    src/caller.rs  line 2";
const ESTIMATE_GOLDEN: &str =
    "Estimate for analyze_file_impact: ~265 tokens (include_co_changes=true)";
const MISSING_GOLDEN: &str = "File not found on disk: src/missing.rs";
const NEW_FILE_GOLDEN: &str = "Language: Rust\nSymbols: 1 fn, 1 struct\n[Indexed, 0 callers yet]";

/// Three commits in which `src/lib.rs` and `src/other.rs` change together.
fn fixture(root: &Path) {
    let repository = git2::Repository::init(root).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    for round in 1..=3 {
        let files = [
            (".gitignore", ".symforge/\n".to_string()),
            (
                "src/lib.rs",
                LIB_BEFORE.replace("round 3", &format!("round {round}")),
            ),
            (
                "src/other.rs",
                format!("pub fn other() -> u32 {{\n    {round}\n}}\n"),
            ),
            (
                "src/caller.rs",
                "pub fn call() -> u32 {\n    alpha()\n}\n".to_string(),
            ),
        ];
        let mut index = repository.index().unwrap();
        for (path, content) in &files {
            fs::write(root.join(path), content).unwrap();
            index.add_path(Path::new(path)).unwrap();
        }
        index.write().unwrap();
        let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
        let parent = repository
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "round",
                &tree,
                &parents,
            )
            .unwrap();
    }
}

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source never reached Current");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

fn impact(handle: &EmbeddedSourceHandle, request: FileImpactRequest) -> String {
    let request = QueryRequest::FileImpact(request);
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        match handle.query_with_policy(
            &request,
            QueryLimits::default(),
            QueryPolicy {
                allow_derived_state_preparation: true,
            },
            None,
            None,
        ) {
            Ok(claim) => match claim.value() {
                QueryOutput::FileImpact(report) => return report.rendered.clone(),
                other => panic!("unexpected output: {other:?}"),
            },
            Err(refusal)
                if matches!(
                    refusal.kind(),
                    QueryRefusalKind::SourceUnavailable | QueryRefusalKind::StalePublication
                ) =>
            {
                assert!(Instant::now() < until, "source never returned to Current");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(refusal) => panic!("unexpected refusal: {refusal:?}"),
        }
    }
}

#[test]
fn analyze_file_impact_renders_the_mcp_answer_for_every_option() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());

    let lib = root.path().join("src/lib.rs");
    let mtime = filetime::FileTime::from_last_modification_time(&fs::metadata(&lib).unwrap());
    fs::write(&lib, LIB_AFTER).unwrap();
    filetime::set_file_mtime(&lib, mtime).unwrap();
    let edited = impact(
        &handle,
        FileImpactRequest {
            path: "src/lib.rs".into(),
            include_co_changes: Some(true),
            co_changes_limit: Some(5),
            ..Default::default()
        },
    );
    let co_change_tail = format!(
        "Ownership:\n  Fixture: 3 commits (100%)\n\nCo-changing files (top 1):\n  {:<50} coupling: 1.000  (3 shared commits)",
        "src/other.rs"
    );
    assert!(
        edited.starts_with(&format!(
            "{EDIT_GOLDEN}\n\nGit temporal data for src/lib.rs\n\nChurn score: "
        )),
        "{edited}"
    );
    assert!(edited.contains(" (3 commits)\nLast commit: "), "{edited}");
    assert!(edited.ends_with(&co_change_tail), "{edited}");

    // The re-admission published the edit: a second call diffs against it.
    let again = impact(
        &handle,
        FileImpactRequest {
            path: "src/lib.rs".into(),
            ..Default::default()
        },
    );
    assert!(
        again.starts_with("── Impact: src/lib.rs ──\nStatus: indexed and unchanged\n"),
        "{again}"
    );

    let estimate = impact(
        &handle,
        FileImpactRequest {
            path: "src/lib.rs".into(),
            estimate: Some(true),
            include_co_changes: Some(true),
            co_changes_limit: Some(5),
            ..Default::default()
        },
    );
    assert_eq!(estimate, ESTIMATE_GOLDEN);
    let missing = impact(
        &handle,
        FileImpactRequest {
            path: "src/missing.rs".into(),
            new_file: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(missing, MISSING_GOLDEN);

    fs::write(
        root.path().join("src/fresh.rs"),
        "pub struct Fresh;\n\npub fn make() -> Fresh {\n    Fresh\n}\n",
    )
    .unwrap();
    let fresh = impact(
        &handle,
        FileImpactRequest {
            path: "src/fresh.rs".into(),
            new_file: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(fresh, NEW_FILE_GOLDEN);
}

#[test]
fn analyze_file_impact_reports_unavailable_co_changes_without_derived_state_permission() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let claim = handle
        .query(
            &QueryRequest::FileImpact(FileImpactRequest {
                path: "src/lib.rs".into(),
                include_co_changes: Some(true),
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::FileImpact(report) = claim.value() else {
        panic!("file impact");
    };
    assert!(
        report.rendered.ends_with(
            "\n\nGit temporal data unavailable: host policy does not permit derived-state preparation"
        ),
        "{}",
        report.rendered
    );
}

/// MCP's watcher re-indexes an edited file through `update_file`, which keeps
/// the prior image as the impact baseline. The embedded worker refreshes the
/// whole tree instead; that refresh must keep the same baseline, so an impact
/// request that arrives after the worker already reloaded the edit still
/// reports it rather than "indexed and unchanged".
#[test]
fn analyze_file_impact_keeps_the_pre_edit_baseline_across_a_worker_reload() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let before = handle.runtime_view().source_version;

    fs::write(root.path().join("src/lib.rs"), LIB_AFTER).unwrap();
    handle.request_refresh().unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let view = handle.runtime_view();
        if view.phase == SourceRuntimePhase::Current && view.source_version > before {
            break;
        }
        assert!(Instant::now() < until, "the worker never reloaded the edit");
        std::thread::sleep(Duration::from_millis(5));
    }
    let reloaded = handle
        .query(
            &QueryRequest::SearchSymbols {
                query: Some("zeta".into()),
                path_prefix: None,
                kind: None,
                include_tests: false,
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Symbols(symbols) = reloaded.value() else {
        panic!("symbol search");
    };
    assert_eq!(symbols.len(), 1, "the reload published the edit");

    let edited = impact(
        &handle,
        FileImpactRequest {
            path: "src/lib.rs".into(),
            ..Default::default()
        },
    );
    assert_eq!(edited, EDIT_GOLDEN);

    // Consumed like MCP's: the next request diffs against the edit itself.
    let again = impact(
        &handle,
        FileImpactRequest {
            path: "src/lib.rs".into(),
            ..Default::default()
        },
    );
    assert!(
        again.starts_with("── Impact: src/lib.rs ──\nStatus: indexed and unchanged\n"),
        "{again}"
    );
}
