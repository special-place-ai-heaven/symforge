# Tasks: CCR Output Compression

**Input**: Design documents from `/specs/011-ccr-output-compression/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Organization**: By user story — each story independently testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1–US5

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Module skeleton and profile table

- [ ] T001 Create `src/protocol/ccr.rs` with `CcrStore`, `CcrBlob`, `ToolOutputProfile` const table per `contracts/tool-output-profiles.md`
- [ ] T002 [P] Export `ccr` module from `src/protocol/mod.rs`
- [ ] T003 [P] Extend `src/protocol/session.rs` with `SessionFetchRecord`, fetch key hashing, `record_fetch` / `lookup_fetch`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Session store wiring on `SymForgeServer` / tool dispatch context

**⚠️ CRITICAL**: No user story work until CcrStore is reachable from tool handlers

- [ ] T004 Attach `CcrStore` to per-session state (extend `SessionContext` or `SymForgeServer` session bag in `src/protocol/mod.rs` / server wiring)
- [ ] T005 [P] Add `force_refresh: bool` with `#[serde(default)]` to `get_file_context`, `get_symbol`, `get_file_content` input structs in `src/protocol/tools.rs` if missing
- [ ] T006 [P] Add helper `resolve_tool_max_tokens(tool_name, agent_max)` in `src/protocol/format.rs` using profile table

**Checkpoint**: Session + CCR infrastructure ready

---

## Phase 3: User Story 1 — Session cache hit (Priority: P1) 🎯 MVP

**Goal**: Repeat reads short-circuit with cache-hit body

**Independent Test**: `cargo test --test session_cache_hit`

- [ ] T007 [US1] Implement `check_session_cache_hit` in `src/protocol/session.rs` returning `StelCacheBody`
- [ ] T008 [US1] Wire cache-hit check at top of `get_file_context`, `get_symbol`, `get_file_content` handlers in `src/protocol/tools.rs`
- [ ] T009 [US1] Reuse `format_cache_hit_body_from` from `src/stel/executor.rs` (extract to shared `format.rs` if needed to avoid stel→protocol cycle)
- [ ] T010 [US1] Record fetch on successful full serve in each read handler
- [ ] T011 [US1] Implement compact-STEL→full-tool miss rule per `contracts/session-cache-hit.md`
- [ ] T012 [US1] Ledger `cache_hit` flag on short-circuit in `src/stel/controller.rs` when economics path active
- [ ] T013 [P] [US1] Create `tests/session_cache_hit.rs` covering hit, miss, force_refresh, compact→full miss

**Checkpoint**: US1 complete — SC-001

---

## Phase 4: User Story 2 — CCR retrieve (Priority: P1)

**Goal**: Reversible overflow instead of truncation on bulk discovery

**Independent Test**: `cargo test --test ccr_retrieve`

- [ ] T014 [US2] Implement `CcrStore::insert` + deterministic handle mint (BLAKE3) in `src/protocol/ccr.rs`
- [ ] T015 [US2] Implement `format_with_ccr_overflow(summary, full, store)` in `src/protocol/format.rs`
- [ ] T016 [US2] Add `symforge_retrieve` tool handler in `src/protocol/tools.rs` per `contracts/ccr-retrieve.md`
- [ ] T017 [US2] Register `symforge_retrieve` in `SYMFORGE_TOOL_NAMES` (`src/cli/init.rs`) and daemon alias if needed (`src/daemon.rs`)
- [ ] T018 [US2] Replace `enforce_token_budget` truncation with CCR path on `search_text` when profile.ccr_eligible
- [ ] T019 [P] [US2] Wire CCR overflow on `search_symbols`, `find_references`, `explore` in `src/protocol/tools.rs`
- [ ] T020 [P] [US2] Wire CCR on `get_repo_map` when `detail=full` only
- [ ] T021 [US2] Apply default `max_tokens` from profile when agent omits param (FR-010)
- [ ] T022 [US2] Set result-status trust flags on CCR responses (`src/protocol/result_status.rs`)
- [ ] T023 [P] [US2] Create `tests/ccr_retrieve.rs` — round-trip equality, unknown hash, eviction

