# Feature Specification: CCR Output Compression

**Feature Branch**: `011-ccr-output-compression`

**Created**: 2026-06-18

**Status**: Draft

**Input**: User description: "Adopt Headroom-inspired reversible tool output compression for SymForge MCP: extend STEL session cache hits to full tool surface, CCR-lite blob store with retrieve, ranked search compaction with error preservation, per-tool output profiles, and session dedup hints — without LLM proxy merge, preserving byte-exact edit paths and symbol-aware verbosity."

## User Scenarios & Testing *(mandatory)*

The "users" of SymForge are AI coding agents (and operators driving them)
attached to the MCP server. Each story is a slice of agent experience that can
ship and be demonstrated on its own.

### User Story 1 - Agent avoids re-serving content already in session (Priority: P1)

An agent requests file or symbol content it already loaded earlier in the same
MCP session (for example `get_file_context`, `get_symbol`, or `get_file_content`
with the same path and equivalent parameters). Instead of receiving the full
payload again, the agent receives a short cache-hit response that points it to
the content already in context, with an explicit way to force a fresh fetch when
needed.

**Why this priority**: Highest token savings per line of code. STEL compact
facade already does this for two intents; extending the same session memory to
the full read surface removes duplicate megabyte-scale transfers without any new
storage layer.

**Independent Test**: In a fixture session, call `get_file_context` for a path,
then call it again with the same parameters and `force_refresh` unset/false;
assert the second response is a cache-hit pointer under a fixed token budget and
that `force_refresh=true` returns the full body. Repeat for `get_symbol` and
`get_file_content`.

**Acceptance Scenarios**:

1. **Given** a session where `get_file_context(path)` succeeded once,
   **When** the agent calls `get_file_context(path)` again without
   `force_refresh`, **Then** the response is a cache-hit (not a full re-serve)
   and names the prior fetch.
2. **Given** the same session, **When** the agent passes `force_refresh=true`,
   **Then** the full content is served and the session record updates.
3. **Given** a session where only the compact STEL facade loaded a symbol,
   **When** the agent calls the full `get_symbol` for the same symbol,
   **Then** behavior follows the cache contract (hit or fresh serve — documented
   in plan contract; MUST NOT silently return truncated data).
4. **Given** a structural edit or mutation tool, **When** it succeeds,
   **Then** cache-hit logic does NOT block mutation responses (read-path only).

---

### User Story 2 - Agent recovers full output after reversible compression (Priority: P1)

An agent calls a bulk discovery tool (`search_text`, `search_symbols`,
`find_references`, `explore`, or full-detail `get_repo_map`) and the formatted
result would exceed the applicable token budget. Instead of truncating and
discarding the tail, the server returns a ranked summary plus a stable retrieval
handle. The agent can fetch the full original output on demand via a dedicated
retrieve action.

**Why this priority**: Truncation is the weakest link in current output shaping;
agents lose data silently. Reversible compression (Compress-Cache-Retrieve) is the
core Headroom pattern worth porting — adapted for SymForge's symbol-aware
formatting, not generic JSON crushing.

**Independent Test**: Fixture repo with many search matches; call `search_text`
with a low `max_tokens`; assert response contains a retrieval handle, omitted
match count, and that `symforge_retrieve` (or contracted equivalent) returns
byte-identical full formatted output to what would have been produced without
budget cap.

**Acceptance Scenarios**:

1. **Given** a search that would exceed `max_tokens`, **When** the tool
   completes, **Then** the response includes top-ranked matches, an explicit
   omitted count, and a retrieval handle — not a mid-line truncation.
2. **Given** a valid retrieval handle from step 1, **When** the agent retrieves,
   **Then** the full formatted output is returned intact.
3. **Given** an expired or unknown handle, **When** the agent retrieves,
   **Then** a clear error is returned (no partial/garbage payload).
4. **Given** a read used for structural editing (`get_file_content` on an edit
   path, symbol body for mutation), **When** budget would be exceeded,
   **Then** CCR MUST NOT replace byte-exact content (verbosity/section modes
   apply instead; see FR-012).

