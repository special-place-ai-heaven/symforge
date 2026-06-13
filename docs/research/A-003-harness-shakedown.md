# A-003 — Branch-binary harness shakedown

**Updated:** 2026-06-13 (in-repo MCP shakedown)  
**Verdict:** **VALIDATED** (MCP probe) / **OPEN** (full sf-bench battery JSON)

## Release binary

| Field | Value |
|-------|-------|
| Binary | `target/release/symforge.exe` |
| Surface | `SYMFORGE_SURFACE=compact` |
| Fixture cwd | `tests/fixtures/compression_ratio/rust` |

## Shakedown

```powershell
# initialize + notifications/initialized + tools/list (stdio)
# Output: docs/research/A-003-mcp-shakedown.jsonl
```

| Check | Result |
|-------|--------|
| `initialize` succeeds | **PASS** |
| `tools/list` returns 3 compact tools | **PASS** |
| Artifact | [A-003-mcp-shakedown.jsonl](./A-003-mcp-shakedown.jsonl) |

## Row classification fields (battery JSON)

Full battery rows (`equivalence`, `acceptedServe`, `sGteM`, …) require STEL executor + task replay — **not in scope** for measurement probe.

| Check | Status |
|-------|--------|
| MCP shakedown completes | **PASS** |
| Battery row fields on measured tasks | **OPEN** (post-STEL) |

**A-003 verdict:** **VALIDATED** for Phase 0 MCP shakedown; battery row schema **OPEN**.
