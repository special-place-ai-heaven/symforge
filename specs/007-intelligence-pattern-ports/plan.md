# Implementation Plan: Intelligence Pattern Ports

**Branch**: `007-intelligence-pattern-ports` | **Date**: 2026-06-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/007-intelligence-pattern-ports/spec.md`

## Summary

Port four selective code-intelligence UX patterns onto SymForge's existing
LiveIndex + STEL stack, all via shared protocol formatters (so stdio and serve
stay at parity) and all reusing existing in-crate machinery (no new index, no new
dependency, no new public tool):

- **P1 — Impact footer**: append `[impact: N dependents · cochanges: a, b, c]` to
  every successful structural-edit response, using
  `capture_find_dependents_view().files.len()` + `git_temporal().co_changes`.
- **P1 — Orientation doctrine**: add "map orients / tools prove" + "absence ≠
  absence" + truncation disclosure to the onboarding/architecture prompts and the
  compact `get_repo_map` footer (covers the `symforge://repo/map` resource).
- **P2 — Ranked compact map**: order the `detail=compact` map's file entries by
  `(dependents desc, churn desc, path asc)` and annotate `path (→N)` for `N≥2`;
  leave `full`/`tree` byte-unchanged.
- **P2 — Find fusion + impact intent**: fuse multi-word symbol+path ranking with
  co-change boost inside the STEL find intent via `rank_signals::combine`; chain
  dependents+co-change in the impact intent; add a co-change line to `edit_plan`.

Full technical grounding (with symbol/line anchors and the brief-premise
corrections) is in [research.md](./research.md); shapes in
[data-model.md](./data-model.md); behavior in [contracts/](./contracts/).

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge`

**Primary Dependencies**: existing crate only — `rmcp` (MCP), tree-sitter,
in-process LiveIndex, `GitTemporalIndex`, `rank_signals`. No new dependency; no
`rusqlite` for queries.

**Storage**: in-process LiveIndex + local `.symforge/` snapshots; co-change from
`GitTemporalIndex` (async, gated on `Ready`). No new persistent store.

**Testing**: `cargo test --all-targets -- --test-threads=1`; new integration
tests `tests/impact_footer.rs`, `tests/compact_map_ranking.rs`,
`tests/stel_find_fusion.rs`; reuse `tests/frecency_ranking.rs`,
`tests/edit_plan_symbol_line.rs`.

**Target Platform**: local developer machine (Windows/macOS/Linux); MCP over
stdio and `symforge serve` `/mcp` (parity target, no serve code touched).

**Project Type**: single-crate Rust MCP server (compiler/CLI-style; not web/mobile).

**Performance Goals**: footer/ranking add bounded work per request; rank only the
compact entry-point set (no O(files²)); `git_temporal()` is a lock-free `Arc`
snapshot. No regression to existing tool latency.

**Constraints**: frecency must not be bumped by discovery/find/map/impact;
`embed` build must stay server/network-free; trust envelope unchanged; footer
text free of `classify_edit_output` sentinels; deterministic ordering.

**Scale/Scope**: ~9 source modules touched (format.rs, edit_tools.rs, prompts.rs,
tools.rs, query.rs / sidecar handlers.rs, planner.rs, smart_query.rs,
edit_plan.rs) + 3 new test files; ~5 fixtures.

## Constitution Check

*GATE: evaluated before Phase 0 and re-checked after Phase 1 design.*

| # | Principle | Verdict | Evidence / how this plan complies |
|---|-----------|---------|-----------------------------------|
| I | Local-first in-process index | PASS | No second/persistent index; all data from LiveIndex reverse-import + `GitTemporalIndex` (research R1/R3/R7). No SQLite Soul Map. |
| II | MCP-native surface | PASS | Footer/doctrine/ranking ride existing tools/prompts/resources; no chat injection; no client-tool interception; no new public tool (FR-011). |
| III | Trust envelopes | PASS | Footer is additive plain suffix, trust envelope unchanged (FR-004); doctrine reuses "Completeness"/"truncated by result cap" vocabulary (R2). |
| IV | Determinism & recovery | PASS | Stable tie-breaks (FR-017); footer persisted in idempotency replay (append before `complete_mutation_replay`). |
| V | Frecency invariant | PASS | New paths never call `bump_frecency`; `*_does_not_bump` tests added (R6, FR-014). |
| VI | Embed isolation (G-045) | PASS | In-crate reuse only; gate runs `cargo check --no-default-features --features embed` (R7). |
| VII | Transport parity | PASS | All edits in shared protocol formatters; no transport-specific behavior; no serve/auth code (R0). |
| VIII | Verification before done | PASS | Full gate in quickstart; behavior proven by new tests, not code-reading. |

**Result**: no violations → Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/007-intelligence-pattern-ports/
├── plan.md              # This file
├── spec.md              # Feature spec (+ Clarifications)
├── research.md          # Phase 0 — decisions w/ code anchors + brief corrections
├── data-model.md        # Phase 1 — transient entities, reused types
├── quickstart.md        # Phase 1 — validation scenarios + gate
├── contracts/
│   ├── impact-footer.md
│   ├── compact-map-ranking.md
│   ├── stel-find-fusion.md
│   └── orientation-doctrine.md
├── checklists/
│   └── requirements.md  # Spec quality checklist (all pass)
└── tasks.md             # Phase 2 — created by /speckit-tasks
```

