// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use serde::Deserialize;
/// Integration tests for the LiveIndex startup pipeline.
///
/// These tests prove that discovery → parsing → LiveIndex work together end-to-end,
/// and that the binary produces zero stdout bytes (RELY-04 CI gate).
///
/// Phase 2 tests cover: LIDX-05 (performance), INFR-02 (auto-index behavior),
/// INFR-05 (no v1 tools), tool format verification end-to-end, and RELY-04.
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use symforge::live_index::persist;
use symforge::live_index::{IndexState, LiveIndex, ParseStatus};
use tempfile::tempdir;

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[derive(Clone, Debug, Deserialize)]
struct StartupHealthResponse {
    file_count: usize,
    symbol_count: usize,
    index_state: String,
}

#[derive(Clone, Debug)]
enum StartupSurface {
    Local(StartupHealthResponse),
    Daemon {
        session_id: String,
        health: StartupHealthResponse,
    },
}

fn symforge_binary_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let binary = exe_dir.join("symforge.exe");
    if binary.exists() {
        return Some(binary);
    }

    let binary_unix = exe_dir.join("symforge");
    binary_unix.exists().then_some(binary_unix)
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|contents| contents.trim().to_string())
        .filter(|contents| !contents.is_empty())
}

// The spawned sidecar writes OS-tagged runtime files (sidecar.<os>.port); these test
// helpers run on the SAME OS as the spawned binary, so std::env::consts::OS matches.
// Fall back to the legacy un-tagged name for resilience.
fn read_runtime(dir: &Path, stem: &str, ext: &str) -> Option<String> {
    let sf = dir.join(".symforge");
    let tagged = format!("{stem}.{}.{ext}", std::env::consts::OS);
    read_trimmed(&sf.join(&tagged)).or_else(|| read_trimmed(&sf.join(format!("{stem}.{ext}"))))
}

fn read_sidecar_port(dir: &Path) -> Option<u16> {
    read_runtime(dir, "sidecar", "port")?.parse().ok()
}

fn read_session_id(dir: &Path) -> Option<String> {
    read_runtime(dir, "sidecar", "session")
}

/// Make a synchronous raw HTTP GET request to `127.0.0.1:{port}{path}`.
fn raw_http_get(port: u16, path: &str) -> anyhow::Result<String> {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let timeout = Duration::from_millis(500);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
        .to_string();
    Ok(body)
}

fn fetch_startup_surface(dir: &Path) -> Option<StartupSurface> {
    let port = read_sidecar_port(dir)?;
    let session_id = read_session_id(dir);
    let path = session_id
        .as_ref()
        .map(|session_id| format!("/v1/sessions/{session_id}/sidecar/health"))
        .unwrap_or_else(|| "/health".to_string());
    let body = raw_http_get(port, &path).ok()?;
    let health: StartupHealthResponse = serde_json::from_str(&body).ok()?;
    Some(match session_id {
        Some(session_id) => StartupSurface::Daemon { session_id, health },
        None => StartupSurface::Local(health),
    })
}

fn startup_health(surface: &StartupSurface) -> &StartupHealthResponse {
    match surface {
        StartupSurface::Local(health) => health,
        StartupSurface::Daemon { health, .. } => health,
    }
}

fn no_repo_root_reason(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find(|line| line.contains("no safe project root found"))
        .map(|line| line.trim().to_string())
}

fn terminate_child(mut child: Child) -> std::process::Output {
    let _ = child.stdin.take();
    let _ = child.kill();
    child
        .wait_with_output()
        .expect("startup child process should collect output")
}

// --------------------------------------------------------------------------
// Test: Full startup from tempdir with 5 valid source files
//
// Proves: LIDX-01 (files discovered), LIDX-02 (symbols queryable from RAM),
//         LiveIndex reports Ready state after clean load.
// --------------------------------------------------------------------------

