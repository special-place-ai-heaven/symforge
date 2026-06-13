# Phase 0 §12A — independent reviewer packet

**Prepared:** 2026-06-13 (refreshed post-remediation)  
**Evidence commit:** `46a63c2` on `v8/stel-architecture`  
**Producer:** Cursor agent (speckit.implement remediation)  
**Purpose:** Hand to a reviewer who **did not** produce this evidence bundle (T048).

**Do not sign if you authored the artifacts below.**

---

## 1. Start here (≤15 min dry-run path)

1. Read producer decision stub: [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md)
2. Scan checklist traceability: [phase0-12a-evidence-index.md](./phase0-12a-evidence-index.md) §12A table
3. Verify scope boundary: [phase0-12a-scope-boundary.md](./phase0-12a-scope-boundary.md) (`src/stel/**` untouched)
4. Confirm binding §12A source: [v8-gap-closure-plan.md](../v8-gap-closure-plan.md) lines 429–460
5. Re-run gather (optional): `powershell -ExecutionPolicy Bypass -File scripts/gather-phase0-evidence.ps1`
6. Record GO or NO-GO in sign-off § Independent sign-off + § Final decision

---

## 2. Decision contract

From [preflight-evidence-contract.md](../../specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md):

- **GO** requires: empty `blocking_gaps`, all §12A items satisfied, your identity + sign-off reference
- **NO-GO** if: any failed threshold, Phase 1-blocking OPEN assumption without pivot, or missing independent sign-off

**7.x bench is informational only** — do not use `results-7.21.1-baseline.json` as a v8 gate.

**External sf-bench is optional** — in-repo path is active per [phase0-12a-sf-bench-path.md](./phase0-12a-sf-bench-path.md). Do **not** block on B-SFBENCH.

---

## 3. Checklist quick reference (18 applicable)

| # | §12A item | Evidence | Producer status | Reviewer action |
|---|-----------|----------|-----------------|-----------------|
| 1 | A-001 2× battery | [A-001](./A-001-measurement-repeatability.md), [run1](./A-001-tool-battery-run1.json), [run2](./A-001-tool-battery-run2.json) | **PASS** (0% session_net variance, 20 rows) | Confirm run1/run2 `session_net_accepted` match |
| 2 | A-002 manual 6/6 | [A-002](./A-002-manual-spotcheck.md) | **PASS** | Spot-check formula vs `format.rs` |
| 3 | A-003 harness shakedown | [A-003](./A-003-harness-shakedown.md), [jsonl](./A-003-mcp-shakedown.jsonl) | **PARTIAL** (MCP PASS) | Confirm compact `tools/list` in jsonl |
| 4 | A-004 equiv audit ≤10% | [A-004](./A-004-equiv-audit.md) | **PASS** (0% FP+FN, n=20) | **Spot-check ≥5 rows** vs battery JSON |
| 5 | compare-results `--preflight` | [G-005](./G-005-compare-results-preflight.md), [inrepo](./G-005-inrepo-preflight.json) | **PARTIAL** (H1/H4/H7 diagnostic) | Accept in-repo preflight or re-run |
| 6 | golden 36 rows | [A-028](./A-028-golden-routes.md), [corpus](../fixtures/routes.golden.jsonl) | **PASS** | Run `node scripts/validate-routes-golden.cjs` |
| 7 | RESULTS.md §8.7 | — | **DEFERRED** (v8 baseline runs only) | Not a Phase 0 GO gate |
| 8 | No 7.21.1 gate | [scope](./phase0-12a-scope-boundary.md) | **PASS** | — |
| 9 | A-005 H1 ≤5kB | [A-005 summary](./A-005-schema-bytes-summary.md), [json](./A-005-schema-bytes.json) | **PASS** (891 B) | Re-run `measure-schema-bytes.ps1` if desired |
| 10 | A-025 edit ≤1.5kB | [surface_probe.rs](../../src/protocol/surface_probe.rs) tests | **PASS** | `cargo test -p symforge --lib surface_probe` |
| 11 | A-019 L0 locked | [A-019](./A-019-l0-surface-choice.md) | **INTERIM** (compact-3 on H1) | Full A/B battery not run — **blocks strict GO** |
| 12 | A-006/A-027 doc | [A-006](./A-006-host-schema.md) | **DOC PASS** | — |
| 13 | A-012 bypass/H3 | [A-012](./A-012-bypass-policy.md) | **DOC PASS** (serve-only interim) | — |
| 14 | P-FF / H6 rules | [A-012](./A-012-bypass-policy.md), [fixtures README](../fixtures/README.md) | **DOC PASS** (4 P-FF rows seeded) | — |
| 15 | A-030 crosswalk | [A-030](./A-030-phase-crosswalk.md) | **PASS** | — |
| 16 | ideation decision log | [ideation.md](../ideation.md) | **PASS** | — |
| 17 | §9 no OPEN blockers | [stel-assumptions.md](../stel-assumptions.md) | **PARTIAL** (A-019 interim) | — |
| 18 | Independent sign-off | [signoff](./phase0-12a-review-signoff.md) | **PENDING YOU** | Required for GO |

