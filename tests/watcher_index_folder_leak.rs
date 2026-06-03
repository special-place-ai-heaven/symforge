// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use parking_lot::Mutex;
use rmcp::handler::server::wrapper::Parameters;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::tools::IndexFolderInput;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[allow(unsafe_code)] // test-only env guard serializes daemon home mutation.
impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: project tests are run with `--test-threads=1`.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

#[allow(unsafe_code)] // test-only env guard restores serialized daemon home mutation.
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => {
                // SAFETY: project tests are run with `--test-threads=1`.
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            }
            None => {
                // SAFETY: project tests are run with `--test-threads=1`.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
}

fn write_project_files(root: &Path, prefix: &str, count: usize) {
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    for idx in 0..count {
        let function_name = format!("{prefix}_{idx:03}");
        let relative_path = format!("src/{function_name}.rs");
        std::fs::write(
            root.join(relative_path),
            format!("pub fn {function_name}() -> usize {{ {idx} }}\n"),
        )
        .expect("write source file");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_index_folder_preserves_file_count() {
    let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");
    let project_a = TempDir::new().expect("project a");
    let project_b = TempDir::new().expect("project b");
    write_project_files(project_a.path(), "a_file", 40);
    write_project_files(project_b.path(), "b_file", 25);

    let server = SymForgeServer::new(
        LiveIndex::empty(),
        "watcher_index_folder_leak_test".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        None,
        None,
    );

    let first = server
        .index_folder(Parameters(IndexFolderInput {
            path: project_a.path().display().to_string(),
            idempotency_key: None,
        }))
        .await;
    assert!(
        first.starts_with("Indexed "),
        "first index_folder should succeed, got: {first}"
    );

    let second = server
        .index_folder(Parameters(IndexFolderInput {
            path: project_b.path().display().to_string(),
            idempotency_key: None,
        }))
        .await;
    assert!(
        second.starts_with("Indexed "),
        "second index_folder should succeed, got: {second}"
    );

    tokio::time::sleep(Duration::from_millis(3_200)).await;

    let published = server.index().published_state();
    assert_eq!(
        published.file_count, 25,
        "second root's file count must remain intact after the old watcher \
         has had time to reconcile"
    );

    let index = server.index().read();
    for idx in 0..25 {
        let path = format!("src/b_file_{idx:03}.rs");
        assert!(
            index.get_file(&path).is_some(),
            "project B file should remain indexed after repeated index_folder: {path}"
        );
    }
}
