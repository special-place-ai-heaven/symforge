use std::fs;
use std::path::Path;

use symforge::live_index::LiveIndex;
use symforge::live_index::coupling::{AnchorKey, CouplingRow, CouplingStore};
use tempfile::TempDir;

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[allow(unsafe_code)] // test-only env guard serializes process env mutation.
impl EnvGuard {
    fn remove(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: the project verification command runs tests with --test-threads=1.
        unsafe {
            std::env::remove_var(key);
        }
        Self { key, previous }
    }
}

#[allow(unsafe_code)] // test-only env guard restores serialized process env mutation.
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

fn write_workspace(root: &Path) {
    fs::create_dir_all(root.join("src/auth")).unwrap();
    fs::create_dir_all(root.join("src/server")).unwrap();
    fs::write(root.join("src/auth/routes.rs"), "pub fn auth_routes() {}\n").unwrap();
    fs::write(
        root.join("src/server/routes.rs"),
        "pub fn server_routes() {}\n",
    )
    .unwrap();
}

fn init_git_repo_with_workspace() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_workspace(tmp.path());

    let repo = git2::Repository::init(tmp.path()).unwrap();
    let sig = git2::Signature::now("SymForge Tests", "symforge-tests@example.com").unwrap();
    let tree_id = {
        let mut index = repo.index().unwrap();
        index
            .add_all(["src"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write_tree().unwrap()
    };
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "root", &tree, &[])
        .unwrap();
    drop(tree);
    drop(repo);
    tmp
}

#[test]
fn env_unset_startup_does_not_create_missing_coupling_store() {
    let _env = EnvGuard::remove("SYMFORGE_COUPLING");
    let tmp = init_git_repo_with_workspace();
    let db_path = tmp.path().join(symforge::paths::SYMFORGE_COUPLING_DB_PATH);

    let shared = LiveIndex::load(tmp.path()).unwrap();

    assert!(shared.read().coupling_store().is_none());
    assert!(
        !db_path.exists(),
        "env-unset startup must stay lazy and avoid creating coupling.db"
    );
}

#[test]
fn env_unset_startup_opens_existing_ready_coupling_store_without_warm_build() {
    let _env = EnvGuard::remove("SYMFORGE_COUPLING");
    let tmp = init_git_repo_with_workspace();
    let db_path = tmp.path().join(symforge::paths::SYMFORGE_COUPLING_DB_PATH);
    let store = CouplingStore::open(&db_path).unwrap();
    let head = symforge::git::head_sha(tmp.path()).unwrap();
    store.set_last_head(&head).unwrap();
    store.set_cold_built_at(1_700_000_000).unwrap();
    store
        .bulk_upsert(&[CouplingRow {
            anchor: AnchorKey::file("src/auth/routes.rs"),
            partner: AnchorKey::file("src/server/routes.rs"),
            shared_commits: 3,
            weighted_score: 11.0,
            last_commit_ts: 1_700_000_000,
        }])
        .unwrap();
    drop(store);

    let shared = LiveIndex::load(tmp.path()).unwrap();

    assert!(
        shared.read().coupling_store().is_some(),
        "env-unset startup should expose an existing ready store for call-time ranking"
    );
}
