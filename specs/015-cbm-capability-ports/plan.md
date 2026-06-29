# Implementation Plan: CBM Capability Ports (Graph Intelligence Program)

**Branch**: `015-cbm-capability-ports` | **Date**: 2026-06-29 | **Spec**: [spec.md](./spec.md)

**Execution model**: [execution-model.md](./execution-model.md) — **60% planning · 30% coding · 10% validation**

**Task list**: [tasks.md](./tasks.md) — `[P]` → `[C]` → `[V]` per sprint; no coding until Planning Gate.

**Planning artifacts**: [planning/README.md](./planning/README.md) · **Code evidence**: [planning/code-evidence.md](./planning/code-evidence.md)

## Summary

Port codebase-memory-mcp's graph-native query, change-impact, team artifacts,
Hybrid LSP resolution, algorithmic semantic relations, and cross-service linking
into SymForge as **derived projections over LiveIndex** — preserving compact STEL
default, symbol editing, byte-exact persistence, and Constitution I (single
authoritative index).

CBM reference: `E:/project/codebase-memory-mcp` (pure C, SQLite graph). SymForge
reimplements algorithms in Rust; no vendored C, no SQLite Soul Map.

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge`.

**Primary Dependencies**: Existing only — tree-sitter, `arc_swap`, `rmcp`, `postcard`,
`rusqlite` (existing frecency/coupling/ledger stores only), `rayon`, optional `zstd`
for artifacts (evaluate: may use `zstd` crate if not already present — check
Cargo.toml before adding).

**Storage**:
- **Authority**: in-process `LiveIndex` + `.symforge/index.bin` (unchanged).
- **Derived**: `GraphProjection` rebuilt on load from references + resolved calls.
- **Team artifact**: `.symforge/index.bin.zst` (bootstrap only, not query authority).
- **ADR**: `.symforge/adr.json` (metadata, not query authority).

**Testing**: Full gate per sprint; new integration tests under `tests/cbm_*` and
`tests/graph_*`; ignored smoke for large-repo perf.

**Target Platform**: Windows/Linux/macOS; stdio + `symforge serve` parity.

**Performance Goals**:
- BFS trace depth-5: p95 <100ms on symforge repo (SC-003)
- Artifact bootstrap: ≥80% index time reduction (SC-002)
- Hook augment: <100ms fail-open (FR-005)

**Constraints**: Constitution I–VIII; 007 FR-015; embed isolation; frecency on
discovery paths forbidden.

## Constitution Check

| Principle | Verdict | Note |
|-----------|---------|------|
| I. Local-First Index | **PASS** | GraphProjection is derived; artifact is snapshot bootstrap only |
| II. MCP-Native | **PASS** | New tools via full surface + STEL intents; ADR as resource |
| III. Trust Envelopes | **PASS** | Pagination honesty; resolver confidence disclosed |
| IV. Determinism | **PASS** | Stable BFS ordering; artifact integrity hash |
| V. Frecency | **PASS** | Tests per tool; impact/trace/search unchanged policy |
| VI. Embed | **PASS w/ design** | Resolver + graph in `live_index`/`parsing`; reachable via embed deep path; contract test excludes volatile internals |
| VII. Transport Parity | **PASS** | Shared formatters in `protocol/format.rs` |
| VIII. Verification | **PASS** | Gate per sprint in quickstart.md |

No unjustified violations.

## Project Structure

### Documentation
```text
specs/015-cbm-capability-ports/
├── spec.md
├── plan.md              # this file
├── sprints.md
├── research.md
├── data-model.md
├── quickstart.md
├── tasks.md             # all sprints, all tasks
├── checklists/requirements.md
└── contracts/
    ├── graph-projection.md
    ├── detect-impact.md
    ├── team-artifact.md
    ├── trace-path.md
    ├── query-graph.md
    ├── hybrid-resolver.md
    └── semantic-edges.md
```

### Source (new + touch)
```text
src/live_index/graph.rs          # GraphProjection, BFS
src/live_index/cypher/           # query subset compiler + executor
src/live_index/semantic.rs       # algorithmic semantic pass
src/live_index/cluster.rs        # architecture clusters
src/live_index/diagnostics.rs    # NDJSON trajectory
src/live_index/traces.rs         # OTLP ingest (minimal)
src/parsing/resolver/            # Hybrid LSP Rust port
│   mod.rs, rust.rs, typescript.rs, registry.rs
src/parsing/routes/              # HTTP route extractors
src/live_index/persist.rs        # zstd artifact export/import
src/protocol/tools.rs            # detect_impact, trace_path, query_graph, manage_adr
src/stel/planner.rs              # trace/impact/find semantic routing
src/cli/hook.rs                  # Grep/Glob augment
src/cli/mirror.rs                # CLI tool mirror (new)
```

## Implementation Sequencing

See [sprints.md](./sprints.md) and [tasks.md](./tasks.md).

1. **S0**: Spikes falsify or confirm graph, artifact, resolver.
2. **S1**: Agent-visible wins (impact, artifact, search rank, hooks).
3. **S2**: Graph + trace + Cypher — unlocks architecture and dead-code workflows.
4. **S3**: Resolver depth — quality jump for trace/impact accuracy.
5. **S4**: Semantic — vocabulary bridging without embeddings.
6. **S5**: Routes + clusters — microservice/monorepo workflows.
7. **S6**: ADR, diagnostics, CLI — operator and team maturity.

## Complexity Tracking

| Item | Why needed | Simpler alternative rejected |
|------|------------|------------------------------|
| GraphProjection module | Multi-hop BFS, Cypher, clusters need adjacency | Repeated find_references chains — slow, no composable query |
| Hybrid resolver | CBM's main accuracy moat for call graphs | tree-sitter xref alone — insufficient cross-file |
| zstd artifact | Team onboarding (CBM proven pattern) | Raw index.bin in git — too large |

## STEL Surface Mapping

| Intent | Absorbs |
|--------|---------|
| `impact` | `detect_impact` (upgrade from what_changed chain) |
| `trace` | `trace_path` multi-hop |
| `find` | semantic keyword param (S4) |
| `orient` | architecture clusters + graph schema resource |
| `read` | unchanged |
| `edit` | unchanged (SymForge moat) |

Compact-3 schema budget: new params on existing STEL tools only; no 4th default tool.
