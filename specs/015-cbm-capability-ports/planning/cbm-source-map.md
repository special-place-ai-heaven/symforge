# CBM Source Map → SymForge Port

**Rule**: Every `[C]` task in S1+ MUST cite at least one row read in its sprint Planning Gate.

**CBM root**: `E:/project/codebase-memory-mcp`

| CBM path | Lines (approx) | Capability | SymForge target | Sprint | Read before |
|----------|----------------|------------|-----------------|--------|-------------|
| `src/mcp/mcp.c` | 311–496 | Tool schemas | `protocol/tools.rs`, `stel/surface_list.rs` | S1–S2 | P-S1-001, P-S2-001 |
| `src/mcp/mcp.c` | 4415–4600 | detect_changes | `git.rs`, `live_index/graph.rs` | S1 | P-S1-010 |
| `src/mcp/mcp.c` | 1685–1800 | search_graph BM25 | `live_index/search.rs` | S1 | P-S1-011 (defer BM25 per D-015-011) |
| `src/cli/hook_augment.c` | all | Grep augment | `cli/hook.rs` | S1 | P-S1-012 |
| `src/pipeline/artifact.c` | all | Team zstd artifact | `live_index/persist.rs` | S1 | P-S1-004 |
| `src/pipeline/pipeline.c` | 1–120 | Index phases | `live_index/store.rs` modes | S1 | P-S1-013 |
| `src/store/store.c` | BFS fns | Graph traversal | `live_index/graph.rs` | S0,S2 | P-S0-002, P-S2-001 |
| `src/store/store.c` | Leiden | Clusters | `live_index/cluster.rs` | S5 | P-S5-005 |
| `src/cypher/cypher.c` | lexer/parser | query_graph | `live_index/cypher/` | S2 | P-S2-002 |
| `internal/cbm/lsp/rust_lsp.c` | all | Rust resolver | `parsing/resolver/rust.rs` | S3 | P-S3-001 |
| `internal/cbm/lsp/ts_lsp.c` | all | TS resolver | `parsing/resolver/typescript.rs` | S3 | P-S3-003 |
| `internal/cbm/lsp/go_lsp.c` | all | (future) Go | deferred S3+ | — | — |
| `internal/cbm/cbm.c` | 497–650 | LSP dispatch | `parsing/mod.rs` | S3 | P-S3-001 |
| `src/semantic/semantic.c` | all | 11-signal | `live_index/semantic.rs` | S4 | P-S4-005 |
| `src/simhash/minhash.c` | all | SIMILAR_TO | partial S4 | S4 | P-S4-008 |
| `internal/cbm/ac.c` | all | Route patterns | `parsing/routes/` | S5 | P-S5-005 |
| `src/pipeline/pass_parallel.c` | all | Parallel index | reference only S6 | S6 | P-S6-005 |
| `src/watcher/watcher.c` | all | Git watcher | compare `watcher/` | S6 | P-S6-008 |
| `src/foundation/mem.c` | budget | RSS back-pressure | document only | S6 | P-S6-005 |

## SymForge files always in scope (read first)

| SymForge path | Why |
|---------------|-----|
| `src/live_index/store.rs` | LiveIndex authority, reload, generation fence |
| `src/live_index/persist.rs` | Snapshot v4, quarantine, checkpoint |
| `src/live_index/query.rs` | find_references, dependents |
| `src/parsing/xref.rs` | Current call extraction baseline |
| `src/protocol/tools.rs` | Tool handlers, what_changed |
| `src/stel/planner.rs` | Intent routing |
| `src/git.rs` | Git diff helpers |
| `src/daemon.rs` | Tool proxy, multi-project |
| `.specify/memory/constitution.md` | Gates |

## Deliberate non-ports

| CBM | Reason |
|-----|--------|
| `internal/cbm/sqlite_writer.c` | No SQLite graph authority |
| `src/ui/*` | 3D UI out of scope |
| `vendored/nomic/*` | Embeddings deferred post-S4 |
| `src/store/store.c` FTS5 | SymForge uses trigram; BM25 optional later |

## MCP dogfood checklist (planning)

During `[P]` tasks, index both repos and record in sprint spec:

1. `E:/project/symforge` — baseline tool behavior
2. `E:/project/codebase-memory-mcp/src` — CBM implementation reference

Record SymForge MCP session notes in `planning/dogfood-notes.md` (create per sprint).
