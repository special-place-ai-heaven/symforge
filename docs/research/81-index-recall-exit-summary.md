# 8.1 Index-Recall Program — Exit Summary

**Program:** 8.1 index-recall (gap plan §6.1 Program T2)
**Status:** **CLOSED — VALIDATED 2/4, P-T2 partial**
**Baseline:** `main` @ `5bbde13` (post-#319 TX-04)
**Task plan:** [`specs/003-81-index-recall/tasks.md`](../../specs/003-81-index-recall/tasks.md)

Final closure recorded 2026-06-15 after T2.4 restoration sign-off **GO**
([`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md))
and the row-level restoration landed in **#322**. The restoration was
**retargeted** from `docs/fixtures/routes.golden.jsonl` to the external
A-029 fixture [`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl):
`routes.golden.jsonl` is the **frozen 36-row in-repo sf-bench golden corpus**
(rows pinned via `corpus_for_row_id` in `src/stel/golden_replay.rs`, count
asserted at exactly 36 in three guards), so adding external `tokio/`/`django/`
rows there would require `src/**` changes — out of scope. `routes.golden.jsonl`
is intentionally **unchanged**.

## Program outcome (measurement)

| Field | Value |
|-------|-------|
| Phase 2 entry | 0/4 T2 equiv · **PIVOT** · P-T2 registered |
| Post-program replay | **2/4** T2 equiv · machine **PASS** |
| Program verdict | **VALIDATED** (≥2/4 threshold met) |
| Golden corpus | **Unchanged** — `routes.golden.jsonl` frozen at 36 rows by design |
| Row-level `eligible_h6` | **Restored** in `tests/fixtures/a029-t2/tasks.jsonl` (#322): 2 serve-eligible, 2 bypass-only |
| Final closure | **CLOSED** — T2.4 sign-off **GO**; restoration #322 merged; main CI green |

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
| Restoration applied? | **Yes** — row-level, landed in #322 |
| Restoration target | [`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl) (retargeted; `routes.golden.jsonl` unchanged) |
| Proposal doc | [`A-029-t24-policy-reconsideration.md`](./A-029-t24-policy-reconsideration.md) |
| Sign-off | [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md) — **GO** (2026-06-15) |

**P-T2 final posture (partial):**

| Task | Equiv | Posture |
|------|-------|---------|
| `tokio/t2_block_on` | EQUIVALENT 70.9% | `serve`, `eligible_h6=true` |
| `django/t2_model` | EQUIVALENT 28.2% (≥25%) | `serve`, `eligible_h6=true` |
| `tokio/t2_spawn` | SYMFORGE-LESS 34.5% (<35%) | `bypass`, `eligible_h6=false` |
| `django/t2_queryset` | SYMFORGE-LESS 26.8% (<35%) | `bypass`, `eligible_h6=false` |

P-T2 is **partial**: 2 serve-eligible external T2 rows + 2 bypass-only rows. Not a blanket lift; not full 4/4 closure.

## Scope audit (binding exclusions — honored)

| Excluded scope | Status |
|----------------|--------|
| B-RESULTS / §8.7 | Not touched |
| Persistence / SQLite / EMA→L2 | Not touched |
| H6 / H7 / H8 gate closure claims | **Not claimed** |
| New compact MCP tools | Not added |
| T3 outline program (§6.2) | Not started |
| TX-03 bench remediation | Not started — deferred; not required for this closure |
| Runtime changes (whole program) | **None** — no `src/**` edits in T2.4 proposal (#321) or restoration (#322) |

## Unclaimed gates (explicit)

This closure makes **no H6/H7/H8 PASS claim**. VALIDATED 2/4 is a measurement
verdict on T2 equivalence, not a gate pass.

- **H6** — not claimed. Restoration finalizes the row-level eligible posture in the
  external A-029 fixture only; no H6 gate PASS is asserted.
- **H7** — not evaluated by this program
- **H8** — not evaluated by this program
- **B-RESULTS** — out of scope
- **Persistence / SQLite / EMA→L2** — Phase 3, out of scope
- **T3 outline program / deploy** — not started, out of scope

## Closure record

| Step | Status |
|------|--------|
| T2.4 sign-off (independent reviewer) | **GO** — [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md) (2026-06-15) |
| Row-level restoration | **Merged** — #322 (retargeted to `tests/fixtures/a029-t2/tasks.jsonl`) |
| Main CI after #322 | **Green** |
| Milestone sign-off (T058) | Recorded in [`81-index-recall-review-signoff.md`](./81-index-recall-review-signoff.md) |
| Optional follow-up | TX-03 bench tranche for `tokio/t2_spawn` gap class — **deferred**, not required |

## Evidence index pointer

Primary artifacts:

- [`81-index-recall-evidence-index.md`](./81-index-recall-evidence-index.md)
- [`A-029-t2-replay.json`](./A-029-t2-replay.json)
- [`A-029-t24-policy-reconsideration.md`](./A-029-t24-policy-reconsideration.md)
- [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md)

**8.1 index-recall is CLOSED at VALIDATED 2/4 with P-T2 partial.** No H6/H7/H8
PASS claim, no B-RESULTS, no persistence/EMA→L2, no T3/deploy work.
