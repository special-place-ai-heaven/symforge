# Sprint Calendar: 015 CBM Capability Ports

**Program**: [spec.md](./spec.md) · **Tasks**: [tasks.md](./tasks.md) · **Plan**: [plan.md](./plan.md) · **Model**: [execution-model.md](./execution-model.md)

## Overview

Six production sprints (+ S1 split into **S1a/S1b**) after a mandatory spike gate. **60% planning · 30% coding · 10% validation** per [execution-model.md](./execution-model.md). Each sprint: `[P]` → Planning Gate → `[C]` → `[V]` → release.

```
S0 Spike ──gate──► S1a Impact+artifact ──► S1b Search+hooks ──► S2 Graph ──► S3 Resolver ──► S4 Semantic ──► S5 Cross-svc ──► S6 Ops
```

**Agent rule**: ≤6 `[C]` tasks per session — [planning/agent-workload.md](./planning/agent-workload.md).

## Sprint 0 — Spike & Falsifiers (2 weeks)

**Goal**: Prove graph BFS, artifact round-trip, and Rust resolver feasibility before
main investment.

**Exit gate (all must pass)**:
- [ ] S0.1 BFS on LiveIndex <50ms depth-5 (symforge repo)
- [ ] S0.2 Artifact import + incremental <20% full index time
- [ ] S0.3 Rust resolver ≥80% on benchmark set OR documented fallback plan
- [ ] S0.4 Constitution check signed off in research.md

**Deliverables**: `research.md` spike section, `tests/cbm_spike_*`, go/no-go note.

---

## Sprint 1a — Impact + artifact (2 weeks) → 8.10.0

**User stories**: US1–US2

| Deliverable | Primary modules |
|-------------|-----------------|
| `detect_impact` / STEL impact | `git.rs`, `graph.rs`, `protocol/tools.rs`, `stel/planner.rs` |
| Team zstd artifact | `live_index/persist.rs`, `paths.rs` |

**Release criteria**: S1a tasks green; quickstart S1a scenarios pass.

---

## Sprint 1b — Search + hooks (2 weeks) → 8.10.1

**User stories**: US3–US4

| Deliverable | Primary modules |
|-------------|-----------------|
| Graph-augmented search rank | `live_index/search.rs` |
| Pagination struct | `protocol/format.rs` |
| Hook augment | `cli/hook.rs`, `sidecar/handlers.rs` |

**Release criteria**: S1b tasks green; full S1 quickstart pass.

---

## Sprint 1 — Quick wins (4 weeks) → 8.10.x — ARCHIVED

Split into **S1a** + **S1b** (balance pass 2026-06-29). Original combined scope:

---

## Sprint 2 — Graph Query Layer (6 weeks) → 8.11.x

**User stories**: US5–US7

| Deliverable | Primary modules |
|-------------|-----------------|
| `GraphProjection` engine | `live_index/graph.rs` (new) |
| `trace_path` tool + STEL trace | `protocol/tools.rs`, `stel/planner.rs` |
| `query_graph` subset | `live_index/cypher/` (new) |
| `symforge://repo/graph-schema` resource | `protocol/resources.rs` |

**Release criteria**: S2 tasks T056–T095 green; dead-code query fixture passes.

---

## Sprint 3 — Hybrid Resolution (8 weeks) → 8.12.x

**User stories**: US8–US9

| Deliverable | Primary modules |
|-------------|-----------------|
| Rust resolver | `parsing/resolver/rust.rs`, `parsing/resolver/mod.rs` |
| TS/JS resolver | `parsing/resolver/typescript.rs` |
| Cross-file registry merge | `parsing/resolver/registry.rs` |
| Pipeline integration | `parsing/mod.rs`, `live_index/store.rs` |

**Release criteria**: S3 tasks T096–T135 green; resolver benchmarks in CI (ignored smoke).

---

## Sprint 4 — Algorithmic Semantic (6 weeks) → 8.13.x

**User stories**: US10

| Deliverable | Primary modules |
|-------------|-----------------|
| Semantic edge pass | `live_index/semantic.rs` (new) |
| STEL find keyword bridging | `stel/planner.rs`, `protocol/tools.rs` |
| Deep index mode integration | `live_index/store.rs` |

**Release criteria**: S4 tasks T136–T165 green; vocabulary fixture set passes.

---

## Sprint 5 — Cross-Service & Architecture (6 weeks) → 8.14.x

**User stories**: US11–US12

| Deliverable | Primary modules |
|-------------|-----------------|
| HTTP route extractors | `parsing/routes/` (new) |
| Architecture clusters | `live_index/cluster.rs`, `sidecar/handlers.rs` |
| `get_architecture` or deep map mode | `protocol/tools.rs` |

**Release criteria**: S5 tasks T166–T195 green; axum fixture + cluster smoke.

---

## Sprint 6 — Operational Parity (4 weeks) → 8.15.x

**User stories**: US13–US15

| Deliverable | Primary modules |
|-------------|-----------------|
| ADR persistence | `protocol/tools.rs`, `paths.rs`, `protocol/resources.rs` |
| Diagnostics NDJSON | `live_index/diagnostics.rs`, `main.rs` |
| CLI mirror | `cli/mod.rs` (new subcommand tree) |
| Trace ingest (minimal) | `live_index/traces.rs` |

**Release criteria**: S6 tasks T196–T220 green; program quickstart full pass.

---

## Parallel workstreams

| Stream | Can run parallel to | Notes |
|--------|---------------------|-------|
| 012 multi-project | S2+ | Graph tools gain `project` param when 012 Phase 3 lands |
| 013 calibration | All | No conflict |
| npm packaging | S6 | CLI mirror docs for npm bin |

## Risk register

| Risk | Mitigation |
|------|------------|
| Resolver scope creep | Language milestones; confidence thresholds |
| Graph memory on huge repos | Lazy edge build; depth caps; CCR for bulk results |
| Cypher scope creep | Fail-closed subset; explicit unsupported errors |
| Sprint 3 slips | Ship Rust-only resolver; TS follows in 8.12.1 |
