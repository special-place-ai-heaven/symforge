# A-029 — Tokio Recall Spike (T2.1 inventory)

**Repo:** tokio (shallow clone)
**Corpus SHA:** `7892f6020d9c914a41d0c350693fb71937d43c03`
**SymForge commit:** `470826a`
**Measured:** 2026-06-14

## Tasks

| Task ID | Symbol | min recall | baseline files | cited files | matched | recall | equiv (A-029) |
|---------|--------|------------|----------------|-------------|---------|--------|---------------|
| `tokio/t2_spawn` | spawn | 35% | 252 | 20 | 16 | **6.3%** | SYMFORGE-LESS |
| `tokio/t2_block_on` | block_on | 35% | 141 | 20 | 20 | **14.2%** | SYMFORGE-LESS |

Artifacts: [`rg-hits/tokio/t2_spawn.json`](./rg-hits/tokio/t2_spawn.json), [`rg-hits/tokio/t2_block_on.json`](./rg-hits/tokio/t2_block_on.json)

## Missed-site bucket summary

| Task | missed total | source | test | example | bench (in missed) |
|------|--------------|--------|------|---------|-------------------|
| `t2_spawn` | 236 | 86 | 149 | 1 | 13 |
| `t2_block_on` | 121 | 49 | 72 | 0 | 7 |

## Top missed prefixes (`t2_spawn`)

| Prefix | Count |
|--------|-------|
| `tokio/tests` | 98 |
| `tokio/src` | 81 |
| `tokio-util/tests` | 11 |
| `tokio-stream/tests` | 10 |

## Observations

1. **Routing preserved:** compact `symforge` routes `find_references` for both queries (same as Phase 2 A-029).
2. **Cited file count ~20** aligns with default compact `OutputLimits` (`max_files=20`, `max_per_file=10`) — output cap is a first-class recall limiter for the A-029 file-recall metric.
3. **Tests dominate misses** but some `tokio/src` production files also missed — not explained by cap alone (index/xref gap likely).
4. **Bench files:** 13 bench paths in rg baseline for `spawn` are missed while other bench files appear in cited set — partial bench coverage, not all-or-nothing.
5. **No `.md` in rg baseline** (glob `*.rs` only); markdown hypothesis requires separate audit pass.

## Reproduce

```bash
node scripts/a029-t21-rg-inventory.cjs "$(pwd)/target/debug/symforge"
```

Phase 2 baseline for comparison: [`a029-t2-results.json`](./a029-t2-results.json) (recall 7.1% / 14.2% — minor file-match delta from path normalization on `t2_spawn`).

**No A-029 PASS claimed.** P-T2 retained.
