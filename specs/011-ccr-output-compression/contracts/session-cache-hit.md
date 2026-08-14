# Contract: session cache hit (US1)

**Surface**: `get_file_context`, `get_symbol`, `get_file_content`

## Parameters

| Param | Type | Default | Rule |
|-------|------|---------|------|
| `force_refresh` | bool | false | When true, bypass cache-hit; serve full body |

(Add to tool input structs if not already present; serde default false.)

## Cache key

Canonical key = `(tool_kind, path, symbol_name?, params_hash)` where
`params_hash` covers: `verbosity`, `compact`, `detail`, line range fields, batch
mode flags — any param that changes formatted output.

## Hit behavior

When cache hit and `force_refresh` is false **and** the CCR blob for that
serve is still retrievable:

1. Do **not** re-execute index query/format for full body.
2. Return a small redeemable hit body:

```text
Decision: cache_hit
Economics: cache_hit (session_repeat_read)
Session cache: {kind} {target} (prior_tokens={n}, session_age_secs={s})

SymForge did not re-execute the read for this request.
These bytes were served earlier on this MCP connection; that is not proof they are in your context.
retrieve: symforge_retrieve with hash="{hash}"
force_refresh=true re-reads the live index and is not the recovery path for missing bytes.

--- cache payload ---
{cache JSON including retrieve_handle}
```

3. A caller who does not have those bytes (for example a Cursor subagent
   sharing the parent's MCP connection) recovers them with existing
   `symforge_retrieve`, paying once. Do not re-execute the read tool and do
   not use `force_refresh=true` as byte recovery.
4. If the fetch record exists but `ccr_store.get(handle)` is `None` (evicted),
   that is a **miss** — re-serve and re-insert. Never return `cache_hit` for
   bytes that cannot be handed back.
5. Record ledger `cache_hit=true` when STEL economics path active.

STEL compact admission does **not** cache-hit these tools: the plan layer has
no generation identity. The primitive applies the generation-aware key.

## Miss behavior

Full serve; update `SessionFetchRecord` with new `approx_tokens` and
`fetched_at`.

## Compact STEL vs full tool

| Prior fetch | Current request | Result |
|-------------|-----------------|--------|
| STEL compact symbol | `get_symbol` full | **Miss** — serve full |
| `get_file_context` full | same params | **Hit** |
| `get_file_content` lines 1-50 | lines 1-100 | **Miss** — different params_hash |

## Exclusions

- Mutation tools: never cache-hit.
- `symforge_retrieve`: never cache-hit (always serves blob).
- Failed/empty reads: do not record fetch.

## Tests

- `tests/session_cache_hit.rs`: hit, miss, force_refresh, compact→full miss.
