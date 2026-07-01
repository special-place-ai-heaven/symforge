# Operator Session Report — 2026-06-30 (CBM benchmark + doc tightening)

**For**: the 015 speckit agent · **Branch**: `015-cbm-capability-ports` · **Operator**: rakovnik
**Nature**: out-of-band operator session (benchmark + planning-doc tightening). Does **not** change scope, contracts, or gate decisions. All edits are surgical doc fixes + one new evidence intake.

---

## 1. What happened

1. Installed `codebase-memory-mcp` (v0.10.0) locally and registered it for Claude Code + Cursor (environment only — not in this repo).
2. Built a 1:1 benchmark harness in `E:/project/sf-bench` (a **separate throwaway bench, not product code**) and ran **CBM vs SymForge-compact vs SymForge-full** over 4 repos (tokio/django/typescript/fmt), 9 tasks each, measuring **tokens + warm latency + mcp-calls + index cost**, judged against the bench's shared manual/naive baselines.
3. Recorded results into this feature's `planning/benchmark-intake.md` and derived an evidence-backed "what to adopt from CBM" mapping to the 015 sprints.
4. Audited all ~40 spec docs for staleness and applied the surgical tightenings below.

Benchmark headline (full table: `E:/project/sf-bench/out/COMPARISON.md`):
- CBM **0s cold-start** (persistent SQLite) vs SymForge in-process index **1.3s → 58–63s (TypeScript)** → strongest evidence for **US2 team artifact / SC-002**.
- CBM `trace_path` **72–104 tok** for find-refs (vs SymForge 230–2,869) → **US5/US6 graph projection + trace**, but CBM's answer scored *under-served* (too terse) → port the token win **with** file:line+caller context.
- CBM `get_architecture` **689,943 tok** on TypeScript vs SymForge repo-map 630–1,455 → **US12 clusters with hard output caps (PD-02)**; treat CBM's dump as the falsifier.
- Schema tax/session: CBM 2,897 · SF-compact 1,145 · SF-full 17,641 → confirms keeping compact-3 (SC-007).

---

## 2. Files changed in THIS repo (branch 015) — by me, this session

All on `015-cbm-capability-ports`, uncommitted.

| File | Change | Why |
|------|--------|-----|
| `planning/benchmark-intake.md` | Filled matrix B-001/002/004/005/006 with real numbers; added setup-tax line + append-only session-log row | New operator evidence (the doc's designated purpose) |
| `analyze.md` | Total tasks **151→159 (91/41/27)**; coverage IDs `C-S1-*`→`C-S1A/B-*`; frecency `V-S1-004`→`V-S1B-002`; added dated **patch note** (kept 2026-06-29 findings as history) | Count drift + S1-split remap |
| `checklists/requirements.md` | Task count **151→159 (91/41/27)** (line 27) | Count drift |
| `sprints.md` | Reworded archived S1 stub + version `8.10.x`→`8.10.0+8.10.1`; **5 release-criteria lines**: dead `T056…T220` ranges → links to `tasks.md` § Sprint N (drift-proof) | Dead T-numbering + empty stub |
| `plan.md` | zstd hedge → **adopted** (D-015-009); sequencing `S1`→`S1a/S1b`; doc tree adds `execution-model.md` + `planning/`; CLI mirror `cli/mirror.rs`→`cli/mod.rs` (matches sprints.md S6) | Resolved-decision drift + tree gaps + path mismatch |
| `planning/cbm-source-map.md` | "Read before" IDs `P-S1-*`→`P-S1A/B-*`; IndexMode row sprint `S1`→`S4`, ID `P-S1-013`→`P-S4-007` | S1-split + IndexMode move |
| `execution-model.md` | Task-ID convention **examples** → live IDs (`P-S1A-003`/`C-S1A-003`/`V-S1A-001`) | Examples used renumbered IDs |
| `planning/code-evidence.md` | 3 prose refs `C-S1-001/007/009` → `C-S1A-001` / `C-S1B-001` / `C-S1B-003` | S1-split remap |
| `planning/task-index.md` | **New** — live execution board: all 159 tasks with tristate status (○/◐/●) + per-task blocker + parallel, seeded from tasks.md/parallelism.md | Requested live todo / bootstrap |
| `tasks.md` | One-line link added (header) to the live board | Discoverability (no task content touched) |
| `planning/operator-session-2026-06-30-report.md` | **This file** (new) | Session handoff |

**Deliberately NOT touched** (your call): `tasks.md` (live source of truth — you're actively editing it; P-POL-002, etc.) and the historical `analyze.md` Findings rows I1–I5 (kept as audit changelog).

---

## 3. Flagged for your confirmation (1 residual)

- **`tasks.md:144`** remap note reads `P-S1-013 IndexMode → P-S4-008`, but the actual task is **`tasks.md:254` P-S4-007** ("Design IndexMode enum … **P-S1-013**"). Looks like an `008`/`007` typo in the remap line. I used **P-S4-007** (the real task) in `cbm-source-map.md`. Please confirm and fix line 144 if it's a typo — I left `tasks.md` untouched on purpose.

---

## 4. Recommended next actions

1. **Re-run `/speckit-analyze`** before the S1a Planning Gate (already listed in `analyze.md` Next Actions). My count/coverage patches are interim; the analyzer regenerates them authoritatively from current `tasks.md`.
2. **Confirm tasks.md:144** (P-S4-008 → P-S4-007) per §3.
3. **Benchmark gap B-003** (change impact): not run this session. Adding a "T7 detect_impact vs detect_changes" scenario to the sf-bench battery would fill it and directly inform the US1/detect-impact contract.
4. **Prioritization signal** from the evidence (see `benchmark-intake.md` + COMPARISON §5): US2 (team artifact) ≫ US5/US6 (graph+trace) ≫ US12 (clusters, capped). The data backs S1a's artifact work as highest-ROI.

---

## 5. Context: the bench (not in this repo)

`E:/project/sf-bench/` — standalone benchmark rig (Node, untracked, not product code). Artifacts: `out/COMPARISON.md` (full 4-axis table + §5 adopt-from-CBM), `out/results-{cbm,sf-compact,sf-full}.json`, `out/cbm-index.json`. One incidental fix lives there only: `lib/mcp.js` got 3 patches so the harness drives SymForge 8.9.7 headless (set `SYMFORGE_WORKSPACE_ROOT`, use `status` not `symforge_status` for compact readiness, set `SYMFORGE_SURFACE=full` explicitly). These are bench-client fixes — **no SymForge product code was modified**.