---

### User Story 3 - Agent gets tighter search results with critical lines kept (Priority: P2)

An agent runs `search_text` or similar log-heavy discovery. Results are grouped
by file, ranked by relevance (query match, enclosing symbol, error/fatal
keywords), and capped with explicit disclosure. Lines matching error severity
patterns are never dropped solely due to ranking cap.

**Why this priority**: Direct port of Headroom's search-compressor idea, but
SymForge can rank using symbol context — strictly better signal than keyword-only
scoring. Ships value even before full CCR store is wired everywhere.

**Independent Test**: Fixture with 50 low-relevance matches and 2 lines containing
`ERROR`; cap to 10 lines; assert both error lines appear in output and footer
discloses truncation.

**Acceptance Scenarios**:

1. **Given** many matches across files, **When** `search_text` runs with default
   caps, **Then** matches are grouped by file and ordered by relevance within
   file.
2. **Given** matches include error-severity lines, **When** output is capped,
   **Then** error-severity lines are preserved per the tool profile contract.
3. **Given** any capped search response, **When** returned, **Then** trust
   metadata states ranking/cap applied (Principle III).

---

### User Story 4 - Agent sees dedup hints on large reads (Priority: P2)

When a large read response is served, a compact footer tells the agent if the
same file or symbol body was already fetched in this session (approximate age and
prior size). The agent can skip redundant follow-up calls without guessing.

**Why this priority**: Low cost; uses existing session memory; complements US1
cache-hit short-circuit.

**Independent Test**: Fetch `get_file_content` twice with `force_refresh=true`;
assert second response includes a dedup hint footer naming the prior fetch.

**Acceptance Scenarios**:

1. **Given** a prior successful fetch of the same target in-session,
   **When** a full fetch is forced again, **Then** a dedup hint footer is
   appended (cache-hit short-circuit from US1 is a separate path).
2. **Given** first fetch in session, **When** content is served,
   **Then** no dedup hint footer (nothing to dedupe against).

---

### User Story 5 - Operator sees compression economics (Priority: P3)

An operator inspecting session or admin economics sees per-tool served tokens,
cache-hit skips, and CCR offload counts — extending existing STEL ledger
semantics without claiming unmeasured savings.

**Why this priority**: Visibility for tuning; depends on US1–US2 instrumentation.
P3 because agents get value without admin UI.

**Independent Test**: Run a scripted session (search + cache hit + retrieve);
query admin summary or economics envelope; assert non-zero cache_hit and
ccr_offload counters where applicable, with heuristic labels per 010 contract.

**Acceptance Scenarios**:

1. **Given** a session with at least one cache hit, **When** economics are
   queried, **Then** cache_hit is recorded with heuristic token estimates
   labeled as such.
2. **Given** a CCR offload, **When** economics are queried, **Then** served vs
   stored bytes are distinguishable in ledger fields.

---

### Edge Cases

- Session restart: retrieval handles and blob store entries MUST NOT cross
  process boundaries without explicit persistence contract; stale handles fail
  clearly.
- Concurrent sessions (serve mode): blob store and session context MUST be
  scoped per MCP session, not global process state.
- `max_tokens` omitted on noisy tools: sensible defaults apply (documented in
  contract) so unbounded-then-truncate does not occur.
- Empty search results: no CCR handle; normal empty response.
- Idempotent mutation replay: CCR blobs MUST NOT alter mutation idempotency
  records.
- Discovery tools MUST NOT bump frecency when ranking or compressing (Principle
  V).
- Embed build: CCR store and session extensions MUST compile without server-only
  network deps (Principle VI).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST short-circuit repeat read requests in the same MCP
  session when `force_refresh` is not true, for `get_file_context`, `get_symbol`,
  and `get_file_content`, returning a cache-hit response per US1 contract.
- **FR-002**: System MUST honor `force_refresh=true` on read tools, bypassing
  cache-hit and updating session records.
- **FR-003**: System MUST replace line-boundary truncation on bulk discovery
  tools with reversible CCR offload when output exceeds budget (US2).
