//! Feature 037 Slice 1: Pure-Rust Monty sandbox, capability containment,
//! resource limits, value conversion, spill-on-overflow, and stub/idiom drift tests.

#![cfg(feature = "server")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use symforge::codemode::{
    CODE_READ_OPS, CodeBackendError, CodeModeLimits, CodeModeRunner, CodeReadOp, ExecuteRequest,
    ExecuteStatus, execute_script, render_idioms, render_stubs,
};

fn mock_backend(op: CodeReadOp, kwargs: Map<String, Value>) -> Result<Value, CodeBackendError> {
    match op {
        CodeReadOp::SearchSymbols => {
            let q = kwargs.get("query").and_then(Value::as_str).unwrap_or("");
            Ok(json!([
                {
                    "path": "src/protocol/surface_probe.rs",
                    "name": format!("{q}_fn"),
                    "kind": "fn",
                    "start_line": 42,
                    "end_line": 68,
                    "tier": "exact"
                }
            ]))
        }
        CodeReadOp::FindReferences => Ok(json!([
            {
                "path": "src/protocol/mod.rs",
                "line": 120,
                "text": "enforce_compact_surface(name)?;",
                "enclosing_symbol": "call_tool",
                "caller_kind": "fn"
            },
            {
                "path": "tests/surface_default.rs",
                "line": 15,
                "text": "enforce_compact_surface(\"search_text\")",
                "enclosing_symbol": "test_gate",
                "caller_kind": "fn"
            }
        ])),
        CodeReadOp::GetSymbol => {
            let count = kwargs
                .get("targets")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
            Ok(json!([
                {
                    "path": "src/protocol/mod.rs",
                    "name": "call_tool",
                    "kind": "fn",
                    "start_line": 100,
                    "end_line": 140,
                    "source": format!("fn call_tool() {{ /* batch={count} */ }}"),
                    "refusal": Value::Null,
                    "withheld_reason": Value::Null
                }
            ]))
        }
        _ => Ok(json!({"op": op.as_str(), "kwargs": kwargs})),
    }
}

#[test]
fn multi_step_python_chain_executes_and_returns_distilled_json() {
    let runner = CodeModeRunner::default();
    let code = r#"
hits = search_symbols(query="enforce_compact", kind="fn")
refs = find_references(name=hits[0]["name"], kind="call")
non_test = [{"path": r["path"], "name": r["enclosing_symbol"]} for r in refs if not r["path"].startswith("tests/")]
bodies = get_symbol(targets=non_test)
print("checked", len(refs), "refs")
{"hit": hits[0]["path"], "non_test_callers": len(non_test), "body": bodies[0]["source"]}
"#;
    let out = runner.execute(code, &mock_backend);
    assert_eq!(
        out.status,
        ExecuteStatus::Ok,
        "unexpected error: {:?}",
        out.error
    );
    assert_eq!(out.calls, 3);
    assert_eq!(out.stdout, "checked 2 refs\n");
    assert_eq!(
        out.value,
        Some(json!({
            "hit": "src/protocol/surface_probe.rs",
            "non_test_callers": 1,
            "body": "fn call_tool() { /* batch=1 */ }"
        }))
    );
}

