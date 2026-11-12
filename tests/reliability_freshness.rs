//! Feature 036 slice A3: trust statements are about this answer
//! (specs/036-tool-reliability, T024, User Story 4, FR-015 to FR-018, FR-023).
//!
//! Every test works on its own copy of the reliability fixture, because the
//! tests change files behind the index's back. Each negative assertion has an
//! accepting control in the same test.
#![cfg(feature = "server")]

mod reliability_support;

use std::path::Path;

use reliability_support::{call, fixture_copy, server_for, server_from_shared};
use serde_json::json;
use symforge::live_index::LiveIndex;

/// The per-file caveat of contracts/tool-outcome.md (Freshness).
const NOT_CONFIRMED: &str = "Not confirmed against disk";
/// The FR-017 line for empty or complete claims while observations are lost.
const LOST_OBSERVATIONS: &str =
    "Files created since the last scan may be missing; run index_folder to rescan.";
/// The index-wide caveats this slice removes from answers that do not use the
/// affected file.
const BLANKET_CAVEATS: [&str; 2] = ["UNVERIFIED against disk", "index may be stale"];

/// Holds `path` so that reading it fails until the guard is dropped:
/// an exclusive share mode on Windows, mode 000 elsewhere.
struct ReadBlock {
    #[cfg(windows)]
    _handle: std::fs::File,
    #[cfg(not(windows))]
    path: std::path::PathBuf,
}

impl ReadBlock {
    /// `None` when the platform cannot block the read (unix as root).
    fn hold(path: &Path) -> Option<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let handle = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .expect("open with exclusive share mode");
            Some(Self { _handle: handle })
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000))
                .expect("chmod 000");
            if std::fs::read(path).is_ok() {
                // Root reads through mode 000; the scenario cannot be built here.
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).ok();
                return None;
            }
            Some(Self {
                path: path.to_path_buf(),
            })
        }
    }
}

#[cfg(not(windows))]
impl Drop for ReadBlock {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o644)).ok();
    }
}

/// Every regular file beneath `dir`.
fn walk_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files.extend(walk_files(&path));
        } else {
            files.push(path);
        }
    }
    files
}

fn assert_no_freshness_caveat(text: &str) {
    for caveat in BLANKET_CAVEATS.iter().chain([&NOT_CONFIRMED]) {
        assert!(
            !text.contains(caveat),
            "answer must carry no freshness caveat ({caveat}):\n{text}"
        );
    }
}

/// Quickstart scenario 9: the file that gains a match was not in the previous
/// result, so only a check of the whole scope can see it.
#[tokio::test]
async fn outside_edit_that_adds_a_match_is_found_by_the_next_search() {
    let dir = fixture_copy();
    let server = server_for(dir.path());
    let query = json!({ "query": "zebra_marker_036" });

    let before = call(&server, "search_text", query.clone()).await;
    assert!(
        !before.contains("src/short.rs"),
        "control: the marker is absent before the edit:\n{before}"
    );

    let short = dir.path().join("src/short.rs");
    let mut text = std::fs::read_to_string(&short).expect("read short.rs");
    text.push_str("// zebra_marker_036\n");
    std::fs::write(&short, text).expect("edit short.rs outside the tool");

    let after = call(&server, "search_text", query).await;
    assert!(
        after.contains("src/short.rs") && after.contains("zebra_marker_036"),
        "the next search must see the outside edit:\n{after}"
    );
}

/// FR-016: an edit in the same clock second with the same size leaves size and
/// modification time equal; only the content tells it apart.
#[tokio::test]
async fn same_second_same_size_edit_is_detected_by_content() {
    let dir = fixture_copy();
    let short = dir.path().join("src/short.rs");
    let original = std::fs::read_to_string(&short).expect("read short.rs");
    let first_word = "alpha_036";
    let replacement = "omega_036";
    assert_eq!(first_word.len(), replacement.len());
    // Review 7b: fixed modification times, never the wall clock. Every other
    // file is a day older, so short.rs carries the newest time the filesystem
    // shows the index: its edit is inside the racy window by construction.
    let stamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let older = stamp - std::time::Duration::from_secs(86_400);
    let set_mtime = |path: &Path, time: std::time::SystemTime| {
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open for set_modified")
            .set_modified(time)
            .expect("set the modification time");
    };
    for entry in walk_files(dir.path()) {
        set_mtime(&entry, older);
    }
    std::fs::write(&short, format!("{original}// {first_word}\n")).expect("seed short.rs");
    set_mtime(&short, stamp);

    let server = server_for(dir.path());
    let params = json!({ "path": "src/short.rs" });
    let before = call(&server, "get_file_content", params.clone()).await;
    assert!(
        before.contains(first_word) && !before.contains(replacement),
        "control: the indexed content holds the original word:\n{before}"
    );

    std::fs::write(&short, format!("{original}// {replacement}\n")).expect("same-size edit");
    set_mtime(&short, stamp);
    let after_meta = std::fs::metadata(&short).expect("stat");
    assert_eq!(after_meta.modified().expect("mtime"), stamp);

    let after = call(&server, "get_file_content", params).await;
    assert!(
        after.contains(replacement) && !after.contains(first_word),
        "a same-second, same-size edit must be detected by content:\n{after}"
    );
}

