# Task Index — Live Execution Board (Program 015)

**The single live status board.** One row per task: current status + what must finish before it starts + what it can run alongside. Seeded 2026-06-30 from [tasks.md](../tasks.md) checkboxes.

**Authority split (no drift):**
- [tasks.md](../tasks.md) = task *definitions* (the spec).
- [parallelism.md](./parallelism.md) = the *dependency/parallel rules* (gate graph, never-parallel list). This board encodes them per-task; that file is the tiebreaker.
- **This file = live status.** A coding agent updates the Status cell here as it works; sync `tasks.md` checkboxes at sprint/gate boundaries.

## Legends

**Status:** ○ not started · ◐ in progress · ● done
**Dependency (from parallelism.md):** `→ X` starts only after X · `∥ X` safe-parallel with X (different files) · `⇢ X` may overlap X, coordinate at STOP · `gate` = sprint Planning Gate sign-off

## Status rollup

| Phase | Done | Total | Sprint-[C] unblocked when… |
|-------|-----:|------:|----------------------------|
| PROG | 9 | 9 | — (done) |
| S0 | 18 | 18 | **GO 2026-06-30** (research.md § Spike Results) — done |
| S1a | 25 | 25 | **● shipped 2026-07-01** — detect_impact + team artifact coded, adversarially verified, gate-checked, merged (PR #395), released as **v8.10.0** (tag + 4-platform binaries + npm + crates.io, all green) |
| S1b | 0 | 14 | S1a **ship** — **satisfied 2026-07-01** (8.10.0 released) — `P-S1B-007` gate still required before `[C]` |
| S2 | 0 | 21 | S1a graph shipped + `P-S2-010` gate |
| S3 | 0 | 21 | S2 graph stable + `P-S3-011` gate |
| S4 | 0 | 13 | S2 (+S3 rec) + `P-S4-008` gate |
| S5 | 0 | 14 | S2 + S3 rec + `P-S5-009` gate |
| S6 | 0 | 16 | S1a tools registered + `P-S6-007a/b` gates |
| POLISH | 1 | 8 | all sprints `[V]` |
| **Total** | **53** | **159** | |

## ▶ Executable now (the frontier)

1. ~~**S0 `[C]` spike**~~ — **GO 2026-06-30** (adversarially verified by 3 agents): SP-0A p95≈46–48ms, SP-0B 607/607, SP-0C 73% strict. research.md § Spike Results.
2. ~~**Finish S1a `[P]`**~~ — **DONE 2026-06-30**: contracts frozen, risk review recorded, **S1a Planning Gate signed**.
3. ~~**S1a `[C]`+`[V]`**~~ — **DONE 2026-06-30**: detect_impact + team artifact implemented (2 sequential agents) + adversarially verified (3 parallel reviewers found and got 3 real defects fixed: base_branch default, daemon-bootstrap artifact consumption, missing-sidecar integrity bypass). Gate: fmt/check/clippy/embed green; extensive test coverage green (full lib suite + dozens of integration binaries + every changed test file, zero failures); `cargo build --release` could not complete in this sandbox (wall-time ceiling, not a code failure) — CI is authoritative. Full writeup: research.md § S1a Implementation Results.

4. ~~**8.10.0 release**~~ — **SHIPPED 2026-07-01**: PR #395 merged (branch was 83 commits behind `main`, including the rmcp 1.7→2.0 migration — resolved via a real merge commit, one textual conflict, full rmcp-2.0 API-shape audit clean, full `--all-targets` suite green 3267/0/7-ign); release-please cut `v8.10.0`; all 4 platform binaries built, GitHub release published, npm + crates.io published. Verified live via `gh` (tag, release assets, job-by-job green), not just workflow "success".

**▶ New frontier**: S1a has shipped — **S1b `[C]` is unblocked**. Still need `P-S1B-007` (S1b Planning Gate sign-off) before starting `[C]`; S1b `[P]` tasks (P-S1B-001..006) can be completed now if not already done.

> Discipline (parallelism.md "Never parallel"): no `[C]` before its sprint gate; don't start S2 `[C]` until S1a graph ships (now true — S2 gate `P-S2-010` is the remaining blocker for S2); one agent per `protocol/tools.rs` per wave.

---

## Board

### PROG — program planning ● complete (9/9)
`P-PROG-001 … P-PROG-009` — all ●. No blockers; foundation for S0.

### S0 — Spike (18/18) ● GO 2026-06-30
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S0-001..010 | P | ● | PROG | (in sprint-0 spec) |
| C-S0-001 | S | ● | done | graph.rs scaffold |
| C-S0-002 | M | ● | done | BFS p95≈46–48ms |
| C-S0-003 | M | ● | done | persist.rs zstd round-trip |
| C-S0-004 | M | ● | done | resolver same-file |
| C-S0-005 | S | ● | done | cbm_resolver_rust fixture |
| V-S0-001 | V | ● | done | SP-0A GO |
| V-S0-002 | V | ● | done | SP-0B GO 607/607 |
| V-S0-003 | V | ● | done | **GO/NO-GO written** (research.md) |

### S1a — Impact + artifact → 8.10.0 (25/25) ● complete 2026-06-30
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S1A-001,002,004,006,007,008 | P | ● | PROG | done |
| P-S1A-009,010,011,012,014 | P | ● | — | done |
| P-S1A-003 | P | ● | — | frozen 2026-06-30 |
| P-S1A-005 | P | ● | — | frozen 2026-06-30 |
| P-S1A-013 | P | ● | — | risk review + S1a touch-set done |
| P-S1A-015 | P | ● | → 003,005,013 | **S1a gate signed** 2026-06-30 |
| C-S1A-001 | M | ● | done | git.rs merge helper |
| C-S1A-002 | L | ● | done | graph.rs compute_impact |
| C-S1A-003 | L | ● | done | tools.rs/format.rs handler |
| C-S1A-004 | M | ● | done | STEL impact routing |
| C-S1A-005 | L | ● | done | persist.rs artifact (promoted from spike) |
| C-S1A-006 | M | ● | done | checkpoint_now(export_artifact) |
| C-S1A-007 | S | ● | done | init.rs + daemon alias |
| V-S1A-001..003 | V | ● | done | adversarially verified; 3 real defects found+fixed (research.md § S1a Implementation Results) |

### S1b — Search rank + hooks → 8.10.1 (0/14)
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S1B-001..006 | P | ○ | (plan ∥ S1a) | read-only, can start early |
| P-S1B-007 | P | ○ | → P-S1B-001..006 | **S1b gate** |
| C-S1B-001 | M | ○ | gate + **S1a ship** | rank chain |
| C-S1B-002 | S | ○ | → C-S1B-001 | |
| C-S1B-003 | M | ○ | gate + S1a ship | ∥ rank (format.rs) |
| C-S1B-004 | M | ○ | gate + S1a ship | ∥ pagination (isolate — sidecar) |
| V-S1B-001..003 | V | ○ | → all S1b [C] | V-S1B-002 absorbs old V-S1-004 |

### S2 — Graph (0/21)
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S2-001..008 | P | ○ | plan ∥ S1b | read-only |
| P-S2-009 | P | ○ | → P-S2-001..008 | eager-vs-lazy decision |
| P-S2-010 | P | ○ | → P-S2-009 | **S2 gate** |
| C-S2-001 | L | ○ | gate + **S1a graph shipped** | graph engine |
| C-S2-002 | M | ○ | → C-S2-001 | |
| C-S2-003 | L | ○ | → C-S2-001 | trace (Agent 1: 002→003→004) |
| C-S2-004 | M | ○ | → C-S2-003 | |
| C-S2-005a | L | ○ | → C-S2-001 | cypher (Agent 2, ∥ trace) |
| C-S2-005b | L | ○ | → C-S2-005a | |
| C-S2-006 | M | ○ | → C-S2-005b | |
| V-S2-001..004 | V | ○ | → all S2 [C] | |

### S3 — Resolver (0/21)
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S3-001,002,005..009 | P | ○ | plan ∥ | Rust focus |
| P-S3-003,004,010 | P | ○ | plan ∥ | TS reads |
| P-S3-011 | P | ○ | → all S3 [P] | **S3 gate** |
| C-S3-001 | L | ○ | gate + **S2 graph stable** | Rust (no ∥ TS — shared dir) |
| C-S3-002 | L | ○ | → C-S3-001 | |
| C-S3-003 | L | ○ | → C-S3-001 + **Rust ≥60%** | TS (separate wave) |
| C-S3-004 | M | ○ | → C-S3-003 | wire |
| C-S3-005 | M | ○ | → C-S3-004 + D-015-008 yes | snapshot v5 (optional) |
| C-S3-006 | M | ○ | → C-S3-004 | feed graph |
| V-S3-001..004 | V | ○ | → all S3 [C] | |

### S4 — Semantic + IndexMode (0/13)
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S4-001..006 | P | ○ | plan ∥ | semantic design |
| P-S4-007 | P | ○ | plan ∥ | IndexMode (moved from S1) |
| P-S4-008 | P | ○ | → all S4 [P] | **S4 gate** |
| C-S4-001 | L | ○ | gate + **S2** (+S3 rec) | semantic ∥ P-S4-007 design |
| C-S4-002 | M | ○ | → C-S4-001 | Deep/IndexMode |
| C-S4-003 | M | ○ | → C-S4-001 | edges + STEL find |
| V-S4-001,002 | V | ○ | → all S4 [C] | |

### S5 — Cross-service (0/14)
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S5-001,003,004,007 | P | ○ | plan ∥ | routes |
| P-S5-002,005,006,008 | P | ○ | plan ∥ | clusters (PD-02) |
| P-S5-009 | P | ○ | → all S5 [P] | **S5 gate** |
| C-S5-001 | L | ○ | gate + **S2 + S3 rec** | routes |
| C-S5-002 | L | ○ | → C-S5-001 (needs graph) | clusters ⇢ routes |
| C-S5-003 | S | ○ | → C-S5-002 | orient intent |
| V-S5-001,002 | V | ○ | → all S5 [C] | |

### S6 — Ops (0/16)
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-S6-001,002,006 | P | ○ | plan ∥ | ADR + diagnostics |
| P-S6-007a | P | ○ | → P-S6-001,002,006 | **S6a gate** |
| P-S6-003,004,005,006b | P | ○ | plan ∥ | CLI + traces |
| P-S6-007b | P | ○ | → P-S6-003..006b | **S6b gate** |
| C-S6-001 | M | ○ | S6a gate + **S1a tools registered** | ADR (∥ CLI wave) |
| C-S6-002 | M | ○ | S6a gate | diagnostics ∥ ADR |
| C-S6-003 | M | ○ | S6b gate | CLI (separate agent) |
| C-S6-004 | M | ○ | S6b gate | traces ∥ CLI |
| V-S6-001..003 | V | ○ | → all S6 [C] | |

### POLISH (1/8)
| Task | T | Status | After | ∥ |
|------|---|--------|-------|---|
| P-POL-001 | P | ○ | → all PD-* closed | |
| P-POL-002 | P | ● | — | done |
| P-POL-003 | P | ○ | — | AGENTS.md delta draft |
| C-POL-001 | C | ○ | → all sprints [C] | init.rs sync |
| C-POL-002 | C | ○ | → all sprints [C] | frecency/honesty gaps |
| V-POL-001..003 | V | ○ | → C-POL-* | program close |

---

## Maintenance

- Agent picks a task whose **After** column is satisfied, sets it `◐`, completes the wave (≤6 `[C]` — [agent-workload.md](./agent-workload.md)), sets `●`, STOPs.
- Update the **Status rollup** Done counts when a phase advances.
- Dependency questions → [parallelism.md](./parallelism.md) is authoritative; if a new `[C]` task changes file-touch overlap, update both.
