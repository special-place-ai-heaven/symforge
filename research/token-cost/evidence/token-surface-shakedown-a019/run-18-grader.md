Oversized discovery output uses a per-tool budget, approximating one token as four bytes. When the formatted result exceeds that budget, SymForge returns a line-bounded summary, stores the complete pre-token-truncation string in an in-memory CCR store, and exposes a 12-hex-character retrieval handle.

## 1. Eligible tools and default budgets

The authoritative table is [`TOOL_OUTPUT_PROFILES`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:23); [`resolve_tool_max_tokens`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:58) gives an explicit caller value precedence over the default.

| Tool | Default | Scope |
|---|---:|---|
| `search_text` | 8,000 tokens | All normal and structural-search output paths |
| `search_symbols` | 8,000 tokens | All returned symbol-search output |
| `find_references` | 8,000 tokens | Successful reference results |
| `explore` | 12,000 tokens | Exploration output |
| `get_repo_map` | 16,000 tokens | Only `detail="full"` |

The handler call sites are [`search_text`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5170), [`search_symbols`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5089), [`find_references`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8367), [`explore`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:9603), and [`get_repo_map`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4293). Compact and tree repo maps retain ordinary lossy token truncation.

No read-content, symbol-body, context, edit, or other discovery tool is CCR-eligible.

## 2. Complete result versus stored result

[`SymForgeServer::apply_ccr_budget`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:692) resolves the budget and sends eligible output through [`enforce_token_budget_with_ccr`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:228).

The decision is:

- `max_tokens=0` disables limiting; the complete string is returned.
- If `result.len() <= max_tokens * 4`, the complete string is returned unchanged.
- Otherwise, [`enforce_token_budget`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4780) cuts at a line boundary and appends its ordinary truncation notice.
- If that summary is genuinely shorter, [`apply_ccr_overflow`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194) stores the original string and returns the summary plus a continuation footer.
- Defensive exception: if truncation saved nothing—`summary.len() >= full.len()`—the summary is returned without storing a continuation.

“Complete” here means the complete formatted output entering token enforcement. Earlier tool-specific ranking, result limits, and search compaction have already happened; CCR does not recover matches discarded before formatting.

## 3. Continuation exposure

The originating response receives:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="<12 hex chars>"
```

That exact footer is constructed by [`apply_ccr_overflow`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:215). Recovery is a separate read-only MCP tool, [`symforge_retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10788), whose input schema is [`SymforgeRetrieveInput`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:394).

It is available on the default full surface. The opt-in compact-3 surface centrally rejects tools outside its three names, so it does not expose `symforge_retrieve`; see [`ServerHandler::call_tool`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:1303).

## 4. Identifier validation and retrieval

[`mint_handle`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186) hashes the tool name and complete formatted string using Rust’s `DefaultHasher`, masks it to 48 bits, and formats 12 lowercase hexadecimal characters.

On retrieval, the handler:

1. Trims whitespace and lowercases the supplied hash.
2. Requires exactly 12 ASCII hexadecimal characters.
3. Calls [`CcrStore::retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:159).
4. Returns an exact clone of the stored string, or `unknown or expired hash` if absent.

The store is attached to [`SymForgeServer::ccr_store`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:179), is memory-only, and is limited to 32 MiB/256 entries with oldest-first eviction in [`CcrStore::new` and `evict_oldest`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:110).

Notable implementation details:

- Despite the planning contract mentioning BLAKE3, the shipped symbol uses `DefaultHasher`.
- Uppercase hashes and surrounding whitespace are accepted because normalization precedes validation.
- The retrieval handler does not proxy to the daemon, although discovery handlers can return daemon-generated CCR handles. That creates a source-level store-affinity risk in daemon-proxy topology: retrieval consults the proxy server’s local store.

## 5. Usage accounting on retrieval

A successful `CcrStore::retrieve`:

- increments `CcrEconomics.retrieves` by one;
- adds the complete body’s byte length to `bytes_retrieved`;
- uses saturating arithmetic.

Invalid or missing handles change neither counter. The counter definitions and mutation are in [`CcrEconomics` and `CcrStore::retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:71).

[`format_context_inventory`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:564) exposes `ccr_offloads`, `ccr_bytes_stored`, and `ccr_bytes_retrieved`; it uses `retrieves` to decide whether to show the section but does not print the retrieval count itself.

Retrieval does **not** call `SessionContext::record_summary_output`, so the session’s `total_tokens` does not increase when the recovered body is served. It also has no direct `record_tool_completion` analytics call. Conversely, eligible origin handlers record their pre-CCR formatted output before applying the budget—for example [`search_symbols`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5076)—so session-token accounting reflects the full origin string, not strictly the compressed bytes initially returned.