**Producer coverage:** **15 / 18** satisfied (2 interim/partial blockers + 1 deferred RESULTS.md + sign-off pending).

---

## 4. Active blockers (verify independently)

| ID | Status | Verify by |
|----|--------|-----------|
| B-A019 | **OPEN (interim)** | [A-019](./A-019-l0-surface-choice.md) — compact-3 locked on H1 only; full L0 A/B not run |
| B-SIGNOFF | **OPEN** | This packet — independent reviewer identity required for GO |
| B-RESULTS | **DEFERRED** | RESULTS.md §8.7 — requires post-8.0 baseline runs; not Phase 0 pre-flight |

**Superseded / closed:**

| ID | Was | Now |
|----|-----|-----|
| B-SFBENCH | external sf-bench missing | In-repo path active; optional legacy harness |
| B-A001 | battery not run | **VALIDATED** — 2× battery, 0% variance |
| B-A004 | equiv audit not run | **VALIDATED** — 20 samples, 0% FP+FN |
| B-A028 | golden not seeded | **VALIDATED** — 36 rows in `docs/fixtures/` |
| B-A005 / B-A025 | schema not measured | **VALIDATED** — 891 B compact, edit ≤1.5 kB |

---

## 5. Key numbers to verify

| Assumption | Artifact | Pass criterion | Recorded |
|------------|----------|----------------|----------|
| A-001 | run1 + run2 JSON | session_net variance ≤ **2%** | **0%** (14,389 both runs) |
| A-002 | A-002 md | **6/6** spot checks | **6/6** |
| A-004 | A-004 md | FP + FN ≤ **10%** over 20 samples | **0%** |
| A-005 | A-005-schema-bytes.json | compact `tools/list` ≤ **5,000 B** | **891 B** |
| A-025 | surface_probe unit test | edit schema ≤ **1,500 B** | PASS |
| A-028 | routes.golden.jsonl | **36** unique valid rows | PASS (validator) |

**Battery corpora:** clone on demand — [tests/fixtures/phase0-corpus/README.md](../../tests/fixtures/phase0-corpus/README.md)

**Daemon hygiene:** scripts set `SYMFORGE_NO_DAEMON=1` so battery runs exit cleanly.

---

## 6. Sign-off template (reviewer fills in)

Copy into [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md):

```yaml
independent_reviewer: "<your name or agent id>"
sign_off_reference: "<PR comment | dated email | review note path>"
review_date: YYYY-MM-DD
decision: GO | NO-GO
checklist_satisfied: <n>
checklist_total_applicable: 18
blocking_gaps:
  - id: "<only if NO-GO>"
    reason: "<why>"
notes: "<optional — cite spot-checks on A-004 rows>"
```

**Producer attestation:** Evidence bundle produced by Cursor agent; commit `46a63c2`. **Not** independently reviewed as of this packet refresh.

---

## 7. Recommended reviewer verdict (producer opinion — not binding)

**NO-GO** until:

1. You complete independent sign-off (T048), **and**
2. You accept **A-019 interim** compact-3 lock **or** require full L0 A/B battery first.

If you accept interim A-019 and spot-check A-004 passes, **GO** is defensible at **15/18** with RESULTS.md explicitly deferred.

First `src/stel/` commit: **NOT AUTHORIZED** without independent GO on refreshed sign-off.
