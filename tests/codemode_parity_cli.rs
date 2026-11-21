//! Feature 037 Slice 2: Post-MCP CLI (`symforge code` + `symforge q`),
//! warm daemon `/code-query` parity vs read-only local fallback, secret-withholding
//! parity, and discovery non-frecency verification.

#![cfg(feature = "server")]

use std::path::Path;

use serde_json::{Value, json};
use symforge::codemode::{CodeModeRunner, DaemonCodeBackend, ExecuteStatus, LocalCodeBackend};
use symforge::daemon::{connect_existing_session_at, spawn_daemon_at};
use symforge::domain::ControlStateDir;
use symforge::process_util::hidden_command;
use tempfile::TempDir;

fn symforge_bin() -> &'static str {
    env!("CARGO_BIN_EXE_symforge")
}

fn build_fixture_repo() -> TempDir {
    let dir = TempDir::new().expect("fixture tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    std::fs::create_dir_all(root.join("docs")).expect("mkdir docs");

    std::fs::write(
        root.join("src/engine.rs"),
        r#"pub struct QueryConfig {
    pub limit: usize,
}

pub fn compute_rank(cfg: &QueryConfig, raw: usize) -> usize {
    raw.min(cfg.limit)
}
"#,
    )
    .expect("write engine.rs");

    std::fs::write(
        root.join("src/caller.rs"),
        r#"use crate::engine::{QueryConfig, compute_rank};

pub fn invoke_pipeline() -> usize {
    let cfg = QueryConfig { limit: 10 };
    compute_rank(&cfg, 25)
}
"#,
    )
    .expect("write caller.rs");

    // Secret-bearing credential file that SymForge's path/content admission must withhold.
    std::fs::write(
        root.join(".env"),
        format!(
            "{}={}-{}\n",
            ["OPENAI_API", "_KEY"].concat(),
            "sk",
            "proj-testfixturekey0123456789abcdef"
        ),
    )
    .expect("write .env");

    std::fs::write(
        root.join("docs/architecture.md"),
        "# Engine Architecture\n\n`compute_rank` caps raw scores using `QueryConfig`.\n",
    )
    .expect("write docs/architecture.md");

    dir
}

#[test]
fn cli_code_stubs_and_idioms_succeed_with_zero_stderr() {
    let idioms_out = hidden_command(symforge_bin())
        .args(["code", "idioms"])
        .output()
        .expect("run symforge code idioms");
    assert!(idioms_out.status.success());
    assert!(
        idioms_out.stderr.is_empty(),
        "stderr must be empty: {}",
        String::from_utf8_lossy(&idioms_out.stderr)
    );
    let idioms = String::from_utf8(idioms_out.stdout).expect("utf8");
    assert!(idioms.contains("def search_symbols(*,"));
    assert!(idioms.contains("symforge q <op>"));

    let stubs_out = hidden_command(symforge_bin())
        .args(["code", "stubs", "references"])
        .output()
        .expect("run symforge code stubs");
    assert!(stubs_out.status.success());
    assert!(stubs_out.stderr.is_empty());
    let stubs = String::from_utf8(stubs_out.stdout).expect("utf8");
    assert!(stubs.contains("class RefHit(TypedDict):"));
    assert!(stubs.contains("def find_references(*,"));
}

#[test]
fn cli_code_run_and_q_work_in_local_no_daemon_mode_without_spawning_daemon() {
    let repo = build_fixture_repo();
    let isolated_home = TempDir::new().expect("isolated home");

    // 1. Single-query fast path: `symforge q`
    let q_out = hidden_command(symforge_bin())
        .args([
            "q",
            "search_symbols",
            r#"{"query":"compute_rank","kind":"fn"}"#,
            "--root",
            repo.path().to_str().unwrap(),
        ])
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", isolated_home.path())
        .output()
        .expect("run symforge q");
    assert!(
        q_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&q_out.stderr)
    );
    assert!(
        q_out.stderr.is_empty(),
        "stdout must not be polluted by stderr logs: {}",
        String::from_utf8_lossy(&q_out.stderr)
    );
    let parsed_q: Value = serde_json::from_slice(&q_out.stdout).expect("valid json stdout");
    assert_eq!(parsed_q["status"], "ok");
    assert_eq!(parsed_q["op"], "search_symbols");
    assert_eq!(parsed_q["value"][0]["name"], "compute_rank");
    assert_eq!(parsed_q["value"][0]["path"], "src/engine.rs");

    // 2. Multi-step script: `symforge code run -c`
    let script = r#"