/// FR-018 / quickstart scenario 8: file A's unreadable record does not put a
/// caveat on a query that only uses file B.
#[tokio::test]
async fn stale_unreadable_record_for_a_leaves_query_on_b_without_caveat() {
    let dir = fixture_copy();
    let a = dir.path().join("src/unrelated.rs");
    let Some(block) = ReadBlock::hold(&a) else {
        eprintln!("skipped: this platform reads through the block (root)");
        return;
    };
    let shared = LiveIndex::load(dir.path()).expect("load index");
    drop(block); // A is readable again; its record is now stale.
    let server = server_from_shared(dir.path(), shared);

    let health = call(&server, "health", json!({})).await;
    assert!(
        health.contains("src/unrelated.rs"),
        "control: the index holds an unreadable record for A:\n{health}"
    );

    let on_b = call(
        &server,
        "search_text",
        json!({ "query": "target_fn", "path_prefix": "src/user.rs" }),
    )
    .await;
    assert!(
        on_b.contains("src/user.rs"),
        "control: the query on B answers from B:\n{on_b}"
    );
    assert_no_freshness_caveat(&on_b);

    // Review L16: a repo-wide answer that never uses A carries no caveat either.
    let repo_wide = call(&server, "find_references", json!({ "name": "target_fn" })).await;
    assert!(
        repo_wide.contains("src/user.rs"),
        "control: the repo-wide answer has rows:\n{repo_wide}"
    );
    assert_no_freshness_caveat(&repo_wide);
    // Review 7c: a repo-wide reference answer whose rows never touch A.
    let elsewhere = call(&server, "find_references", json!({ "name": "run_parent" })).await;
    assert!(
        elsewhere.contains("src/lib.rs") && !elsewhere.contains("src/unrelated.rs"),
        "control: the repo-wide answer cites B-side files only:\n{elsewhere}"
    );
    assert_no_freshness_caveat(&elsewhere);
}

/// FR-017: the lost-observation line appears on an empty claim and not on a
/// partial (truncated) answer; without lost observations it does not appear.
#[tokio::test]
async fn empty_result_while_observations_are_lost_names_unseen_files() {
    let dir = fixture_copy();
    let empty = json!({ "query": "no_such_text_anywhere_036" });
    let partial = json!({ "query": "target_fn", "limit": 1 });

    let clean = server_for(dir.path());
    let clean_empty = call(&clean, "search_text", empty.clone()).await;
    assert!(
        !clean_empty.contains(LOST_OBSERVATIONS),
        "control: no lost observations, no line:\n{clean_empty}"
    );

    let shared = LiveIndex::load(dir.path()).expect("load index");
    shared.latch_observer_gap_for_tests();
    let lossy = server_from_shared(dir.path(), shared);

    let lossy_empty = call(&lossy, "search_text", empty).await;
    assert!(
        lossy_empty.contains(LOST_OBSERVATIONS),
        "an empty claim while observations are lost must say so:\n{lossy_empty}"
    );
    let lossy_partial = call(&lossy, "search_text", partial).await;
    assert!(
        lossy_partial.contains("target_fn"),
        "control: the partial answer has rows:\n{lossy_partial}"
    );
    assert!(
        !lossy_partial.contains(LOST_OBSERVATIONS),
        "a partial answer makes no empty or complete claim:\n{lossy_partial}"
    );
    assert_no_freshness_caveat(&lossy_partial);
    // Review M8: with observations lost, no banner may say "current index";
    // it states what this call compared.
    assert!(
        !lossy_partial.contains("current index"),
        "the banner must not claim the whole index is current:\n{lossy_partial}"
    );
    assert!(
        lossy_partial.contains("confirmed against disk"),
        "the banner names what this call confirmed:\n{lossy_partial}"
    );

    // Review M7: a truncated reference answer is not a complete claim.
    let truncated = call(
        &lossy,
        "find_references",
        json!({ "name": "target_fn", "max_tokens": 60 }),
    )
    .await;
    assert!(
        truncated.contains("budget-limited") || truncated.contains("truncated"),
        "control: the reference answer was cut:\n{truncated}"
    );
    assert!(
        !truncated.contains(LOST_OBSERVATIONS),
        "a cut answer claims no completeness:\n{truncated}"
    );
}

