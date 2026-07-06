# Planning README — 016 Perl Parser Hardening

## Artifact index

| File | Purpose |
|------|---------|
| [program-planning-gate.md](./program-planning-gate.md) | PROG sign-off |
| [code-evidence.md](./code-evidence.md) | SymForge-backed EV-* rows |
| [acceptance-matrix.md](./acceptance-matrix.md) | US × SC × gate mapping |
| [risk-register.md](./risk-register.md) | Program risks |
| [file-touch-matrix.md](./file-touch-matrix.md) | Paths by sprint |
| [decision-log.md](./decision-log.md) | D-016-* decisions |
| [BRANCH.md](./BRANCH.md) | Branch + worktree |
| [sprint-0-retrofit-audit-spec.md](./sprint-0-retrofit-audit-spec.md) | S0 |
| [sprint-1-evidence-corpus-spec.md](./sprint-1-evidence-corpus-spec.md) | S1 |
| [sprint-2-coverage-expansion-spec.md](./sprint-2-coverage-expansion-spec.md) | S2 |
| [sprint-3-operational-spec.md](./sprint-3-operational-spec.md) | S3 |

## Workflow

1. Complete `[P]` tasks → update code-evidence.md
2. Sprint Planning Gate checklist in sprint spec
3. Then `[C]` / `[V]`
4. Release Gate in [quickstart.md](../quickstart.md)
5. `/speckit-converge` if needed

## SymForge MCP (mandatory for [P])

Repo: `E:/project/symforge`  
Preferred tools: `explore`, `diff_symbols`, `get_file_context`, `get_symbol`, `search_text`, `analyze_file_impact`

Stamp rows with symforge version from MCP `status` when available.
