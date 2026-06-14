//! Phase 2 compact-surface battery gates — deterministic H3/H4/H5 computation.
#![cfg(feature = "server")]

use std::path::PathBuf;
use std::process::Command;

use symforge::stel::{
    self, GateStatus, compute_phase2_gates, normalize_battery_results, phase2_minimum_gates_pass,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures/phase2-gate").join(name)
}

fn load_fixture(name: &str) -> stel::BatteryResults {
    let text = std::fs::read_to_string(fixture_path(name)).expect("fixture readable");
    serde_json::from_str(&text).expect("fixture json")
}

#[test]
fn synthetic_pass_fixture_computes_h3_h4_h5_pass() {
    let results = normalize_battery_results(load_fixture("synthetic-pass.json"));
    let report = compute_phase2_gates(&results);
    assert_eq!(report.gates.H3, GateStatus::Pass);
    assert_eq!(report.gates.H4, GateStatus::Pass);
    assert_eq!(report.gates.H5, GateStatus::Pass);
    assert!(phase2_minimum_gates_pass(&report));
    assert_eq!(report.h3_scope_row_count, 2);
    assert!(report.session_net_accepted > 0);
}

#[test]
fn synthetic_h3_fail_detects_s_gte_m_on_small_serve_row() {
    let results = normalize_battery_results(load_fixture("synthetic-h3-fail.json"));
    let report = compute_phase2_gates(&results);
    assert_eq!(report.gates.H3, GateStatus::Fail);
    assert_eq!(report.h3_small_serve_s_gte_m_count, 1);
}

#[test]
fn synthetic_h4_fail_detects_negative_session_net_accepted() {
    let results = normalize_battery_results(load_fixture("synthetic-h4-fail.json"));
    let report = compute_phase2_gates(&results);
    assert_eq!(report.gates.H4, GateStatus::Fail);
    assert!(report.session_net_accepted < 0);
}

#[test]
fn compare_results_script_matches_rust_gate_computation() {
    let fixture = fixture_path("synthetic-pass.json");
    let output = Command::new("node")
        .arg("scripts/compare-results.cjs")
        .arg(&fixture)
        .current_dir(repo_root())
        .output()
        .expect("spawn compare-results");
    assert!(
        output.status.success(),
        "compare-results failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("json stdout");
    assert_eq!(parsed["gates"]["H3"], "PASS");
    assert_eq!(parsed["gates"]["H4"], "PASS");
    assert_eq!(parsed["gates"]["H5"], "PASS");
}

#[test]
fn compare_results_script_fails_on_h4_negative_net() {
    let fixture = fixture_path("synthetic-h4-fail.json");
    let output = Command::new("node")
        .arg("scripts/compare-results.cjs")
        .arg(&fixture)
        .current_dir(repo_root())
        .output()
        .expect("spawn compare-results");
    assert!(!output.status.success());
    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("json stdout");
    assert_eq!(parsed["gates"]["H4"], "FAIL");
}

#[test]
fn battery_rows_require_stel_extension_fields_in_pass_fixture() {
    let results = load_fixture("synthetic-pass.json");
    for row in &results.rows {
        let stel = row.stel.as_ref().expect("stel block present");
        assert!(!stel.plan_id.is_empty());
        assert!(!stel.decision.is_empty());
        assert!(!stel.route_confidence.is_empty());
    }
}

#[test]
fn live_candidate_artifact_computes_gates_deterministically() {
    let path = repo_root().join("docs/research/results-v8-phase2-candidate.json");
    if !path.is_file() {
        eprintln!("skip live_candidate_artifact: run phase2-compact-battery first");
        return;
    }
    let results = normalize_battery_results(load_fixture_from_path(path));
    assert_eq!(results.rowCount, 36);
    let report = compute_phase2_gates(&results);
    assert_eq!(report.gates.H4, GateStatus::Pass);
    assert_eq!(report.gates.H5, GateStatus::Pass);
    assert!(report.session_net_accepted >= 0);
    for row in &results.rows {
        assert!(row.stel.is_some(), "missing stel block on {}", row.id);
    }
}

fn load_fixture_from_path(path: PathBuf) -> stel::BatteryResults {
    let text = std::fs::read_to_string(path).expect("candidate readable");
    serde_json::from_str(&text).expect("candidate json")
}