**Checkpoint**: US2 complete — SC-002, SC-004

---

## Phase 5: User Story 3 — Search compaction (Priority: P2)

**Goal**: Ranked, error-preserving search output

**Independent Test**: `cargo test --test search_compaction`

- [ ] T024 [US3] Implement `rank_search_matches` + group-by-file in `src/protocol/format.rs` per `contracts/search-compaction.md`
- [ ] T025 [US3] Integrate ranking before CCR/format in `search_text` handler
- [ ] T026 [P] [US3] Share ranking helper with `search_symbols` where applicable
- [ ] T027 [US3] Add ranked/truncated disclosure footer + result-status
- [ ] T028 [P] [US3] Create `tests/search_compaction.rs` — error preservation, grouping
- [ ] T029 [US3] Add `search_compaction_does_not_bump_frecency` test (mirror `tests/frecency_ranking.rs` patterns)

**Checkpoint**: US3 complete — SC-003

---

## Phase 6: User Story 4 — Dedup hint footer (Priority: P2)

**Goal**: Agents see prior fetch on forced refresh

**Independent Test**: covered in `session_cache_hit` or dedicated assert

- [ ] T030 [US4] Implement `append_dedup_hint_footer` in `src/protocol/format.rs` per `contracts/dedup-hint-footer.md`
- [ ] T031 [US4] Call from read handlers when `force_refresh=true` and prior record exists
- [ ] T032 [P] [US4] Add test assertions in `tests/session_cache_hit.rs`

**Checkpoint**: US4 complete

---

## Phase 7: User Story 5 — Compression economics (Priority: P3)

**Goal**: Ledger visibility for cache hit and CCR

**Independent Test**: unit test on ledger insert + optional admin API test

- [ ] T033 [US5] Extend ledger event struct + `ledger_store.rs` with `ccr_bytes_stored`, `ccr_bytes_retrieved`
- [ ] T034 [US5] Record metrics on CCR insert/retrieve and cache-hit in tool/STEL paths
- [ ] T035 [P] [US5] Expose `ccr_offloads` / `cache_hits` in admin `/api/v1/summary` (`src/server/admin/api_v1.rs`) — heuristic labels per 010
- [ ] T036 [P] [US5] Unit test ledger fields in `src/stel/ledger_store.rs` tests module

**Checkpoint**: US5 complete

---

## Phase 8: Polish & Cross-Cutting

- [ ] T037 [P] Run full gate from `quickstart.md`; fix regressions
- [ ] T038 [P] Verify `cargo check --no-default-features --features embed` (Principle VI)
- [ ] T039 [P] Verify `tests/persist_compression_ratio.rs` still passes (SC-005)
- [ ] T040 [P] Transport parity smoke: same tool call via test harness simulating stdio vs serve session (if harness exists; else document manual in quickstart)
- [ ] T041 Update cross-tool descriptions mentioning truncation → CCR retrieve in `src/protocol/tools.rs` tool doc strings

---

## Dependencies & Execution Order

```text
Phase 1 (T001–T003)
  → Phase 2 (T004–T006)
  → US1 (T007–T013) ──┐
  → US2 (T014–T023) ──┼── US3 benefits from US2 CCR path (T024 after T018)
  → US3 (T024–T029)
  → US4 (T030–T032) — after US1 fetch records
  → US5 (T033–T036) — after US1+US2 instrumentation
  → Polish (T037–T041)
```

### Parallel opportunities

- T001/T002/T003 after T001 starts (T002/T003 need module name only)
- T018–T020 can parallelize per tool handler file regions
- T028/T029 parallel after T024
- T037–T040 parallel in polish phase

### MVP scope (minimum shippable)

**US1 only** (Phase 1–3): session cache hit — immediate token win, zero CCR complexity.

**US1 + US2**: full P1 value — dedup + reversible bulk compression.

---

## Implementation Strategy

1. Ship **US1** first as MVP (1–2 days).
2. Land **US2** `search_text` only, then expand tools.
3. **US3** ranking can ship before CCR if needed (rank + truncate) then swap truncate for CCR.
4. **US4/US5** polish after core paths proven.