syms = search_symbols(query="compute_rank", kind="fn")
refs = find_references(name="compute_rank", kind="call")
callers = [{"path": r["path"], "name": r["enclosing_symbol"]} for r in refs if r["enclosing_symbol"]]
bodies = get_symbol(targets=callers)
{"sym_path": syms[0]["path"], "caller_fn": bodies[0]["name"], "has_call": "compute_rank" in bodies[0]["source"]}
"#;
    let run_out = hidden_command(symforge_bin())
        .args([
            "code",
            "run",
            "-c",
            script,
            "--root",
            repo.path().to_str().unwrap(),
        ])
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", isolated_home.path())
        .output()
        .expect("run symforge code run");
    assert!(run_out.status.success());
    assert!(run_out.stderr.is_empty());
    let parsed_run: Value = serde_json::from_slice(&run_out.stdout).expect("valid json");
    assert_eq!(parsed_run["status"], "ok");
    assert_eq!(parsed_run["calls"], 3);
    assert_eq!(
        parsed_run["value"],
        json!({
            "sym_path": "src/engine.rs",
            "caller_fn": "invoke_pipeline",
            "has_call": true,
        })
    );

    // Ensure no daemon port file was created in isolated SYMFORGE_HOME.
    let daemon_dir = isolated_home.path().join("daemon");
    assert!(
        !daemon_dir.exists(),
        "local fallback must never spawn a background daemon"
    );
}

