use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{Value, json};
use symforge::edit_safety::trust::{
    CI_ENV_VARS, ProjectConfigTrust, TRUST_ENV_OVERRIDE, TrustStatus, default_trust_store_path,
};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;

const TRUST_MODE_ENV: &str = "SYMFORGE_PROJECT_CONFIG_TRUST_MODE";

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

#[allow(unsafe_code)] // test-only env guard serializes process env mutation.
impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: this test binary is run with --test-threads=1 and env-mutating
        // tests also take ENV_LOCK, so there is no concurrent environment access.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }

    fn unset(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: see EnvGuard::set.
        unsafe {
            std::env::remove_var(key);
        }
        Self { key, previous }
    }
}

#[allow(unsafe_code)] // test-only env guard restores serialized process env mutation.
impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: see EnvGuard::set.
        unsafe {
            match &self.previous {
                Some(previous) => std::env::set_var(self.key, previous),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn clean_trust_env() -> (MutexGuard<'static, ()>, Vec<EnvGuard>) {
    let guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut vars = vec![
        EnvGuard::unset(TRUST_ENV_OVERRIDE),
        EnvGuard::unset(TRUST_MODE_ENV),
    ];
    vars.extend(CI_ENV_VARS.iter().copied().map(EnvGuard::unset));
    (guard, vars)
}

fn write_project_config(project_root: &Path, body: &str) {
    let symforge_dir = project_root.join(".symforge");
    std::fs::create_dir_all(&symforge_dir).unwrap();
    std::fs::write(symforge_dir.join("config.toml"), body).unwrap();
}

fn trust_for(test_root: &Path) -> ProjectConfigTrust {
    ProjectConfigTrust::with_store_path(test_root.join("data").join("symforge").join("trust.json"))
}

fn assert_sha256_hex(hash: &str) {
    assert_eq!(hash.len(), 64, "hash should be SHA-256 hex: {hash}");
    assert!(
        hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "hash should contain only hex digits: {hash}"
    );
}

fn isolated_data_home(data_home: &Path) -> Vec<EnvGuard> {
    let value = data_home
        .to_str()
        .expect("temp data home path should be UTF-8");
    vec![
        EnvGuard::set("LOCALAPPDATA", value),
        EnvGuard::set("XDG_DATA_HOME", value),
    ]
}

fn symforge_command(data_home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_symforge"));
    command
        .env("LOCALAPPDATA", data_home)
        .env("XDG_DATA_HOME", data_home)
        .env_remove(TRUST_ENV_OVERRIDE)
        .env_remove(TRUST_MODE_ENV);
    for var in CI_ENV_VARS {
        command.env_remove(var);
    }
    command
}

fn command_stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn command_stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_command_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected command success\nstdout:\n{}\nstderr:\n{}",
        command_stdout(output),
        command_stderr(output)
    );
}

fn extract_actual_hash(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("actual_hash: "))
        .expect("status output should include actual_hash")
        .to_string()
}

fn write_source(project_root: &Path, rel: &str, body: &str) {
    let path = project_root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn edit_test_server(project_root: &Path) -> SymForgeServer {
    let shared = LiveIndex::load(project_root).expect("LiveIndex::load");
    let watcher_info = Arc::new(parking_lot::Mutex::new(WatcherInfo::default()));
    SymForgeServer::new(
        shared,
        "edit_safety_trust_test".to_string(),
        watcher_info,
        Some(project_root.to_path_buf()),
        None,
    )
}

async fn edit_target_value(server: &SymForgeServer, old_text: &str, new_text: &str) -> String {
    server
        .dispatch_tool_for_tests(
            "edit_within_symbol",
            json!({
                "path": "src/lib.rs",
                "name": "target",
                "old_text": old_text,
                "new_text": new_text
            }),
        )
        .await
}

#[test]
fn missing_store_returns_untrusted_with_computed_hash_evidence() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());

    let evaluation = trust.evaluate(temp.path());

    assert!(matches!(evaluation.status, TrustStatus::Untrusted));
    assert_sha256_hex(&evaluation.actual_hash);
    assert!(evaluation.project_key.is_some());
    assert!(evaluation.warnings.iter().any(|warning| {
        warning.contains("trust store")
            && warning.contains("does not exist")
            && warning.contains("untrusted")
    }));
}

