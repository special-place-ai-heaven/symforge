# T2.4 — Golden Row Restoration Sign-off Packet

**Gate:** Blocks golden-row and `eligible_h6` edits until **GO**
**Program:** 8.1 index-recall · T2.4
**Replay evidence:** [`A-029-t2-replay.json`](./A-029-t2-replay.json)
**Policy proposal:** [`A-029-t24-policy-reconsideration.md`](./A-029-t24-policy-reconsideration.md)
**Evidence producer:** T2.4 proposal branch (`cursor/81-index-recall-t24-reconsideration`)

## Decision

| Field | Value |
|-------|-------|
| **Reviewer** | _Independent reviewer (not evidence producer)_ |
| **Evidence producer** | Cloud agent — T2.4 proposal packet |
| **Date** | _Pending_ |
| **Decision** | **PENDING** |

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

**Target file:** [`docs/fixtures/routes.golden.jsonl`](../../docs/fixtures/routes.golden.jsonl)

**Current state:** No external T2 rows present (36 in-repo rows only). Restoration commit would **add four** T2 reference rows sourced from [`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl).

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

- [ ] Replay evidence valid — [`A-029-t2-replay.json`](./A-029-t2-replay.json) matches [`a029-tx04-results.json`](./a029-tx04-results.json) row data
- [ ] **2/4 threshold met** — machine verdict PASS confirmed
- [ ] **Row-level restoration only** — equiv rows proposed individually; no blanket 4/4 lift
- [ ] **Non-equivalent rows remain bypass-only** — `tokio/t2_spawn`, `django/t2_queryset` stay P-T2
- [ ] **No H6/H7/H8 claim** — restoration adjusts eligible denominator only; no gate pass asserted
- [ ] **No runtime changes** in proposal packet; restoration commit limited to golden + assumption/docs
- [ ] In-repo `t4_refs` rows not conflated with external A-029 T2 tasks
- [ ] TX-01/TX-02/TX-04 remediation commits referenced and merged on `main`

## Sign-off record (fill on review)

```yaml
decision: PENDING  # GO | NO-GO
reviewer: ""
date: ""
comments: ""
restoration_commit_authorized: false
```

**GO** authorizes a **separate** restoration PR implementing the table above.
**NO-GO** retains full P-T2 bypass on all four T2 rows; program exit may still record VALIDATED measurement without golden edits.

## Scope attestation (this proposal packet)

- [x] Evidence / proposal / sign-off docs only
- [x] No `src/**` diff
- [x] No golden row edits
- [x] No `eligible_h6` data changes on `main`
- [x] Decision remains **PENDING**