#[test]
fn positional_arguments_raise_type_error_with_signature_hint() {
    let runner = CodeModeRunner::default();
    let out = runner.execute(r#"search_symbols("foo")"#, &mock_backend);
    assert_eq!(out.status, ExecuteStatus::Error);
    assert_eq!(
        out.calls, 0,
        "positional rejection happens before backend call"
    );
    let err = out.error.expect("error payload");
    assert_eq!(err.kind, "type_error");
    assert_eq!(err.exception_type, "TypeError");
    assert!(
        err.message.contains("keyword arguments only")
            && err.message.contains("def search_symbols(*,"),
        "got message: {}",
        err.message
    );
}

#[test]
fn filesystem_env_and_external_imports_are_blocked_by_zero_egress_sandbox() {
    let runner = CodeModeRunner::default();

    for (label, snippet, expected_exc) in [
        ("open", r#"open(".env").read()"#, "PermissionError"),
        (
            "os_listdir",
            r#"import os; os.listdir(".")"#,
            "PermissionError",
        ),
        (
            "pathlib_read",
            r#"import pathlib; pathlib.Path("Cargo.toml").read_text()"#,
            "PermissionError",
        ),
        ("urllib", r#"import urllib.request"#, "ModuleNotFoundError"),
        ("socket", r#"import socket"#, "ModuleNotFoundError"),
    ] {
        let out = runner.execute(snippet, &mock_backend);
        assert_eq!(out.status, ExecuteStatus::Error, "case {label} should fail");
        let err = out.error.expect("error info");
        assert_eq!(
            err.exception_type, expected_exc,
            "case {label} expected {expected_exc}, got {err:?}"
        );
    }
}

#[test]
fn sleep_resolves_immediately_under_zero_sleep_policy() {
    let runner = CodeModeRunner::default();
    let t0 = Instant::now();
    let out = runner.execute("import time\ntime.sleep(5)\n42", &mock_backend);
    let elapsed = t0.elapsed();
    assert_eq!(
        out.status,
        ExecuteStatus::Ok,
        "sleep should not fail: {:?}",
        out.error
    );
    assert_eq!(out.value, Some(json!(42)));
    assert!(
        elapsed < Duration::from_millis(500),
        "SleepMode::Zero should not sleep wall clock, took {elapsed:?}"
    );
}

#[test]
fn infinite_loop_is_preempted_by_feed_timeout() {
    let limits = CodeModeLimits {
        max_feed_duration: Duration::from_millis(50),
        ..CodeModeLimits::default()
    };
    let runner = CodeModeRunner::new(limits);
    let t0 = Instant::now();
    let out = runner.execute("while True:\n    pass\n", &mock_backend);
    let elapsed = t0.elapsed();
    assert_eq!(out.status, ExecuteStatus::Error);
    let err = out.error.expect("timeout error");
    assert_eq!(err.kind, "timeout");
    assert_eq!(err.exception_type, "TimeoutError");
    assert!(
        elapsed < Duration::from_secs(2),
        "took too long to preempt: {elapsed:?}"
    );
}

#[test]
fn large_allocation_is_rejected_before_oom() {
    let limits = CodeModeLimits {
        max_memory_bytes: 4 * 1024 * 1024,
        ..CodeModeLimits::default()
    };
    let runner = CodeModeRunner::new(limits);
    let out = runner.execute("'x' * 100_000_000", &mock_backend);
    assert_eq!(out.status, ExecuteStatus::Error);
    let err = out.error.expect("memory error");
    assert_eq!(err.kind, "memory_limit");
    assert_eq!(err.exception_type, "MemoryError");
}

#[test]
fn deep_recursion_is_capped_without_stack_overflow() {
    let limits = CodeModeLimits {
        max_recursion_depth: 40,
        ..CodeModeLimits::default()
    };
    let runner = CodeModeRunner::new(limits);
    let out = runner.execute("def f(n):\n    return f(n + 1)\nf(0)\n", &mock_backend);
    assert_eq!(out.status, ExecuteStatus::Error);
    let err = out.error.expect("recursion error");
    assert_eq!(err.kind, "recursion_limit");
    assert_eq!(err.exception_type, "RecursionError");
}

#[test]
fn host_call_budget_aborts_runaway_tool_loops() {
    let calls = AtomicUsize::new(0);
    let counting_backend = |_op: CodeReadOp, _kw: Map<String, Value>| {
        calls.fetch_add(1, Ordering::Relaxed);
        Ok(json!([]))
    };
    let limits = CodeModeLimits {
        max_host_calls: 5,
        ..CodeModeLimits::default()
    };
    let runner = CodeModeRunner::new(limits);
    // Even if script wraps in try/except, `call.abort` raises an uncatchable exception!
    let code = r#"
for i in range(50):
    try:
        search_symbols(query="x")
    except Exception:
        pass
"#;
    let out = runner.execute(code, &counting_backend);
    assert_eq!(out.status, ExecuteStatus::Error);
    assert_eq!(out.calls, 5);
    assert_eq!(calls.load(Ordering::Relaxed), 5);
    let err = out.error.expect("call limit error");
    assert_eq!(err.kind, "host_call_limit");
}

#[test]
fn oversized_output_spills_losslessly_to_disk_and_caps_inline_preview() {
    let temp = tempfile::tempdir().expect("tempdir");
    let req = ExecuteRequest {
        code: "{'big': 'é' * 4000}".to_owned(),
        limits: CodeModeLimits {
            max_output_chars: 512,
            ..CodeModeLimits::default()
        },
        spill_dir: Some(temp.path().to_path_buf()),
    };
    let out = execute_script(&req, &mock_backend);
    assert_eq!(out.status, ExecuteStatus::Ok);
    assert_eq!(out.truncated, Some(true));
    let spill = out.spill_path.expect("spill_path present");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&spill).expect("read spill"))
        .expect("parse spill");
    assert_eq!(
        saved["value"]["big"].as_str().map(|s| s.chars().count()),
        Some(4000)
    );
}

#[test]
fn stubs_and_idioms_stay_in_exact_lockstep_with_registered_ops() {
    assert_eq!(CODE_READ_OPS.len(), 15);
    let index = render_stubs(None, None);
    let idioms = render_idioms();

    for desc in CODE_READ_OPS {
        assert_eq!(CodeReadOp::from_name(desc.name), Some(desc.op));
        let sig = desc.op.signature_one_liner();
        assert!(
            index.contains(&sig),
            "empty-query stub index missing signature `{sig}`"
        );
        assert!(
            idioms.contains(&sig),
            "idioms guide missing signature `{sig}`"
        );

        let detailed = render_stubs(Some(desc.name), Some(3));
        assert!(
            detailed.contains("TypedDict") && detailed.contains(&sig),
            "detailed stub for `{}` missing TypedDict or signature:\n{detailed}",
            desc.name
        );
    }

    // Mutating tools must never appear in the sandbox catalog.
    for forbidden in [
        "symforge_edit",
        "replace_symbol_body",
        "edit_within_symbol",
        "insert_symbol",
        "delete_symbol",
        "batch_edit",
        "batch_insert",
        "batch_rename",
        "index_folder",
        "checkpoint_now",
        "curate_knowledge",
        "secret_remediate",
        "analyze_file_impact",
    ] {
        assert_eq!(
            CodeReadOp::from_name(forbidden),
            None,
            "mutating tool `{forbidden}` must not be representable in CodeReadOp"
        );
    }
}

#[test]
fn spill_ring_evicts_oldest_files_beyond_32_and_respects_allow_spill_files_false() {
    let temp = tempfile::tempdir().expect("tempdir");

    // 1. Remote/shared lane (`allow_spill_files = false`) truncates inline without writing disk files.
    let no_spill_req = ExecuteRequest {
        code: "{'big': 'z' * 2000}".to_owned(),
        limits: CodeModeLimits {
            max_output_chars: 256,
            allow_spill_files: false,
            ..CodeModeLimits::default()
        },
        spill_dir: Some(temp.path().to_path_buf()),
    };
    let no_spill_out = execute_script(&no_spill_req, &mock_backend);
    assert_eq!(no_spill_out.status, ExecuteStatus::Ok);
    assert_eq!(no_spill_out.truncated, Some(true));
    assert!(
        no_spill_out.spill_path.is_none(),
        "allow_spill_files=false must not emit a spill_path"
    );
    assert!(no_spill_out.total_bytes.unwrap_or(0) >= 2000);
    assert_eq!(
        std::fs::read_dir(temp.path()).unwrap().count(),
        0,
        "allow_spill_files=false must write zero files"
    );

    // 2. Local lane (`allow_spill_files = true`): writing 36 spills evicts the oldest so <= 32 remain.
    let mut recorded_paths = Vec::new();
    for idx in 0..36 {
        let req = ExecuteRequest {
            code: format!("{{'idx': {idx}, 'pad': 'x' * 2000}}"),
            limits: CodeModeLimits {
                max_output_chars: 256,
                allow_spill_files: true,
                ..CodeModeLimits::default()
            },
            spill_dir: Some(temp.path().to_path_buf()),
        };
        let out = execute_script(&req, &mock_backend);
        assert_eq!(out.status, ExecuteStatus::Ok);
        let p = out.spill_path.expect("spill_path created");
        assert!(
            p.contains("symforge-code-spill-") && p.ends_with(".json"),
            "unexpected spill filename: {p}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "spill file must be owner-only 0o600");
        }
        recorded_paths.push(p);
    }

    let remaining_in_dir = std::fs::read_dir(temp.path()).unwrap().count();
    assert!(
        remaining_in_dir <= 32,
        "spill ring buffer must cap retained spill files at <= 32, found {remaining_in_dir}"
    );
    assert!(
        !std::path::Path::new(&recorded_paths[0]).exists(),
        "oldest spill file should have been evicted"
    );
    assert!(
        std::path::Path::new(recorded_paths.last().unwrap()).exists(),
        "newest spill file must still exist"
    );
}

#[test]
fn local_backend_rejects_unknown_kwargs_and_invalid_enum_or_range_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn sample_target() -> usize { 7 }\n",
    )
    .unwrap();

    let backend =
        symforge::codemode::LocalCodeBackend::open_read_only(dir.path()).expect("open backend");
    let runner = CodeModeRunner::default();

    for (label, snippet, expected_substr) in [
        (
            "unknown_top_level_kwarg",
            r#"search_symbols(bogus=1)"#,
            "does not accept \"bogus\"",
        ),
        (
            "unknown_find_references_kwarg",
            r#"find_references(name="sample_target", file="src/lib.rs")"#,
            "does not accept \"file\"",
        ),
        (
            "invalid_repo_map_detail_enum",
            r#"repo_map(detail="bogus")"#,
            "Valid values: \"compact\", \"full\", \"tree\"",
        ),
        (
            "invalid_find_references_mode_enum",
            r#"find_references(name="sample_target", mode="bogus")"#,
            "Valid values: \"references\", \"implementations\"",
        ),
        (
            "invalid_file_context_section",
            r#"file_context(path="src/lib.rs", sections=["git"])"#,
            "\"outline\", \"imports\", \"consumers\"",
        ),
        (
            "zero_limit_range_refusal",
            r#"search_symbols(query="sample", limit=0)"#,
            "Valid range: 1",
        ),
    ] {
        let out = runner.execute(snippet, &backend);
        assert_eq!(
            out.status,
            ExecuteStatus::Error,
            "case `{label}` must be rejected, got {out:?}"
        );
        let err = out.error.expect("error info");
        assert!(
            err.message.contains("invalid_request") && err.message.contains(expected_substr),
            "case `{label}` expected `{expected_substr}` in `{}`",
            err.message
        );
    }
}
