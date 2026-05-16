//! Call-time ranking diagnostics coverage for `search_files(debug_ranking=true)`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::live_index::coupling::{AnchorKey, CouplingRow, CouplingStore};
use symforge::live_index::frecency::FrecencyStore;
use symforge::paths::{SYMFORGE_COUPLING_DB_PATH, SYMFORGE_FRECENCY_DB_PATH};
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn remove(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: project verification runs env-mutating tests with --test-threads=1.
        unsafe {
            std::env::remove_var(key);
        }
        Self { key, previous }
    }

    fn set(key: &'static str, value: &'static str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: project verification runs env-mutating tests with --test-threads=1.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => {
                // SAFETY: see EnvGuard::remove.
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            }
            None => {
                // SAFETY: see EnvGuard::remove.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
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
        write_files(&root, files);
        Self::from_dir(dir)
    }

    fn new_git(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        write_files(&root, files);
        init_git_repo(&root);
        Self::from_dir(dir)
    }

    fn from_dir(dir: TempDir) -> Self {
        let root = dir.path().to_path_buf();
        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "search_files_ranking_debug_test".to_string(),
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

    fn frecency_store(&self) -> FrecencyStore {
        FrecencyStore::open(&self.root.join(SYMFORGE_FRECENCY_DB_PATH))
            .expect("open frecency store")
    }
}

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write fixture file");
    }
}