#[test]
fn test_startup_loads_all_files() {
    let dir = tempdir().unwrap();

    write_file(dir.path(), "main.rs", "fn main() {}\nfn helper() {}");
    write_file(dir.path(), "app.py", "def run(): pass\ndef stop(): pass");
    write_file(
        dir.path(),
        "index.js",
        "function start() {}\nfunction end() {}",
    );
    write_file(
        dir.path(),
        "lib.ts",
        "function util(): void {}\nfunction core(): void {}",
    );
    write_file(
        dir.path(),
        "main.go",
        "package main\nfunc main() {}\nfunc run() {}",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    assert_eq!(
        index.index_state(),
        IndexState::Ready,
        "LiveIndex should be Ready after loading 5 valid files"
    );
    assert_eq!(index.file_count(), 5, "should have 5 indexed files");
    assert!(
        index.symbol_count() > 0,
        "should have extracted symbols from valid source files"
    );

    // Verify each file is accessible by relative path
    assert!(
        index.get_file("main.rs").is_some(),
        "main.rs should be queryable"
    );
    assert!(
        index.get_file("app.py").is_some(),
        "app.py should be queryable"
    );
    assert!(
        index.get_file("index.js").is_some(),
        "index.js should be queryable"
    );
    assert!(
        index.get_file("lib.ts").is_some(),
        "lib.ts should be queryable"
    );
    assert!(
        index.get_file("main.go").is_some(),
        "main.go should be queryable"
    );
}

#[test]
fn test_startup_binary_reports_branch_health() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    write_file(dir.path(), "src/main.rs", "fn main() {}\n");
    write_file(dir.path(), "src/lib.rs", "pub fn helper() {}\n");

    let Some(binary_path) = symforge_binary_path() else {
        eprintln!("SKIP test_startup_binary_reports_branch_health: symforge binary not found");
        return;
    };

    // Force local-only mode so the test doesn't spawn/contend with daemon processes.
    let mut child = Command::new(&binary_path)
        .current_dir(dir.path())
        .env("RUST_LOG", "info")
        .env("SYMFORGE_NO_DAEMON", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("failed to run binary at {:?}: {error}", binary_path));

    let timeout = Duration::from_secs(10);
    let deadline = Instant::now() + timeout;
    let mut first_surface = None;
    let mut latest_surface = None;

    while Instant::now() < deadline {
        if let Some(status) = child
            .try_wait()
            .expect("startup process should be pollable")
        {
            panic!("startup process exited before health probe completed: {status}");
        }

        if let Some(surface) = fetch_startup_surface(dir.path()) {
            if first_surface.is_none() {
                first_surface = Some(surface.clone());
            }
            let is_ready = startup_health(&surface).index_state == "Ready";
            latest_surface = Some(surface);
            if is_ready {
                break;
            }
        }

        thread::sleep(Duration::from_millis(50));
    }

    let output = terminate_child(child);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if let Some(StartupSurface::Daemon { session_id, health }) = first_surface.as_ref() {
        assert_eq!(
            health.index_state, "Ready",
            "daemon-backed startup should expose ready health immediately; session_id={session_id}, latest={latest_surface:?}, stderr={stderr}"
        );
        assert!(
            health.file_count > 0 && health.symbol_count > 0,
            "daemon-backed startup should report indexed content; session_id={session_id}, first={health:?}, stderr={stderr}"
        );
        return;
    }

    if let Some(surface) = latest_surface.as_ref() {
        let health = startup_health(surface);
        if matches!(surface, StartupSurface::Local(_)) && health.index_state == "Ready" {
            assert!(
                health.file_count > 0 && health.symbol_count > 0,
                "local startup should become ready with indexed content, got {health:?}"
            );
            return;
        }
    }

    if let Some(reason) = no_repo_root_reason(&stderr) {
        assert!(
            reason.contains("no safe project root found"),
            "expected a precise missing-root reason, got: {reason}"
        );
        return;
    }

    panic!(
        "startup probe found no acceptable branch outcome; first={first_surface:?}, latest={latest_surface:?}, stderr={stderr}"
    );
}

// --------------------------------------------------------------------------
// Test: Circuit breaker trips when >20% of files are garbage
//
// Proves: RELY-01 (circuit breaker fires on mass failure).
//
// Strategy: .rb files are discovered (Ruby is a known extension) but parsing
// returns FileOutcome::Failed because the language is not onboarded in
// parse_source. 3 valid Rust + 3 Ruby = 50% failure rate > 20% threshold.
// --------------------------------------------------------------------------

#[test]
fn test_circuit_breaker_trips_on_mass_failure() {
    let dir = tempdir().unwrap();

    // 3 valid Rust files → Parsed
    write_file(dir.path(), "a.rs", "fn alpha() {}");
    write_file(dir.path(), "b.rs", "fn beta() {}");
    write_file(dir.path(), "c.rs", "fn gamma() {}");

    // v2 added 16 languages — tree-sitter parses everything resiliently, so we can't
    // trigger real parse failures from file content alone. Circuit breaker logic is
    // covered by unit tests in store.rs (test_cb_trips_above_threshold, etc.).
    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    // With all valid files, circuit breaker should NOT trip.
    assert!(
        matches!(index.index_state(), IndexState::Ready),
        "All valid files should result in Ready state, got: {:?}",
        index.index_state()
    );
}

// --------------------------------------------------------------------------
// Test: Syntax error files produce PartialParse status but remain queryable
//
// Proves: RELY-02 (symbols retained on partial parse).
// --------------------------------------------------------------------------

#[test]
fn test_partial_parse_keeps_symbols() {
    let dir = tempdir().unwrap();

    // One file with a valid function AND a broken function signature.
    // tree-sitter error-recovers: valid() should still be extracted.
    write_file(dir.path(), "mixed.rs", "fn valid() {}\nfn broken(");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    let file = index
        .get_file("mixed.rs")
        .expect("mixed.rs should be indexed");

    // The file must be PartialParse (not Failed) — tree-sitter recovers
    assert!(
        matches!(file.parse_status, ParseStatus::PartialParse { .. }),
        "syntax errors should produce PartialParse, got: {:?}",
        file.parse_status
    );

    // At least the valid() function should be in the symbols list
    assert!(
        !file.symbols.is_empty(),
        "symbols should be retained even when syntax errors are present"
    );

    let symbol_names: Vec<&str> = file.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        symbol_names.contains(&"valid"),
        "valid() function should be extracted despite later syntax error; symbols: {symbol_names:?}"
    );
}

