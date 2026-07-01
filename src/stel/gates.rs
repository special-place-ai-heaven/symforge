//! Phase 2 compact-surface battery gate computation (H3/H4/H5).
//!
//! Formulas match [`docs/v8-gap-closure-plan.md`](../../docs/v8-gap-closure-plan.md) §5.1
//! with A-012 serve-only H3 scope when no `*_small` rows are present.

#![allow(non_snake_case)] // sf-bench / compare-results JSON field names

use serde::{Deserialize, Serialize};

/// STEL extension block required on each battery row (Phase 2 contract).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatteryRowStel {
    pub plan_id: String,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools_called: Vec<String>,
    pub predicted_tokens: u32,
    pub actual_tokens: u32,
    pub net_vs_manual: i32,
    pub route_confidence: String,
}

/// One measured compact-surface battery row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatteryRow {
    pub id: String,
    #[serde(default)]
    pub corpus: String,
    pub S: u32,
    pub M: u32,
    pub sGteM: bool,
    pub acceptedServe: bool,
    pub equivalence: String,
    #[serde(default)]
    pub goldenId: String,
    pub decision: String,
    #[serde(default = "default_chain")]
    pub chain: String,
    pub mcpCalls: u32,
    pub eligibleH6: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stel: Option<BatteryRowStel>,
}

fn default_chain() -> String {
    "single".to_string()
}

/// sf-bench-style battery output consumed by compare-results.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatteryResults {
    #[serde(default)]
    pub measuredAt: String,
    #[serde(default)]
    pub symforgeBin: String,
    #[serde(default)]
    pub surface: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub baselineCommit: String,
    pub rows: Vec<BatteryRow>,
    #[serde(default)]
    pub session_net_accepted: i64,
    #[serde(default)]
    pub session_net_all36: i64,
    #[serde(default)]
    pub rowCount: usize,
    #[serde(default)]
    pub skippedRows: Vec<String>,
}

/// Per-gate PASS/FAIL/NOT_CLAIMED status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateStatus {
    Pass,
    Fail,
    NotClaimed,
}

impl GateStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotClaimed => "NOT_CLAIMED",
        }
    }
}

/// Computed H3/H4/H5 gate report for Phase 2 evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Phase2GateReport {
    pub report_id: String,
    pub surface: String,
    pub baseline_commit: String,
    pub candidate_results: String,
    pub compare_results_command: String,
    pub session_net_accepted: i64,
    pub session_net_all36: i64,
    pub h3_small_serve_s_gte_m_count: usize,
    pub h3_scope_row_count: usize,
    pub h3_policy_ref: String,
    pub h5_single_chain_violations: Vec<String>,
    pub gates: Phase2GateStatuses,
    pub diagnostics: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Phase2GateStatuses {
    pub H1: GateStatus,
    pub H2: GateStatus,
    pub H3: GateStatus,
    pub H4: GateStatus,
    pub H5: GateStatus,
    pub H6: GateStatus,
    pub H7: GateStatus,
    pub H8: GateStatus,
}

/// Whether a row id matches the sf-bench small-file task suffix.
pub fn is_small_file_task_id(id: &str) -> bool {
    id.contains("_small") || id.ends_with("/small")
}

/// Rows in scope for H3 under A-012 serve-only policy.
pub fn h3_scope_rows(rows: &[BatteryRow]) -> Vec<&BatteryRow> {
    let serve_accepted: Vec<_> = rows
        .iter()
        .filter(|row| row.decision == "serve" && row.acceptedServe)
        .collect();
    let small: Vec<_> = serve_accepted
        .iter()
        .copied()
        .filter(|row| is_small_file_task_id(&row.id))
        .collect();
    if small.is_empty() {
        serve_accepted
    } else {
        small
    }
}

/// Recompute session nets and sGteM flags from row economics.
pub fn normalize_battery_results(mut results: BatteryResults) -> BatteryResults {
    for row in &mut results.rows {
        row.sGteM = row.S >= row.M;
        row.acceptedServe = row.decision == "serve" && row.equivalence == "EQUIVALENT";
        if row.goldenId.is_empty() {
            row.goldenId.clone_from(&row.id);
        }
    }
    results.session_net_accepted = results
        .rows
        .iter()
        .filter(|row| row.acceptedServe)
        .map(|row| i64::from(row.M) - i64::from(row.S))
        .sum();
    results.session_net_all36 = results
        .rows
        .iter()
        .map(|row| i64::from(row.M) - i64::from(row.S))
        .sum();
    results.rowCount = results.rows.len();
    results
}

