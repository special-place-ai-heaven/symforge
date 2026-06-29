# File Touch Matrix — Program 015

**Legend**: ● new file | ◐ major change | ○ minor/wiring | — not touched

## Program-wide

| Path | S0 | S1 | S2 | S3 | S4 | S5 | S6 | Notes |
|------|----|----|----|----|----|----|-----|-------|
| `src/live_index/mod.rs` | ◐ | ○ | ◐ | ◐ | ◐ | ◐ | ○ | mod declarations |
| `src/live_index/store.rs` | ○ | ◐ | ◐ | ◐ | ◐ | ○ | ○ | graph patch, modes |
| `src/live_index/persist.rs` | ◐ | ● | ○ | ◐ | ○ | ○ | ○ | artifact, snap v5 |
| `src/live_index/query.rs` | — | ○ | ○ | ○ | ○ | ○ | — | |
| `src/live_index/search.rs` | — | ◐ | ○ | ○ | ○ | ○ | — | rank |
| `src/live_index/graph.rs` | ● | ◐ | ● | ◐ | ◐ | ◐ | ○ | core new |
| `src/live_index/cypher/` | — | — | ● | ○ | ○ | ◐ | — | new dir |
| `src/live_index/semantic.rs` | — | — | — | — | ● | ○ | — | |
| `src/live_index/cluster.rs` | — | — | — | — | — | ● | — | |
| `src/live_index/diagnostics.rs` | — | — | — | — | — | — | ● | |
| `src/live_index/traces.rs` | — | — | — | — | — | — | ● | |
| `src/domain/index.rs` | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ○ | types |
| `src/parsing/mod.rs` | ◐ | ○ | ○ | ● | ○ | ◐ | — | resolver hook |
| `src/parsing/xref.rs` | ○ | ○ | ○ | ◐ | ○ | ○ | — | baseline |
| `src/parsing/resolver/` | ◐ | — | — | ● | ○ | ○ | — | new dir |
| `src/parsing/routes/` | — | — | — | — | — | ● | — | new dir |
| `src/git.rs` | — | ● | ○ | — | — | — | — | merged diff |
| `src/protocol/tools.rs` | — | ● | ● | ○ | ◐ | ◐ | ◐ | new tools |
| `src/protocol/format.rs` | — | ◐ | ◐ | ○ | ○ | ◐ | ○ | output |
| `src/protocol/resources.rs` | — | ○ | ◐ | — | — | ◐ | ◐ | schema, adr |
| `src/protocol/search_tools.rs` | — | ◐ | — | — | — | — | — | |
| `src/stel/planner.rs` | — | ◐ | ◐ | — | ◐ | ◐ | — | intents |
| `src/stel/handler.rs` | — | ◐ | ◐ | — | ○ | ○ | — | |
| `src/stel/surface_list.rs` | — | ○ | ○ | — | ○ | ○ | ◐ | descriptions |
| `src/cli/hook.rs` | — | ● | — | — | — | — | ○ | augment |
| `src/cli/mirror.rs` | — | — | — | — | — | — | ● | |
| `src/cli/init.rs` | — | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | tool names |
| `src/cli/mod.rs` | — | ○ | — | — | — | — | ◐ | cli subcmd |
| `src/sidecar/handlers.rs` | — | ◐ | ○ | — | — | ◐ | — | hook + arch |
| `src/daemon.rs` | — | ○ | ○ | ○ | — | — | ○ | proxy |
| `src/main.rs` | ○ | ◐ | — | — | — | — | ◐ | import, diag |
| `src/paths.rs` | — | ◐ | — | — | — | — | ◐ | adr path |
| `Cargo.toml` | ◐ | ◐ | — | — | — | — | — | zstd? |

## Test files (created per sprint)

| Path | Sprint | Purpose |
|------|--------|---------|
| `tests/cbm_spike_*.rs` | S0 | Spikes |
| `tests/detect_impact.rs` | S1 | US1 |
| `tests/team_artifact.rs` | S1 | US2 |
| `tests/graph_augmented_search.rs` | S1 | US3 |
| `tests/pagination_envelope.rs` | S1 | US4 |
| `tests/hook_augment.rs` | S1 | US4 |
| `tests/graph_projection.rs` | S2 | US5 |
| `tests/trace_path.rs` | S2 | US6 |
| `tests/query_graph.rs` | S2 | US7 |
| `tests/rust_resolver.rs` | S3 | US8 |
| `tests/typescript_resolver.rs` | S3 | US9 |
| `tests/semantic_edges.rs` | S4 | US10 |
| `tests/route_extraction.rs` | S5 | US11 |
| `tests/architecture_clusters.rs` | S5 | US12 |
| `tests/manage_adr.rs` | S6 | US13 |
| `tests/diagnostics_ndjson.rs` | S6 | US14 |
| `tests/cli_mirror.rs` | S6 | US15 |

## Files explicitly frozen (no touch without decision-log)

- `src/protocol/edit*.rs` — edit moat unless US requires re-index hook only
- `src/embed.rs` contract test list
- `src/stel_core/*` — economics orthogonal

## Blast radius review (before each sprint Planning Gate)

Answer in sprint spec:

1. Does this sprint change snapshot version?
2. Does it add default MCP tools (compact surface)?
3. Does it introduce new persistent stores?
4. Does it affect daemon protocol?

If any YES → decision-log entry required.
