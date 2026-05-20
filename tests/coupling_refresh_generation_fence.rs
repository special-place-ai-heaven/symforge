//! Generation-fence tests for coupling refresh pre-flight checks.

use std::path::Path;
use std::sync::Mutex;

use symforge::live_index::LiveIndex;
use symforge::live_index::coupling::refresh_on_reconcile_tick;
use tempfile::tempdir;

mod git_test_helpers {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/git/test_helpers.rs"
    ));
}

static COUPLING_ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[allow(unsafe_code)] // test-only env guard serializes coupling flag mutation.
impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: these tests run under the project-mandated `--test-threads=1`;
        // this guard also serializes mutation within this test binary.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

#[allow(unsafe_code)] // test-only env guard restores serialized coupling flag mutation.
impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => {
                // SAFETY: see EnvGuard::set.
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            }
            None => {
                // SAFETY: see EnvGuard::set.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
}

fn init_repo_with_root_commit(root: &Path) {
    let repo = git2::Repository::init(root).expect("git init");
    let sig = git2::Signature::now("t", "t@x").expect("sig");
    let tree_id = {
        let mut idx = repo.index().expect("index");
        idx.write_tree().expect("write tree")
    };
    let tree = repo.find_tree(tree_id).expect("tree");
    git_test_helpers::commit_head_with_retry(&repo, &sig, &sig, "root", &tree, &[]);
}

#[test]
fn stale_refresh_aborts_pre_flight() {
    let _lock = COUPLING_ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("SYMFORGE_COUPLING", "1");

    let root_a = tempdir().unwrap();
    init_repo_with_root_commit(root_a.path());
    let root_b = tempdir().unwrap();
    init_repo_with_root_commit(root_b.path());

    let shared = LiveIndex::empty();
    let gen_a = shared.current_project_generation();
    let rejected_before = shared.current_rejected_stale_mutations();
    shared.reload(root_b.path()).unwrap();

    let db_path = root_a
        .path()
        .join(symforge::paths::SYMFORGE_COUPLING_DB_PATH);
    assert!(
        !db_path.exists(),
        "test setup must start without a coupling db"
    );

    refresh_on_reconcile_tick(root_a.path(), gen_a, &shared);

    assert!(
        !db_path.exists(),
        "stale pre-flight rejection must return before disk writes"
    );
    assert_eq!(
        shared.current_rejected_stale_mutations(),
        rejected_before + 1,
        "stale pre-flight rejection must increment telemetry exactly once"
    );
}

#[test]
fn current_refresh_proceeds_normally() {
    let _lock = COUPLING_ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("SYMFORGE_COUPLING", "1");

    let root = tempdir().unwrap();
    init_repo_with_root_commit(root.path());

    let shared = LiveIndex::empty();
    let expected_gen = shared.current_project_generation();
    let rejected_before = shared.current_rejected_stale_mutations();
    let db_path = root.path().join(symforge::paths::SYMFORGE_COUPLING_DB_PATH);
    assert!(
        !db_path.exists(),
        "test setup must start without a coupling db"
    );

    refresh_on_reconcile_tick(root.path(), expected_gen, &shared);

    assert!(
        db_path.exists(),
        "current-generation refresh must proceed to coupling db creation"
    );
    assert_eq!(
        shared.current_rejected_stale_mutations(),
        rejected_before,
        "current-generation refresh must not increment stale telemetry"
    );
}