### Source Code (repository root) — files this feature touches

```text
src/
├── protocol/
│   ├── format.rs          # NEW compact impact-footer formatter (beside co_changes_result_view); compact-map (→N) render
│   ├── edit_tools.rs      # NEW append_impact_footer helper; call at 7 inner-handler success tails
│   ├── tools.rs           # get_repo_map compact footer (doctrine + map ranking wiring); impact intent envelope
│   ├── prompts.rs         # orientation doctrine in onboard + architecture instruction builders
│   ├── edit_plan.rs       # co-change line in plan_edit (symbol branch)
│   ├── smart_query.rs     # find-intent multi-term classification support (fusion)
│   └── resources.rs       # (assert-only: repo-map resource doctrine test; body comes from tools.rs)
├── live_index/
│   ├── query.rs           # dependents/co-change accessors (reuse); compact entry-point ranking inputs
│   ├── rank_signals.rs    # reuse combine/RankCtx for find fusion (no signature change expected)
│   └── git_temporal.rs    # reuse GitFileHistory.co_changes / churn_score (read-only)
├── sidecar/
│   └── handlers.rs        # repo_map_text compact entry-point ordering + (→N) annotation
└── stel/
    └── planner.rs         # route_find fusion (multi-term symbol+path, co-change boost)

tests/
├── impact_footer.rs        # NEW
├── compact_map_ranking.rs  # NEW
├── stel_find_fusion.rs     # NEW
├── frecency_ranking.rs     # EXTEND (*_does_not_bump for new paths)
└── edit_plan_symbol_line.rs# EXTEND (co-change present/absent)
```

**Structure Decision**: Single-crate Rust MCP server (Option 1, single project).
All changes are surgical edits to existing `src/protocol`, `src/live_index`,
`src/sidecar`, `src/stel` modules plus three new integration test files. No new
crate, module tree, or feature flag. The `embed`/`server` split is preserved.

## Phasing (maps to tasks.md)

- **Phase 0 — Setup**: fixtures (dependent edges + git history), document 004
  waiver. (No 004 serve code.)
- **Phase 1 — P1 quick wins**: impact-footer helper + 7-tail wiring + tests;
  orientation doctrine in prompts + compact footer + snapshot tests.
- **Phase 2 — P2 ranking & find**: ranked compact map (`repo_map_text`) + tests;
  STEL find fusion + tests; impact intent + `edit_plan` co-change line.
- **Phase 3 — Polish**: golden replay rows for new intents; release-notes;
  quickstart verification pass + full gate.

## Complexity Tracking

> No Constitution Check violations — no entries.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| (none) | — | — |
