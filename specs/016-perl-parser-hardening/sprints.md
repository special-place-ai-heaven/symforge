# Sprints: Perl Parser Hardening

**Program**: 016 · **Model**: [execution-model.md](./execution-model.md)

| Sprint | Dates (target) | Theme | US | Release note |
|--------|----------------|-------|-----|--------------|
| PROG | 2026-07-06 | Bootstrap | — | — |
| S0 | 2026-07-06 | Retrofit audit | US0 | Validates `9572b31` |
| S1 | 2026-07-07 – 07-10 | Evidence + corpus | US1 | Fixtures land |
| S2 | 2026-07-10 – 07-17 | Coverage expansion | US2 | Xref gaps closed |
| S3 | 2026-07-17 – 07-20 | Operational | US3–US4 | Doc + bump protocol |

## Dependencies

```text
PROG → S0 → S1 → S2 → S3
         ↑ hotfix only if S0 fails
```

S2 construct-class waves are parallelizable **after** S1 taxonomy sign-off.

## Calendar notes

- S0 should complete in one session (audit only)
- S1 is planning-heavy (~60% of program `[P]` effort)
- S2 coding waves capped at 6 `[C]` per session
- S3 can overlap S2 doc draft but investigation doc final waits for S2 metrics
