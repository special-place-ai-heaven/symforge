//! Golden route replay — classify and validate `routes.golden.jsonl` against compact `symforge`.

use std::path::Path;

use super::controller::evaluate_plan;
use super::planner::build_plan;
use super::types::{AdmissionDecision, GoldenRouteRow, StelRequest};

/// Five single-hop rows aligned with current L1 planner routing (S4 minimum subset).
pub const S4_EXIT_ROW_IDS: [&str; 5] = [
    "cfg-if/t3_symbols",
    "cfg-if/t8_explore",
    "records/t4_refs",
    "records/t6_dependents",
    "compression/t5_dependents",
];

/// Multi-hop golden rows closed in Phase 2 (formerly deferred at Phase 1 exit).
pub const MULTI_HOP_GOLDEN_ROW_IDS: [&str; 3] = [
    "cfg-if/multi_search_symbol",
    "records/multi_context_refs",
    "is-plain/multi_files_content",
];

/// Back-compat alias — prefer [`MULTI_HOP_GOLDEN_ROW_IDS`].
pub const DEFERRED_MULTI_HOP_ROW_IDS: [&str; 3] = MULTI_HOP_GOLDEN_ROW_IDS;

/// Relative path to the canonical golden corpus from the repo root.
pub const GOLDEN_ROUTES_FIXTURE: &str = "docs/fixtures/routes.golden.jsonl";

/// cfg-if corpus used for golden replay (pinned Phase 0 battery repo).
pub const S4_REPLAY_CORPUS: &str = "tests/fixtures/phase0-corpus/cfg-if-rust";

/// Replay support category for one golden row (honest classification, no forced fit).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoldenReplayCategory {
    SupportedServe,
    SupportedPffBypass,
    DeferredMultiHop,
    DeferredPlannerMismatch { expected: String, planned: String },
}

/// Partition of the full golden corpus by replay support.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GoldenCorpusClassification {
    pub supported_serve: Vec<String>,
    pub supported_pff_bypass: Vec<String>,
    pub deferred_multi_hop: Vec<String>,
    pub deferred_planner_mismatch: Vec<String>,
}

impl GoldenCorpusClassification {
    pub fn row_count(&self) -> usize {
        self.supported_serve.len()
            + self.supported_pff_bypass.len()
            + self.deferred_multi_hop.len()
            + self.deferred_planner_mismatch.len()
    }
}

/// Pinned corpus root for a golden row id.
pub fn corpus_for_row_id(id: &str) -> &'static str {
    if id.starts_with("cfg-if/") {
        S4_REPLAY_CORPUS
    } else if id.starts_with("records/") {
        "tests/fixtures/phase0-corpus/records-python"
    } else if id.starts_with("is-plain/") {
        "tests/fixtures/phase0-corpus/is-plain-obj-ts"
    } else if id.starts_with("compression/") {
        "tests/fixtures/compression_ratio/rust"
    } else {
        panic!("golden row `{id}` has no pinned corpus mapping")
    }
}

/// Marker file within a corpus root used to detect clone availability in integration tests.
pub fn corpus_marker_for_row_id(id: &str) -> &'static str {
    if id.starts_with("cfg-if/") {
        "src/lib.rs"
    } else if id.starts_with("records/") {
        "records.py"
    } else if id.starts_with("is-plain/") {
        "index.js"
    } else if id.starts_with("compression/") {
        "service.rs"
    } else {
        panic!("golden row `{id}` has no corpus marker")
    }
}

/// Build the planner request used for golden classification and replay.
pub fn request_for_golden_row(row: &GoldenRouteRow) -> StelRequest {
    let mut request = row.to_request();
    request.intent = row.intent;
    request
}

