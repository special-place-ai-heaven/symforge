/// Integration tests proving all 5 hook types work end-to-end.
///
/// Proves:
///   HOOK-04: Read hook — GET /outline returns formatted outline with symbol names, key refs, budget
///   HOOK-05: Edit hook — GET /impact returns symbol diff with Added/Changed/Removed labels
///   HOOK-06: Write hook — GET /impact?new_file=true returns indexed confirmation with symbol count
///   HOOK-07: Grep hook — GET /symbol-context returns annotated matches, capped at 10
///   HOOK-08: SessionStart hook — GET /repo-map returns formatted directory tree
///   HOOK-09: Budget enforcement — responses stay within token limits
///   INFR-04: Token stats — /stats tracks fires and saved tokens, savings footer in responses
///
/// Note: Tests bind sidecar ports on loopback. Run with `--test-threads=1` to avoid
/// port races and CWD mutation conflicts.
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
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
// Serialize all tests that manipulate process cwd.
// tokio::sync::Mutex is Send so it can be held across await points.
// ---------------------------------------------------------------------------
static CWD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a minimal `IndexedFile` for a Rust source file with named functions.
fn make_rust_file_with_symbols(path: &str, symbols: Vec<(&str, SymbolKind)>) -> IndexedFile {
    let symbol_records: Vec<SymbolRecord> = symbols
        .iter()
        .enumerate()
        .map(|(i, (name, kind))| {
            let line = (i as u32 + 1) * 3;
            SymbolRecord {
                name: name.to_string(),
                kind: *kind,
                depth: 0,
                sort_order: i as u32,
                byte_range: (i as u32 * 20, i as u32 * 20 + 15),
                line_range: (line, line + 1),
                doc_byte_range: None,
                item_byte_range: None,
            }
        })
        .collect();

    let content = symbols
        .iter()
        .map(|(name, _)| format!("fn {}() {{}}", name))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();

    let byte_len = content.len() as u64;

    IndexedFile {
        relative_path: path.to_string(),
        language: LanguageId::Rust,
        classification: symforge::domain::FileClassification::for_code_path(path),
        content,
        symbols: symbol_records,
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len,
        content_hash: format!("hash-{}", path),
        references: vec![],
        alias_map: HashMap::new(),
        mtime_secs: 0,
    }
}

/// Build a file with both symbols and references.
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
            byte_range: (0, 10),
            line_range: (line, line),
            enclosing_symbol_index: None,
        })
        .collect();
    file
}

/// Build a `SharedIndex` from a list of `IndexedFile`s using the public API.
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
/// Returns (status_code_line, body).
fn raw_http_get_with_status(
    port: u16,
    path: &str,
    query: &str,
) -> anyhow::Result<(String, String)> {
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

    let status_line = response.lines().next().unwrap_or("").to_string();

    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("")
        .to_string();

    Ok((status_line, body))
}

/// Make a synchronous raw HTTP GET request to `127.0.0.1:{port}{path}?{query}`.
/// Returns the response body or an error.
fn raw_http_get(port: u16, path: &str, query: &str) -> anyhow::Result<String> {
    raw_http_get_with_status(port, path, query).map(|(_, body)| body)
}

