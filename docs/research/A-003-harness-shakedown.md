# A-003 — Branch-binary harness shakedown

**Tasks:** T020–T021  
**Verdict:** **OPEN — BLOCKED**

## Blocker

**B-SFBENCH:** Harness driver and shakedown fixtures live in sf-bench workspace.

## SymForge binary (local)

| Field | Value |
|-------|-------|
| Debug binary | `target/debug/symforge.exe` (built 2026-06-13) |
| Release binary | Not built in this session |

Release shakedown requires `cargo build --release` + sf-bench harness command (blocked).

## Required shakedown command (when unblocked)

```powershell
# From sf-bench workspace — exact command TBD by harness README
cargo build --release
node compare-results.js --preflight --release 8.0 --baseline <shakedown.json> --candidate <shakedown.json>
```

## Row classification validation (T021)

Every measured row must expose:

- `equivalence`, `acceptedServe`, `sGteM`, `decision`, `mcpCalls`, `eligibleH6`

| Check | Status |
|-------|--------|
| Shakedown JSON exists | **FAIL** (blocked) |
| All rows have required fields | **FAIL** (blocked) |

**A-003 verdict:** OPEN (blocked)