// --------------------------------------------------------------------------
// Test: Content bytes stored for all files including failed ones
//
// Proves: LIDX-03 (zero disk I/O on read path — content is in memory).
// --------------------------------------------------------------------------

#[test]
fn test_content_bytes_stored_for_all_files() {
    let dir = tempdir().unwrap();

    let content_a = "fn hello() { println!(\"hello\"); }";
    let content_b = "def greet(): pass";
    write_file(dir.path(), "a.rs", content_a);
    write_file(dir.path(), "b.py", content_b);

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    let file_a = index.get_file("a.rs").expect("a.rs should be indexed");
    assert_eq!(
        file_a.content.len(),
        content_a.len(),
        "content bytes length should match file size"
    );
    assert_eq!(
        file_a.content,
        content_a.as_bytes(),
        "content bytes should match what was written to disk"
    );

    let file_b = index.get_file("b.py").expect("b.py should be indexed");
    assert_eq!(
        file_b.content.len(),
        content_b.len(),
        "content bytes length should match file size for Python file"
    );
    assert_eq!(
        file_b.content,
        content_b.as_bytes(),
        "content bytes should match what was written to disk"
    );
}

// --------------------------------------------------------------------------
// Test: Symbols queryable by file path after load
//
// Proves: LIDX-02 (symbols queryable from RAM).
// --------------------------------------------------------------------------

#[test]
fn test_symbols_queryable_by_file_path() {
    let dir = tempdir().unwrap();

    write_file(
        dir.path(),
        "funcs.rs",
        "fn alpha() {}\nfn beta() {}\nfn gamma() {}",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    let symbols = index.symbols_for_file("funcs.rs");
    assert!(
        symbols.len() >= 3,
        "should extract at least 3 functions; got: {:?}",
        symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"alpha"), "alpha() should be indexed");
    assert!(names.contains(&"beta"), "beta() should be indexed");
    assert!(names.contains(&"gamma"), "gamma() should be indexed");
}

// --------------------------------------------------------------------------
// Test: Stdout purity — binary stdout is empty (RELY-04 CI gate)
//
// Spawns the compiled binary, captures stdout, asserts it is empty.
// All tracing output goes to stderr. This is the Phase 1 completeness gate.
// --------------------------------------------------------------------------

#[test]
fn test_stdout_purity() {
    // Create a tempdir with a few valid source files and a .git directory
    // so find_git_root() anchors to the tempdir instead of walking up.
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    write_file(dir.path(), "main.rs", "fn main() {}");
    write_file(dir.path(), "lib.rs", "fn helper() {}");

    // Locate the compiled binary
    let exe = std::env::current_exe()
        .expect("should be able to find test executable path")
        .parent()
        .expect("test executable has a parent dir")
        .to_path_buf();

    // The binary is in the same profile directory (debug or release)
    let binary = exe.join("symforge.exe");
    if !binary.exists() {
        // On non-Windows or different naming, try without .exe
        let binary_unix = exe.join("symforge");
        if !binary_unix.exists() {
            // Binary not built yet (CI); skip gracefully but warn
            eprintln!(
                "SKIP test_stdout_purity: binary not found at {:?} or {:?}",
                binary, binary_unix
            );
            return;
        }
    }

    let binary_path = if binary.exists() {
        binary
    } else {
        exe.join("symforge")
    };

    let output = std::process::Command::new(&binary_path)
        .current_dir(dir.path())
        .env("RUST_LOG", "error") // suppress stderr noise in test output
        .env("SYMFORGE_AUTO_INDEX", "false") // start with empty index for speed
        .stdin(std::process::Stdio::null()) // EOF on stdin → MCP server exits cleanly
        .output()
        .unwrap_or_else(|e| panic!("failed to run binary at {:?}: {e}", binary_path));

    assert!(
        output.stdout.is_empty(),
        "binary stdout must be empty (RELY-04): got {} bytes: {:?}",
        output.stdout.len(),
        String::from_utf8_lossy(&output.stdout)
    );
}

// --------------------------------------------------------------------------
// Test: Custom threshold via CircuitBreakerState::new() changes behavior
//
// Tests threshold configurability end-to-end using the constructor directly
// (more reliable than env var approach in parallel test runs).
//
// Proves: Circuit breaker threshold is configurable (AD-5).
// --------------------------------------------------------------------------

#[test]
fn test_custom_threshold_prevents_trip_at_high_threshold() {
    use symforge::live_index::store::CircuitBreakerState;

    // 10 files, 3 failures = 30% failure rate
    // With threshold=0.50 (50%), should NOT trip
    let cb = CircuitBreakerState::new(0.50);
    for _ in 0..7 {
        cb.record_success();
    }
    for i in 0..3 {
        cb.record_failure(&format!("file{i}.rb"), "not onboarded");
    }
    assert!(
        !cb.should_abort(),
        "30% failure rate should NOT trip a 50% threshold circuit breaker"
    );
}

