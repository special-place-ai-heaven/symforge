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
| B-001 | Cold index ready | — | `status` | startup to ready | **0.0s** (persistent SQLite; 4.7k–280k nodes) | 1.3s fmt · 1.3s tokio · 7.3s django · **58–63s typescript** (in-process) | **CBM** | The gap US2/SC-002 targets. SF in-proc index reaches `index_ready:true` then serves |
| B-002 | Symbol search (T4) | `search_graph` query | `symforge` find / `search_symbols` | tokens · latency | 74–1,898 tok · 2–370ms | 290–1,166 (compact) / 437–948 (full) · 1–16ms | **mixed** — CBM lower tok on TS, SF lower latency at scale; SF more consistent | CBM BM25 has recall gaps ('validation'→0) |
| B-003 | Change impact | `detect_changes` | `what_changed` + STEL impact | impacted_symbols | _not run_ | _not run_ | — | **TODO**: add a T7 impact task to the sf-bench battery (informs US1/detect_impact) |
| B-004 | Multi-hop trace (T2) | `trace_path` inbound | STEL trace → find_references | tokens · latency | **72–104 tok** · 0–3ms | 230–2,869 (compact) / 477–3,237 (full) · 2–19ms | **CBM (tokens)** | CBM cheapest on all 4 repos but scored *under-served* vs manual — too terse; port the win + keep file:line context |
| B-005 | Architecture map (T6) | `get_architecture` | `get_repo_map` / orient | payload tokens | 3,854 fmt · 12,906 tokio · 14,415 django · **689,943 typescript** | 630–1,455 (all repos) | **SymForge** (200–800×) | CBM dumps the whole graph; the falsifier for any "dump" design (PD-02) |
| B-006 | Repo-wide read (T_FULL) | — (no file-read tool) | `symforge` read | tokens served | unsupported → falls back to N (25k–788k) | 148–1,377 (partial, judged LESS) | **SymForge** | Neither fully serves; SF stays compact, CBM can't serve at all |

**Setup tax (per-session, B-001 adjacent):** schema tokens — CBM **2,897** (14 tools) · SF-compact **1,145** (3 tools) · SF-full **17,641** (35 tools). Index footprint — CBM 826 MB SQLite (sum of 4 repos) vs SF <2 MB `.symforge`.

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
| 2026-06-30 | rakovnik | Full 3-engine × 4-repo battery via `sf-bench`: CBM (0.10.0) vs SF-compact + SF-full (8.9.7), 9 tasks/repo (tokio/django/typescript/fmt), measuring tokens + warm latency + mcp-calls + index cost, judged vs shared manual/naive baselines. Headline: SF-compact Σ21.6k tok vs CBM Σ1.73M (skewed by TS get_architecture 690k + no file-read); CBM wins T2 trace tokens (72–104), CBM 0s cold-start, SF wins repo-map/payload + large-repo search latency. | B-001/002/004/005/006 above; `E:/project/sf-bench/out/COMPARISON.md` §5 maps findings→015 sprints; `results-{cbm,sf-compact,sf-full}.json` + `cbm-index.json` |

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
