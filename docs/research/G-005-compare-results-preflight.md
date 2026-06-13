# G-005 — compare-results preflight gate computation

**Tasks:** T023–T024  
**Verdict:** **OPEN — BLOCKED**

## Blocker

**B-SFBENCH:** `compare-results.js` and `RESULTS.md` not present on this machine.

## Required command (when unblocked)

```powershell
node <sf-bench>\compare-results.js --preflight --release 8.0 `
  --baseline <shakedown-or-fixture.json> `
  --candidate <shakedown-or-fixture.json>
```

Acceptable pre-GO inputs per gap plan §5.2:

- Self-diff (same file baseline + candidate), or
- `fixtures/preflight-minimal.json`

## H1–H8 fields (preflight mode)

| Gate | Field | Value | Pass |
|------|-------|-------|------|
| H1 | schemaBytes | — | — |
| H2 | trajectoryPassRate | — | — |
| H3 | smallServeSGteMCount | — | — |
| H4 | sessionNetAccepted | — | — |
| H5 | singleChainMcpCallsOk | — | — |
| H6 | equivalent / eligible | — / — | — |
| H7 | acceptedNetVariance | — | — |
| H8 | perLanguageAcceptedLosses | — | — |

**Exit status:** — (not run)

## RESULTS.md §8.7 (T024)

| Check | Status |
|-------|--------|
| §8.7 documents compare-results columns for **v8 runs only** | **FAIL** (file missing) |
| 7.x results marked informational only | N/A until workspace restored |

**G-005 / compare-results §12A item:** OPEN (blocked)