/// Review H2: an indexed directory replaced by a link to a folder outside the
/// repository is reported as not confirmed, never read and counted as current.
#[tokio::test]
async fn a_directory_replaced_by_a_link_to_outside_is_not_confirmed() {
    let dir = fixture_copy();
    let outside = tempfile::tempdir().expect("outside dir");
    std::fs::create_dir_all(dir.path().join("linked")).expect("linked dir");
    std::fs::write(dir.path().join("linked/x.rs"), "fn linked_036() {}\n").expect("x.rs");
    let server = server_for(dir.path());
    let query = json!({ "query": "linked_036" });
    let before = call(&server, "search_text", query.clone()).await;
    assert!(
        before.contains("linked/x.rs") && !before.contains(NOT_CONFIRMED),
        "control: the file is indexed and confirmed:\n{before}"
    );

    std::fs::remove_dir_all(dir.path().join("linked")).expect("remove linked");
    std::fs::write(
        outside.path().join("x.rs"),
        "fn linked_036() {}\n// outside_036\n",
    )
    .expect("outside x.rs");
    #[cfg(windows)]
    {
        let status = symforge::process_util::hidden_command("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(dir.path().join("linked"))
            .arg(outside.path())
            .output()
            .expect("mklink runs");
        assert!(status.status.success(), "mklink /J failed");
    }
    #[cfg(not(windows))]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).expect("symlink");

    let after = call(&server, "search_text", query).await;
    assert!(
        after.contains(&format!("{NOT_CONFIRMED}: linked/x.rs")),
        "a path that resolves outside the repository is not confirmed:\n{after}"
    );
    assert!(
        !after.contains("outside_036") && !after.starts_with("Trust:"),
        "outside bytes are never served or counted as confirmed:\n{after}"
    );
}

/// Review M14: a targeted read of a file the check could not confirm says so.
#[tokio::test]
async fn targeted_read_of_an_unconfirmed_file_names_the_cause() {
    let dir = fixture_copy();
    let server = server_for(dir.path());
    let params = json!({ "path": "src/short.rs" });
    let readable = call(&server, "get_file_content", params.clone()).await;
    assert!(
        !readable.contains(NOT_CONFIRMED),
        "control: a readable file carries no caveat:\n{readable}"
    );
    // A fresh server has no content stamp, so the next read hashes the file.
    let server = server_for(dir.path());
    let Some(block) = ReadBlock::hold(&dir.path().join("src/short.rs")) else {
        eprintln!("skipped: this platform reads through the block (root)");
        return;
    };
    let blocked = call(&server, "get_file_content", params).await;
    drop(block);
    assert!(
        blocked.contains(&format!("{NOT_CONFIRMED}: src/short.rs (")),
        "the unconfirmed file is named with its cause:\n{blocked}"
    );
}

/// Review M7: the caveat of a resolved path survives the token budget.
#[tokio::test]
async fn caveat_survives_the_token_budget() {
    let dir = fixture_copy();
    let server = server_for(dir.path());
    let Some(block) = ReadBlock::hold(&dir.path().join("src/long.rs")) else {
        eprintln!("skipped: this platform reads through the block (root)");
        return;
    };
    let answer = call(
        &server,
        "search_files",
        json!({ "query": "src/long.rs", "resolve": true, "max_tokens": 20 }),
    )
    .await;
    drop(block);
    assert!(
        answer.contains(&format!("{NOT_CONFIRMED}: src/long.rs")),
        "the caveat must survive truncation:\n{answer}"
    );
}

/// The measurement behind `ANSWER_CHECK_FILE_BOUND`: stat every indexed file
/// of this repository, then time the first answer (which hashes each file
/// once) and the second (which trusts the stat). Run with `--ignored
/// --nocapture`.
#[tokio::test]
#[ignore = "measurement; prints timings"]
async fn answer_check_cost_on_this_repository() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let shared = LiveIndex::load(root).expect("load this repository");
    let paths: Vec<String> = shared
        .published_generation()
        .live
        .all_files()
        .map(|(path, _)| path.clone())
        .collect();
    for pass in ["cold", "warm"] {
        let started = std::time::Instant::now();
        for path in &paths {
            let _ = std::fs::symlink_metadata(root.join(path));
        }
        let elapsed = started.elapsed();
        eprintln!(
            "stat {pass}: {} files in {elapsed:?} ({:?}/file)",
            paths.len(),
            elapsed / u32::try_from(paths.len().max(1)).unwrap_or(u32::MAX)
        );
    }
    // The query alone: a server bound to no root compares nothing.
    let unchecked = symforge::protocol::SymForgeServer::new(
        shared.clone(),
        "reliability".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        None,
        None,
    );
    let server = server_from_shared(root, shared);
    // Item 1: a partial answer with rows checks its rows only; an empty
    // answer checks its whole scope.
    let rows = json!({ "query": "answer_checked", "limit": 2 });
    let empty = json!({ "query": "no_such_text_anywhere_036" });
    let refs = json!({ "name": "answer_checked" });
    for (tool, label, params) in [
        ("search_text", "rows (limit 2)", rows),
        ("search_text", "zero hits", empty),
        ("find_references", "find_references", refs),
    ] {
        let started = std::time::Instant::now();
        let _ = call(&unchecked, tool, params.clone()).await;
        eprintln!("{label} query alone: {:?}", started.elapsed());
        for pass in ["first", "second"] {
            let started = std::time::Instant::now();
            let _ = call(&server, tool, params.clone()).await;
            eprintln!("{label} {pass} answer: {:?}", started.elapsed());
        }
    }
}

