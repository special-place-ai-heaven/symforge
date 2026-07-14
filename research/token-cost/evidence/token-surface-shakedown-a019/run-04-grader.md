Oversized discovery output uses CCR (“Compress-Cache-Retrieve”): the tool returns a budgeted summary and keeps the complete pre-budget formatted result in a per-session memory store.

## 1. Eligible tools and default budgets

Defined by `TOOL_OUTPUT_PROFILES`:

| Tool | Default `max_tokens` | Handler |
|---|---:|---|
| `search_text` | 8,000 | `search_text` |
| `search_symbols` | 8,000 | `search_symbols` |
| `find_references` | 8,000 | `find_references` |
| `explore` | 12,000 | `explore` |
| `get_repo_map` | 16,000 | `get_repo_map`, but CCR is applied only when `detail == "full"` |

Profile definitions: [`TOOL_OUTPUT_PROFILES`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:23). Handler wiring: [`get_repo_map`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4298), [`search_symbols`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5089), [`search_text`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5170), [`find_references`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8367), [`explore`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:9603).

A supplied `max_tokens` overrides the default. Budgeting approximates one token as four UTF-8 bytes. An explicit zero bypasses limiting because non-positive budgets are filtered out. See [`resolve_tool_max_tokens`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:58) and [`enforce_token_budget_with_ccr`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:228).

## 2. Complete return versus storage

`SymForgeServer::apply_ccr_budget` resolves the effective budget and sends eligible tools through the common CCR gate. Non-eligible tools receive ordinary irreversible budget truncation. See [`apply_ccr_budget`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:692).

For an eligible tool:

- If no positive budget exists, or the result is at most `max_tokens * 4` bytes, the complete formatted result is returned unchanged.
- If it exceeds that threshold, `format::enforce_token_budget` creates the visible summary from a clone, while the original complete string remains available for storage.
- If that summary is genuinely smaller, the complete original is stored and the summary is returned with a CCR footer.
- Defensive edge case: if the generated summary is not smaller than the original, the summary is returned without storing a continuation.

See [`enforce_token_budget_with_ccr`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:219) and [`apply_ccr_overflow`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:193).

“Complete” here means the tool’s complete pre-CCR formatted/ranked result. It does not recover matches already excluded by the tool’s independent result limits or filters.

## 3. How continuation is exposed

The truncated response appends a textual MCP-native continuation:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="<12-hex-handle>"
```

The recovery tool accepts:

```json
{"hash": "<12-hex-handle>"}
```

See the footer construction in [`apply_ccr_overflow`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:212) and the input schema [`SymforgeRetrieveInput`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:394).

This is an opaque full-result handle, not a page cursor.

## 4. Identifier validation and retrieval

`mint_handle` hashes the originating tool name and complete formatted output with Rust’s `DefaultHasher`, masks it to 48 bits, and formats it as exactly 12 lowercase hexadecimal characters. See [`mint_handle`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186).

On retrieval, `symforge_retrieve`:

1. Trims surrounding whitespace.
2. Lowercases the identifier.
3. Requires exactly 12 ASCII hexadecimal characters.
4. Performs an O(1) lookup in the current server/session’s `CcrStore`.
5. Returns the stored string verbatim, without reformatting or reranking.
6. Returns distinct errors for malformed and missing/expired handles.

See [`symforge_retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792) and [`CcrStore::retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:158).

The store is per session, memory-only, and bounded to 32 MiB or 256 entries; overflow evicts the oldest blob. Therefore a handle can become unknown after eviction or session/process replacement. See [`CcrStore::new`, `insert`, and `evict_oldest`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:110) and the server-owned [`ccr_store`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:178).

## 5. Usage accounting on retrieval

A successful `CcrStore::retrieve`:

- increments `CcrEconomics.retrieves` by one;
- adds the complete stored string’s byte length to `bytes_retrieved`;
- leaves the blob stored, so repeated successful retrievals count again.

Malformed or unknown/expired handles change neither counter. See [`CcrEconomics`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:71) and [`CcrStore::retrieve`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:158).

These are heuristic, in-memory CCR economics. `context_inventory` exposes `ccr_offloads`, `ccr_bytes_stored`, and `ccr_bytes_retrieved`; it uses `retrieves` only to decide whether to display the economics section. See [`format_context_inventory`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:564). Retrieval does not call `SessionContext::record_summary_output`, so it does not add the recovered body to the session’s ordinary `total_tokens` accounting; only the CCR retrieval counters change. [`record_summary_output`](<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:167) is called by the originating discovery handlers before CCR budgeting.