/// Compute Phase 2 gate statuses from normalized battery results.
pub fn compute_phase2_gates(results: &BatteryResults) -> Phase2GateReport {
    let h3_rows = h3_scope_rows(&results.rows);
    let h3_violations: Vec<_> = h3_rows.iter().filter(|row| row.sGteM).collect();
    let h3_status = if h3_violations.is_empty() {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };

    let h4_status = if results.session_net_accepted >= 0 {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };

    let h5_violations: Vec<String> = results
        .rows
        .iter()
        .filter(|row| row.chain == "single" && row.mcpCalls > 1)
        .map(|row| format!("{}: mcpCalls={}", row.id, row.mcpCalls))
        .collect();
    let h5_status = if h5_violations.is_empty() {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };

    let mut diagnostics = String::new();
    if !h3_violations.is_empty() {
        diagnostics.push_str("H3 violations (accepted serve rows with sGteM): ");
        for row in &h3_violations {
            diagnostics.push_str(&format!("{} (S={} M={}); ", row.id, row.S, row.M));
        }
    }
    if results.session_net_accepted < 0 {
        diagnostics.push_str(&format!(
            "H4 session_net_accepted={} < 0. ",
            results.session_net_accepted
        ));
    }
    if !h5_violations.is_empty() {
        diagnostics.push_str(&format!("H5 violations: {}. ", h5_violations.join(", ")));
    }
    if !results.skippedRows.is_empty() {
        diagnostics.push_str(&format!(
            "Skipped {} rows (missing corpus): {}. ",
            results.skippedRows.len(),
            results.skippedRows.join(", ")
        ));
    }
    if diagnostics.is_empty() {
        diagnostics.push_str("All computed Phase 2 gates passed on measured rows.");
    }

    Phase2GateReport {
        report_id: format!("phase2-gate-{}", chrono_date_stub()),
        surface: if results.surface.is_empty() {
            "compact".to_string()
        } else {
            results.surface.clone()
        },
        baseline_commit: results.baselineCommit.clone(),
        candidate_results: String::new(),
        compare_results_command: String::new(),
        session_net_accepted: results.session_net_accepted,
        session_net_all36: results.session_net_all36,
        h3_small_serve_s_gte_m_count: h3_violations.len(),
        h3_scope_row_count: h3_rows.len(),
        h3_policy_ref: "docs/research/A-012-bypass-policy.md".to_string(),
        h5_single_chain_violations: h5_violations,
        gates: Phase2GateStatuses {
            H1: GateStatus::NotClaimed,
            H2: GateStatus::NotClaimed,
            H3: h3_status,
            H4: h4_status,
            H5: h5_status,
            H6: GateStatus::NotClaimed,
            H7: GateStatus::NotClaimed,
            H8: GateStatus::NotClaimed,
        },
        diagnostics: diagnostics.trim().to_string(),
    }
}

fn chrono_date_stub() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0);
    format!("epoch-day-{days}")
}

/// Render gate report as markdown (typically written to `phase2-gate-report.generated.md`).
pub fn format_gate_report_markdown(report: &Phase2GateReport) -> String {
    let gates = &report.gates;
    format!(
        "# Phase 2 compact-surface gate report\n\n\
         **Report ID:** {report_id}  \n\
         **Surface:** {surface}  \n\
         **Baseline commit:** `{baseline}`  \n\
         **Candidate results:** `{candidate}`  \n\
         **Compare command:** `{command}`  \n\
         **H3 policy:** [{h3_policy}]({h3_policy})\n\n\
         ## Gate statuses\n\n\
         | Gate | Status |\n\
         |------|--------|\n\
         | H1 | {h1} |\n\
         | H2 | {h2} |\n\
         | H3 | {h3} |\n\
         | H4 | {h4} |\n\
         | H5 | {h5} |\n\
         | H6 | {h6} |\n\
         | H7 | {h7} |\n\
         | H8 | {h8} |\n\n\
         ## Computed metrics\n\n\
         - `session_net_accepted`: {accepted}\n\
         - `session_net_all36`: {all36}\n\
         - H3 scope rows: {h3_scope}\n\
         - H3 sGteM violations: {h3_violations}\n\
         - H5 single-chain violations: {h5_count}\n\n\
         ## Diagnostics\n\n\
         {diagnostics}\n\n\
         ## H3 scope note (A-012)\n\n\
         H3 evaluates **accepted serve** rows only (bypass/degrade/cache_hit excluded). \
         When no `*_small` task ids are present, all accepted serve rows in the golden corpus \
         are used (Phase 2 golden naming uses `tN` ids, not sf-bench `*_small` suffix).\n\n\
         ## H5 note\n\n\
         Compact surface uses one external `symforge` MCP call per task. A fused find plan \
         may execute multiple tools in-process (e.g. search_files + search_text) but must still \
         report `mcpCalls=1`.\n",
        report_id = report.report_id,
        surface = report.surface,
        baseline = report.baseline_commit,
        candidate = report.candidate_results,
        command = report.compare_results_command,
        h3_policy = report.h3_policy_ref,
        h1 = gates.H1.as_str(),
        h2 = gates.H2.as_str(),
        h3 = gates.H3.as_str(),
        h4 = gates.H4.as_str(),
        h5 = gates.H5.as_str(),
        h6 = gates.H6.as_str(),
        h7 = gates.H7.as_str(),
        h8 = gates.H8.as_str(),
        accepted = report.session_net_accepted,
        all36 = report.session_net_all36,
        h3_scope = report.h3_scope_row_count,
        h3_violations = report.h3_small_serve_s_gte_m_count,
        h5_count = report.h5_single_chain_violations.len(),
        diagnostics = report.diagnostics,
    )
}

