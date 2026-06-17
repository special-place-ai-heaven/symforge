//! Automated honesty gate (feature 010, US6, FR-017 / FR-018 / FR-004).
//!
//! This is the enforced half of the honesty contract: a `cargo test` that FAILS
//! the build when a shipped capability claim outruns its evidence. CI already
//! runs `cargo test --all-targets`, so a test is the deterministic, locally
//! runnable enforcement and needs no new CI infrastructure.
//!
//! # What the gate enforces
//!
//! It parses two real markdown documents — the capability matrix
//! (`docs/v8-capability-matrix.md`) and the assumption register
//! (`docs/stel-assumptions.md`) — and cross-references them:
//!
//! 1. **FR-018 (claim of proof => proof exists).** A matrix row whose Proof
//!    state is `Implemented` (a claim that the capability is PROVEN) must be
//!    backed by evidence: at least one of its referenced assumptions is
//!    `VALIDATED` in the register, OR the row carries a real Evidence artifact
//!    (a test/measurement reference) and references no effectively-`OPEN`
//!    assumption. A row claiming `Implemented` while its only backing assumption
//!    is `OPEN` and it has no artifact FAILS.
//!    - Rows labeled `Heuristic` / `Observational` / `Deferred` are HONEST
//!      LABELS and always pass — even when the backing assumption is `OPEN`.
//!      Labeling an OPEN assumption honestly is the explicit non-failure case
//!      (`contracts/honesty-ci-gate.md`: "relabel != validate"). The gate keys
//!      on "claim of proof", never on the mere presence of the word OPEN.
//! 2. **FR-004 (artifact-backed VALIDATED).** Every register assumption marked
//!    `VALIDATED` (in any of its register rows) must carry an artifact reference
//!    (a markdown link or an inline-code path) somewhere in its rows. A
//!    `VALIDATED` verdict with no artifact anywhere is validated-by-assertion
//!    and FAILS.
//! 3. **FR-017 (single source of truth).** Every Assumption ID the matrix
//!    references must exist in the register. A dangling reference FAILS. The
//!    register is authoritative; the matrix never restates a verdict.
//!
//! # What the gate deliberately does NOT do
//!
//! It is a *structured-record* gate, not a prose scanner. It does **not**
//! NLP-scan arbitrary documentation prose or source code for capability claims —
//! that surface is unbounded and cannot be enforced deterministically. It
//! enforces only the two structured honesty records (matrix + register) and
//! their cross-references. A dishonest claim made in free prose somewhere else
//! is out of this gate's scope by construction. Stating that limit honestly is
//! itself in the 010 spirit: the gate does not overclaim its own reach.
//!
//! When the verdict for an assumption appears in more than one register table
//! (the register has both an "initial" register and later phase-evidence
//! tables), the gate uses the **most conservative** verdict across all rows for
//! the FR-018 check (if any row says OPEN, the assumption is treated as not
//! fully proven), while artifact-backing (FR-004) is satisfied if *any* row
//! carries an artifact. This prevents a claim from leaning on a stale stronger
//! verdict when another row has demoted it.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// A register verdict, ordered from weakest (least proven) to strongest so that
/// `min` yields the most conservative verdict across multiple register rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Verdict {
    Open,
    Invalidated,
    Partial,
    Validated,
}

impl Verdict {
    /// Parse a verdict from a register Status/Verdict cell. The cell may carry
    /// trailing prose and bold markers, e.g. `**PARTIAL** (demoted ...)` or
    /// `**OPEN** (cited, not reproduced)`. We key on the first recognized token.
    fn parse(cell: &str) -> Option<Verdict> {
        let upper = cell.to_ascii_uppercase();
        // Order matters: check the more specific tokens first. INVALIDATED
        // contains "VALIDATED" as a substring, so test it before VALIDATED.
        if upper.contains("INVALIDATED") {
            Some(Verdict::Invalidated)
        } else if upper.contains("VALIDATED") {
            Some(Verdict::Validated)
        } else if upper.contains("PARTIAL") {
            Some(Verdict::Partial)
        } else if upper.contains("OPEN") {
            Some(Verdict::Open)
        } else {
            None
        }
    }
}

