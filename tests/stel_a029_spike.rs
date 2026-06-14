//! A-029 T2 equivalence spike integration tests (P2-S5 evidence).
#![cfg(feature = "server")]

use std::path::PathBuf;

use symforge::stel::{A029SpikeResults, A029Verdict, T2Equivalence, normalize_spike_results};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_results_path() -> PathBuf {
    repo_root().join("docs/research/a029-t2-results.json")
}

fn load_committed_spike_results() -> Option<A029SpikeResults> {
    let path = fixture_results_path();
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

#[test]
fn normalize_spike_results_matches_gate_contract_threshold() {
    let results = A029SpikeResults {
        measured_at: String::new(),
        surface: "compact".to_string(),
        method: "test".to_string(),
        baseline_commit: String::new(),
        rows: vec![],
        t2_equiv_pass: 0,
        t2_tasks_total: 0,
        verdict: symforge::stel::A029Verdict::Kill,
        pivot_policy: None,
        notes: String::new(),
    };
    let normalized = normalize_spike_results(results);
    assert_eq!(normalized.t2_tasks_total, 0);
    assert_eq!(normalized.verdict, A029Verdict::Kill);
}

#[test]
fn committed_a029_artifact_has_four_rows_and_truthful_verdict() {
    let Some(results) = load_committed_spike_results() else {
        eprintln!(
            "skip committed_a029_artifact: missing {}; run scripts/a029-t2-spike.cjs",
            fixture_results_path().display()
        );
        return;
    };
    assert_eq!(results.t2_tasks_total, 4, "A-029 requires 4 T2 tasks");
    assert_eq!(results.rows.len(), 4);
    let normalized = normalize_spike_results(results.clone());
    assert_eq!(normalized.t2_equiv_pass, results.t2_equiv_pass);
    assert_eq!(normalized.verdict.as_str(), results.verdict.as_str());
    for row in &normalized.rows {
        assert!(
            matches!(
                row.equivalence,
                T2Equivalence::Equivalent
                    | T2Equivalence::SymforgeLess
                    | T2Equivalence::NotEquivalent
                    | T2Equivalence::Bypass
            ),
            "row {} has classifiable equivalence",
            row.id
        );
    }
}

#[test]
fn a029_verdict_is_pass_pivot_or_kill_only() {
    let Some(results) = load_committed_spike_results() else {
        eprintln!("skip a029_verdict_is_pass_pivot_or_kill_only: missing artifact");
        return;
    };
    match results.verdict {
        A029Verdict::Pass => {
            assert!(
                results.t2_equiv_pass >= symforge::stel::A029_T2_PASS_THRESHOLD,
                "PASS requires >=2 equiv"
            );
        }
        A029Verdict::Pivot => {
            assert!(results.t2_equiv_pass < symforge::stel::A029_T2_PASS_THRESHOLD);
            assert!(results.pivot_policy.is_some());
        }
        A029Verdict::Kill => {
            assert!(results.t2_equiv_pass < symforge::stel::A029_T2_PASS_THRESHOLD);
        }
    }
}