/// FR-023: health names every unreadable file with its classified cause.
#[tokio::test]
async fn health_names_each_unreadable_file_with_its_cause() {
    let dir = fixture_copy();
    let first = dir.path().join("src/unrelated.rs");
    let second = dir.path().join("src/long.rs");
    let (Some(first_block), Some(second_block)) =
        (ReadBlock::hold(&first), ReadBlock::hold(&second))
    else {
        eprintln!("skipped: this platform reads through the block (root)");
        return;
    };
    let shared = LiveIndex::load(dir.path()).expect("load index");
    let server = server_from_shared(dir.path(), shared);
    let health = call(&server, "health", json!({})).await;
    drop((first_block, second_block));

    #[cfg(windows)]
    let cause = "in use by another process (os error 32)";
    #[cfg(not(windows))]
    let cause = "access denied (os error 13)";
    for path in ["src/unrelated.rs", "src/long.rs"] {
        assert!(
            health.contains(&format!("{path} ({cause}")),
            "health must name {path} with its cause `{cause}`:\n{health}"
        );
    }
    assert!(
        !health.contains("src/short.rs ("),
        "control: a readable file is not listed as unreadable:\n{health}"
    );
}

/// Item 1a/1c: an answer with rows and no completeness claim compares only
/// the files it cites, and carries no bound line.
#[tokio::test]
async fn a_partial_answer_confirms_only_the_files_it_cites() {
    let dir = fixture_copy();
    let server = server_for(dir.path());
    let partial = call(
        &server,
        "search_text",
        json!({ "query": "target_fn", "limit": 1 }),
    )
    .await;
    assert!(
        partial.contains("truncated by result cap"),
        "control: the answer is cut by its limit:\n{partial}"
    );
    assert!(
        partial.contains("| 1 file confirmed against disk |")
            || partial.contains("Source authority: 1 file confirmed against disk"),
        "a one-row answer confirms its one file, not the whole scope:\n{partial}"
    );
    assert!(
        !partial.contains("check bound"),
        "a partial answer carries no bound line:\n{partial}"
    );
    // Control: a complete answer still compares its whole scope.
    let complete = call(&server, "search_text", json!({ "query": "target_fn" })).await;
    assert!(
        !complete.contains("| 1 file confirmed against disk |"),
        "control: a complete answer compares more than its rows:\n{complete}"
    );
}