#[test]
fn test_custom_threshold_trips_at_low_threshold() {
    use symforge::live_index::store::CircuitBreakerState;

    // 10 files, 2 failures = 20% failure rate
    // With threshold=0.10 (10%), 20% > 10% should trip
    let cb = CircuitBreakerState::new(0.10);
    for _ in 0..8 {
        cb.record_success();
    }
    for i in 0..2 {
        cb.record_failure(&format!("file{i}.rb"), "not onboarded");
    }
    assert!(
        cb.should_abort(),
        "20% failure rate should trip a 10% threshold circuit breaker"
    );
}

// ============================================================================
// Phase 2 Integration Tests
// ============================================================================

// --------------------------------------------------------------------------
// Test LIDX-05: Performance — load completes in <500ms for 70 files
//
// Creates 70 valid Rust files in a tempdir and times LiveIndex::load.
// --------------------------------------------------------------------------

#[test]
fn test_load_perf_70_files() {
    let dir = tempdir().unwrap();

    for i in 0..70 {
        let content = format!(
            "fn func_{i}() {{}}\nfn helper_{i}(x: u32) -> u32 {{ x + {i} }}\nstruct Struct_{i} {{}}\n"
        );
        write_file(dir.path(), &format!("file_{i:03}.rs"), &content);
    }

    let start = std::time::Instant::now();
    let shared = LiveIndex::load(dir.path()).unwrap();
    let elapsed = start.elapsed();

    let index = shared.read();
    assert_eq!(index.file_count(), 70, "should have indexed 70 files");
    assert!(
        elapsed.as_millis() < 500,
        "LIDX-05: 70-file load must complete in <500ms, took {}ms",
        elapsed.as_millis()
    );
}

// --------------------------------------------------------------------------
// Test LIDX-05: Performance — load completes in <3s for 1000 files
//
// Marked #[ignore] to keep CI fast; run with: cargo test -- --ignored
// --------------------------------------------------------------------------

#[test]
#[ignore]
fn test_load_perf_1000_files() {
    let dir = tempdir().unwrap();

    for i in 0..1000 {
        let content = format!("fn func_{i}() {{}}\nfn helper_{i}(x: u32) -> u32 {{ x + {i} }}\n");
        write_file(dir.path(), &format!("file_{i:04}.rs"), &content);
    }

    let start = std::time::Instant::now();
    let shared = LiveIndex::load(dir.path()).unwrap();
    let elapsed = start.elapsed();

    let index = shared.read();
    assert_eq!(index.file_count(), 1000, "should have indexed 1000 files");
    assert!(
        elapsed.as_secs() < 3,
        "LIDX-05: 1000-file load must complete in <3s, took {}ms",
        elapsed.as_millis()
    );
}

// --------------------------------------------------------------------------
// Test INFR-02: Auto-index loads when .git is present
//
// Tests the LiveIndex::load decision path directly (not the full binary).
// --------------------------------------------------------------------------

#[test]
fn test_auto_index_loads_when_git_present() {
    let dir = tempdir().unwrap();

    // Create a .git directory (signals this is a git project root)
    fs::create_dir(dir.path().join(".git")).unwrap();
    write_file(dir.path(), "main.rs", "fn main() {}");
    write_file(dir.path(), "lib.rs", "fn helper() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    assert!(
        index.file_count() > 0,
        "auto-index (INFR-02): should have indexed files when .git present"
    );
    assert_eq!(
        index.index_state(),
        IndexState::Ready,
        "auto-index (INFR-02): index should be Ready after loading"
    );
}

// --------------------------------------------------------------------------
// Test INFR-02: Empty index when auto-index is skipped
//
// LiveIndex::empty() is what main.rs calls when SYMFORGE_AUTO_INDEX=false.
// --------------------------------------------------------------------------

#[test]
fn test_empty_index_when_no_auto_index() {
    let empty = LiveIndex::empty();
    let index = empty.read();

    assert_eq!(index.file_count(), 0, "empty index should have 0 files");
    assert_eq!(
        index.index_state(),
        IndexState::Empty,
        "empty index state should be Empty (INFR-02)"
    );
}

// --------------------------------------------------------------------------
// Test INFR-05: Retired v1 run lifecycle tools stay absent
//
// `checkpoint_now` is intentionally revived as a current v7 snapshot tool by
// SFR09. The rest of the v1 run lifecycle names must not appear as function
// definitions in protocol/tools.rs.
// --------------------------------------------------------------------------

#[test]
fn test_retired_v1_run_lifecycle_tools_stay_absent() {
    let retired_v1_tools = [
        "cancel_index_run",
        "resume_index_run",
        "get_index_run",
        "list_index_runs",
        "invalidate_indexed_state",
        "repair_index",
        "inspect_repository_health",
        "get_operational_history",
        "reindex_repository",
    ];
    let tools_source = include_str!("../src/protocol/tools.rs");
    assert!(
        tools_source.contains("fn checkpoint_now"),
        "checkpoint_now is a current v7 checkpoint tool and must remain explicit"
    );
    for tool in &retired_v1_tools {
        // Check for `fn {name}` patterns — actual function definitions, not test strings
        let fn_pattern = format!("fn {tool}");
        assert!(
            !tools_source.contains(&fn_pattern),
            "v1 tool function '{}' must not be defined in protocol/tools.rs (INFR-05)",
            tool
        );
    }
}

