# Parity Backlog — Superiority Expansions (post-core)

**North star**: [spec.md § Superiority doctrine](../spec.md#superiority-doctrine) — default adopt; skip inferior mechanisms only.

**Scope**: Items **not** in S0–S6 core path but **scheduled** so CBM parity does not become "optional forever." Each row must pass the ship check before merge.

**Core program**: S0 gate → S1–S6 ([sprints.md](../sprints.md), [tasks.md](../tasks.md)).

---

## Backlog table

| ID | CBM capability | SymForge target | Target sprint | Depends on | Superiority claim |
|----|----------------|-----------------|---------------|------------|-------------------|
| PB-01 | BM25 / FTS rank | Structural rank + **trigram/BM25 hybrid** in `live_index/search.rs` (no SQLite FTS) | **S7** (8.16.x) | S1 rank baseline | Better discovery, same token path |
| PB-02 | Nomic / embedding semantic | Optional local embed pass; STEL find keywords | **S8** (8.17.x) | S4 algorithmic semantic | Vocabulary-mismatch recall |
| PB-03 | 158-grammar **breadth** | **Tier B** generic tree-sitter walker (defs/calls); extension → grammar registry | **S9** (8.18.x) | S3 resolver patterns | Long-tail repos without 140 hand modules |
| PB-04 | Go Hybrid LSP | `parsing/resolver/go.rs` | **S3.1** (8.12.x patch) | S3 Rust/TS ship | Polyglot call graph |
| PB-05 | Python LSP depth | Extend resolver beyond S3 minimum | **S3.1** | S3 | Match CBM 9-lang LSP set |
| PB-06 | Full Cypher subset | Expand `live_index/cypher/` beyond S2 v1 | **S2.1** (8.11.x patch) | S2 query_graph | Power-user graph queries |
| PB-07 | SIMILAR_TO / MinHash | `live_index/similarity.rs` or semantic module | **S4.1** (8.13.x patch) | S4 semantic | Near-duplicate detection |
| PB-08 | CBM parallel index ideas | Cherry-pick from `pass_parallel.c` if index perf lags | **S10** (as needed) | S1 index modes + perf smoke | Faster large-repo index |
| PB-09 | Cross-project graph reads | Graph tools scoped per 012 Phase 3 | **012 Phase 3** | S2 graph + 012 daemon | Multi-repo agents |

---

## Tier A vs Tier B (languages)

| Tier | Languages | Depth | Program |
|------|-----------|-------|---------|
| **A** | ~17 today (Rust, Py, TS/JS, Go, Java, C/C++, C#, Ruby, PHP, Swift, Kotlin, Dart, Elixir, Perl, …) | Full symbols + xref + resolver (S3+) | S0–S6 |
| **B** | Long tail (Zig, Lua, Vue, Haskell, …) | Generic AST extraction + text search; promote to A when agent demand + tests justify | **S9** PB-03 |

**Rule**: Never shallow-port 158 hand-written `languages/*.rs` files. Tier B is the superiority path to CBM breadth.

---

## Sprint numbering note

| Label | Meaning |
|-------|---------|
| S7–S10 | New program slices after S6 ops parity; cut separate specs or extend 015 |
| S2.1 / S3.1 / S4.1 | Patch releases on same branch theme if core sprint ships early |
| 012 Phase 3 | Owned by harness-agnostic-mcp; coordinate, don't duplicate |

---

## Gate before scheduling

Each PB item needs at `[P]`:

1. SymForge + CBM code evidence row in [code-evidence.md](./code-evidence.md)
2. Ship check forecast (latency, tokens, capability)
3. `[C]` tasks appended to [tasks.md](../tasks.md) or new `specs/016-*` feature dir

---

## Review

- End of **S6 [V]**: confirm PB-01..09 priorities still correct; bump or cut rows.
- Any PB item that fails spike/falsifier → decision-log entry, not silent drop.
