# 8.1 Index-Recall Program — Exit Summary (Draft)

**Program:** 8.1 index-recall (gap plan §6.1 Program T2)
**Status:** **PENDING RESTORATION SIGN-OFF** — not final closure
**Baseline:** `main` @ `5bbde13` (post-#319 TX-04)
**Task plan:** [`specs/003-81-index-recall/tasks.md`](../../specs/003-81-index-recall/tasks.md)

## Program outcome (measurement)

| Field | Value |
|-------|-------|
| Phase 2 entry | 0/4 T2 equiv · **PIVOT** · P-T2 registered |
| Post-program replay | **2/4** T2 equiv · machine **PASS** |
| Program verdict | **VALIDATED** (≥2/4 threshold met) |
| Golden / `eligible_h6` | **Unchanged** — awaiting T2.4 sign-off |
| Final closure | **Blocked** on [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md) **GO** |

Authoritative replay: [`A-029-t2-replay.json`](./A-029-t2-replay.json)

## Remediation tranches shipped

| Tranche | Focus | PR / commit | Equiv after | Key artifact |
|---------|-------|-------------|-------------|--------------|
| T2.0/T2.1 | Gap audit + taxonomy | #314 | 0/4 | [`A-029-gap-taxonomy.md`](./A-029-gap-taxonomy.md) |
| TX-01 | FM-CAP — serve reference cap | `830ad8f` | 1/4 | [`A-029-tx01-cap-evidence.md`](./A-029-tx01-cap-evidence.md) |
| TX-02 | FM-RG-TEXT — Python xref | #317 | 1/4 | [`A-029-tx02-xref-evidence.md`](./A-029-tx02-xref-evidence.md) |
| TX-04 | FM-TEST — test-path recall | #319 | **2/4** | [`A-029-tx04-tests-evidence.md`](./A-029-tx04-tests-evidence.md) |
| TX-03 | FM-BENCH | **Not started** | — | Deferred |

### Per-row final replay

| Task | Phase 2 recall | Post-TX-04 recall | Equiv |
|------|----------------|-------------------|-------|
| `tokio/t2_spawn` | 7.1% | 34.5% | SYMFORGE-LESS |
| `tokio/t2_block_on` | 14.2% | **70.9%** | **EQUIVALENT** |
| `django/t2_queryset` | 9.9% | 26.8% | SYMFORGE-LESS |
| `django/t2_model` | 4.8% | **28.2%** | **EQUIVALENT** |

## P-T2 policy status

| Item | Status |
|------|--------|
| Reconsideration warranted? | **Yes** (≥2/4) |
| Restoration applied? | **No** — proposal only |
| Proposal doc | [`A-029-t24-policy-reconsideration.md`](./A-029-t24-policy-reconsideration.md) |
| Sign-off | [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md) — **PENDING** |

**Row-level proposal:** restore `tokio/t2_block_on` and `django/t2_model` only; keep `tokio/t2_spawn` and `django/t2_queryset` bypass-only.

## Scope audit (binding exclusions — honored)

| Excluded scope | Status |
|----------------|--------|
| B-RESULTS / §8.7 | Not touched |
| Persistence / SQLite / EMA→L2 | Not touched |
| H6 / H7 / H8 gate closure claims | **Not claimed** |
| New compact MCP tools | Not added |
| T3 outline program (§6.2) | Not started |
| TX-03 bench remediation | Not started |
| Runtime changes in T2.4 proposal packet | **None** |

## Unclaimed gates (explicit)

- **H6** — eligible denominator not finalized until golden restoration lands
- **H7** — not evaluated by this program
- **H8** — not evaluated by this program
- **B-RESULTS** — out of scope
- **Persistence / EMA→L2** — Phase 3

## Remaining work to close program

1. **Independent reviewer** — complete T2.4 sign-off checklist → **GO** or **NO-GO**
2. **Restoration commit** (GO only) — golden rows + `docs/stel-assumptions.md` A-029 update
3. **Milestone sign-off** — [`81-index-recall-review-signoff.md`](./81-index-recall-review-signoff.md) (T058)
4. **Optional follow-up** — TX-03 bench tranche for `tokio/t2_spawn` gap class

## Evidence index pointer

Primary artifacts for this exit draft:

- [`81-index-recall-evidence-index.md`](./81-index-recall-evidence-index.md) (update on final closure)
- [`A-029-t2-replay.json`](./A-029-t2-replay.json)
- [`A-029-t24-policy-reconsideration.md`](./A-029-t24-policy-reconsideration.md)
- [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md)

**This document does not claim final program closure** until restoration sign-off and any authorized restoration commit are complete.
