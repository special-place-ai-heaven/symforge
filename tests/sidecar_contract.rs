// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Golden-file contract tests for the sidecar HTTP surface.
//!
//! Every sidecar endpoint is a published contract consumed by hooks in user
//! environments we don't control (ADR 0002). A rename, field removal, or
//! response-shape change is a breaking change to every hook-installed user.
//!
//! These tests lock a representative request + response for each endpoint as
//! a golden file on disk. Any contract change forces the golden to update in
//! the same commit, making drift reviewable in diff.
//!
//! Regenerate goldens after an intentional contract change:
//!
//! ```bash
//! UPDATE_GOLDENS=1 cargo test --test sidecar_contract -- --test-threads=1
//! ```
//!
//! Coverage:
//! - `/health`          (JSON; uptime_secs normalized)
//! - `/stats`           (JSON; fresh-spawn zero state)
//! - `/outline`         (text)
//! - `/impact`          (text; edit-impact path)
//! - `/impact?new_file` (text; new-file path)
//! - `/symbol-context`  (text)
//! - `/repo-map`        (text)
//! - `/prompt-context`  (text)
//!
//! Each text-endpoint test additionally asserts that the paired
//! `/workflows/*` adapter returns byte-identical output. This locks the alias
//! contract from ADR 0001 for the sidecar layer.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use symforge::{
    domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord},
    live_index::{IndexedFile, LiveIndex, ParseStatus, SharedIndex},
    sidecar::spawn_sidecar,
};
use tempfile::TempDir;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Serialize all cwd-manipulating tests (spawn_sidecar writes port files into
// cwd/.symforge). tokio::sync::Mutex is Send so it can be held across awaits.
// ---------------------------------------------------------------------------

static CWD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ---------------------------------------------------------------------------
// Golden-file plumbing
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sidecar_contract")
}

fn update_goldens() -> bool {
    std::env::var("UPDATE_GOLDENS")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Normalize line endings to `\n` so goldens are portable across Windows/Unix
/// regardless of `core.autocrlf` settings.
fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn assert_golden(name: &str, actual: &str) {
    let path = fixtures_dir().join(name);
    let actual = normalize_newlines(actual);

    if update_goldens() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixtures dir");
        std::fs::write(&path, actual.as_bytes()).expect("write golden");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing sidecar contract golden: {}\n\
             \n\
             Regenerate with:\n\
             \n    UPDATE_GOLDENS=1 cargo test --test sidecar_contract -- --test-threads=1\n\
             \n\
             Golden files lock the sidecar HTTP contract (ADR 0002). They are consumed\n\
             by external hooks we don't control — any change must be explicit and reviewable.",
            path.display()
        )
    });
    let expected = normalize_newlines(&expected);

    if expected != actual {
        panic!(
            "sidecar HTTP contract drift in golden `{name}`.\n\
             \n\
             The live response diverged from the committed golden. If the change is\n\
             intentional, regenerate the goldens in the same commit so reviewers see\n\
             the contract diff:\n\
             \n    UPDATE_GOLDENS=1 cargo test --test sidecar_contract -- --test-threads=1\n\
             \n\
             Note: this contract is consumed by hooks in user environments. A rename,\n\
             field removal, or shape change is a breaking change (ADR 0002).\n\
             \n\
             --- expected (golden) ---\n{expected}\n\
             --- actual (live) ---\n{actual}\n\
             --- end ---\n"
        );
    }
}

// ---------------------------------------------------------------------------
// Fixture builders (shape of `make_rust_file*` duplicated from
// tests/sidecar_integration.rs and tests/hook_enrichment_integration.rs —
// test code is fine to duplicate and the existing helpers aren't `pub`).
// ---------------------------------------------------------------------------

fn make_rust_file_with_symbols(path: &str, symbols: Vec<(&str, SymbolKind)>) -> IndexedFile {
    let content = b"// fixed fixture content\n".to_vec();
    let symbol_records: Vec<SymbolRecord> = symbols
        .into_iter()
        .enumerate()
        .map(|(i, (name, kind))| SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: i as u32,
            byte_range: (0, content.len() as u32),
            line_range: (1, 1),
            doc_byte_range: None,
            item_byte_range: None,
        })
        .collect();
    IndexedFile {
        relative_path: path.to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path(path),
        content: content.clone(),
        symbols: symbol_records,
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: content.len() as u64,
        content_hash: "golden".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    }
}

