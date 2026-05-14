use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::json;
use symforge::daemon::{DaemonHandle, OpenProjectRequest, spawn_daemon};
use tempfile::{TempDir, tempdir};

const CRATE_COUNT: usize = 8;
const MODULES_PER_CRATE: usize = 128;
const TESTS_PER_CRATE: usize = 4;
const EXPECTED_MIN_FILES: usize = 1_000;
const EXPECTED_MAX_FILES: usize = 1_200;
const FILE_COUNT_TOLERANCE: usize = 2;
const FAST_IDLE: Duration = Duration::from_secs(35);
const FULL_IDLE: Duration = Duration::from_secs(5 * 60);

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: this test binary serializes environment mutation with ENV_LOCK.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => {
                // SAFETY: see EnvVarGuard::set.
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            }
            None => {
                // SAFETY: see EnvVarGuard::set.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
}

struct AapFixture {
    root: TempDir,
    file_count: usize,
}

struct SmokeObservation {
    elapsed: Duration,
    root_a_disk_before: usize,
    root_a_disk_after: usize,
    root_a_index_before_idle: usize,
    root_a_index_after_idle: usize,
    root_b_disk_before: usize,
    root_b_disk_after: usize,
    root_b_index_before_idle: usize,
    root_b_index_after_idle: usize,
}

impl SmokeObservation {
    fn log(&self, label: &str) {
        eprintln!(
            "aap smoke {label}: elapsed={:?}; root_a_index={} -> {} (delta {}); root_b_index={} -> {} (delta {}); root_a_disk={} -> {} (delta {}); root_b_disk={} -> {} (delta {})",
            self.elapsed,
            self.root_a_index_before_idle,
            self.root_a_index_after_idle,
            self.root_a_index_before_idle
                .abs_diff(self.root_a_index_after_idle),
            self.root_b_index_before_idle,
            self.root_b_index_after_idle,
            self.root_b_index_before_idle
                .abs_diff(self.root_b_index_after_idle),
            self.root_a_disk_before,
            self.root_a_disk_after,
            self.root_a_disk_before.abs_diff(self.root_a_disk_after),
            self.root_b_disk_before,
            self.root_b_disk_after,
            self.root_b_disk_before.abs_diff(self.root_b_disk_after)
        );
    }
}

fn run_with_runtime<F>(future: F)
where
    F: Future<Output = ()>,
{
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()
        .expect("build test runtime")
        .block_on(future);
}

fn write_file(root: &Path, relative_path: impl AsRef<Path>, contents: impl AsRef<[u8]>) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directory");
    }
    std::fs::write(path, contents).expect("write fixture file");
}

fn crate_relative_dir(prefix: &str, crate_index: usize) -> PathBuf {
    match crate_index % 4 {
        0 => PathBuf::from(format!("{prefix}_workspace/crate_{crate_index:02}")),
        1 => PathBuf::from(format!(
            "{prefix}_workspace/group_{:02}/crate_{crate_index:02}",
            crate_index / 2
        )),
        2 => PathBuf::from(format!(
            "{prefix}_workspace/group_{:02}/nested/crate_{crate_index:02}",
            crate_index / 2
        )),
        _ => PathBuf::from(format!(
            "{prefix}_workspace/group_{:02}/nested/deeper/crate_{crate_index:02}",
            crate_index / 2
        )),
    }
}

fn path_for_toml(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn build_fixture(prefix: &str) -> AapFixture {
    let root = tempdir().expect("create fixture root");
    let mut members = Vec::with_capacity(CRATE_COUNT);

    for crate_index in 0..CRATE_COUNT {
        let crate_dir = crate_relative_dir(prefix, crate_index);
        members.push(path_for_toml(&crate_dir));

        let package_name = format!("{prefix}_crate_{crate_index:02}");
        write_file(
            root.path(),
            crate_dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n"
            ),
        );

        write_file(
            root.path(),
            crate_dir.join("src/lib.rs"),
            format!(
                "pub mod generated;\n\npub fn {prefix}_crate_{crate_index:02}_marker() -> usize {{ {crate_index} }}\n"
            ),
        );

        let mut generated_mod = String::new();
        for module_index in 0..MODULES_PER_CRATE {
            let module_name = format!("{prefix}_mod_{crate_index:02}_{module_index:03}");
            generated_mod.push_str(&format!("pub mod {module_name};\n"));
            write_file(
                root.path(),
                crate_dir
                    .join("src/generated")
                    .join(format!("{module_name}.rs")),
                format!(
                    "pub fn value() -> usize {{ {} }}\n",
                    crate_index * 10_000 + module_index
                ),
            );
        }
        write_file(
            root.path(),
            crate_dir.join("src/generated/mod.rs"),
            generated_mod,
        );

        for test_index in 0..TESTS_PER_CRATE {
            write_file(
                root.path(),
                crate_dir
                    .join("tests")
                    .join(format!("smoke_{test_index:02}.rs")),
                format!(
                    "#[test]\nfn {prefix}_crate_{crate_index:02}_smoke_{test_index:02}() {{\n    assert_eq!({}, {});\n}}\n",
                    crate_index + test_index,
                    crate_index + test_index
                ),
            );
        }
    }

    let members_toml = members
        .iter()
        .map(|member| format!("    \"{member}\","))
        .collect::<Vec<_>>()
        .join("\n");
    write_file(
        root.path(),
        "Cargo.toml",
        format!("[workspace]\nmembers = [\n{members_toml}\n]\nresolver = \"2\"\n"),
    );
    write_file(
        root.path(),
        "src/lib.rs",
        format!("pub fn {prefix}_root_marker() -> usize {{ {CRATE_COUNT} }}\n"),
    );
    write_file(
        root.path(),
        "tests/root_smoke.rs",
        "#[test]\nfn root_smoke() {\n    assert!(true);\n}\n",
    );

    let file_count = count_regular_files(root.path());
    assert!(
        (EXPECTED_MIN_FILES..=EXPECTED_MAX_FILES).contains(&file_count),
        "AAP fixture should stay near 1100 files, got {file_count}"
    );

    AapFixture { root, file_count }
}

