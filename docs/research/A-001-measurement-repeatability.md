# A-001 — Measurement repeatability

**Updated:** 2026-06-13 (in-repo battery)  
**Verdict:** **VALIDATED** (session_net 2× battery + schema proxy)

## In-repo session_net battery (primary)

External sf-bench optional. Repeatability measured via `scripts/phase0-mcp-battery.cjs` on four corpora (Rust cfg-if, Python records, TS is-plain-obj, in-repo compression fixture). Token method: `ceil(utf8Bytes/4)`; M = competent-manual window per sf-bench convention.

| Run | Artifact | Rows | session_net_accepted |
|-----|----------|------|----------------------|
| 1 | [A-001-tool-battery-run1.json](./A-001-tool-battery-run1.json) | 20 | **14,389** |
| 2 | [A-001-tool-battery-run2.json](./A-001-tool-battery-run2.json) | 20 | **14,389** |

| Metric | Value | Threshold | Pass |
|--------|-------|-----------|------|
| session_net variance | **0.0%** | ≤ 2% (H7 / A-001) | **PASS** |
| Row count | 20 / 36 golden | diagnostic | partial corpus |

**Note:** Golden file defines 36 rows; battery currently exercises 20 single-hop legacy-tool scenarios across 4 corpora. Remaining 16 rows (4 P-FF bypass + 3 multi-chain + 9 not yet wired) are seeded for H2 replay in Phase 1+.

## Schema measurement proxy (secondary)

| Run | Artifact | compact schemaBytes |
|-----|----------|---------------------|
| 1 | [A-005-schema-bytes.json](./A-005-schema-bytes.json) | **891** |
| 2 | [A-005-schema-bytes-run2.json](./A-005-schema-bytes-run2.json) | **891** |

| Metric | Value | Threshold | Pass |
|--------|-------|-----------|------|
| compact schema variance | **0.0%** | ≤ 2% | **PASS** |

**A-001 verdict:** **VALIDATED** — 2× in-repo battery session_net repeatability PASS (0% variance).
