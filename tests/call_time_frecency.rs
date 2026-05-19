//! Call-time capability-resolution coverage for `search_files(rank_by="frecency")`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::live_index::frecency::{self, FRECENCY_FLAG_ENV, FrecencyStore};
use symforge::paths::SYMFORGE_FRECENCY_DB_PATH;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

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
            "call_time_frecency_test".to_string(),
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

static FRECENCY_ENV_LOCK: StdMutex<()> = StdMutex::new(());

struct EnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
    previous: Option<OsString>,
}

#[allow(unsafe_code)] // test-only env guard serializes frecency flag mutation.
impl EnvGuard {
    fn unset() -> Self {
        let guard = FRECENCY_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os(FRECENCY_FLAG_ENV);
        // SAFETY: tests hold FRECENCY_ENV_LOCK and are run with test-threads=1
        // in this suite's verification command.
        unsafe { std::env::remove_var(FRECENCY_FLAG_ENV) };
        Self {
            _guard: guard,
            previous,
        }
    }

    fn set(value: &str) -> Self {
        let guard = FRECENCY_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os(FRECENCY_FLAG_ENV);
        // SAFETY: see EnvGuard::unset.
        unsafe { std::env::set_var(FRECENCY_FLAG_ENV, value) };
        Self {
            _guard: guard,
            previous,
        }
    }
}

#[allow(unsafe_code)] // test-only env guard restores serialized frecency flag mutation.
impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => {
                // SAFETY: see EnvGuard::unset.
                unsafe { std::env::set_var(FRECENCY_FLAG_ENV, value) };
            }
            None => {
                // SAFETY: see EnvGuard::unset.
                unsafe { std::env::remove_var(FRECENCY_FLAG_ENV) };
            }
        }
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn assert_contains(result: &str, needle: &str) {
    assert!(
        result.contains(needle),
        "expected result to contain `{needle}`; result was:\n{result}"
    );
}

fn assert_before(result: &str, first: &str, second: &str) {
    let first_pos = result
        .find(first)
        .unwrap_or_else(|| panic!("missing `{first}` in result:\n{result}"));
    let second_pos = result
        .find(second)
        .unwrap_or_else(|| panic!("missing `{second}` in result:\n{result}"));
    assert!(
        first_pos < second_pos,
        "expected `{first}` before `{second}`; result was:\n{result}"
    );
}

fn init_repo_with_root_commit(root: &Path) -> String {
    let repo = git2::Repository::init(root).expect("git init");
    let sig = git2::Signature::now("t", "t@x").expect("sig");
    let tree_id = {
        let mut idx = repo.index().expect("index");
        idx.write_tree().expect("write tree")
    };
    let tree = repo.find_tree(tree_id).expect("find tree");
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, "root", &tree, &[])
        .expect("commit");
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
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("c{i}"),
            &tree,
            &[&parent],
        )
        .expect("commit");
    }
}

#[tokio::test]
async fn rank_by_frecency_env_unset_without_history_reports_no_history_without_db() {
    let _env = EnvGuard::unset();
    let fx = Fixture::new(&[
        ("src/alpha.rs", "pub fn alpha() {}\n"),
        ("src/beta.rs", "pub fn beta() {}\n"),
    ]);

    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "alpha", "limit": 10, "rank_by": "frecency"}),
    )
    .await;

    assert_contains(&result, "src/alpha.rs");
    assert_contains(&result, "Capability: frecency ranking fallback used");
    assert_contains(&result, "no frecency history");
    assert_contains(&result, "path ranking returned");
    assert!(
        !fx.db_path().exists(),
        "rank_by=frecency without history must not create a frecency database"
    );
}

#[tokio::test]
async fn rank_by_frecency_env_unset_uses_existing_persistent_history() {
    let _env = EnvGuard::unset();
    let fx = Fixture::new(&[
        ("src/file_a_old.rs", "pub fn item_a() {}\n"),
        ("src/file_b_new.rs", "pub fn item_b() {}\n"),
    ]);
    fx.open_store()
        .bump(&[PathBuf::from("src/file_b_new.rs")], now_ts())
        .expect("seed persistent frecency row");

    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "src/file_", "limit": 10, "rank_by": "frecency"}),
    )
    .await;

    assert_before(&result, "src/file_b_new.rs", "src/file_a_old.rs");
    assert_contains(&result, "Capability: frecency ranking applied");
    assert_contains(&result, "1/2 returned candidates had frecency scores");
    assert_contains(&result, "persistent frecency history");
}

