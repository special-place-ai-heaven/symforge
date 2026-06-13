//! L3 enforcement — P-FF bypass skips legacy tool dispatch; serve rows still execute.
#![cfg(feature = "server")]
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::stel::{self, GoldenRouteRow};

static COMPACT_ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => unsafe {
                std::env::set_var(self.key, previous);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn golden_fixture_path() -> PathBuf {
    repo_root().join(stel::GOLDEN_ROUTES_FIXTURE)
}

fn corpus_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

fn corpus_available(relative: &str, marker: &str) -> bool {
    corpus_path(relative).join(marker).is_file()
}

fn l3_corpora_available() -> bool {
    corpus_available(stel::S4_REPLAY_CORPUS, "src/lib.rs")
        && corpus_available("tests/fixtures/phase0-corpus/records-python", "records.py")
        && corpus_available("tests/fixtures/compression_ratio/rust", "service.rs")
}

fn tool_result_text(result: &serde_json::Value) -> &str {
    result["content"][0]["text"]
        .as_str()
        .expect("symforge result must contain text content")
}

fn server_for_corpus(relative: &str, project: &str) -> SymForgeServer {
    let root = corpus_path(relative);
    let shared = LiveIndex::load(&root).unwrap_or_else(|error| {
        panic!("index {}: {error}", root.display());
    });
    SymForgeServer::new(
        shared,
        project.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root),
        None,
    )
}

async fn replay_row(server: &SymForgeServer, row: &GoldenRouteRow) -> String {
    let request = row.to_request();
    let params = serde_json::to_value(stel::SymforgeCallInput {
        request,
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("symforge params serialize");
    let result = server
        .dispatch_tool_result_for_tests("symforge", params)
        .await
        .expect("symforge dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize CallToolResult");
    tool_result_text(&serialized).to_string()
}

fn row_by_id<'a>(rows: &'a [GoldenRouteRow], id: &str) -> &'a GoldenRouteRow {
    rows.iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("missing golden row {id}"))
}

#[tokio::test]
async fn pff_golden_rows_bypass_without_legacy_tool_execution() {
    if !l3_corpora_available() {
        eprintln!(
            "skip pff_golden_rows_bypass: clone phase0 corpora per tests/fixtures/phase0-corpus/README.md"
        );
        return;
    }

    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let pff_ids = [
        "cfg-if/pff_whole_lib",
        "records/pff_whole_module",
        "compression/pff_whole_service",
    ];

    let mut failures = Vec::new();
    for id in pff_ids {
        let row = row_by_id(&rows, id);
        let corpus = stel::corpus_for_row_id(id);
        let project = Path::new(corpus)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("replay");
        let server = server_for_corpus(corpus, project);
        let output = replay_row(&server, row).await;

        if !output.starts_with("── stel ──") {
            failures.push(format!("{id}: missing STEL envelope"));
            continue;
        }
        if !output.contains("decision: bypass") {
            failures.push(format!("{id}: expected decision: bypass"));
        }
        if !output.contains("did not execute a legacy tool") {
            failures.push(format!("{id}: missing bypass execution guard message"));
        }
        if !output.contains("Host read:") {
            failures.push(format!("{id}: missing host-read instruction"));
        }
        if !output.contains("(whole file)") {
            failures.push(format!(
                "{id}: P-FF bypass must request whole-file host read"
            ));
        }
        if output.contains("lines 1-50") {
            failures.push(format!(
                "{id}: P-FF bypass must not cap host read at lines 1-50"
            ));
        }
        if output.contains("Chosen tool:") {
            failures.push(format!(
                "{id}: must not dispatch legacy tool (Chosen tool present)"
            ));
        }
        for tool in &row.must_not_call {
            if output.contains(&format!("Chosen tool: {tool}")) {
                failures.push(format!("{id}: must_not_call violated: {tool}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "P-FF L3 enforcement failures:\n{}",
        failures.join("\n")
    );
}

#[tokio::test]
async fn serve_golden_row_still_executes_legacy_tool() {
    if !l3_corpora_available() {
        eprintln!("skip serve_golden_row_still_executes: missing corpora");
        return;
    }

    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/t4_refs");
    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "cfg-if-rust");
    let output = replay_row(&server, row).await;

    assert!(
        output.contains("decision: serve"),
        "output tail: {}",
        &output[output.len().saturating_sub(200)..]
    );
    assert!(
        output.contains("Chosen tool: find_references"),
        "serve path must still execute planned legacy tool"
    );
    let validation = stel::validate_s4_replay_output(row, &output);
    assert!(validation.passed, "{:?}", validation.errors);
}
