## 1. Eligible tools and default budgets

The canonical list is `TOOL_OUTPUT_PROFILES`; a caller-supplied `max_tokens` overrides these defaults. [`resolve_tool_max_tokens`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:58>) uses `agent_max.or(profile_default)`.

| Tool | Default budget | Handler |
|---|---:|---|
| `search_text` | 8,000 tokens | [`SymForgeServer::search_text`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5111>) |
| `search_symbols` | 8,000 tokens | [`SymForgeServer::search_symbols`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4942>) |
| `find_references` | 8,000 tokens | [`SymForgeServer::find_references`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8088>) |
| `explore` | 12,000 tokens | [`SymForgeServer::explore`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8896>) |
| `get_repo_map` with `detail="full"` | 16,000 tokens | [`SymForgeServer::get_repo_map`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4135>) |

The values and eligibility flags are defined together in [`TOOL_OUTPUT_PROFILES`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:23>). Only full repo maps use CCR; compact/tree maps use ordinary hard truncation and provide no continuation. That branch is at [`get_repo_map` lines 4297–4300](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4297>).

## 2. Complete result versus stored result

[`SymForgeServer::apply_ccr_budget`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:692>) resolves the effective budget and sends eligible tools through [`enforce_token_budget_with_ccr`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:228>).

Its decision is:

- Missing or zero budget: return the complete payload unchanged. For profiled tools, an omitted budget normally becomes the profile default; explicit zero therefore acts as the no-cap case.
- Payload at or below `max_tokens × 4` bytes: return it unchanged, with no store entry or footer.
- Payload above that approximate cap: create a line-boundary-truncated summary through [`enforce_token_budget_flagged`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4793>), retain the original pre-token-cut string, and pass both to [`apply_ccr_overflow`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194>).
- If truncation actually made the output shorter, store the complete original and return the summary plus continuation. If the summary is not shorter than the original, return the summary without storing or exposing a handle.

The budget is approximate: the truncation notice and CCR footer are appended afterward, so the final response may be slightly larger than `max_tokens × 4`.

“Complete” here means complete before the CCR token cut, not necessarily every raw index match. Tool-level limits and ranking happen first. In particular, [`compact_text_search_result`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:269>) caps `search_text` to 20 files and generally 10 non-error lines per file before formatting; error-severity lines are preserved.

## 3. Continuation exposure

The truncated response ends with:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="{12-hex-handle}"
```

This is produced by [`apply_ccr_overflow`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194>). Recovery is exposed as the read-only MCP tool [`SymForgeServer::symforge_retrieve`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792>), accepting the single `hash` field defined by [`SymforgeRetrieveInput`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:396>).

[`mint_handle`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186>) hashes the originating tool name and complete formatted payload, takes the low 48 bits, and formats them as exactly 12 hexadecimal characters.

## 4. Identifier validation and retrieval

`symforge_retrieve`:

1. Trims the supplied hash and lowercases it.
2. Requires exactly 12 bytes and requires every character to be an ASCII hexadecimal digit.
3. Rejects malformed values with `CCR retrieve: invalid hash (expected 12 hex chars)`.
4. Calls [`CcrStore::retrieve`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:159>) and returns an exact clone of the stored formatted string.
5. Reports a valid-but-missing identifier as `CCR retrieve: unknown or expired hash '…'`.

The store is session-local on [`SymForgeServer::ccr_store`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:181>), memory-only, and initialized with a 32 MiB/256-entry limit in [`CcrStore::new`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:111>). Oldest blobs are evicted, so identifiers can expire through eviction, session termination, or restart. Retrieval itself has no `max_tokens`; a successful lookup returns the entire stored payload.

## 5. Usage-accounting changes on retrieval

On every successful lookup, `CcrStore::retrieve`:

- increments `CcrEconomics.retrieves` by one;
- adds the complete body’s byte length to `CcrEconomics.bytes_retrieved`.

Both use saturating arithmetic. Malformed and unknown/expired identifiers change neither counter because validation or lookup fails before incrementing. Repeated successful retrievals count repeatedly. Retrieval does not change `offloads` or `bytes_stored`.

The accounting is explicitly in-memory heuristic state in [`CcrEconomics`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:73>). [`format_context_inventory`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:468>) exposes `ccr_offloads`, `ccr_bytes_stored`, and `ccr_bytes_retrieved`; it does not print the `retrieves` count. The retrieval handler also does not add the returned body to the normal fetched-file/symbol/session-token inventory, nor does it emit a separate durable `ccr_retrieve` ledger event.

No files were changed, and no builds or tests were run.