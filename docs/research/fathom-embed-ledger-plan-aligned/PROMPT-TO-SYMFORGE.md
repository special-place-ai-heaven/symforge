# Prompt for the SymForge coding agent (paste into that terminal)

You are the SymForge agent on `feature/fathom-embed-contract`. Another session already wrote code and launched the plan-aligned ledger. **Do not redo that work. Do not `--approve` gates. Do not commit or push unless the owner asks.** First ledger `docs/research/fathom-embed-ledger/` is superseded — do not use or edit it.

Governing plan: `docs/plans/2026-09-15-fathom-embed-alignment.md` (11:02: Task 3 = 1s close, Task 6 = embed `search_knowledge`). Active ledger: `docs/research/fathom-embed-ledger-plan-aligned/`. Inspect: **149 unmet / 0 met**, lint **LINT OK (17 warnings)**. Owner approval is still open.

## Code already on the dirty tree (do not re-implement)

### CR-1 / shared-factory lock (this session)

- `src/index_lifecycle/embedded.rs` — `close_one` / `shutdown_owner` keep the map entry so same-root still refuses, **drop the registration mutex before `join`**, remove after. Identity/owner checked on remove.
- `src/live_index/store.rs` — `hold_reload_after_parses_for_test` still releases on cancel; **`hold_reload_through_cancel_for_test`** does not (Drop still releases).
- `tests/embed_bound_index.rs` — `drop_while_loading_returns_within_one_second` (cancel-release hold); **`unrelated_open_completes_while_another_root_closes`** (through-cancel hold). Embed cell green: 3 passed / 4.10s, `-j 8`, `--no-default-features --features embed --test embed_bound_index`.
- **Known hole `leaf-1.2:G12` will catch:** unrelated-open still does `sleep(50ms)` after spawn. It can pass before close is inside `join`. After the owner approves, replace that with a **deterministic join-entered signal**. Do not treat G11/G12 as done.

### CI pin for the new embed integration step (this session; no task Files list)

- `.github/workflows/ci.yml` already had `cargo test --no-default-features --features embed --test embed_bound_index`.
- `tests/preventive_runtime_dark_v11.rs` — `ci.yml` fingerprint **`2b410d713e4822de:15798`**, new `CARGO_LINES` row, assertion **(39, 32, 2)**. Owner revision 2: this pair belongs to Task 3 Step 5.

### MCP tool descriptions (other subagent + this session follow-up; **out of plan Tasks 1–7**)

- `src/protocol/tools.rs` — 33 `#[tool]` descriptions, ASCII, 145–193 chars.
- `src/protocol/edit_tools.rs` — 7 edit-tool descriptions, same cap.
- `tests/conformance.rs` — `tool_descriptions_are_nonempty` requires nonempty ASCII `chars().count() <= 200` on **`tool_definitions()` and `compact_surface_tools()`**.
- `src/stel/surface_list.rs` — compact STEL blurbs: em-dashes replaced with ASCII `-` (pin failed on `is_ascii()` at 83 chars).
- Focused test was green: `cargo test -j 8 --test conformance tool_descriptions_are_nonempty -- --test-threads=1`.
- Do not rewrite descriptions. Do not drop the compact pin. Owner revision 2: these files are the named 200-character harness companion. Isolate as their own commit. Do not drop them.

### Docs this session added (not gates)

- `docs/research/fathom-embed-ledger-plan-aligned/PROMPT-TO-FATHOM.md` — paste brief for the Fathom agent.
- This file.

## What is not done

- No `index_census` / `index_progress` / embed `search_knowledge` yet. Owner revision 2 requires `OperationKind::{IndexCensus, SearchKnowledge}` (`ALL: [Self; 9]`). No new dependency.
- `activation_cut_v11` CR-0 bodies are `#![cfg(feature = "server")]`. Not re-run after the embed cell (do not interleave feature sets in one `target/`).
- v11.2.0 baselines **not recorded**. `E:` ~10.5 GB free; no worktree at `E:/project/symforge-baseline-v11.2.0`. Need owner: `cargo clean` here, another drive, or wait.
- No `--approve`. Approvals bind command, EXPECT, cwd, shell, timeout, PATH. Owner must approve or name the exact ledger files.

## Owner decisions (revision 2 — closed)

1. `OperationKind` gains `IndexCensus` and `SearchKnowledge`. Major accepted. Do not reuse `SearchSymbols` / `SearchText`.
2. G16 fixture is this checkout, real Loading. Replace the 50 ms sleep with a join-entered signal. Do not treat the 32-file hold as G16.
3. Type names stay. Pin C28/C29 in PLAN.md.
4. CI pins are Task 3. Tool descriptions are a named companion commit, not extras to drop.

## What you do next

1. Stay on this dirty branch. No new worktree that abandons it. Do not edit the plan, the brief, or `docs/research/fathom-embed-ledger/`.
2. Until the owner answers 1–4: **idle on gates**. You may read; you may not `--approve`, mutate, or start Task 4–7.
3. After approval, in order: local commit of in-scope WIP (owner must ask — mutant gates refuse dirty files), baselines if disk is freed, fix the 50 ms race, then Tasks 1–7 per `PROMPT.md`. One feature set at a time, `-j 8`, Terminal Commander for long cargo. No push/PR/tag without the owner.
4. One release only: CR-0+CR-1+CR-2+CR-3+F-1+F-2 together. Fathom stays on `v11.2.0` until that tag exists.
