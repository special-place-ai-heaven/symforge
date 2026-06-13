//! S4 exit validation — replay `routes.golden.jsonl` rows against the compact `symforge` handler.

use std::path::Path;

use super::types::{AdmissionDecision, GoldenRouteRow};

/// Five single-hop rows aligned with current `ask` routing (S4 stub handler path).
/// Replaced cfg-if-only seed set until L1 planner honors golden `must_call` directly.
pub const S4_EXIT_ROW_IDS: [&str; 5] = [
    "cfg-if/t3_symbols",
    "cfg-if/t8_explore",
    "records/t4_refs",
    "records/t6_dependents",
    "compression/t5_dependents",
];

/// Pinned corpus root for an S4 replay row id.
pub fn corpus_for_row_id(id: &str) -> &'static str {
    if id.starts_with("cfg-if/") {
        S4_REPLAY_CORPUS
    } else if id.starts_with("records/") {
        "tests/fixtures/phase0-corpus/records-python"
    } else if id.starts_with("compression/") {
        "tests/fixtures/compression_ratio/rust"
    } else {
        panic!("S4 replay row `{id}` has no pinned corpus mapping")
    }
}

/// Relative path to the canonical golden corpus from the repo root.
pub const GOLDEN_ROUTES_FIXTURE: &str = "docs/fixtures/routes.golden.jsonl";

/// cfg-if corpus used for S4 replay (pinned Phase 0 battery repo).
pub const S4_REPLAY_CORPUS: &str = "tests/fixtures/phase0-corpus/cfg-if-rust";

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

/// Select the five S4 exit rows from a parsed golden corpus.
pub fn s4_exit_rows<'a>(rows: &'a [GoldenRouteRow]) -> Vec<&'a GoldenRouteRow> {
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

/// Validate compact `symforge` output for one golden row (S4 stub economics path).
pub fn validate_s4_replay_output(row: &GoldenRouteRow, output: &str) -> ReplayValidation {
    let mut errors = Vec::new();

    if !output.starts_with("── stel ──") {
        errors.push("missing STEL trust envelope header (`── stel ──`)".to_string());
    }
    if !output.contains('\n') || !output.contains("──\n\n") {
        errors.push("envelope must be separated from body by a blank line".to_string());
    }

    match row.expected_decision {
        AdmissionDecision::Serve => {
            if !output.contains("decision: serve") {
                errors.push("envelope missing `decision: serve`".to_string());
            }
        }
        other => {
            errors.push(format!(
                "S4 replay slice only validates serve rows; got expected {:?}",
                other
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let output = "── stel ──\ndecision: serve\n──\n\nChosen tool: search_text\n\nresults";
        let validation = validate_s4_replay_output(&row, output);
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
        let validation = validate_s4_replay_output(&row, "Chosen tool: search_text");
        assert!(!validation.passed);
        assert!(validation.errors.iter().any(|e| e.contains("envelope")));
    }

    #[test]
    fn fixture_loads_s4_exit_rows() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_ROUTES_FIXTURE);
        let rows = load_golden_rows(&path).expect("golden fixture");
        let exit = s4_exit_rows(&rows);
        assert_eq!(exit.len(), 5);
        assert_eq!(exit[0].id, "cfg-if/t3_symbols");
        assert_eq!(exit[4].id, "compression/t5_dependents");
    }
}
