# Sprint 6 Planning Spec — Operational Parity

**Status**: draft  
**Release**: 8.15.x (program complete)  
**User stories**: US13, US14, US15  
**Depends on**: S1–S5 functionally complete

## Objective

Team maturity: ADR persistence, operator diagnostics, CLI mirror, minimal trace ingest.

## US13 — ADR

- Mirror CBM `manage_adr` modes: get, update, sections
- Store `.symforge/adr.json` — NOT in index snapshot authority
- Resource `symforge://repo/adr`

## US14 — Diagnostics

- Mirror CBM `CBM_DIAGNOSTICS` → `SYMFORGE_DIAGNOSTICS`
- NDJSON: rss, committed (Windows), fd count — **no source, no queries**
- 5s interval; file retained on exit

## US15 — CLI mirror

```bash
symforge cli trace_path '{"name":"foo","direction":"inbound","depth":2}'
symforge cli detect_impact '{}'
```

**Planning [P]**: map each mirrored tool to existing handler — no duplicate logic.

## Trace ingest (minimal)

- Accept OTLP JSON array stub
- Boost `HttpCall` edge confidence when path matches — no collector

## CBM reference

- `traces/` module skim
- `foundation/` diagnostics
- `main.c` CLI dispatch

## Out of scope

- 11-agent installer expansion (document only vs CBM cli/)
- 3D UI
- Auto-update on startup

## Program completion checklist ([V] P-S6-099)

- [ ] All acceptance-matrix rows PASS
- [ ] All decision-log PD-* closed
- [ ] Obsidian report updated
- [ ] AGENTS.md surface table delta (minimal)
- [ ] 015 marked complete in checklist

**Sign-off**: _________________ Date: _______