fn make_rust_file_with_refs(
    path: &str,
    symbols: Vec<(&str, SymbolKind)>,
    refs: Vec<(&str, ReferenceKind, u32)>,
) -> IndexedFile {
    let mut file = make_rust_file_with_symbols(path, symbols);
    file.references = refs
        .into_iter()
        .map(|(name, kind, line)| ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind,
            byte_range: (0, 0),
            line_range: (line, line),
            enclosing_symbol_index: Some(0),
        })
        .collect();
    file
}

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

// ---------------------------------------------------------------------------
// HTTP + cwd helpers
// ---------------------------------------------------------------------------

fn raw_http_get(port: u16, path: &str, query: &str) -> anyhow::Result<String> {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let timeout = Duration::from_millis(1000);
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
// JSON normalization helpers
// ---------------------------------------------------------------------------

/// Pretty-print JSON (stable key order via serde_json's serializer) and
/// zero-out `uptime_secs` so the golden is timing-independent.
fn normalize_health_json(body: &str) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(body).expect("/health body must be valid JSON");
    if let Some(obj) = value.as_object_mut()
        && obj.contains_key("uptime_secs")
    {
        obj.insert("uptime_secs".into(), serde_json::Value::from(0u64));
    }
    let mut out = serde_json::to_string_pretty(&value).unwrap();
    out.push('\n');
    out
}

/// Pretty-print JSON for stable golden comparison. Used for `/stats` where
/// all counters are zero on a fresh spawn (no hook fires invoked).
fn normalize_stats_json(body: &str) -> String {
    let value: serde_json::Value =
        serde_json::from_str(body).expect("/stats body must be valid JSON");
    let mut out = serde_json::to_string_pretty(&value).unwrap();
    out.push('\n');
    out
}

