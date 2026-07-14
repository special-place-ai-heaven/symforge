## Trace summary

CCR applies only to five discovery paths. It stores the fully assembled, pre-token-truncation string and returns a line-truncated summary plus a continuation hash. Retrieval returns the stored string unchanged.

### 1. Eligible tools and default budgets

Defined by `TOOL_OUTPUT_PROFILES`; an explicit `max_tokens` wins over the default. [`ToolOutputProfile` / `resolve_tool_max_tokens`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:15)

| Tool | Default budget | Eligibility |
|---|---:|---|
| `search_text` | 8,000 tokens | Always |
| `search_symbols` | 8,000 tokens | Always |
| `find_references` | 8,000 tokens | Always |
| `explore` | 12,000 tokens | Always |
| `get_repo_map` | 16,000 tokens | Only when `detail == "full"` |

The first four handlers all finish through `apply_ccr_budget`: [`search_symbols`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5089), [`search_text`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5341), [`find_references`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8367), and [`explore`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:9603). `get_repo_map` explicitly selects CCR only for full detail; compact/tree use ordinary token truncation. [`get_repo_map`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4293)

### 2. Complete result versus stored result

`SymForgeServer::apply_ccr_budget` resolves the explicit/default budget and selects CCR only for eligible profiles. [`apply_ccr_budget`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:691)

The decision uses an approximate four bytes per token:

- No positive budget—including explicit `max_tokens: 0`—returns the complete string unchanged.
- If `result.len() <= max_tokens * 4`, the complete string is returned unchanged and nothing is stored.
- If oversized, `enforce_token_budget` cuts at a line boundary and appends a `[truncated]` notice containing the estimated original size. [`enforce_token_budget_flagged`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4780)
- CCR stores the original string before that token cut, then returns the truncated summary plus a retrieval footer. If truncation did not actually make the summary smaller, it returns the summary without storing. [`enforce_token_budget_with_ccr`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:219), [`apply_ccr_overflow`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:193)

“Complete” here means the complete formatted payload presented to the CCR stage—not necessarily every raw match. For example, `search_text` first compacts to at most 20 files and 10 non-error lines per file while retaining error-severity lines, and only then formats and applies CCR. [`compact_text_search_result`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:268), [`search_text` ordering](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5316)

### 3. Continuation exposure

The originating response receives this footer:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="<12-hex-handle>"
```

The exact construction is in [`apply_ccr_overflow`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:212). The continuation is consumed through the read-only `symforge_retrieve` MCP tool with input:

```json
{"hash": "a1b2c3d4e5f6"}
```

See [`SymforgeRetrieveInput`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:394) and [`symforge_retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10788).

The tool is available on the default full surface. Compact-3 dispatch rejects it because only `symforge`, `symforge_edit`, and `status` are admitted there. [`compact_surface_blocks` / `list_tools_for_profile`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:135)

### 4. Identifier creation, validation, and retrieval

`mint_handle` hashes the originating tool name followed by the complete formatted string using Rust’s `DefaultHasher`, keeps the low 48 bits, and renders exactly 12 lowercase hexadecimal characters. It is not the BLAKE3/salted scheme described by the older design documents. [`mint_handle`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186)

Retrieval:

1. Trims surrounding whitespace and lowercases the supplied hash.
2. Requires exactly 12 ASCII hexadecimal characters. Thus uppercase input is accepted after normalization.
3. Performs a direct lookup in the current `CcrStore`.
4. Returns a clone of `formatted_bytes`, without reformatting or reranking.
5. Returns `invalid hash` for malformed identifiers or `unknown or expired hash` for a store miss. [`symforge_retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792)

Validation is shape-only; the digest is not recomputed at retrieval and the stored `tool_name` is not checked.

The store is in-memory and bounded to 32 MiB and 256 entries. Insertions evict the oldest entries until within limits, so eviction or process/session loss makes a handle “unknown or expired.” [`CcrStore::new`, `insert`, and `evict_oldest`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:100)

### 5. Usage accounting on retrieval

A successful `CcrStore::retrieve` changes two internal counters:

- `retrieves += 1`
- `bytes_retrieved += stored_string.len()`

Every successful repeated retrieval counts again. Invalid or unknown hashes change neither counter. [`CcrEconomics`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:71), [`CcrStore::retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:158)

New storage similarly increments `offloads` and `bytes_stored`; reinserting identical content only refreshes its age and does not recount it. [`CcrStore::insert`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:121)

The user-facing `context_inventory` and admin heuristic expose offloads, stored bytes, and retrieved bytes, but not the retrieve count. These are explicitly in-memory heuristic counters, not durable STEL-ledger rows. [`format_context_inventory`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:564), [`CompressionHeuristicView`](<temp>/symforge-token-shakedown-a10ff102/src/server/admin/api_v1.rs:40). The retrieval handler also does not add the retrieved body to `SessionContext.total_tokens`.

## Source-level recovery gap

The local/in-process round trip uses the same `CcrStore` and works. The daemon-proxy path appears to use different stores:

- All five originating handlers return immediately from `proxy_tool_call`, so CCR storage occurs in the daemon session server. [`proxy branches`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4135)
- The daemon does expose a `symforge_retrieve` route against that session server. [`execute_tool_call`](<temp>/symforge-token-shakedown-a10ff102/src/daemon.rs:4484)
- But the front-end `symforge_retrieve` handler does not proxy; it directly reads the front-end server’s local `ccr_store`. [`symforge_retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792)

Consequently, a continuation minted by a daemon-proxied discovery request will be looked up in the wrong store when retrieved through that MCP front end and will report `unknown or expired`. Existing round-trip tests construct one local `SymForgeServer`; they do not cover this proxy round trip. No builds or tests were run.