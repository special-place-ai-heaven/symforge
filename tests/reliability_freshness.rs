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
    // Written now, so the index sees it inside the racy window.
    std::fs::write(&short, format!("{original}// {first_word}\n")).expect("seed short.rs");
    let stamp = std::fs::metadata(&short)
        .expect("stat")
        .modified()
        .expect("mtime");

    let server = server_for(dir.path());
    let params = json!({ "path": "src/short.rs" });
    let before = call(&server, "get_file_content", params.clone()).await;
    assert!(
        before.contains(first_word) && !before.contains(replacement),
        "control: the indexed content holds the original word:\n{before}"
    );

    std::fs::write(&short, format!("{original}// {replacement}\n")).expect("same-size edit");
    let file = std::fs::File::options()
        .write(true)
        .open(&short)
        .expect("open for set_modified");
    file.set_modified(stamp)
        .expect("restore the modification time");
    drop(file);
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
    let server = server_from_shared(root, shared);
    for pass in ["first answer (hashes)", "second answer (stat only)"] {
        let started = std::time::Instant::now();
        let _ = call(
            &server,
            "search_text",
            json!({ "query": "no_such_text_anywhere_036" }),
        )
        .await;
        eprintln!("{pass}: {:?}", started.elapsed());
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
