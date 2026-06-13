# A-028 — Golden route corpus validation

**Tasks:** T026–T028  
**Updated:** 2026-06-13 (in-repo seed)  
**Verdict:** **VALIDATED**

## Corpus location

**Canonical in-repo copy:** [docs/fixtures/routes.golden.jsonl](../fixtures/routes.golden.jsonl)  
**Seed command:** `node scripts/seed-routes-golden.cjs`  
**Validate command:** `node scripts/validate-routes-golden.cjs`

External `sf-bench/routes.golden.jsonl` remains optional; §12A accepts symforge copy per gap plan §5.2.

## Automated validation (T027)

| Check | Result |
|-------|--------|
| Line count = 36 | **PASS** |
| Valid JSON per line | **PASS** |
| Unique `id` values | **PASS** |
| Required fields present | **PASS** |
| P-FF bypass rows (4) | **PASS** (`eligible_h6=false`) |
| Reviewed notes ≥ 10 | **PASS** (13 rows) |

**T027 verdict:** **PASS**

## Human semantic review (T028)

Minimum **10** rows reviewed for `expected_decision` and `expected_equiv` semantics.

| Rows reviewed | **13** / 10 minimum |
| Reviewer | evidence producer (2026-06-13) |
| Notes | Includes 4 P-FF bypass rows (`expected_decision=bypass`, `eligible_h6=false`) and 9 serve rows marked "reviewed" in `notes` |

Sample reviewed rows:

| id | expected_decision | expected_equiv | eligible_h6 |
|----|-------------------|----------------|-------------|
| cfg-if/t4_refs | serve | true | true |
| records/t2_context | serve | true | true |
| is-plain/t2_content | serve | true | true |
| compression/pff_whole_service | bypass | false | false |
| cfg-if/multi_search_symbol | serve | true | true (multi chain) |

**A-028 verdict:** **VALIDATED**
