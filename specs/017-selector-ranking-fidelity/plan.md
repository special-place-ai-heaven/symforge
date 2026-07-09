# Implementation Plan: Selector & Concept-Ranking Fidelity

**Branch**: `017-selector-ranking-fidelity` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/017-selector-ranking-fidelity/spec.md`

## Summary

Two independent, live-dogfood-confirmed defects in the 8.13.8 build:

- **P1 — `edit_plan` cannot resolve `Type::method` selectors.** Root-caused: `plan_edit` (`src/protocol/edit_plan.rs:30`) splits a `X::Y` target via `split_path_qualified_target` and only calls `collect_selector_hits` when the left segment `X` matches a **file path** (`path == target_path || path.ends_with(target_path)`). A type name like `GitRepo` matches no file, so the method is never searched and the tool answers "not found". Fix: when the left segment matches no file, treat it as a **type name** and resolve the right segment as a method whose enclosing `impl`/type is that type. Keep every other selector form byte-identical.
- **P2 — `explore` ranking over-weights symbol-name token overlap.** The scorer lives in the live-index explore query (`src/live_index/query.rs`, ~line 769; reason labels in `src/protocol/format.rs::explore_symbol_reason`), not in `src/protocol/explore.rs` (which is only concept *mapping*). It awards one 1.00 hit on best name-token overlap then craters, missing concept-central symbols that don't share query words. Fix: rebalance scoring so concept relevance (file/import/co-occurrence proximity to the matched concept) counts alongside raw name-token overlap, without over-correcting away from legitimate exact-name matches.

P1 is the crisp, high-value item and ships first; P2 is softer/riskier and ships separately. Both stay inside the in-process LiveIndex read path.

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge`.
**Primary Dependencies**: tree-sitter (parsing), in-process LiveIndex; no new deps.
**Storage**: in-process LiveIndex + local `.symforge/` snapshots (read path only — Constitution I). No new persistence.
**Testing**: `cargo test --all-targets -- --test-threads=1`; regression tests mirror `tests/edit_plan_symbol_line.rs`, `tests/symbol_disambiguation.rs`, and the existing `explore_result_view_*` tests in `src/protocol/format.rs`.
**Target Platform**: cross-platform CLI/MCP server (stdio + `serve`), plus `embed` engine build.
**Project Type**: single-crate MCP code-intelligence server (compiler/analyzer-like).
**Performance Goals**: no regression to explore/edit_plan latency; both run over the existing in-memory index; P1 adds at most one bounded pass over candidate symbols when the file-path interpretation fails; P2 changes score weighting, not retrieval breadth.
**Constraints**: deterministic ordering (IV); frecency-neutral (V); ranked/truncated disclosure preserved (III); stdio↔serve parity (VII); `embed` build stays green (VI).
**Scale/Scope**: ~284 Rust source files / ~21k symbols in this repo; the two anchor queries and five `Type::method` selectors are the measurable acceptance set.

## Constitution Check

*GATE: evaluated against `.specify/memory/constitution.md` v1.0.0. Re-checked post-design below.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Local-First In-Process Index | PASS | Both fixes read from the existing LiveIndex; no second index, no external store, no SQLite graph. P1 reuses `resolve_symbol_selector` + impl-range containment already in the index; P2 rebalances an existing scorer. |
| II. MCP-Native Surface | PASS | No new tool; both are behavior improvements to existing `edit_plan` and `explore`/`ask` tools. No chat injection, no client-tool shadowing. |
| III. Trust Envelopes | PASS | `explore` output must keep its "ranked / truncated / map orients, tools prove" disclosure; P2 changes *scores*, not the disclosure. P1 not-found path (FR-004) stays a truthful negative. |
| IV. Determinism & Recovery | PASS (must verify) | Ranking must stay deterministic for identical query+index (stable sort, deterministic tie-break). P1 resolution deterministic. Regression tests assert stable ordering. |
| V. Frecency Invariant | PASS (must verify) | `explore` and `edit_plan` are discovery/planning tools — MUST NOT bump frecency. The P2 rebalance MUST NOT introduce a frecency read that writes back. A test asserts frecency is not bumped by explore. |
| VI. Embed Isolation (G-045) | PASS (must verify) | Changes are in `edit_plan.rs`, `query.rs`, `disambiguation.rs`, `format.rs` — all reachable under `embed`. Must keep `cargo check --no-default-features --features embed` green; no server/network deps added. |
| VII. Transport Parity | PASS | Both tools route through shared protocol formatters; stdio and serve return the same result for the same index. If a shared formatter changes (explore reason/score), parity holds because both transports call it. |
| VIII. Verification Before Done | PASS (gate at end) | Full gate — fmt/check/clippy -D warnings/test --all-targets --test-threads=1/build --release — plus the embed check, must be green with the new regression tests before "done". |

**No violations. Complexity Tracking is empty (below).**

## Project Structure

### Documentation (this feature)

```text
specs/017-selector-ranking-fidelity/
├── plan.md              # This file
├── research.md          # Phase 0 — root causes + mechanism decisions
├── quickstart.md        # Phase 1 — runnable acceptance checks
├── checklists/
│   └── requirements.md  # spec quality checklist (from /speckit-specify)
└── tasks.md             # Phase 2 — /speckit-tasks (not created here)
```

`data-model.md` and `contracts/` are **N/A** for this feature: it introduces no new data entities and no new external interface — it corrects the behavior of two existing MCP tools. The "contract" is the observable tool output asserted by the regression tests and `quickstart.md`.

### Source Code (repository root)

```text
src/
├── protocol/
│   ├── edit_plan.rs        # P1: split_path_qualified_target / plan_edit — Type::method resolution
│   ├── explore.rs          # (concept mapping only; likely unchanged)
│   └── format.rs           # P2: explore_symbol_reason + explore_result_view_* (reason/score presentation + tests)
└── live_index/
    ├── query.rs            # P1: resolve_symbol_selector + SymbolSelectorMatch; P2: explore scoring (~line 769)
    └── disambiguation.rs   # P1: resolve_symbol_selector helpers, kind_disambiguation_tier

tests/
├── edit_plan_symbol_line.rs      # P1 regression home (Type::method resolution + disambiguation)
├── symbol_disambiguation.rs      # P1 disambiguation coverage
└── (explore ranking assertions)  # P2 — anchor-query top-N tests (unit tests in query.rs/format.rs or an integration test)
```

**Structure Decision**: Single-crate, in-place edits to the two existing tool paths. No new modules, no new files beyond tests. P1 and P2 touch disjoint code (edit_plan/selector vs explore scoring) so they are independently implementable and verifiable.

## Complexity Tracking

*No constitution violations — no entries.*

## Post-Design Constitution Re-Check

Re-evaluated after the design in `research.md`: still PASS on all eight. The two must-verify items (V frecency neutrality, VI embed build) are covered by explicit tasks and gate checks; IV determinism is covered by stable-sort + regression tests. No new complexity introduced.