#[test]
fn warm_daemon_and_local_backend_return_identical_json_and_withhold_secrets() {
    let repo = build_fixture_repo();
    let daemon_home = TempDir::new().expect("daemon home");
    let control_state = ControlStateDir::new(daemon_home.path().to_path_buf());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio rt");

    let (daemon_outcome, daemon_secret_outcome) = rt.block_on(async {
        let loopback = std::net::Ipv4Addr::LOCALHOST.to_string();
        let daemon = spawn_daemon_at(&loopback, &control_state)
            .await
            .expect("spawn isolated daemon");

        let session = connect_existing_session_at(
            repo.path(),
            "parity-test",
            Some(std::process::id()),
            &control_state,
        )
        .await
        .expect("connect existing")
        .expect("daemon should be found");

        let handle = tokio::runtime::Handle::current();
        let out = tokio::task::spawn_blocking({
            let handle = handle.clone();
            let session = session.clone();
            move || {
                let backend = DaemonCodeBackend::new_ephemeral(handle, session, None);
                let runner = CodeModeRunner::default();
                let chain = runner.execute(
                    r#"
s = search_symbols(query="compute_rank")
ctx = file_context(path="src/engine.rs")
txt = search_text(query="QueryConfig")
{"sym": s[0]["name"], "outline_has_fn": "compute_rank" in ctx["rendered"], "txt_hits": len(txt)}
"#,
                    &backend,
                );
                let withheld_out = runner.execute(r#"file_content(path=".env")"#, &backend);
                backend.close_best_effort();
                (chain, withheld_out)
            }
        })
        .await
        .expect("join blocking");

        let _ = daemon.shutdown_tx.send(());
        out
    });

    let local_backend = LocalCodeBackend::open_read_only(repo.path()).expect("open local");
    let runner = CodeModeRunner::default();
    let local_outcome = runner.execute(
        r#"
s = search_symbols(query="compute_rank")
ctx = file_context(path="src/engine.rs")
txt = search_text(query="QueryConfig")
{"sym": s[0]["name"], "outline_has_fn": "compute_rank" in ctx["rendered"], "txt_hits": len(txt)}
"#,
        &local_backend,
    );
    let local_secret_outcome = runner.execute(r#"file_content(path=".env")"#, &local_backend);

    assert_eq!(
        daemon_outcome.status,
        ExecuteStatus::Ok,
        "daemon_outcome failed: {daemon_outcome:?}"
    );
    assert_eq!(
        local_outcome.status,
        ExecuteStatus::Ok,
        "local_outcome failed: {local_outcome:?}"
    );
    assert_eq!(
        daemon_outcome.value, local_outcome.value,
        "warm daemon and local snapshot backends must return identical JSON values"
    );

    // Secret-withholding parity: neither backend may disclose `.env` secret bytes.
    assert_eq!(daemon_secret_outcome.status, ExecuteStatus::Error);
    assert_eq!(local_secret_outcome.status, ExecuteStatus::Error);
    let daemon_err = daemon_secret_outcome.error.unwrap();
    let local_err = local_secret_outcome.error.unwrap();
    assert!(
        daemon_err.message.contains("policy_withheld")
            && !daemon_err
                .message
                .contains("testfixturekey0123456789abcdef"),
        "daemon leaked or missed policy refusal: {daemon_err:?}"
    );
    assert!(
        local_err.message.contains("policy_withheld")
            && !local_err.message.contains("testfixturekey0123456789abcdef"),
        "local leaked or missed policy refusal: {local_err:?}"
    );
}

#[test]
fn discovery_ops_do_not_bump_frecency_while_commitment_reads_do() {
    let repo = build_fixture_repo();
    let canonical = repo.path().canonicalize().expect("canon");
    let local_backend = LocalCodeBackend::open_read_only(&canonical).expect("open local");
    let runner = CodeModeRunner::default();

    let paths = [Path::new("src/engine.rs"), Path::new("src/caller.rs")];
    let before = symforge::live_index::frecency::ranking_scores_for_paths(
        &canonical,
        None,
        &paths,
        1_900_000_000,
    )
    .expect("frecency query");
    assert!(
        before.is_none() || before.unwrap().scores.is_empty(),
        "fresh fixture must start with zero frecency"
    );

    // Discovery-only script: search_symbols, search_text, search_files, repo_map
    let disc = runner.execute(
        r#"
_a = search_symbols(query="compute_rank")
_b = search_text(query="QueryConfig")
_c = search_files(query="engine")
_d = repo_map()
len(_a) + len(_b) + len(_c)
"#,
        &local_backend,
    );
    assert_eq!(disc.status, ExecuteStatus::Ok);

    let after_discovery = symforge::live_index::frecency::ranking_scores_for_paths(
        &canonical,
        None,
        &paths,
        1_900_000_000,
    )
    .expect("frecency after discovery");
    let disc_scores = after_discovery.map(|s| s.scores).unwrap_or_default();
    assert!(
        disc_scores.is_empty(),
        "discovery calls must NEVER bump frecency, got {disc_scores:?}"
    );

    // Commitment read: get_symbol on src/engine.rs
    let commit = runner.execute(
        r#"get_symbol(path="src/engine.rs", name="compute_rank")[0]["name"]"#,
        &local_backend,
    );
    assert_eq!(commit.status, ExecuteStatus::Ok);

    let after_commit = symforge::live_index::frecency::ranking_scores_for_paths(
        &canonical,
        None,
        &paths,
        1_900_000_000,
    )
    .expect("frecency after commitment");
    let commit_scores = after_commit.expect("session frecency recorded").scores;
    assert!(
        commit_scores.contains_key(Path::new("src/engine.rs")),
        "commitment read get_symbol must bump frecency for src/engine.rs, got {commit_scores:?}"
    );
}

#[test]
fn cli_q_supports_quote_free_key_value_args_for_windows_and_powershell() {
    let repo = build_fixture_repo();
    let isolated_home = TempDir::new().expect("isolated home");

    let q_out = hidden_command(symforge_bin())
        .args([
            "q",
            "search_symbols",
            "--arg",
            "query=compute_rank",
            "--arg",
            "kind=fn",
            "--arg",
            "limit=5",
            "--root",
            repo.path().to_str().unwrap(),
        ])
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", isolated_home.path())
        .output()
        .expect("run symforge q with --arg");
    assert!(
        q_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&q_out.stderr)
    );
    assert!(q_out.stderr.is_empty());
    let parsed: Value = serde_json::from_slice(&q_out.stdout).expect("valid json stdout");
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["op"], "search_symbols");
    assert_eq!(parsed["value"][0]["name"], "compute_rank");

    // Unknown argument via `--arg` is rejected cleanly in JSON output.
    let bad_out = hidden_command(symforge_bin())
        .args([
            "q",
            "search_symbols",
            "-a",
            "bogus_kwarg=123",
            "--root",
            repo.path().to_str().unwrap(),
        ])
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", isolated_home.path())
        .output()
        .expect("run symforge q with unknown --arg");
    assert!(
        !bad_out.status.success(),
        "failed symforge q must exit non-zero so shell callers never mask errors"
    );
    assert!(
        bad_out.stderr.is_empty(),
        "JSON error responses must keep stderr empty: {}",
        String::from_utf8_lossy(&bad_out.stderr)
    );
    let bad_parsed: Value = serde_json::from_slice(&bad_out.stdout).expect("valid error json");
    assert_eq!(bad_parsed["status"], "error");
    assert_eq!(bad_parsed["error"]["kind"], "invalid_request");
    assert!(
        bad_parsed["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("bogus_kwarg"),
        "got: {bad_parsed}"
    );

    // Failed `symforge code run` script also exits non-zero with pure JSON stdout and empty stderr.
    let fail_script_out = hidden_command(symforge_bin())
        .args([
            "code",
            "run",
            "-c",
            "1 / 0",
            "--root",
            repo.path().to_str().unwrap(),
        ])
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", isolated_home.path())
        .output()
        .expect("run failing symforge code run");
    assert!(
        !fail_script_out.status.success(),
        "failing symforge code run must exit non-zero"
    );
    assert!(fail_script_out.stderr.is_empty());
    let fail_parsed: Value =
        serde_json::from_slice(&fail_script_out.stdout).expect("valid error json");
    assert_eq!(fail_parsed["status"], "error");
    assert_eq!(fail_parsed["error"]["exception_type"], "ZeroDivisionError");
}

#[test]
fn init_skill_only_writes_skill_md_without_touching_claude_settings_or_mcp_configs() {
    let isolated_home = TempDir::new().expect("isolated home");
    let work_dir = TempDir::new().expect("work dir");

    let out = hidden_command(symforge_bin())
        .args(["init", "--client", "claude", "--code-skill", "--skill-only"])
        .current_dir(work_dir.path())
        .env("HOME", isolated_home.path())
        .env("USERPROFILE", isolated_home.path())
        .env("SYMFORGE_HOME", isolated_home.path().join(".symforge-home"))
        .output()
        .expect("run symforge init --skill-only");
    assert!(
        out.status.success(),
        "symforge init --skill-only failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let skill_path = isolated_home
        .path()
        .join(".claude")
        .join("skills")
        .join("symforge-code")
        .join("SKILL.md");
    assert!(
        skill_path.is_file(),
        "expected SKILL.md at {}",
        skill_path.display()
    );
    let content = std::fs::read_to_string(&skill_path).expect("read SKILL.md");
    assert!(content.contains("name: symforge-code"));
    assert!(content.contains("-a query=target_fn"));

    // Must NOT touch Claude hooks, MCP config, or CLAUDE.md memory!
    assert!(
        !isolated_home
            .path()
            .join(".claude")
            .join("settings.json")
            .exists(),
        "settings.json must not be created under --skill-only"
    );
    assert!(
        !isolated_home.path().join(".claude.json").exists(),
        ".claude.json must not be created under --skill-only"
    );
    assert!(
        !isolated_home
            .path()
            .join(".claude")
            .join("CLAUDE.md")
            .exists(),
        "CLAUDE.md must not be created under --skill-only"
    );
}

#[test]
fn symbol_context_callers_report_enclosing_caller_identity_and_top_level_module() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("pkg")).expect("mkdir pkg");
    std::fs::create_dir_all(root.join("tests")).expect("mkdir tests");

    std::fs::write(
        root.join("pkg/worker.py"),
        "def run_job(x: int) -> int:\n    return x + 1\n",
    )
    .expect("write worker.py");
    std::fs::write(
        root.join("pkg/app.py"),
        "from pkg.worker import run_job\n\ndef dispatch() -> int:\n    return run_job(5)\n\nBOOT = run_job(1)\n",
    )
    .expect("write app.py");
    std::fs::write(
        root.join("tests/test_worker.py"),
        "from pkg.worker import run_job\n\ndef test_run_job() -> None:\n    assert run_job(2) == 3\n",
    )
    .expect("write test_worker.py");

    let backend = LocalCodeBackend::open_read_only(root).expect("open local");
    let runner = CodeModeRunner::default();

    let prod_only = runner.execute(
        r#"symbol_context(name="run_job", path="pkg/worker.py", include_tests=False)"#,
        &backend,
    );
    assert_eq!(prod_only.status, ExecuteStatus::Ok, "{prod_only:?}");
    let val = prod_only.value.expect("value");
    assert_eq!(val["total_callers"], 2);
    let callers = val["callers"].as_array().expect("callers array");
    assert_eq!(callers.len(), 2);

    let fn_caller = callers
        .iter()
        .find(|c| c["name"] == "dispatch")
        .expect("dispatch caller");
    assert_eq!(fn_caller["caller_kind"], "fn");
    assert_eq!(fn_caller["caller_line"], 3);
    assert_eq!(fn_caller["call_line"], 4);
    assert_eq!(fn_caller["call_expr"], "run_job");
    assert_eq!(fn_caller["confidence"], "exact");

    let mod_caller = callers
        .iter()
        .find(|c| c["name"] == "<module>")
        .expect("top-level module caller");
    assert!(mod_caller["caller_kind"].is_null());
    assert!(mod_caller["caller_line"].is_null());
    assert_eq!(mod_caller["call_line"], 6);

    let with_tests = runner.execute(
        r#"symbol_context(name="run_job", path="pkg/worker.py", include_tests=True)"#,
        &backend,
    );
    assert_eq!(with_tests.value.expect("val")["total_callers"], 3);
}

