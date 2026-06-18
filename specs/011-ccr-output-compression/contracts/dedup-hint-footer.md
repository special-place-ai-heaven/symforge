# Contract: dedup hint footer (US4)

**Surface**: `get_file_context`, `get_symbol`, `get_file_content`

## When to append

ALL of:
- `force_refresh=true` (or full serve after bypass)
- Matching `SessionFetchRecord` exists for same cache key
- Response is full body (not cache-hit short-circuit)

## Format

Single line suffix after main body:

```text

[session: same {kind} fetched {age_secs}s ago (~{approx_tokens} est tokens); reuse prior unless content changed]
```

Where `{kind}` is `file` or `symbol`.

## When NOT to append

- First fetch in session
- Cache-hit response (US1)
- CCR summary responses (US2)
- Mutation tool outputs

## Tests

Assert footer present on second forced fetch; absent on first.
