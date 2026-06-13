# A-019 — L0 surface choice (compact-3 vs meta-tool vs full-32)

**Updated:** 2026-06-13 (in-repo H1 evidence)  
**Verdict:** **INTERIM LOCK** — compact-3 for Phase 1 schema; **OPEN** for full session_net battery

## H1 evidence (gathered in-repo)

| Candidate | tools/list bytes | Pass H1 |
|-----------|------------------|---------|
| full-32 | 62,574 | FAIL |
| **compact-3** | **891** | **PASS** |
| meta-tool | not probed | — |

## Session_net battery (A-019 full validation)

Full A/B on 36-row corpus **not run** — external sf-bench deprioritized.

## Interim decision (Phase 0)

**Select compact-3** for L0 public surface based on:

1. H1 PASS (891 B vs 62,574 B full surface)
2. `stel-schema.md` L0 registry alignment (`symforge`, `symforge_edit`, `status`)
3. Gap plan tie-break: if meta-tool battery tied → compact-3 (simpler)

Meta-tool surface probe **deferred** until STEL Phase 1 or explicit A/B request.

## Revisit trigger

Invalidate interim lock if meta-tool battery beats compact on **session_net_accepted + equivalence** on pinned corpus.

**A-019 verdict:** **INTERIM LOCK (compact-3)** — full VALIDATED pending battery
