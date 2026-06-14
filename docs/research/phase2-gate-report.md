# Phase 2 compact-surface gate report

**Report ID:** phase2-gate-2026-06-14  
**Surface:** `compact` (`SYMFORGE_SURFACE=compact`)  
**Baseline commit:** `896840f984738ce0f77e9a9c1aae94011ceaee45` (P2-S4 first pin)
**Candidate results:** [`results-v8-phase2-candidate.json`](./results-v8-phase2-candidate.json)
**Refreshed at:** H3 remediation merge (post-#308)
**Compare command (reproducible script output — does not overwrite this file):**

```bash
node scripts/compare-results.cjs docs/research/results-v8-phase2-candidate.json --report docs/research/phase2-gate-report.generated.md
```

**Generated report mirror:** [`phase2-gate-report.generated.md`](./phase2-gate-report.generated.md) (auto-written by compare-results; numeric results must match this curated artifact)

**Battery command:**

```bash
cargo build -p symforge
node scripts/phase2-compact-battery.cjs target/debug/symforge docs/research/results-v8-phase2-candidate.json
```

**H3 policy:** [A-012-bypass-policy.md](./A-012-bypass-policy.md) (serve-only scope; bypass rows excluded)

## Gate definitions (binding gap plan §5.1)

| Gate | Definition | PASS criterion |
|------|------------|----------------|
| **H3** | Accepted serve rows (`decision=serve` ∧ `equivalence=EQUIVALENT`); A-012 excludes bypass | Zero rows with `sGteM=true` in H3 scope |
| **H4** | `session_net_accepted = Σ(M−S)` over accepted serve rows only (A-026) | `session_net_accepted ≥ 0` |
| **H5** | External MCP calls per golden row | `mcpCalls ≤ 1` for all `chain=single` rows |

H3 scope uses all accepted serve rows when no `*_small` task ids exist (Phase 2 golden corpus naming).

## Gate statuses

| Gate | Status |
|------|--------|
| H1 | NOT_CLAIMED |
| H2 | NOT_CLAIMED |
| H3 | **PASS** |
| H4 | **PASS** |
| H5 | **PASS** |
| H6 | NOT_CLAIMED |
| H7 | NOT_CLAIMED |
| H8 | NOT_CLAIMED |

## Computed metrics

| Metric | Value |
|--------|------:|
| `session_net_accepted` | 13753 |
| `session_net_all36` | 22812 |
| H3 scope rows | 24 |
| H3 sGteM violations | 0 |
| H5 single-chain violations | 0 |
| Measured rows | 36 |
| Skipped rows | 0 |

## Diagnostics

All computed Phase 2 gates passed on measured rows.

Prior P2-S4 failure (`records/t8_explore` S=1143, M=1000) remediated by compact-serve explore `max_tokens` cap (`COMPACT_SERVE_EXPLORE_MAX_TOKENS=750` in `src/stel/executor.rs`); refreshed row S=929, M=1000.

## H3 remediation note (P2-S4.1)

**Row:** `records/t8_explore` — explore guidance on `records-python` exceeded competent-manual window before cap.

**Fix:** On compact `symforge` **serve**, apply `max_tokens=750` to `explore` steps (250-token reserve for STEL envelope + serve routing meta vs H3 M=1000). Decision remains `serve`; guidance is truncated with standard budget footer when needed.

**Not changed:** L2 economics thresholds, A-029, persistence, H6–H8 claims, compact-3 tool names.

## STEL extension fields (T030)

All 36 measured rows include `stel.{plan_id,decision,tools_called,predicted_tokens,actual_tokens,net_vs_manual,route_confidence}` parsed from ledger envelope metadata.

## Reproducibility

- Deterministic gate math: `src/stel/gates.rs` + `scripts/compare-results.cjs`
- CI fixtures: `tests/fixtures/phase2-gate/synthetic-*.json`
- Integration tests: `tests/stel_battery_gates.rs`
- Full 36-row battery requires phase0 corpora clone per [`tests/fixtures/phase0-corpus/README.md`](../../tests/fixtures/phase0-corpus/README.md)
