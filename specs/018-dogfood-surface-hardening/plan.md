# Implementation Plan: Dogfood Surface Hardening

**Branch**: `018-dogfood-surface-hardening` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/018-dogfood-surface-hardening/spec.md`

## Summary

Four independently-verified, code-disjoint defects from the Grok dogfood report (`docs/grok_report.md`) make the 36-tool surface noisy or leaky on real repos. Fix each with the smallest principled change, in priority order, each with a fail-first regression test:

- **US1 (P1, MVP)** — `what_changed` (uncommitted) and `detect_impact` default to source-focused so non-source data files (and their symbols) don't dominate; explicit opt-in still includes them. Root: `tools.rs` `code_only.unwrap_or(false)` (~6958/7035/7109), `resources.rs` uncommitted resource, `git.rs uncommitted_paths`, `graph.rs compute_impact` starting-set.
- **US2 (P2)** — `search_symbols` browse mode (empty query + scope filter) ranks by importance (reference count → kind → path) instead of path order. Root: `search.rs:870`, `view.rs:346`.
- **US3 (P3)** — `get_repo_map` full detail applies a workspace-root containment guard so no out-of-root path appears. Root: `query.rs:2388 capture_repo_outline_view`.
- **US4 (P4)** — big-response builders run the CCR decision on the complete payload before the final hard token cut and always emit the `symforge_retrieve` footer on truncation. Root: `ccr.rs:186 apply_ccr_overflow`.

## Technical Context

**Language/Version**: Rust 2024 (single crate `symforge`).

**Primary Dependencies**: tree-sitter (parsing), git2 (`uncommitted_paths`), in-process LiveIndex, STEL controller, MCP protocol surface. No new dependencies.

**Storage**: in-process LiveIndex + local `.symforge/` snapshots. No external store (Constitution I).

**Testing**: `cargo test --all-targets -- --test-threads=1`; per-story fail-first regression tests colocated with existing tool tests (`src/protocol/tools.rs` tests, `src/live_index/*` tests, or `tests/*.rs` integration files).

**Target Platform**: local dev host (Windows/Linux/macOS); stdio + `serve` transports.

**Project Type**: single-crate Rust MCP server (code-intelligence). Not web/mobile.

**Performance Goals**: no regression; browse ranking stays within the existing result cap; root guard and CCR decision are O(files)/O(payload) on already-bounded outputs.

**Constraints**: Constitution I/III/IV/V/VI/VII/VIII (below). Frecency-neutral read paths, deterministic ordering, embed build green, stdio↔serve parity, full gate.

**Scale/Scope**: four small-to-medium fixes; net additive; no schema/persistence change.

## Constitution Check

*GATE: evaluated against constitution v1.0.0. All eight principles.*

| # | Principle | Assessment | Verdict |
|---|-----------|------------|---------|
| I | Local-First In-Process Index | All fixes read from the existing LiveIndex; no second index, no external store. US1 admission-tier option (if taken) changes classification within the one index, not a new store. | PASS |
| II | MCP-Native Surface | No new tools; behavior modes of existing tools (`what_changed`/`detect_impact`/`search_symbols`/`get_repo_map`) and the CCR/`symforge_retrieve` path. No chat injection, no client-tool shadowing. | PASS |
| III | Trust Envelopes | US1 keeps mode/filter disclosure; US2/US3 stay ranked/guarded with disclosure; **US4 strengthens** the truncation envelope (the retrieve footer is exactly the trust signal). | PASS |
| IV | Determinism & Recovery | US2 adds explicit deterministic tie-breaks; US1/US3/US4 are pure read-path filters with stable ordering. Tests assert determinism. | PASS |
| V | Frecency Invariant | All four are discovery/read paths and MUST NOT write frecency. US2 ranking *reads* reference counts (not frecency) and writes nothing back. Frecency-neutrality test per touched path. | PASS |
| VI | Embed Isolation | Changes live in query/format/git/graph paths already present in `embed`; no server/network dep added. `cargo check --no-default-features --features embed` gate in Polish. | PASS |
| VII | Transport Parity | Behaviors flow through shared handlers/formatters both transports call. If a shared formatter signature changes (US4 footer), add/settle a parity assertion. | PASS (verify in Polish) |
| VIII | Verification Before Done | Full gate + embed check + fail-first tests per story before any completion claim. | PASS |

**No violations → Complexity Tracking empty. Cleared for Phase 0.**

## Project Structure

### Documentation (this feature)

```text
specs/018-dogfood-surface-hardening/
├── plan.md              # This file
├── research.md          # Phase 0 — the four design decisions (root cause + chosen approach)
├── data-model.md        # Phase 1 — touched behavioral entities (no new persistence)
├── quickstart.md        # Phase 1 — runnable validation (live MCP calls + cargo tests)
├── contracts/
│   └── tool-behavior.md # Phase 1 — observable contract deltas per tool
├── checklists/
│   └── requirements.md  # spec quality checklist (16/16 pass)
└── tasks.md             # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

Single-crate Rust. The four stories touch disjoint files:

```text
src/
├── protocol/
│   ├── tools.rs         # US1: what_changed/detect_impact code_only default; US4: big-response builders
│   ├── resources.rs     # US1: uncommitted resource default
│   ├── ccr.rs           # US4: apply_ccr_overflow / footer emission ordering
│   └── format.rs        # US4: (only if a shared formatter footer path changes → parity check)
├── live_index/
│   ├── search.rs        # US2: browse-mode detection + importance ranking (search.rs:870)
│   ├── view.rs          # US2: sibling empty-query guard (view.rs:346)
│   ├── query.rs         # US3: capture_repo_outline_view root guard (query.rs:2388)
│   └── graph.rs         # US1: compute_impact starting-set (only if defaulting alone is insufficient)
└── git.rs               # US1: uncommitted_paths (only if source filter is pushed to the git layer)

tests/                   # per-story fail-first regression tests (or colocated #[cfg(test)] modules)
```

**Structure Decision**: Single project (Option 1). No new modules; each story is a surgical change to an existing file plus a regression test. Stories are disjoint enough to land and verify independently in priority order (US1 → US2 → US3 → US4), matching the spec's independent-shippability requirement.

## Complexity Tracking

> No Constitution Check violations. Section intentionally empty.
