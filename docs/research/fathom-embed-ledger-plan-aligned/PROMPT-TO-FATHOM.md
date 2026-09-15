# Prompt for the Fathom coding agent (paste into that terminal)

You are the Fathom coding agent. SymForge's session has the plan-aligned ledger and has **stopped for owner approval**. Do not wait for SymForge gates to go green. Do not edit SymForge's first ledger. Do not invent owner decisions.

## What SymForge already did

- Branch `feature/fathom-embed-contract` HEAD is `v11.2.0` / `b549aa7`. Dirty tree on top. First ledger `docs/research/fathom-embed-ledger/` is **superseded and untouched**.
- Active ledger: `docs/research/fathom-embed-ledger-plan-aligned/` (11:02 plan: Task 3 = 1s close, Task 6 = embed `search_knowledge`).
- Inspect only: `--status` is **149 unmet / 0 met**. Lint **LINT OK (17 warnings)**. Oracle read. **No `--approve`. No gate run. No commit. No push.**
- v11.2.0 baselines **not recorded**. Drive `E:` has ~10.5 GB free. A detached worktree plus a cold default-feature suite would fill it. Worktree `E:/project/symforge-baseline-v11.2.0` was **not** created.

## What will fail if you assume the current tests are the contract

- `unrelated_open_completes_while_another_root_closes` waits **50 ms** after spawning close. That can pass **before close has entered join**. `leaf-1.2:G12` will catch it. SymForge will replace the sleep with a join-entered signal **after** the owner approves. Do not treat the current test as G11/G12-done.
- Leaf-1.3:G16 **is this checkout** (owner revision 2): real Loading, full reload ≥10 s, twenty timed closes. The 32-file hold is **not** that gate.
- `OperationKind` will gain **`IndexCensus` and `SearchKnowledge` only** (`ALL: [Self; 9]`). That is a major. Fathom does not match `OperationKind` today; when you take the combined tag, extend any exhaustive mirrors. Do not add other variants. Do not treat a `SearchText` refusal as a knowledge-search refusal.

## Files on the SymForge branch (`leaf-1.1:G8`, owner revision 2)

- Task 3 Step 5: `.github/workflows/ci.yml` + `tests/preventive_runtime_dark_v11.rs`.
- Named 200-character harness companion (own commit, not dropped): `src/protocol/tools.rs`, `src/protocol/edit_tools.rs`, `tests/conformance.rs`, `src/stel/surface_list.rs`. Do not depend on MCP copy changes for Fathom embed work.

## What you should do now (Fathom)

1. Keep going on **Fathom-owned** work: frontend census, wave 1, your ledger fix loop, PR #64 Windows CI. Do not block on SymForge `--approve`.
2. Pin Fathom to **today's** SymForge contract: `v11.2.0` / `b549aa7`. Do **not** bump `SYMFORGE_REV` or the lockfile until the owner tags a release that carries **CR-0 + CR-1 + CR-2 + CR-3 + F-1 + F-2** together. A tag with CR-0 and not CR-1 is forbidden.
3. Embed API you may assume only after that tag: additive `IndexCensus` / `CensusFile` / `index_census`, `IndexProgress` `{ files_discovered, files_parsed, symbols_found }`, `index_progress`, `search_knowledge` + `KnowledgeSearchRequest` / `KnowledgeSearchResult` / `KnowledgeMatch`. Until then, Fathom still: `acquire` → `open_embedded_source` → poll `Current` → `search_symbols`. **`search_text` is code-scoped.** Knowledge is `search_knowledge` (server today; embed method is Task 6, not shipped).
4. Do **not** edit `docs/plans/2026-09-15-fathom-embed-alignment.md`, `docs/research/fathom-embed-change-brief.md`, or `docs/research/fathom-embed-ledger/`. If the 11:02 plan moves again, the plan-aligned gates need a re-check — tell the owner, do not silently rewrite SymForge ledgers.
5. If you need something from SymForge before the tag: write a **one-line ask** (missing atom, wrong refusal, close still blocking). Do not send a new ledger.

## Owner decisions (revision 2 — closed)

1. `OperationKind`: new `IndexCensus` + `SearchKnowledge`, major accepted.
2. Close fixture: this checkout, real Loading, G16 as written. Not the 32-file hold.
3. Type names confirmed. Knowledge result is evidence-complete, not a thin text-search clone. Progress field meanings pinned (executable work-set / post-fold parsed / symbols from those files).
4. G8: CI pins are Task 3; tool descriptions are a named companion, not dropped.

SymForge still needs an explicit `--approve` before gates run. You update Fathom's pin only after the combined tag exists.
