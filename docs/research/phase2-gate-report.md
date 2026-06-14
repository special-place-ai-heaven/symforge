# Phase 2 compact-surface gate report

**Report ID:** phase2-gate-2026-06-14  
**Surface:** `compact` (`SYMFORGE_SURFACE=compact`)  
**Baseline commit:** `896840f984738ce0f77e9a9c1aae94011ceaee45`  
**Candidate results:** [`results-v8-phase2-candidate.json`](./results-v8-phase2-candidate.json)  
**Baseline results:** self (first compact-surface Phase 2 battery pin)  
**Compare command:**

```bash
node scripts/compare-results.cjs docs/research/results-v8-phase2-candidate.json --report docs/research/phase2-gate-report.md
```

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
| H3 | **FAIL** |
| H4 | **PASS** |
| H5 | **PASS** |
| H6 | NOT_CLAIMED |
| H7 | NOT_CLAIMED |
| H8 | NOT_CLAIMED |

## Computed metrics

| Metric | Value |
|--------|------:|
| `session_net_accepted` | 13543 |
| `session_net_all36` | 22602 |
| H3 scope rows | 24 |
| H3 sGteM violations | 1 |
| H5 single-chain violations | 0 |
| Measured rows | 36 |
| Skipped rows | 0 |

## Diagnostics

H3 violations: `records/t8_explore` (S=1143, M=1000)

All other accepted serve rows: `sGteM=false`.

## Reviewer / action notes (H3 FAIL)

**Row:** `records/t8_explore` — `explore` guidance response exceeds competent-manual window (M=1000 tokens at 4000-char cap).

**Observed:** Single compact `symforge` call returns 1143 response tokens including STEL envelope + explore guidance body.

**Not a multi-hop or MCP-call regression:** H5 PASS; decision=`serve`; `mcpCalls=1`.

**Recommended follow-up (out of P2-S4 scope):**

1. Route high-token explore queries through L2 `degrade` with `max_tokens_cap`, or
2. Tighten explore output budget on compact surface, or
3. Revisit M baseline for guidance-class tasks in a future measurement spike (A-011).

**Phase 2 minimum exit:** H3 FAIL blocks full Phase 2 exit until resolved or spec-amended. H4 + H5 PASS on this artifact.

## STEL extension fields (T030)

All 36 measured rows include `stel.{plan_id,decision,tools_called,predicted_tokens,actual_tokens,net_vs_manual,route_confidence}` parsed from ledger envelope metadata.

## Reproducibility

- Deterministic gate math: `src/stel/gates.rs` + `scripts/compare-results.cjs`
- CI fixtures: `tests/fixtures/phase2-gate/synthetic-*.json`
- Integration tests: `tests/stel_battery_gates.rs`
- Full 36-row battery requires phase0 corpora clone per [`tests/fixtures/phase0-corpus/README.md`](../../tests/fixtures/phase0-corpus/README.md)