/// Classify one golden row without mutating runtime behavior.
pub fn classify_golden_row(row: &GoldenRouteRow) -> GoldenReplayCategory {
    if row.expected_decision == AdmissionDecision::Bypass {
        let request = request_for_golden_row(row);
        let plan = build_plan(&request);
        let decision = evaluate_plan(&request, &plan);
        if decision.decision == AdmissionDecision::Bypass && decision.bypass.is_some() {
            return GoldenReplayCategory::SupportedPffBypass;
        }
        return GoldenReplayCategory::DeferredPlannerMismatch {
            expected: "bypass".to_string(),
            planned: decision.decision.as_str().to_string(),
        };
    }

    let request = request_for_golden_row(row);
    let plan = build_plan(&request);
    let planned_tools: Vec<String> = plan.steps.iter().map(|step| step.tool.clone()).collect();

    if planned_tools.len() != row.must_call.len() {
        return GoldenReplayCategory::DeferredPlannerMismatch {
            expected: row.must_call.join(" → "),
            planned: planned_tools.join(" → "),
        };
    }
    for (planned, expected) in planned_tools.iter().zip(row.must_call.iter()) {
        if planned != expected {
            return GoldenReplayCategory::DeferredPlannerMismatch {
                expected: expected.clone(),
                planned: planned.clone(),
            };
        }
    }

    let decision = evaluate_plan(&request, &plan);
    if decision.decision != AdmissionDecision::Serve {
        return GoldenReplayCategory::DeferredPlannerMismatch {
            expected: "serve".to_string(),
            planned: decision.decision.as_str().to_string(),
        };
    }

    GoldenReplayCategory::SupportedServe
}

/// Classify every row in the golden corpus.
pub fn classify_golden_corpus(rows: &[GoldenRouteRow]) -> GoldenCorpusClassification {
    let mut out = GoldenCorpusClassification::default();
    for row in rows {
        match classify_golden_row(row) {
            GoldenReplayCategory::SupportedServe => out.supported_serve.push(row.id.clone()),
            GoldenReplayCategory::SupportedPffBypass => {
                out.supported_pff_bypass.push(row.id.clone())
            }
            GoldenReplayCategory::DeferredMultiHop => out.deferred_multi_hop.push(row.id.clone()),
            GoldenReplayCategory::DeferredPlannerMismatch { .. } => {
                out.deferred_planner_mismatch.push(row.id.clone())
            }
        }
    }
    out.supported_serve.sort();
    out.supported_pff_bypass.sort();
    out.deferred_multi_hop.sort();
    out.deferred_planner_mismatch.sort();
    out
}

/// Select rows classified as supported single-hop serve replay.
pub fn supported_serve_rows(rows: &[GoldenRouteRow]) -> Vec<&GoldenRouteRow> {
    rows.iter()
        .filter(|row| classify_golden_row(row) == GoldenReplayCategory::SupportedServe)
        .collect()
}

/// Select rows classified as supported P-FF bypass replay.
pub fn supported_pff_rows(rows: &[GoldenRouteRow]) -> Vec<&GoldenRouteRow> {
    rows.iter()
        .filter(|row| classify_golden_row(row) == GoldenReplayCategory::SupportedPffBypass)
        .collect()
}

/// Parse all golden rows from JSONL text.
pub fn parse_golden_rows(jsonl: &str) -> Vec<GoldenRouteRow> {
    jsonl
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|error| {
                panic!("invalid golden route row JSON: {error}\nline: {line}")
            })
        })
        .collect()
}

/// Load golden rows from a path on disk.
pub fn load_golden_rows(path: &Path) -> std::io::Result<Vec<GoldenRouteRow>> {
    let text = std::fs::read_to_string(path)?;
    Ok(parse_golden_rows(&text))
}

/// Select the five S4 minimum subset rows from a parsed golden corpus.
pub fn s4_exit_rows(rows: &[GoldenRouteRow]) -> Vec<&GoldenRouteRow> {
    S4_EXIT_ROW_IDS
        .iter()
        .map(|id| {
            rows.iter()
                .find(|row| row.id == *id)
                .unwrap_or_else(|| panic!("golden corpus missing S4 row `{id}`"))
        })
        .collect()
}

/// Outcome of validating one compact `symforge` replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayValidation {
    pub row_id: String,
    pub passed: bool,
    pub errors: Vec<String>,
}