- **FR-004**: System MUST provide a retrieve action (new tool or consolidated
  mode) that returns the full stored formatted output for a valid handle.
- **FR-005**: CCR blob store MUST be bounded (max entries and/or max bytes per
  session) with deterministic eviction (oldest or LRU — chosen in plan).
- **FR-006**: Search and log-like discovery output MUST preserve
  error-severity lines under ranking caps (US3).
- **FR-007**: Capped or ranked discovery output MUST include trust/completeness
  metadata (Principle III).
- **FR-008**: Large read responses MUST optionally include session dedup hint
  footers when prior fetch exists (US4).
- **FR-009**: Per-tool output profiles MUST define at minimum: `search_text`,
  `find_references`, `search_symbols`, `explore`, `get_repo_map` (US3/US2).
- **FR-010**: Default `max_tokens` MUST apply on discovery tools when the agent
  omits the parameter (values in contract, not unbounded).
- **FR-011**: Economics/ledger MUST record cache_hit and ccr_offload events
  with heuristic labeling per 010 economics envelope rules (US5).
- **FR-012**: Byte-exact edit paths MUST NOT use CCR opaque replacement;
  structural edits and symbol bodies for mutation use existing verbosity modes
  only.
- **FR-013**: CCR and cache-hit behavior MUST be identical on stdio and serve
  transports (Principle VII).
- **FR-014**: Discovery ranking and compression MUST NOT bump frecency
  (Principle V).
- **FR-015**: No new mandatory external dependency (Headroom is reference only;
  Rust-native implementation).

### Key Entities

- **SessionFetchRecord**: Prior read of a file/symbol/query key; timestamp,
  approximate token size, content kind.
- **CcrBlob**: Full formatted output bytes; content hash; tool name; creation
  time; session scope.
- **CcrHandle**: Short stable reference (hash prefix) pointing to `CcrBlob`.
- **ToolOutputProfile**: Per-tool rules — cap strategy, preserve keywords,
  CCR-eligible flag, default max_tokens.
- **CompressionEvent**: Ledger row kind — cache_hit | ccr_store | ccr_retrieve.

### Out of Scope

- LLM API proxy, provider prefix-cache alignment, Bedrock routing (Headroom
  proxy product).
- ML-based compression (Kompress, embeddings) on code bodies.
- gzip/deflate on MCP JSON transport envelopes.
- Headroom as a runtime dependency.
- Multi-repo `ProjectRouter` (separate feature; CCR store is per-session within
  one index).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Repeat identical read in the same session returns a cache-hit
  response at least 80% smaller than the original full payload (by character
  count) for fixture files ≥2 KB.
- **SC-002**: Bulk search over fixture with 100+ matches: capped response
  includes retrieval handle and `symforge_retrieve` returns output matching
  uncapped formatted result byte-for-byte.
- **SC-003**: Search fixture with embedded error lines: 100% of error-severity
  lines appear in capped output when cap ≥ number of error lines.
- **SC-004**: Agents calling discovery tools without `max_tokens` receive
  bounded output (no silent unbounded payloads > contract default).
- **SC-005**: Existing compression-ratio CI (`get_file_context` ≤50% raw bytes)
  remains green; no regression on structural edit success paths.
- **SC-006**: Full verification gate passes (`cargo fmt`, `check`, `clippy`,
  `test --test-threads=1`, `build --release`).

## Assumptions

- Headroom (`E:\project\headroom`) is competitive research reference only; patterns
  are reimplemented in SymForge Rust, not vendored.
- Session identity is already available on MCP stdio and serve paths via
  existing `SessionContext` / STEL controller wiring.
- Retrieve surface ships as `symforge_retrieve` on full MCP surface and is
  reachable from compact facade via mode param or documented alias (constitution
  tool-consolidation pattern evaluated in plan).
- CCR blobs live under `.symforge/session-blobs/` for serve durability optional
  in v1; in-memory-only acceptable for stdio-only v1 if contract documents
  restart invalidation.
- Default max_tokens for discovery tools: 8_000 estimated tokens (chars/4) unless
  contract revises after benchmark.
