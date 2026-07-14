The CCR path applies to five discovery tools. It stores the complete formatted payload before token-budget truncation and exposes a session-local retrieval hash.

## 1. Eligible tools and default budgets

`TOOL_OUTPUT_PROFILES` is the authoritative table; a caller-supplied `max_tokens` overrides these defaults via `resolve_tool_max_tokens`. [ccr.rs — `TOOL_OUTPUT_PROFILES`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:23) [ccr.rs — `resolve_tool_max_tokens`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:58)

| Tool | Default budget | Handler behavior |
|---|---:|---|
| `search_text` | 8,000 tokens | All successfully rendered search variants pass through `apply_ccr_budget`. [tools.rs — `search_text`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5111) |
| `search_symbols` | 8,000 tokens | Final rendered output passes through CCR. [tools.rs — `search_symbols`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4942) |
| `find_references` | 8,000 tokens | Successful reference results pass through CCR. [tools.rs — `find_references`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8363) |
| `explore` | 12,000 tokens | Final ranked exploration output passes through CCR. [tools.rs — `explore`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:9594) |
| `get_repo_map` | 16,000 tokens | CCR is used only for `detail="full"`; compact/tree modes use ordinary truncation and do not get the profile default or a continuation. [tools.rs — `get_repo_map`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4293) |

Estimate, validation-error, refusal, and similar early-return paths do not reach CCR.

## 2. Complete result versus stored result

`SymForgeServer::apply_ccr_budget` resolves the explicit/default budget and sends eligible outputs to `enforce_token_budget_with_ccr`. [mod.rs — `apply_ccr_budget`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:692)

The decision is byte-based, using approximately four bytes per token:

- If `max_tokens` is zero/non-positive, budgeting is disabled and the complete payload is returned.
- If the payload is at most `max_tokens × 4` bytes, it is returned unchanged with no CCR footer.
- If oversized, `enforce_token_budget` creates a line-boundary-truncated summary with a `[truncated]` notice. [format.rs — `enforce_token_budget_flagged`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4793)
- If that summary is smaller than the original, the original is stored and the summary plus continuation footer is returned.
- Defensive edge case: if truncation saved no bytes (`summary.len() >= full.len()`), the summary is returned without storing or exposing a handle.

See [ccr.rs — `enforce_token_budget_with_ccr`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:228) and [ccr.rs — `apply_ccr_overflow`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194).

“Complete” here means the complete formatted payload that reached CCR. Upstream ranking, result limits, compaction, or `get_repo_map.max_files` may already have constrained it.

## 3. Continuation exposure

An oversized stored result receives this footer:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="<12-hex-handle>"
```

The footer is constructed by `apply_ccr_overflow`. [ccr.rs](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:212)

Recovery is exposed as the read-only MCP tool `symforge_retrieve`, whose input contains a single `hash: String` documented as a 12-character hex handle. [read_tools.rs — `SymforgeRetrieveInput`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:394) [tools.rs — `symforge_retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10788)

## 4. Identifier validation and retrieval

`mint_handle` hashes the tool name and complete formatted output with Rust’s `DefaultHasher`, retains the low 48 bits, and formats them as 12 lowercase hexadecimal characters. [ccr.rs — `mint_handle`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186)

On retrieval:

1. Input is trimmed and lowercased.
2. It must be exactly 12 ASCII hexadecimal characters; otherwise the handler returns `invalid hash`.
3. The normalized handle is looked up in the session’s `CcrStore`.
4. A hit returns a clone of the complete stored output.
5. A miss returns `unknown or expired hash`.

[tools.rs — `symforge_retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792) [ccr.rs — `CcrStore::retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:158)

The store is in-memory and per session, capped at 32 MiB and 256 entries; capacity pressure evicts the oldest blob. Re-inserting identical content refreshes its age without duplicating it. Thus handles can become unavailable after eviction or session/process loss. [ccr.rs — `CcrStore::new`/`insert`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:110) [mod.rs — `SymForgeServer::ccr_store`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:178)

## 5. Retrieval accounting

Only a successful lookup changes CCR usage accounting:

- `retrieves` increases by one.
- `bytes_retrieved` increases by the full stored payload’s byte length.
- Both use saturating addition.
- Invalid and unknown/expired handles change neither counter.
- Retrieval does not remove the blob or alter `offloads`/`bytes_stored`.

[ccr.rs — `CcrEconomics`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:71) [ccr.rs — `CcrStore::retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:159)

These counters feed `compression_economics`; `context_inventory` displays offloads, stored bytes, and retrieved bytes. The internal retrieval count controls whether the section is shown, but is not itself printed there. [mod.rs — `compression_economics`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:715) [session.rs — `format_context_inventory`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:564)

The retrieval handler does not write a normal STEL ledger event or call the regular discovery-tool savings/completion accounting; its explicit accounting is confined to these CCR counters.

No files were changed, and no builds or tests were run.