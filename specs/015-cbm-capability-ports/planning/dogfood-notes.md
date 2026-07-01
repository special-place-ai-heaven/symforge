# Dogfood Notes — Program 015

Append-only log for SymForge MCP + **CBM MCP** + CBM clone reads during `[P]` phase.
Structured evidence lives in [code-evidence.md](./code-evidence.md).

## §0 — CBM vs SymForge claims (P-PROG-003)

| CBM claim (README/paper) | SymForge today | EV row |
|--------------------------|----------------|--------|
| detect_changes (git + blast + risk) | STEL impact → find_dependents only | EV-S1-001 |
| search_graph + BFS | find_references single-hop | EV-S2-001 |
| zstd team artifact | postcard v4 only | EV-S1-003 |
| Hybrid LSP resolver | tree-sitter xref only | EV-S3-001 |
| hook symbol inject | text suggestions only | EV-S1-004 |

## Session log

| Date | Actor | Notes |
|------|-------|-------|
| 2026-06-29 | agent | Bootstrap code-evidence.md; SymForge status 574 files v8.9.7 |
| 2026-06-29 | speckit | clarify: D-015-009/011/012; analyze.md 0 critical; PROG+S0 [P] gate |
| 2026-06-29 | cbm-mcp | Enabled in Cursor (`E-project-symforge` indexed). SymForge = primary for this repo; CBM = graph/architecture/cross-repo reference when useful. |
| 2026-06-29 | speckit | S1a [P] wave 1: contracts candidate-freeze, merge_git API, STEL before/after; benchmark-intake.md for operator terminal |
| 2026-06-29 | obsidian | Refreshed `Projects/SymForge/015 CBM Capability Ports Program.md` (P-POL-002) |
