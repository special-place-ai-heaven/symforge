# Phase 2 compact-surface gate report

**Report ID:** phase2-gate-2026-06-14
**Surface:** compact
**Baseline commit:** `b1f60196e763b19cdde35584bde284f625576f2a`
**Candidate results:** `docs/research/results-v8-phase2-candidate.json`
**Baseline results:** `(self)`
**Compare command:** `node scripts/compare-results.cjs docs/research/results-v8-phase2-candidate.json`
**H3 policy:** [docs/research/A-012-bypass-policy.md](docs/research/A-012-bypass-policy.md)

## Gate statuses

| Gate | Status |
|------|--------|
| H1 | NOT_CLAIMED |
| H2 | NOT_CLAIMED |
| H3 | PASS |
| H4 | PASS |
| H5 | PASS |
| H6 | NOT_CLAIMED |
| H7 | NOT_CLAIMED |
| H8 | NOT_CLAIMED |

## Computed metrics

- `session_net_accepted`: 13753
- `session_net_all36`: 22812
- H3 scope rows: 24
- H3 sGteM violations: 0
- H5 single-chain violations: 0
- Measured rows: 36
- Skipped rows: 0

## Diagnostics

All computed Phase 2 gates passed on measured rows.

## H3 scope note (A-012)

H3 evaluates **accepted serve** rows only (bypass/degrade/cache_hit excluded). When no `*_small` task ids are present, all accepted serve rows in the golden corpus are used.

## H5 note

Compact surface uses one external `symforge` MCP call per task. Multi-hop rows (`chain=multi`) execute legacy tools in-process but report `mcpCalls=1`.
