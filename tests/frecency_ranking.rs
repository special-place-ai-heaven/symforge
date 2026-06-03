// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Acceptance matrix for frecency-weighted file ranking.
//!
//! Each test exercises one row from the spec's test matrix
//! (`[[SymForge Frecency-Weighted File Ranking]]` Implementation Notes
//! §"Test matrix") end-to-end against a real `FrecencyStore` on a tempdir.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::live_index::frecency::{FRECENCY_FLAG_ENV, FrecencyStore};
use symforge::live_index::persist::init_frecency_store;
use symforge::paths::SYMFORGE_FRECENCY_DB_PATH;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

// ─── Fixture ─────────────────────────────────────────────────────────────────

mod git_test_helpers {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/git/test_helpers.rs"
    ));
}

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    server: SymForgeServer,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        for (rel, content) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            fs::write(&path, content).expect("write fixture file");
        }
        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "frecency_ranking_test".to_string(),
            watcher_info,
            Some(root.clone()),
            None,
        );
        Self {
            _dir: dir,
            root,
            server,
        }
    }

    fn db_path(&self) -> PathBuf {
        self.root.join(SYMFORGE_FRECENCY_DB_PATH)
    }

    fn open_store(&self) -> FrecencyStore {
        FrecencyStore::open(&self.db_path()).expect("open frecency store")
    }
}

async fn call(server: &SymForgeServer, tool: &str, params: Value) -> String {
    server.dispatch_tool_for_tests(tool, params).await
}

// ─── Env mutation guard ──────────────────────────────────────────────────────
//
// Tests in this file mutate `SYMFORGE_FRECENCY`. The project runs with
// `--test-threads=1`, but this lock is belt-and-suspenders against a future
// parallel runner. `FlagGuard::on()` sets the flag and clears it on drop.

static FRECENCY_ENV_LOCK: StdMutex<()> = StdMutex::new(());

struct FlagGuard {
    _g: std::sync::MutexGuard<'static, ()>,
}

#[allow(unsafe_code)] // test-only flag guard serializes frecency env mutation.
impl FlagGuard {
    fn on() -> Self {
        let g = FRECENCY_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: --test-threads=1 + this lock serialize env mutation; no
        // concurrent reader can observe the transition.
        unsafe { std::env::set_var(FRECENCY_FLAG_ENV, "1") };
        Self { _g: g }
    }
}

#[allow(unsafe_code)] // test-only flag guard restores serialized frecency env mutation.
impl Drop for FlagGuard {
    fn drop(&mut self) {
        // SAFETY: see FlagGuard::on.
        unsafe { std::env::remove_var(FRECENCY_FLAG_ENV) };
    }
}

