//! Integration close-out coverage for call-time capability resolution.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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

const HELLO_RS: &str =
    "fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";

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
                unsafe { std::env::set_var(self.key, previous) };
            }
            None => {
                // SAFETY: see EnvGuard::remove.
                unsafe { std::env::remove_var(self.key) };
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
        init_git_repo_with_git2(&root);
        Self::from_dir(dir)
    }

    fn from_dir(dir: TempDir) -> Self {
        let root = dir.path().to_path_buf();
        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "capability_status_integration_test".to_string(),
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

struct WorktreeFixture {
    _dir: TempDir,
    indexed_root: PathBuf,
    worktree_root: PathBuf,
    server: SymForgeServer,
}

impl WorktreeFixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let indexed_root = dir.path().join("indexed");
        let worktree_root = dir.path().join("linked-worktree");
        write_files(&indexed_root, files);
        git_init_with_initial_commit(&indexed_root);
        git_worktree_add(&indexed_root, &worktree_root, "task");

        let shared = LiveIndex::load(&indexed_root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "capability_status_worktree_test".to_string(),
            watcher_info,
            Some(indexed_root.clone()),
            None,
        );

        Self {
            _dir: dir,
            indexed_root,
            worktree_root,
            server,
        }
    }

    fn read_indexed(&self, rel: &str) -> String {
        fs::read_to_string(self.indexed_root.join(rel)).expect("read indexed file")
    }

    fn read_worktree(&self, rel: &str) -> String {
        fs::read_to_string(self.worktree_root.join(rel)).expect("read worktree file")
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

fn init_git_repo_with_git2(root: &Path) -> String {
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

fn run_git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git spawn failed for {args:?}: {e}"));
    if !out.status.success() {
        panic!(
            "git {args:?} in {cwd:?} failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn git_init_with_initial_commit(root: &Path) {
    run_git(root, &["init", "-b", "main"]);
    run_git(root, &["config", "user.email", "test@example.com"]);
    run_git(root, &["config", "user.name", "capability-status-test"]);
    run_git(root, &["add", "-A"]);
    run_git(root, &["commit", "-m", "initial"]);
}

fn git_worktree_add(indexed_root: &Path, worktree_root: &Path, branch: &str) {
    if let Some(parent) = worktree_root.parent() {
        fs::create_dir_all(parent).expect("worktree parent dir");
    }
    run_git(
        indexed_root,
        &[
            "worktree",
            "add",
            "-b",
            branch,
            worktree_root.to_str().expect("utf-8 worktree path"),
        ],
    );
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

async fn call(server: &SymForgeServer, tool: &str, params: Value) -> String {
    server.dispatch_tool_for_tests(tool, params).await
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
async fn health_reports_compact_capability_states_with_envs_unset() {
    let _frecency = EnvGuard::remove("SYMFORGE_FRECENCY");
    let _coupling = EnvGuard::remove("SYMFORGE_COUPLING");
    let _worktree = EnvGuard::remove("SYMFORGE_WORKTREE_AWARE");
    let _debug = EnvGuard::remove("SYMFORGE_DEBUG_RANKING");
    let fx = Fixture::new_git(&[
        ("src/auth/routes.rs", "pub fn auth_routes() {}\n"),
        ("src/server/routes.rs", "pub fn server_routes() {}\n"),
    ]);

    let health = call(&fx.server, "health", json!({})).await;
    assert_contains(&health, "Capabilities:");
    assert_contains(&health, "frecency: ready/session");
    assert_contains(&health, "co-change: preparing/lazy-on-request");
    assert_contains(&health, "worktree routing: explicit-call enabled");
    assert_contains(
        &health,
        "ranking diagnostics: call-time explain available/default-off",
    );

    let compact = call(&fx.server, "health_compact", json!({})).await;
    assert_contains(&compact, "Capabilities:");
    assert_contains(&compact, "frecency=ready/session");
    assert_contains(&compact, "co-change=preparing/lazy-on-request");
    assert_contains(&compact, "worktree=explicit-call enabled");
    assert_contains(&compact, "ranking=call-time explain available/default-off");
}

#[tokio::test]
async fn health_reports_disabled_policy_capability_states() {
    let _frecency = EnvGuard::set("SYMFORGE_FRECENCY", "disabled");
    let _coupling = EnvGuard::set("SYMFORGE_COUPLING", "disabled");
    let _worktree = EnvGuard::set("SYMFORGE_WORKTREE_AWARE", "disabled");
    let _debug = EnvGuard::set("SYMFORGE_DEBUG_RANKING", "1");
    let fx = Fixture::new_git(&[("src/lib.rs", HELLO_RS)]);

    let health = call(&fx.server, "health", json!({})).await;

    assert_contains(&health, "frecency: disabled by policy");
    assert_contains(&health, "co-change: disabled by policy");
    assert_contains(&health, "worktree routing: disabled by policy");
    assert_contains(
        &health,
        "ranking diagnostics: call-time explain available/default-on",
    );

    let compact = call(&fx.server, "health_compact", json!({})).await;
    assert_contains(&compact, "ranking=call-time explain available/default-on");
}

#[tokio::test]
async fn health_reports_ranking_diagnostics_disabled_by_policy_when_env_disabled() {
    let _debug = EnvGuard::set("SYMFORGE_DEBUG_RANKING", "disabled");
    let fx = Fixture::new_git(&[("src/lib.rs", HELLO_RS)]);

    let health = call(&fx.server, "health", json!({})).await;

    assert_contains(&health, "ranking diagnostics: disabled by policy");
    assert_not_contains(
        &health,
        "ranking diagnostics: call-time explain available/default-off",
    );
    assert_not_contains(
        &health,
        "ranking diagnostics: call-time explain available/default-on",
    );

    let compact = call(&fx.server, "health_compact", json!({})).await;
    assert_contains(&compact, "ranking=disabled by policy");
}

#[tokio::test]
async fn health_reports_ready_and_stale_cochange_store_states() {
    let _coupling = EnvGuard::remove("SYMFORGE_COUPLING");
    let fx = Fixture::new_git(&[
        ("src/auth/routes.rs", "pub fn auth_routes() {}\n"),
        ("src/server/routes.rs", "pub fn server_routes() {}\n"),
    ]);
    let head = symforge::git::head_sha(&fx.root).expect("head sha");
    seed_ready_coupling(&fx.root, &head);
    let fx = Fixture::from_dir(fx._dir);

    let ready = call(&fx.server, "health", json!({})).await;
    assert_contains(&ready, "co-change: ready/current");

    let stale_store =
        CouplingStore::open(&fx.root.join(SYMFORGE_COUPLING_DB_PATH)).expect("open coupling store");
    stale_store.set_last_head("stale-head").expect("stale head");

    let stale = call(&fx.server, "health", json!({})).await;
    assert_contains(&stale, "co-change: stale/head-mismatch");
}

#[tokio::test]
async fn env_unset_frecency_request_reports_applied_evidence() {
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
        "search_files",
        json!({
            "query": "src/file_",
            "limit": 10,
            "rank_by": "frecency"
        }),
    )
    .await;

    assert_before(&result, "src/file_b_new.rs", "src/file_a_old.rs");
    assert_contains(&result, "Capability: frecency ranking applied");
}

#[tokio::test]
async fn env_unset_path_cochange_request_reports_applied_evidence() {
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
        "search_files",
        json!({
            "query": "routes.rs",
            "limit": 10,
            "rank_by": "path+cochange",
            "anchor_path": "src/auth/routes.rs"
        }),
    )
    .await;

    assert_before(&result, "src/server/routes.rs", "src/auth/routes.rs");
    assert_contains(&result, "Capability: co-change ranking applied");
}

#[tokio::test]
async fn working_directory_call_time_reports_routing_evidence_with_env_unset() {
    let _worktree = EnvGuard::remove("SYMFORGE_WORKTREE_AWARE");
    let fx = WorktreeFixture::new(&[("src/lib.rs", HELLO_RS)]);
    let wt_arg = fx.worktree_root.to_str().unwrap().to_string();

    let result = call(
        &fx.server,
        "replace_symbol_body",
        json!({
            "path": "src/lib.rs",
            "name": "hello",
            "new_body": "fn hello() {\n    println!(\"HELLO_WT\");\n}",
            "working_directory": wt_arg,
        }),
    )
    .await;

    assert_contains(&result, "rerouted: true");
    assert_contains(&result, "wrote_to");
    assert_contains(&result, "indexed_path");
    assert!(fx.read_worktree("src/lib.rs").contains("HELLO_WT"));
    assert!(!fx.read_indexed("src/lib.rs").contains("HELLO_WT"));
}

#[tokio::test]
async fn debug_ranking_call_time_reports_without_global_env() {
    let _debug = EnvGuard::remove("SYMFORGE_DEBUG_RANKING");
    let fx = Fixture::new(&[
        ("src/auth/routes.rs", "pub fn auth_routes() {}\n"),
        ("src/server/routes.rs", "pub fn server_routes() {}\n"),
    ]);

    let result = call(
        &fx.server,
        "search_files",
        json!({
            "query": "routes.rs",
            "limit": 10,
            "debug_ranking": true
        }),
    )
    .await;

    assert_contains(&result, "Ranking explanation");
    assert_contains(&result, "requested rank mode: default path");
}