#[test]
fn existing_store_without_project_record_returns_untrusted_with_hash_evidence() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());
    std::fs::create_dir_all(trust.store_path().parent().unwrap()).unwrap();
    std::fs::write(trust.store_path(), r#"{"schema_version":1,"records":{}}"#).unwrap();

    let evaluation = trust.evaluate(temp.path());

    assert!(matches!(evaluation.status, TrustStatus::Untrusted));
    assert_sha256_hex(&evaluation.actual_hash);
    assert!(
        evaluation.warnings.iter().any(|warning| {
            warning.contains("no trust record") && warning.contains("untrusted")
        })
    );
}

#[test]
fn record_trust_persists_precomputed_hash_and_rfc3339_timestamp() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());
    let evaluation = trust.evaluate(temp.path());
    let reviewed_hash = evaluation.actual_hash.clone();

    write_project_config(temp.path(), "rank_by = \"frecency\"\n");
    let record = trust.record_trust(&evaluation).unwrap();

    assert_eq!(record.trusted_hash, reviewed_hash);
    assert_ne!(record.trusted_hash, trust.evaluate(temp.path()).actual_hash);
    chrono::DateTime::parse_from_rfc3339(&record.trusted_at).expect("trusted_at should be RFC3339");

    let store: Value = serde_json::from_str(&std::fs::read_to_string(trust.store_path()).unwrap())
        .expect("store should be valid JSON");
    let project_key = evaluation.project_key.unwrap();
    let stored = &store["records"][&project_key];
    assert_eq!(store["schema_version"], 1);
    assert_eq!(stored["project_key"], project_key);
    assert_eq!(stored["trusted_hash"], reviewed_hash);
    assert_eq!(stored["trusted_at"], record.trusted_at);
    assert_eq!(
        stored["writer"],
        format!("symforge/{}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn unchanged_project_config_returns_trusted() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());
    let evaluation = trust.evaluate(temp.path());
    trust.record_trust(&evaluation).unwrap();

    let trusted = trust.evaluate(temp.path());

    assert!(matches!(trusted.status, TrustStatus::Trusted));
    assert_eq!(trusted.actual_hash, evaluation.actual_hash);
    assert!(trusted.warnings.is_empty());
}

#[test]
fn changed_project_config_reports_expected_and_actual_hashes() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());
    let evaluation = trust.evaluate(temp.path());
    trust.record_trust(&evaluation).unwrap();

    write_project_config(temp.path(), "rank_by = \"frecency\"\n");
    let changed = trust.evaluate(temp.path());

    match changed.status {
        TrustStatus::ContentChanged { expected, actual } => {
            assert_eq!(expected, evaluation.actual_hash);
            assert_eq!(actual, changed.actual_hash);
            assert_ne!(expected, actual);
        }
        other => panic!("expected content changed, got {other:?}"),
    }
}