/// Validate compact `symforge` serve output (trust envelope, routing, ledger metadata).
pub fn validate_serve_replay_output(row: &GoldenRouteRow, output: &str) -> ReplayValidation {
    let mut errors = Vec::new();

    if !output.starts_with("── stel ──") {
        errors.push("missing STEL trust envelope header (`── stel ──`)".to_string());
    }
    if !output.contains('\n') || !output.contains("──\n\n") {
        errors.push("envelope must be separated from body by a blank line".to_string());
    }
    if !output.contains("decision: serve") {
        errors.push("envelope missing `decision: serve`".to_string());
    }
    if !output.contains("ledger: ") {
        errors.push("missing ledger metadata line (`ledger:`)".to_string());
    }

    for tool in &row.must_call {
        let chosen = format!("Chosen tool: {tool}");
        let plan = format!("→ {tool} ");
        if !output.contains(&chosen) && !output.contains(&plan) {
            errors.push(format!(
                "expected compact symforge route to `{tool}` (Chosen tool or envelope plan line)"
            ));
        }
    }

    for tool in &row.must_not_call {
        let chosen = format!("Chosen tool: {tool}");
        if output.contains(&chosen) {
            errors.push(format!("must_not_call violated: `{tool}` was chosen"));
        }
    }

    if output.contains("Index not loaded.") {
        errors.push("index was not loaded".to_string());
    }
    if output.contains("symforge STEL handler requires SYMFORGE_SURFACE=compact") {
        errors.push("compact surface was not selected".to_string());
    }

    ReplayValidation {
        row_id: row.id.clone(),
        passed: errors.is_empty(),
        errors,
    }
}

/// Validate compact `symforge` P-FF bypass output (no legacy execution, ledger metadata).
pub fn validate_pff_replay_output(row: &GoldenRouteRow, output: &str) -> ReplayValidation {
    let mut errors = Vec::new();

    if !output.starts_with("── stel ──") {
        errors.push("missing STEL trust envelope header (`── stel ──`)".to_string());
    }
    if !output.contains("decision: bypass") {
        errors.push("envelope missing `decision: bypass`".to_string());
    }
    if !output.contains("ledger: ") {
        errors.push("missing ledger metadata line (`ledger:`)".to_string());
    }
    if output.contains("Chosen tool:") {
        errors.push("P-FF bypass must not execute a legacy tool".to_string());
    }
    if !output.contains("did not execute a legacy tool") {
        errors.push("missing bypass host-read instruction".to_string());
    }
    if output.contains("lines 1-50") {
        errors.push("P-FF whole-file bypass must not cap host read at lines 1-50".to_string());
    }
    if !output.contains("(whole file)") {
        errors.push("P-FF bypass must instruct whole-file host read".to_string());
    }
    if output.contains("Index not loaded.") {
        errors.push("index was not loaded".to_string());
    }
    if output.contains("symforge STEL handler requires SYMFORGE_SURFACE=compact") {
        errors.push("compact surface was not selected".to_string());
    }

    let _ = row;
    ReplayValidation {
        row_id: row.id.clone(),
        passed: errors.is_empty(),
        errors,
    }
}

