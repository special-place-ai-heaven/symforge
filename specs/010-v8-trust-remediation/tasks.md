---
description: "Task list for v8 Trust Remediation implementation"
---

# Tasks: v8 Trust Remediation

**Input**: Design documents from `specs/010-v8-trust-remediation/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: REQUESTED — FR-019 mandates the three named regression tests + the per-phase
gate; quickstart defines per-story acceptance. Test tasks are included and written before
the fix where practical.

**Organization**: by user story (US1=Phase A … US6=Phase F), sequenced per the ledger
(relabel → status truth → edit safety → recovery → economics → matrix+CI). Each story is an
independently shippable increment; the full gate runs after each.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different file, no dependency on an incomplete task)
- **[Story]**: US1–US6; Setup/Foundational/Polish carry no story label
- Anchors (file:line) are from the ledger; **re-confirm against live code at use** (line
  numbers drift). Harness MCP may be any version; correctness is proven by `cargo`.

## Path Conventions

Single Rust crate `symforge`; sources under `src/`, tests under `tests/`, docs under `docs/`.

---

## Phase 1: Setup (Baseline — goal PHASE 0)

**Purpose**: establish a green baseline and re-confirm anchors before touching code.

- [ ] T001 Capture baseline green gate: run `cargo fmt --check`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, `cargo check --no-default-features --features embed`; record pass/fail in a scratch note. Do NOT blanket-kill `symforge*` (the user's live MCP is a global binary; cargo builds to `target/`, no conflict).
- [X] T002 [P] Re-confirm anchors against live `src/` — DONE 2026-06-17 (45/45 verified, 0 line-drift; 5 divergences below).
- [X] T003 [P] Confirm `target/` warm and stays warm (no `cargo clean` until campaign end). Confirmed.

**Checkpoint**: anchors verified live; baseline gate running (T001).

### Live anchor notes (T002 result, 2026-06-17) — read before editing

Current exact lines (no drift): `status_stel_tool` tools.rs:8524-8557 (reads `self.index` @8541);
daemon match daemon.rs:2306-2435 (NO `status` arm → "unknown tool"); `ReplaceSymbolBodyInput`
edit.rs:1160-1189 (no `if_match`); `StelEditRequest` types.rs:108-126 (HAS `if_match` @122);
pre-flight `run_pre_apply_gates` edit_apply.rs:38-90 (if_match check @73-79 under index.read);
planner 400/800 planner.rs:51-52; estimator format.rs:4930 / 5006; controller branches
controller.rs:55-134 (bypass@58 net<=0, mandatory_degrade@75, economics_degrade@77, serve@125);
`summary().ok()` ledger_store.rs:227-231; status literals status.rs:110-116 + DEFERRED_ITEMS@15-16;
4 dead-end strings format.rs:4774, edit_apply.rs:48, tools.rs:6033, edit_tools.rs:23(macro);
`loading_guard!` ~26 sites; wrapper init.rs:837 `cd /d %USERPROFILE%`, env:{} @761-765;
`find_project_root` discovery/mod.rs:483-515; expected_equiv types.rs:313 (write-only/dead);
A-005 OPEN@77 vs VALIDATED@146; A-009 VALIDATED@86; A-016 OPEN@103; A-028 VALIDATED@129;
"32 canonical tools" README:24,328 AGENTS:125 CLAUDE:34; CI `.github/workflows/ci.yml` (rust job 55-79, embed 81-103).

**Divergences that change implementation:**
- **D-a (T014)**: A-005 self-contradicts itself (OPEN@77, VALIDATED@146) — single-source it.
- **D-b (T023-T025)**: `if_match` is plumbed at L0 (`StelEditRequest`@122) + validated in pre-flight
  under the read guard, but DROPPED by the planner (edit_planner.rs:72-80) before the legacy write,
  and `ReplaceSymbolBodyInput` has no field; the write (tools.rs:519) never re-verifies → TOCTOU real.
  Fix = thread field through to legacy input + re-verify at the write critical section (D1). The pre-flight
  check stays but is NOT the guarantee.
- **D-c (T018/T019)**: daemon has NO `status` arm; `status_stel_tool` reads the front-end `self.index`.
  Add a daemon `status` arm + proxy the front-end read to it.
- **D-d (T011)**: no `"pending"` literal; `CalibrationState` (types.rs:292-298) is rendered via
  `format_calibration_section` (status.rs:155) from `src/stel/calibration.rs`. Relabel target is there;
  first verify the EMA is never meaningfully updated (N-1 dead-seam claim) before labeling `deferred`/
  `observational`. Keep the seam (Do-Not #7).
- **D-e (T037/T038)**: controller.rs:55 ALREADY calls `estimate_economics` → the real estimator;
  the planner just stamps 400/800 with `index_refs`/`raw_chars` inert. Grounding = feed real raw_chars
  to the already-wired estimator (smaller than "wire a new path").

---

## Phase 2: Foundational (Shared honest-label vocabulary)

**Purpose**: the enumerated-state types US1/US2/US5 all consume. BLOCKS the story phases
that label state. Pure type additions — no behavior, no surface change yet.

**⚠️ CRITICAL**: no labeling story can finish until these types exist.

- [~] T004 `ProofState` enum — FOLDED INTO US1: static `&str` labels were the smallest honest change; no single-consumer enum added (justified in commit 6c0fa14).
- [~] T005 `SubsystemState` enum — DEFERRED TO US2/Phase B: it only earns its place where status probes live state (Disabled(reason) vs Unavailable, N-3).
- [~] T006 `IndexState` enum — ALREADY EXISTS (loading_guard! macro: Ready/Empty/Loading/CircuitBreakerTripped); reuse in Phase B. No duplicate added.

**Checkpoint**: shared vocabulary resolved (existing IndexState reused; runtime enum lands in B). `cargo check` green.

---

## Phase 3: User Story 1 — Every reported number/label is honest (Priority: P1) 🎯 MVP

**Goal**: every status + economics-envelope field is true or explicitly labeled
heuristic/observational/deferred. **Zero behavior change** (relabel only).

**Independent Test**: read every status/envelope field — each is `Measured` or carries a
qualifier; no `net`/`saved`/`active`/`validated` presents a constant or gross counter as a
measured result; golden replay unchanged.

### Tests for User Story 1

- [X] T007 [P] [US1] surface-honesty test `tests/surface_honesty.rs` — DONE (7/7 pass, asserts honest state, fails pre-010).
- [X] T008 [P] [US1] golden-replay invariance — DONE (11/11 + conformance 19/19 unchanged).

### Implementation for User Story 1

- [X] T009 [US1] `session_net_vs_manual` → `session_tokens_served` + dropped `+` framing — DONE (envelope.rs, handler.rs, tools.rs 10 sites).
- [X] T010 [US1] envelope figures `est.`/`heuristic`; reject → `n/a (rejected)` — DONE (envelope.rs, format string).
- [X] T011 [US1] `calibration: pending` → `deferred`; `CalibrationState` seam kept + documented — DONE (handler.rs, types.rs).
- [X] T012 [US1] status `l*/handler_*: active` → honest static `wired`/`in_memory`; dropped `ledger_persistence` — DONE (status.rs).
- [X] T013 [P] [US1] chars/4 figures documented as estimates — DONE (format.rs, handler.rs).
- [X] T014 [P] [US1] A-005 single-sourced VALIDATED(caveat); A-009→PARTIAL; A-028→OPEN; A-011/15/16 untouched — DONE (stel-assumptions.md; +stel-schema.md honest).
- [X] T015 [US1] per-phase gate + golden replay green; committed Phase A (6c0fa14).

**Checkpoint**: US1 shippable alone — all surfaces honest, behavior identical. MVP done. ✅ committed 6c0fa14.

---

## Phase 4: User Story 2 — Status tells the truth about the index (Priority: P1)

**Goal**: a working index never reports empty; `status` reads the same index that serves.

**Independent Test**: serve + query to populate the index, read `status`, counts match the
served index (Ready, non-zero); a failing-open ledger reads `Disabled(reason)` ≠ `Unavailable`.

### Tests for User Story 2

- [X] T016 [P] [US2] regression `status_index_matches_daemon_proxy_after_symforge_serve` (in-crate full daemon-proxy, daemon.rs tests) + HTTP arm test (`tests/status_truth.rs`) — DONE; both proven to FAIL pre-fix (daemon `unknown tool 'status'` + empty front-end read).
- [X] T017 [P] [US2] disabled-vs-unavailable test (`subsystem_state_distinguishes_broken_from_off_and_healthy` + surface render test) — DONE (N-3, FR-008).

### Implementation for User Story 2

- [X] T018 [US2] daemon `status` arm in `src/daemon.rs` (execute_tool_call, before catch-all) → `status_for_daemon_session` — DONE (TR-01).
- [X] T019 [US2] `status_stel_tool` proxies to daemon (mirrors `health`) + shared `render_stel_status_body`; local/embed fallback intact — DONE (TR-01, FR-006/007).
- [X] T020 [US2] `summary().ok()` no longer swallows; new `subsystem_state()` → `Disabled{reason}` vs `Unavailable` (`ledger_store.rs`, `mod.rs`, `status.rs` DurableLedgerState) — DONE (N-3, TR-17). Reachability caveat documented (Disabled distinct only on serve `/mcp` surface).
- [X] T021 [US2] full gate green (fmt/check/clippy/test/build/embed); code-reviewer: no Critical, SHOULD-FIX (dead staleness guard) + nit + caveat all resolved (staleness guard now fires for status + 2 tests). Commit Phase B.

**Checkpoint**: US1+US2 work independently; status never lies about the index. ✅

---

## Phase 5: User Story 3 — A guarded edit actually guards (Priority: P1)

**Goal**: `if_match` is enforced at the write; a concurrent divergence is rejected, never a
false success.

**Independent Test**: guarded apply + injected concurrent change ⟹ rejected, on-disk change
intact, no false "guarded apply succeeded"; negative control succeeds.

### Tests for User Story 3

- [ ] T022 [P] [US3] Add the named regression `symforge_edit_if_match_rejected_after_concurrent_disk_change` in `tests/edit_safety.rs` using a **deterministic injected interleave point** (test hook between guard-read and write, NOT a sleep) (TR-06, SC-003); include the negative control.

### Implementation for User Story 3

- [ ] T023 [US3] Add an `if_match` field to `ReplaceSymbolBodyInput` in `src/protocol/edit_tools.rs` (~570-676) (TR-06).
- [ ] T024 [US3] Thread `if_match` through the edit planner so it is not dropped in `src/protocol/edit_planner.rs` (~72) (TR-06).
- [ ] T025 [US3] Re-verify the guard against the bytes actually written, in the same critical section as the splice + `atomic_write`; reject on divergence in `src/protocol/edit_apply.rs` (~73-91) and `src/protocol/edit.rs` (~1160) (FR-009, D1).
- [ ] T026 [P] [US3] Honest response + N-6 boundary: claim a guarded apply only when enforced at write; mark the batch path "no if_match (same TOCTOU if extended)" and keep `verify_index_matches_disk` labeled pre-flight-only (FR-010, N-6); ensure the tee backup is not called transactional rollback.
- [ ] T027 [US3] Run the per-phase gate; confirm T022 passes (both reject + negative control). Commit Phase C.

**Checkpoint**: US1+US2+US3 — the three real-bug/keystone P1s done.

---

## Phase 6: User Story 4 — Recoverable cold start, no dead-end (Priority: P2)

**Goal**: a fresh attach indexes automatically, or the error names only callable recovery on
the active surface — never a gated tool.

**Independent Test**: fresh default attach with no pre-index ⟹ auto-index OR a recovery
message naming only surface-callable actions; the desktop launch path discovers the project
root (not `%USERPROFILE%`).

### Tests for User Story 4

- [ ] T028 [P] [US4] Add the named regression `compact_surface_index_not_loaded_message_never_mentions_blocked_tools` in `tests/recovery.rs`: on the compact profile, assert no empty-index message names a gated tool (TR-02, SC-004).
- [ ] T029 [P] [US4] Add a test that the cold-start root discovery resolves the project workspace, not the home dir (TR-03).

### Implementation for User Story 4

- [ ] T030 [US4] Add a single surface-aware `empty_index_recovery_hint(profile)` in `src/protocol/format.rs` (~4774) that never names a gated tool (compact: re-launch from root / documented opt-out; full: may name `index_folder`) (TR-02, D4).
- [ ] T031 [US4] Route all 4 dead-end strings + the `loading_guard!` sites through `empty_index_recovery_hint` in `src/protocol/format.rs`, `edit_apply.rs` (~48), `tools.rs` (~6033), `edit_tools.rs` (~263) (N-5, FR-012).
- [ ] T032 [US4] Fix the desktop wrapper so it does not `cd /d "%USERPROFILE%"` before launch in `src/cli/init.rs` (~837) so `find_project_root()` discovers the workspace (TR-03).
- [ ] T033 [US4] Write a proven init `env` (root / `SYMFORGE_SURFACE` / auto-index hint) instead of `env:{}` in `src/cli/init.rs` (~761); verify `find_project_root()` in `src/main.rs` (~217-248) populates the index (TR-03, FR-013).
- [ ] T034 [US4] Run the per-phase gate; confirm T028/T029 pass. Commit Phase D.

**Checkpoint**: cold start recovers; no dead-end loop.

---

## Phase 7: User Story 5 — Economics grounded in reality (Priority: P2)

**Goal**: predictions derive from real size; the adaptive branches (degrade/bypass) become
reachable. (Ground-now, clarified 2026-06-17.)

**Independent Test**: same op over small vs large file ⟹ predictions differ proportionally; a
non-serve branch is reachable for a small request; `expected_equiv` is asserted or removed.

### Tests for User Story 5

- [ ] T035 [P] [US5] Add a test asserting predictions vary with real file size (two materially different inputs ⟹ different predictions) in `tests/economics.rs` (SC-005, US5 AC-1).
- [ ] T036 [P] [US5] Add a test asserting a non-serve economics branch (`degrade`/`bypass`/`mandatory_degrade`) is reachable for a small/cheap request (TR-04b, N-2, US5 AC-2).

### Implementation for User Story 5

- [ ] T037 [US5] Wire the existing estimator (`competent_manual_baseline_chars` / `saved_tokens_vs_competent_manual`, `src/protocol/format.rs` ~4925-5029) into the planner, replacing the `400/800` constants in `src/stel/planner.rs` (~44-55) (TR-04, D2).
- [ ] T038 [US5] Make `index_refs`/`raw_chars` carry real values so `predicted_net` varies; verify the economics gate now routes on real input in `src/stel/controller.rs` (~40-135) (TR-04, TR-04b).
- [ ] T039 [P] [US5] Assert-or-remove `expected_equiv`: either assert it in golden replay or delete the write-only dead data; purge any "95% trajectory" tautology claim in `src/stel/golden_replay.rs` (~244-310) and `src/stel/types.rs` (~313) (TR-13, FR-015).
- [ ] T040 [US5] Run the per-phase gate + golden replay; confirm T035/T036 pass. Commit Phase E.

**Checkpoint**: economics is real (or honestly heuristic where a figure isn't yet grounded).

---

## Phase 8: User Story 6 — Honest public record + enforced honesty (Priority: P2)

**Goal**: docs describe the real default surface; a capability matrix maps proof states; CI
fails a claim that outruns the evidence.

**Independent Test**: docs state compact-3 default + 32-tool opt-out; `v8-capability-matrix.md`
maps feature → assumption ID → proof state; the honesty CI gate fails a surface claiming an
OPEN-assumption capability, and passes honest OPEN-labeling.

### Tests for User Story 6

- [ ] T041 [P] [US6] Add a test/lint asserting `docs/v8-capability-matrix.md` exists and every row has feature + proof state + assumption ID (FR-017).
- [ ] T042 [P] [US6] Add a test for the honesty gate: a fixture surface claiming a capability whose assumption is OPEN ⟹ gate FAILS; an honest OPEN-labeled fixture ⟹ passes (FR-018, US6 AC-3).

### Implementation for User Story 6

- [ ] T043 [P] [US6] Publish `docs/v8-capability-matrix.md` (feature → assumption ID → Implemented/Heuristic/Observational/Deferred), framing A-017/A-011 as bet-under-test (TR-09, FR-017).
- [ ] T044 [P] [US6] Update README.md (L24, L328), AGENTS.md (L125), CLAUDE.md (L34) to describe the compact-3 default with the 32-tool surface as a documented opt-out (TR-07, FR-016). Ship only AFTER Phase A (Do-Not #4).
- [ ] T045 [US6] Implement the honesty CI gate (static parse + cross-reference: OPEN-assumption + validated-claim = FAIL; one-source-of-truth per number; VALIDATED requires artifact) as a `.github/workflows/` check (FR-018).
- [ ] T046 [US6] Run the per-phase gate + the new honesty gate; confirm T041/T042 pass. Commit Phase F.

**Checkpoint**: public record honest; regression of the honesty work is CI-blocked.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: campaign-level verification and integration.

- [ ] T047 Run the full quickstart acceptance pass for all six stories (SC-001..SC-007).
- [ ] T048 Live keystone dogfood (SC-008): build the local 8.0.0 binary (`cargo build --release`), run `symforge serve`, reconnect a client; `status` compact reports real index; orient query succeeds; `status` full counts MATCH the served query; `symforge_edit` preview honest. Record evidence.
- [ ] T049 [P] Confirm Constitution VI/VII: `cargo check --no-default-features --features embed` green and stdio↔serve parity for any touched formatter.
- [ ] T050 git-master: integrate all phase commits onto a review branch; HARD-STOP before any push/merge (await explicit human approval).
- [ ] T051 Write the honest results doc (objective / changes / verification / evidence / known gaps) under `docs/reviews/`; `cargo clean` only now (campaign end).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (P1)**: no deps — start immediately.
- **Foundational (P2)**: depends on Setup — BLOCKS the labeling stories (US1, US2, US5 use the enums).
- **US1 (P3)**: after Foundational. The relabel; ships first (Do-Not #4 — before any "token-efficient" doc in US6/T044).
- **US2 (P4)**: after Foundational. Uses `SubsystemState`/`IndexState`; otherwise independent of US1.
- **US3 (P5)**: after Setup only (edit path — no enum dep); independent.
- **US4 (P6)**: after Setup; independent (recovery text + init).
- **US5 (P7)**: after Foundational (proof-state labels); reopens economics branches.
- **US6 (P8)**: docs/CI; T044 (README "token-efficient") MUST follow US1; the gate (T045) can land last.
- **Polish (P9)**: after all desired stories.

### Within Each Story

- Tests written before the fix (assert they fail), then implement, then the per-phase gate, then commit.
- Each story ends green on the full gate before the next begins (FR-019).

### Parallel Opportunities

- Setup T002/T003 [P]; Foundational T006 [P] (T004/T005 touch shared files first).
- The two **real-bug** stories US2 (status) and US3 (edit safety) touch disjoint files (`daemon.rs`/`tools.rs`/`status.rs` vs `edit_*`) → can be implemented in parallel by separate agents after Foundational.
- US4 (init/format recovery) is disjoint from US2/US3 → parallelizable.
- Within a story, `[P]` test tasks run together; doc tasks (T014, T043, T044) are file-disjoint.
- **Throttle**: each phase's gate is a heavy `cargo` run — serialize the gates (do not run 3 phase-gates concurrently); keep `target/` warm.

---

## Implementation Strategy

### MVP First (P1 trio)

1. Setup + Foundational.
2. US1 (relabel, zero-behavior) → **STOP, validate** (the keystone honesty quick-win, shippable alone).
3. US2 (status truth) + US3 (edit safety) — the two real bugs. These three P1s are the highest-leverage delivery.

### Incremental Delivery

US1 → US2 → US3 (P1 done) → US4 → US5 → US6 (P2 done) → Polish. Each story is a green,
independently-testable increment; integrate to a review branch and STOP for human approval
before any push/merge (T050).

---

## Notes

- Anchors are ledger line numbers; re-confirm live before editing (Step-0 / EDIT INTEGRITY).
- Phase A (US1) ships before any README "token-efficient" language (Do-Not #4).
- relabel ≠ validate — never promote A-011/A-015/A-016/A-028 for a label change.
- Do NOT revert compact-3 / re-expose 32 tools, gate daemon IPC on compact, or delete the
  `CalibrationState` seam (Do-Not #1/#3/#7).
- No push/merge without explicit human approval — commit to a review branch and stop.
