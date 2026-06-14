//! A-029 T2 equivalence spike — verdict computation (P2-S5 evidence only).

use serde::{Deserialize, Serialize};

/// Minimum T2 equivalence count for A-029 PASS (tokio + django program).
pub const A029_T2_PASS_THRESHOLD: usize = 2;

/// Spike verdict per gate evidence contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum A029Verdict {
    Pass,
    Pivot,
    Kill,
}

impl A029Verdict {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Pivot => "PIVOT",
            Self::Kill => "KILL",
        }
    }
}

/// T2 row equivalence class for A-029 spike rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum T2Equivalence {
    Equivalent,
    #[serde(alias = "SYMFORGE-LESS")]
    SymforgeLess,
    NotEquivalent,
    Bypass,
    Pending,
}

impl T2Equivalence {
    pub const fn is_equiv(self) -> bool {
        matches!(self, Self::Equivalent)
    }
}

/// One measured A-029 T2 spike row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct A029T2Row {
    pub id: String,
    pub repo: String,
    pub query: String,
    pub symbol: String,
    pub decision: String,
    #[serde(default)]
    pub tools_called: Vec<String>,
    pub equivalence: T2Equivalence,
    #[serde(default)]
    pub baseline_paths: usize,
    #[serde(default)]
    pub matched_paths: usize,
    #[serde(default)]
    pub baseline_recall: f32,
    #[serde(default)]
    pub min_baseline_recall: f32,
    #[serde(default)]
    pub chain_failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<String>,
}

/// Machine-readable A-029 spike output (`docs/research/a029-t2-results.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A029SpikeResults {
    #[serde(default)]
    pub measured_at: String,
    #[serde(default)]
    pub surface: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub baseline_commit: String,
    pub rows: Vec<A029T2Row>,
    pub t2_equiv_pass: usize,
    pub t2_tasks_total: usize,
    pub verdict: A029Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot_policy: Option<String>,
    #[serde(default)]
    pub notes: String,
}

/// Classify one spike row from measured fields (deterministic; no runtime I/O).
pub fn classify_t2_equivalence(row: &A029T2Row) -> T2Equivalence {
    if row.decision == "bypass" {
        return T2Equivalence::Bypass;
    }
    if row.chain_failed {
        return T2Equivalence::SymforgeLess;
    }
    if row.decision == "reject" || row.tools_called.is_empty() {
        return T2Equivalence::NotEquivalent;
    }
    let routed_refs = row
        .tools_called
        .iter()
        .any(|tool| tool == "find_references");
    if !routed_refs {
        return T2Equivalence::NotEquivalent;
    }
    if row.baseline_paths == 0 {
        return T2Equivalence::Pending;
    }
    if row.baseline_recall >= row.min_baseline_recall {
        T2Equivalence::Equivalent
    } else {
        T2Equivalence::SymforgeLess
    }
}

/// Recompute equivalence classes and aggregate counts on spike results.
pub fn normalize_spike_results(mut results: A029SpikeResults) -> A029SpikeResults {
    for row in &mut results.rows {
        row.equivalence = classify_t2_equivalence(row);
    }
    results.t2_tasks_total = results.rows.len();
    results.t2_equiv_pass = results
        .rows
        .iter()
        .filter(|row| row.equivalence.is_equiv())
        .count();
    results.verdict = evaluate_a029_verdict(results.t2_equiv_pass, results.t2_tasks_total);
    if results.verdict == A029Verdict::Pivot {
        results.pivot_policy = Some(
            "P-T2 bypass-only for reference tasks (grep envelope; eligible_h6=false)".to_string(),
        );
    }
    results
}

/// Evaluate A-029 verdict from T2 equivalence count.
pub fn evaluate_a029_verdict(t2_equiv_pass: usize, t2_tasks_total: usize) -> A029Verdict {
    if t2_equiv_pass >= A029_T2_PASS_THRESHOLD {
        A029Verdict::Pass
    } else if t2_tasks_total >= A029_T2_PASS_THRESHOLD {
        A029Verdict::Pivot
    } else {
        A029Verdict::Kill
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row(id: &str, recall: f32, decision: &str) -> A029T2Row {
        A029T2Row {
            id: id.to_string(),
            repo: id.split('/').next().unwrap_or("").to_string(),
            query: "who references X".to_string(),
            symbol: "X".to_string(),
            decision: decision.to_string(),
            tools_called: vec!["find_references".to_string()],
            equivalence: T2Equivalence::Pending,
            baseline_paths: 10,
            matched_paths: (recall * 10.0) as usize,
            baseline_recall: recall,
            min_baseline_recall: 0.35,
            chain_failed: false,
            diagnostics: None,
        }
    }

    #[test]
    fn pass_when_two_of_four_equiv() {
        let results = A029SpikeResults {
            measured_at: String::new(),
            surface: "compact".to_string(),
            method: "test".to_string(),
            baseline_commit: String::new(),
            rows: vec![
                sample_row("tokio/a", 0.5, "serve"),
                sample_row("tokio/b", 0.5, "serve"),
                sample_row("django/a", 0.1, "serve"),
                sample_row("django/b", 0.1, "serve"),
            ],
            t2_equiv_pass: 0,
            t2_tasks_total: 4,
            verdict: A029Verdict::Kill,
            pivot_policy: None,
            notes: String::new(),
        };
        let normalized = normalize_spike_results(results);
        assert_eq!(normalized.t2_equiv_pass, 2);
        assert_eq!(normalized.verdict, A029Verdict::Pass);
    }

    #[test]
    fn pivot_when_one_of_four_equiv() {
        let results = A029SpikeResults {
            measured_at: String::new(),
            surface: "compact".to_string(),
            method: "test".to_string(),
            baseline_commit: String::new(),
            rows: vec![sample_row("tokio/a", 0.5, "serve")],
            t2_equiv_pass: 0,
            t2_tasks_total: 1,
            verdict: A029Verdict::Kill,
            pivot_policy: None,
            notes: String::new(),
        };
        let mut results = results;
        results.rows.extend([
            sample_row("tokio/b", 0.1, "serve"),
            sample_row("django/a", 0.1, "serve"),
            sample_row("django/b", 0.1, "serve"),
        ]);
        let normalized = normalize_spike_results(results);
        assert_eq!(normalized.t2_equiv_pass, 1);
        assert_eq!(normalized.verdict, A029Verdict::Pivot);
        assert!(normalized.pivot_policy.is_some());
    }

    #[test]
    fn symforge_less_when_routing_misses_find_references() {
        let mut row = sample_row("tokio/a", 0.9, "serve");
        row.tools_called = vec!["search_text".to_string()];
        assert_eq!(classify_t2_equivalence(&row), T2Equivalence::NotEquivalent);
    }
}
