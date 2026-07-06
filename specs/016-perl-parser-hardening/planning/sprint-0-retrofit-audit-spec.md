# Sprint S0 — Retrofit Audit

**Feature**: 016 · **US**: US0 · **Baseline**: `9572b31` on main

## Objective

Validate the merged ts-parser-perl swap before any new fixtures or xref edits.

## Baseline table

| Item | Value | Evidence |
|------|-------|----------|
| Merge commit | `9572b31` | git log |
| Parent | `30dd4c3` | rustfmt after merge integration |
| perl.rs delta | +1 test, ~3 symbols | EV-S0-002 |
| xref.rs delta | +4 tests, compile_xref_query | EV-S0-002 |
| Grammar lock | ts-parser-perl 1.1.3 | Cargo.lock |
| C++ neighbor | qualified_call preserved | EV-S0-003 |

## Planning Gate checklist

- [ ] P-S0-001..005 complete
- [ ] EV-S0-001..003 rows populated
- [ ] contracts/compile-xref-degradation reviewed
- [ ] acceptance-matrix S0 rows understood

## Release Gate checklist

- [ ] quickstart § S0 all commands exit 0
- [ ] sexp archived to docs/research/perl/
- [ ] No `[C]` required OR C-S0-001 hotfix merged and V-S0 re-run

## Out of scope

- New fixtures
- PERL_XREF_QUERY edits
- MCP changes

## V-S0 results log

| Command | Exit | Date | Notes |
|---------|------|------|-------|
| fmt --check | | | |
| clippy | | | |
| perl tests | | | |
| cpp qualified | | | |
| probe --ignored | | | |

(Fill during implement.)