fn count_regular_files(root: &Path) -> usize {
    fn visit(path: &Path, count: &mut usize) {
        for entry in std::fs::read_dir(path).expect("read directory") {
            let entry = entry.expect("read directory entry");
            if entry.file_name() == ".symforge" {
                continue;
            }
            let file_type = entry.file_type().expect("read file type");
            if file_type.is_dir() {
                visit(&entry.path(), count);
            } else if file_type.is_file() {
                *count += 1;
            }
        }
    }

    let mut count = 0;
    visit(root, &mut count);
    count
}

fn normalized_path_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .expect("canonicalize project root")
        .to_string_lossy()
        .replace('\\', "/")
}

fn project_file_count(daemon: &DaemonHandle, root: &Path) -> usize {
    let canonical_root = normalized_path_string(root);
    let projects = daemon.state.list_projects();
    let project = projects
        .iter()
        .find(|project| project.canonical_root == canonical_root)
        .unwrap_or_else(|| {
            panic!(
                "project for {canonical_root} not found; active roots: {:?}",
                projects
                    .iter()
                    .map(|project| project.canonical_root.as_str())
                    .collect::<Vec<_>>()
            )
        });
    daemon
        .state
        .project_health(&project.project_id)
        .expect("project health")
        .file_count
}

fn assert_within_tolerance(label: &str, before: usize, after: usize) {
    let delta = before.abs_diff(after);
    assert!(
        delta <= FILE_COUNT_TOLERANCE,
        "{label} file count changed by {delta}; before={before}, after={after}, tolerance={FILE_COUNT_TOLERANCE}"
    );
}

async fn call_index_folder(
    client: &reqwest::Client,
    daemon: &DaemonHandle,
    session_id: &str,
    root: &Path,
) {
    let response = client
        .post(format!(
            "http://127.0.0.1:{}/v1/sessions/{session_id}/tools/index_folder",
            daemon.port
        ))
        .json(&json!({ "path": root.display().to_string() }))
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
}

async fn run_aap_smoke(idle: Duration) -> SmokeObservation {
    let started = Instant::now();
    let daemon_home = tempdir().expect("daemon home");
    let _home = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
    let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "30");

    let root_a = build_fixture("aap_a");
    let root_b = build_fixture("aap_b");
    let root_a_disk_before = root_a.file_count;
    let root_b_disk_before = root_b.file_count;

    let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
    let open = daemon
        .state
        .open_project_session(OpenProjectRequest {
            project_root: root_a.root.path().display().to_string(),
            client_name: "watcher-aap-smoke".to_string(),
            pid: Some(std::process::id()),
        })
        .expect("open project session");
    let client = reqwest::Client::new();

    call_index_folder(&client, &daemon, &open.session_id, root_a.root.path()).await;
    let root_a_index_before_idle = project_file_count(&daemon, root_a.root.path());
    tokio::time::sleep(idle).await;
    let root_a_index_after_idle = project_file_count(&daemon, root_a.root.path());
    assert_within_tolerance(
        "root A indexed",
        root_a_index_before_idle,
        root_a_index_after_idle,
    );

    call_index_folder(&client, &daemon, &open.session_id, root_b.root.path()).await;
    let root_b_index_before_idle = project_file_count(&daemon, root_b.root.path());
    tokio::time::sleep(idle).await;
    let root_b_index_after_idle = project_file_count(&daemon, root_b.root.path());
    assert_within_tolerance(
        "root B indexed",
        root_b_index_before_idle,
        root_b_index_after_idle,
    );

    let root_a_disk_after = count_regular_files(root_a.root.path());
    let root_b_disk_after = count_regular_files(root_b.root.path());
    assert_within_tolerance("root A disk", root_a_disk_before, root_a_disk_after);
    assert_within_tolerance("root B disk", root_b_disk_before, root_b_disk_after);

    let _ = daemon.shutdown_tx.send(());
    tokio::time::sleep(Duration::from_millis(100)).await;

    SmokeObservation {
        elapsed: started.elapsed(),
        root_a_disk_before,
        root_a_disk_after,
        root_a_index_before_idle,
        root_a_index_after_idle,
        root_b_disk_before,
        root_b_disk_after,
        root_b_index_before_idle,
        root_b_index_after_idle,
    }
}

#[test]
fn aap_smoke_no_destruction() {
    let _env_lock = ENV_LOCK.lock().expect("env lock");
    run_with_runtime(async {
        // CI fast variant: two 35s idles, enough for one 30s reconcile interval plus margin.
        let observation = run_aap_smoke(FAST_IDLE).await;
        observation.log("fast");
    });
}

#[test]
#[ignore = "full AAP smoke: two 5-minute idle windows"]
fn aap_smoke_no_destruction_full_5_min() {
    let _env_lock = ENV_LOCK.lock().expect("env lock");
    run_with_runtime(async {
        // Manual full variant: mirrors G1.8's original two 5-minute idle windows.
        let observation = run_aap_smoke(FULL_IDLE).await;
        observation.log("full");
    });
}
