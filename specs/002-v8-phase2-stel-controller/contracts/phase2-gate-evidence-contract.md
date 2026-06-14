# Contract: Phase 2 STEL Gate Evidence

This contract defines evidence reviewers must inspect before claiming Phase 2 exit. It does **not** authorize B-RESULTS or 8.0 baseline closure.

## Phase 2 Exit Record

```yaml
phase: 2
decision: PASS | FAIL | IN_PROGRESS
decision_date: YYYY-MM-DD
main_commit: "<full sha at merge>"
reviewer: "<name or agent id>"
golden_replay:
  total_rows: 36
  deferred_multi_hop: 0
  deferred_planner_mismatch: 0
  artifact: "tests/stel_golden_replay.rs CI log or local run reference"
gate_report: "<path to phase2-gate-report.md>"
a029_spike: "<path to A-029-t2-spike.md>"
assumption_updates:
  - id: A-008
    verdict: OPEN | VALIDATED | INVALIDATED
  - id: A-009
    verdict: OPEN | VALIDATED | INVALIDATED
  - id: A-010
    verdict: OPEN | VALIDATED | INVALIDATED
  - id: A-011
    verdict: OPEN | VALIDATED | INVALIDATED
  - id: A-012
    verdict: OPEN | VALIDATED | INVALIDATED
  - id: A-013
    verdict: OPEN | VALIDATED | INVALIDATED
  - id: A-014
    verdict: OPEN | VALIDATED | INVALIDATED
  - id: A-029
    verdict: OPEN | VALIDATED | INVALIDATED
blocking_gaps: []
```

**Rules**:

- `decision: PASS` requires `deferred_multi_hop: 0`.
- `decision: PASS` requires gate report with **H3: PASS** and **H4: PASS** on compact surface.
- `decision: PASS` requires A-029 artifact with PASS or documented P-T2 pivot (not OPEN).
- `decision: PASS` must not list `b_results` or `baseline_8_0_pin` in scope.

## Gate Report Record

```yaml
report_id: phase2-gate-YYYY-MM-DD
surface: compact
candidate_results: "<path/to/candidate.json>"
baseline_results: "<path/to/baseline.json>"
compare_results_command: "<exact command run>"
gates:
  H1: PASS | FAIL | NOT_CLAIMED
  H2: PASS | FAIL | NOT_CLAIMED
  H3: PASS | FAIL
  H4: PASS | FAIL
  H5: PASS | FAIL | NOT_CLAIMED
  H6: NOT_CLAIMED
  H7: NOT_CLAIMED
  H8: NOT_CLAIMED
h3_policy_ref: "docs/research/A-012-bypass-policy.md"
session_net_accepted: <number>
diagnostics: "<free text if any gate FAIL>"
```

**Rules**:

- Phase 2 exit minimum: **H3 PASS**, **H4 PASS**.
- H6/H7/H8 must be `NOT_CLAIMED` unless separate Phase 3+ evidence exists.
- `session_net_accepted` must match compare-results H4 computation (A-026).

## Multi-Hop Golden Replay Record

For each deferred row, evidence must show plan + replay PASS:

```yaml
row_id: cfg-if/multi_search_symbol
must_call: [search_symbols, get_symbol]
planned_tools: [search_symbols, get_symbol]
decision: serve
replay_category: SupportedServe
```

Repeat for:

- `records/multi_context_refs` → `[get_file_context, find_references]`
- `is-plain/multi_files_content` → `[search_files, get_file_content]`

**Rules**:

- `planned_tools` order must match `must_call`.
- `replay_category` must not be `DeferredMultiHop` at Phase 2 exit.

## Battery Row STEL Extension (required fields)

Each measured row in candidate results must include:

```json
{
  "equivalence": "EQUIVALENT|SYMFORGE-LESS|SYMFORGE-MORE|BYPASS",
  "acceptedServe": true,
  "sGteM": false,
  "decision": "serve|bypass|degrade|cache_hit",
  "mcpCalls": 1,
  "eligibleH6": true,
  "stel": {
    "plan_id": "string",
    "decision": "serve|bypass|degrade|cache_hit",
    "tools_called": ["string"],
    "predicted_tokens": 0,
    "actual_tokens": 0,
    "net_vs_manual": 0,
    "route_confidence": "exact|inferred|fallback"
  }
}
```

**Rules**:

- Missing fields invalidate gate report.
- External `mcpCalls` must be 1 for compact facade tasks (H5).

## A-029 Spike Record

```yaml
id: A-029
repos: [tokio, django]
t2_tasks_total: 4
t2_equiv_pass: <0-4>
verdict: PASS | PIVOT | KILL
pivot_policy: null | "P-T2 bypass-only for reference tasks"
artifact: "<path>"
validated_at: YYYY-MM-DD
notes: "<pass, pivot, or kill conclusion>"
```

**Rules**:

- PASS: `t2_equiv_pass >= 2`
- PIVOT: must define `pivot_policy` and H6 denominator impact
- KILL: blocks Phase 2 PASS until resolved or scope revised via spec amendment

## Explicitly Rejected Evidence Types (Phase 2)

The following do **not** satisfy Phase 2 exit:

- RESULTS.md §8.7 post-8.0 baseline narrative without A-024 pin
- Ledger SQLite migration screenshots
- Calibration EMA changing live L2 margins in production path
- 7.x baseline comparison as v8 gate (informational only)

## Reviewer Checklist (5 minutes)

1. Golden replay: 0 deferred multi-hop?
2. Gate report: H3 + H4 PASS on compact?
3. A-029: PASS or P-T2 pivot documented?
4. Assumption register updated for A-008..A-014, A-029?
5. No persistence / B-RESULTS claims in PR description?