/// Back-compat alias for S4 serve validation.
pub fn validate_s4_replay_output(row: &GoldenRouteRow, output: &str) -> ReplayValidation {
    validate_serve_replay_output(row, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_rows() -> Vec<GoldenRouteRow> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        load_golden_rows(&path).expect("golden fixture")
    }

    #[test]
    fn s4_exit_ids_match_fixture_prefix() {
        for id in S4_EXIT_ROW_IDS {
            assert!(
                id.starts_with("cfg-if/")
                    || id.starts_with("records/")
                    || id.starts_with("compression/"),
                "S4 replay row must map to a pinned corpus: {id}"
            );
        }
    }

    #[test]
    fn validate_accepts_stub_serve_output() {
        let row = GoldenRouteRow {
            id: "cfg-if/t1_search".to_string(),
            query: "find cfg_if macro usage".to_string(),
            intent: None,
            must_call: vec!["search_text".to_string()],
            must_not_call: vec![],
            expected_decision: AdmissionDecision::Serve,
            expected_equiv: Some(true),
            chain: Some("single".to_string()),
            eligible_h6: Some(true),
            notes: None,
        };
        let output =
            "── stel ──\ndecision: serve\nledger: {}\n──\n\nChosen tool: search_text\n\nresults";
        let validation = validate_serve_replay_output(&row, output);
        assert!(validation.passed, "{:?}", validation.errors);
    }

    #[test]
    fn validate_rejects_missing_envelope() {
        let row = GoldenRouteRow {
            id: "cfg-if/t1_search".to_string(),
            query: "q".to_string(),
            intent: None,
            must_call: vec!["search_text".to_string()],
            must_not_call: vec![],
            expected_decision: AdmissionDecision::Serve,
            expected_equiv: None,
            chain: None,
            eligible_h6: None,
            notes: None,
        };
        let validation = validate_serve_replay_output(&row, "Chosen tool: search_text");
        assert!(!validation.passed);
        assert!(validation.errors.iter().any(|e| e.contains("envelope")));
    }

    #[test]
    fn fixture_loads_s4_exit_rows() {
        let rows = fixture_rows();
        let exit = s4_exit_rows(&rows);
        assert_eq!(exit.len(), 5);
        assert_eq!(exit[0].id, "cfg-if/t3_symbols");
        assert_eq!(exit[4].id, "compression/t5_dependents");
    }

    #[test]
    fn golden_corpus_partitions_all_rows() {
        let rows = fixture_rows();
        assert_eq!(rows.len(), 36, "golden fixture must contain 36 rows");
        let classification = classify_golden_corpus(&rows);
        assert_eq!(classification.row_count(), 36);
        assert!(
            classification.deferred_multi_hop.is_empty(),
            "Phase 2 closes multi-hop deferrals: {:?}",
            classification.deferred_multi_hop
        );
        for id in MULTI_HOP_GOLDEN_ROW_IDS {
            assert!(
                classification
                    .supported_serve
                    .iter()
                    .any(|row_id| row_id == id),
                "multi-hop row {id} must classify as supported serve"
            );
        }
        for id in S4_EXIT_ROW_IDS {
            assert!(
                classification
                    .supported_serve
                    .iter()
                    .any(|row_id| row_id == id),
                "S4 minimum subset row {id} must be supported serve replay"
            );
        }
        for id in [
            "cfg-if/pff_whole_lib",
            "records/pff_whole_module",
            "is-plain/pff_whole_index",
            "compression/pff_whole_service",
        ] {
            assert!(
                classification
                    .supported_pff_bypass
                    .iter()
                    .any(|row_id| row_id == id),
                "P-FF row {id} must be supported bypass replay"
            );
        }
        assert!(
            classification.deferred_planner_mismatch.is_empty(),
            "planner mismatches must be empty or listed explicitly in deferred_planner_mismatch_ids_are_stable"
        );
        assert!(
            classification.supported_serve.len() >= 5,
            "need broader serve replay than S4 minimum subset"
        );
        assert_eq!(classification.supported_pff_bypass.len(), 4);
    }

    #[test]
    fn deferred_planner_mismatch_ids_are_stable() {
        let rows = fixture_rows();
        let classification = classify_golden_corpus(&rows);
        let expected: [&str; 0] = [];
        assert_eq!(classification.deferred_planner_mismatch, expected);
    }

    #[test]
    fn supported_serve_ids_are_stable() {
        let rows = fixture_rows();
        let classification = classify_golden_corpus(&rows);
        let expected = [
            "cfg-if/multi_search_symbol",
            "cfg-if/t1_search",
            "cfg-if/t2_context",
            "cfg-if/t3_symbols",
            "cfg-if/t4_refs",
            "cfg-if/t5_symbol",
            "cfg-if/t6_map",
            "cfg-if/t7_content",
            "cfg-if/t8_explore",
            "compression/t1_search",
            "compression/t2_context",
            "compression/t3_symbol",
            "compression/t4_refs",
            "compression/t5_dependents",
            "is-plain/multi_files_content",
            "is-plain/t1_search",
            "is-plain/t2_context",
            "is-plain/t3_content",
            "is-plain/t4_symbols",
            "is-plain/t5_symbol",
            "is-plain/t6_refs",
            "is-plain/t7_files",
            "is-plain/t8_health",
            "records/multi_context_refs",
            "records/t1_search",
            "records/t2_context",
            "records/t3_files",
            "records/t4_refs",
            "records/t5_symbol",
            "records/t6_dependents",
            "records/t7_content",
            "records/t8_explore",
        ];
        assert_eq!(classification.supported_serve, expected);
    }
}
