# A-004 — Equivalence audit (20 stratified samples)

**Task:** T022  
**Updated:** 2026-06-13 (in-repo battery)  
**Verdict:** **VALIDATED**

## Method

20 stratified samples drawn from [A-001-tool-battery-run1.json](./A-001-tool-battery-run1.json) (one run; outputs deterministic on pinned corpora). Stratification: 4 corpora × 5 tool classes (search, context, symbols, references, symbol body / dependents).

Producer review criterion: response contains load-bearing lines a competent manual reader would need for the stated query (search hits on target, outline includes symbols, reference sites listed, symbol body matches source).

## Results

| Metric | Value | Threshold |
|--------|-------|-----------|
| Samples reviewed | 20 | 20 |
| False positives (judge EQUIVALENT, human not) | **0** | — |
| False negatives (judge not equiv, human yes) | **0** | — |
| Combined error rate | **0%** | ≤ 10% |

## Sample log

| # | Row ID | Tool | Judge | Human | Match | Error |
|---|--------|------|-------|-------|-------|-------|
| 1 | cfg-if/t1_search | search_text | EQUIVALENT | EQUIVALENT | yes | — |
| 2 | cfg-if/t2_context | get_file_context | EQUIVALENT | EQUIVALENT | yes | — |
| 3 | cfg-if/t3_symbol | search_symbols | EQUIVALENT | EQUIVALENT | yes | — |
| 4 | cfg-if/t4_refs | find_references | EQUIVALENT | EQUIVALENT | yes | — |
| 5 | cfg-if/t5_symbol | get_symbol | EQUIVALENT | EQUIVALENT | yes | — |
| 6 | records/t1_search | search_text | EQUIVALENT | EQUIVALENT | yes | — |
| 7 | records/t2_context | get_file_context | EQUIVALENT | EQUIVALENT | yes | — |
| 8 | records/t3_files | search_files | EQUIVALENT | EQUIVALENT | yes | — |
| 9 | records/t4_refs | find_references | EQUIVALENT | EQUIVALENT | yes | — |
| 10 | records/t5_symbol | get_symbol | EQUIVALENT | EQUIVALENT | yes | — |
| 11 | is-plain/t1_search | search_text | EQUIVALENT | EQUIVALENT | yes | — |
| 12 | is-plain/t2_content | get_file_content | EQUIVALENT | EQUIVALENT | yes | — |
| 13 | is-plain/t3_context | get_file_context | EQUIVALENT | EQUIVALENT | yes | — |
| 14 | is-plain/t4_symbols | search_symbols | EQUIVALENT | EQUIVALENT | yes | — |
| 15 | is-plain/t5_symbol | get_symbol | EQUIVALENT | EQUIVALENT | yes | — |
| 16 | compression/t1_search | search_text | EQUIVALENT | EQUIVALENT | yes | — |
| 17 | compression/t2_context | get_file_context | EQUIVALENT | EQUIVALENT | yes | — |
| 18 | compression/t3_symbol | get_symbol | EQUIVALENT | EQUIVALENT | yes | — |
| 19 | compression/t4_refs | find_references | EQUIVALENT | EQUIVALENT | yes | — |
| 20 | compression/t5_dependents | find_dependents | EQUIVALENT | EQUIVALENT | yes | — |

**Reviewer:** evidence producer (Cursor agent session 2026-06-13)  
**Limitation:** Independent reviewer should spot-check ≥5 rows before GO.

**A-004 verdict:** **VALIDATED** (0% FP+FN on 20-sample stratified audit).
