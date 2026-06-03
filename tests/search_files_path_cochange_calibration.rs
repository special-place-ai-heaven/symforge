// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Query-level calibration corpus for `search_files(rank_by="path+cochange")`.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::live_index::coupling::{AnchorKey, CouplingRow, CouplingStore};
use symforge::paths::SYMFORGE_COUPLING_DB_PATH;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

mod git_test_helpers {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/git/test_helpers.rs"
    ));
}

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[allow(unsafe_code)] // test-only env guard serializes ranking policy mutation.
impl EnvGuard {
    fn remove(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: project verification runs env-mutating tests with --test-threads=1.
        unsafe {
            std::env::remove_var(key);
        }
        Self { key, previous }
    }
}

#[allow(unsafe_code)] // test-only env guard restores serialized ranking policy mutation.
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
    server: SymForgeServer,
}

impl Fixture {
    fn new(files: &[(&str, &str)], rows: &[CouplingRow]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        write_files(&root, files);
        let head = init_git_repo(&root);
        seed_ready_coupling(&root, &head, rows);
        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "search_files_path_cochange_calibration_test".to_string(),
            watcher_info,
            Some(root.clone()),
            None,
        );
        Self { _dir: dir, server }
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
    let sig =
        git2::Signature::now("SymForge Tests", "symforge-tests@example.com").expect("git sig");
    let tree_id = {
        let mut index = repo.index().expect("git index");
        index
            .add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("git add");
        index.write_tree().expect("git write tree")
    };
    let tree = repo.find_tree(tree_id).expect("git tree");
    let commit = git_test_helpers::commit_head_with_retry(&repo, &sig, &sig, "root", &tree, &[]);
    let head = commit.to_string();
    drop(tree);
    drop(repo);
    head
}

fn seed_ready_coupling(root: &Path, head: &str, rows: &[CouplingRow]) {
    let store =
        CouplingStore::open(&root.join(SYMFORGE_COUPLING_DB_PATH)).expect("open coupling store");
    store.set_last_head(head).expect("set last head");
    store
        .set_cold_built_at(1_700_000_000)
        .expect("set cold built timestamp");
    store.bulk_upsert(rows).expect("seed coupling rows");
}

fn row(anchor: &str, partner: &str, shared: u32, weighted: f64) -> CouplingRow {
    CouplingRow {
        anchor: AnchorKey::file(anchor),
        partner: AnchorKey::file(partner),
        shared_commits: shared,
        weighted_score: weighted,
        last_commit_ts: 1_700_000_000,
    }
}

async fn call(server: &SymForgeServer, params: Value) -> String {
    server.dispatch_tool_for_tests("search_files", params).await
}

fn returned_paths(result: &str) -> Vec<String> {
    result
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if !line.starts_with("  ") || trimmed.starts_with('[') {
                return None;
            }
            trimmed
                .split("  [")
                .next()
                .filter(|path| !path.is_empty())
                .map(str::to_string)
        })
        .collect()
}

fn assert_contains(result: &str, needle: &str) {
    assert!(
        result.contains(needle),
        "expected result to contain `{needle}`; result was:\n{result}"
    );
}

#[tokio::test]
async fn weak_prefix_anchor_keeps_baseline_path_order_for_path_cochange() {
    let _coupling = EnvGuard::remove("SYMFORGE_COUPLING");
    let fx = Fixture::new(
        &[
            ("src/auth/routes.rs", "pub fn auth_routes() {}\n"),
            ("src/client/routes.rs", "pub fn client_routes() {}\n"),
            ("src/server/routes.rs", "pub fn server_routes() {}\n"),
        ],
        &[row(
            "src/auth/routes.rs",
            "src/server/routes.rs",
            4,
            10_000.0,
        )],
    );

    let baseline = call(&fx.server, json!({"query": "rou", "limit": 10})).await;
    let reranked = call(
        &fx.server,
        json!({
            "query": "rou",
            "limit": 10,
            "rank_by": "path+cochange",
            "anchor_path": "src/auth/routes.rs",
            "debug_ranking": true
        }),
    )
    .await;

    assert_eq!(returned_paths(&reranked), returned_paths(&baseline));
    assert_contains(&reranked, "Capability: co-change ranking fallback used");
    assert_contains(
        &reranked,
        "none matched returned candidates or passed rank gates; path ranking returned",
    );
}

#[tokio::test]
async fn hardcoded_changelog_chore_anchor_keeps_baseline_path_order_for_path_cochange() {
    let _coupling = EnvGuard::remove("SYMFORGE_COUPLING");
    let fx = Fixture::new(
        &[
            ("CHANGELOG.md", "# root changelog\n"),
            ("src/release/CHANGELOG.md", "# nested changelog\n"),
        ],
        &[row(
            "CHANGELOG.md",
            "src/release/CHANGELOG.md",
            120,
            50_000.0,
        )],
    );

    let baseline = call(&fx.server, json!({"query": "CHANGELOG.md", "limit": 10})).await;
    let reranked = call(
        &fx.server,
        json!({
            "query": "CHANGELOG.md",
            "limit": 10,
            "rank_by": "path+cochange",
            "anchor_path": "CHANGELOG.md",
            "debug_ranking": true
        }),
    )
    .await;

    assert_eq!(returned_paths(&reranked), returned_paths(&baseline));
    assert_contains(&reranked, "Capability: co-change ranking fallback used");
    assert_contains(
        &reranked,
        "none matched returned candidates or passed rank gates; path ranking returned",
    );
}