fn init_git_repo(root: &Path) -> String {
    let repo = git2::Repository::init(root).expect("git init");
    let sig = git2::Signature::now("SymForge Tests", "symforge-tests@example.com")
        .expect("git signature");
    let tree_id = {
        let mut index = repo.index().expect("git index");
        index
            .add_all(["src"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("git add");
        index.write_tree().expect("git write tree")
    };
    let tree = repo.find_tree(tree_id).expect("git tree");
    let commit = repo
        .commit(Some("HEAD"), &sig, &sig, "root", &tree, &[])
        .expect("git commit");
    let head = commit.to_string();
    drop(tree);
    drop(repo);
    head
}

fn seed_ready_coupling(root: &Path, head: &str) {
    let store =
        CouplingStore::open(&root.join(SYMFORGE_COUPLING_DB_PATH)).expect("open coupling store");
    store.set_last_head(head).expect("set last head");
    store
        .set_cold_built_at(1_700_000_000)
        .expect("set cold built timestamp");
    store
        .bulk_upsert(&[CouplingRow {
            anchor: AnchorKey::file("src/auth/routes.rs"),
            partner: AnchorKey::file("src/server/routes.rs"),
            shared_commits: 3,
            weighted_score: 11.0,
            last_commit_ts: 1_700_000_000,
        }])
        .expect("seed coupling row");
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

async fn call(server: &SymForgeServer, params: Value) -> String {
    server.dispatch_tool_for_tests("search_files", params).await
}

fn assert_contains(result: &str, needle: &str) {
    assert!(
        result.contains(needle),
        "expected result to contain `{needle}`; result was:\n{result}"
    );
}

fn assert_not_contains(result: &str, needle: &str) {
    assert!(
        !result.contains(needle),
        "expected result to omit `{needle}`; result was:\n{result}"
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

#[tokio::test]
async fn default_search_files_output_omits_ranking_explanation() {
    let _debug = EnvGuard::remove("SYMFORGE_DEBUG_RANKING");
    let fx = Fixture::new(&[
        ("src/auth/routes.rs", "pub fn auth_routes() {}\n"),
        ("src/server/routes.rs", "pub fn server_routes() {}\n"),
    ]);

    let result = call(&fx.server, json!({"query": "routes.rs", "limit": 10})).await;

    assert_contains(&result, "src/auth/routes.rs");
    assert_contains(&result, "src/server/routes.rs");
    assert_not_contains(&result, "Ranking explanation");
    assert_not_contains(&result, "requested rank mode:");
    assert_not_contains(&result, "frecency signal:");
    assert_not_contains(&result, "co-change signal:");
}

#[tokio::test]
async fn debug_ranking_true_explains_default_path_ranking_with_env_unset() {
    let _debug = EnvGuard::remove("SYMFORGE_DEBUG_RANKING");
    let fx = Fixture::new(&[
        ("src/auth/routes.rs", "pub fn auth_routes() {}\n"),
        ("src/server/routes.rs", "pub fn server_routes() {}\n"),
    ]);

    let result = call(
        &fx.server,
        json!({"query": "routes.rs", "limit": 10, "debug_ranking": true}),
    )
    .await;

    assert_contains(&result, "Ranking explanation");
    assert_contains(&result, "requested rank mode: default path");
    assert_contains(&result, "path signal: applied");
    assert_contains(&result, "frecency signal: not requested");
    assert_contains(&result, "co-change signal: not requested");
    assert_contains(&result, "final ordering:");
}

#[tokio::test]
async fn debug_ranking_true_explains_frecency_ranking_with_envs_unset() {
    let _debug = EnvGuard::remove("SYMFORGE_DEBUG_RANKING");
    let _frecency = EnvGuard::remove("SYMFORGE_FRECENCY");
    let fx = Fixture::new(&[
        ("src/file_a_old.rs", "pub fn item_a() {}\n"),
        ("src/file_b_new.rs", "pub fn item_b() {}\n"),
    ]);
    fx.frecency_store()
        .bump(&[PathBuf::from("src/file_b_new.rs")], now_ts())
        .expect("seed frecency");

    let result = call(
        &fx.server,
        json!({
            "query": "src/file_",
            "limit": 10,
            "rank_by": "frecency",
            "debug_ranking": true
        }),
    )
    .await;

    assert_before(&result, "src/file_b_new.rs", "src/file_a_old.rs");
    assert_contains(&result, "Capability: frecency ranking applied");
    assert_contains(&result, "Ranking explanation");
    assert_contains(&result, "requested rank mode: frecency");
    assert_contains(&result, "frecency signal: applied");
    assert_contains(&result, "co-change signal: not requested");
    assert_contains(&result, "final ordering:");
}

#[tokio::test]
async fn debug_ranking_true_explains_path_cochange_ranking_with_envs_unset() {
    let _debug = EnvGuard::remove("SYMFORGE_DEBUG_RANKING");
    let _coupling = EnvGuard::remove("SYMFORGE_COUPLING");
    let fx = Fixture::new_git(&[
        ("src/auth/routes.rs", "pub fn auth_routes() {}\n"),
        ("src/server/routes.rs", "pub fn server_routes() {}\n"),
        ("src/client/routes.rs", "pub fn client_routes() {}\n"),
    ]);
    let head = symforge::git::head_sha(&fx.root).expect("head sha");
    seed_ready_coupling(&fx.root, &head);
    let fx = Fixture::from_dir(fx._dir);

    let result = call(
        &fx.server,
        json!({
            "query": "routes.rs",
            "limit": 10,
            "rank_by": "path+cochange",
            "anchor_path": "src/auth/routes.rs",
            "debug_ranking": true
        }),
    )
    .await;

    assert_before(&result, "src/server/routes.rs", "src/auth/routes.rs");
    assert_contains(&result, "Capability: co-change ranking applied");
    assert_contains(&result, "Ranking explanation");
    assert_contains(&result, "requested rank mode: path+cochange");
    assert_contains(&result, "frecency signal: not requested");
    assert_contains(&result, "co-change signal: applied");
    assert_contains(&result, "final ordering:");
}

#[tokio::test]
async fn debug_env_defaults_ranking_explanation_on_without_request_field() {
    let _debug = EnvGuard::set("SYMFORGE_DEBUG_RANKING", "1");
    let fx = Fixture::new(&[("src/auth/routes.rs", "pub fn auth_routes() {}\n")]);

    let result = call(&fx.server, json!({"query": "routes.rs", "limit": 10})).await;

    assert_contains(&result, "Ranking explanation");
    assert_contains(&result, "requested rank mode: default path");
}
