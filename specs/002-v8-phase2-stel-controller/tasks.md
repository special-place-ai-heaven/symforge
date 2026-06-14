# Tasks: SymForge v8 Phase 2 STEL Controller Maturity

**Input**: Design documents from `specs/002-v8-phase2-stel-controller/`

**Prerequisites**: [plan.md](./plan.md) (required), [spec.md](./spec.md) (required), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/phase2-gate-evidence-contract.md](./contracts/phase2-gate-evidence-contract.md)

**Status**: **P2-S4.1 H3 remediation** — H3/H4/H5 PASS on refreshed battery evidence.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1–US5) from spec.md

---

## P2-S0: Spec & Review (current)

- [x] T001 Record Spec Kit inputs in `docs/research/phase2-evidence-index.md` (spec, plan, contract paths)
- [x] T002 Reviewer sign-off on `specs/002-v8-phase2-stel-controller/spec.md` (independent from implementer) — GO in [`docs/research/phase2-spec-review-signoff.md`](../../docs/research/phase2-spec-review-signoff.md)
- [x] T003 Open milestone branch `cursor/v8-phase2-stel-controller` from green `main`

**Checkpoint**: Spec approved; branch created; v8 Phase 1 CI green on branch base.

---

## P2-S1 / P2-S2: Multi-Hop L1 + Executor (US1)

- [x] T010 [US1] Extend `StelPlan` / types for ordered multi-step plans in `src/stel/types.rs`
- [x] T011 [US1] Implement multi-hop routing in `src/stel/planner.rs` for three golden row patterns
- [x] T012 [US1] Extend `src/stel/executor.rs` for in-process step chain (fail-fast on mid-chain error)
- [x] T013 [US1] Wire multi-step dispatch in `symforge_stel_handler` (`src/protocol/tools.rs`)
- [x] T014 [US1] Remove `DEFERRED_MULTI_HOP_ROW_IDS` deferral — replay as SupportedServe in `src/stel/golden_replay.rs`
- [x] T015 [US1] Add/extend `tests/stel_golden_replay.rs` for 36/36 classification (0 deferred multi-hop)
- [x] T016 [P] [US1] Add fixture corpus for `is-plain/multi_files_content` if needed under `tests/fixtures/` (existing is-plain corpus sufficient)

**Checkpoint**: `cargo test --test stel_golden_replay` — 0 deferred multi-hop; CI green.

---

## P2-S3: L2 Admission Hardening (US2)

- [x] T020 [US2] Implement `cache_hit` decision path in `src/stel/controller.rs` (session target match)
- [x] T021 [US2] Implement `degrade` path with degrade_flags (outline_only, token caps) per stel-schema
- [x] T022 [US2] Implement non-P-FF `bypass` when predicted net ≤ 0 (honest StelBypassBody)
- [x] T023 [US2] Ensure L3 honors all four decisions in `src/stel/executor.rs`
- [x] T024 [P] [US2] Add L2 admission unit tests (`src/stel/controller.rs` tests or `tests/stel_l2_admission.rs`)
- [x] T025 [US2] Verify P-FF bypass enforcement unchanged (`tests/stel_l3_enforcement.rs`)

**Checkpoint**: All admission states covered by tests; golden + L3 suites green.

---

## P2-S4: Battery Gates H3/H4/H5 (US3)

- [x] T030 [US3] Ensure sf-bench row writer populates STEL extension fields per contract
- [x] T031 [US3] Run compact-surface battery; save candidate results JSON (operator path)
- [x] T032 [US3] Run compare-results; write `docs/research/phase2-gate-report.md`
- [x] T033 [US3] Verify H3 PASS under A-012 documented scope — **PASS** (remediated `records/t8_explore`; see gate report)
- [x] T034 [US3] Verify H4 PASS (`session_net_accepted ≥ 0`)
- [x] T035 [P] [US3] Verify H5 PASS for `chain=single` rows (external mcpCalls ≤ 1)

**Checkpoint**: Gate report shows H3/H4/H5 PASS on refreshed candidate artifact.

---

## P2-S5: A-029 Spike (US4)

- [ ] T040 [US4] Create `docs/research/A-029-t2-spike.md` with method and repos
- [ ] T041 [US4] Run T2 spike on tokio + django reference tasks (compact surface)
- [ ] T042 [US4] Record PASS (≥2/4 equiv) or P-T2 pivot or KILL
- [ ] T043 [P] [US4] Optional T3 large-row degrade validation for A-014 (document pass/pivot)

**Checkpoint**: A-029 artifact complete; assumption register updated.

---

## P2-S6: Assumptions, Docs, Exit (US5)

- [ ] T050 [P] [US5] Update `docs/stel-assumptions.md` verdicts A-008..A-014, A-029
- [ ] T051 [US5] Create `docs/phase2-stel-checkpoint.md` with exit record per contract
- [ ] T052 [US5] Scope audit: confirm no persistence, B-RESULTS, or 8.0 baseline pin in PR
- [ ] T053 [US5] Merge milestone branch to `main` after reviewer gate sign-off

**Checkpoint**: Phase 2 exit record PASS; main CI green.

---

## Explicitly Excluded Tasks (do not add without spec amendment)

- SQLite / durable ledger migration (Phase 3)
- Calibration EMA → L2 auto-tuning (Phase 3, A-016)
- B-RESULTS / RESULTS.md §8.7 closure (post–8.0, A-024)
- `symforge serve` / HTTP transport (Phase 4)
- Multi-file `symforge_edit` apply

---

## Dependencies & Execution Order

```text
T001–T003 (spec review)
  → T010–T016 (multi-hop) — blocks battery honesty
  → T020–T025 (L2) — can overlap late multi-hop if interfaces stable
  → T030–T035 (battery) — requires T010–T025
  → T040–T043 (A-029) — parallel with battery after L2 stable
  → T050–T053 (exit docs)
```

---

## Parallel Opportunities

- T016 fixture corpus ∥ T011 planner work (after T010 types)
- T024 L2 tests ∥ T025 L3 regression once controller API stable
- T035 H5 check ∥ T033–T034 if same battery artifact
- T050 assumption register ∥ T051 checkpoint doc (after gates known)

---

## Implementation Strategy

1. **MVP (Slice 1 only)**: T010–T016 — closes golden deferrals; no gate claim yet.
2. **Controller slice**: T020–T025 — admission maturity.
3. **Evidence slice**: T030–T043 — H3/H4/H5 + A-029.
4. **Exit**: T050–T053 — docs + merge.

**Stop rule**: Any task that introduces persistence or B-RESULTS → halt and amend spec.
