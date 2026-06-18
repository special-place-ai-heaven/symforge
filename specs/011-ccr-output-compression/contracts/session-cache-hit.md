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

When cache hit and `force_refresh` is false:

1. Do **not** re-execute index query/format for full body.
2. Return body shaped like STEL cache-hit:

```text
Decision: cache_hit
Economics: cache_hit (session_repeat_read)
Session cache: {kind} {target} (prior_tokens={n}, session_age_secs={s})

SymForge did not re-execute a legacy tool for this request.
Reuse the content already loaded in this session.

--- cache payload ---
{StelCacheBody JSON}
```

3. Record ledger `cache_hit=true` when STEL economics path active.

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
