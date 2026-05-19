use parking_lot::Mutex;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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

async fn wait_until<F>(timeout: Duration, mut condition: F) -> bool
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    condition()
}

#[cfg(windows)]
#[test]
fn transient_av_lock_does_not_remove_file() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    run_with_single_blocking_thread(async {
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");
        let project = TempDir::new().expect("project");
        let paths = write_project_files(project.path(), "locked", 1);
        let locked_path = project.path().join(&paths[0]);
        let shared = LiveIndex::load(project.path()).expect("load project");

        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&locked_path)
            .expect("open file with FILE_SHARE_NONE");

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let task = spawn_watcher_task(
            project.path().to_path_buf(),
            Arc::clone(&shared),
            Arc::clone(&watcher_info),
            Arc::clone(&stop),
        );

        tokio::time::sleep(Duration::from_millis(1_300)).await;

        let index = shared.read();
        assert!(
            index.get_file(&paths[0]).is_some(),
            "transient exclusive lock must not remove an indexed file"
        );
        drop(index);

        stop.store(true, Ordering::Release);
        task.abort();
    });
}

#[cfg(not(windows))]
#[test]
#[ignore = "Windows-only FILE_SHARE_NONE semantics are required to simulate the AV-lock case"]
fn transient_av_lock_does_not_remove_file() {}

#[test]
fn permanent_deletion_still_removes() {
    run_with_single_blocking_thread(async {
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");
        let project = TempDir::new().expect("project");
        let paths = write_project_files(project.path(), "deleted", 1);
        let shared = LiveIndex::load(project.path()).expect("load project");

        std::fs::remove_file(project.path().join(&paths[0])).expect("delete indexed file");

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let task = spawn_watcher_task(
            project.path().to_path_buf(),
            Arc::clone(&shared),
            Arc::clone(&watcher_info),
            Arc::clone(&stop),
        );

        let removed = wait_until(Duration::from_secs(5), || {
            let index = shared.read();
            index.get_file(&paths[0]).is_none()
        })
        .await;

        stop.store(true, Ordering::Release);
        task.abort();

        assert!(
            removed,
            "persistent NotFound should remove the file after the retry window"
        );
    });
}

#[test]
fn bulk_deletion_storm_completes_within_baseline() {
    run_with_single_blocking_thread(async {
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");
        let project = TempDir::new().expect("project");
        let paths = write_project_files(project.path(), "storm", 120);
        let shared = LiveIndex::load(project.path()).expect("load project");

        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let task = spawn_watcher_task(
            project.path().to_path_buf(),
            Arc::clone(&shared),
            Arc::clone(&watcher_info),
            Arc::clone(&stop),
        );
        tokio::time::sleep(Duration::from_millis(150)).await;

        let start = Instant::now();
        for path in &paths {
            std::fs::remove_file(project.path().join(path)).expect("delete storm file");
        }

        let converged = wait_until(Duration::from_secs(4), || {
            let index = shared.read();
            index.file_count() == 0
        })
        .await;
        let elapsed = start.elapsed();

        stop.store(true, Ordering::Release);
        task.abort();

        let fixed_window_baseline = Duration::from_secs(2);
        assert!(
            converged,
            "bulk deletion storm should converge within the fixed observation window"
        );
        assert!(
            elapsed <= fixed_window_baseline * 2,
            "bulk deletion storm convergence should stay within 2x baseline: elapsed={elapsed:?}, baseline={fixed_window_baseline:?}"
        );
    });
}