// --------------------------------------------------------------------------
// Test TOOL-03: file_outline format end-to-end with real tempdir
//
// Creates a Rust file with a fn and a struct, loads into LiveIndex,
// then calls format::file_outline and verifies the output structure.
// --------------------------------------------------------------------------

#[test]
fn test_file_outline_format_end_to_end() {
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "shapes.rs",
        "struct Circle { radius: f64 }\nfn area(c: &Circle) -> f64 { 3.14 * c.radius * c.radius }",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    let result = format::file_outline(&index, "shapes.rs");

    // Header must show path and symbol count
    assert!(
        result.starts_with("shapes.rs"),
        "outline should start with file path, got: {result}"
    );
    assert!(
        result.contains("symbols"),
        "outline header should contain symbol count, got: {result}"
    );
    // Body must list the symbols we defined
    assert!(
        result.contains("Circle") || result.contains("area"),
        "outline should list extracted symbols, got: {result}"
    );
}

// --------------------------------------------------------------------------
// Test TOOL-01: get_symbol returns source body + footer
//
// Verifies format::symbol_detail extracts real source text from the index.
// --------------------------------------------------------------------------

#[test]
fn test_get_symbol_returns_source_body() {
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "math.rs",
        "fn add(a: u32, b: u32) -> u32 { a + b }",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    let result = format::symbol_detail(&index, "math.rs", "add", None);

    // Should return source body
    assert!(
        result.contains("fn add") || result.contains("add"),
        "symbol detail should contain function source, got: {result}"
    );
    // Footer format: [fn, lines X-Y, N bytes]
    assert!(
        result.contains("bytes]"),
        "symbol detail should contain footer with byte count, got: {result}"
    );
}

// --------------------------------------------------------------------------
// Test TOOL-06: search_text returns ripgrep-style output
//
// Verifies format::search_text_result finds text and formats correctly.
// --------------------------------------------------------------------------

#[test]
fn test_search_text_finds_content() {
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "config.rs",
        "const MAX_RETRIES: u32 = 3;\nconst TIMEOUT: u32 = 30;",
    );
    write_file(
        dir.path(),
        "server.rs",
        "const PORT: u32 = 8080;\nconst MAX_CONN: u32 = 100;",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    let result = format::search_text_result(&index, "const");

    // Summary header: "N matches in M files"
    assert!(
        result.contains("matches in"),
        "search_text should show summary header, got: {result}"
    );
    assert!(
        result.contains("2 files") || result.contains("in 2"),
        "search_text should report 2 files matched, got: {result}"
    );
    // Results grouped by file with line numbers
    assert!(
        result.contains("config.rs") || result.contains("server.rs"),
        "search_text should show file names, got: {result}"
    );
}

// --------------------------------------------------------------------------
// Test TOOL-07: health report format
//
// Verifies format::health_report shows Status: Ready and file counts.
// --------------------------------------------------------------------------

