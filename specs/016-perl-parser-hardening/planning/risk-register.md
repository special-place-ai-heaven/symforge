# Risk Register — 016 Perl Parser Hardening

| ID | Risk | Likelihood | Impact | Owner | Mitigation |
|----|------|------------|--------|-------|------------|
| R-016-001 | C++ qualified_call regression in S2 xref edits | M | H | implement | V-S0-003 + every S2 partial gate |
| R-016-002 | Fixture corpus too small for SC-002 90% | M | M | S1 | Document failure buckets; adjust threshold with evidence |
| R-016-003 | Grammar bump undetected in CI | L | H | S3 | Manual checklist + optional lockfile hook |
| R-016-004 | Speculative xref rules without sexp proof | M | M | S2 | P-S2-002 blocks C-S2-* |
| R-016-005 | compile_xref_query regression across langs | L | H | S0 | test_compile_xref_query_degrades_on_mismatch |
| R-016-006 | Accepted-loss constructs silently shipped | M | M | S1/S2 | FailureBucket + investigation doc § Limits |

## Escalation

CRITICAL constitution conflict → stop implement; update spec/plan before `[C]`.
