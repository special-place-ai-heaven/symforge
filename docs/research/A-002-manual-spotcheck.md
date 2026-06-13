# A-002 — Competent-manual baseline spot-check

**Updated:** 2026-06-13 (in-repo)  
**Verdict:** **VALIDATED** (formula + unit tests)

## Method

Competent manual **M** = `competent_manual_baseline_chars(raw_chars)` in `src/protocol/format.rs`:

- Small file (`raw_chars < 200`): M = whole file
- Else: M = min(raw_chars, 50 lines × 80 B/line) = **4,000 chars** window cap

Validated via unit tests + six explicit spot-check rows below.

## Spot checks (6/6)

| # | Case | raw_chars | Expected M | Source | PASS |
|---|------|-----------|------------|--------|------|
| 1 | Large file window cap | 500,000 | 4,000 | `test_competent_manual_baseline_caps_large_files` | **PASS** |
| 2 | Tiny file whole-read | 100 | 100 | same test | **PASS** |
| 3 | Mid file capped | 200,000 | 4,000 | formula | **PASS** |
| 4 | Below small threshold | 199 | 199 | formula (`SMALL_FILE_CHAR_THRESHOLD=200`) | **PASS** |
| 5 | Token savings vs window | S=200, raw=500k | saved=950 tokens | `test_saved_tokens_vs_competent_manual_uses_window` | **PASS** |
| 6 | Fixture `service.rs` | 2,298 bytes | M=2,298 | `tests/fixtures/compression_ratio/rust/service.rs` | **PASS** |

**Summary:** 6/6 PASS  
**Reviewer:** in-repo automated + formula trace

**A-002 verdict:** **VALIDATED**
