# G-005 — Gate preflight computation (in-repo)

**Updated:** 2026-06-13  
**Verdict:** **PARTIAL PASS** (H1 + H7 proxy)

## Source

In-repo preflight summary: [G-005-inrepo-preflight.json](./G-005-inrepo-preflight.json)  
Gather: `scripts/gather-phase0-evidence.ps1`

Legacy external `compare-results.js` **not required**.

## H1–H8 (preflight / diagnostic mode)

| Gate | Field | Value | Pass |
|------|-------|-------|------|
| H1 | schemaBytes | **891** | **PASS** (≤5,000) |
| H2 | trajectoryPassRate | — | OPEN (needs golden replay) |
| H3 | smallServeSGteMCount | — | OPEN (needs battery rows) |
| H4 | sessionNetAccepted | — | OPEN |
| H5 | singleChainMcpCallsOk | — | OPEN (needs STEL executor) |
| H6 | equivalent / eligible | — | 8.1 |
| H7 | acceptedNetVariance | **0.0%** (schema proxy) | **PASS** (≤2%) |
| H8 | perLanguageAcceptedLosses | — | 8.1 |

**Exit status:** `diagnostic` (H1 + H7 proxy pass; other gates await STEL/battery)

## RESULTS.md §8.7

In-repo equivalent: gate fields documented in [G-005-inrepo-preflight.json](./G-005-inrepo-preflight.json). External `RESULTS.md` not required.

**G-005 §12A item:** **PARTIAL** — preflight computes H1/H7; full column set when battery exists.
