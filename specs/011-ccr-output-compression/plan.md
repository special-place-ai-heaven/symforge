# Implementation Plan: CCR Output Compression

**Branch**: `011-ccr-output-compression` | **Date**: 2026-06-18 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/011-ccr-output-compression/spec.md`

## Summary

Port Headroom-inspired **reversible bulk compression** and **session dedup**
onto SymForge's existing `SessionContext`, STEL controller, and
`protocol/format.rs` output pipeline — without LLM proxy merge, without new
dependencies, and without CCR on byte-exact edit paths.

Phased delivery:

- **P1 — Session cache hit (US1)**: Extend `detect_session_cache_hit` /
  `SessionContext` to full read tools (`get_file_context`, `get_symbol`,
  `get_file_content`) with `force_refresh` escape hatch; reuse
  `format_cache_hit_body` pattern from `stel/executor.rs`.
- **P1 — CCR-lite (US2)**: New `src/protocol/ccr.rs` module — blob store,
  handle minting, `symforge_retrieve` tool; wire overflow path on
  `search_text`, `search_symbols`, `find_references`, `explore`,
  `get_repo_map(full)` instead of `enforce_token_budget` truncation.
- **P2 — Search compaction + profiles (US3)**: `ToolOutputProfile` table;
  group-by-file ranking in search formatters; error-line preservation.
- **P2 — Dedup hints (US4)**: Footer on forced refresh when prior fetch exists.
- **P3 — Economics (US5)**: Ledger fields `ccr_store` / `ccr_retrieve` on STEL
  events; admin summary counters.

Grounding: [research.md](./research.md). Shapes: [data-model.md](./data-model.md).
Behavior: [contracts/](./contracts/).

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge`

**Primary Dependencies**: existing crate only — `SessionContext`, STEL controller,
`protocol/format.rs`, `blake3` (already used for idempotency hashes — reuse for
CCR keys). No Headroom crate, no Python, no ONNX/tiktoken in v1.

**Storage**: in-memory `CcrStore` per MCP session; optional spill to
`.symforge/session-blobs/<session_id>/` on serve for crash resilience (v1 may
ship memory-only with documented restart invalidation — see research R4).

**Testing**: `cargo test --all-targets -- --test-threads=1`; new integration
tests `tests/ccr_retrieve.rs`, `tests/session_cache_hit.rs`,
`tests/search_compaction.rs`; extend `tests/persist_compression_ratio.rs` guard.

**Target Platform**: local developer machine; MCP stdio + `symforge serve` `/mcp`
(parity required).

**Project Type**: single-crate Rust MCP server.

**Performance Goals**: CCR store lookup O(1); ranking adds bounded work per
search (no full-index resort); cache-hit path skips format + index re-read.

**Constraints**: frecency-neutral discovery; trust envelope on capped output;
embed build stays network-free; byte-exact edit paths exempt from CCR opaque
refs; deterministic handle generation from content hash + session salt.

**Scale/Scope**: ~8 source modules (`protocol/ccr.rs` new, `session.rs`,
`format.rs`, `tools.rs`, `stel/controller.rs`, `stel/ledger_store.rs`,
`server/mcp_http.rs` session wiring if needed) + 3 test files.

## Constitution Check

*GATE: evaluated before Phase 0 and re-checked after Phase 1 design.*

| # | Principle | Verdict | Evidence / how this plan complies |
|---|-----------|---------|-----------------------------------|
| I | Local-first in-process index | PASS | CCR stores **formatted tool output**, not a second symbol index. Reads still resolve from LiveIndex. |
| II | MCP-native surface | PASS | `symforge_retrieve` as new tool (or mode on `inspect_match` — see R5); no chat injection. |
| III | Trust envelopes | PASS | CCR responses include omitted count + handle; search caps disclose ranking; cache-hit names prior fetch. |
| IV | Determinism & recovery | PASS | Handles are content-addressed; same inputs → same ranked summary; blob eviction deterministic; stale handle = explicit error. |
| V | Frecency invariant | PASS | Ranking/compression paths do not call `bump_frecency`; tests assert. |
| VI | Embed isolation (G-045) | PASS | `ccr.rs` + session extensions compile under `embed`; no server-only deps in shared protocol path. |
| VII | Transport parity | PASS | All logic in `protocol/` + shared session; stdio and serve use same `SymForgeServer` dispatch. |
| VIII | Verification before done | PASS | Full gate + new integration tests per quickstart.md. |

**Result**: no violations → Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/011-ccr-output-compression/
├── plan.md              # This file
├── spec.md
├── research.md          # Phase 0 — Headroom port decisions + code anchors
├── data-model.md        # Phase 1 — CcrBlob, SessionFetchRecord, profiles
├── quickstart.md        # Phase 1 — validation scenarios + gate
├── contracts/
│   ├── session-cache-hit.md
│   ├── ccr-retrieve.md
│   ├── search-compaction.md
│   ├── tool-output-profiles.md
│   └── dedup-hint-footer.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 — /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── protocol/
│   ├── ccr.rs              # NEW — CcrStore, handle mint, retrieve handler
│   ├── session.rs          # SessionFetchRecord, cache-hit detection keys
│   ├── format.rs           # CCR overflow path, search rank+group, dedup footer
│   └── tools.rs            # wire retrieve tool, force_refresh, default max_tokens
├── stel/
│   ├── controller.rs       # extend cache hit detection for read intents
│   ├── executor.rs           # format_cache_hit_body reuse
│   └── ledger_store.rs     # ccr_store / ccr_retrieve event columns
└── server/
    └── mcp_http.rs         # per-connection session CcrStore (if not in SymForgeServer)

tests/
├── session_cache_hit.rs
├── ccr_retrieve.rs
└── search_compaction.rs
```

**Structure Decision**: single-crate; new `protocol/ccr.rs` colocated with
formatters (parity boundary). No new top-level crate.

## Complexity Tracking

> No constitution violations requiring justification.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |

## Phase 0 Output

See [research.md](./research.md) — all technical unknowns resolved.

## Phase 1 Output

- [data-model.md](./data-model.md)
- [contracts/](./contracts/)
- [quickstart.md](./quickstart.md)

## Phase 2

See [tasks.md](./tasks.md) — generated by `/speckit-tasks`.
