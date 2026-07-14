CCR is a post-formatting, reversible byte cap for five local discovery paths. It uses an approximate four bytes per token, stores the pre-token-truncation formatted payload in session memory, and returns a line-bounded summary with a retrieval handle.

## 1. Eligible tools and defaults

The authoritative profile is [`TOOL_OUTPUT_PROFILES`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:23). A caller-supplied `max_tokens` overrides these defaults via [`resolve_tool_max_tokens`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:58).

| Tool | Default budget | Effective scope |
|---|---:|---|
| `search_text` | 8,000 tokens | Successful locally formatted search results; its result branches call CCR at [`search_text`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:5111). |
| `search_symbols` | 8,000 tokens | Final local result at [`search_symbols`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4942). |
| `find_references` | 8,000 tokens | Successful normal reference mode at [`find_references`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:8088). `mode="implementations"` returns early and bypasses CCR. |
| `explore` | 12,000 tokens | Final local exploration output at [`explore` CCR call](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:9603). |
| `get_repo_map` | 16,000 tokens | Only `detail="full"`; compact/tree modes use ordinary truncation at [`get_repo_map`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:4135). |

[`SymForgeServer::apply_ccr_budget`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/mod.rs:692) resolves the override/default, checks eligibility, locks the store, and delegates to CCR. Unprofiled outputs receive ordinary irreversible token-budget truncation.

Cross-project daemon aggregation is separate: [`apply_cross_project_token_budget`](/<temp>/symforge-token-shakedown-a10ff102/src/daemon.rs:3828) performs disclosed line truncation without CCR recovery.

## 2. Complete result versus stored result

[`enforce_token_budget_with_ccr`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:228) makes the decision:

1. `max_tokens=None` or `max_tokens=0` returns the complete formatted result unchanged. For profiled tools, omission normally resolves to the profile default; explicitly passing zero therefore disables the cap.
2. If the result is at most `max_tokens × 4` bytes, it is returned unchanged, with no storage or continuation footer.
3. If oversized, [`enforce_token_budget_flagged`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/format.rs:4793) cuts at a line boundary and appends a truncation notice.
4. [`apply_ccr_overflow`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194) stores the original, complete formatted string and returns the truncated summary plus continuation footer. Its defensive exception is when the summary is no smaller than the original; then it returns the summary without storing it.

“Complete” here means complete before the token cut, not an unlimited query. Upstream result caps have already applied—for example, full `get_repo_map` defaults to at most 200 files before CCR.

## 3. Continuation exposure

The originating result gets this footer:

```text
---
CCR: full ranked output stored · retrieve: symforge_retrieve with hash="{handle}"
```

This is emitted by [`apply_ccr_overflow`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:194). The continuation is exposed as the read-only `symforge_retrieve` MCP tool declared at [`SymForgeServer::symforge_retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10789). It is available on the full surface, not compact-3.

## 4. Identifier validation and retrieval

- [`mint_handle`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:186) hashes the originating tool name plus the complete formatted string with Rust’s `DefaultHasher`, keeps the low 48 bits, and renders 12 lowercase hexadecimal characters. Despite older design documentation mentioning BLAKE3, the current implementation does not use BLAKE3 here.
- The public input is simply `SymforgeRetrieveInput { hash: String }` at [`read_tools.rs`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/read_tools.rs:396).
- [`symforge_retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/tools.rs:10792) trims and lowercases the value, then requires exactly 12 ASCII hexadecimal characters. Thus uppercase input is accepted and normalized.
- It performs an O(1) lookup in the session’s `HashMap`. A miss returns `CCR retrieve: unknown or expired hash '…'`; invalid syntax returns the fixed “expected 12 hex chars” error.
- Success clones and returns the stored formatted string without reformatting, reranking, removal, tool-name checking, or content revalidation.
- [`CcrStore`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:110) is in-memory, bounded to 32 MiB and 256 entries, evicting the oldest entry first. Consequently identifiers expire on eviction, server/session loss, or restart. Identical insertions refresh the entry’s age without double-counting storage.

## 5. Retrieval accounting

On every successful retrieval, [`CcrStore::retrieve`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:159):

- increments `retrieves` by one;
- increments `bytes_retrieved` by the complete stored string’s byte length;
- uses saturating arithmetic.

Repeated retrieval of the same handle counts repeatedly. Invalid and unknown handles change nothing because lookup failure occurs before either increment. Retrieval does not remove or refresh the blob.

The accounting remains separate from ordinary session token inventory:

- [`CcrEconomics`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/ccr.rs:73) holds in-memory offload, stored-byte, retrieve, and retrieved-byte counters.
- [`format_context_inventory`](/<temp>/symforge-token-shakedown-a10ff102/src/protocol/session.rs:468) exposes `ccr_offloads`, `ccr_bytes_stored`, and `ccr_bytes_retrieved`; it does not print the retrieve count.
- The retrieval handler does not record the returned body in `SessionContext.total_tokens` or call the normal per-tool completion recorder. Thus retrieval increases CCR byte economics, but not ordinary fetched/summary token accounting.

No files were changed, and no builds or tests were run.