/// The aggregate register state for one assumption ID across every table row
/// that mentions it.
#[derive(Debug, Clone)]
struct RegisterEntry {
    /// Most conservative (weakest) verdict observed for this ID.
    effective: Verdict,
    /// True if at least one register row for this ID carries an artifact
    /// reference (a markdown link or an inline-code path).
    has_artifact: bool,
}

/// A capability-matrix proof state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProofState {
    Implemented,
    Heuristic,
    Observational,
    Deferred,
}

impl ProofState {
    fn parse(cell: &str) -> Option<ProofState> {
        // The matrix may decorate the state, e.g. `Implemented (serve /mcp)`.
        // Key on the leading recognized word.
        let lower = cell.to_ascii_lowercase();
        if lower.contains("implemented") {
            Some(ProofState::Implemented)
        } else if lower.contains("heuristic") {
            Some(ProofState::Heuristic)
        } else if lower.contains("observational") {
            Some(ProofState::Observational)
        } else if lower.contains("deferred") {
            Some(ProofState::Deferred)
        } else {
            None
        }
    }
}

/// One parsed matrix row.
#[derive(Debug, Clone)]
struct MatrixRow {
    feature: String,
    proof_state: ProofState,
    /// Assumption IDs referenced by this row (may be empty when the cell is
    /// `n/a ...`; may be more than one, e.g. `A-012 (PARTIAL), A-013 (...)`).
    assumption_ids: Vec<String>,
    /// True when the Assumption ID cell explicitly says `n/a`.
    is_na: bool,
    /// True when the Evidence cell carries a real test/measurement artifact
    /// reference (a `::` test path, a markdown link, or an inline-code path).
    has_artifact: bool,
}

/// A single honesty violation, with enough context to point at the offending
/// document row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    rule: &'static str,
    detail: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.rule, self.detail)
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers (std-only; no new dependency)
// ---------------------------------------------------------------------------

/// Split a markdown table row into trimmed cells. A table row looks like
/// `| a | b | c |`; the leading/trailing empty cells from the bounding pipes
/// are dropped. Returns `None` for lines that are not table rows.
fn table_cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    // Strip the leading and trailing pipe, then split on the interior pipes.
    let inner = trimmed
        .strip_prefix('|')
        .unwrap_or(trimmed)
        .strip_suffix('|')
        .unwrap_or(trimmed.strip_prefix('|').unwrap_or(trimmed));
    let cells: Vec<String> = inner.split('|').map(|c| c.trim().to_string()).collect();
    Some(cells)
}

/// True if a row of cells is a markdown table separator (`|---|---|`).
fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
}