#[test]
fn symbol_context_cross_file_callers_resolve_csharp_java_and_go_with_confidence() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("cs")).expect("mkdir cs");
    std::fs::create_dir_all(root.join("java/com/acme")).expect("mkdir java");
    std::fs::create_dir_all(root.join("go/pr")).expect("mkdir go");

    // 1. C# fixture: namespace + partial class + explicit receiver + unknown lambda receiver
    std::fs::write(
        root.join("cs/ResiliencePipeline.Async.cs"),
        "namespace Polly;\npublic partial class ResiliencePipeline {\n    public void ExecuteAsync() {}\n}\n",
    )
    .expect("write C# target");
    std::fs::write(
        root.join("cs/Bench.cs"),
        "namespace Polly.Benchmarks;\npublic class Bench {\n    private ResiliencePipeline _pipeline;\n    public void RunExplicit() { _pipeline.ExecuteAsync(); }\n    public void RunHeuristic() { Invoke(s => s.ExecuteAsync()); }\n}\n",
    )
    .expect("write C# caller");

    // 2. Java fixture: same package without explicit import
    std::fs::write(
        root.join("java/com/acme/Gson.java"),
        "package com.acme;\npublic class Gson {\n    public void getDelegateAdapter() {}\n}\n",
    )
    .expect("write Java target");
    std::fs::write(
        root.join("java/com/acme/Factory.java"),
        "package com.acme;\npublic class Factory {\n    public void create(Gson gson) { gson.getDelegateAdapter(); }\n}\n",
    )
    .expect("write Java caller");

    // 3. Go fixture: same package directory without import
    std::fs::write(
        root.join("go/pr/create.go"),
        "package pr\nfunc submitPR() error {\n    return nil\n}\n",
    )
    .expect("write Go target");
    std::fs::write(
        root.join("go/pr/run.go"),
        "package pr\nfunc createRun() error {\n    return submitPR()\n}\n",
    )
    .expect("write Go caller");

    let backend = LocalCodeBackend::open_read_only(root).expect("open local");
    let runner = CodeModeRunner::default();

    let cs = runner
        .execute(
            r#"symbol_context(name="ExecuteAsync", path="cs/ResiliencePipeline.Async.cs", symbol_line=3)"#,
            &backend,
        )
        .value
        .expect("cs value");
    let cs_callers = cs["callers"].as_array().expect("cs callers");
    assert_eq!(cs_callers.len(), 2, "got: {cs_callers:?}");
    assert!(
        cs_callers
            .iter()
            .any(|c| c["name"] == "RunExplicit" && c["confidence"] == "exact")
    );
    assert!(
        cs_callers
            .iter()
            .any(|c| c["name"] == "RunHeuristic" && c["confidence"] == "heuristic_receiver")
    );

    let jv = runner
        .execute(
            r#"symbol_context(name="getDelegateAdapter", path="java/com/acme/Gson.java")"#,
            &backend,
        )
        .value
        .expect("java value");
    let jv_callers = jv["callers"].as_array().expect("java callers");
    assert_eq!(jv_callers.len(), 1, "got: {jv_callers:?}");
    assert_eq!(jv_callers[0]["name"], "create");
    assert_eq!(jv_callers[0]["confidence"], "exact");

    let go = runner
        .execute(
            r#"symbol_context(name="submitPR", path="go/pr/create.go")"#,
            &backend,
        )
        .value
        .expect("go value");
    let go_callers = go["callers"].as_array().expect("go callers");
    assert_eq!(go_callers.len(), 1, "got: {go_callers:?}");
    assert_eq!(go_callers[0]["name"], "createRun");
    assert_eq!(go_callers[0]["confidence"], "exact");
}