#[test]
fn cli_project_config_status_accept_and_revoke_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let data_home = temp.path().join("data-home");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_project_config(&project, "rank_by = \"path\"\n");

    let status = symforge_command(&data_home)
        .arg("trust")
        .arg("project-config")
        .arg("status")
        .arg("--project")
        .arg(&project)
        .output()
        .expect("status command should spawn");
    assert_command_success(&status);
    let status_stdout = command_stdout(&status);
    assert!(status_stdout.contains("status: Untrusted"));
    let actual_hash = extract_actual_hash(&status_stdout);
    assert_sha256_hex(&actual_hash);

    let accept = symforge_command(&data_home)
        .arg("trust")
        .arg("project-config")
        .arg("accept")
        .arg("--project")
        .arg(&project)
        .arg("--hash")
        .arg(&actual_hash)
        .output()
        .expect("accept command should spawn");
    assert_command_success(&accept);
    let accept_stdout = command_stdout(&accept);
    assert!(accept_stdout.contains("ProjectConfigTrustAccepted"));
    assert!(accept_stdout.contains(&format!("trusted_hash: {actual_hash}")));

    let trusted = symforge_command(&data_home)
        .arg("trust")
        .arg("project-config")
        .arg("status")
        .arg("--project")
        .arg(&project)
        .output()
        .expect("trusted status command should spawn");
    assert_command_success(&trusted);
    assert!(command_stdout(&trusted).contains("status: Trusted"));

    let revoke = symforge_command(&data_home)
        .arg("trust")
        .arg("project-config")
        .arg("revoke")
        .arg("--project")
        .arg(&project)
        .output()
        .expect("revoke command should spawn");
    assert_command_success(&revoke);
    let revoke_stdout = command_stdout(&revoke);
    assert!(revoke_stdout.contains("ProjectConfigTrustRevoked"));
    assert!(revoke_stdout.contains("removed: true"));

    let untrusted = symforge_command(&data_home)
        .arg("trust")
        .arg("project-config")
        .arg("status")
        .arg("--project")
        .arg(&project)
        .output()
        .expect("post-revoke status command should spawn");
    assert_command_success(&untrusted);
    assert!(command_stdout(&untrusted).contains("status: Untrusted"));
}

#[allow(clippy::await_holding_lock)] // ENV_LOCK intentionally serializes process env across the async edit.
#[tokio::test]
async fn log_only_edit_response_warns_and_allows_untrusted_project_config() {
    let (_guard, mut env_guards) = clean_trust_env();
    let temp = tempfile::tempdir().unwrap();
    env_guards.extend(isolated_data_home(&temp.path().join("data-home")));
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_project_config(&project, "rank_by = \"path\"\n");
    write_source(
        &project,
        "src/lib.rs",
        "pub fn target() -> i32 {\n    1\n}\n",
    );
    let server = edit_test_server(&project);

    let result = edit_target_value(&server, "1", "2").await;

    assert!(
        result.contains("ProjectConfigTrustWarning: status=Untrusted"),
        "result was:\n{result}"
    );
    assert!(result.contains("mode=LOG_ONLY"));
    assert!(result.contains("actual_hash="));
    assert_eq!(
        std::fs::read_to_string(project.join("src/lib.rs")).unwrap(),
        "pub fn target() -> i32 {\n    2\n}\n"
    );
}

#[allow(clippy::await_holding_lock)] // ENV_LOCK intentionally serializes process env across the async edit.
#[tokio::test]
async fn enforce_mode_blocks_untrusted_project_config_before_editing() {
    let (_guard, mut env_guards) = clean_trust_env();
    let temp = tempfile::tempdir().unwrap();
    env_guards.extend(isolated_data_home(&temp.path().join("data-home")));
    env_guards.push(EnvGuard::set(TRUST_MODE_ENV, "enforce"));
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_project_config(&project, "rank_by = \"path\"\n");
    write_source(
        &project,
        "src/lib.rs",
        "pub fn target() -> i32 {\n    1\n}\n",
    );
    let server = edit_test_server(&project);

    let result = edit_target_value(&server, "1", "2").await;

    assert!(
        result.contains("ProjectConfigTrustEnforced: status=Untrusted"),
        "result was:\n{result}"
    );
    assert!(result.contains("actual_hash="));
    assert_eq!(
        std::fs::read_to_string(project.join("src/lib.rs")).unwrap(),
        "pub fn target() -> i32 {\n    1\n}\n"
    );
}

