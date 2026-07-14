CCR is a reversible overflow layer over five discovery tools. It stores the fully formatted, pre-token-truncation payload and returns a shortened response containing a recovery hash.

## 1. Eligible tools and default budgets

The authoritative table is `TOOL_OUTPUT_PROFILES`; an explicit `max_tokens` overrides the default. [`ToolOutputProfile` and `resolve_tool_max_tokens`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:17)

| Tool | Default budget | Runtime scope |
|---|---:|---|
| `search_text` | 8,000 tokens | Normal and structural search result paths. [`search_text`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5111) |
| `search_symbols` | 8,000 tokens | Final formatted result. [`search_symbols`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4942) |
| `find_references` | 8,000 tokens | Applied to the default `references` branch. `mode="implementations"` returns before the CCR call. [`find_references`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8088) |
| `explore` | 12,000 tokens | Final formatted exploration result. [`explore`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:9555) |
| `get_repo_map` | 16,000 tokens | CCR is used only for `detail="full"`; compact/tree modes use ordinary irreversible budget truncation. [`get_repo_map`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4171) |

`SymForgeServer::apply_ccr_budget` resolves the override/default and routes eligible tools through CCR. [`SymForgeServer::apply_ccr_budget`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:691)

An explicit `max_tokens=0` effectively disables the limit: it overrides the profile default, then `enforce_token_budget_with_ccr` treats non-positive budgets as unlimited.

## 2. Complete result versus stored result

`enforce_token_budget_with_ccr` uses an approximate four bytes per token:

- No positive budget, or payload length ≤ `max_tokens * 4`: return the complete string unchanged; store nothing.
- Oversized: clone the complete string, truncate the response at a line boundary through `format::enforce_token_budget`, and append its canonical truncation notice.
- If truncation did not actually shorten the payload, return the summary without storing.
- Otherwise, store the original string and return the truncated summary plus the CCR footer.

[`enforce_token_budget_with_ccr` and `apply_ccr_overflow`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:193)<br>
[`format::enforce_token_budget_flagged`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4780)

“Complete” means the complete string handed to CCR, not every possible repository match. Earlier tool limits remain irreversible. In particular:

- `search_text` first keeps at most 20 ranked files and 10 non-error lines per file while preserving all error-severity lines; CCR stores the result after that compaction. [`compact_text_search_result`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:268)
- `get_repo_map(detail="full")` applies `max_files`, defaulting to 200, before CCR. [`get_repo_map`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4190)

The in-memory store is bounded to 32 MiB and 256 entries, evicting the oldest entry first. [`CcrStore::new` and `CcrStore::evict_oldest`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:110)

## 3. Continuation exposure

This is not MCP cursor pagination. The originating response receives a textual footer:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="<12-hex-hash>"
```

The client then calls the separate read-only `symforge_retrieve` tool with `{ "hash": "..." }`. [`apply_ccr_overflow`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:193) [`SymforgeRetrieveInput`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:394)

The recovery tool is available on the default full surface. It is neither advertised nor callable on compact-3, whose only tools are `symforge`, `symforge_edit`, and `status`. [`COMPACT_TOOL_NAMES`](/<temp>/symforge-token-shakedown-a10ff102/src/stel/surface.rs:3) [`compact_surface_blocks`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/surface_probe.rs:135)

## 4. Identifier validation and retrieval

`mint_handle` hashes the tool name followed by the complete formatted string using Rust’s `DefaultHasher`, masks it to 48 bits, and renders exactly 12 lowercase hexadecimal characters. It is not a BLAKE3 identifier in the current implementation. [`mint_handle`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186)

`SymForgeServer::symforge_retrieve`:

1. Trims the supplied hash and lowercases it—so uppercase hex is accepted.
2. Requires exactly 12 ASCII hexadecimal characters.
3. Calls `CcrStore::retrieve` using that normalized key.
4. Returns an exact clone of `formatted_bytes`, with no reformatting or reranking.
5. Returns `unknown or expired` when the key is absent or was evicted.

[`SymForgeServer::symforge_retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10788) [`CcrStore::retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:158)

There is no secondary collision/content check or origin-tool validation during retrieval; lookup is solely by the 12-character key.

The store is created with each `SymForgeServer` and retained only in memory. Server clones share its `Arc`; the HTTP service clones one runtime server for all stateless requests, so HTTP storage is runtime-scoped and disappears on restart. [`SymForgeServer::new`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:235) [`build_mcp_service`](/<temp>/symforge-token-shakedown-a10ff102/src/server/mcp_http.rs:97)

## 5. Accounting on retrieval

A successful retrieval:

- Increments `CcrEconomics.retrieves` by one.
- Adds the complete stored payload length to `bytes_retrieved`.
- Leaves the blob in the store, so repeated retrievals increment both values again.

Invalid or unknown hashes change no counters. [`CcrEconomics` and `CcrStore::retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:71)

The surfaced session/admin heuristic exposes `ccr_bytes_retrieved`, but not the retrieval count. `context_inventory` uses `retrieves > 0` to decide whether to show the compression section, then prints only offloads, stored bytes, and retrieved bytes. [`format_context_inventory`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:564) [`CompressionHeuristicView`](/<temp>/symforge-token-shakedown-a10ff102/src/server/admin/api_v1.rs:40)

The retrieval handler does not add the recovered body to `SessionContext.total_tokens`, record token savings, or emit a separate durable STEL ledger event; its only accounting mutation is through `CcrStore::retrieve`.

No files were changed, and no builds or tests were run.