/// Extract every assumption ID (`A-NNN`) appearing in a cell, tolerating bold
/// `**A-019**` and parenthetical verdicts. Returns them in appearance order,
/// de-duplicated.
fn extract_assumption_ids(cell: &str) -> Vec<String> {
    let bytes = cell.as_bytes();
    let mut ids = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        // Match the literal "A-" followed by ASCII digits.
        if (bytes[i] == b'A') && bytes[i + 1] == b'-' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 2 {
                let id = format!("A-{}", &cell[i + 2..j]);
                if !ids.contains(&id) {
                    ids.push(id);
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    ids
}

/// Heuristic: does a cell carry a real artifact reference (a link, an
/// inline-code path, or a `::` test-path)? Used both for the register Evidence
/// detection (FR-004) and the matrix Evidence detection (FR-018).
fn cell_has_artifact(cell: &str) -> bool {
    // Markdown link: `[text](path)`.
    if cell.contains("](") {
        return true;
    }
    // A Rust test path reference, e.g. `tests/foo.rs::some_test`.
    if cell.contains("::") {
        return true;
    }
    // An inline-code path token like `tests/...`, `src/...`, `research/...`,
    // or anything ending in a source/file extension inside backticks.
    let lowered = cell.to_ascii_lowercase();
    for marker in [
        "tests/",
        "src/",
        "research/",
        "docs/",
        ".rs",
        ".json",
        ".md",
    ] {
        if lowered.contains(marker) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Register parsing
// ---------------------------------------------------------------------------

/// Parse the assumption register into a map of ID -> aggregate entry. The
/// register has several tables; an ID may appear in more than one. We aggregate
/// across all rows: the verdict is the most conservative observed, and the
/// artifact flag is true if any row carries an artifact.
///
/// A register row is recognized as an assumption row when its first cell
/// contains exactly one `A-NNN` ID (after stripping bold markers) AND some
/// later cell parses to a verdict. Rows like the phase-gate table (whose first
/// cell is a phase name, not an ID) are skipped because they have no `A-NNN` in
/// cell 0; rows that look like assumption rows but carry an unparseable verdict
/// fail loudly (see [`parse_register`]).
fn parse_register(text: &str) -> Result<BTreeMap<String, RegisterEntry>, Vec<Violation>> {
    let mut map: BTreeMap<String, RegisterEntry> = BTreeMap::new();
    let mut violations = Vec::new();

    for line in text.lines() {
        let Some(cells) = table_cells(line) else {
            continue;
        };
        if cells.len() < 2 || is_separator_row(&cells) {
            continue;
        }
        // The first cell must be a single assumption ID for this to be an
        // assumption row. The phase-gate table's first cell is a phase label
        // ("0 baseline"); range cells like "A-008..A-014 evidence recorded"
        // hold more than one ID and are summary prose, not a per-ID verdict —
        // skip both.
        let id0 = extract_assumption_ids(&cells[0]);
        if id0.len() != 1 {
            continue;
        }
        // The first cell must be *just* the ID (allowing bold/whitespace), not
        // a sentence that merely mentions one ID. This filters the header-row
        // crosswalk and any prose row that happens to cite a single ID.
        let bare = cells[0].replace('*', "");
        let bare = bare.trim();
        if bare != id0[0] {
            continue;
        }
        let id = id0[0].clone();

        // Find the verdict cell. In the "initial register" tables the verdict
        // is the last cell (Status); in the phase-evidence tables it is the
        // Verdict cell (3rd of 4). Scan all cells after the first and take the
        // first that parses to a verdict.
        let verdict = cells[1..].iter().find_map(|c| Verdict::parse(c));
        let Some(verdict) = verdict else {
            // A row that looks like an assumption row (bare ID in cell 0) but
            // carries no parseable verdict is a malformed honesty record. Fail
            // loudly rather than silently skipping.
            violations.push(Violation {
                rule: "PARSE",
                detail: format!(
                    "register row for {id} has no parseable verdict (OPEN/PARTIAL/VALIDATED/INVALIDATED): {line}"
                ),
            });
            continue;
        };

        let row_has_artifact = cells.iter().any(|c| cell_has_artifact(c));

        map.entry(id)
            .and_modify(|e| {
                if verdict < e.effective {
                    e.effective = verdict;
                }
                e.has_artifact = e.has_artifact || row_has_artifact;
            })
            .or_insert(RegisterEntry {
                effective: verdict,
                has_artifact: row_has_artifact,
            });
    }

    if violations.is_empty() {
        Ok(map)
    } else {
        Err(violations)
    }
}

// ---------------------------------------------------------------------------
// Matrix parsing
// ---------------------------------------------------------------------------

/// Parse the capability matrix table rows. The matrix is the single table whose
/// header is `Feature | Proof state | Assumption ID | Surface claim | Evidence`.
/// Rows are recognized once that header (and its separator) is seen; parsing
/// stops at the first non-table line after the table.
fn parse_matrix(text: &str) -> Result<Vec<MatrixRow>, Vec<Violation>> {
    let mut rows = Vec::new();
    let mut violations = Vec::new();
    let mut in_table = false;
    let mut header_cols = 0usize;

    for line in text.lines() {
        let Some(cells) = table_cells(line) else {
            if in_table {
                // Left the table region.
                break;
            }
            continue;
        };

        // Detect the matrix header by its first two column names.
        let looks_like_header = cells.len() >= 5
            && cells[0].eq_ignore_ascii_case("Feature")
            && cells[1].to_ascii_lowercase().contains("proof state");
        if looks_like_header {
            in_table = true;
            header_cols = cells.len();
            continue;
        }
        if !in_table {
            continue;
        }
        if is_separator_row(&cells) {
            continue;
        }

        // A data row. It must have the expected column count; a malformed row
        // (wrong arity) is a broken honesty record -> fail loudly.
        if cells.len() != header_cols {
            violations.push(Violation {
                rule: "PARSE",
                detail: format!(
                    "matrix row has {} cells, expected {header_cols}: {line}",
                    cells.len()
                ),
            });
            continue;
        }

        let feature = cells[0].clone();
        let Some(proof_state) = ProofState::parse(&cells[1]) else {
            violations.push(Violation {
                rule: "PARSE",
                detail: format!(
                    "matrix row '{feature}' has unrecognized proof state: {}",
                    cells[1]
                ),
            });
            continue;
        };
        let id_cell = &cells[2];
        let is_na = id_cell.to_ascii_lowercase().contains("n/a");
        let assumption_ids = extract_assumption_ids(id_cell);
        // A row must reference at least one assumption OR be explicitly n/a.
        if assumption_ids.is_empty() && !is_na {
            violations.push(Violation {
                rule: "PARSE",
                detail: format!(
                    "matrix row '{feature}' Assumption ID cell is neither an A-NNN nor n/a: {id_cell}"
                ),
            });
            continue;
        }
        let evidence_cell = &cells[header_cols - 1];
        let has_artifact = cell_has_artifact(evidence_cell);

        rows.push(MatrixRow {
            feature,
            proof_state,
            assumption_ids,
            is_na,
            has_artifact,
        });
    }

    if !in_table {
        violations.push(Violation {
            rule: "PARSE",
            detail: "capability matrix table (Feature | Proof state | ...) not found".to_string(),
        });
    }

    if violations.is_empty() {
        Ok(rows)
    } else {
        Err(violations)
    }
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

/// Pure honesty gate. Parses both documents and enforces FR-017, FR-018, and
/// FR-004. Returns `Ok(())` when every invariant holds, or `Err(violations)`
/// listing every breach found (it does not short-circuit, so a single run
/// reports all problems).
///
/// This function takes the document *text* (not paths) so it is fully testable
/// with fixtures (see the T042 tests) and non-tautological: the same code path
/// runs against fixtures and against the real docs.
fn check_honesty(register_text: &str, matrix_text: &str) -> Result<(), Vec<Violation>> {
    let mut violations = Vec::new();

    let register = match parse_register(register_text) {
        Ok(r) => r,
        Err(mut v) => {
            violations.append(&mut v);
            BTreeMap::new()
        }
    };
    let matrix = match parse_matrix(matrix_text) {
        Ok(m) => m,
        Err(mut v) => {
            violations.append(&mut v);
            Vec::new()
        }
    };

    // FR-004: every VALIDATED register assumption must carry an artifact.
    for (id, entry) in &register {
        if entry.effective == Verdict::Validated && !entry.has_artifact {
            violations.push(Violation {
                rule: "FR-004",
                detail: format!(
                    "register assumption {id} is VALIDATED but carries no artifact reference (validated-by-assertion)"
                ),
            });
        }
    }

    for row in &matrix {
        // FR-017: every referenced assumption ID must exist in the register.
        for id in &row.assumption_ids {
            if !register.contains_key(id) {
                violations.push(Violation {
                    rule: "FR-017",
                    detail: format!(
                        "matrix row '{}' references assumption {id}, which does not exist in the register",
                        row.feature
                    ),
                });
            }
        }

        // FR-018: only `Implemented` rows make a proof claim. Honest labels
        // (Heuristic / Observational / Deferred) always pass.
        if row.proof_state != ProofState::Implemented {
            continue;
        }

        // An Implemented row is honest if EITHER:
        //   (a) at least one referenced assumption is VALIDATED, OR
        //   (b) it carries a real Evidence artifact AND no referenced
        //       assumption is effectively OPEN (n/a + artifact, or
        //       PARTIAL/VALIDATED + artifact are fine; an OPEN assumption with
        //       only a bare artifact is the failure case the gate guards).
        let any_validated = row.assumption_ids.iter().any(|id| {
            register
                .get(id)
                .map(|e| e.effective == Verdict::Validated)
                .unwrap_or(false)
        });
        if any_validated {
            continue;
        }

        let references_open = row.assumption_ids.iter().any(|id| {
            register
                .get(id)
                .map(|e| e.effective == Verdict::Open)
                .unwrap_or(false)
        });

        if row.has_artifact && !references_open {
            // n/a bug-fix / presentation guarantee backed by a real test, or a
            // PARTIAL assumption backed by a real artifact. Honest.
            continue;
        }

        // Otherwise: an Implemented (proof) claim whose backing is OPEN with no
        // validated assumption, or which lacks any artifact entirely.
        let ids = if row.assumption_ids.is_empty() {
            "n/a".to_string()
        } else {
            row.assumption_ids.join(", ")
        };
        violations.push(Violation {
            rule: "FR-018",
            detail: format!(
                "matrix row '{}' claims Implemented (proven) but its backing assumption(s) [{ids}] are OPEN/unproven and it carries no validating artifact (claim outruns evidence)",
                row.feature
            ),
        });
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

// ---------------------------------------------------------------------------
// Doc paths
// ---------------------------------------------------------------------------

fn register_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/stel-assumptions.md")
}

fn matrix_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/v8-capability-matrix.md")
}

// ---------------------------------------------------------------------------
// T041 — structural gate on the real matrix
// ---------------------------------------------------------------------------

/// T041: the capability matrix exists, has the expected header, and every data
/// row is well-formed (a feature, a recognized proof state, an `A-NNN` or
/// `n/a`, and parseable Evidence). A malformed row fails loudly here.
#[test]
fn t041_capability_matrix_is_structurally_well_formed() {
    let path = matrix_path();
    assert!(
        path.exists(),
        "docs/v8-capability-matrix.md must exist (FR-017 capability record): {}",
        path.display()
    );
    let matrix_text = fs::read_to_string(&path).expect("read capability matrix");

    let rows = match parse_matrix(&matrix_text) {
        Ok(rows) => rows,
        Err(violations) => {
            let report = violations
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n  ");
            panic!("capability matrix has malformed rows:\n  {report}");
        }
    };

    assert!(
        !rows.is_empty(),
        "capability matrix parsed to zero rows; expected one row per capability"
    );

    // Every row must have a recognized proof state (guaranteed by the parser)
    // and either at least one assumption ID or an explicit n/a.
    for row in &rows {
        assert!(
            !row.assumption_ids.is_empty() || row.is_na,
            "matrix row '{}' must reference an assumption ID or be explicitly n/a",
            row.feature
        );
        let _ = row.proof_state; // recognized by construction
    }
}

// ---------------------------------------------------------------------------
// T042 — the gate works in both directions, on fixtures
// ---------------------------------------------------------------------------

/// A minimal register fixture with one OPEN assumption and one VALIDATED
/// assumption that carries an artifact.
const FIXTURE_REGISTER: &str = "\
# fixture register

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-901** | small-surface premise helps the LLM | A/B on corpus | **OPEN** (cited, not reproduced) |
| **A-902** | cache_hit saves tokens | [`tests/cache.rs`](../tests/cache.rs) | **VALIDATED** |
";

fn matrix_with(feature_state_id_evidence: &str) -> String {
    format!(
        "# fixture matrix

| Feature | Proof state | Assumption ID | Surface claim | Evidence |
|---------|-------------|---------------|---------------|----------|
{feature_state_id_evidence}
"
    )
}

/// T042(a): an `Implemented` row whose backing assumption is OPEN with no
/// artifact MUST fail FR-018.
#[test]
fn t042a_implemented_claim_on_open_assumption_without_artifact_fails() {
    let matrix = matrix_with(
        "| Premise feature | Implemented | A-901 | this is proven and validated | (none) |",
    );
    let result = check_honesty(FIXTURE_REGISTER, &matrix);
    let violations = result.expect_err("an Implemented claim on an OPEN assumption must fail");
    assert!(
        violations.iter().any(|v| v.rule == "FR-018"),
        "expected an FR-018 violation, got: {violations:?}"
    );
}

/// T042(b): the SAME OPEN assumption, but the row is honestly labeled
/// `Heuristic` / `Observational` / `Deferred`, MUST pass. Labeling an OPEN
/// assumption honestly is allowed (relabel != validate).
#[test]
fn t042b_honest_labels_on_open_assumption_pass() {
    for honest_state in ["Heuristic", "Observational", "Deferred"] {
        let matrix = matrix_with(&format!(
            "| Premise feature | {honest_state} | A-901 | a bet under test, labeled {honest_state} | A-901 OPEN in register |"
        ));
        let result = check_honesty(FIXTURE_REGISTER, &matrix);
        assert!(
            result.is_ok(),
            "honest '{honest_state}' label on an OPEN assumption must pass, got: {:?}",
            result.err()
        );
    }
}

/// T042(c): a `VALIDATED` register entry with no artifact anywhere MUST fail
/// FR-004 (validated-by-assertion).
#[test]
fn t042c_validated_without_artifact_fails() {
    let register = "\
# fixture register

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-903** | some claim | asserted, no artifact | **VALIDATED** |
";
    // An empty matrix so only FR-004 can fire.
    let matrix =
        matrix_with("| Placeholder | Deferred | n/a | nothing claimed | honestly deferred |");
    let result = check_honesty(register, &matrix);
    let violations = result.expect_err("a VALIDATED entry with no artifact must fail FR-004");
    assert!(
        violations.iter().any(|v| v.rule == "FR-004"),
        "expected an FR-004 violation, got: {violations:?}"
    );
}

/// T042(extra, FR-017): a matrix row that references an assumption absent from
/// the register MUST fail FR-017 (dangling reference / single-source-of-truth).
#[test]
fn t042d_dangling_assumption_reference_fails() {
    let matrix = matrix_with(
        "| Ghost feature | Heuristic | A-999 | references a missing assumption | est. |",
    );
    let result = check_honesty(FIXTURE_REGISTER, &matrix);
    let violations = result.expect_err("a dangling assumption reference must fail FR-017");
    assert!(
        violations.iter().any(|v| v.rule == "FR-017"),
        "expected an FR-017 violation, got: {violations:?}"
    );
}

/// T042(extra): an `Implemented` row backed by a VALIDATED assumption passes,
/// and one backed by an n/a + a real test artifact (a bug-fix guarantee)
/// passes. Confirms the gate is not a blanket "Implemented always fails".
#[test]
fn t042e_implemented_with_validated_or_artifact_passes() {
    // Backed by a VALIDATED assumption.
    let validated = matrix_with(
        "| Cache feature | Implemented | A-902 | cache_hit proven | `tests/cache.rs::saves` |",
    );
    assert!(
        check_honesty(FIXTURE_REGISTER, &validated).is_ok(),
        "Implemented backed by VALIDATED must pass: {:?}",
        check_honesty(FIXTURE_REGISTER, &validated).err()
    );

    // Bug-fix guarantee: n/a assumption + a real test artifact.
    let bugfix = matrix_with(
        "| Guard fix | Implemented | n/a (bug fix) | guard is enforced | `src/edit.rs::guard_test` |",
    );
    assert!(
        check_honesty(FIXTURE_REGISTER, &bugfix).is_ok(),
        "Implemented n/a backed by a real artifact must pass: {:?}",
        check_honesty(FIXTURE_REGISTER, &bugfix).err()
    );
}

// ---------------------------------------------------------------------------
// The real-docs gate — the enforcement that runs in CI
// ---------------------------------------------------------------------------

/// FR-018 enforcement against the ACTUAL shipped docs. If this fails, the
/// matrix or the register is dishonest and the build must not pass. Do NOT
/// weaken the gate to make it green — fix the doc.
#[test]
fn real_docs_pass_the_honesty_gate() {
    let register_text = fs::read_to_string(register_path()).expect("read assumption register");
    let matrix_text = fs::read_to_string(matrix_path()).expect("read capability matrix");

    if let Err(violations) = check_honesty(&register_text, &matrix_text) {
        let report = violations
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n  ");
        panic!(
            "the shipped honesty record is dishonest (fix the doc, do not weaken the gate):\n  {report}"
        );
    }
}