/// Phase 2 minimum exit gates (H3 + H4; H5 strongly recommended).
pub fn phase2_minimum_gates_pass(report: &Phase2GateReport) -> bool {
    report.gates.H3 == GateStatus::Pass && report.gates.H4 == GateStatus::Pass
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row(
        id: &str,
        decision: &str,
        equiv: &str,
        s: u32,
        m: u32,
        chain: &str,
    ) -> BatteryRow {
        BatteryRow {
            id: id.to_string(),
            corpus: "test".to_string(),
            S: s,
            M: m,
            sGteM: s >= m,
            acceptedServe: decision == "serve" && equiv == "EQUIVALENT",
            equivalence: equiv.to_string(),
            goldenId: id.to_string(),
            decision: decision.to_string(),
            chain: chain.to_string(),
            mcpCalls: 1,
            eligibleH6: true,
            stel: None,
        }
    }

    #[test]
    fn h3_passes_when_no_accepted_serve_s_gte_m() {
        let results = normalize_battery_results(BatteryResults {
            measuredAt: String::new(),
            symforgeBin: String::new(),
            surface: "compact".to_string(),
            method: String::new(),
            baselineCommit: "abc".to_string(),
            rows: vec![
                sample_row(
                    "cfg-if/t1_search",
                    "serve",
                    "EQUIVALENT",
                    100,
                    500,
                    "single",
                ),
                sample_row(
                    "cfg-if/pff_whole_lib",
                    "bypass",
                    "BYPASS",
                    50,
                    500,
                    "single",
                ),
            ],
            session_net_accepted: 0,
            session_net_all36: 0,
            rowCount: 0,
            skippedRows: vec![],
        });
        let report = compute_phase2_gates(&results);
        assert_eq!(report.gates.H3, GateStatus::Pass);
        assert_eq!(report.h3_scope_row_count, 1);
    }

    #[test]
    fn h3_fails_on_accepted_serve_s_gte_m() {
        let results = normalize_battery_results(BatteryResults {
            measuredAt: String::new(),
            symforgeBin: String::new(),
            surface: "compact".to_string(),
            method: String::new(),
            baselineCommit: String::new(),
            rows: vec![sample_row(
                "tokio/t2_small",
                "serve",
                "EQUIVALENT",
                600,
                500,
                "single",
            )],
            session_net_accepted: 0,
            session_net_all36: 0,
            rowCount: 0,
            skippedRows: vec![],
        });
        let report = compute_phase2_gates(&results);
        assert_eq!(report.gates.H3, GateStatus::Fail);
        assert_eq!(report.h3_small_serve_s_gte_m_count, 1);
    }

    #[test]
    fn h4_uses_accepted_serve_net_only() {
        let results = normalize_battery_results(BatteryResults {
            measuredAt: String::new(),
            symforgeBin: String::new(),
            surface: "compact".to_string(),
            method: String::new(),
            baselineCommit: String::new(),
            rows: vec![
                sample_row("a/t1", "serve", "EQUIVALENT", 100, 500, "single"),
                sample_row("b/pff", "bypass", "BYPASS", 900, 500, "single"),
            ],
            session_net_accepted: 0,
            session_net_all36: 0,
            rowCount: 0,
            skippedRows: vec![],
        });
        assert_eq!(results.session_net_accepted, 400);
        let report = compute_phase2_gates(&results);
        assert_eq!(report.gates.H4, GateStatus::Pass);
        assert!(results.session_net_all36 < results.session_net_accepted);
    }

    #[test]
    fn h5_fails_when_single_chain_exceeds_one_mcp_call() {
        let mut row = sample_row("x/t1", "serve", "EQUIVALENT", 10, 100, "single");
        row.mcpCalls = 2;
        let results = normalize_battery_results(BatteryResults {
            measuredAt: String::new(),
            symforgeBin: String::new(),
            surface: "compact".to_string(),
            method: String::new(),
            baselineCommit: String::new(),
            rows: vec![row],
            session_net_accepted: 0,
            session_net_all36: 0,
            rowCount: 0,
            skippedRows: vec![],
        });
        let report = compute_phase2_gates(&results);
        assert_eq!(report.gates.H5, GateStatus::Fail);
    }
}
