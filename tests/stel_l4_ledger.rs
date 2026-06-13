//! L4 ledger — serve and P-FF bypass invocations record decision/execution metadata.
#![cfg(feature = "server")]
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::stel::{self, AdmissionDecision, GoldenRouteRow};

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

fn corpora_available() -> bool {
    corpus_available(stel::S4_REPLAY_CORPUS, "src/lib.rs")
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

fn ledger_meta_line(output: &str) -> &str {
    output
        .lines()
        .find(|line| line.starts_with("ledger: "))
        .expect("ledger line in envelope")
}

#[tokio::test]
async fn serve_row_records_ledger_with_legacy_execution() {
    if !corpora_available() {
        eprintln!("skip serve_row_records_ledger: missing corpora");
        return;
    }

    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/t4_refs");
    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "cfg-if-rust");
    let output = replay_row(&server, row).await;

    assert!(output.contains("ledger: "));
    let ledger_json = ledger_meta_line(&output).trim_start_matches("ledger: ");
    let meta: stel::LedgerEnvelopeMeta =
        serde_json::from_str(ledger_json).expect("ledger json");
    assert_eq!(meta.decision, "serve");
    assert!(!meta.bypass);
    assert!(meta.legacy_executed);
    assert_eq!(meta.route_tool, "find_references");
    assert!(meta.schema_tokens > 0);
    assert!(meta.invoke_tokens > 0);
    assert!(meta.output_bytes > 0);

    let event = server.stel_ledger().lock().last().expect("ledger event");
    assert_eq!(event.decision, AdmissionDecision::Serve);
    assert_eq!(event.tools_called, vec!["find_references".to_string()]);
}

#[tokio::test]
async fn pff_row_records_ledger_without_legacy_execution() {
    if !corpora_available() {
        eprintln!("skip pff_row_records_ledger: missing corpora");
        return;
    }

    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/pff_whole_lib");
    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "cfg-if-rust");
    let output = replay_row(&server, row).await;

    assert!(output.contains("ledger: "));
    let ledger_json = ledger_meta_line(&output).trim_start_matches("ledger: ");
    let meta: stel::LedgerEnvelopeMeta =
        serde_json::from_str(ledger_json).expect("ledger json");
    assert_eq!(meta.decision, "bypass");
    assert!(meta.bypass);
    assert!(!meta.legacy_executed);
    assert!(meta.predicted_net != 0);

    let event = server.stel_ledger().lock().last().expect("ledger event");
    assert_eq!(event.decision, AdmissionDecision::Bypass);
    assert!(event.tools_called.is_empty());
}
