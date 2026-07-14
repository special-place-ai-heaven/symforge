Oversized discovery output follows this path:

`tool handler → apply_ccr_budget → line-bounded summary + store full formatted result → footer hash → symforge_retrieve → per-session store lookup`

### 1. Eligible tools and default budgets

The authoritative `TOOL_OUTPUT_PROFILES` table defines five eligible tools. A caller-supplied `max_tokens` overrides the default. [`TOOL_OUTPUT_PROFILES`, `resolve_tool_max_tokens`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:23>)

| Tool | Default budget | CCR-routed scope |
|---|---:|---|
| `search_text` | 8,000 tokens | Successful normal and structural searches. [`search_text`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5111>) |
| `search_symbols` | 8,000 tokens | Successful formatted search results. [`search_symbols`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4942>) |
| `find_references` | 8,000 tokens | `mode="references"` results. `mode="implementations"` returns before CCR processing. [`find_references`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8088>) |
| `explore` | 12,000 tokens | Successful exploration output. [`explore`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8896>) |
| `get_repo_map` | 16,000 tokens | Only when the resolved detail is `"full"`; compact/tree output uses ordinary truncation. [`get_repo_map`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4135>) |

For `get_repo_map`, omitted detail normally means compact. Supplying `max_tokens` without detail may adaptively select full mode when its estimate fits.

### 2. Complete result versus stored result

`apply_ccr_budget` resolves the override/default and routes eligible tools through `enforce_token_budget_with_ccr`. [`SymForgeServer::apply_ccr_budget`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:692>)

The decision is byte-based, using an approximation of four bytes per token:

- No positive budget— including explicit `max_tokens=0`—returns the result unchanged.
- If `result.len() <= max_tokens × 4`, the complete result is returned unchanged; nothing is stored and no continuation footer appears.
- If oversized, the output is truncated at a line boundary and receives the normal token-budget notice. [`enforce_token_budget_flagged`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4793>)
- The full pre-budget-cut formatted string is stored when that summary is actually shorter than the full output. A defensive fallback returns only the summary when it saved nothing. [`enforce_token_budget_with_ccr`, `apply_ccr_overflow`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194>)

“Full” here means the complete formatted result after each tool’s normal ranking, compaction, and result-count caps—not every possible repository match.

### 3. Continuation exposure

An oversized response appends:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="<12-hex-handle>"
```

This is produced by `apply_ccr_overflow`. The public continuation tool accepts a single `hash` string documented as a 12-character hexadecimal handle. [`apply_ccr_overflow`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194>), [`SymforgeRetrieveInput`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:396>)

### 4. Identifier validation and retrieval

`mint_handle` hashes the tool name plus complete formatted output, masks the result to 48 bits, and renders exactly 12 lowercase hexadecimal characters. [`mint_handle`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186>)

On retrieval, `symforge_retrieve`:

1. Trims and lowercases the supplied value.
2. Requires exactly 12 ASCII hexadecimal characters.
3. Looks it up in the current server session’s `CcrStore`.
4. Returns a cloned full formatted string, or `unknown or expired hash` if absent. [`SymForgeServer::symforge_retrieve`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792>)

The store is explicitly per session. It is bounded to 32 MiB and 256 entries, evicting the oldest entry when necessary; there is no time-based TTL. Restart, session replacement, or capacity eviction can therefore invalidate a handle. [`SymForgeServer::ccr_store`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:181>), [`CcrStore::new`, `insert`, `evict_oldest`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:111>)

### 5. Usage accounting on retrieval

A successful retrieval:

- increments `CcrEconomics.retrieves` by one;
- adds the full stored byte length to `bytes_retrieved`;
- counts again on every successful repeated retrieval.

Invalid or unknown handles change neither counter because lookup fails before accounting. [`CcrStore::retrieve`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:159>)

Retrieval does not update session-context token totals, fetched/listed entries, frecency, or the STEL ledger—the handler only accesses `ccr_store`. `context_inventory` exposes `ccr_bytes_retrieved` and uses the internal retrieval count to decide whether to show compression economics, but does not print the retrieval count itself. [`format_context_inventory`](</<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:468>)