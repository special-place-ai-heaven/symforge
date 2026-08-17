# A-019 — L0 surface choice (compact-3 vs meta-tool vs full-32)

**Updated:** 2026-06-13 (full L0 A/B battery)  
**Verdict:** **VALIDATED** — **compact-3** selected for Phase 1 L0

## H1 evidence (schema bytes)

| Candidate | tools/list bytes | tool count | Pass H1 |
|-----------|------------------|------------|---------|
| full-32 | 62,574 | 32 | **FAIL** |
| **compact-3** | **891** | 3 | **PASS** |
| meta-1 | 407 | 1 | **PASS** |

Source: [A-005-schema-bytes.json](./A-005-schema-bytes.json), [A-019-l0-ab-results.json](./A-019-l0-ab-results.json).

## Session_net battery (20-row pinned corpus)

**Method:** `scripts/phase0-l0-ab-battery.cjs` on four pinned corpora (same 20 scenarios as A-001).  
`SYMFORGE_NO_DAEMON=1`. Token method: `ceil(utf8Bytes/4)`; M = competent-manual window.

| Surface | MCP path | session_net_accepted | equiv rows | H1 |
|---------|----------|----------------------|------------|-----|
| full-32 | legacy tools direct | **14,389** | 20/20 | FAIL |
| compact-3 | `symforge` facade relay | **14,389** | 20/20 (byte parity vs full) | PASS |
| meta-1 | `symforge` facade relay | **14,389** | 20/20 (byte parity vs full) | PASS |

**Artifact:** [A-019-l0-ab-results.json](./A-019-l0-ab-results.json)

### Measurement relay (Phase 0 only)

Compact/meta surfaces call `symforge` with schema-hidden `_probe_legacy_tool` /
`_probe_legacy_args` compatibility fields. The relay is production-reachable but
restricted to a source-mutation-safe measurement allowlist; `batch_rename` is
admitted only with `dry_run=true`. This is not a side-effect-free promise: normal
read-path cache, frecency, coupling, and index-refresh effects may still occur.
The relay preserves exact legacy renderer text and deliberately omits
`symforge/result_status`, because arbitrary returned source text cannot prove a
semantic outcome class. The shared MCP boundary still attaches
`symforge/project_evidence`, but exempts an admitted relay from its rendered-text
`isError` heuristic so valid source beginning with error-shaped bytes remains data.
The input lives in `src/stel_core/types.rs`; the policy and dispatch live in the
`symforge` tool handler; and the boundary exemption lives in
`src/protocol/mod.rs::call_tool`. The path bypasses the normal STEL planner only for
this Phase 0 measurement compatibility case.

The numeric table above is the historical 2026-06-13 A-019 measurement and linked
artifact. It selected compact-3 but does not itself validate the later containment
policy; the current policy is covered by focused handler, real MCP-boundary, and
both fixture-harness controls recorded in the Slice 3 evidence.

## Winner selection (gap plan §4.1)

1. Eligible surfaces: H1 PASS **and** output parity with full-32 on all 20 rows.
2. Rank by `session_net_accepted`.
3. Tie-break: compact-3 (simpler).

**Result:** compact-3 and meta-1 tied on session_net (14,389). **Winner: compact-3** (tie-break).

full-32 disqualified on H1 despite matching session_net.

## Decision

**Select compact-3** for L0 public surface:

1. H1 PASS (891 B)
2. Session_net parity with full-32 on pinned battery (14,389 accepted)
3. Equivalence: 20/20 row output byte-match vs full-32
4. `stel-schema.md` L0 registry alignment (`symforge`, `symforge_edit`, `status`)
5. Gap-plan tie-break vs meta-1

**A-019 verdict:** **VALIDATED (compact-3)**
