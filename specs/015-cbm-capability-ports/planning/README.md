# Planning Artifacts Index — Program 015

**Read first**: [execution-model.md](../execution-model.md) (60/30/10) · [program-planning-gate.md](./program-planning-gate.md)

## Speckit artifacts

| Step | File |
|------|------|
| analyze | [../analyze.md](../analyze.md) |
| clarify | [../spec.md](../spec.md) § Clarifications |
| gate | [program-planning-gate.md](./program-planning-gate.md) |

## Sprint specs (freeze at Planning Gate)

| Sprint | Document | Planning Gate owner |
|--------|----------|---------------------|
| S0 Spike | [sprint-0-spike-spec.md](./sprint-0-spike-spec.md) | Engine lead |
| S1a Quick wins A | [sprint-1-quick-wins-spec.md](./sprint-1-quick-wins-spec.md) | Protocol lead |
| S1b Search+hooks | [sprint-1b-search-hooks-spec.md](./sprint-1b-search-hooks-spec.md) | Protocol lead |
| S2 Graph | [sprint-2-graph-spec.md](./sprint-2-graph-spec.md) | Engine lead |
| S3 Resolver | [sprint-3-resolver-spec.md](./sprint-3-resolver-spec.md) | Parsing lead |
| S4 Semantic | [sprint-4-semantic-spec.md](./sprint-4-semantic-spec.md) | Engine lead |
| S5 Cross-svc | [sprint-5-cross-service-spec.md](./sprint-5-cross-service-spec.md) | Parsing lead |
| S6 Ops | [sprint-6-ops-spec.md](./sprint-6-ops-spec.md) | CLI/ops lead |

## Cross-cutting references

| Document | Purpose |
|----------|---------|
| **[benchmark-intake.md](./benchmark-intake.md)** | **Operator CBM vs SymForge runs** (parallel to Speckit) |
| **[WORKTREE.md](./WORKTREE.md)** | **Isolated branch path `E:/project/symforge-015`** |
| **[parallelism.md](./parallelism.md)** | **What can run ∥ vs must wait** (multi-agent dispatch) |
| **[agent-workload.md](./agent-workload.md)** | Sprint balance + ≤6 [C] per agent wave |
| **[code-evidence.md](./code-evidence.md)** | **SymForge-verified anchors** — every US/sprint claim links here |
| [cbm-source-map.md](./cbm-source-map.md) | CBM file → SymForge port → read-before-code |
| [acceptance-matrix.md](./acceptance-matrix.md) | Given/When/Then + fixtures + metrics per US |
| [file-touch-matrix.md](./file-touch-matrix.md) | Every Rust path touched per sprint |
| [risk-register.md](./risk-register.md) | Risks, triggers, mitigations, owners |
| [decision-log.md](./decision-log.md) | D-015-NNN architectural decisions |
| [test-strategy.md](./test-strategy.md) | 10% validation layer — what proves what |

## Contracts (../contracts/)

Frozen alongside sprint Planning Gate. Amend only via decision-log.

## Task list

[tasks.md](../tasks.md) — all work as `[P]` → `[C]` → `[V]` per sprint.
