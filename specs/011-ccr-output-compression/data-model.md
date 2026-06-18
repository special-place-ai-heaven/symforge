# Data Model: CCR Output Compression (011)

**Branch**: `011-ccr-output-compression` · **Date**: 2026-06-18

Transient session entities and configuration shapes. Nothing here is persisted to
the LiveIndex or `.symforge/index.bin`.

---

## SessionFetchRecord

Tracks a prior successful read for cache-hit and dedup-hint logic.

| Field | Type | Notes |
|-------|------|-------|
| `kind` | enum | `FileContext` \| `Symbol` \| `FileContent` |
| `path` | string | Repo-relative path |
| `symbol` | optional string | For symbol-scoped reads |
| `params_hash` | u64 | Canonical hash of verbosity, compact, line range, etc. |
| `fetched_at` | Instant | Monotonic session time |
| `approx_tokens` | u64 | chars/4 estimate at serve time |

**Key**: `(kind, path, symbol, params_hash)`

**Lifecycle**: Insert/update on successful full serve; consulted before format on
repeat read; ignored when `force_refresh=true` (then dedup hint may apply).

---

## CcrBlob

Stored formatted output for reversible compression.

| Field | Type | Notes |
|-------|------|-------|
| `handle` | string | 12-char hex BLAKE3 prefix |
| `tool_name` | string | Originating tool |
| `formatted_bytes` | Vec<u8> | Full output that would have been returned |
| `created_at` | Instant | For eviction ordering |
| `byte_len` | usize | Denormalized for budget accounting |

**Invariants**:
- Immutable after insert.
- `formatted_bytes` is UTF-8 valid (tool output invariant).
- Not used for edit/mutation payloads.

---

## CcrHandle

Opaque reference embedded in compressed tool output.

| Field | Type | Notes |
|-------|------|-------|
| `hash` | string | Same as `CcrBlob.handle` |

**Footer grammar** (human + machine parseable):

```text
---
CCR: {omitted_count} items omitted · full output {byte_len} bytes · retrieve: symforge_retrieve hash="{hash}"
```

---

## CcrStore

Per-session container.

| Field | Type | Default |
|-------|------|---------|
| `blobs` | Map<Handle, CcrBlob> | empty |
| `total_bytes` | usize | 0 |
| `max_bytes` | usize | 33_554_432 (32 MiB) |
| `max_entries` | usize | 256 |

**Eviction**: When insert would exceed limits, drop oldest `created_at` until
within budget (deterministic LRU-by-time).

**Scope**: One store per MCP session / stdio server session.

---

## ToolOutputProfile

Static per-tool configuration.

| Field | Type | Notes |
|-------|------|-------|
| `tool_name` | &str | MCP tool name |
| `ccr_eligible` | bool | If false, verbosity/truncation only |
| `default_max_tokens` | u64 | When agent omits `max_tokens` |
| `preserve_error_lines` | bool | Search-style error regex bypass |
| `rank_group_by_file` | bool | Search compaction |

Defined as `const PROFILES` — see [tool-output-profiles.md](./contracts/tool-output-profiles.md).

---

## StelCacheBody (reused)

Existing STEL type for cache-hit responses. Extended use for full read tools:

| Field | Existing | Notes |
|-------|----------|-------|
| `kind` | yes | `file` \| `symbol` |
| `path` | yes | |
| `name` | yes | Symbol name if applicable |
| `prior_tokens` | yes | From `SessionFetchRecord` |
| `session_age_secs` | yes | |

No schema change required for US1.

---

## CompressionEvent (ledger)

Optional extensions to STEL ledger row:

| Field | Type | When set |
|-------|------|----------|
| `cache_hit` | bool | US1 short-circuit |
| `ccr_bytes_stored` | optional u64 | US2 store |
| `ccr_bytes_retrieved` | optional u64 | US2 retrieve |

Labeled heuristic in economics envelope per 010 contract.

---

## Relationships

```text
SessionContext
  ├── fetch_records: Map<FetchKey, SessionFetchRecord>
  └── ccr_store: CcrStore

Tool handler
  → check SessionFetchRecord (cache hit?)
  → format output
  → if over budget && profile.ccr_eligible
       → CcrStore.insert(formatted) → summary + CcrHandle
  → else enforce verbosity / line budget

symforge_retrieve(hash)
  → CcrStore.get(hash) → formatted_bytes or error
```

---

## Validation Rules

- Handle MUST match `[0-9a-f]{12}`.
- Retrieve with unknown/expired handle → deterministic error string, no partial body.
- `ccr_eligible=false` tools MUST NOT mint handles.
- Discovery tools MUST NOT write frecency as side effect of store/insert.
