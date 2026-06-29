# Benchmark Intake — Operator Terminal (CBM vs SymForge)

**Purpose**: Capture comparison/benchmark runs performed **outside** this agent session.
Feeds planning only — does **not** block Speckit gates unless a result falsifies a spike hypothesis.

**Consumers**: [research.md](../research.md) § Benchmark Results · [sprint-0-spike-spec.md](./sprint-0-spike-spec.md) baseline table · [decision-log.md](./decision-log.md) · [dogfood-notes.md](./dogfood-notes.md)

---

## How to record a run

1. Copy a row into **Session log** below.
2. If the run affects a spike hypothesis (S0 SP-0A/B/C), append to **research.md** § Spike Results.
3. If it changes architecture (e.g. CBM faster at depth-5 BFS), open a **decision-log** row — do not edit contracts without D-015-NNN.
4. SymForge rows: note `symforge_version` from MCP `status`.
5. CBM rows: note project alias + whether index was refreshed (`index_repository`).

---

## Comparison matrix (fill as runs complete)

| ID | Scenario | CBM tool / args | SymForge tool / args | Metric | CBM | SymForge | Winner | Notes |
|----|----------|-----------------|----------------------|--------|-----|----------|--------|-------|
| B-001 | Cold index ready | — | `status` | files indexed | | | | |
| B-002 | Symbol search | `search_graph` / search | `symforge` find | latency p50/p95 | | | | |
| B-003 | Change impact | `detect_changes` | `what_changed` + STEL impact | impacted_symbols count | | | | CBM may return empty if stale |
| B-004 | Multi-hop trace | `trace_path` | STEL trace → find_references | hops / tokens | | | | |
| B-005 | Architecture map | `get_architecture` | `get_repo_map` | payload tokens | | | | |
| B-006 | Repo-wide read | — | `symforge` read | tokens served | | | | |

---

## S0 spike crosswalk

| Spike | Benchmark IDs that inform GO/NO-GO | Threshold (sprint-0 spec) |
|-------|-----------------------------------|-----------------------------|
| SP-0A BFS | B-004 (SymForge after C-S0-002) | p95 <200ms local |
| SP-0B artifact | — (unit test V-S0-002) | hash match |
| SP-0C resolver | — (fixture V-S0-003) | ≥60% S0 minimum |

Operator CBM benchmarks **do not substitute** SP-0A/B/C unit spikes; they inform superiority doctrine and parity-backlog priority.

---

## Session log (append-only)

| Date | Operator | Run summary | Artifacts updated |
|------|----------|-------------|-------------------|
| | | | |

---

## Known asymmetries (planning assumptions)

| Topic | CBM | SymForge | Planning implication |
|-------|-----|----------|----------------------|
| Index store | SQLite Soul Map | LiveIndex in-process | Constitution I — no Soul Map port |
| Default MCP surface | many tools | compact-3 STEL | Compare via full surface or STEL intents |
| symforge in CBM | `E-project-symforge` may be stale | LiveIndex authoritative for 015 | Re-index CBM before fair B-002/B-003 |
| Structural edits | CBM read-only | symforge_edit | Out of benchmark scope |

---

## Speckit placement

| Phase | This doc |
|-------|----------|
| PROG / S0 [P] | Optional baseline rows |
| S0 [V] | Spike numbers only from `tests/cbm_spike_*` |
| S1a+ [P] | B-003 informs detect_impact contract; log in dogfood-notes |
| Parity backlog | B-002/B-005 may elevate PB-01/PB-03 priority |

**This agent session**: planning via Speckit `[P]` tasks — **not** running benchmarks.
