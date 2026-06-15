# T2.4 — Golden Row Restoration Sign-off Packet

**Gate:** Blocks golden-row and `eligible_h6` edits until **GO**
**Program:** 8.1 index-recall · T2.4
**Replay evidence:** [`A-029-t2-replay.json`](./A-029-t2-replay.json)
**Policy proposal:** [`A-029-t24-policy-reconsideration.md`](./A-029-t24-policy-reconsideration.md)
**Evidence producer:** T2.4 proposal branch (`cursor/81-index-recall-t24-reconsideration`)

## Decision

| Field | Value |
|-------|-------|
| **Reviewer** | Independent reviewer (not evidence producer) |
| **Evidence producer** | Cloud agent — T2.4 proposal packet (#321) |
| **Date** | 2026-06-15 |
| **Decision** | **GO** — row-level restoration authorized (retargeted; see note below) |

## Implementation note — restoration retarget (2026-06-15)

During restoration the authorized target was changed from
[`docs/fixtures/routes.golden.jsonl`](../../docs/fixtures/routes.golden.jsonl) to
[`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl).

**Reason:** `routes.golden.jsonl` is the frozen 36-row **in-repo sf-bench** golden
corpus. Its rows are pinned to local corpora (`cfg-if/`, `records/`, `is-plain/`,
`compression/`) via `corpus_for_row_id` in [`src/stel/golden_replay.rs`](../../src/stel/golden_replay.rs),
and the row count is asserted at exactly 36 in three guards
(`scripts/validate-routes-golden.cjs`, `src/stel/golden_replay.rs` tests, and
`tests/stel_golden_replay.rs` / `tests/stel_l3_enforcement.rs` which panic on any
unmapped id). Adding external `tokio/`/`django/` rows there would require `src/**`
runtime/test changes and break those guards — outside the authorized scope.

`tokio`/`django` are **external A-029 reference fixtures**, so their row-level
serve/eligibility posture is correctly recorded in `tests/fixtures/a029-t2/tasks.jsonl`
(their data home). `routes.golden.jsonl` is intentionally left **unchanged**.

This retarget does not alter the row-level decision: only the two EQUIVALENT rows
(`tokio/t2_block_on`, `django/t2_model`) move to `serve` + `eligible_h6=true`; the two
SYMFORGE-LESS rows remain `bypass` + `eligible_h6=false`.

## Replay summary (binding inputs)

| Field | Value |
|-------|-------|
| Replay commit | `5bbde13` (`main` post-#319) |
| Equivalent count | **2 / 4** |
| Machine verdict | **PASS** (≥2/4) |
| Program verdict | **VALIDATED** |

| Task ID | Equivalence | Restoration proposed? |
|---------|-------------|----------------------|
| `tokio/t2_block_on` | EQUIVALENT | **Yes** |
| `django/t2_model` | EQUIVALENT | **Yes** |
| `tokio/t2_spawn` | SYMFORGE-LESS | **No** — remain P-T2 bypass |
| `django/t2_queryset` | SYMFORGE-LESS | **No** — remain P-T2 bypass |

## Exact proposed changes (after reviewer GO only)

**Target file (retargeted):** [`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl) — see [Implementation note](#implementation-note--restoration-retarget-2026-06-15). `docs/fixtures/routes.golden.jsonl` is **unchanged**.

**Current state:** The four external A-029 T2 tasks already exist in `tests/fixtures/a029-t2/tasks.jsonl`. The restoration commit **adds row-level posture fields** (`expected_decision`, `expected_equiv`, `eligible_h6`) to those existing rows; the two EQUIVALENT rows become serve-eligible, the two SYMFORGE-LESS rows stay bypass-only.

### Rows proposed for serve + H6 eligibility

| Row ID | `expected_decision` | `expected_equiv` | `eligible_h6` | `must_call` | Notes |
|--------|---------------------|------------------|---------------|-------------|-------|
| `tokio/t2_block_on` | `serve` | `true` | `true` | `["find_references"]` | A-029 replay EQUIVALENT @ 70.9% recall |
| `django/t2_model` | `serve` | `true` | `true` | `["find_references"]` | A-029 replay EQUIVALENT @ 28.2% recall |

### Rows proposed to remain bypass-only (P-T2)

| Row ID | `expected_decision` | `expected_equiv` | `eligible_h6` | `must_call` | Notes |
|--------|---------------------|------------------|---------------|-------------|-------|
| `tokio/t2_spawn` | `bypass` | `false` | `false` | `[]` | SYMFORGE-LESS @ 34.5% (threshold 35%) |
| `django/t2_queryset` | `bypass` | `false` | `false` | `[]` | SYMFORGE-LESS @ 26.8% (threshold 35%) |

**P-T2 envelope (non-restored rows):** host `rg -r <pattern>` + line window per [`docs/v8-gap-closure-plan.md`](../v8-gap-closure-plan.md) §6.1.

### Assumption register (proposed — restoration commit only)

| Register | Field | Current (`main`) | Proposed after GO |
|----------|-------|------------------|-------------------|
| `docs/stel-assumptions.md` | **A-029** verdict | `PIVOT` (0/4) | `VALIDATED` (2/4) |
| `docs/stel-assumptions.md` | P-T2 posture | Full bypass (4/4) | **Partial** — 2 bypass, 2 serve-eligible |
| `docs/research/A-029-t2-spike.md` | Replay section | Phase 2 only | Append post-program replay pointer |

**Do not edit** `docs/stel-assumptions.md` until this sign-off records **GO**.

### Rows explicitly unchanged

- All 36 existing in-repo golden rows (`cfg-if/*`, `records/*`, `is-plain/*`, `compression/*`)
- All four P-FF bypass rows (`*/pff_*`) — `eligible_h6=false` unchanged
- No TX-03 bench work
- No `src/**` runtime changes in restoration commit beyond golden/assumption/docs

## Reviewer checklist

- [x] Replay evidence valid — [`A-029-t2-replay.json`](./A-029-t2-replay.json) matches [`a029-tx04-results.json`](./a029-tx04-results.json) row data
- [x] **2/4 threshold met** — machine verdict PASS confirmed
- [x] **Row-level restoration only** — equiv rows restored individually; no blanket 4/4 lift
- [x] **Non-equivalent rows remain bypass-only** — `tokio/t2_spawn`, `django/t2_queryset` stay P-T2
- [x] **No H6/H7/H8 claim** — restoration adjusts eligible denominator only; no gate pass asserted
- [x] **No runtime changes** — restoration limited to external fixture + assumption/docs (routes.golden.jsonl unchanged)
- [x] In-repo `t4_refs` rows not conflated with external A-029 T2 tasks
- [x] TX-01/TX-02/TX-04 remediation commits referenced and merged on `main`

## Sign-off record (fill on review)

```yaml
decision: GO  # GO | NO-GO
reviewer: "Independent reviewer (not evidence producer)"
date: "2026-06-15"
comments: "2/4 EQUIVALENT confirmed; replay matches a029-tx04-results.json; row-level scope upheld. Restoration retargeted from routes.golden.jsonl to tests/fixtures/a029-t2/tasks.jsonl (see implementation note)."
restoration_commit_authorized: true
```

**GO** authorizes a **separate** restoration PR implementing the table above.
**NO-GO** retains full P-T2 bypass on all four T2 rows; program exit may still record VALIDATED measurement without golden edits.

## Scope attestation

**Proposal packet (#321, merged `ce7da6f`):**

- [x] Evidence / proposal / sign-off docs only
- [x] No `src/**` diff
- [x] No golden row edits
- [x] No `eligible_h6` data changes on `main`
- [x] Decision recorded **PENDING** at merge

**Restoration commit (this branch, after GO):**

- [x] No `src/**` runtime changes
- [x] `docs/fixtures/routes.golden.jsonl` unchanged (frozen 36-row in-repo corpus)
- [x] Row-level eligibility recorded in `tests/fixtures/a029-t2/tasks.jsonl` (external fixture)
- [x] Only the two EQUIVALENT rows restored to serve + `eligible_h6=true`
- [x] Two SYMFORGE-LESS rows remain bypass-only (`eligible_h6=false`)
- [x] No H6/H7/H8 PASS claim; no TX-03 bench; no compact MCP tool changes
