# Agent Workload Balance — Program 015

**Goal**: Each agent campaign completes one **wave** without context overload. Sprints differ in *difficulty*, not in *task avalanche*.

## Targets (per wave)

| Metric | Target | Hard cap |
|--------|--------|----------|
| `[C]` tasks in one agent session | 4–6 | **6** |
| `[P]` tasks before mini-gate | 6–8 | **10** |
| `[V]` tests filled in one session | 2–3 | **4** |
| New modules touched | ≤3 | **4** |
| Estimated agent hours (one wave) | 2–6 h | **8 h** |

**Rule**: Finish wave → run scoped check (`cargo test <module>`, quickstart section) → **stop**. Next agent/wave continues.

## Difficulty tiers ([C] only)

| Tier | Meaning | Max per wave |
|------|---------|--------------|
| **S** | Single file, <100 LOC, tests colocated | 6 |
| **M** | 2–3 files, wiring + format | 4 |
| **L** | New submodule, algorithm, migration | **2** |

Never stack two **L** tasks in one wave.

## Sprint balance (after rebalance 2026-06-29)

| Sprint | Waves | [P] | [C] | [V] | Total | Notes |
|--------|-------|-----|-----|-----|-------|-------|
| S0 | 2 | 10 | 5 | 3 | 18 | Spike; [P] done |
| **S1a** | 4 | 15 | 7 | 3 | 25 | US1–US2; max 4 [C]/wave |
| **S1b** | 2 | 7 | 4 | 3 | 14 | US3–US4 |
| S2 | 3 | 10 | 7 | 4 | 21 | Cypher split 005a/b |
| S3 | 4 | 11 | 6 | 4 | 21 | Rust wave then TS wave |
| S4 | 2 | 8 | 3 | 2 | 13 | Semantic + IndexMode |
| S5 | 3 | 9 | 3 | 2 | 14 | Routes then clusters |
| S6 | 4 | 9 | 4 | 3 | 16 | ADR/diag then CLI/traces |
| POLISH | 1 | 3 | 2 | 3 | 8 | Program close |

**S1 split** fixes the original imbalance (37 tasks / 4 US in one bucket).

## Wave map (agent entry points)

See [tasks.md](../tasks.md) — each sprint has `### Wave N` headers with explicit stop gates.

## When a sprint still feels heavy

1. Split the **L** `[C]` task (e.g. `C-S2-005` cypher → lexer + executor).
2. Move `[P]` fixture design earlier; never combine fixture + impl + full `[V]` in one wave.
3. Record overflow in decision-log; do not add silent scope in the same wave.

## Parity backlog (S7+)

Same caps apply — one PB item per wave default.