#[allow(clippy::await_holding_lock)] // ENV_LOCK intentionally serializes process env across both async edits.
#[tokio::test]
async fn trust_mode_changes_are_observed_at_call_time() {
    let (_guard, mut env_guards) = clean_trust_env();
    let temp = tempfile::tempdir().unwrap();
    env_guards.extend(isolated_data_home(&temp.path().join("data-home")));
    env_guards.push(EnvGuard::set(TRUST_MODE_ENV, "enforce"));
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_project_config(&project, "rank_by = \"path\"\n");
    write_source(
        &project,
        "src/lib.rs",
        "pub fn target() -> i32 {\n    1\n}\n",
    );
    let server = edit_test_server(&project);

    let blocked = edit_target_value(&server, "1", "2").await;
    assert!(
        blocked.contains("ProjectConfigTrustEnforced"),
        "blocked result was:\n{blocked}"
    );
    #[allow(unsafe_code)] // serialized by ENV_LOCK and restored by EnvGuard.
    unsafe {
        std::env::set_var(TRUST_MODE_ENV, "log_only");
    }

    let allowed = edit_target_value(&server, "1", "2").await;

    assert!(
        allowed.contains("ProjectConfigTrustWarning: status=Untrusted"),
        "allowed result was:\n{allowed}"
    );
    assert!(allowed.contains("mode=LOG_ONLY"));
    assert_eq!(
        std::fs::read_to_string(project.join("src/lib.rs")).unwrap(),
        "pub fn target() -> i32 {\n    2\n}\n"
    );
}

#[allow(clippy::await_holding_lock)] // ENV_LOCK intentionally serializes process env across the async edit.
#[tokio::test]
async fn edit_response_without_project_config_preserves_no_trust_warning() {
    let (_guard, mut env_guards) = clean_trust_env();
    let temp = tempfile::tempdir().unwrap();
    env_guards.extend(isolated_data_home(&temp.path().join("data-home")));
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_source(
        &project,
        "src/lib.rs",
        "pub fn target() -> i32 {\n    1\n}\n",
    );
    let server = edit_test_server(&project);

    let result = edit_target_value(&server, "1", "2").await;

    assert!(
        !result.contains("ProjectConfigTrust"),
        "result was:\n{result}"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("src/lib.rs")).unwrap(),
        "pub fn target() -> i32 {\n    2\n}\n"
    );
}

#[test]
fn corrupt_store_returns_untrusted_warning_without_panic() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());
    std::fs::create_dir_all(trust.store_path().parent().unwrap()).unwrap();
    std::fs::write(trust.store_path(), "{not valid json").unwrap();

    let evaluation = trust.evaluate(temp.path());

    assert!(matches!(evaluation.status, TrustStatus::Untrusted));
    assert_sha256_hex(&evaluation.actual_hash);
    assert!(
        evaluation
            .warnings
            .iter()
            .any(|warning| warning.contains("corrupt") && warning.contains("untrusted")),
        "warnings were {:?}",
        evaluation.warnings
    );
}

#[test]
fn unsupported_config_path_warning_cannot_become_trusted() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".symforge").join("config.toml")).unwrap();
    let trust = trust_for(temp.path());
    let evaluation = trust.evaluate(temp.path());
    trust.record_trust(&evaluation).unwrap();

    let reevaluated = trust.evaluate(temp.path());

    assert!(matches!(reevaluated.status, TrustStatus::Untrusted));
    assert!(
        reevaluated
            .warnings
            .iter()
            .any(|warning| warning.contains("non-file config path")),
        "warnings were {:?}",
        reevaluated.warnings
    );
}

#[test]
fn non_ci_env_override_is_ignored() {
    let (_guard, mut env_guards) = clean_trust_env();
    env_guards.push(EnvGuard::set(TRUST_ENV_OVERRIDE, "1"));
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");

    let evaluation = trust_for(temp.path()).evaluate(temp.path());

    assert!(matches!(evaluation.status, TrustStatus::Untrusted));
    assert!(
        evaluation
            .warnings
            .iter()
            .any(|warning| warning.contains("ignored") && warning.contains("not recognized as CI")),
        "warnings were {:?}",
        evaluation.warnings
    );
}

