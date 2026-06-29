# Research: CBM Capability Ports

**Feature**: 015 · **Date**: 2026-06-29

## R0 — Program framing

**Decision**: Port CBM capabilities as LiveIndex-derived projections, not SQLite graph clone.

**Rationale**: Constitution I and 007 FR-015 forbid second authoritative index. CBM's
SQLite is query authority; SymForge's moat is byte-exact LiveIndex + edits.

**Alternatives considered**:
- Embed CBM binary as sidecar — rejected (two truths, Windows process overhead).
- Full SQLite graph mirror in `.symforge/` — rejected (Soul Map).

## R1 — CBM reference architecture (verified)

**Source tree**: `E:/project/codebase-memory-mcp/src/`

| Module | Role |
|--------|------|
| `mcp/mcp.c` | 14 tools, pagination contracts |
| `pipeline/pipeline.c` | RAM graph buffer → SQLite dump |
| `store/store.c` | BFS, FTS5, vector search, Leiden |
| `internal/cbm/lsp/*.c` | Hybrid LSP per language |
| `semantic/semantic.c` | 11-signal index-time semantic |
| `pipeline/artifact.c` | zstd team artifact |

**CBM moat**: graph-native query, Hybrid LSP, bundled semantic, team artifacts.
**SymForge moat**: edits, STEL, recovery, resources/prompts.

## R2 — Graph projection design

**Decision**: `GraphProjection` built from `ReferenceRecord` + `ResolvedCall` at index
load and after incremental updates; stored in-memory only.

**Rationale**: Matches Principle I; rebuild cost amortized on snapshot load (same as
trigram index today in `persist.rs`).

**Alternatives**:
- Persist adjacency in snapshot v5 — deferred; rebuild from references is O(edges)
  and simpler for v1.

## R3 — detect_impact vs existing tools

**Decision**: New `detect_impact` tool + STEL impact intent upgrade; keep
`what_changed` and `analyze_file_impact` unchanged.

**Rationale**: CBM merges git sources + symbol blast + risk in one call; chaining
existing tools costs agent round-trips.

**CBM reference**: `mcp.c` `handle_detect_changes` — merges diff + status porcelain
(#520 untracked fix).

## R4 — Team artifact

**Decision**: zstd compress `index.bin`; two tiers (fast watcher / best checkpoint);
`.gitattributes merge=ours` on first export.

**CBM reference**: `pipeline/artifact.c` — VACUUM INTO + zstd -3/-9.

**SymForge delta**: Postcard not SQLite; strip nonessential rebuildable fields in
"best" tier if size critical (document in contract).

## R5 — Hybrid LSP port strategy

**Decision**: Rust-first in `parsing/resolver/`; reverse-engineer CBM `rust_lsp.c`
algorithm structure (use/import/type eval/method dispatch); no FFI to CBM.

**Rationale**: symforge dogfood is Rust; CBM proves in-process resolver works without
LSP subprocess.

**Milestone order**: Rust → TypeScript → Python → Go (matches SymForge language priority).

## R6 — Semantic without embeddings (v1)

**Decision**: Port CBM algorithmic signals (TF-IDF, MinHash on signatures, module
proximity) before Nomic int8 vectors.

**Rationale**: AGENTS.md "start simple"; CBM uses 11-signal edges without query-time
LLM; embeddings optional in S4+ extension.

## R7 — Cypher subset scope

**Decision**: v1 supports MATCH (single pattern), WHERE (comparisons, NOT EXISTS
single-hop), RETURN, LIMIT, count aggregate.

**CBM reference**: `src/cypher/cypher.c` — fail-closed on unsupported.

**Ponytail ceiling**: No variable-length paths `[*1..3]` in v1; add in 8.11.x patch if
needed.

## R8 — Hook augment

**Decision**: Extend existing `src/cli/hook.rs` sidecar path; match CBM
`hook_augment.c` behavior (Grep/Glob only, exit 0 always).

**SymForge already has**: sidecar HTTP hook infra; CBM has broader 11-agent installer —
defer installer expansion to S6 docs only.

## R9 — Spike falsifiers (S0 gate)

| Spike | Falsifier |
|-------|-----------|
| Graph BFS | p95 >200ms depth-5 on symforge repo |
| Artifact | Import corrupts byte-exact content hash |
| Rust resolver | <60% on benchmark set after 2 weeks |

## R10 — Dependencies on 012

Cross-project graph queries defer until `WorkingSet` Phase 3 routing lands; S1–S2 tools
are single-project scoped with `project_root` in envelope.

## R11 — zstd dependency

**Decision**: Check `Cargo.toml` for existing zstd; if absent, add `zstd` crate (pure
Rust safe) — one dependency justified by team artifact (CBM uses zstd 1.5.7).

**Ponytail**: If dependency rejected, use gzip in v1 with documented ratio tradeoff.