#[test]
fn symbol_context_excludes_same_name_method_on_unrelated_type() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");

    std::fs::write(
        root.join("src/ResiliencePipeline.Async.cs"),
        "namespace Polly;\npublic partial class ResiliencePipeline {\n    public void ExecuteAsync() {}\n}\n",
    )
    .expect("write target");
    std::fs::write(
        root.join("src/AsyncPolicy.cs"),
        "namespace Polly;\npublic class AsyncPolicy {\n    public void ExecuteAsync() {}\n}\n",
    )
    .expect("write unrelated type");
    std::fs::write(
        root.join("src/MixedCallers.cs"),
        "namespace Polly.Benchmarks;\npublic class MixedCallers {\n    private AsyncPolicy _policy;\n    private ResiliencePipeline<string> _genericPipeline;\n    public void CallPolicy() { _policy.ExecuteAsync(); }\n    public void CallGeneric() { _genericPipeline.ExecuteAsync(); }\n    public void CallCast(object o) { ((AsyncPolicy)o).ExecuteAsync(); }\n}\n",
    )
    .expect("write unrelated callers");
    std::fs::write(
        root.join("src/UnrelatedLambda.cs"),
        "namespace Polly.Benchmarks;\npublic class UnrelatedLambda {\n    public void CallUnknown() { Run(s => s.ExecuteAsync()); }\n}\n",
    )
    .expect("write unknown lambda without target type mention");

    let backend = LocalCodeBackend::open_read_only(root).expect("open local");
    let runner = CodeModeRunner::default();
    let out = runner
        .execute(
            r#"symbol_context(name="ExecuteAsync", path="src/ResiliencePipeline.Async.cs", symbol_line=3)"#,
            &backend,
        )
        .value
        .expect("value");
    let callers = out["callers"].as_array().expect("callers");
    assert!(
        callers.is_empty(),
        "unrelated ExecuteAsync methods must not leak into callers: {callers:?}"
    );
}