#[test]
fn ci_env_override_is_honored_without_writing_store() {
    let (_guard, mut env_guards) = clean_trust_env();
    env_guards.push(EnvGuard::set(TRUST_ENV_OVERRIDE, "1"));
    env_guards.push(EnvGuard::set("CI", "true"));
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());

    let evaluation = trust.evaluate(temp.path());

    assert!(matches!(evaluation.status, TrustStatus::EnvOverride));
    assert_sha256_hex(&evaluation.actual_hash);
    assert!(!trust.store_path().exists());
}

#[test]
fn project_config_hash_ignores_volatile_symforge_runtime_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let trust = trust_for(temp.path());
    let before = trust.evaluate(temp.path()).actual_hash;

    std::fs::write(temp.path().join(".symforge").join("index.bin"), b"runtime").unwrap();
    std::fs::create_dir_all(temp.path().join(".symforge").join("tee")).unwrap();
    std::fs::write(
        temp.path().join(".symforge").join("tee").join("snapshot"),
        b"runtime",
    )
    .unwrap();

    let after_runtime_change = trust.evaluate(temp.path()).actual_hash;
    assert_eq!(after_runtime_change, before);

    std::fs::create_dir_all(temp.path().join(".symforge").join("config")).unwrap();
    std::fs::write(
        temp.path()
            .join(".symforge")
            .join("config")
            .join("extra.toml"),
        b"enabled = true\n",
    )
    .unwrap();
    let after_config_change = trust.evaluate(temp.path()).actual_hash;
    assert_ne!(after_config_change, before);
}

#[test]
fn project_config_hash_is_stable_across_creation_order() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    for root in [left.path(), right.path()] {
        std::fs::create_dir_all(root.join(".symforge").join("config")).unwrap();
    }

    std::fs::write(
        left.path().join(".symforge").join("config.toml"),
        b"root = true\n",
    )
    .unwrap();
    std::fs::write(
        left.path().join(".symforge").join("config").join("a.toml"),
        b"a = 1\n",
    )
    .unwrap();
    std::fs::write(
        left.path().join(".symforge").join("config").join("b.toml"),
        b"b = 2\n",
    )
    .unwrap();

    std::fs::write(
        right.path().join(".symforge").join("config").join("b.toml"),
        b"b = 2\n",
    )
    .unwrap();
    std::fs::write(
        right.path().join(".symforge").join("config").join("a.toml"),
        b"a = 1\n",
    )
    .unwrap();
    std::fs::write(
        right.path().join(".symforge").join("config.toml"),
        b"root = true\n",
    )
    .unwrap();

    let left_hash = trust_for(left.path()).evaluate(left.path()).actual_hash;
    let right_hash = trust_for(right.path()).evaluate(right.path()).actual_hash;

    assert_eq!(left_hash, right_hash);
}

#[test]
fn default_store_path_uses_user_local_symforge_trust_json() {
    let path = default_trust_store_path().expect("data_local_dir should exist on test host");

    assert_eq!(path.file_name().unwrap(), "trust.json");
    assert_eq!(path.parent().unwrap().file_name().unwrap(), "symforge");
}

#[cfg(windows)]
#[test]
fn windows_canonical_project_key_strips_verbatim_prefix() {
    let temp = tempfile::tempdir().unwrap();
    write_project_config(temp.path(), "rank_by = \"path\"\n");
    let verbatim = Path::new(r"\\?\").join(temp.path());

    let evaluation = trust_for(temp.path()).evaluate(&verbatim);
    let key = evaluation.project_key.expect("project should canonicalize");

    assert!(
        !key.starts_with(r"\\?\"),
        "dunce normalization should strip verbatim prefix from {key}"
    );
}
