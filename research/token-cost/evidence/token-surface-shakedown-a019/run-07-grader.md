## 1. Eligible tools and default budgets

The static `TOOL_OUTPUT_PROFILES` table defines five CCR-eligible discovery tools. An explicit `max_tokens` overrides the default; omission selects the profile default via `resolve_tool_max_tokens`. See [`ToolOutputProfile` and `TOOL_OUTPUT_PROFILES`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:17) and [`resolve_tool_max_tokens`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:58).

| Tool | Default budget | Handler boundary |
|---|---:|---|
| `search_text` | 8,000 tokens | [`search_text`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5111), including its alternate return paths |
| `search_symbols` | 8,000 tokens | [`search_symbols`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4942), budget applied at line 5089 |
| `find_references` | 8,000 tokens | [`find_references`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8088), budget applied at line 8367 |
| `explore` | 12,000 tokens | [`explore`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8896), budget applied at line 9603 |
| `get_repo_map` | 16,000 tokens | [`get_repo_map`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4135), but CCR is applied only when resolved detail is `"full"` at line 4298 |

For `get_repo_map`, compact and tree modes use ordinary `enforce_token_budget`, without CCR or the 16,000-token default. If detail is omitted but `max_tokens` is supplied, the handler may adaptively select full or compact first.

## 2. Complete return versus storage

[`SymForgeServer::apply_ccr_budget`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:692) resolves the override/default, confirms eligibility, locks the session store, and calls [`enforce_token_budget_with_ccr`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:228).

The decision uses an approximate conversion of four bytes per token:

- If the resolved budget is zero, the complete formatted result is returned. A zero override therefore disables limiting.
- If `result.len() <= max_tokens * 4`, the result is returned byte-for-byte, with no storage or footer.
- If it exceeds that limit, the ordinary [`enforce_token_budget`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4785) produces a line-boundary-truncated summary.
- If that summary is strictly smaller than the complete result, the complete pre-budget payload is stored and the summary is returned with a continuation footer.
- Defensive edge case: if truncation saved nothing (`summary.len() >= full.len()`), the summary is returned without storing a continuation.

“Complete” here means the complete formatted payload arriving at the CCR boundary. Earlier tool-specific caps still apply—for example `get_repo_map.max_files` and search-result compaction.

[`CcrStore::insert`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:121) keeps the payload in memory. The store is attached to `SymForgeServer` as an `Arc<Mutex<CcrStore>>` and initialized with each server instance; its limits are 32 MiB and 256 entries, evicting the oldest entries first. See [`CcrStore`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:102) and the server field at [`protocol/mod.rs`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:181).

## 3. Continuation exposure

[`apply_ccr_overflow`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194) appends this footer to the truncated summary:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="<12-hex-handle>"
```

Recovery is exposed as the read-only MCP tool [`symforge_retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792). Its schema accepts one required `hash` string through [`SymforgeRetrieveInput`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:396).

The continuation returns the stored string directly—there is no reformatting, reranking, or second token-budget pass.

## 4. Identifier creation, validation, and retrieval

[`mint_handle`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186) hashes both:

- originating tool name;
- complete formatted result.

It uses Rust’s `DefaultHasher`, masks the result to 48 bits, and renders exactly 12 hexadecimal characters. Identical tool/output pairs reuse the same handle; duplicate insertion refreshes its age without recounting the offload.

On retrieval, [`symforge_retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792):

1. Trims whitespace.
2. Lowercases the identifier.
3. Requires exactly 12 ASCII hexadecimal characters.
4. Calls `CcrStore::retrieve` under the store mutex.
5. Returns either the cloned full body or `CCR retrieve: unknown or expired hash '<hash>'`.

Thus uppercase input is accepted after normalization, despite planning documents describing lowercase-only handles. Also, the current implementation uses `DefaultHasher`, not the BLAKE3-prefix algorithm stated in the feature contract.

Identifiers are effectively store/session scoped: an identifier from another server session, or one evicted by the capacity limits, resolves as unknown/expired.

## 5. Retrieval accounting

A successful [`CcrStore::retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:159) makes two saturating changes:

- `retrieves += 1`;
- `bytes_retrieved += stored_body.len()`.

Failed validation and unknown/expired handles change neither counter. Retrieval does not remove the blob, so every successful repeat retrieval increments both again. It does not alter `offloads` or `bytes_stored`.

These are explicitly per-session, in-memory heuristic counters in [`CcrEconomics`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:73). The combined [`SessionCompressionHeuristic`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:82) exposes `ccr_bytes_retrieved` but omits the retrieval count. The admin projection similarly exposes offloads, stored bytes, and retrieved bytes through [`CompressionHeuristicView`](<temp>/symforge-token-shakedown-a10ff102/src/server/admin/api_v1.rs:40).

No durable STEL ledger field is updated by this path, and `symforge_retrieve` itself does not call the normal saved-token or `record_tool_completion` accounting helpers.