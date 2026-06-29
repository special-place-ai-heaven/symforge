# Sprint 2 Planning Spec — Graph Query Layer

**Status**: draft  
**Release**: 8.11.x  
**User stories**: US5, US6, US7  
**Depends on**: S1 complete, S0 SP-0A GO

## Objective

Production **GraphProjection** + agent-facing **trace_path** + **query_graph**
(Cypher subset) — the graph-native query moat begins here.

## In scope

| US | Deliverable | Contract |
|----|-------------|----------|
| US5 | GraphProjection build/patch | [graph-projection.md](../contracts/graph-projection.md) |
| US6 | trace_path + STEL trace | [trace-path.md](../contracts/trace-path.md) |
| US7 | query_graph + graph-schema resource | [query-graph.md](../contracts/query-graph.md) |

## Out of scope

- Resolved call edges (S3) — use xref Call edges only in S2
- data_flow / cross_service trace modes (document defer)
- Variable-length Cypher `[*n..m]`

## CBM deep-read list

| File | Focus |
|------|-------|
| `store/store.c` | BFS, degree filters |
| `cypher/cypher.c` | supported subset, error messages |
| `mcp.c` trace_path handler | direction, depth, pagination |
| `mcp.c` get_graph_schema | resource parity |

## Architecture ([P] P-S2-010)

```text
LiveIndex load
  → rebuild GraphProjection (from ReferenceRecord)
  → serve trace_path / query_graph

File update (watcher)
  → patch GraphProjection (file-scoped edge rebuild)
```

**Generation fence**: If `project_generation` changes, discard projection rebuild.

## Cypher v1 grammar ([P] P-S2-020)

Document EBNF in contract appendix:

```
match = 'MATCH' pattern where? return limit?
pattern = node rel node
rel = '-[:CALLS]->' | '<-[:CALLS]-'
```

**Unsupported**: record `unsupported: <clause>` errors.

## trace_path vs find_references ([P] P-S2-025)

| Aspect | find_references | trace_path |
|--------|-----------------|------------|
| Hops | 1 | 1–5 |
| Output | flat list | paths (tree) |
| STEL | legacy trace | upgraded trace |
| Breaking | none | none |

## Resource: symforge://repo/graph-schema

Static resource body format ([P] P-S2-030):

```markdown
## Node kinds (counts)
## Edge kinds (counts)
## Example queries (3)
```

## Performance budget

| Op | p95 target |
|----|------------|
| build graph symforge | <2s incremental on load |
| BFS depth-5 | <100ms |
| query_graph 1k rows | <50ms |

## Error catalog

| Error | When |
|-------|------|
| `unsupported: MERGE` | write clause |
| `unsupported: variable-length path` | `[*]` |
| `graph not ready` | index loading |
| `depth exceeds max` | depth>5 |

## Fixtures

- Extend `cbm_impact` with call chain A→B→C for golden trace file
- `tests/fixtures/cbm_cypher/` with dead-code fn `never_called`

## Planning Gate

- [ ] Cypher subset frozen in contract
- [ ] PD-02 not applicable yet
- [ ] R-04 mitigation documented (lazy vs eager build)
- [ ] find_references tests still green

**Sign-off**: _________________ Date: _______

## Linked tasks

tasks.md S2 [P], [C], [V]