#[tokio::test]
async fn commitment_read_collects_session_history_with_env_unset() {
    let _env = EnvGuard::unset();
    let fx = Fixture::new(&[
        ("src/file_a_old.rs", "pub fn item_a() {}\n"),
        ("src/file_b_new.rs", "pub fn item_b() {}\n"),
    ]);

    let _ = call(
        &fx.server,
        "get_file_context",
        json!({"path": "src/file_b_new.rs", "sections": ["outline"]}),
    )
    .await;

    assert!(
        !fx.db_path().exists(),
        "env-unset session collection must not create the persistent frecency database"
    );

    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "src/file_", "limit": 10, "rank_by": "frecency"}),
    )
    .await;

    assert_before(&result, "src/file_b_new.rs", "src/file_a_old.rs");
    assert_contains(&result, "Capability: frecency ranking applied");
    assert_contains(&result, "1/2 returned candidates had frecency scores");
    assert_contains(&result, "session frecency history");
}

#[tokio::test]
async fn rank_by_frecency_policy_disabled_reports_fallback_even_with_history() {
    let _env = EnvGuard::set("0");
    let fx = Fixture::new(&[
        ("src/file_a_old.rs", "pub fn item_a() {}\n"),
        ("src/file_b_new.rs", "pub fn item_b() {}\n"),
    ]);
    fx.open_store()
        .bump(&[PathBuf::from("src/file_b_new.rs")], now_ts())
        .expect("seed persistent frecency row");

    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "src/file_", "limit": 10, "rank_by": "frecency"}),
    )
    .await;

    assert_before(&result, "src/file_a_old.rs", "src/file_b_new.rs");
    assert_contains(&result, "Capability: frecency ranking disabled by policy");
    assert_contains(&result, "path ranking returned");
}

#[tokio::test]
async fn discovery_without_requested_frecency_stays_footprint_free() {
    let _env = EnvGuard::unset();
    let fx = Fixture::new(&[("src/file_b_new.rs", "pub fn item_b() {}\n")]);

    let _ = call(
        &fx.server,
        "search_files",
        json!({"query": "file_b", "limit": 10}),
    )
    .await;
    let _ = call(
        &fx.server,
        "search_text",
        json!({"query": "item_b", "limit": 10}),
    )
    .await;
    let _ = call(
        &fx.server,
        "search_symbols",
        json!({"query": "item_b", "limit": 10}),
    )
    .await;

    assert!(
        !fx.db_path().exists(),
        "discovery tools must not create the persistent frecency database"
    );
    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "file_b", "limit": 10, "rank_by": "frecency"}),
    )
    .await;
    assert_contains(&result, "Capability: frecency ranking fallback used");
    assert_contains(&result, "no frecency history");
}

#[tokio::test]
async fn cached_writer_post_reset_visible_to_ranking() {
    let _env = EnvGuard::set("1");
    let fx = Fixture::new(&[
        ("src/file_a_old.rs", "pub fn item_a() {}\n"),
        ("src/file_b_new.rs", "pub fn item_b() {}\n"),
    ]);
    let first = init_repo_with_root_commit(&fx.root);

    {
        let store = fx.open_store();
        for _ in 0..10 {
            store
                .bump(&[PathBuf::from("src/file_a_old.rs")], 0)
                .expect("seed old frecency row");
        }
        store
            .reset_or_halve_on_head_change(None, &first, None)
            .expect("anchor first HEAD");
    }

    advance_head(&fx.root, 501);
    frecency::bump(&fx.root, &[PathBuf::from("src/file_b_new.rs")]);

    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "src/file_", "limit": 10, "rank_by": "frecency"}),
    )
    .await;

    assert_before(&result, "src/file_b_new.rs", "src/file_a_old.rs");
    assert_contains(&result, "Capability: frecency ranking applied");
    assert_contains(&result, "persistent (cached) frecency history");
}

#[tokio::test]
async fn concurrent_bump_and_rank_under_persistent_policy_completes() {
    let _env = EnvGuard::set("1");
    let fx = Fixture::new(&[
        ("src/file_a.rs", "pub fn item_a() {}\n"),
        ("src/file_b.rs", "pub fn item_b() {}\n"),
    ]);
    frecency::bump(&fx.root, &[PathBuf::from("src/file_a.rs")]);

    let bump_root = fx.root.clone();
    let bump_task = tokio::task::spawn_blocking(move || {
        for _ in 0..20 {
            frecency::bump(&bump_root, &[PathBuf::from("src/file_b.rs")]);
        }
    });
    let result = call(
        &fx.server,
        "search_files",
        json!({"query": "src/file_", "limit": 10, "rank_by": "frecency"}),
    )
    .await;
    bump_task.await.expect("bump task completed");

    assert!(
        !result.contains("unable to read frecency history"),
        "rank_by=frecency must not error during same-process bump traffic; result was:\n{result}"
    );
    assert_contains(&result, "Capability: frecency ranking applied");
    assert_contains(&result, "persistent (cached) frecency history");
}
