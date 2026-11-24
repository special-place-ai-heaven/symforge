//! Feature 037 Slice 3: MCP `SYMFORGE_SURFACE=code`, `serve --surface code`,
//! `init --code-skill`, byte budget <= 4800 B, dispatch gating, and e2e chains.

#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::daemon::{connect_existing_session_at, spawn_daemon_at};
use symforge::domain::ControlStateDir;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::surface_probe::{
    CODE_SURFACE_INSTRUCTIONS, SERVER_INSTRUCTIONS_MAX_BYTES, SurfaceProfile, advertised_tools,
    enforce_compact_surface, list_tools_schema_bytes, server_instructions,
    surface_profile_from_env,
};
use symforge::stel::{CODE_SURFACE_TOOL_COUNT, CODE_TOOL_NAMES, code_surface_list_schema_bytes};
use symforge::watcher::WatcherInfo;

use tempfile::TempDir;

fn build_server_fixture() -> (TempDir, SymForgeServer) {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    std::fs::write(
        root.join("src/math.rs"),
        r#"pub fn add_pair(a: i32, b: i32) -> i32 {
    a + b
}

pub fn sum_triple(x: i32, y: i32, z: i32) -> i32 {
    add_pair(add_pair(x, y), z)
}
"#,
    )
    .expect("write src/math.rs");

    let canonical = root.canonicalize().expect("canon");
    let shared = LiveIndex::load(&canonical).expect("load index");
    let server = SymForgeServer::new(
        shared,
        "code-surface-test".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(canonical),
        None,
    );
    (dir, server)
}

