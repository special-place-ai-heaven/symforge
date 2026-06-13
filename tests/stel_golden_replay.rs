//! S4 exit validation — replay five ask-aligned golden rows on the compact `symforge` path.
#![cfg(feature = "server")]
#![allow(unsafe_code)] // test-only SYMFORGE_SURFACE guard (serialized by COMPACT_ENV_LOCK)

use std::collections::BTreeMap;
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

fn corpus_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

fn corpus_available(relative: &str, marker: &str) -> bool {
    corpus_path(relative).join(marker).is_file()
}

fn s4_corpora_available() -> bool {
    corpus_available(stel::S4_REPLAY_CORPUS, "src/lib.rs")
        && corpus_available(
            "tests/fixtures/phase0-corpus/records-python",
            "records.py",
        )
        && corpus_available(
            "tests/fixtures/compression_ratio/rust",
            "service.rs",
        )
}

fn golden_fixture_path() -> PathBuf {
    repo_root().join(stel::GOLDEN_ROUTES_FIXTURE)
}

fn tool_result_text(result: &serde_json::Value) -> &str {
    result["content"][0]["text"]
        .as_str()
        .expect("symforge result must contain text content")
}

fn ask_routed_tool(query: &str) -> String {
    use symforge::protocol::smart_query;
    let intent = smart_query::classify_intent(query.trim());
    smart_query::route_tool_name(&intent).to_string()
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

#[test]
fn find_s4_rows_matching_ask_routing() {
    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let matching: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.must_call.len() == 1
                && row.chain.as_deref() == Some("single")
                && row.expected_decision == stel::AdmissionDecision::Serve
                && ask_routed_tool(&row.query) == row.must_call[0]
        })
        .collect();
    eprintln!(
        "ask-aligned single-hop serve rows ({}): {:?}",
        matching.len(),
        matching.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
    );
    assert!(
        matching.len() >= 5,
        "need at least 5 ask-aligned rows for S4 exit replay"
    );
    for id in stel::S4_EXIT_ROW_IDS {
        assert!(
            matching.iter().any(|row| row.id == id),
            "S4_EXIT_ROW_IDS must stay ask-aligned; missing or stale: {id}"
        );
    }
}

#[test]
fn s4_exit_rows_align_with_ask_routing() {
    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let mut mismatches = Vec::new();
    for row in stel::s4_exit_rows(&rows) {
        let routed = ask_routed_tool(&row.query);
        if row.must_call.first().map(String::as_str) != Some(routed.as_str()) {
            mismatches.push(format!(
                "{}: golden `{}` vs ask `{}` for {:?}",
                row.id,
                row.must_call.first().map(String::as_str).unwrap_or("?"),
                routed,
                row.query
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "S4 exit rows must match current ask routing (L1 planner not wired yet):\n{}",
        mismatches.join("\n")
    );
}

#[tokio::test]
async fn s4_golden_rows_replay_on_compact_symforge() {
    if !s4_corpora_available() {
        eprintln!(
            "skip s4_golden_rows_replay_on_compact_symforge: clone phase0 corpora per tests/fixtures/phase0-corpus/README.md"
        );
        return;
    }

    let _guard = COMPACT_ENV_LOCK.lock().expect("env lock");
    let _surface = EnvVarGuard::set("SYMFORGE_SURFACE", "compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("load golden fixture");
    let exit_rows = stel::s4_exit_rows(&rows);

    let mut by_corpus: BTreeMap<&str, Vec<&GoldenRouteRow>> = BTreeMap::new();
    for row in exit_rows {
        by_corpus
            .entry(stel::corpus_for_row_id(&row.id))
            .or_default()
            .push(row);
    }

    let mut failures = Vec::new();
    for (corpus, corpus_rows) in by_corpus {
        let project = Path::new(corpus)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("replay");
        let server = server_for_corpus(corpus, project);
        for row in corpus_rows {
            let output = replay_row(&server, row).await;
            let validation = stel::validate_s4_replay_output(row, &output);
            if !validation.passed {
                let tail: String = output
                    .chars()
                    .rev()
                    .take(400)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                failures.push(format!(
                    "{}: {}\n--- output tail ---\n{tail}",
                    validation.row_id,
                    validation.errors.join("; "),
                ));
            }
        }
    }

    drop(_guard);

    assert!(
        failures.is_empty(),
        "S4 golden replay failures:\n{}",
        failures.join("\n\n")
    );
}
