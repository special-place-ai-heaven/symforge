# Phase 0 Research: CCR Output Compression (011)

**Date**: 2026-06-18 · **Branch**: `011-ccr-output-compression`

Grounds the Headroom competitive analysis against SymForge's shipped output
pipeline. Headroom clone at `E:\project\headroom` is reference only.

---

## R0. What Headroom actually ships (do not merge)

- **Decision**: Borrow **CCR pattern**, **search compressor ranking**, and **MCP
  tool profiles** — reject proxy server, provider routing, ML Kompress, Magika,
  Python runtime.
- **Rationale**: Headroom `headroom/proxy/server.py` and `headroom/ccr/` target
  LLM API spend; SymForge targets MCP tool output to agents. Different layer.
- **Alternatives rejected**: Vendor `headroom-core` crate (rejected — extra dep,
  JSON-first, no symbol context). Run Headroom upstream of SymForge (rejected —
  composable for operators, not core).

## R1. Current SymForge compression baseline

- **Decision**: Extend existing machinery; do not replace `apply_verbosity` /
  `adaptive_verbosity` (`src/protocol/format.rs`).
- **Evidence**:
  - `enforce_token_budget` truncates at line boundaries — lossy (weak link per
    spec FR-003).
  - STEL `CacheHit` + `format_cache_hit_body` (`src/stel/executor.rs:109-137`)
    works for compact facade only via `detect_session_cache_hit`
    (`src/stel/controller.rs:188`).
  - `SessionContext` (`src/protocol/session.rs`) tracks fetched paths/symbols.
  - `compact_savings_footer` + economics envelope (`010` contracts) already
    label heuristic token estimates.
