/// Integration tests for the HTTP sidecar and hook infrastructure.
///
/// Proves HOOK-01 (sidecar binds ephemeral port, port file written, endpoints respond),
/// HOOK-02 (shared index mutation visible through sidecar),
/// HOOK-03 (hook round-trip under 100ms),
/// HOOK-10 (hook stdout is valid JSON for all paths including fail-open).
///
/// Note: Tests that change process cwd are run with `--test-threads=1` (the full integration
/// test suite is invoked with that flag) to avoid cwd races.  Within the file, all async
/// tests that mutate cwd acquire `CWD_LOCK` which is a `tokio::sync::Mutex` so it can be
/// held across await points on the multi-thread runtime.
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use symforge::{
    cli::HookSubcommand,
    cli::hook::{event_name_for, fail_open_json, run_hook, success_json},
    domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord},
    live_index::{IndexedFile, LiveIndex, ParseStatus, SharedIndex},
    sidecar::spawn_sidecar,
};
use tempfile::TempDir;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Serialize all cwd-manipulating tests.
// tokio::sync::Mutex is Send so it can be held across await points.
// ---------------------------------------------------------------------------
static CWD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a minimal `IndexedFile` for a Rust source file with one function symbol.
fn make_rust_file(path: &str, fn_name: &str) -> IndexedFile {
    let content = format!("fn {fn_name}() {{}}").into_bytes();
    IndexedFile {
        relative_path: path.to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path(path),
        content: content.clone(),
        symbols: vec![SymbolRecord {
            name: fn_name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: content.len() as u64,
        content_hash: "test".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    }
}

/// Build a `SharedIndex` using the public API (`LiveIndex::empty()` + `add_file`).
fn build_shared_index(files: Vec<IndexedFile>) -> SharedIndex {
    let shared = LiveIndex::empty();
    {
        let mut guard = shared.write();
        for file in files {
            let path = file.relative_path.clone();
            guard.add_file(path, file);
        }
    }
    shared
}

/// Make a synchronous raw HTTP GET request to `127.0.0.1:{port}{path}?{query}`.
/// Returns the response body or an error.
fn raw_http_get(port: u16, path: &str, query: &str) -> anyhow::Result<String> {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let timeout = Duration::from_millis(500);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let request_path = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };

    let request = format!(
        "GET {request_path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("")
        .to_string();
    Ok(body)
}

fn stable_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn restore_cwd(path: &Path) {
    if std::env::set_current_dir(path).is_err() {
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"))
            .expect("manifest dir must be a valid cwd fallback");
    }
}

// ---------------------------------------------------------------------------
// HOOK-01: Sidecar binds ephemeral port and writes port file
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_sidecar_binds_ephemeral_port() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/main.rs", "main")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    assert!(handle.port > 0, "port must be a valid non-zero value");

    let port_file = tmp.path().join(".symforge/sidecar.port");
    assert!(port_file.exists(), "sidecar.port file must exist");
    let content = std::fs::read_to_string(&port_file).unwrap();
    let file_port: u16 = content
        .trim()
        .parse()
        .expect("port file must contain a valid u16");
    assert_eq!(file_port, handle.port, "port file must match handle port");

    let pid_file = tmp.path().join(".symforge/sidecar.pid");
    assert!(pid_file.exists(), "sidecar.pid file must exist");

    // Send shutdown and await server-task completion (listener fully dropped).
    handle.shutdown_and_join().await;

    assert!(
        !port_file.exists(),
        "sidecar.port file must be cleaned up after shutdown"
    );
    assert!(
        !pid_file.exists(),
        "sidecar.pid file must be cleaned up after shutdown"
    );

    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-01: Health endpoint responds within 50ms
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_health_endpoint_responds() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![
        make_rust_file("src/main.rs", "main"),
        make_rust_file("src/lib.rs", "run"),
    ]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let start = Instant::now();
    let body = raw_http_get(handle.port, "/health", "").expect("GET /health must succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "health response latency must be <50ms, got {:?}",
        elapsed
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&body).expect("health response must be valid JSON");
    assert!(
        parsed.get("file_count").is_some(),
        "health response must contain 'file_count'"
    );
    assert!(
        parsed.get("symbol_count").is_some(),
        "health response must contain 'symbol_count'"
    );
    assert_eq!(parsed["file_count"], 2, "file_count must match index");

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-01: /outline endpoint returns symbols for a known file
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_outline_endpoint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/foo.rs", "hello")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/outline", "path=src/foo.rs")
        .expect("GET /outline must succeed");

    assert!(
        body.contains("src/foo.rs"),
        "outline should mention the requested file"
    );
    assert!(
        body.contains("hello"),
        "outline should include the symbol name"
    );
    assert!(
        body.contains("tokens saved"),
        "outline should include the token savings footer"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_workflow_source_read_endpoint_matches_outline() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/foo.rs", "hello")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let canonical = raw_http_get(handle.port, "/outline", "path=src/foo.rs")
        .expect("GET /outline must succeed");
    let workflow = raw_http_get(handle.port, "/workflows/source-read", "path=src/foo.rs")
        .expect("GET /workflows/source-read must succeed");

    assert_eq!(
        workflow, canonical,
        "workflow source-read adapter should stay identical to the canonical outline route"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-02: Shared index mutation visible through sidecar
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_shared_index_mutation() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/a.rs", "alpha")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Verify initial state via sidecar.
    let body = raw_http_get(handle.port, "/health", "").expect("GET /health must succeed");
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["file_count"], 1, "initially 1 file");

    // Add a new file through the shared Arc<LiveIndex>.
    {
        let mut guard = index.write();
        let new_file = make_rust_file("src/b.rs", "beta");
        guard.add_file("src/b.rs".to_string(), new_file);
    }

    // Sidecar should now report 2 files (same Arc).
    let body2 =
        raw_http_get(handle.port, "/health", "").expect("GET /health after mutation must succeed");
    let parsed2: serde_json::Value = serde_json::from_str(&body2).unwrap();
    assert_eq!(
        parsed2["file_count"], 2,
        "sidecar must see mutated index — file_count must be 2"
    );

    // Outline for the new file must also be visible.
    let outline = raw_http_get(handle.port, "/outline", "path=src/b.rs")
        .expect("GET /outline for new file must succeed");
    assert!(
        outline.contains("src/b.rs"),
        "outline should mention the new file"
    );
    assert!(
        outline.contains("beta"),
        "new file symbol must be visible through sidecar"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-03: Hook binary completes round-trip in under 100ms
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_hook_binary_latency() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/main.rs", "main")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Port file already written by spawn_sidecar.
    // SessionStart calls /repo-map — no file path env var needed.
    let start = Instant::now();
    // run_hook writes JSON to stdout — acceptable in test context.
    run_hook(Some(&HookSubcommand::SessionStart)).expect("run_hook must succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "hook round-trip must be <100ms, got {:?}",
        elapsed
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-10: Hook output is valid JSON for all subcommands (direct function test)
// ---------------------------------------------------------------------------

#[test]
fn test_hook_output_valid_json() {
    // Test the JSON-building functions directly (these are what run_hook outputs).
    let subcommands = [
        HookSubcommand::Read,
        HookSubcommand::Edit,
        HookSubcommand::Grep,
        HookSubcommand::SessionStart,
        HookSubcommand::PromptSubmit,
    ];

    for sub in &subcommands {
        let event_name = event_name_for(sub);

        // Test fail-open JSON.
        let fail_json = fail_open_json(event_name);
        let parsed: serde_json::Value = serde_json::from_str(&fail_json)
            .unwrap_or_else(|e| panic!("fail_open_json for {:?} must be valid JSON: {e}", sub));
        assert!(
            parsed["hookSpecificOutput"].get("hookEventName").is_some(),
            "hookEventName must be present in fail_open output for {:?}",
            sub
        );
        assert!(
            parsed["hookSpecificOutput"]
                .get("additionalContext")
                .is_some(),
            "additionalContext must be present in fail_open output for {:?}",
            sub
        );

        // Test success JSON.
        let success = success_json(event_name, "some context data");
        let parsed2: serde_json::Value = serde_json::from_str(&success)
            .unwrap_or_else(|e| panic!("success_json for {:?} must be valid JSON: {e}", sub));
        assert!(
            parsed2["hookSpecificOutput"].get("hookEventName").is_some(),
            "hookEventName must be present in success output for {:?}",
            sub
        );
        assert!(
            parsed2["hookSpecificOutput"]
                .get("additionalContext")
                .is_some(),
            "additionalContext must be present in success output for {:?}",
            sub
        );
        assert_eq!(
            parsed2["hookSpecificOutput"]["additionalContext"], "some context data",
            "additionalContext value must match for {:?}",
            sub
        );
    }
}

// ---------------------------------------------------------------------------
// HOOK-10: Hook fail-open path outputs valid JSON when no sidecar running
// ---------------------------------------------------------------------------

#[test]
fn test_hook_failopen_valid_json() {
    let tmp = TempDir::new().unwrap();
    // Sync tests use std::sync::Mutex for cwd lock.
    let _guard = CWD_LOCK.blocking_lock();
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    // No sidecar running — no port file — fail-open path.
    let subcommands = [
        HookSubcommand::Read,
        HookSubcommand::Edit,
        HookSubcommand::Grep,
        HookSubcommand::SessionStart,
        HookSubcommand::PromptSubmit,
    ];

    for sub in &subcommands {
        let event_name = event_name_for(sub);
        let json = fail_open_json(event_name);
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("fail-open JSON for {:?} must be valid: {e}", sub));

        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"], "",
            "fail-open additionalContext must be empty string for {:?}",
            sub
        );
    }

    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-01: /repo-map endpoint returns all indexed files
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_repo_map_endpoint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let files = vec![
        make_rust_file("src/a.rs", "alpha"),
        make_rust_file("src/b.rs", "beta"),
        make_rust_file("src/c.rs", "gamma"),
    ];
    let index = build_shared_index(files);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/repo-map", "").expect("GET /repo-map must succeed");

    assert!(
        body.contains("3 files"),
        "repo-map should summarize file count"
    );
    assert!(
        body.contains("3 symbols"),
        "repo-map should summarize symbol count"
    );
    assert!(
        body.contains("Rust: 3"),
        "repo-map should include language breakdown"
    );
    assert!(
        body.contains("src"),
        "repo-map should include the src directory bucket"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_workflow_repo_start_endpoint_matches_repo_map() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let files = vec![
        make_rust_file("src/a.rs", "alpha"),
        make_rust_file("src/b.rs", "beta"),
        make_rust_file("src/c.rs", "gamma"),
    ];
    let index = build_shared_index(files);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let canonical = raw_http_get(handle.port, "/repo-map", "").expect("GET /repo-map must succeed");
    let workflow = raw_http_get(handle.port, "/workflows/repo-start", "")
        .expect("GET /workflows/repo-start must succeed");

    assert_eq!(
        workflow, canonical,
        "workflow repo-start adapter should stay identical to the canonical repo-map route"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_prefers_file_hint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/foo.rs", "hello")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=please%20inspect%20src%2Ffoo.rs",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        body.contains("src/foo.rs"),
        "prompt context should mention the hinted file"
    );
    assert!(
        body.contains("hello"),
        "prompt context should include the hinted file symbol"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_workflow_prompt_context_endpoint_matches_prompt_context() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/foo.rs", "hello")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let query = "text=please%20inspect%20src%2Ffoo.rs";
    let canonical = raw_http_get(handle.port, "/prompt-context", query)
        .expect("GET /prompt-context must succeed");
    let workflow = raw_http_get(handle.port, "/workflows/prompt-context", query)
        .expect("GET /workflows/prompt-context must succeed");

    assert_eq!(
        workflow, canonical,
        "workflow prompt-context adapter should stay identical to the canonical prompt-context route"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_extensionless_path_line_hint_disambiguates_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let src_target = IndexedFile {
        relative_path: "src/db.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/db.rs"),
        content: b"fn connect() {}\nfn connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (16, 30),
                line_range: (2, 2),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 31,
        content_hash: "db".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let test_target = IndexedFile {
        relative_path: "tests/db.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("tests/db.py"),
        content: b"def connect():\n    pass\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "connect".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 13),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 24,
        content_hash: "db-py".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let src_dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/other.rs"),
        content: b"fn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 23,
        content_hash: "other".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (11, 17),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![src_target, test_target, src_dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src%2Fdb%3A2%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "extensionless path alias should disambiguate combined prompt routing: {body}"
    );
    assert!(
        body.contains("src/service.rs"),
        "extensionless path alias should still produce symbol context output: {body}"
    );
    assert!(
        !body.contains("src/other.rs"),
        "extensionless path alias should exclude unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_module_alias_line_hint_disambiguates_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let src_target = IndexedFile {
        relative_path: "src/db.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/db.rs"),
        content: b"fn connect() {}\nfn connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (16, 30),
                line_range: (2, 2),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 31,
        content_hash: "db".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let test_target = IndexedFile {
        relative_path: "tests/db.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("tests/db.py"),
        content: b"def connect():\n    pass\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "connect".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 13),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 24,
        content_hash: "db-py".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let src_dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/other.rs"),
        content: b"fn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 23,
        content_hash: "other".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (11, 17),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![src_target, test_target, src_dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20crate%3A%3Adb%3A2%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "module alias should disambiguate combined prompt routing: {body}"
    );
    assert!(
        body.contains("src/service.rs"),
        "module alias should still produce symbol context output: {body}"
    );
    assert!(
        !body.contains("src/other.rs"),
        "module alias should exclude unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_module_alias_without_line_prefers_exact_file_hint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let src_target = IndexedFile {
        relative_path: "src/db.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/db.rs"),
        content: b"fn connect() {}\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "connect".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 14),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 15,
        content_hash: "db".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let test_target = IndexedFile {
        relative_path: "tests/db.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("tests/db.py"),
        content: b"def connect():\n    pass\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "connect".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 13),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 24,
        content_hash: "db-py".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let src_dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/other.rs"),
        content: b"fn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 23,
        content_hash: "other".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (11, 17),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![src_target, test_target, src_dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20crate%3A%3Adb%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "module alias without line should still resolve the exact file hint: {body}"
    );
    assert!(
        body.contains("src/service.rs"),
        "module alias without line should still produce symbol context output: {body}"
    );
    assert!(
        !body.contains("src/other.rs"),
        "module alias without line should exclude unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_slash_module_alias_without_line_prefers_exact_file_hint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/utils/index.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/utils/index.ts"),
        content: b"export function connect() {}\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "connect".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 24),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 28,
        content_hash: "utils-ts".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/app.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/app.ts"),
        content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (36, 46),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 49,
        content_hash: "app-ts".to_string(),
        references: vec![
            ReferenceRecord {
                name: "utils".to_string(),
                qualified_name: Some("src/utils".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (24, 33),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("src/utils/connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (36, 42),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/other.ts"),
        content: b"connect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 9),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 10,
        content_hash: "other-ts".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (0, 6),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src/utils%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "slash module aliases without line should still resolve the exact file hint: {body}"
    );
    assert!(
        body.contains("src/app.ts"),
        "slash module aliases without line should still produce symbol context output: {body}"
    );
    assert!(
        !body.contains("src/other.ts"),
        "slash module aliases without line should exclude unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_slash_module_alias_line_hint_disambiguates_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/utils/index.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/utils/index.ts"),
        content: b"export function connect() {}\n\nexport function connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 24),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (28, 52),
                line_range: (3, 3),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 57,
        content_hash: "utils-ts-lines".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/app.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/app.ts"),
        content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (36, 46),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 49,
        content_hash: "app-ts".to_string(),
        references: vec![
            ReferenceRecord {
                name: "utils".to_string(),
                qualified_name: Some("src/utils".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (24, 33),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("src/utils/connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (36, 42),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/other.ts"),
        content: b"connect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 9),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 10,
        content_hash: "other-ts".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (0, 6),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src%2Futils%3A4%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "slash module aliases should allow direct line-hint disambiguation: {body}"
    );
    assert!(
        body.contains("src/app.ts"),
        "slash module aliases with line hints should keep exact-selector matches: {body}"
    );
    assert!(
        !body.contains("src/other.ts"),
        "slash module aliases with line hints should drop unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_qualified_symbol_alias_prefers_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = make_rust_file("src/db.rs", "connect");
    let dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/other.rs"),
        content: b"fn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 23,
        content_hash: "other".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (11, 17),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20crate%3A%3Adb%3A%3Aconnect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        body.contains("src/service.rs"),
        "qualified symbol aliases should keep exact-selector matches: {body}"
    );
    assert!(
        !body.contains("src/other.rs"),
        "qualified symbol aliases should drop unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_dotted_qualified_symbol_alias_prefers_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "pkg/db.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("pkg/db.py"),
        content: b"def connect():\n    pass\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "connect".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 13),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 24,
        content_hash: "db-py".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "pkg/service.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("pkg/service.py"),
        content: b"from pkg.db import connect\n\ndef run():\n    connect()\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (28, 38),
            line_range: (3, 3),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 54,
        content_hash: "service-py".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("pkg.db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (5, 11),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("pkg.db.connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (41, 47),
                line_range: (3, 3),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "pkg/other.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("pkg/other.py"),
        content: b"def run():\n    connect()\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 25,
        content_hash: "other-py".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (15, 21),
            line_range: (1, 1),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20pkg.db.connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        body.contains("pkg/service.py"),
        "dotted qualified symbol aliases should keep exact-selector matches: {body}"
    );
    assert!(
        !body.contains("pkg/other.py"),
        "dotted qualified symbol aliases should drop unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_slash_qualified_symbol_alias_prefers_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/utils/index.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/utils/index.ts"),
        content: b"export function connect() {}\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "connect".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 24),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 28,
        content_hash: "utils-ts".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/app.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/app.ts"),
        content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (36, 46),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 49,
        content_hash: "app-ts".to_string(),
        references: vec![
            ReferenceRecord {
                name: "utils".to_string(),
                qualified_name: Some("src/utils".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (24, 33),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("src/utils/connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (36, 42),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/other.ts"),
        content: b"connect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 9),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 10,
        content_hash: "other-ts".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (0, 6),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src/utils/connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        body.contains("src/app.ts"),
        "slash qualified symbol aliases should keep exact-selector matches: {body}"
    );
    assert!(
        !body.contains("src/other.ts"),
        "slash qualified symbol aliases should drop unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_slash_qualified_symbol_alias_line_hint_disambiguates_exact_selector()
 {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/utils/index.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/utils/index.ts"),
        content: b"export function connect() {}\n\nexport function connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 24),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (28, 52),
                line_range: (3, 3),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 57,
        content_hash: "utils-ts-lines".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/app.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/app.ts"),
        content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (36, 46),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 49,
        content_hash: "app-ts".to_string(),
        references: vec![
            ReferenceRecord {
                name: "utils".to_string(),
                qualified_name: Some("src/utils".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (24, 33),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("src/utils/connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (36, 42),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.ts".to_string(),
        language: LanguageId::TypeScript,
        classification: symforge::domain::FileClassification::for_code_path("src/other.ts"),
        content: b"connect();\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 9),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 10,
        content_hash: "other-ts".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (0, 6),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src/utils/connect:4",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "slash qualified symbol aliases should allow direct line-hint disambiguation: {body}"
    );
    assert!(
        body.contains("src/app.ts"),
        "slash qualified symbol aliases with line hints should keep exact-selector matches: {body}"
    );
    assert!(
        !body.contains("src/other.ts"),
        "slash qualified symbol aliases with line hints should drop unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_dotted_qualified_symbol_alias_line_hint_disambiguates_exact_selector()
 {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "pkg/db.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("pkg/db.py"),
        content: b"def connect():\n    pass\n\ndef connect():\n    pass\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 13),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (25, 38),
                line_range: (4, 4),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 49,
        content_hash: "db-py".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "pkg/service.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("pkg/service.py"),
        content: b"from pkg.db import connect\n\ndef run():\n    connect()\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (28, 38),
            line_range: (3, 3),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 54,
        content_hash: "service-py".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("pkg.db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (5, 11),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("pkg.db.connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (41, 47),
                line_range: (3, 3),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "pkg/other.py".to_string(),
        language: LanguageId::Python,
        classification: symforge::domain::FileClassification::for_code_path("pkg/other.py"),
        content: b"def run():\n    connect()\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 25,
        content_hash: "other-py".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (15, 21),
            line_range: (1, 1),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20pkg.db.connect:5",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "dotted qualified symbol aliases should allow direct line-hint disambiguation: {body}"
    );
    assert!(
        body.contains("pkg/service.py"),
        "dotted qualified symbol aliases with line hints should keep exact-selector matches: {body}"
    );
    assert!(
        !body.contains("pkg/other.py"),
        "dotted qualified symbol aliases with line hints should drop unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_combined_hint_uses_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = make_rust_file("src/db.rs", "connect");
    let dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/other.rs"),
        content: b"fn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 23,
        content_hash: "other".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (11, 17),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src%2Fdb.rs%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        body.contains("src/service.rs"),
        "combined prompt should keep exact-selector matches: {body}"
    );
    assert!(
        !body.contains("src/other.rs"),
        "combined prompt should drop unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_line_hint_disambiguates_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/db.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/db.rs"),
        content: b"fn connect() {}\nfn connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (16, 30),
                line_range: (2, 2),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 31,
        content_hash: "db".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src%2Fdb.rs%20connect%20line%202",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "line hint should disambiguate combined prompt routing: {body}"
    );
    assert!(
        body.contains("src/service.rs"),
        "line hint should still produce symbol context output: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_path_line_hint_disambiguates_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/db.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/db.rs"),
        content: b"fn connect() {}\nfn connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (16, 30),
                line_range: (2, 2),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 31,
        content_hash: "db".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20src%2Fdb.rs%3A2%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "path:line hint should disambiguate combined prompt routing: {body}"
    );
    assert!(
        body.contains("src/service.rs"),
        "path:line hint should still produce symbol context output: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_basename_line_hint_disambiguates_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/db.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/db.rs"),
        content: b"fn connect() {}\nfn connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (16, 30),
                line_range: (2, 2),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 31,
        content_hash: "db".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20db.rs%3A2%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "basename:line hint should disambiguate combined prompt routing: {body}"
    );
    assert!(
        body.contains("src/service.rs"),
        "basename:line hint should still produce symbol context output: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_extensionless_alias_line_hint_disambiguates_exact_selector() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let target = IndexedFile {
        relative_path: "src/db.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/db.rs"),
        content: b"fn connect() {}\nfn connect() {}\n".to_vec(),
        symbols: vec![
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 1,
                byte_range: (16, 30),
                line_range: (2, 2),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 31,
        content_hash: "db".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let dependent = IndexedFile {
        relative_path: "src/service.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/service.rs"),
        content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (24, 47),
            line_range: (2, 2),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 47,
        content_hash: "service".to_string(),
        references: vec![
            ReferenceRecord {
                name: "db".to_string(),
                qualified_name: Some("crate::db".to_string()),
                kind: ReferenceKind::Import,
                byte_range: (0, 6),
                line_range: (0, 0),
                enclosing_symbol_index: Some(0),
            },
            ReferenceRecord {
                name: "connect".to_string(),
                qualified_name: Some("crate::db::connect".to_string()),
                kind: ReferenceKind::Call,
                byte_range: (34, 40),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            },
        ],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let unrelated = IndexedFile {
        relative_path: "src/other.rs".to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path("src/other.rs"),
        content: b"fn run() { connect(); }\n".to_vec(),
        symbols: vec![SymbolRecord {
            name: "run".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        }],
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: 23,
        content_hash: "other".to_string(),
        references: vec![ReferenceRecord {
            name: "connect".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (11, 17),
            line_range: (0, 0),
            enclosing_symbol_index: Some(0),
        }],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    };
    let index = build_shared_index(vec![target, dependent, unrelated]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=inspect%20db%3A2%20connect",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        !body.contains("Ambiguous symbol selector"),
        "extensionless alias should disambiguate combined prompt routing: {body}"
    );
    assert!(
        body.contains("src/service.rs"),
        "extensionless alias should still produce symbol context output: {body}"
    );
    assert!(
        !body.contains("src/other.rs"),
        "extensionless alias should exclude unrelated same-name hits: {body}"
    );

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}