fn first_text_block(res: &rmcp::model::CallToolResult) -> &str {
    res.content
        .iter()
        .find_map(|c| match c {
            rmcp::model::ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("expected text content block")
}

#[tokio::test]
async fn code_surface_advertises_four_tools_under_4800_bytes_and_gates_legacy_tools() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("code");

    assert_eq!(surface_profile_from_env(), SurfaceProfile::Code);

    let tools = advertised_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(names.as_slice(), CODE_TOOL_NAMES);
    assert_eq!(tools.len(), CODE_SURFACE_TOOL_COUNT);

    let (count, bytes) = code_surface_list_schema_bytes();
    let (probe_count, probe_bytes) = list_tools_schema_bytes(SurfaceProfile::Code);
    assert_eq!(count, 4);
    assert_eq!(probe_count, 4);
    assert_eq!(bytes, probe_bytes);
    assert!(
        bytes <= 4_800,
        "code surface tools/list JSON (with outputSchema) must be <= 4800 B (~1200 tokens); got {bytes} B"
    );
    // Instructions budget check
    let instr = server_instructions(SurfaceProfile::Code).expect("code instructions");
    assert_eq!(instr, CODE_SURFACE_INSTRUCTIONS);
    assert!(
        instr.len() <= SERVER_INSTRUCTIONS_MAX_BYTES,
        "CODE_SURFACE_INSTRUCTIONS must fit <= {} B, got {} B",
        SERVER_INSTRUCTIONS_MAX_BYTES,
        instr.len()
    );

    // Dispatch gate: advertised 4 pass, unlisted legacy tools and compact `symforge` fail.
    for allowed in CODE_TOOL_NAMES {
        assert!(
            enforce_compact_surface(allowed).is_ok(),
            "`{allowed}` must be callable on code surface"
        );
    }
    for blocked in ["search_text", "get_symbol", "index_folder", "symforge"] {
        let err = enforce_compact_surface(blocked)
            .expect_err("unlisted tool must be rejected on code surface");
        assert!(
            err.message.contains("code surface") && err.message.contains("code_execute"),
            "rejection must guide caller to code_execute; got: {}",
            err.message
        );
    }

    // Also verify Full surface never advertises code_search or code_execute.
    drop(_surface);
    let _full = stel_surface_env::set_symforge_surface("full");
    let full_names: BTreeSet<String> = advertised_tools()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    assert!(!full_names.contains("code_search"));
    assert!(!full_names.contains("code_execute"));
    assert_eq!(full_names.len(), 40);
}

#[tokio::test]
async fn code_search_and_code_execute_and_symforge_edit_and_status_chain_e2e() {
    let (_dir, server) = build_server_fixture();

    // 1. `code_search` with empty query -> 1-line index
    let index_res = server
        .dispatch_tool_result_for_tests("code_search", json!({}))
        .await
        .expect("code_search index");
    assert_ne!(index_res.is_error, Some(true));
    let index_text = first_text_block(&index_res);
    assert!(index_text.contains("def search_symbols(*,"));
    assert!(index_text.contains("def find_references(*,"));

    // 2. `code_search(query="references")` -> full TypedDict + function stub
    let stub_res = server
        .dispatch_tool_result_for_tests("code_search", json!({"query": "references", "limit": 2}))
        .await
        .expect("code_search references");
    let stub_text = first_text_block(&stub_res);
    assert!(stub_text.contains("class RefHit(TypedDict):"));

    // 3. `code_execute` chaining search_symbols -> find_references -> get_symbol
    let exec_res = server
        .dispatch_tool_result_for_tests(
            "code_execute",
            json!({
                "code": r#"
defs = search_symbols(query="add_pair", kind="fn")
refs = find_references(name="add_pair", kind="call")
caller_targets = [{"path": r["path"], "name": r["enclosing_symbol"]} for r in refs if r["enclosing_symbol"]]
bodies = get_symbol(targets=caller_targets)
{"def_line": defs[0]["start_line"], "caller": bodies[0]["name"], "call_sites": len(refs)}
"#,
                "timeout_ms": 5000
            }),
        )
        .await
        .expect("code_execute");
    assert_ne!(exec_res.is_error, Some(true));
    let parsed: Value = serde_json::from_str(first_text_block(&exec_res)).expect("json result");
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["calls"], 3);
    assert_eq!(
        parsed["value"],
        json!({
            "def_line": 1,
            "caller": "sum_triple",
            "call_sites": 2,
        })
    );

    // 4. `symforge_edit` preview works cleanly alongside Code Mode
    let edit_res = server
        .dispatch_tool_result_for_tests(
            "symforge_edit",
            json!({
                "path": "src/math.rs",
                "symbol": "add_pair",
                "op": "replace",
                "body": "pub fn add_pair(a: i32, b: i32) -> i32 {\n    a.saturating_add(b)\n}",
                "apply": false
            }),
        )
        .await
        .expect("symforge_edit preview");
    assert_ne!(edit_res.is_error, Some(true));
    assert!(first_text_block(&edit_res).contains("saturating_add"));

    // 5. `status` reports surface=code when SYMFORGE_SURFACE=code
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("code");
    let status_res = server
        .dispatch_tool_result_for_tests("status", json!({}))
        .await
        .expect("status");
    assert!(
        first_text_block(&status_res).contains("surface: code"),
        "status must report surface: code, got: {}",
        first_text_block(&status_res)
    );
}

#[test]
fn serve_surface_flag_and_init_code_skill_install_cleanly() {
    use clap::Parser;

    let parsed = symforge::cli::Cli::try_parse_from(["symforge", "serve", "--surface", "code"])
        .expect("parse serve --surface code");
    match parsed.command {
        Some(symforge::cli::Commands::Serve(args)) => {
            assert_eq!(
                args.surface.map(|s| s.into_profile()),
                Some(SurfaceProfile::Code)
            );
        }
        other => panic!("expected Serve command, got {other:?}"),
    }

    // Verify `install_code_skill_for_client` writes SKILL.md for Claude, Codex, and OMP
    let fake_home = TempDir::new().expect("fake home");
    std::fs::create_dir_all(fake_home.path().join(".claude")).unwrap();
    std::fs::create_dir_all(fake_home.path().join(".codex")).unwrap();
    std::fs::create_dir_all(fake_home.path().join(".omp")).unwrap();

    let written = symforge::cli::init::install_code_skill_for_client(
        symforge::cli::InitClient::All,
        fake_home.path(),
        Path::new("/usr/local/bin/symforge"),
    )
    .expect("install code skill");

    assert_eq!(written.len(), 3);
    for path in &written {
        assert!(path.is_file(), "expected skill file at {}", path.display());
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("name: symforge-code"));
        assert!(content.contains("symforge\" code run"));
        assert!(content.contains("symforge\" q search_symbols -a query=target_fn"));
    }
}