- **Alternatives rejected**: gzip MCP responses (rejected — agents can't use).
  Replace verbosity with SmartCrusher on code (rejected — loses structure).

## R2. Session cache hit — full read surface (US1)

- **Decision**: Mirror STEL cache-hit for standalone read tools:
  1. On successful read, `SessionContext::record_fetch(kind, path, symbol, approx_tokens)`.
  2. Before format, if `!force_refresh` and matching record exists → return
     cache-hit body (reuse `StelCacheBody` shape or slim `SessionCacheHit` —
     same formatter).
  3. `force_refresh` bypasses and refreshes record.
- **Wiring**: `get_file_context`, `get_symbol`, `get_file_content` in
  `src/protocol/tools.rs` — check session **before** expensive format when
  params match canonical key (path + symbol + verbosity/compact flags).
- **Compact vs full**: If only compact STEL fetch exists and agent requests
  full `get_symbol`, serve fresh full body (do not upgrade silently from
  compact cache) — document in `contracts/session-cache-hit.md`.
- **Alternatives rejected**: Global process cache (rejected — cross-session
  bleed). Cache-hit on mutations (rejected — FR-012).

## R3. CCR-lite store design (US2)

- **Decision**: New `src/protocol/ccr.rs`:
  - `CcrStore` per session: `HashMap<Handle, CcrBlob>` + byte budget (default
    32 MiB/session, 256 blobs max).
  - Handle = first 12 hex chars of `blake3(formatted_bytes || tool_name ||
    session_salt)` — matches Headroom opaque ref length, deterministic.
  - Overflow path: `format_with_ccr(output, budget, store) -> (summary, Option<Handle>)`.
  - Summary = ranked head + footer:
    `---\nCCR: N items omitted · retrieve with symforge_retrieve(hash=...) ---`
- **Retrieve tool**: `symforge_retrieve` with `{ "hash": "..." }` — new
  `#[tool]` on full surface; daemon alias optional; **not** on compact-3 default
  surface (agents get handle in discovery output; retrieve is opt-in full surface
  or `SYMFORGE_SURFACE=full`).
- **Wire first on**: `search_text`, then `search_symbols`, `find_references`,
  `explore`, `get_repo_map` when `detail=full`.
- **Alternatives rejected**: Extend `inspect_match` only (rejected — wrong
  semantics for bulk search blob). Store raw JSON (rejected — store **formatted**
  string agents would have seen uncapped, for byte-identical replay per SC-002).

## R4. Persistence tier

- **Decision**: **v1 in-memory only** per session; handles invalid on process
  restart. Optional follow-up: spill to `.symforge/session-blobs/<session_id>/`
  on serve (not blocking US2 MVP).
- **Rationale**: ponytail ceiling — ship retrieve path; disk durability is
  upgrade path when serve long-lived sessions need it.
- **Alternatives rejected**: SQLite blob table (rejected — constitution prohibits
  second query store; file spill is fine as opaque bytes).

## R5. Retrieve surface naming (tool consolidation)

- **Decision**: New tool `symforge_retrieve` (not mode on `inspect_match`).
- **Rationale**: `inspect_match` is match-scoped deep-dive; CCR blob is
  whole-tool-output. Consolidation pattern still applies: single-purpose retrieve,
  backward-compat alias in `daemon.rs` if we later rename.
- **Add to**: `SYMFORGE_TOOL_NAMES` in `src/cli/init.rs`; full surface only.

## R6. Search compaction (US3) — Headroom `search_compressor` port

- **Decision**: In `format.rs` search formatters:
  1. Group matches by `file_path`.
  2. Score: exact query match in line (+3), enclosing symbol relevance (+2),
     error regex `(?i)\b(error|fatal|panic|exception|failed)\b` (+5).
  3. Take top-K per file, then top-K files by best-line score.
  4. Preserve all error-scored lines even if over per-file cap (rebalance).
- **Headroom reference**: `headroom/transforms/search_compressor.py` — keyword
  scoring only; SymForge adds symbol context from existing match metadata.
- **Alternatives rejected**: Port full SmartCrusher JSON walker (rejected — search
  output is already structured text).

## R7. Tool output profiles (US3/FR-009)

- **Decision**: `static PROFILES: &[ToolOutputProfile]` in `ccr.rs` or
  `format.rs`:

| Tool | ccr_eligible | default_max_tokens | preserve_errors |
|------|--------------|-------------------|-----------------|
| search_text | true | 8000 | true |
| search_symbols | true | 8000 | false |
| find_references | true | 8000 | false |
| explore | true | 12000 | false |
| get_repo_map | true (full only) | 16000 | false |
| get_file_content | false | resolve_read | false |
| get_symbol | false | adaptive | false |

- **Alternatives rejected**: YAML config file (rejected — YAGNI; const table).

## R8. Dedup hint footer (US4)

- **Decision**: `append_dedup_hint_footer(output, prior: &SessionFetchRecord)` —
  only when `force_refresh=true` and prior exists; ~1 line:
  `[session: same content fetched {age}s ago (~{tokens} est tokens)]`
- **Does not fire** on cache-hit short-circuit path (US1 returns before full body).

## R9. Economics / ledger (US5)

- **Decision**: Extend STEL ledger event with optional `ccr_bytes_stored`,
  `ccr_bytes_served` on store/retrieve; cache_hit already has column. Admin
  `/api/v1/summary` adds `ccr_offloads` counter — P3, after US2 instrumentation.
- **Labeling**: Follow `010` economics envelope — chars/4 = heuristic.

## R10. Frecency + discovery

- **Decision**: No `bump_frecency` in any new path; add
  `search_compaction_does_not_bump_frecency` test mirroring `007` patterns.
- **Evidence**: Ranking signal invariants in `AGENTS.md` — discovery must not
  create commitment signal.

## R11. Transport parity

- **Decision**: `CcrStore` lives on `SymForgeServer` session state (or
  `SessionContext` extension), created per MCP connection on serve and per
  stdio server instance.
- **Evidence**: `ServerRuntime` single index today — session scoping is the
  parity seam, not per-project (multi-repo is out of scope).

## R12. Headroom patterns explicitly deferred

| Pattern | Defer reason |
|---------|--------------|
| ContentRouter ML routing | No ML deps in v1 |
| SharedContext cross-agent | Needs serve multi-session design |
| Proxy session_stats.jsonl | STEL ledger covers economics |
| Tabular JSON SmartCrusher | Search output is text, not JSON arrays |
| tiktoken | chars/4 sufficient for budgets; revisit if SC fails |

---

**Phase 0 status**: all NEEDS CLARIFICATION resolved. Ready for Phase 1 contracts.