fn bump_paths(store: &FrecencyStore) -> Vec<PathBuf> {
    store
        .last_10_bumps()
        .expect("last_10_bumps")
        .into_iter()
        .map(|e| e.path)
        .collect()
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ─── Bump on edit tools (7 tests) ───────────────────────────────────────────

#[tokio::test]
async fn replace_symbol_body_bumps_frecency() {
    let fx = Fixture::new(&[("src/lib.rs", "fn hello() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "replace_symbol_body",
        json!({"path": "src/lib.rs", "name": "hello", "new_body": "fn hello() { 1 }"}),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/lib.rs")),
        "replace_symbol_body must bump touched path; got {bumps:?}"
    );
}

#[tokio::test]
async fn insert_symbol_bumps_frecency() {
    let fx = Fixture::new(&[("src/lib.rs", "fn hello() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "insert_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "position": "after",
            "content": "fn world() {}\n",
        }),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/lib.rs")),
        "insert_symbol must bump touched path; got {bumps:?}"
    );
}

#[tokio::test]
async fn delete_symbol_bumps_frecency() {
    let fx = Fixture::new(&[("src/lib.rs", "fn hello() {}\nfn goodbye() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "delete_symbol",
        json!({"path": "src/lib.rs", "name": "goodbye"}),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/lib.rs")),
        "delete_symbol must bump touched path; got {bumps:?}"
    );
}

#[tokio::test]
async fn edit_within_symbol_bumps_frecency() {
    let fx = Fixture::new(&[("src/lib.rs", "fn hello() {\n    let x = 1;\n}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "edit_within_symbol",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "old_text": "let x = 1;",
            "new_text": "let x = 2;",
        }),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/lib.rs")),
        "edit_within_symbol must bump touched path; got {bumps:?}"
    );
}

#[tokio::test]
async fn batch_edit_bumps_each_touched_file_once() {
    let fx = Fixture::new(&[
        ("src/a.rs", "fn alpha() {\n    a_old();\n}\n"),
        ("src/b.rs", "fn beta() {\n    b_old();\n}\n"),
    ]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "batch_edit",
        json!({
            "edits": [
                {"path": "src/a.rs", "name": "alpha", "operation": {"type": "edit_within", "old_text": "a_old()", "new_text": "a_new()"}},
                {"path": "src/b.rs", "name": "beta", "operation": {"type": "edit_within", "old_text": "b_old()", "new_text": "b_new()"}},
            ]
        }),
    )
    .await;

    let store = fx.open_store();
    let entries = store.last_10_bumps().expect("last_10_bumps");
    let a_hits = entries
        .iter()
        .find(|e| e.path == Path::new("src/a.rs"))
        .map(|e| e.hit_count);
    let b_hits = entries
        .iter()
        .find(|e| e.path == Path::new("src/b.rs"))
        .map(|e| e.hit_count);
    assert_eq!(a_hits, Some(1), "src/a.rs should bump exactly once");
    assert_eq!(b_hits, Some(1), "src/b.rs should bump exactly once");
}

#[tokio::test]
async fn batch_rename_bumps_definition_and_call_site() {
    let fx = Fixture::new(&[
        ("src/a.rs", "pub fn old_name() {}\n"),
        (
            "src/b.rs",
            "use crate::old_name;\nfn caller() { old_name(); }\n",
        ),
    ]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "batch_rename",
        json!({"path": "src/a.rs", "name": "old_name", "new_name": "new_name"}),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/a.rs")),
        "batch_rename must bump definition file; got {bumps:?}"
    );
}

#[tokio::test]
async fn batch_insert_bumps_each_target_once() {
    let fx = Fixture::new(&[
        ("src/a.rs", "fn alpha() {}\n"),
        ("src/b.rs", "fn beta() {}\n"),
    ]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "batch_insert",
        json!({
            "content": "fn injected() {}\n",
            "position": "after",
            "targets": [
                {"path": "src/a.rs", "name": "alpha"},
                {"path": "src/b.rs", "name": "beta"},
            ],
        }),
    )
    .await;

    let store = fx.open_store();
    let entries = store.last_10_bumps().expect("last_10_bumps");
    let a_hits = entries
        .iter()
        .find(|e| e.path == Path::new("src/a.rs"))
        .map(|e| e.hit_count);
    let b_hits = entries
        .iter()
        .find(|e| e.path == Path::new("src/b.rs"))
        .map(|e| e.hit_count);
    assert_eq!(a_hits, Some(1), "src/a.rs should bump once");
    assert_eq!(b_hits, Some(1), "src/b.rs should bump once");
}

// ─── Bump on read tools (4 tests) ───────────────────────────────────────────

#[tokio::test]
async fn get_file_context_bumps_frecency() {
    let fx = Fixture::new(&[("src/foo.rs", "pub fn foo() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "get_file_context",
        json!({"path": "src/foo.rs"}),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/foo.rs")),
        "get_file_context must bump accessed path; got {bumps:?}"
    );
}

#[tokio::test]
async fn get_file_content_bumps_frecency() {
    let fx = Fixture::new(&[("src/foo.rs", "pub fn foo() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "get_file_content",
        json!({"path": "src/foo.rs"}),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/foo.rs")),
        "get_file_content must bump accessed path; got {bumps:?}"
    );
}

#[tokio::test]
async fn get_symbol_bumps_frecency() {
    let fx = Fixture::new(&[("src/foo.rs", "pub fn thing() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "get_symbol",
        json!({"path": "src/foo.rs", "name": "thing"}),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/foo.rs")),
        "get_symbol must bump accessed path; got {bumps:?}"
    );
}

#[tokio::test]
async fn get_symbol_context_bumps_frecency() {
    let fx = Fixture::new(&[("src/foo.rs", "pub fn thing() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "get_symbol_context",
        json!({"path": "src/foo.rs", "name": "thing"}),
    )
    .await;

    let bumps = bump_paths(&fx.open_store());
    assert!(
        bumps.contains(&PathBuf::from("src/foo.rs")),
        "get_symbol_context must bump accessed path; got {bumps:?}"
    );
}

// ─── No-bump on discovery tools (3 tests) ───────────────────────────────────
//
// Spec §"Search tools deliberately do NOT bump" — positive-feedback-loop
// prevention. The single most important invariant of the whole feature.

#[tokio::test]
async fn search_files_does_not_bump() {
    let fx = Fixture::new(&[
        ("src/alpha.rs", "pub fn alpha() {}\n"),
        ("src/beta.rs", "pub fn beta() {}\n"),
    ]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "search_files",
        json!({"query": "alpha", "limit": 10}),
    )
    .await;

    // Discovery: no DB should have been created — bump short-circuits before
    // opening the store, so the file should not exist on disk.
    assert!(
        !fx.db_path().exists(),
        "search_files must not create a frecency database"
    );
}

#[tokio::test]
async fn search_files_frecency_rank_does_not_create_db_when_empty() {
    let fx = Fixture::new(&[
        ("src/alpha.rs", "pub fn alpha() {}\n"),
        ("src/beta.rs", "pub fn beta() {}\n"),
    ]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "search_files",
        json!({"query": "alpha", "limit": 10, "rank_by": "frecency"}),
    )
    .await;

    assert!(
        !fx.db_path().exists(),
        "search_files rank_by=frecency must not create a frecency database"
    );
}

#[tokio::test]
async fn search_text_does_not_bump() {
    let fx = Fixture::new(&[("src/lib.rs", "pub fn find_user() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "search_text",
        json!({"query": "find", "limit": 10}),
    )
    .await;

    assert!(
        !fx.db_path().exists(),
        "search_text must not create a frecency database"
    );
}

#[tokio::test]
async fn search_symbols_does_not_bump() {
    let fx = Fixture::new(&[("src/lib.rs", "pub fn find_user() {}\n")]);
    let _flag = FlagGuard::on();

    let _ = call(
        &fx.server,
        "search_symbols",
        json!({"query": "find", "limit": 10}),
    )
    .await;

    assert!(
        !fx.db_path().exists(),
        "search_symbols must not create a frecency database"
    );
}

// ─── Decay (1 test) ─────────────────────────────────────────────────────────

#[test]
fn score_decays_to_half_after_seven_days() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = FrecencyStore::open(&tmp.path().join("frecency.db")).expect("open");
    let p = PathBuf::from("src/lib.rs");
    let now: i64 = 1_700_000_000;
    let seven_days = 7 * 24 * 60 * 60;

    store
        .bump(std::slice::from_ref(&p), now - seven_days)
        .expect("bump");
    let score = store.score(&p, now).expect("score");

    let delta = (score - 0.5).abs();
    assert!(
        delta < 1e-9,
        "7-day-old single bump should decay to ~0.5; got {score}"
    );
}

// ─── Fresh file baseline (1 test) ───────────────────────────────────────────

#[test]
fn fresh_file_with_no_row_returns_baseline_score() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = FrecencyStore::open(&tmp.path().join("frecency.db")).expect("open");
    let score = store
        .score(Path::new("src/never-bumped.rs"), 1_700_000_000)
        .expect("score on missing path");
    assert_eq!(
        score, 0.0,
        "fresh file must score 0 (not error, not negative)"
    );
}

// ─── Fusion (1 test) ────────────────────────────────────────────────────────

#[tokio::test]
async fn recent_single_bump_outranks_old_ten_bumps() {
    let fx = Fixture::new(&[
        ("src/file_a_old.rs", "pub fn alpha() {}\n"),
        ("src/file_b_new.rs", "pub fn beta() {}\n"),
    ]);
    let _flag = FlagGuard::on();

    // Seed: file_A has 10 bumps from 6 months ago, file_B has 1 bump just now.
    {
        let store = fx.open_store();
        let now = now_ts();
        let six_months_ago = now - 180 * 24 * 60 * 60;
        let a = PathBuf::from("src/file_a_old.rs");
        let b = PathBuf::from("src/file_b_new.rs");
        for _ in 0..10 {
            store
                .bump(std::slice::from_ref(&a), six_months_ago)
                .expect("bump a");
        }
        store.bump(&[b], now).expect("bump b");
    }

    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "src/file_", "limit": 10, "rank_by": "frecency"}),
    )
    .await;

    let a_pos = result.find("file_a_old.rs");
    let b_pos = result.find("file_b_new.rs");
    assert!(
        a_pos.is_some() && b_pos.is_some(),
        "both files must appear: {result}"
    );
    assert!(
        b_pos < a_pos,
        "file_b (recent 1×) must rank above file_a (old 10×) under frecency fusion; result:\n{result}"
    );
}

// ─── HEAD-change reset (2 tests) ────────────────────────────────────────────
//
// End-to-end boot wiring: seed a real git repo, bump the frecency store,
// advance HEAD, then run `init_frecency_store` with `SYMFORGE_FRECENCY=1`
// and assert the policy landed.

fn init_repo_with_root_commit(root: &Path) -> String {
    let repo = git2::Repository::init(root).expect("git init");
    let sig = git2::Signature::now("t", "t@x").expect("sig");
    let tree_id = {
        let mut idx = repo.index().expect("index");
        idx.write_tree().expect("write tree")
    };
    let tree = repo.find_tree(tree_id).expect("find tree");
    let oid = git_test_helpers::commit_head_with_retry(&repo, &sig, &sig, "root", &tree, &[]);
    oid.to_string()
}

fn advance_head(root: &Path, count: usize) {
    let repo = git2::Repository::open(root).expect("open repo");
    let sig = git2::Signature::now("t", "t@x").expect("sig");
    let tree_id = {
        let mut idx = repo.index().expect("index");
        idx.write_tree().expect("write tree")
    };
    let tree = repo.find_tree(tree_id).expect("find tree");
    for i in 0..count {
        let parent_oid = repo.head().expect("head").target().expect("head target");
        let parent = repo.find_commit(parent_oid).expect("find parent");
        git_test_helpers::commit_head_with_retry(
            &repo,
            &sig,
            &sig,
            &format!("c{i}"),
            &tree,
            &[&parent],
        );
    }
}

#[test]
fn head_change_halves_scores_at_100_commits() {
    let _flag = FlagGuard::on();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let first = init_repo_with_root_commit(root);
    let db_path = root.join(SYMFORGE_FRECENCY_DB_PATH);

    // Seed: bump src/a.rs ten times, anchor HEAD.
    {
        let store = FrecencyStore::open(&db_path).expect("open");
        for _ in 0..10 {
            store.bump(&[PathBuf::from("src/a.rs")], 0).expect("bump");
        }
        store
            .reset_or_halve_on_head_change(None, &first, None)
            .expect("anchor");
    }

    advance_head(root, 100);
    init_frecency_store(root);

    let store = FrecencyStore::open(&db_path).expect("reopen");
    assert_eq!(
        store.score(Path::new("src/a.rs"), 0).expect("score"),
        5.0,
        "100 commits between stored and current HEAD must halve hit counts"
    );
}

#[test]
fn head_change_resets_scores_at_1000_commits() {
    let _flag = FlagGuard::on();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let first = init_repo_with_root_commit(root);
    let db_path = root.join(SYMFORGE_FRECENCY_DB_PATH);

    {
        let store = FrecencyStore::open(&db_path).expect("open");
        for _ in 0..10 {
            store.bump(&[PathBuf::from("src/a.rs")], 0).expect("bump");
        }
        store
            .reset_or_halve_on_head_change(None, &first, None)
            .expect("anchor");
    }

    advance_head(root, 1000);
    init_frecency_store(root);

    let store = FrecencyStore::open(&db_path).expect("reopen");
    assert_eq!(
        store.score(Path::new("src/a.rs"), 0).expect("score"),
        0.0,
        ">500 commits between stored and current HEAD must zero hit counts"
    );
}

// ─── Concurrency (1 test) ───────────────────────────────────────────────────

#[test]
fn ten_parallel_bumps_yield_hit_count_ten() {
    let _flag = FlagGuard::on();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let p = PathBuf::from("src/x.rs");

    thread::scope(|s| {
        for _ in 0..10 {
            let root = root.clone();
            let p = p.clone();
            s.spawn(move || {
                symforge::live_index::frecency::bump(&root, &[p]);
            });
        }
    });

    let store = FrecencyStore::open(&root.join(SYMFORGE_FRECENCY_DB_PATH)).expect("open");
    let entries = store.last_10_bumps().expect("last_10_bumps");
    let entry = entries
        .iter()
        .find(|e| e.path == p)
        .expect("src/x.rs row exists");
    assert_eq!(
        entry.hit_count, 10,
        "10 parallel bumps must land 10 increments (no lost updates)"
    );
}