// ---------------------------------------------------------------------------
// /health — JSON contract
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_health_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![
        make_rust_file_with_symbols("src/alpha.rs", vec![("alpha", SymbolKind::Function)]),
        make_rust_file_with_symbols("src/beta.rs", vec![("beta", SymbolKind::Function)]),
    ]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/health", "").expect("GET /health");
    let normalized = normalize_health_json(&body);
    assert_golden("health.json", &normalized);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// /stats — JSON contract (fresh-spawn zero state)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stats_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Empty index is fine — /stats is independent of index contents.
    let index = build_shared_index(vec![]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Hit /stats FIRST, before any other endpoint, so counters stay at zero.
    let body = raw_http_get(handle.port, "/stats", "").expect("GET /stats");
    let normalized = normalize_stats_json(&body);
    assert_golden("stats.json", &normalized);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// /outline — text contract + workflow alias byte-equality
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_outline_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file_with_symbols(
        "src/lib.rs",
        vec![
            ("hello", SymbolKind::Function),
            ("Config", SymbolKind::Struct),
        ],
    )]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let canonical = raw_http_get(handle.port, "/outline", "path=src/lib.rs").expect("GET /outline");
    let workflow = raw_http_get(handle.port, "/workflows/source-read", "path=src/lib.rs")
        .expect("GET /workflows/source-read");

    assert_eq!(
        normalize_newlines(&workflow),
        normalize_newlines(&canonical),
        "workflow adapter /workflows/source-read must be byte-identical to /outline"
    );
    assert_golden("outline.txt", &canonical);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// /impact (edit-impact) — text contract + workflow alias byte-equality
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_impact_edit_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Write file to disk so /impact can re-read it; match the in-memory
    // fixture content byte-for-byte so the diff outcome is deterministic.
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("edit.rs"), b"// fixed fixture content\n").unwrap();

    let index = build_shared_index(vec![make_rust_file_with_symbols(
        "src/edit.rs",
        vec![("edited", SymbolKind::Function)],
    )]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // `/impact` mutates the index (re-parses the file on disk and updates
    // stored symbols). The FIRST call reconciles pre-state; subsequent calls
    // are idempotent. Run one warmup call before locking the golden so the
    // workflow-alias comparison is a fair byte-equal check against a stable
    // post-reconciliation response shape.
    let _warmup =
        raw_http_get(handle.port, "/impact", "path=src/edit.rs").expect("warmup GET /impact");

    let canonical = raw_http_get(handle.port, "/impact", "path=src/edit.rs").expect("GET /impact");
    let workflow = raw_http_get(
        handle.port,
        "/workflows/post-edit-impact",
        "path=src/edit.rs",
    )
    .expect("GET /workflows/post-edit-impact");

    assert_eq!(
        normalize_newlines(&workflow),
        normalize_newlines(&canonical),
        "workflow adapter /workflows/post-edit-impact must be byte-identical to /impact"
    );
    assert_golden("impact.txt", &canonical);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// /impact?new_file=true — text contract
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_impact_new_file_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    // File is on disk but NOT in the index — mirrors the Write-hook flow.
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("fresh.rs"),
        b"pub fn make_thing() {}\npub struct Thing;\n",
    )
    .unwrap();

    let index = build_shared_index(vec![]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/impact", "path=src/fresh.rs&new_file=true")
        .expect("GET /impact?new_file=true");
    assert_golden("impact_new_file.txt", &body);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// /symbol-context — text contract + workflow alias byte-equality
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_symbol_context_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let definer =
        make_rust_file_with_symbols("src/def.rs", vec![("do_thing", SymbolKind::Function)]);
    let caller = make_rust_file_with_refs(
        "src/call.rs",
        vec![("caller", SymbolKind::Function)],
        vec![("do_thing", ReferenceKind::Call, 1)],
    );
    let index = build_shared_index(vec![definer, caller]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let canonical =
        raw_http_get(handle.port, "/symbol-context", "name=do_thing").expect("GET /symbol-context");
    let workflow = raw_http_get(
        handle.port,
        "/workflows/search-hit-expansion",
        "name=do_thing",
    )
    .expect("GET /workflows/search-hit-expansion");

    assert_eq!(
        normalize_newlines(&workflow),
        normalize_newlines(&canonical),
        "workflow adapter /workflows/search-hit-expansion must be byte-identical to /symbol-context"
    );
    assert_golden("symbol_context.txt", &canonical);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// /repo-map — text contract + workflow alias byte-equality
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_repo_map_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![
        make_rust_file_with_symbols("src/alpha.rs", vec![("alpha", SymbolKind::Function)]),
        make_rust_file_with_symbols("src/beta.rs", vec![("beta", SymbolKind::Function)]),
        make_rust_file_with_symbols("src/gamma.rs", vec![("gamma", SymbolKind::Function)]),
    ]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let canonical = raw_http_get(handle.port, "/repo-map", "").expect("GET /repo-map");
    let workflow =
        raw_http_get(handle.port, "/workflows/repo-start", "").expect("GET /workflows/repo-start");

    assert_eq!(
        normalize_newlines(&workflow),
        normalize_newlines(&canonical),
        "workflow adapter /workflows/repo-start must be byte-identical to /repo-map"
    );
    assert_golden("repo_map.txt", &canonical);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// /prompt-context — text contract + workflow alias byte-equality
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_contract_golden() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file_with_symbols(
        "src/lib.rs",
        vec![("hello", SymbolKind::Function)],
    )]);
    let handle = spawn_sidecar(
        Arc::clone(&index),
        "127.0.0.1",
        Some(tmp.path().to_path_buf()),
    )
    .await
    .expect("spawn_sidecar");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Fixed text that triggers the "exact path" hint path — stable output.
    let query = "text=please%20inspect%20src%2Flib.rs";
    let canonical =
        raw_http_get(handle.port, "/prompt-context", query).expect("GET /prompt-context");
    let workflow = raw_http_get(handle.port, "/workflows/prompt-context", query)
        .expect("GET /workflows/prompt-context");

    assert_eq!(
        normalize_newlines(&workflow),
        normalize_newlines(&canonical),
        "workflow adapter /workflows/prompt-context must be byte-identical to /prompt-context"
    );
    assert_golden("prompt_context.txt", &canonical);

    handle.shutdown_and_join().await;
    restore_cwd(&original);
}