#[tokio::test]
async fn daemon_proxied_code_execute_preserves_session_across_multiple_calls_and_status() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    std::fs::write(
        root.join("src/math.rs"),
        "pub fn add_pair(a: i32, b: i32) -> i32 { a + b }\npub fn triple(x: i32) -> i32 { add_pair(x, x) + x }\n",
    )
    .expect("write src/math.rs");

    let daemon_home = TempDir::new().expect("daemon home");
    let control_state = ControlStateDir::new(daemon_home.path().to_path_buf());
    let loopback = std::net::Ipv4Addr::LOCALHOST.to_string();
    let daemon = spawn_daemon_at(&loopback, &control_state)
        .await
        .expect("spawn isolated daemon");

    let session = connect_existing_session_at(
        root,
        "mcp-code-multi-call-test",
        Some(std::process::id()),
        &control_state,
    )
    .await
    .expect("connect daemon session")
    .expect("daemon session available");

    let server = SymForgeServer::new_daemon_proxy(session.clone());

    for round in 1..=3 {
        let res = symforge::protocol::result_status::with_project_evidence_scope(
            None,
            server.dispatch_tool_result_for_tests(
                "code_execute",
                json!({
                    "code": format!(
                        "hits = search_symbols(query=\"add_pair\", kind=\"fn\")\n{{\"round\": {round}, \"name\": hits[0][\"name\"]}}"
                    )
                }),
            ),
        )
        .await
        .expect("code_execute over daemon");
        assert_ne!(
            res.is_error,
            Some(true),
            "round {round} failed: {}",
            first_text_block(&res)
        );
        let parsed: Value =
            serde_json::from_str(first_text_block(&res)).expect("parse code_execute json");
        assert_eq!(parsed["status"], "ok", "round {round} outcome: {parsed}");
        assert_eq!(parsed["value"]["round"], round);
        assert_eq!(parsed["value"]["name"], "add_pair");
        // Verify daemon ProjectEvidence receipt was attached to `_meta` (never degraded/unavailable).
        let meta = res.meta.as_ref().expect("result _meta present");
        let ev = meta
            .0
            .get("symforge/project_evidence")
            .expect("symforge/project_evidence attached");
        assert!(
            ev.get("project_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.starts_with("project-v1-")),
            "expected verified daemon project_id receipt on _meta, got: {ev}"
        );
    }

    // Verify the underlying daemon session is still alive and healthy (not degraded or closed).
    let status_res = server
        .dispatch_tool_result_for_tests("status", json!({}))
        .await
        .expect("status after 3x code_execute");
    let status_text = first_text_block(&status_res);
    assert!(
        !status_text.contains("daemon_degraded_local_fallback"),
        "daemon session should remain healthy and open across code_execute calls, got:\n{status_text}"
    );

    // Explicit direct query on the original `session` proves `DELETE /v1/sessions/{id}` was never called.
    let (probe_res, probe_ev) = session
        .call_code_query(&symforge::codemode::DaemonCodeQueryRequest {
            op: symforge::codemode::CodeReadOp::SearchSymbols,
            kwargs: serde_json::from_value(json!({"query": "triple"})).unwrap(),
            project: None,
        })
        .await
        .expect("session must still be open on daemon");
    assert!(
        probe_res.is_ok(),
        "direct code-query probe failed: {probe_res:?}"
    );
    assert!(
        probe_ev.is_some(),
        "daemon /code-query must emit PROJECT_EVIDENCE_HEADER receipt"
    );

    let _ = session.close().await;
    let _ = daemon.shutdown_tx.send(());
}
