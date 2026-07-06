# Sprint S1 — Evidence + Corpus

**Feature**: 016 · **US**: US1

## Objective

Build committed fixture corpus and measured parse-quality metrics; draft investigation doc.

## Fixture sourcing guidelines

1. Prefer real patterns from CPAN-style code (short excerpts, license-permitting or synthetic equivalent)
2. Tag every file with ≥1 ConstructClass from [data-model.md](../data-model.md)
3. Include at least 2 partial-parse candidates if natural
4. Minimum 20 files before Release Gate

## Taxonomy sign-off

Output: `docs/research/perl/taxonomy-signoff.md`

Required sections:
- Reviewer
- Date
- P1 construct list for S2
- Accepted-loss list with rationale
- GO / NO-GO for S2

**NO-GO** blocks S2 Planning Gate.

## Planning Gate checklist

- [ ] P-S1-001..009 complete
- [ ] Fixture README design approved
- [ ] Investigation outline matches dart template sections

## Release Gate checklist

- [ ] SC-001: ≥20 fixtures
- [ ] SC-002: corpus-metrics.json populated
- [ ] V-S1-002 taxonomy sign-off GO
- [ ] investigation doc draft exists

## Metrics template

See [data-model.md § CorpusMetrics](../data-model.md#corpusmetrics).

Target: report clean_parse_pct honestly — if <90%, document why in investigation doc § Failure classes.