#[test]
fn find_references_emits_enclosing_line_for_overloaded_method_chaining() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");

    std::fs::write(
        root.join("src/Adapter.java"),
        "package com.acme;\npublic class Adapter {\n    public static void targetHook() {}\n}\n",
    )
    .expect("write Adapter.java");
    std::fs::write(
        root.join("src/Factory.java"),
        "package com.acme;\npublic class Factory {\n    public void create(int a) {\n        return;\n    }\n    public void create(String b) {\n        Adapter.targetHook();\n    }\n}\n",
    )
    .expect("write overloaded Factory.java");

    let backend = LocalCodeBackend::open_read_only(root).expect("open local");
    let runner = CodeModeRunner::default();
    let out = runner.execute(
        r#"
refs = find_references(name="targetHook")
r0 = refs[0]
sym = get_symbol(path=r0["path"], name=r0["enclosing_symbol"], symbol_line=r0["enclosing_line"])[0]
{"enclosing_line": r0["enclosing_line"], "call_line": r0["line"], "source": sym["source"], "refusal": sym["refusal"]}
"#,
        &backend,
    );
    assert_eq!(out.status, ExecuteStatus::Ok, "{out:?}");
    let val = out.value.expect("value");
    assert_eq!(val["enclosing_line"], 6);
    assert_eq!(val["call_line"], 7);
    assert!(val["refusal"].is_null());
    assert!(val["source"].as_str().unwrap_or("").contains("String b"));
}

#[test]
fn search_text_group_by_count_returns_exact_per_file_totals() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    let repeated = (0..65)
        .map(|i| format!("pub fn f_{i}() {{ marker_tok(); }}\n"))
        .collect::<String>();
    std::fs::write(root.join("src/many.rs"), repeated).expect("write many.rs");

    let backend = LocalCodeBackend::open_read_only(root).expect("open local");
    let runner = CodeModeRunner::default();
    let out = runner.execute(
        r#"search_text(query="marker_tok", group_by="count")"#,
        &backend,
    );
    assert_eq!(out.status, ExecuteStatus::Ok, "{out:?}");
    let counts = out.value.expect("value");
    assert_eq!(counts[0]["path"], "src/many.rs");
    assert_eq!(counts[0]["count"], 65);
}
