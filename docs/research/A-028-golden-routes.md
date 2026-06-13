# A-028 — Golden route corpus validation

**Tasks:** T026–T028  
**Verdict:** **OPEN — BLOCKED**

## Blocker

**B-SFBENCH:** `routes.golden.jsonl` not accessible. See [phase0-12a-sf-bench-path.md](./phase0-12a-sf-bench-path.md).

## Required corpus shape

Exactly **36** JSONL rows with fields:

`id`, `query`, `must_call`, `must_not_call`, `expected_decision`, `expected_equiv`, `chain`, `eligible_h6`, `notes`

## Automated validation (T027)

| Check | Result |
|-------|--------|
| Line count = 36 | **FAIL** (blocked) |
| Valid JSON per line | **FAIL** (blocked) |
| Unique `id` values | **FAIL** (blocked) |
| Required fields present | **FAIL** (blocked) |

**T027 verdict:** FAIL (blocked — not a corpus defect, workspace missing)

## Human semantic review (T028)

Minimum **10** rows reviewed for `expected_decision` and `expected_equiv` semantics.

| Rows reviewed | 0 / 10 minimum |
| Reviewer | — |
| Notes | Blocked until corpus available |

**A-028 verdict:** OPEN (blocked)
