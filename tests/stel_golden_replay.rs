//! Golden route replay — classify and replay supported rows on compact `symforge`.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::stel::{self, GoldenRouteRow};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn corpus_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

fn corpus_available(relative: &str, marker: &str) -> bool {
    corpus_path(relative).join(marker).is_file()
}

fn all_replay_corpora_available() -> bool {
    corpus_available(stel::S4_REPLAY_CORPUS, "src/lib.rs")
        && corpus_available("tests/fixtures/phase0-corpus/records-python", "records.py")
        && corpus_available("tests/fixtures/phase0-corpus/is-plain-obj-ts", "index.js")
        && corpus_available("tests/fixtures/compression_ratio/rust", "service.rs")
}

fn corpus_available_for_row(row: &GoldenRouteRow) -> bool {
    let corpus = stel::corpus_for_row_id(&row.id);
    let marker = stel::corpus_marker_for_row_id(&row.id);
    corpus_available(corpus, marker)
}

fn golden_fixture_path() -> PathBuf {
    repo_root().join(stel::GOLDEN_ROUTES_FIXTURE)
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

#[test]
fn golden_corpus_classification_has_zero_deferred_multi_hop() {
    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let classification = stel::classify_golden_corpus(&rows);
    assert_eq!(classification.row_count(), rows.len());
    assert!(
        classification.deferred_multi_hop.is_empty(),
        "Phase 2 closes multi-hop deferrals: {:?}",
        classification.deferred_multi_hop
    );
    for id in stel::MULTI_HOP_GOLDEN_ROW_IDS {
        assert!(
            classification
                .supported_serve
                .iter()
                .any(|row_id| row_id == id),
            "multi-hop row {id} must classify as supported serve"
        );
    }
    assert!(
        classification.deferred_planner_mismatch.is_empty(),
        "planner mismatches must be empty or listed explicitly in deferred_planner_mismatch_ids_are_stable"
    );
    for id in stel::S4_EXIT_ROW_IDS {
        assert!(
            classification
                .supported_serve
                .iter()
                .any(|row_id| row_id == id),
            "S4 minimum subset must remain supported: {id}"
        );
    }
}

#[test]
fn s4_exit_rows_align_with_planner_routing() {
    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let mut mismatches = Vec::new();
    for row in stel::s4_exit_rows(&rows) {
        let request = stel::request_for_golden_row(row);
        let plan = stel::build_plan(&request);
        if row.must_call.first().map(String::as_str) != Some(plan.steps[0].tool.as_str()) {
            mismatches.push(format!(
                "{}: golden `{}` vs planner `{}` for {:?}",
                row.id,
                row.must_call.first().map(String::as_str).unwrap_or("?"),
                plan.steps[0].tool,
                row.query
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "S4 exit rows must match L1 planner routing:\n{}",
        mismatches.join("\n")
    );
}

#[tokio::test]
async fn s4_minimum_subset_replays_on_compact_symforge() {
    if !all_replay_corpora_available() {
        eprintln!(
            "skip s4_minimum_subset_replays: clone phase0 corpora per tests/fixtures/phase0-corpus/README.md"
        );
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: the replay validators assert the full
    // contract (`── stel ──`, `decision: serve|bypass`, `ledger:`), which the
    // live serve path generates only in full mode.
    let _full = stel_surface_env::force_full_stel_envelope();

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("load golden fixture");
    let exit_rows = stel::s4_exit_rows(&rows);
    replay_serve_rows_grouped_by_corpus(&exit_rows).await;
}

#[tokio::test]
async fn multi_hop_golden_rows_replay_on_compact_symforge() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: the replay validators assert the full
    // contract (`── stel ──`, `decision: serve|bypass`, `ledger:`), which the
    // live serve path generates only in full mode.
    let _full = stel_surface_env::force_full_stel_envelope();

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("load golden fixture");
    let mut missing = Vec::new();
    let multi_hop: Vec<_> = stel::MULTI_HOP_GOLDEN_ROW_IDS
        .iter()
        .map(|id| {
            rows.iter()
                .find(|row| row.id == *id)
                .unwrap_or_else(|| panic!("golden corpus missing multi-hop row `{id}`"))
        })
        .filter(|row| {
            let corpus = stel::multi_hop_replay_corpus_for_row_id(&row.id);
            let marker = stel::multi_hop_replay_corpus_marker(&row.id);
            let path = corpus_path(corpus).join(marker);
            if path.is_file() {
                true
            } else {
                missing.push(format!("{} (expected {})", row.id, path.display()));
                false
            }
        })
        .collect();
    assert!(
        missing.is_empty(),
        "multi-hop replay requires checked-in fixtures under tests/fixtures/stel_multi_hop/: {}",
        missing.join(", ")
    );
    assert_eq!(
        multi_hop.len(),
        stel::MULTI_HOP_GOLDEN_ROW_IDS.len(),
        "all multi-hop golden rows must replay"
    );

    let mut failures = Vec::new();
    for row in multi_hop {
        let corpus = stel::multi_hop_replay_corpus_for_row_id(&row.id);
        let project = Path::new(corpus)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("replay");
        let server = server_for_corpus(corpus, project);
        let output = replay_row(&server, row).await;
        let validation = stel::validate_serve_replay_output(row, &output);
        if !validation.passed {
            failures.push(format!(
                "{}: {}",
                validation.row_id,
                validation.errors.join("; ")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "multi-hop golden replay failures:\n{}",
        failures.join("\n")
    );
}

#[tokio::test]
async fn supported_serve_rows_replay_with_envelope_and_ledger() {
    if !all_replay_corpora_available() {
        eprintln!("skip supported_serve_rows_replay: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: the replay validators assert the full
    // contract (`── stel ──`, `decision: serve|bypass`, `ledger:`), which the
    // live serve path generates only in full mode.
    let _full = stel_surface_env::force_full_stel_envelope();

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("load golden fixture");
    let serve_rows: Vec<_> = stel::supported_serve_rows(&rows)
        .into_iter()
        .filter(|row| corpus_available_for_row(row))
        .collect();
    assert!(
        serve_rows.len() >= stel::S4_EXIT_ROW_IDS.len(),
        "supported serve replay must be broader than the S4 minimum subset"
    );
    replay_serve_rows_grouped_by_corpus(&serve_rows).await;
}

#[tokio::test]
async fn supported_pff_rows_bypass_without_legacy_execution() {
    if !all_replay_corpora_available() {
        eprintln!("skip supported_pff_rows_bypass: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    // Force the FULL trust envelope: the replay validators assert the full
    // contract (`── stel ──`, `decision: serve|bypass`, `ledger:`), which the
    // live serve path generates only in full mode.
    let _full = stel_surface_env::force_full_stel_envelope();

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("load golden fixture");
    let pff_rows: Vec<_> = stel::supported_pff_rows(&rows)
        .into_iter()
        .filter(|row| corpus_available_for_row(row))
        .collect();
    assert_eq!(pff_rows.len(), 4, "all four P-FF golden rows must replay");

    let mut by_corpus: BTreeMap<&str, Vec<&GoldenRouteRow>> = BTreeMap::new();
    for row in pff_rows {
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
            let validation = stel::validate_pff_replay_output(row, &output);
            if !validation.passed {
                failures.push(format!(
                    "{}: {}",
                    validation.row_id,
                    validation.errors.join("; ")
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "P-FF golden replay failures:\n{}",
        failures.join("\n")
    );
}

async fn replay_serve_rows_grouped_by_corpus(rows: &[&GoldenRouteRow]) {
    let mut by_corpus: BTreeMap<&str, Vec<&GoldenRouteRow>> = BTreeMap::new();
    for row in rows {
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
            let validation = stel::validate_serve_replay_output(row, &output);
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

    assert!(
        failures.is_empty(),
        "serve golden replay failures:\n{}",
        failures.join("\n\n")
    );
}
