# Execution Model: 60% Planning · 30% Coding · 10% Validation

**Program**: 015 CBM Capability Ports  
**Principle**: Surprises are a planning failure. Code is the last mile.  
**North Star**: [spec.md § Superiority doctrine](../spec.md#superiority-doctrine) — **default adopt** CBM; skip **inferior parts only**.

## Why 60% planning?

CBM is a mature C codebase (~1,700 files, 5,600 tests, published paper). SymForge
has different architecture (LiveIndex authority, STEL, edits). Blind porting causes:

- Constitution violations (Soul Map creep)
- Duplicate tools that fight STEL
- Snapshot format breaks without migration plan
- Resolver scope explosion without language milestones
- "Works in test" that fails on Windows git porcelain

**Planning deliverables ARE the product** until a sprint's **Planning Gate** clears.
Coding tasks are blocked until their `[P]` predecessors are checked off.

## Effort budget (program-level)

| Phase | Share | What it includes | What it excludes |
|-------|-------|------------------|------------------|
| **Planning [P]** | **60%** | Specs, contracts, CBM source maps, acceptance matrices, fixture design, API sketches, error catalogs, file-touch lists, spike analysis, decision log, review sign-off | Writing production logic |
| **Coding [C]** | **30%** | Production Rust, wiring, formatters | New scope not in sprint spec |
| **Validation [V]** | **10%** | Run gates, dogfood MCP, record metrics, fix only gate failures | Feature expansion |

Estimated task counts (see [tasks.md](./tasks.md)):

| Type | Tasks | Ratio |
|------|-------|-------|
| `[P]` Planning | ~95 | 58% |
| `[C]` Coding | ~50 | 31% |
| `[V]` Validation | ~18 | 11% |

## Per-sprint workflow

```text
┌─────────────────────────────────────────────────────────────┐
│ 1. READ sprint spec (planning/sprint-N-*.md)                │
│ 2. READ CBM sources listed in cbm-source-map.md             │
│ 3. Complete all [P] tasks for sprint                        │
│ 4. PLANNING GATE review (checklist in sprint spec)            │
├─────────────────────────────────────────────────────────────┤
│ 5. Complete [C] tasks (one US at a time)                      │
│ 6. Complete [V] tasks                                       │
│ 7. RELEASE GATE (quickstart + constitution)                 │
└─────────────────────────────────────────────────────────────┘
```

**Hard rule**: No `[C]` task starts until its linked `[P]` tasks and sprint
Planning Gate are done.

**Agent cap**: ≤6 `[C]` tasks per session; **STOP** gates between waves —
[planning/agent-workload.md](./planning/agent-workload.md).  
**Parallel dispatch**: [planning/parallelism.md](./planning/parallelism.md).

## Code-backed planning (mandatory)

Every plan claim must trace to **verified code**, not memory or CBM README alone.

| Source | When to use | Record in |
|--------|-------------|-----------|
| **SymForge MCP** on `E:/project/symforge` | SymForge touch points, gaps, reuse | [planning/code-evidence.md](./planning/code-evidence.md) |
| **SymForge MCP** on CBM `src/` | CBM algorithm ports (after index) | same + `cbm-source-map.md` |
| Direct read | CBM only until indexed; line anchors required | `code-evidence.md` § CBM |

**Workflow for each `[P]` task**:

1. Run SymForge (`symforge` intent=read/trace/find, or `status`) before writing spec text.
2. Add or update an `EV-Sn-###` row in `code-evidence.md` — **symbol + file first**, lines second.
3. Link the row from sprint spec / contract / task description.
4. Stamp row with `verified` date + `symforge_version` from MCP `status`.

**Workflow for each `[C]` task**:

1. Confirm ≥1 `EV-*` row exists for every file in file-touch-matrix.
2. After merge, re-run SymForge on touched files and refresh anchors.

Planning Gate **FAIL** if any in-scope US lacks SymForge-confirmed touch points in
`code-evidence.md`.

**Drift**: line-number mismatch alone is not a gate failure. Re-run SymForge at each
Planning Gate; fail only when symbols/gaps moved without a spec update. See
[code-evidence.md § Drift policy](./planning/code-evidence.md#drift-policy).

## Planning Gate checklist (every sprint)

- [ ] Sprint spec reviewed against Constitution I–VIII
- [ ] All contracts for this sprint marked `status: frozen`
- [ ] Acceptance matrix rows for this sprint have fixture paths assigned
- [ ] File-touch matrix reviewed (no surprise modules)
- [ ] CBM source files read and mapped (cbm-source-map.md rows ticked)
- [ ] [code-evidence.md](./planning/code-evidence.md) has SymForge-verified rows for every in-scope US
- [ ] Stale EV rows refreshed (SymForge re-query at gate; lines updated, claims unchanged unless spec amended)
- [ ] Every `[C]` task in tasks.md links to ≥1 `EV-*` row (or WILL CREATE row)
- [ ] Error catalog draft complete for new tools
- [ ] Rollback plan documented (what to revert if gate fails)
- [ ] Operator sign-off line in sprint spec (name + date)

## Release Gate checklist (every sprint)

- [ ] All `[V]` tasks green
- [ ] Full backend gate (CLAUDE.md)
- [ ] quickstart.md sprint section executed and logged
- [ ] No new `[NEEDS CLARIFICATION]` in decision-log.md
- [ ] Frecency neutrality tests pass for new discovery paths
- [ ] Compact-3 schema budget test pass (`surface_list.rs`)

## Artifact hierarchy

```text
spec.md              ← program intent (stable)
execution-model.md   ← this file (stable)
planning/
  code-evidence.md   ← SymForge-verified anchors (required for Planning Gate)
  sprint-N-*.md      ← sprint truth (frozen at Planning Gate)
  acceptance-matrix.md
  cbm-source-map.md
  file-touch-matrix.md
  risk-register.md
  decision-log.md
  test-strategy.md
contracts/           ← API contracts (frozen at Planning Gate)
tasks.md             ← [P]/[C]/[V] ordered work
```

## Task ID convention

| Prefix | Meaning | Example |
|--------|---------|---------|
| `P-Sn-###` | Planning for sprint n | `P-S1A-003` |
| `C-Sn-###` | Coding for sprint n | `C-S1A-003` |
| `V-Sn-###` | Validation for sprint n | `V-S1A-001` |
| `P-PROG-###` | Program-wide planning | `P-PROG-001` |

Legacy `T###` IDs in older notes map to `[C]` tasks in tasks.md.

## When planning expands scope

If `[C]` work discovers missing requirement:

1. STOP coding on that US
2. Add decision-log entry (D-015-NNN)
3. Update sprint spec + contract
4. Add `[P]` task for spec amendment
5. Re-run Planning Gate for affected US only

No silent scope creep.

## Dogfood during planning (not validation)

During `[P]` phase, allowed:

- **Required**: SymForge MCP on symforge repo for every SymForge touch-point claim
- Read CBM clone with SymForge MCP (after index) or direct read with line anchors
- Trace SymForge call sites (`intent=trace`) for reuse vs greenfield decisions
- Measure baseline perf (record in sprint spec + `code-evidence.md` dogfood log)

Not allowed before `[C]`:

- Shipping partial tools to default MCP surface
- Snapshot format changes without migration spec

## Program timeline (indicative)

| Sprint | Planning weeks | Coding weeks | Validation days |
|--------|----------------|--------------|-----------------|
| S0 | 1.5 | 0.5 | 1 |
| S1 | 2.5 | 1.5 | 2 |
| S2 | 3 | 2 | 2 |
| S3 | 4 | 2.5 | 3 |
| S4 | 2 | 1.5 | 2 |
| S5 | 2.5 | 2 | 2 |
| S6 | 1.5 | 1 | 2 |

**Total ~28 planning-weeks equivalent** before counting parallel `[P]` work.

## Success definition for planning phase

Planning is done for a sprint when an implementer can execute `[C]` tasks **without
asking clarifying questions** — every question already answered in sprint spec +
contracts + acceptance matrix.
