# Quickstart: CBM Capability Ports

**Feature**: 015 · Full gate after each sprint.

## Prerequisites

- Rust toolchain matching `rust-toolchain.toml`
- `CARGO_TARGET_DIR=C:/symforge-target` (Windows) or default target
- CBM reference clone optional: `E:/project/codebase-memory-mcp`
- SymForge repo indexed at `E:/project/symforge`

## Verification gate (every sprint)

```bash
cd E:/project/symforge
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed --lib
cd npm && npm test
```

---

## Sprint 0 — Spike validation

```bash
cargo test cbm_spike -- --test-threads=1 --ignored
```

**S0.1 Graph BFS**: `tests/cbm_spike_graph_bfs.rs` — depth-5 inbound on
`execute_tool_call` in symforge repo; assert p95 <50ms (local) / document in CI.

**S0.2 Artifact**: export → delete local snapshot → import → incremental; time ratio.

**S0.3 Resolver**: `tests/cbm_spike_rust_resolver.rs` — benchmark set ≥80%.

**Go/no-go**: Record in `research.md` spike section; do not start S1 until S0.1+S0.2 pass.

---

## Sprint 1 — Quick wins

### S1.1 detect_impact

```bash
# Full surface required
$env:SYMFORGE_SURFACE='full'
# Via MCP or integration test:
cargo test detect_impact -- --test-threads=1
```

**Expected**: Changed file + symbol list + blast radius + risk labels; untracked file
included when present in fixture.

### S1.2 Team artifact

```bash
cargo test team_artifact -- --test-threads=1
```

**Expected**: `.symforge/index.bin.zst` round-trip; hash verified; `.gitattributes` line.

### S1.3 Search rank + pagination

```bash
cargo test graph_augmented_search -- --test-threads=1
cargo test pagination_envelope -- --test-threads=1
```

### S1.4 Hook augment

Manual: trigger Claude hook with Grep matching indexed symbol; additionalContext
contains symbol path. Automated: `cargo test hook_augment -- --test-threads=1`.

---

## Sprint 2 — Graph layer

```bash
cargo test graph_projection -- --test-threads=1
cargo test trace_path -- --test-threads=1
cargo test query_graph -- --test-threads=1
```

**trace_path**: inbound depth=3 on known symbol matches golden path file.

**query_graph**: dead-code Cypher returns fixture zero-caller functions only.

**Resource**: fetch `symforge://repo/graph-schema` — labels and edge counts present.

---

## Sprint 3 — Hybrid resolver

```bash
cargo test rust_resolver -- --test-threads=1
cargo test typescript_resolver -- --test-threads=1
cargo test -- cbm_spike_rust_resolver --ignored --test-threads=1
```

**Expected**: SC-004 ≥80% on symforge `src/` benchmark manifest.

---

## Sprint 4 — Semantic

```bash
cargo test semantic_edges -- --test-threads=1
cargo test stel_find_semantic -- --test-threads=1
```

**Fixture**: `tests/fixtures/semantic_vocabulary/` — publish/send pair linked.

---

## Sprint 5 — Cross-service & architecture

```bash
cargo test route_extraction -- --test-threads=1
cargo test architecture_clusters -- --test-threads=1
```

**Axum fixture**: route node + handler edge queryable via `query_graph`.

---

## Sprint 6 — Operational

```bash
cargo test manage_adr -- --test-threads=1
SYMFORGE_DIAGNOSTICS=1 cargo test diagnostics_ndjson -- --test-threads=1
symforge cli trace_path '{"name":"find_references","direction":"inbound","depth":2}'
```

---

## MCP dogfood checklist

1. `status` — index ready, project_root visible
2. `symforge` intent=impact — blast radius on uncommitted edit
3. `symforge` intent=trace — multi-hop chain
4. Switch project via 012 retarget; repeat trace on second repo

---

## Regression guards

- `tests/frecency_ranking.rs` — new discovery paths do not bump
- `tests/surface_honesty.rs` — compact-3 schema budget unchanged
- `tests/embed_contract.rs` — embed build green
