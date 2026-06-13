# A-006 / A-027 — Host schema amortization policy

**Task:** T033  
**Status:** **POLICY DOCUMENTED** (host measurement still OPEN)

## A-006 statement

Hosts (Cursor) may amortize MCP `tools/list` schema across calls so per-call schema tax is less than sf-bench's **÷50** session divisor.

## A-027 statement

Battery **÷50** schema divisor is **harness-only** until A-006 is host-validated.

## Evidence policy (Phase 0 §12A)

Until host measurement completes:

1. **Controller / economics model** uses **conservative worst-case**: assume full schema bytes **per MCP call** (no amortization credit).
2. **sf-bench battery** continues ÷50 divisor with explicit footnote: "harness assumption — not product claim."
3. **Bypass accounting** must include full schema cost when amortization is unproven (see [A-012-bypass-policy.md](./A-012-bypass-policy.md)).

## Host measurement path (future)

| Step | Method |
|------|--------|
| 1 | Long-session Cursor attach with repeated `tools/call` |
| 2 | Compare observed schema tax vs ÷50 model |
| 3 | If amortization proven → update controller max() and A-027 verdict |

## Verdicts

| ID | Verdict | Notes |
|----|---------|-------|
| A-006 | **OPEN** | Conservative worst-case documented; host measurement pending |
| A-027 | **OPEN** | Harness divisor documented as non-product until A-006 VALIDATED |

**§12A "A-006/A-027 documented":** satisfies **documentation** requirement; assumptions remain OPEN for Phase 1 gate per §9.
