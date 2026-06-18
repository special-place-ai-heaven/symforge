# Contract: CCR retrieve (US2)

**Surface**: `symforge_retrieve` (full MCP surface; not compact-3 default)

## Input

```json
{
  "hash": "a1b2c3d4e5f6"
}
```

- `hash`: required, 12 lowercase hex chars (BLAKE3 prefix).

## Success output

Full `formatted_bytes` from store — **identical** to uncapped tool output
(string equality). No re-formatting, no re-ranking.

Optional economics footer allowed **after** body only if it does not alter
stored bytes (prefer: retrieve returns raw stored string only).

## Error output

| Condition | Message pattern |
|-----------|-----------------|
| Unknown hash | `CCR retrieve: unknown or expired hash '{hash}'` |
| Invalid hash format | `CCR retrieve: invalid hash (expected 12 hex chars)` |
| Session scope miss | Same as unknown (do not leak cross-session existence) |

## Overflow footer (on originating tool)

When CCR triggers on a discovery tool:

```text
---
CCR: {omitted_count} matches omitted · full output {bytes} bytes
retrieve: symforge_retrieve with hash="{hash}"
```

Trust envelope MUST include `truncated: true` or equivalent result-status flag
with reason `ccr_offload`.

## Eligible origin tools (v1)

- `search_text`
- `search_symbols`
- `find_references`
- `explore`
- `get_repo_map` when `detail=full`

## Ineligible (never CCR opaque)

- `get_file_content`, `get_symbol`, `get_symbol_context` when used for edit prep
- All structural edit tools
- `get_file_context` (use verbosity + section gating; existing ratio CI)

## Store limits

Per session: 32 MiB total, 256 entries — eviction oldest first.

## Tests

- `tests/ccr_retrieve.rs`: round-trip equality, unknown hash, budget trigger.