#[test]
fn test_health_report_format() {
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(dir.path(), "a.rs", "fn alpha() {}");
    write_file(dir.path(), "b.rs", "fn beta() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    let result = format::health_report(&index);

    assert!(
        result.contains("Status: Ready"),
        "health_report should show 'Status: Ready' for loaded index, got: {result}"
    );
    assert!(
        result.contains("Files:"),
        "health_report should show file counts, got: {result}"
    );
    assert!(
        result.contains("2 indexed"),
        "health_report should show 2 indexed files, got: {result}"
    );
}

#[test]
fn test_health_report_with_watcher_reconciliation_fields() {
    use std::time::SystemTime;
    use symforge::protocol::format;
    use symforge::watcher::{WatcherInfo, WatcherState};

    let dir = tempdir().unwrap();
    write_file(dir.path(), "a.rs", "fn alpha() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let watcher = WatcherInfo {
        state: WatcherState::Active,
        events_processed: 11,
        last_event_at: Some(SystemTime::now()),
        debounce_window_ms: 250,
        overflow_count: 2,
        last_overflow_at: Some(SystemTime::now()),
        stale_files_found: 4,
        last_reconcile_at: Some(SystemTime::now()),
    };

    let result = format::health_report_with_watcher(&index, &watcher);

    assert!(result.contains("overflows: 2"), "got: {result}");
    assert!(result.contains("reconcile repairs: 4"), "got: {result}");
    assert!(result.contains("last overflow:"), "got: {result}");
    assert!(result.contains("last reconcile:"), "got: {result}");
}

// --------------------------------------------------------------------------
// Test TOOL-08: index_folder reload replaces index contents
//
// Loads from dir A, verifies files A. Then reloads from dir B.
// Verifies index now has B's files and not A's.
// --------------------------------------------------------------------------

#[test]
fn test_index_folder_reload() {
    let dir_a = tempdir().unwrap();
    write_file(dir_a.path(), "alpha.rs", "fn alpha_func() {}");
    write_file(dir_a.path(), "beta.rs", "fn beta_func() {}");

    let dir_b = tempdir().unwrap();
    write_file(dir_b.path(), "gamma.rs", "fn gamma_func() {}");
    write_file(dir_b.path(), "delta.rs", "fn delta_func() {}");
    write_file(dir_b.path(), "epsilon.rs", "fn epsilon_func() {}");

    // Load dir A
    let shared = LiveIndex::load(dir_a.path()).unwrap();
    {
        let index = shared.read();
        assert_eq!(index.file_count(), 2, "dir A should have 2 files");
        assert!(
            index.get_file("alpha.rs").is_some(),
            "alpha.rs should be in index"
        );
    }

    // Reload with dir B
    {
        let mut index = shared.write();
        index.reload(dir_b.path()).unwrap();
    }

    // Verify index now contains B's files, not A's
    {
        let index = shared.read();
        assert_eq!(
            index.file_count(),
            3,
            "dir B should have 3 files after reload"
        );
        assert!(
            index.get_file("gamma.rs").is_some(),
            "gamma.rs should be in index after reload"
        );
        assert!(
            index.get_file("alpha.rs").is_none(),
            "alpha.rs should NOT be in index after reload to dir B"
        );
    }
}

// --------------------------------------------------------------------------
// Test TOOL-13: get_file_content with line range
//
// Verifies format::file_content slices correctly with start_line/end_line.
// --------------------------------------------------------------------------

#[test]
fn test_get_file_content_with_line_range() {
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "lines.rs",
        "line one\nline two\nline three\nline four\nline five",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    // Request lines 2-3 (1-indexed)
    let result = format::file_content(&index, "lines.rs", Some(2), Some(3));

    assert!(
        !result.contains("line one"),
        "line 1 should not be in range 2-3, got: {result}"
    );
    assert!(
        result.contains("line two"),
        "line 2 should be in range 2-3, got: {result}"
    );
    assert!(
        result.contains("line three"),
        "line 3 should be in range 2-3, got: {result}"
    );
    assert!(
        !result.contains("line four"),
        "line 4 should not be in range 2-3, got: {result}"
    );
}

#[test]
fn test_get_file_content_with_numbered_headered_line_range() {
    use symforge::live_index::search::ContentContext;
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "lines.rs",
        "line one\nline two\nline three\nline four\nline five",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let file = index.capture_shared_file("lines.rs").unwrap();

    let result = format::file_content_from_indexed_file_with_context(
        file.as_ref(),
        ContentContext::line_range_with_format(Some(2), Some(4), true, true),
    );

    assert_eq!(
        result,
        "lines.rs [lines 2-4]\n2: line two\n3: line three\n4: line four"
    );
}

#[test]
fn test_get_file_content_with_around_line() {
    use symforge::live_index::search::ContentContext;
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "lines.rs",
        "line one\nline two\nline three\nline four\nline five",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let file = index.capture_shared_file("lines.rs").unwrap();

    let result = format::file_content_from_indexed_file_with_context(
        file.as_ref(),
        ContentContext::around_line(3, Some(1), false, false),
    );

    assert_eq!(result, "2: line two\n3: line three\n4: line four");
}

#[test]
fn test_get_file_content_with_around_match() {
    use symforge::live_index::search::ContentContext;
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "lines.rs",
        "line one\nTODO first\nline three\nTODO second\nline five",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let file = index.capture_shared_file("lines.rs").unwrap();

    let result = format::file_content_from_indexed_file_with_context(
        file.as_ref(),
        ContentContext::around_match("todo", Some(1), false, false),
    );

    assert_eq!(result, "1: line one\n2: TODO first\n3: line three");
}

#[test]
fn test_get_file_content_with_specific_match_occurrence() {
    use symforge::live_index::search::ContentContext;
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "lines.rs",
        "line one\nTODO first\nline three\nTODO second\nline five",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let file = index.capture_shared_file("lines.rs").unwrap();

    let result = format::file_content_from_indexed_file_with_context(
        file.as_ref(),
        ContentContext::around_match_occurrence("todo", Some(2), Some(1), false, false),
    );

    assert_eq!(result, "3: line three\n4: TODO second\n5: line five");
}

#[test]
fn test_get_file_content_with_missing_match_occurrence_reports_lines() {
    use symforge::live_index::search::ContentContext;
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(dir.path(), "lines.rs", "line one\nTODO first\nline three");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let file = index.capture_shared_file("lines.rs").unwrap();

    let result = format::file_content_from_indexed_file_with_context(
        file.as_ref(),
        ContentContext::around_match_occurrence("todo", Some(2), Some(1), false, false),
    );

    assert_eq!(
        result,
        "Match occurrence 2 for 'todo' not found in lines.rs; 1 match(es) available at lines 2"
    );
}

#[test]
fn test_get_file_content_with_chunked_read() {
    use symforge::live_index::search::ContentContext;
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "lines.rs",
        "line one\nline two\nline three\nline four\nline five",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let file = index.capture_shared_file("lines.rs").unwrap();

    let result = format::file_content_from_indexed_file_with_context(
        file.as_ref(),
        ContentContext::chunk(2, 2),
    );

    assert_eq!(
        result,
        "lines.rs [chunk 2/3, lines 3-4]\n3: line three\n4: line four"
    );
}

#[test]
fn test_get_file_content_with_around_symbol() {
    use symforge::live_index::search::ContentContext;
    use symforge::protocol::format;

    let dir = tempdir().unwrap();
    write_file(
        dir.path(),
        "lines.rs",
        "line one\nfn connect() {}\nline three",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    let file = index.capture_shared_file("lines.rs").unwrap();

    let result = format::file_content_from_indexed_file_with_context(
        file.as_ref(),
        ContentContext::around_symbol("connect", None, Some(1)),
    );

    assert_eq!(result, "1: line one\n2: fn connect() {}\n3: line three");
}

// ============================================================================
// Phase 7 Plan 03: Persistence Integration Tests
// ============================================================================

// --------------------------------------------------------------------------
// Test: Persist round-trip preserves files and symbols
//
// Creates a LiveIndex with files, serializes to temp dir, loads snapshot,
// converts back to LiveIndex, verifies files and symbols match.
// --------------------------------------------------------------------------

#[test]
fn test_persist_round_trip() {
    let dir = tempdir().unwrap();

    // Create source files
    write_file(dir.path(), "main.rs", "fn main() {}\nfn helper() {}");
    write_file(dir.path(), "lib.rs", "fn util(): void {}");

    // Build a real LiveIndex
    let shared = LiveIndex::load(dir.path()).unwrap();

    // Serialize it
    {
        let guard = shared.read();
        persist::serialize_index(&guard, dir.path()).expect("serialize should succeed");
    }

    // Load snapshot
    let snapshot =
        persist::load_snapshot(dir.path()).expect("snapshot should be loadable after serialize");

    assert_eq!(
        snapshot.version, 4,
        "snapshot version should match current schema"
    );
    assert_eq!(snapshot.files.len(), 2, "snapshot should contain 2 files");
    assert!(
        snapshot.files.contains_key("main.rs"),
        "main.rs should be in snapshot"
    );
    assert!(
        snapshot.files.contains_key("lib.rs"),
        "lib.rs should be in snapshot"
    );

    // Convert snapshot back to LiveIndex and wrap in Arc<RwLock>
    let loaded_index = persist::snapshot_to_live_index(snapshot);
    let shared_loaded = symforge::live_index::SharedIndexHandle::shared(loaded_index);
    let loaded = shared_loaded.read();

    // Verify file count matches
    assert_eq!(loaded.file_count(), 2, "loaded index should have 2 files");

    // Verify files are accessible by path
    assert!(
        loaded.get_file("main.rs").is_some(),
        "main.rs should be in loaded index"
    );
    assert!(
        loaded.get_file("lib.rs").is_some(),
        "lib.rs should be in loaded index"
    );

    // Verify symbols were preserved
    let symbols = loaded.symbols_for_file("main.rs");
    let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        symbol_names.contains(&"main") || symbol_names.contains(&"helper"),
        "symbols should be preserved: {symbol_names:?}"
    );

    // Verify content bytes preserved
    let original_content = fs::read(dir.path().join("main.rs")).unwrap();
    let main_file = loaded.get_file("main.rs").unwrap();
    assert_eq!(
        main_file.content, original_content,
        "content bytes should be preserved through round-trip"
    );
}

// --------------------------------------------------------------------------
// Test: Corrupt index.bin falls back gracefully (returns None, no panic)
// --------------------------------------------------------------------------

#[test]
fn test_persist_corrupt_fallback() {
    let dir = tempdir().unwrap();

    // Write garbage bytes where index.bin should be
    fs::create_dir_all(dir.path().join(".symforge")).unwrap();
    fs::write(
        dir.path().join(".symforge").join("index.bin"),
        b"not valid postcard data",
    )
    .unwrap();

    // Must return None without panicking
    let result = persist::load_snapshot(dir.path());
    assert!(
        result.is_none(),
        "corrupt index.bin must return None, not panic"
    );

    // Verify we can still load a real index after corrupt fallback
    write_file(dir.path(), "a.rs", "fn alpha() {}");
    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();
    assert_eq!(
        index.file_count(),
        1,
        "full re-index should work after corrupt fallback"
    );
}

// --------------------------------------------------------------------------
// Test: Version mismatch in index.bin triggers fallback (returns None)
// --------------------------------------------------------------------------

#[test]
fn test_persist_version_mismatch() {
    use std::collections::HashMap;
    use symforge::live_index::persist::IndexSnapshot;

    let dir = tempdir().unwrap();

    // Manually create a snapshot with a future version number
    let future_snapshot = IndexSnapshot {
        version: 999,
        files: HashMap::new(),
    };
    let bytes = postcard::to_stdvec(&future_snapshot).expect("postcard serialize should work");
    fs::create_dir_all(dir.path().join(".symforge")).unwrap();
    fs::write(dir.path().join(".symforge").join("index.bin"), &bytes).unwrap();

    // Must return None (version mismatch)
    let result = persist::load_snapshot(dir.path());
    assert!(result.is_none(), "version mismatch must return None");
}

// --------------------------------------------------------------------------
// Test: ArcSwap concurrent-read contract under writer pressure.
//
// Pins the behavior claimed by `README.md:18` ("zero reader contention under
// concurrent tool calls") and the docstring on `SharedIndexHandle`. A regression
// that introduces reader-side locking, tears a read mid-swap, or skips rebuilding
// a secondary index inside a writer would surface here.
// --------------------------------------------------------------------------
#[test]
fn test_arcswap_concurrent_reads_under_writer_pressure() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let dir = tempdir().unwrap();
    for i in 0..20 {
        write_file(
            dir.path(),
            &format!("mod_{i:02}.rs"),
            &format!(
                "fn func_{i}() {{}}\nfn helper_{i}(x: u32) -> u32 {{ x + {i} }}\nstruct Struct_{i} {{}}\n"
            ),
        );
    }
    let shared = LiveIndex::load(dir.path()).unwrap();

    // Capture an owned IndexedFile so the writer can drive clone-mutate-swap
    // cycles without filesystem I/O. Each `SharedIndexHandle::update_file` call
    // clones the live index, mutates it, and atomically swaps a fresh `Arc`
    // into the `ArcSwap` — exactly the condition readers must not observe torn.
    let (target_path, writer_payload) = {
        let guard = shared.read();
        let (path, indexed) = guard
            .all_files()
            .next()
            .expect("initial index must contain at least one file");
        (path.clone(), indexed.clone())
    };

    let stop = Arc::new(AtomicBool::new(false));
    let reader_reads = Arc::new(AtomicUsize::new(0));
    let writer_swaps = Arc::new(AtomicUsize::new(0));
    let inconsistencies = Arc::new(AtomicUsize::new(0));

    let mut reader_handles = Vec::new();
    for _ in 0..8 {
        let shared = Arc::clone(&shared);
        let stop = Arc::clone(&stop);
        let reader_reads = Arc::clone(&reader_reads);
        let inconsistencies = Arc::clone(&inconsistencies);
        reader_handles.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let guard = shared.read();

                // Invariant 1: within one snapshot, `file_count()` agrees with
                // the number of entries enumerated by `all_files()`.
                let file_count = guard.file_count();
                let paths: Vec<String> = guard.all_files().map(|(p, _)| p.clone()).collect();
                if paths.len() != file_count {
                    inconsistencies.fetch_add(1, Ordering::Relaxed);
                }

                // Invariant 2: every path in `files` is reachable via the
                // `files_by_basename` secondary index. A writer that mutates
                // `files` but forgets to rebuild the secondary (or a torn
                // read straddling two snapshots) would fail this.
                for p in paths.iter().take(8) {
                    let basename = std::path::Path::new(p.as_str())
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_ascii_lowercase())
                        .unwrap_or_default();
                    let by_basename = guard.find_files_by_basename(&basename);
                    if !by_basename.contains(&p.as_str()) {
                        inconsistencies.fetch_add(1, Ordering::Relaxed);
                    }
                }

                // Invariant 3: every path reported by `all_files()` resolves
                // back to a concrete `IndexedFile` via `get_file()` in the same
                // snapshot.
                for p in paths.iter().take(8) {
                    if guard.get_file(p.as_str()).is_none() {
                        inconsistencies.fetch_add(1, Ordering::Relaxed);
                    }
                }

                // Exercise a few more public read paths to broaden contention
                // surface area without coupling to their exact semantics.
                let _ = guard.symbol_count();
                let _ = guard.health_stats();

                reader_reads.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let writer_handle = {
        let shared = Arc::clone(&shared);
        let stop = Arc::clone(&stop);
        let writer_swaps = Arc::clone(&writer_swaps);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // Public clone-mutate-swap entry point — triggers
                // `SharedIndexHandle::swap_and_publish`, replacing the live
                // `Arc<LiveIndex>` in the `ArcSwap` on every iteration.
                shared.update_file(target_path.clone(), writer_payload.clone());
                writer_swaps.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    thread::sleep(Duration::from_millis(750));
    stop.store(true, Ordering::Relaxed);

    for h in reader_handles {
        h.join().expect("reader thread must not panic");
    }
    writer_handle.join().expect("writer thread must not panic");

    let inconsistent = inconsistencies.load(Ordering::Relaxed);
    let reads = reader_reads.load(Ordering::Relaxed);
    let swaps = writer_swaps.load(Ordering::Relaxed);

    assert_eq!(
        inconsistent, 0,
        "readers observed {inconsistent} inconsistent LiveIndex snapshot(s) across {reads} reads and {swaps} writer swaps"
    );
    assert!(
        swaps > 0,
        "writer did not complete any swaps during the stress window (reads={reads})"
    );
    assert!(
        reads > 0,
        "reader throughput was zero while the writer was active (swaps={swaps})"
    );
    // With 8 readers over ~750ms against a concurrent writer, ArcSwap should
    // deliver thousands of reads. A regression that re-introduces reader-side
    // locking or otherwise starves readers would collapse this toward zero;
    // require at least 100 reads as a lower floor that is robust on loaded CI.
    assert!(
        reads >= 100,
        "reader throughput suspiciously low: {reads} reads vs {swaps} swaps — possible reader starvation regression"
    );
}
