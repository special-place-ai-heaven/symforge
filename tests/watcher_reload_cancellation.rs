// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use parking_lot::Mutex;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::json;
use symforge::daemon::{OpenProjectRequest, spawn_daemon};
use symforge::live_index::{LiveIndex, SharedIndex};
use symforge::watcher::{WatcherInfo, run_watcher_with_stop};
use tempfile::TempDir;

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[allow(unsafe_code)] // test-only env guard serializes daemon home mutation.
impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: these tests run under the project-mandated `--test-threads=1`.
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
                // SAFETY: these tests run under the project-mandated `--test-threads=1`.
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            }
            None => {
                // SAFETY: these tests run under the project-mandated `--test-threads=1`.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
}

fn write_project_files(root: &Path, prefix: &str, count: usize) -> Vec<String> {
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");

    (0..count)
        .map(|idx| {
            let relative_path = format!("src/{prefix}_{idx:03}.rs");
            let function_name = format!("{prefix}_{idx:03}");
            std::fs::write(
                root.join(&relative_path),
                format!("pub fn {function_name}() -> usize {{ {idx} }}\n"),
            )
            .expect("write source file");
            relative_path
        })
        .collect()
}

fn run_with_single_blocking_thread<F>(future: F)
where
    F: Future<Output = ()>,
{
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .expect("build test runtime")
        .block_on(future);
}

fn spawn_watcher_task(
    root: PathBuf,
    shared: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    stop_token: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run_watcher_with_stop(root, shared, watcher_info, stop_token).await;
    })
}

#[test]
fn reload_cross_root_preserves_file_count() {
    run_with_single_blocking_thread(async {
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");
        let project_a = TempDir::new().expect("project a");
        let project_b = TempDir::new().expect("project b");
        let _a_paths = write_project_files(project_a.path(), "a_file", 50);
        let b_paths = write_project_files(project_b.path(), "b_file", 30);

        let shared = LiveIndex::load(project_a.path()).expect("load project a");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let old_stop = Arc::new(AtomicBool::new(false));
        let old_task = spawn_watcher_task(
            project_a.path().to_path_buf(),
            Arc::clone(&shared),
            Arc::clone(&watcher_info),
            Arc::clone(&old_stop),
        );

        let (release_blocker, blocker_released) = std::sync::mpsc::channel::<()>();
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = blocker_released.recv();
        });

        tokio::time::sleep(Duration::from_millis(1_300)).await;
        let stale_generation = shared.current_project_generation();
        shared.reload(project_b.path()).expect("reload project b");

        let slipped_paths: Vec<String> = {
            let index = shared.read();
            index.all_files().map(|(path, _)| path.clone()).collect()
        };
        assert_eq!(
            slipped_paths.len(),
            30,
            "slipped doomed task should have read project B's path set"
        );

        old_stop.store(true, Ordering::Release);
        old_task.abort();

        for path in &slipped_paths {
            assert!(
                !shared.remove_file_at_generation(path, stale_generation),
                "stale-generation remove should be rejected for slipped path: {path}"
            );
        }

        let new_stop = Arc::new(AtomicBool::new(false));
        let new_task = spawn_watcher_task(
            project_b.path().to_path_buf(),
            Arc::clone(&shared),
            Arc::clone(&watcher_info),
            Arc::clone(&new_stop),
        );

        release_blocker.send(()).expect("release blocking pool");
        blocker.await.expect("blocking guard should finish");
        tokio::time::sleep(Duration::from_millis(3_200)).await;

        let published = shared.published_state();
        assert_eq!(
            published.file_count, 30,
            "reload should preserve exactly project B's indexed files"
        );

        let index = shared.read();
        for path in &b_paths {
            assert!(
                index.get_file(path).is_some(),
                "project B file should remain reachable after old watcher cancellation: {path}"
            );
        }

        new_stop.store(true, Ordering::Release);
        new_task.abort();
    });
}

#[test]
fn reload_signals_token_before_new_watcher() {
    run_with_single_blocking_thread(async {
        let project_a = TempDir::new().expect("project a");
        let project_b = TempDir::new().expect("project b");
        let _a_paths = write_project_files(project_a.path(), "a_file", 2);
        let _b_paths = write_project_files(project_b.path(), "b_file", 2);
        let shared = LiveIndex::load(project_a.path()).expect("load project a");

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let old_stop = Arc::new(AtomicBool::new(false));
        let old_task = spawn_watcher_task(
            project_a.path().to_path_buf(),
            Arc::clone(&shared),
            Arc::clone(&watcher_info),
            Arc::clone(&old_stop),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;

        old_stop.store(true, Ordering::Release);
        let old_cancelled_before_spawn = old_stop.load(Ordering::Acquire);

        let new_stop = Arc::new(AtomicBool::new(false));
        let new_spawn_observed_old_cancelled = old_stop.load(Ordering::Acquire);
        let new_task = spawn_watcher_task(
            project_b.path().to_path_buf(),
            Arc::clone(&shared),
            Arc::clone(&watcher_info),
            Arc::clone(&new_stop),
        );

        assert!(
            old_cancelled_before_spawn,
            "old stop token must be signaled before spawning the replacement watcher"
        );
        assert!(
            new_spawn_observed_old_cancelled,
            "replacement watcher spawn must observe the old token as already cancelled"
        );
        assert!(
            !new_stop.load(Ordering::Acquire),
            "replacement watcher must receive a fresh unsignaled token"
        );

        old_task.abort();
        new_stop.store(true, Ordering::Release);
        new_task.abort();
    });
}

#[test]
fn index_folder_for_session_signals_token_before_drop() {
    run_with_single_blocking_thread(async {
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");
        // The daemon is fail-closed and always requires a token; pin a known one
        // so the HTTP `index_folder` call below can authenticate.
        let auth_token = "watcher-rebind-test-token";
        let _auth = EnvVarGuard::set("SYMFORGE_DAEMON_AUTH_TOKEN", auth_token);
        let project_a = TempDir::new().expect("project a");
        let project_b = TempDir::new().expect("project b");
        let _a_paths = write_project_files(project_a.path(), "a_file", 40);
        let b_paths = write_project_files(project_b.path(), "b_file", 25);

        let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let open = daemon
            .state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "watcher-rebind-test".to_string(),
                pid: Some(std::process::id()),
            })
            .expect("open project session");

        tokio::time::sleep(Duration::from_millis(1_300)).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!(
                "http://127.0.0.1:{}/v1/sessions/{}/tools/index_folder",
                daemon.port, open.session_id
            ))
            .bearer_auth(auth_token)
            .json(&json!({ "path": project_b.path().display().to_string() }))
            .send()
            .await
            .expect("call daemon index_folder")
            .error_for_status()
            .expect("index_folder status")
            .text()
            .await
            .expect("index_folder body");
        assert!(
            response.starts_with("Indexed "),
            "daemon index_folder should succeed, got: {response}"
        );

        tokio::time::sleep(Duration::from_millis(3_200)).await;

        let projects = daemon.state.list_projects();
        assert_eq!(
            projects.len(),
            1,
            "session rebind should remove the old project after moving the session"
        );
        let health = daemon
            .state
            .project_health(&projects[0].project_id)
            .expect("target project health");
        assert_eq!(
            health.file_count,
            b_paths.len(),
            "target project should retain all files after the old project is dropped"
        );

        let _ = daemon.shutdown_tx.send(());
    });
}