/// Item 2: many files changed behind the index are re-indexed in ONE
/// publication, and the answer then reflects all of them.
#[tokio::test]
async fn many_changed_files_are_republished_once() {
    let dir = fixture_copy();
    let shared = LiveIndex::load(dir.path()).expect("load index");
    let handle = shared.clone();
    let server = server_from_shared(dir.path(), shared);
    let changed = [
        "src/lib.rs",
        "src/long.rs",
        "src/short.rs",
        "src/user.rs",
        "src/unrelated.rs",
        "src/other/leaf.rs",
    ];
    for path in changed {
        let file = dir.path().join(path);
        let mut text = std::fs::read_to_string(&file).expect("read");
        text.push_str("// batch_marker_036\n");
        std::fs::write(&file, text).expect("edit outside the tool");
    }
    let before = handle.publication_fence().publication_generation;
    let answer = call(
        &server,
        "search_text",
        json!({ "query": "batch_marker_036" }),
    )
    .await;
    let after = handle.publication_fence().publication_generation;
    for path in changed {
        assert!(answer.contains(path), "{path} must be found:\n{answer}");
    }
    assert_eq!(
        after - before,
        1,
        "six changed files must be republished in one publication:\n{answer}"
    );
}

/// Items 3 and 5: every tool that serves rows from a file it cites names that
/// file when it cannot be compared with the disk.
#[tokio::test]
async fn row_tools_name_a_cited_file_they_cannot_confirm() {
    let cases: [(&str, serde_json::Value, &str); 6] = [
        (
            "get_file_content",
            json!({ "path": "src/short.rs" }),
            "src/short.rs",
        ),
        (
            "get_file_context",
            json!({ "path": "src/short.rs" }),
            "src/short.rs",
        ),
        (
            "get_symbol",
            json!({ "path": "src/short.rs", "name": "SHORT_A" }),
            "src/short.rs",
        ),
        (
            "inspect_match",
            json!({ "path": "src/short.rs", "line": 2 }),
            "src/short.rs",
        ),
        ("explore", json!({ "query": "SHORT_A" }), "src/short.rs"),
        (
            "find_dependents",
            json!({ "path": "src/parent/leaf.rs" }),
            "src/user.rs",
        ),
    ];
    let mut missing = Vec::new();
    for (tool, params, cited) in cases {
        let dir = fixture_copy();
        // A fresh index has no content stamp, so the check must read the file.
        let server = server_for(dir.path());
        let readable = call(&server, tool, params.clone()).await;
        assert!(
            !readable.contains(NOT_CONFIRMED) && !readable.contains("unknown tool"),
            "control: {tool} confirms a readable {cited}:\n{readable}"
        );
        let server = server_for(dir.path());
        let Some(block) = ReadBlock::hold(&dir.path().join(cited)) else {
            eprintln!("skipped: this platform reads through the block (root)");
            return;
        };
        let blocked = call(&server, tool, params).await;
        drop(block);
        if !blocked.contains(&format!("{NOT_CONFIRMED}: {cited} (")) {
            missing.push(format!("{tool}:\n{blocked}"));
        }
    }
    assert!(
        missing.is_empty(),
        "each tool must name the cited file it could not confirm:\n{}",
        missing.join("\n----\n")
    );
}

/// Item 3: an outside edit to a cited file is seen by the next answer of a
/// tool that had no disk check.
#[tokio::test]
async fn inspect_match_sees_an_outside_edit_of_its_file() {
    let dir = fixture_copy();
    let server = server_for(dir.path());
    let params = json!({ "path": "src/short.rs", "line": 2 });
    let before = call(&server, "inspect_match", params.clone()).await;
    assert!(
        before.contains("SHORT_A"),
        "control: line 2 holds SHORT_A:\n{before}"
    );
    let short = dir.path().join("src/short.rs");
    let text = std::fs::read_to_string(&short).expect("read");
    std::fs::write(&short, text.replace("SHORT_A", "SHORT_Z_036")).expect("edit");
    let after = call(&server, "inspect_match", params).await;
    assert!(
        after.contains("SHORT_Z_036") && !after.contains("SHORT_A"),
        "the next answer must reflect the outside edit:\n{after}"
    );
}

