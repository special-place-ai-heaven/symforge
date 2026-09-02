# Tasks: Repeat-Call Notice and Ledger Retry Collapse

**Input**: Design documents from `specs/032-repeat-call-breaker/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md — all present; the plan-stage 4-lens adversarial round ran 2026-09-01 and every finding is fixed or adjudicated in research.md R12. Implementation still owes its own independent PR review (Constitution VI).

**Tests**: MANDATORY and RED-FIRST (Constitution II). Every oracle is observed failing — with a receipt (test output or job id) recorded in the PR — before its machinery lands. Every negative oracle carries its accepting positive control in the same test. A test whose RED state is a compile refusal must still be receipted (the refusal output), then re-observed as a runtime failure against a stub before the real machinery lands.

**Organization**: US1 (repeat notice) and US2 (ledger collapse) share **no code** — independently implementable, testable, revertable. Either alone is a shippable increment.

## Format: `[ID] [P?] [Story] Description`

## Path Conventions

Single Rust crate at repo root: `src/`, `tests/`. All cargo invocations: serial, `-j 4`, `--test-threads=1`; anything that can exceed 10 minutes goes through Terminal Commander `run_and_watch` (never raw Bash) — Constitution IV. Note: all new US1/US2 code is server-gated at the crate root; the embed cell observes only the `src/stel_core/ledger_store.rs` widening (research.md R10).

---

## Phase 1: Setup (anchor re-verification — line numbers WILL have drifted)

**Purpose**: The plan's anchors were verified 2026-09-01; implementation happens later, possibly after other merges. Trust the symbols, not the line numbers.

- [X] T001 Re-verify every anchor in `specs/032-repeat-call-breaker/research.md` against the current index (`search_symbols`/`get_symbol_context`/`search_text`, not raw reads): `call_tool` seam shape (`src/protocol/mod.rs`), `RequestHash` signature + derive list (`src/idempotency.rs`), `ProjectEvidence` fields + `attach_project_evidence_meta` single-writer + `bound:false` marker + `"unbound"` placeholder (`src/protocol/result_status.rs`), `StelLedgerEvent` field list (`src/stel_core/types.rs`), `StoredLedgerRecord` + `recent()` SELECT + `ORDER BY id DESC` (`src/stel_core/ledger_store.rs`), `StelStatusContext`/`format_last_ledger_lines`/proxy overlay + `sample_context` fixture (`src/stel/status.rs`, `src/protocol/tools.rs:3826` prefix match), `LedgerSummaryView` (`src/server/admin/api_v1.rs`), verify-tools readiness probe (`scripts/verify-tools.cjs`). **Additionally verify the open design input R12-3 depends on: what per-session identity rmcp's `RequestContext` exposes at `call_tool` on the shared HTTP `/mcp` lane vs stdio** — record the answer in research.md R9 (it decides whether oracle 8 pins isolation or lane-inertness). Correct drifted line numbers in research.md in place; if any STRUCTURAL claim no longer holds, STOP and amend plan.md before proceeding.
- [X] T002 Baseline sanity on branch `032-repeat-call-breaker`: `cargo fmt --check` and `cargo check -j 4` green before any edit; note any pre-existing failure explicitly (never let it get blamed on this feature).

**Checkpoint**: anchors current, session-identity question answered, baseline clean.

---

## Phase 2: Foundational

**None.** The two stories share no code and no schema (plan.md Structure Decision). Proceed straight to US1 or US2 (or both in parallel).

---

## Phase 3: User Story 1 - Repeat-Call Notice (Priority: P1) 🎯 MVP

**Goal**: 3rd identical eligible read call, within one observed session, with continuously-equal typed project evidence, gets the contract's appended text + `_meta["symforge/repeat_notice"]`; a false claim is unconstructible.

**Independent Test**: `cargo test --test repeat_notice -- --test-threads=1` green, with each oracle's RED receipt in the PR; quickstart.md US1 manual smoke.

### Tests for User Story 1 (RED first — observe and receipt each failure)

- [X] T003 [US1] Create `tests/repeat_notice.rs` with quickstart oracles 1-2 (`third_identical_eligible_call_carries_notice_and_first_two_do_not` incl. SC-004 byte-stability + isError-untouched controls; `index_change_between_repeats_resets_run` incl. notice-returns control). Harness per R12 finding 13: copy the FULL-JSON subprocess client from `tests/rmcp3_roots_interop.rs` (`call_tool_result`, `SYMFORGE_NO_DAEMON=1`, `SYMFORGE_SURFACE=full`) — NOT `graceful_degradation.rs`'s text-only helper (panics on isError). Observe both RED; record receipts.
- [X] T004 [US1] Add quickstart oracles 4-5 to `tests/repeat_notice.rs`: `ineligible_tools_never_notice` (3× `status`, 3× `what_changed`, 3× `get_symbol` — all no-notice, eligible control in same test) and `eligible_list_is_pinned` (exact FIVE: search_symbols, search_text, get_repo_map, find_references, find_dependents). Observe RED; receipts.
- [X] T005 [US1] Add quickstart oracles 6-9: `internal_failure_clears_run` (+ InvalidRequest-advances control, per the data-model state machine — the single normative rule), `tracker_cap_clears_without_false_claim`, `sessions_never_share_runs` (RESOLVED by T001 2026-09-02: the HTTP `/mcp` lane is stateless with no observable per-session identity — research.md R9 — so this is the **lane-inertness pin**: 3× identical eligible call through the in-process HTTP harness from `tests/rmcp3_protocol.rs` carries NO notice, with the stdio subprocess positive control in the same test; plus an in-file config pin in `src/server/mcp_http.rs` that `legacy_session_mode == false`), `projects_argument_never_accumulates` (+ single-project control). Observe RED; receipts.

### Implementation for User Story 1

- [X] T006 [P] [US1] Create `src/protocol/repeat.rs` per data-model.md: `SessionDiscriminator`, `RepeatKey` (session + tool + `RequestHash`), `RepeatRun` (typed `ProjectEvidence` only — never raw `_meta` JSON), `RepeatWitness` (private constructor, `observe(prior, current) -> Option<_>` on full equality), `RepeatNotice` (only constructor takes a `RepeatWitness`), `RepeatTracker` (bounded map, `clear()` on cap with a `ponytail:` comment naming the ceiling and LRU upgrade path), consts `NOTICE_THRESHOLD = 3`, `REPEAT_TRACKER_MAX_ENTRIES = 512`, `REPEAT_ELIGIBLE_TOOLS` (5 tools). Module unit tests = quickstart oracles 3 (marker / non-deserializable / `"unbound"` project_id ⇒ cleared, + evidence-present control) and 10 (witness inequality ⇒ None; threshold 2-vs-3; saturating count). This module is server-only by location — no embed gating needed (R10).
- [X] T007 [P] [US1] Add `Hash` to `RequestHash`'s derive in `src/idempotency.rs` (one word — R12 finding 11), and add `REPEAT_NOTICE_META_KEY` + `RepeatNoticeView` serde (contract_version, repeat_count, tool, request_hash, evidence_generation) to `src/protocol/result_status.rs` per `contracts/repeat-notice.md`, following the `RESULT_STATUS_META_KEY`/`PROJECT_EVIDENCE_META_KEY` conventions.
- [X] T008 [US1] Wire the seam in `SymForgeServer::call_tool` (`src/protocol/mod.rs`): add `repeat_tracker: Arc<parking_lot::Mutex<RepeatTracker>>` to `SymForgeServer` (beside `stel_ledger`; the struct is `Clone` and cloned per HTTP request / per daemon project, so the `Arc` is what makes every clone share one map) + BOTH `_with_state_placement` constructors (the only two struct-literal sites, ~422/498); derive the session discriminator per T001's answer (research.md R9: `SessionDiscriminator::observe(&context)` → `HttpInert` iff `context.extensions.get::<axum::http::request::Parts>()` is `Some`, else `Stdio`; `HttpInert` = NO tracker interaction); clone `(request.name, request.arguments)` BEFORE `ToolCallContext::new` consumes the request (2157); AFTER dispatch + evidence attach (2173) + the error-reclassification block (2181-2191 — the notice must never be fed to `is_error_output`), for eligible tools: deserialize the response's attached `symforge/project_evidence` into TYPED `ProjectEvidence`, update the tracker per the data-model state machine, and on `RepeatNotice` append the contract's byte-canonical text to the final text content block and insert the `_meta` entry. `isError` and prior bytes untouched (depends on T006, T007).
- [X] T009 [US1] Give `scripts/verify-tools.cjs`'s readiness probe a fingerprint-distinct argument (R12 finding 8 — today it repeats the first `search_symbols` snapshot case's exact query, `verify-tools.cjs:334-352, 465-467`, so ≥2 probe iterations would put the notice into a release-gate snapshot); confirm both fixtures still pass against the release binary later in T019.
- [X] T010 [US1] Observe T003-T005 oracles green + full `tests/repeat_notice.rs` pass.

**Checkpoint**: US1 fully functional, independently testable, revertable in isolation.

---

## Phase 4: User Story 2 - Ledger Retry Collapse (Priority: P2)

**Goal**: Runs of identical consecutive ledger events render as one row `×N` with time span — status `detail:full` (trailing run, `last_ledger_decision:` line) and admin `recent_runs` (chronological, session-scoped, window-clipping labeled) — presentation-only, lossless.

**Independent Test**: collapse property tests + `tests/admin_api_v1.rs` + status oracles green; full pin-inventory sweep (research.md R11) reconciled.

### Tests for User Story 2 (RED first — observe and receipt each failure)

- [X] T011 [US2] Add `collapse_runs` property oracles as unit tests in `src/stel/ledger.rs` (`#[cfg(test)]` module), per lane identity (event lane + durable lane): counts sum to input length; strict consecutiveness (A,A,B,A → 2,1,1); all-distinct ⇒ all counts 1; event-lane identity ignores exactly the six non-identity fields; durable-lane identity includes session_id and the three widened columns, excludes accepted/eligible_h6 (data-model.md Lane B). Observe RED (against a stub); receipts.
- [X] T012 [P] [US2] Add `status_full_annotates_trailing_run` oracle beside the existing status tests (matching where the current last-ledger pins live): trailing run of ≥2 identical events (seeded via public `SessionLedger::push`) renders ` ×N (first=…, last=…)` on the **`last_ledger_decision:` line only**; single-event control renders byte-identically in the same test; one large-N control (10,000 — SC-003 status lane). Observe RED; receipt.
- [X] T013 [P] [US2] Add admin oracles in `tests/admin_api_v1.rs` per `contracts/admin-recent-runs.md`: `admin_recent_runs_collapses_and_scopes_by_session` (two session_ids via two file-backed `StelLedgerStore::open` handles on one temp path — `open_in_memory` binds one session per private DB; collapse within session, never across; chronological order; `available:false` ⇒ `recent_runs: []`; FR-008 control — the existing 3-identical-event seed collapses ×3 with totals unchanged) and `admin_window_edge_is_labeled` (run longer than the window ⇒ `window_clipped: true` + `recent_runs_window` present). Observe RED; receipts.

