# Sprint 0 Planning Spec — Spike & Falsifiers

**Status**: **frozen** at Planning Gate 2026-06-29  
**Release**: none (gate only)  
**User stories**: — (enables US5, US8, US2)

## Objective

Falsify or confirm three load-bearing assumptions **before** S1 coding spend:

1. LiveIndex can support BFS at required latency
2. Postcard snapshot can round-trip via compression without byte corruption
3. Rust resolver can hit ≥80% on benchmark fixture

## Out of scope

- Production tool surfaces
- STEL routing changes
- Snapshot v5

## CBM references read ([P] complete)

- [x] `src/store/store.c` — `cbm_store_bfs` → pseudocode in [data-model.md](../data-model.md) Appendix A
- [x] `pipeline/artifact.c` — integrity pattern → EV-S1-003 / persist.rs touch points
- [x] `internal/cbm/lsp/rust_lsp.c` — skim → [resolver-port-notes.md](./resolver-port-notes.md)

## SymForge baseline measurements

| Measurement | Command/method | Result | Date |
|-------------|----------------|--------|------|
| symforge index ready | MCP `status` | true, 574 files | 2026-06-29 |
| symforge version | MCP `status` | 8.9.7 | 2026-06-29 |
| find_references depth-1 latency | MCP / test | defer to V-S0-001 | — |
| index.bin size | local `.symforge/` | not present in dev tree | 2026-06-29 |
| Reference count | MCP trace | 13 refs `capture_find_dependents_view` | 2026-06-29 |

## Spike specifications

### SP-0A — Graph BFS

**Hypothesis**: `GraphProjection` on symforge repo answers inbound BFS depth-5 in p95 <50ms (local), <100ms (CI smoke).

**Method**:
- Minimal `graph.rs`: nodes from symbols, edges from `ReferenceRecord` Call kind
- Test: `tests/cbm_spike_graph_bfs.rs` `#[ignore]`

**Falsifier**: p95 >200ms → redesign lazy graph or cap depth to 3 for v1 (decision-log)

### SP-0B — Artifact round-trip

**Hypothesis**: zstd compress/decompress preserves exact `content_hash` per file (D-015-009).

**Method**:
- Stub export/import in `persist.rs`
- Test: `tests/cbm_spike_artifact.rs`

**Falsifier**: hash mismatch → abort zstd; investigate postcard field loss

### SP-0C — Rust resolver benchmark

**Hypothesis**: ≥80% resolution on `tests/fixtures/cbm_resolver_rust/` at S3; ≥60% S0 minimum.

**Method**:
- Manifest schema: `tests/fixtures/cbm_resolver_rust/README.md`
- Minimal same-file + use-statement resolver

**Falsifier**: <60% after 2 weeks → narrow S3 to same-file-only v1; defer cross-file to S3.1

## Error catalog (spike-only)

| Condition | Expected |
|-----------|----------|
| Empty index BFS | Empty result, not panic |
| Corrupt compressed bytes | Import error, no partial serve |

## Planning Gate checklist

- [x] cbm-source-map S0 rows read
- [x] Baseline table filled (local index.bin N/A noted)
- [x] Fixture `cbm_resolver_rust` manifest schema drafted
- [x] D-015-009 resolved (zstd); PD-01 deferred S3
- [x] risk-register R-04 reviewed

## Go/no-go criteria

| Spike | GO | NO-GO action |
|-------|-----|--------------|
| SP-0A | p95 <200ms | Reduce scope; decision-log |
| SP-0B | hash match | Fix format before S1 |
| SP-0C | ≥60% minimum | Reschedule S3; S1/S2 proceed |

**Sign-off**: Speckit planning session 2026-06-29 (S0 `[P]` complete).

**S0 `[C]`/`[V]` GO — 2026-06-30** (adversarially verified by 3 independent agents):
SP-0A p95 ≈ 46–48ms (GO, 4× margin); SP-0B 607/607 `content_hash` byte-exact, 3.61× (GO);
SP-0C 73% strict ≥ 60% (GO — S0 feasibility only, S3 80% NOT demonstrated; keystone risk =
bare-name resolution → false-positive edges). Full results + caveats + spike-code disposition:
[research.md](../research.md) § Spike Results.

## Rollback

Spike code behind `#[cfg(test)]` or ignored tests only; no MCP surface change.

## Linked tasks

[P]: P-S0-001..010 ✓ | [C]: C-S0-001..005 | [V]: V-S0-001..003
