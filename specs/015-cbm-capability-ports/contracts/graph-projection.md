# Contract: Graph Projection

**Feature**: 015 · **Sprint**: S0/S2

## Invariants

- GraphProjection is **derived** from LiveIndex; never loaded as authority without
  full snapshot verify.
- Rebuild from identical LiveIndex state MUST produce identical adjacency (deterministic
  edge ordering: sort by `(from, to, kind)`).

## API (engine-internal)

```rust
// live_index/graph.rs
pub struct GraphProjection { /* ... */ }

impl GraphProjection {
    pub fn build_from_index(index: &LiveIndex) -> Self;
    pub fn bfs(&self, start: SymbolId, config: BfsConfig) -> BfsResult;
}
```

## BfsConfig

| Field | Default | Max |
|-------|---------|-----|
| `direction` | `Both` | Inbound/Outbound/Both |
| `max_depth` | 3 | 5 |
| `edge_kinds` | `[Call]` | filter list |
| `include_tests` | false | — |

## Performance

- depth=5, symforge repo, single start node: p95 <100ms (SC-003).

## Frecency

- `bfs()` MUST NOT call `bump_frecency`.