/// Item 6: the same request down both lanes prints the same envelope and
/// caveat lines, in each of three states: every cited file confirmed, one
/// cited file that cannot be compared, and no comparison run (a git mode).
#[cfg(feature = "embed")]
mod lane_parity {
    use super::*;
    use reliability_support::{call_with_meta, committed_fixture_copy};
    use std::time::{Duration, Instant};
    use symforge::embed::parity::stel::StelRequest;
    use symforge::embed::parity::{QueryLimits, QueryOutput, QueryPolicy, QueryRequest};
    use symforge::embed::{
        EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
    };

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

    fn embed_facade(
        handle: &EmbeddedSourceHandle,
        session: &symforge::embed::parity::session::QuerySession,
        query: &str,
    ) -> String {
        let request: StelRequest = serde_json::from_value(json!({ "query": query })).unwrap();
        let claim = handle
            .query_with_policy(
                &QueryRequest::Symforge(request),
                QueryLimits::default(),
                QueryPolicy {
                    allow_derived_state_preparation: true,
                },
                Some(session),
                None,
            )
            .unwrap();
        match claim.value() {
            QueryOutput::Symforge(answer) => answer.rendered.clone(),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    /// `SYMFORGE_SURFACE=compact` (the MCP facade's surface) until dropped,
    /// the pattern of `tests/frecency_ranking.rs`'s `SurfaceGuard`: the suite
    /// runs with `--test-threads=1` (nextest: one process per test).
    struct CompactSurface(Option<std::ffi::OsString>);

    #[allow(unsafe_code)] // test-only env mutation, single-threaded (see above).
    impl CompactSurface {
        fn set() -> Self {
            let previous = std::env::var_os("SYMFORGE_SURFACE");
            // SAFETY: no other test thread mutates the environment meanwhile.
            unsafe { std::env::set_var("SYMFORGE_SURFACE", "compact") };
            Self(previous)
        }
    }

    #[allow(unsafe_code)] // test-only env restore, single-threaded (see above).
    impl Drop for CompactSurface {
        fn drop(&mut self) {
            match &self.0 {
                // SAFETY: see `CompactSurface::set`.
                Some(previous) => unsafe { std::env::set_var("SYMFORGE_SURFACE", previous) },
                None => unsafe { std::env::remove_var("SYMFORGE_SURFACE") },
            }
        }
    }

    /// The envelope and caveat lines an LLM reads about freshness: the
    /// collapsed banner, the source-authority line and the caveats. (The
    /// other envelope lines describe the query, not the disk check.)
    fn trust_lines(text: &str) -> Vec<String> {
        const PREFIXES: [&str; 4] = [
            "Trust:",
            "Source authority:",
            NOT_CONFIRMED,
            "Files created since the last scan",
        ];
        text.lines()
            .filter(|line| PREFIXES.iter().any(|prefix| line.starts_with(prefix)))
            .map(str::to_owned)
            .collect()
    }

    /// Each lane answers over its own copy (one project authority per root),
    /// both copies made and edited the same way.
    async fn both_lanes(
        copy: fn() -> tempfile::TempDir,
        edit: fn(&Path),
        query: &str,
        block: Option<&str>,
    ) -> (String, String) {
        let (mcp_dir, embed_dir) = (copy(), copy());
        edit(mcp_dir.path());
        edit(embed_dir.path());
        let runtime = ProcessIndexRuntime::acquire().unwrap();
        let handle = open(&runtime, embed_dir.path());
        let session = handle.new_query_session().unwrap();
        let server = server_for(mcp_dir.path());
        // Both lanes hold a fresh index with no content stamp; the block, if
        // any, lands after both loaded, so each must read the file to check it.
        let held = block.map(|path| {
            (
                ReadBlock::hold(&mcp_dir.path().join(path)),
                ReadBlock::hold(&embed_dir.path().join(path)),
            )
        });
        if matches!(held, Some((None, _) | (_, None))) {
            return (String::new(), String::new());
        }
        let embed = embed_facade(&handle, &session, query);
        let mcp = {
            let _surface = CompactSurface::set();
            call_with_meta(&server, "symforge", json!({ "query": query }))
                .await
                .text
        };
        drop(held);
        (mcp, embed)
    }

    #[tokio::test]
    async fn both_lanes_print_the_same_freshness_lines() {
        fn unchanged(_: &Path) {}
        fn uncommitted_edit(dir: &Path) {
            std::fs::write(dir.join("src/short.rs"), "pub const SHORT_A: u32 = 9;\n").unwrap();
        }
        type Case = (
            &'static str,
            fn() -> tempfile::TempDir,
            fn(&Path),
            &'static str,
            Option<&'static str>,
        );
        let cases: [Case; 4] = [
            ("confirmed", fixture_copy, unchanged, "grep target_fn", None),
            (
                "unconfirmable",
                fixture_copy,
                unchanged,
                "grep target_fn",
                Some("src/user.rs"),
            ),
            // Round 2 item 2: a targeted read inside the facade.
            (
                "targeted unconfirmable",
                fixture_copy,
                unchanged,
                "outline src/long.rs",
                Some("src/long.rs"),
            ),
            (
                "not run",
                committed_fixture_copy,
                uncommitted_edit,
                "changes",
                None,
            ),
        ];
        for (state, copy, edit, query, block) in cases {
            let (mcp, embed) = both_lanes(copy, edit, query, block).await;
            if mcp.is_empty() && embed.is_empty() {
                eprintln!("skipped {state}: this platform reads through the block (root)");
                continue;
            }
            let (mcp_lines, embed_lines) = (trust_lines(&mcp), trust_lines(&embed));
            assert!(
                !mcp_lines.is_empty(),
                "control ({state}): the answer has an envelope:\n{mcp}"
            );
            assert_eq!(
                mcp_lines, embed_lines,
                "{state}: the lanes differ\nMCP:\n{mcp}\nembed:\n{embed}"
            );
            if let Some(blocked) = block {
                assert!(
                    mcp_lines
                        .iter()
                        .any(|line| line.starts_with(&format!("{NOT_CONFIRMED}: {blocked} ("))),
                    "control ({state}): the blocked file is named:\n{mcp}"
                );
            }
        }
    }

    /// Review round 3 item 1: the native embed text and symbol searches carry
    /// the caveat lines MCP's `search_text` / `search_symbols` print. (No
    /// "not run" state: an embedded source always has a bound root.)
    #[tokio::test]
    async fn native_searches_carry_the_same_caveats() {
        let cases: [(&str, &str, Option<&str>); 4] = [
            ("search_text", "confirmed", None),
            ("search_text", "unconfirmable", Some("src/user.rs")),
            ("search_symbols", "confirmed", None),
            ("search_symbols", "unconfirmable", Some("src/user.rs")),
        ];
        for (tool, state, block) in cases {
            let (mcp_dir, embed_dir) = (fixture_copy(), fixture_copy());
            let runtime = ProcessIndexRuntime::acquire().unwrap();
            let handle = open(&runtime, embed_dir.path());
            // The embed lane answers before the MCP server exists (see the
            // round-2 report: a later embed source refuses otherwise); each
            // block lands after its lane loaded.
            let embed_held = block.map(|path| ReadBlock::hold(&embed_dir.path().join(path)));
            if matches!(embed_held, Some(None)) {
                eprintln!("skipped: this platform reads through the block (root)");
                return;
            }
            let args = json!({ "query": "target_fn" });
            let request = match tool {
                "search_text" => {
                    QueryRequest::TextSearch(serde_json::from_value(args.clone()).unwrap())
                }
                _ => QueryRequest::SymbolSearch(serde_json::from_value(args.clone()).unwrap()),
            };
            let claim = handle
                .query_with_policy(
                    &request,
                    QueryLimits::default(),
                    QueryPolicy::default(),
                    None,
                    None,
                )
                .unwrap();
            let embed = match claim.value() {
                QueryOutput::TextSearch(found) => found.freshness_notes.clone(),
                QueryOutput::SymbolSearch(found) => found.freshness_notes.clone(),
                other => panic!("unexpected output: {other:?}"),
            }
            .unwrap_or_default();
            drop(embed_held);
            let server = server_for(mcp_dir.path());
            let mcp_held = block.map(|path| ReadBlock::hold(&mcp_dir.path().join(path)));
            let mcp = call(&server, tool, args).await;
            drop(mcp_held);
            let caveats = |text: &str| -> Vec<String> {
                text.lines()
                    .filter(|line| {
                        line.starts_with(NOT_CONFIRMED) || line.starts_with("Files created")
                    })
                    .map(str::to_owned)
                    .collect()
            };
            assert_eq!(
                caveats(&mcp),
                caveats(&embed),
                "{tool} {state}: the lanes differ\nMCP:\n{mcp}\nembed:\n{embed}"
            );
            if let Some(blocked) = block {
                assert!(
                    mcp.contains(&format!("{NOT_CONFIRMED}: {blocked} (")),
                    "control ({tool} {state}): MCP names the blocked file:\n{mcp}"
                );
            }
        }
    }
}

/// A tree whose cited files have a space and non-ASCII letters in their paths.
fn spaced_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (path, text) in [
        (
            SPACED,
            "pub fn spaced_036() -> u32 {\n    1 // spaced_line_036 before\n}\n// spaced_line_036 tail\n",
        ),
        (
            UNICODE,
            "pub fn uses_036() -> u32 {\n    spaced_036() // unicode_line_036 before\n}\n// unicode_line_036 tail\n",
        ),
    ] {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    dir
}

const SPACED: &str = "docs/my notes/with space.rs";
const UNICODE: &str = "src/naïve/ünïcode.rs";

/// Replace `before` with `after_036` in `path` behind the index's back.
fn edit_behind(dir: &Path, path: &str) {
    let file = dir.join(path);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("before", "after_036")).unwrap();
}

/// Item 1 (round 2): a partial answer cites files whose paths hold a space or
/// non-ASCII letters, so it checks them and the next answer sees an edit.
#[tokio::test]
async fn cited_paths_with_spaces_and_unicode_are_checked_on_mcp() {
    let dir = spaced_tree();
    let server = server_for(dir.path());
    let cases = [
        (SPACED, json!({ "query": "spaced_line_036", "limit": 1 })),
        (
            UNICODE,
            json!({ "query": "unicode_line_036", "limit": 1, "path_prefix": "src/naïve" }),
        ),
    ];
    let mut stale = Vec::new();
    for (path, query) in cases {
        let before = call(&server, "search_text", query.clone()).await;
        assert!(
            before.contains("before") && before.contains("truncated by result cap"),
            "control: a partial answer citing {path}:\n{before}"
        );
        edit_behind(dir.path(), path);
        let after = call(&server, "search_text", query).await;
        if !after.contains("after_036") {
            stale.push(format!("{path}:\n{after}"));
        }
    }
    // The other two tools named by the brief cite the same files.
    edit_behind(dir.path(), SPACED);
    let refs = call(&server, "find_references", json!({ "name": "spaced_036" })).await;
    assert!(
        refs.contains(UNICODE),
        "control: the call site is cited:\n{refs}"
    );
    let context = call(
        &server,
        "get_symbol_context",
        json!({ "name": "spaced_036", "path": SPACED }),
    )
    .await;
    assert!(
        context.contains("spaced_036") && !context.contains(NOT_CONFIRMED),
        "the symbol context answers from the confirmed file:\n{context}"
    );
    assert!(
        stale.is_empty(),
        "stale answers:\n{}",
        stale.join("\n----\n")
    );
}

/// The same scenario on the embed lane's native text search.
#[cfg(feature = "embed")]
#[test]
fn cited_paths_with_spaces_and_unicode_are_checked_on_embed() {
    use symforge::embed::parity::{QueryLimits, QueryPolicy, QueryRequest};
    use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};
    let dir = spaced_tree();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            dir.path().to_path_buf(),
        ))
        .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(
            std::time::Instant::now() < until,
            "source never reached Current"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let search = |query: serde_json::Value| -> String {
        let request = QueryRequest::TextSearch(serde_json::from_value(query).unwrap());
        let claim = handle
            .query_with_policy(
                &request,
                QueryLimits::default(),
                QueryPolicy::default(),
                None,
                None,
            )
            .unwrap();
        serde_json::to_string(claim.value()).unwrap()
    };
    let mut stale = Vec::new();
    for (path, query) in [
        (SPACED, json!({ "query": "spaced_line_036", "limit": 1 })),
        (
            UNICODE,
            json!({ "query": "unicode_line_036", "limit": 1, "path_prefix": "src/naïve" }),
        ),
    ] {
        let before = search(query.clone());
        assert!(before.contains("before"), "control ({path}): {before}");
        edit_behind(dir.path(), path);
        let after = search(query);
        if !after.contains("after_036") {
            stale.push(format!("{path}: {after}"));
        }
    }
    assert!(
        stale.is_empty(),
        "stale answers:\n{}",
        stale.join("\n----\n")
    );
}
