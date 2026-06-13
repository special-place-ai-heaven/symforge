# Phase 0 §12A — independent reviewer sign-off

**Tasks:** T005, T011–T014, T046–T049  
**Updated:** 2026-06-13 (in-repo evidence refresh)  
**Evidence commit:** `c3581a5` on `v8/stel-architecture`

---

## Readiness decision procedure (T011)

1. Open [phase0-12a-evidence-index.md](./phase0-12a-evidence-index.md).
2. For each §12A row, confirm artifact exists and satisfies [preflight-evidence-contract.md](../../specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md).
3. Check [stel-assumptions.md](../stel-assumptions.md) — any Phase 1-blocking **OPEN** assumption → **NO-GO** (A-019 is **interim**, not full VALIDATED).
4. Confirm evidence producer ≠ independent reviewer (T048).
5. Use [phase0-12a-independent-review-packet.md](./phase0-12a-independent-review-packet.md) for sign or reject.
6. Record decision below.

### NO-GO rules (automatic)

- Any **BLOCKED** or missing required artifact
- Any failed threshold (variance >2%, spot-check <6/6, equiv FP+FN >10%, schema over budget without pivot)
- Any Phase 1-blocking assumption **OPEN** without accepted pivot (includes **rejected** A-019 interim)
- Missing independent reviewer sign-off
- Any contradiction in bypass/H6 policy records

### GO rules (all required)

- Every §12A checkbox satisfied with accepted evidence (reviewer may accept A-019 interim)
- `blocking_gaps` empty
- Independent reviewer identity recorded
- Checklist coverage = total applicable (or documented deferrals)

---

## Checklist coverage (T012)

| Field | Value |
|-------|-------|
| Satisfied | 15 |
| Total applicable | 18 |
| Exempt | 0 |
| Active blockers | 2 |

### Counting rules

| Bucket | Count | Items |
|--------|-------|-------|
| Satisfied | 15 | A-001, A-002, A-004, A-028, A-005, A-025 validated/passed; A-003/G-005 partial accepted; doc-pass rows; scope boundary |
| Interim | 1 | A-019 compact-3 on H1 only |
| Deferred | 1 | RESULTS.md §8.7 (not Phase 0 gate) |
| Not satisfied | 2 | §9 register (A-019 interim), independent sign-off |

*Full row map: [independent review packet](./phase0-12a-independent-review-packet.md) §4.*

---

## Blocker table (T013)

### Active (block GO today)

| ID | Type | Reason |
|----|------|--------|
| B-A019 | interim | Compact-3 interim lock on H1; full L0 A/B battery pending |
| B-SIGNOFF | missing sign-off | Independent reviewer not recorded |

### Deferred (not Phase 0 pre-flight gates)

| ID | Type | Reason |
|----|------|--------|
| B-RESULTS | deferred | RESULTS.md §8.7 columns require post-8.0 baseline runs |

### Closed / superseded

| ID | Type | Reason |
|----|------|--------|
| B-SFBENCH | closed | Superseded by in-repo evidence path ([phase0-12a-sf-bench-path.md](./phase0-12a-sf-bench-path.md)) |
| B-A001 | closed | VALIDATED — 2× in-repo battery, 0% session_net variance |
| B-A004 | closed | VALIDATED — 20-sample equiv audit, 0% FP+FN |
| B-A028 | closed | VALIDATED — 36-row golden corpus |
| B-A005 | closed | VALIDATED — compact `tools/list` 891 B |
| B-A025 | closed | VALIDATED — edit schema ≤1.5 kB |

---

## Reviewer instructions (T014)

**7.x results are informational only.**  
`results-7.21.1-baseline.json` and external sf-bench `RESULTS.md` (7.21.1 appendix) must **not** be used as v8 GO gates.

**In-repo evidence is canonical** for Phase 0: battery JSON, golden corpus, schema bytes, gather scripts. External sf-bench is optional.

**Minimum independent checks before GO:**

1. Confirm A-001 run1/run2 `session_net_accepted` match ([artifacts](./A-001-tool-battery-run1.json)).
2. Spot-check ≥5 A-004 rows against battery output.
3. Confirm A-005 compact schema **891 B** in [A-005-schema-bytes.json](./A-005-schema-bytes.json).
4. Run `node scripts/validate-routes-golden.cjs` for A-028.
5. Decide A-019 interim accept vs require full L0 A/B.

---

## Evidence-link validation (T046)

| Target | Status |
|--------|--------|
| phase0-12a-evidence-index.md | All §12A rows mapped; B-SFBENCH closed |
| stel-assumptions.md | A-001/A-004/A-028 VALIDATED links; A-019 interim |
| v8-gap-closure-plan.md | §12A checkboxes aligned; §9 and RESULTS.md open |

**Blockers recorded:** yes (active table above)

---

## Reviewer dry-run (T047)

| Field | Value |
|-------|-------|
| Date | 2026-06-13 |
| Reviewer role | Evidence producer self-assessment (not independent) |
| Time to reach decision | ~8 minutes (packet → blockers → NO-GO) |
| SC-011 15-minute timebox | **PASS** |
| Notes | Independent reviewer still required for valid GO |

---

## Independent sign-off (T048)

| Field | Value |
|-------|-------|
| Evidence producer | Cursor agent (speckit.implement session) |
| Independent reviewer | **NOT OBTAINED** |
| Sign-off reference | — |

**T048 status:** **FAIL** — producer cannot self-sign per spec clarification 2026-06-13.

---

## Final decision (T049)

```yaml
decision: NO-GO
decision_date: 2026-06-13
independent_reviewer: null
sign_off_reference: null
checklist_coverage:
  satisfied: 15
  total_applicable: 18
blocking_gaps:
  - id: B-A019
    reason: L0 surface interim compact-3 lock only; full A/B battery pending (reviewer may accept interim)
  - id: B-SIGNOFF
    reason: independent reviewer sign-off missing
superseded_gaps:
  - id: B-SFBENCH
    reason: in-repo evidence path active; external sf-bench optional
deferred_gaps:
  - id: B-RESULTS
    reason: RESULTS.md §8.7 deferred until v8 baseline runs (not Phase 0 gate)
evidence_summary: docs/research/phase0-12a-evidence-index.md
evidence_commit: c3581a5
validated_assumptions:
  - A-001
  - A-004
  - A-028
  - A-005
interim_assumptions:
  - A-019
```

### Next action (independent reviewer)

1. Open [phase0-12a-independent-review-packet.md](./phase0-12a-independent-review-packet.md) (refreshed for `c3581a5`).
2. Complete minimum checks in T014 § Minimum independent checks.
3. Accept or reject A-019 interim compact-3 lock.
4. Record GO or NO-GO in § Independent sign-off above.

**First `src/stel/` commit:** **NOT AUTHORIZED**

---

## Evidence refresh log (2026-06-13)

| Item | Status |
|------|--------|
| B-SFBENCH | **CLOSED** — in-repo path |
| A-001 | **VALIDATED** |
| A-004 | **VALIDATED** |
| A-028 | **VALIDATED** |
| A-005 H1 | **VALIDATED** (891 B) |
| A-019 | **INTERIM** |
| B-SIGNOFF | **OPEN** |
| Decision | **NO-GO** (unchanged until independent sign-off) |
