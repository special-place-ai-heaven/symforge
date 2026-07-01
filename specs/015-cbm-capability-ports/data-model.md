# Data Model: CBM Capability Ports

**Feature**: 015 · **Spec**: [spec.md](./spec.md)

## Derived entities (in-memory, rebuildable)

### GraphProjection

Rebuildable adjacency index over the current LiveIndex snapshot.

| Field | Type | Notes |
|-------|------|-------|
| `nodes` | `HashMap<SymbolId, GraphNode>` | SymbolId = (path, name, kind) or stable index |
| `out_edges` | `HashMap<SymbolId, Vec<GraphEdge>>` | Outbound |
| `in_edges` | `HashMap<SymbolId, Vec<GraphEdge>>` | Inbound (for BFS) |
| `generation` | `u64` | Matches index project_generation fence |

**Lifecycle**: Built after snapshot load + after each incremental file update (batched).

### GraphNode

| Field | Type |
|-------|------|
| `symbol_id` | `SymbolId` |
| `path` | `String` |
| `name` | `String` |
| `kind` | `SymbolKind` |
| `in_degree` | `u32` |
| `is_test` | `bool` |
| `is_entry_point` | `bool` |

### GraphEdge

| Field | Type |
|-------|------|
| `from` | `SymbolId` |
| `to` | `SymbolId` |
| `kind` | `GraphEdgeKind` |
| `confidence` | `f32` | 1.0 for syntactic; resolver-scored for Call |
| `properties` | `HashMap<String, String>` | Optional |

### GraphEdgeKind

```text
Call | Import | Implements | Inherits | HttpRoute | HttpCall
DataFlow | CoChange | SemanticallyRelated | SimilarTo
```

v1 implements: `Call`, `Import`, `CoChange` (from git temporal), `SemanticallyRelated` (S4).

### ResolvedCall

Produced by Hybrid resolver; stored on `IndexedFile` or side table.

| Field | Type |
|-------|------|
| `caller_span` | `ByteRange` |
| `callee_symbol_id` | `Option<SymbolId>` |
| `callee_qname` | `String` |
| `strategy` | `ResolverStrategy` |
| `confidence` | `f32` |

### ResolverStrategy

`SameFile | Import | CrossFileRegistry | TraitDispatch | StdlibPrelude | Unresolved`

### ImpactResult (transient response)

| Field | Type |
|-------|------|
| `changed_files` | `Vec<ChangedFile>` |
| `changed_symbols` | `Vec<ChangedSymbol>` |
| `blast_nodes` | `Vec<BlastNode>` |
| `risk_summary` | `RiskSummary` |

### BlastNode

| Field | Type |
|-------|------|
| `symbol` | `SymbolRef` |
| `hop_distance` | `u8` |
| `risk` | `RiskTier` |

### RiskTier

`Critical | High | Medium | Low` — by hop distance + entry-point proximity (CBM-aligned).

### PaginationEnvelope

Attached to search/reference responses.

| Field | Type |
|-------|------|
| `total` | `usize` |
| `returned` | `usize` |
| `offset` | `usize` |
| `has_more` | `bool` |

### IndexArtifact (on disk)

| Field | Type |
|-------|------|
| `path` | `.symforge/index.bin.zst` |
| `tier` | `Fast | Best` |
| `content_hash` | `String` |
| `symforge_version` | `String` |
| `exported_at` | epoch secs |

### SemanticEdge (index-time, optional persist in snapshot v5+)

| Field | Type |
|-------|------|
| `a` | `SymbolId` |
| `b` | `SymbolId` |
| `score` | `f32` |
| `signals` | `SemanticSignals` |

### RouteNode

| Field | Type |
|-------|------|
| `method` | `String` |
| `path_pattern` | `String` |
| `handler_symbol_id` | `SymbolId` |
| `framework` | `Axum | Actix | Express | ...` |

### ArchitectureCluster

| Field | Type |
|-------|------|
| `id` | `u32` |
| `label` | `String` |
| `member_count` | `usize` |
| `cohesion` | `f32` |
| `top_symbols` | `Vec<SymbolRef>` |
| `packages` | `Vec<String>` |

### AdrDocument (persistent, not index authority)

| Field | Type |
|-------|------|
| `content` | `String` |
| `content_hash` | `String` |
| `updated_at` | epoch |
| `sections` | `Vec<String>` |

Stored at `.symforge/adr.json`.

## Relationships

```text
LiveIndex
  ├── IndexedFile
  │     ├── SymbolRecord[]
  │     ├── ReferenceRecord[]
  │     └── ResolvedCall[] (S3+)
  ├── GraphProjection (derived)
  ├── SemanticEdge[] (derived, S4+)
  └── RouteNode[] (derived, S5+)

IndexArtifact ──bootstrap──► LiveIndex (one-time import)
AdrDocument ──orthogonal──► LiveIndex (never parsed for symbols)
```

## Validation rules

- GraphProjection MUST rebuild to identical adjacency given same LiveIndex bytes.
- ResolvedCall confidence MUST be in [0.0, 1.0].
- Artifact import MUST verify content_hash before serving queries.
- Pagination: `has_more == (total > offset + returned)`.
- ADR update MUST not trigger reindex or frecency bump.

## State transitions

```text
IndexLoad → BuildGraphProjection → Ready
FileUpdate → PatchGraphProjection → Ready
Checkpoint(export=true) → WriteArtifact
ColdStart + artifact present → ImportArtifact → IncrementalIndex → Ready
```

## Embed facade boundary

Exported to embedders: read-only graph query functions on frozen index snapshot.
Excluded from semver contract test: overlay mutation of GraphProjection internals.

---

## Appendix A — SymbolId & GraphEdgeKind (S0 planning draft)

**Status**: Draft for spike; finalize at S2 Planning Gate if shape changes.

### SymbolId

Stable graph node key derived from LiveIndex (not a new authority):

```rust
// ponytail: tuple key until S2 proves u32 intern table worth it
pub struct SymbolId {
    pub path: String,      // repo-relative, forward slashes
    pub name: String,
    pub kind: SymbolKind,  // existing domain enum
}
```

**Equality**: all three fields. **Hash**: path + name + kind discriminant.

### GraphEdgeKind (v1 spike subset)

```text
Call | Import | CoChange
```

S3+ adds resolver-weighted `Call`. S4 adds `SemanticallyRelated`. S5 adds `HttpRoute` / `HttpCall`. CBM kinds not in v1: `DataFlow`, `SimilarTo`, `Implements`, `Inherits` (defer).

### BFS pseudocode (from CBM `cbm_store_bfs` — P-S0-002)

```text
bfs(start_id, direction, edge_kinds, max_depth, max_nodes):
  queue ← [(start_id, 0)]
  visited ← {start_id}
  results ← []
  while queue not empty and |results| < max_nodes:
    (node, depth) ← pop queue
    if depth > 0: append node to results
    if depth >= max_depth: continue
    for edge in edges(node, direction, edge_kinds):
      if edge.to not in visited:
        visited.add(edge.to)
        queue.push((edge.to, depth + 1))
  return results
```

Direction: `inbound` uses `in_edges`; `outbound` uses `out_edges`. SP-0A uses inbound Call edges only.