### Implementation for User Story 2

- [X] T014 [US2] Implement generic `collapse_runs(items, identity_key) -> Vec<Run<T>>` + the two lane identity fns in `src/stel/ledger.rs` (event lane via exhaustive destructure — every field named, ignored ones bound explicitly so a future `StelLedgerEvent` field breaks compilation). T011 green.
- [X] T015 [US2] Widen `StelLedgerStore::recent()`'s SELECT + `StoredLedgerRecord` additively with `pff_bypass`, `cache_hit`, `degrade_flags_json` in `src/stel_core/ledger_store.rs` (columns exist in the table; R12 finding 4); reconcile the additive impact on `tests/stel_l4_ledger.rs:241-245` and `tests/stel_ledger_persistence.rs`; run the embed cell afterwards (this is the ONE file the embed gate observes — R10).
- [X] T016 [US2] Status lane (context-shape change, R12 finding 12): add `trailing_run: Option<(u64, u64, u64)>` to `StelStatusContext`, compute it in `from_server` from `ledger.events()` via `collapse_runs`, update the `sample_context` fixture and all three protocol construction sites (`src/protocol/tools.rs` — the `from_server` call sites), render the suffix in `format_last_ledger_lines` on the `last_ledger_decision:` line only (byte-identical when trailing run == 1), and verify the proxy overlay's `starts_with("last_ledger_decision:")` replacement still matches the annotated line. T012 green (depends on T014).
- [X] T017 [US2] Admin lane: add `recent_runs` + `recent_runs_window` + `window_clipped` to `LedgerSummaryView` in `src/server/admin/api_v1.rs` per the contract — fetch `recent(50)`, reverse to chronological, collapse with session_id in identity, label the window-edge run; `[]` on unavailable/error. T013 green (depends on T014, T015).
- [X] T018 [US2] Minimal recent-runs list rendering in `src/server/admin/assets/app.js`; extend the field-name pin in `tests/admin_render.rs` deliberately. Then run the blast-radius sweep: every pinned test in the research.md R11 inventory INCLUDING `scripts/verify-tools.cjs` fixtures; reconcile each change deliberately (a changed pin means a seed produced a ≥2 run — verify that's true before updating it); NEVER loosen an assertion to pass.

**Checkpoint**: US2 fully functional; both stories work independently.

---

## Phase 5: Polish & Delivery

- [X] T019 Full gate battery per quickstart.md, in order, one cargo process at a time via Terminal Commander: fmt → clippy `--all-targets -D warnings` → full serial suite → embed cell (stel_core regression only — R10) → bench `observed_refresh_gate_v1 -- --test` → release build → `node scripts/verify-tools.cjs --bin target/release/symforge` (both fixtures; flags per ci.yml) → npm suite. ALL observed green (Constitution IV) — receipts kept.
- [X] T020 Confirm no doc/test seal was tripped: no `.github/workflows` byte changed (`WORKFLOW_FINGERPRINTS`), no README/AGENTS.md pinned phrase touched, no `SYMFORGE_TOOL_NAMES`/tool-count change; answer the doc-staleness hook's injected CLAUDE.md claims at commit time honestly.
- [ ] T021 `cargo clean` (heavy-session discipline), then commit(s) with conventional subjects and open ONE behavior-changing PR referencing this spec; request independent adversarial review including the cfg-lens sweep (Constitution VI); squash-merge only with explicit `--subject`/`--body` per repo merge rules. Durable finding (if any) → agentmemory with `[symforge]` prefix.

---

## Dependencies & Execution Order

- **Phase 1 → everything** (anchors first; T001's session-identity answer shapes T005/T008).
- **US1 chain**: T003-T005 (RED) → T006/T007 [P] → T008 → T009 → T010.
- **US2 chain**: T011 (RED) → T012/T013 [P] (RED) → T014 → T015 → T016/T017 (different files, [P]-able) → T018.
- **US1 ∥ US2**: fully parallel after Phase 1 (no shared files — T009 touches verify-tools.cjs, T015 touches ledger_store.rs; disjoint).
- **Phase 5** after all delivered stories.

## Parallel Example

```text
After T002: one worker on T003 (US1 RED oracles) while another does T011-T013 (US2 RED oracles).
After T005: T006 and T007 in parallel (different files).
After T015: T016 and T017 in parallel (different files).
```

## Implementation Strategy

**MVP = US1 alone** (Phase 1 + Phase 3 + Phase 5 minus US2 sweep): the notice is the half with measured demand (A019). US2 is an additive rendering increment; either story reverts cleanly without the other.