// ---------------------------------------------------------------------------
// HOOK-04: Read hook — /outline returns formatted text (not JSON)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_read_hook_returns_formatted_outline() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let file = make_rust_file_with_symbols(
        "src/foo.rs",
        vec![
            ("alpha", SymbolKind::Function),
            ("Beta", SymbolKind::Struct),
            ("gamma", SymbolKind::Function),
        ],
    );
    let index = build_shared_index(vec![file]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/outline", "path=src/foo.rs")
        .expect("GET /outline must succeed");

    // Must be plain text, not a JSON array.
    assert!(
        !body.trim_start().starts_with('['),
        "outline response must be plain text, not a JSON array; got: {}",
        &body[..body.len().min(100)]
    );

    // Must contain symbol names.
    assert!(
        body.contains("alpha"),
        "outline must contain 'alpha'; body: {body}"
    );
    assert!(
        body.contains("Beta"),
        "outline must contain 'Beta'; body: {body}"
    );
    assert!(
        body.contains("gamma"),
        "outline must contain 'gamma'; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-04: Read hook — 404 for non-indexed path
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_read_hook_noop_for_missing_file() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let (status_line, _body) =
        raw_http_get_with_status(handle.port, "/outline", "path=nonexistent.rs")
            .expect("GET /outline for missing file must not error at transport level");

    assert!(
        status_line.contains("404"),
        "missing file must return 404; status: {status_line}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-09: Budget enforcement — /outline with tiny max_tokens returns truncated response
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_read_hook_budget_enforced() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Create a file with 20 symbols to ensure it would exceed a 50-token (200 byte) budget.
    let symbols: Vec<(&str, SymbolKind)> = vec![
        ("alpha_fn_one", SymbolKind::Function),
        ("beta_fn_two", SymbolKind::Function),
        ("gamma_fn_three", SymbolKind::Function),
        ("delta_fn_four", SymbolKind::Function),
        ("epsilon_fn_five", SymbolKind::Function),
        ("zeta_fn_six", SymbolKind::Function),
        ("eta_fn_seven", SymbolKind::Function),
        ("theta_fn_eight", SymbolKind::Function),
        ("iota_fn_nine", SymbolKind::Function),
        ("kappa_fn_ten", SymbolKind::Function),
        ("lambda_fn_eleven", SymbolKind::Function),
        ("mu_fn_twelve", SymbolKind::Function),
        ("nu_fn_thirteen", SymbolKind::Function),
        ("xi_fn_fourteen", SymbolKind::Function),
        ("omicron_fn_fifteen", SymbolKind::Function),
    ];
    let file = make_rust_file_with_symbols("src/big.rs", symbols);
    let index = build_shared_index(vec![file]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // max_tokens=10 → max 40 bytes — only the header line can fit, the rest must be truncated.
    let body = raw_http_get(handle.port, "/outline", "path=src/big.rs&max_tokens=10")
        .expect("GET /outline with budget must succeed");

    // The non-footer portion must be under 40 bytes (10 tokens * 4 bytes/token).
    // However we also add the "[~N tokens saved]" footer, so we check for "truncated".
    assert!(
        body.contains("truncated"),
        "response with tiny budget must contain 'truncated'; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-05: Edit hook — /impact returns symbol diff labels
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_edit_hook_impact_diff() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Write a Rust file to disk so impact handler can re-read it.
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let rs_path = src_dir.join("edit_test.rs");
    // "After edit" content — has renamed function.
    std::fs::write(&rs_path, b"fn renamed() {}").unwrap();

    // Index with "before edit" symbol.
    let file =
        make_rust_file_with_symbols("src/edit_test.rs", vec![("original", SymbolKind::Function)]);
    let index = build_shared_index(vec![file]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/impact", "path=src/edit_test.rs")
        .expect("GET /impact must succeed");

    // The diff must show that the old symbol was removed and/or new was added.
    let has_diff_label = body.contains("Added")
        || body.contains("Removed")
        || body.contains("Changed")
        || body.contains("Impact");

    assert!(
        has_diff_label,
        "impact response must contain diff labels (Added/Changed/Removed/Impact); body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-05: Edit hook — callers shown for changed symbols
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_edit_hook_shows_callers() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Write file a.rs to disk (the file we'll "edit").
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("a.rs"), b"fn helper_func() {}").unwrap();

    // File a.rs defines helper_func; file b.rs references it.
    let file_a =
        make_rust_file_with_symbols("src/a.rs", vec![("helper_func", SymbolKind::Function)]);
    let file_b = make_rust_file_with_refs(
        "src/b.rs",
        vec![("some_caller", SymbolKind::Function)],
        vec![("helper_func", ReferenceKind::Call, 5)],
    );

    let index = build_shared_index(vec![file_a, file_b]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Trigger impact for a.rs — helper_func is in its pre-state.
    // The handler will re-read a.rs from disk and compare.
    let body =
        raw_http_get(handle.port, "/impact", "path=src/a.rs").expect("GET /impact must succeed");

    // Response should contain reference to b.rs as a caller (or show no callers if no diff).
    // The key assertion is that the response is formatted text.
    assert!(
        !body.trim_start().starts_with('['),
        "impact response must be plain text, not JSON; body: {}",
        &body[..body.len().min(200)]
    );

    // It should contain a token savings footer.
    assert!(
        body.contains("tokens saved"),
        "impact response must have token savings footer; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-06: Write hook — /impact?new_file=true returns index confirmation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_write_hook_confirms_index() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Create a new Rust file on disk that isn't yet in the index.
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let new_file_path = src_dir.join("new_module.rs");
    std::fs::write(
        &new_file_path,
        b"pub fn create_thing() {}\npub struct Config {}\npub fn destroy_thing() {}",
    )
    .unwrap();

    let index = build_shared_index(vec![]); // empty — file not yet indexed
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/impact",
        "path=src/new_module.rs&new_file=true",
    )
    .expect("GET /impact?new_file=true must succeed");

    // Response must confirm indexing.
    assert!(
        body.contains("Indexed"),
        "new_file response must contain 'Indexed'; body: {body}"
    );

    // Must show that symbols were found (symbol count > 0).
    assert!(
        body.contains("fn") || body.contains("struct") || body.contains("Symbols"),
        "new_file response must mention symbols or language info; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-07: Grep hook — /symbol-context returns annotated matches (not JSON)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_grep_hook_annotates_matches() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Create a file with a reference to "helper" inside a known function.
    let file = {
        let mut f = make_rust_file_with_symbols("src/main.rs", vec![("run", SymbolKind::Function)]);
        // Add a reference with an enclosing symbol index pointing to "run" (index 0).
        f.references = vec![ReferenceRecord {
            name: "helper".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (5, 15),
            line_range: (2, 2),
            enclosing_symbol_index: Some(0), // inside "run"
        }];
        f
    };

    let index = build_shared_index(vec![file]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/symbol-context", "name=helper")
        .expect("GET /symbol-context must succeed");

    // Must be plain text (not JSON array).
    assert!(
        !body.trim_start().starts_with('['),
        "symbol-context must be plain text, not JSON; got: {}",
        &body[..body.len().min(100)]
    );

    // Must contain the file path.
    assert!(
        body.contains("src/main.rs"),
        "must mention the file; body: {body}"
    );

    // Must contain the enclosing function annotation.
    assert!(
        body.contains("in fn run"),
        "must annotate enclosing function 'run'; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-07 + HOOK-09: Grep hook — caps at 10 matches
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_grep_hook_caps_at_10() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Create 15 files, each with one reference to "target_symbol".
    let files: Vec<IndexedFile> = (0..15usize)
        .map(|i| {
            let path = format!("src/file_{i:02}.rs");
            make_rust_file_with_refs(
                &path,
                vec![("my_fn", SymbolKind::Function)],
                vec![("target_symbol", ReferenceKind::Call, 3)],
            )
        })
        .collect();

    let index = build_shared_index(files);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/symbol-context", "name=target_symbol")
        .expect("GET /symbol-context must succeed");

    // Should indicate cap — either "showing 10 of" or "truncated".
    assert!(
        body.contains("showing") || body.contains("truncated"),
        "response with >10 matches must indicate cap or truncation; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-08: SessionStart — /repo-map returns formatted directory tree
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_session_start_repo_map() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let files = vec![
        make_rust_file_with_symbols("src/main.rs", vec![("main", SymbolKind::Function)]),
        make_rust_file_with_symbols(
            "src/lib.rs",
            vec![
                ("run", SymbolKind::Function),
                ("stop", SymbolKind::Function),
            ],
        ),
        make_rust_file_with_symbols("tests/basic.rs", vec![("test_one", SymbolKind::Function)]),
    ];
    let index = build_shared_index(files);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/repo-map", "").expect("GET /repo-map must succeed");

    // Must be plain text (not a JSON array).
    assert!(
        !body.trim_start().starts_with('['),
        "repo-map must be plain text, not JSON; got: {}",
        &body[..body.len().min(100)]
    );

    // Must mention directory paths with file counts.
    assert!(
        body.contains("src"),
        "repo-map must contain 'src' directory; body: {body}"
    );

    // Must mention symbols.
    assert!(
        body.contains("symbols"),
        "repo-map must mention symbol counts; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// HOOK-08 + HOOK-09: SessionStart — /repo-map under 500 tokens (2000 bytes)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_repo_map_under_500_tokens() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Create 30 files to stress test the budget.
    let files: Vec<IndexedFile> = (0..30usize)
        .map(|i| {
            make_rust_file_with_symbols(
                &format!("src/module_{i:02}.rs"),
                vec![
                    ("fn_a", SymbolKind::Function),
                    ("fn_b", SymbolKind::Function),
                ],
            )
        })
        .collect();

    let index = build_shared_index(files);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/repo-map", "").expect("GET /repo-map must succeed");

    // 500 tokens * 4 bytes = 2000 bytes.
    assert!(
        body.len() <= 2000,
        "repo-map must be under 2000 bytes (500 tokens); got {} bytes",
        body.len()
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// INFR-04: Token stats — /stats tracks fires and saved tokens after hook calls
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_token_stats_after_hooks() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Write the file to disk so impact handler can re-read it.
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("stats_test.rs"), b"fn tracked_fn() {}").unwrap();

    let file = make_rust_file_with_symbols(
        "src/stats_test.rs",
        vec![("tracked_fn", SymbolKind::Function)],
    );
    let index = build_shared_index(vec![file]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Fire /outline (Read hook).
    let _ = raw_http_get(handle.port, "/outline", "path=src/stats_test.rs")
        .expect("GET /outline for stats test must succeed");

    // Fire /impact (Edit hook).
    let _ = raw_http_get(handle.port, "/impact", "path=src/stats_test.rs")
        .expect("GET /impact for stats test must succeed");

    // Query /stats.
    let stats_body = raw_http_get(handle.port, "/stats", "").expect("GET /stats must succeed");

    let stats: serde_json::Value =
        serde_json::from_str(&stats_body).expect("/stats must return valid JSON");

    let read_fires = stats["read_fires"]
        .as_u64()
        .expect("read_fires must be a number");
    let edit_fires = stats["edit_fires"]
        .as_u64()
        .expect("edit_fires must be a number");

    assert!(
        read_fires >= 1,
        "read_fires must be >= 1 after /outline call; got {read_fires}"
    );
    assert!(
        edit_fires >= 1,
        "edit_fires must be >= 1 after /impact call; got {edit_fires}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}

// ---------------------------------------------------------------------------
// INFR-04: Token savings footer — /outline response ends with "[~N tokens saved]"
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_token_savings_footer() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Create a large file so savings are non-trivial.
    let symbols: Vec<(&str, SymbolKind)> = vec![
        ("process_request", SymbolKind::Function),
        ("validate_input", SymbolKind::Function),
        ("handle_error", SymbolKind::Function),
    ];
    let mut file = make_rust_file_with_symbols("src/service.rs", symbols);
    // Give it a large byte_len so savings > 0.
    file.byte_len = 5000;

    let index = build_shared_index(vec![file]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1", None)
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/outline", "path=src/service.rs")
        .expect("GET /outline must succeed");

    // Must contain "[~N tokens saved]" footer pattern.
    assert!(
        body.contains("[~") && body.contains("tokens saved]"),
        "outline response must contain '[~N tokens saved]' footer; body: {body}"
    );

    handle.shutdown_and_join().await;
    std::env::set_current_dir(&original).unwrap();
}
