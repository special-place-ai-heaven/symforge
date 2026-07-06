# Execution Model: 55% Planning · 35% Coding · 10% Validation

**Program**: 016 Perl Parser Hardening  
**Principle**: Measure before expanding; corpus drives xref edits, not issue comments.  
**North Star**: [spec.md § Program goal](../spec.md#program-goal)

## Why 55% planning?

The grammar swap landed (`9572b31`) but **trust requires evidence**. Blind xref expansion
risks:

- Regressing C++ `@ref.qualified_call` (D13 neighbor in same file)
- Adding query captures for constructs that don't exist in ts-parser-perl 1.1.3
- Chasing #341 marketing numbers without reproducible fixtures
- Breaking `compile_xref_query` invariants across 21 languages

**Planning deliverables ARE the product** until a sprint's **Planning Gate** clears.

## Effort budget (program-level)

| Phase | Share | Includes | Excludes |
|-------|-------|----------|----------|
| **Planning [P]** | **55%** | Taxonomy, contracts, fixture design, acceptance matrix, code-evidence, sprint specs | Production xref edits |
| **Coding [C]** | **35%** | Fixtures, tests, query/extractor edits, investigation doc body | Scope not in taxonomy |
| **Validation [V]** | **10%** | Full gate, probe runs, metrics capture | Feature expansion |

Estimated task counts ([tasks.md](../tasks.md)):

| Type | Tasks | Ratio |
|------|-------|-------|
| `[P]` | ~42 | 55% |
| `[C]` | ~27 | 35% |
| `[V]` | ~8 | 10% |

## Per-sprint workflow

```text
┌─────────────────────────────────────────────────────────────┐
│ 1. READ sprint spec (planning/sprint-N-*.md)                  │
│ 2. SymForge MCP evidence pass → code-evidence.md            │
│ 3. Complete all [P] tasks for sprint                        │
│ 4. PLANNING GATE review (checklist in sprint spec)          │
├─────────────────────────────────────────────────────────────┤
│ 5. Complete [C] tasks (construct class at a time)           │
│ 6. Complete [V] tasks                                       │
│ 7. RELEASE GATE (quickstart.md)                             │
│ 8. /speckit-converge → append gap tasks if needed           │
└─────────────────────────────────────────────────────────────┘
```

**Hard rule**: No `[C]` until linked `[P]` + Planning Gate done.

**Agent cap**: ≤6 `[C]` tasks per session; **STOP** between waves.

## Code-backed planning (mandatory)

| Source | When | Record in |
|--------|------|-----------|
| SymForge MCP on symforge | Touch points, dispatch map, symbol diff | [code-evidence.md](./code-evidence.md) |
| `probe_perl_grammar_sexp --ignored` | Node shape contract | [contracts/perl-node-shapes.md](../contracts/perl-node-shapes.md) |
| Git `diff_symbols` | Retrofit scope | EV-S0 rows |

**Workflow for each `[P]` task**:

1. SymForge (`explore`, `get_file_context`, `diff_symbols`, `search_text`) before writing.
2. Add/update `EV-Sn-###` row — symbol + file first.
3. Link row from sprint spec / contract / task.

**Workflow for each `[C]` task**:

1. Confirm taxonomy row exists for construct class.
2. Confirm ≥1 fixture before query edit.
3. Post-edit: `analyze_file_impact` on touched `.rs` files (MCP or CLI).

## Loops

### Inner loop (sprint)

`[P]* → Planning Gate → [C]* → [V]* → Release Gate`

### Convergence loop (program)

`/speckit-implement` → `/speckit-converge` → re-implement until spec SC-* met

### S2 micro-loop (per construct class)

```text
taxonomy miss → fixture → unit test (fail) → query/extractor edit → test pass → EV refresh
```

## S0 special case

Phase 0 code **already merged**. S0 is **audit-only**: if `[V]` fails, `[C]` fixes are
hotfix scope on this branch before S1 — not a re-merge of #341.
