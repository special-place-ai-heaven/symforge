# Perl corpus taxonomy sign-off — S1

**Date**: 2026-07-06  
**Reviewer**: implement agent (016 program)  
**Verdict**: **GO** for S2 Planning Gate

## P1 constructs for S2

| Construct | Evidence | Action |
|-----------|----------|--------|
| qualified_call (`Foo::bar()`) | `qualified_call.pl` — xref optional/missing | Extend PERL_XREF_QUERY |

## Accepted loss (S2 will not implement unless corpus disproves)

- dynamic/indirect invocation (`${...}()`, symbolic refs)
- SUPER:: / CORE:: until dedicated fixtures + sexp proof

## Corpus summary

- 22 fixtures, 100% clean parse
- All non-optional symbol/ref expectations pass
- Metrics: `docs/research/perl/corpus-metrics.json`

## GO / NO-GO

**GO** — proceed to S2 qualified_call wave per [sprint-2-coverage-expansion-spec.md](../specs/016-perl-parser-hardening/planning/sprint-2-coverage-expansion-spec.md).
