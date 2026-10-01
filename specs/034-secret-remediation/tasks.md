# Tasks: Agent-Driven Secret Remediation

**Input**: Design documents from `specs/034-secret-remediation/`  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/  
**Note**: Spec-track PR delivers documentation only. Implementation tasks below are for the follow-on implement PR — do not execute them in the docs PR.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1–US4 mapping

---

## Phase 0: Spec track (THIS PR) — complete when analyze.md PASSes

- [x] T000a Author `spec.md` from HANDOVER §3 (specify)
- [x] T000b Clarify — encode settled decisions; zero `NEEDS CLARIFICATION` (checklist)
- [x] T000c Author `plan.md`, `research.md`, `data-model.md`, `quickstart.md`, `contracts/*`
- [x] T000d Author `tasks.md` (this file)
- [x] T000e Author `analyze.md` cross-artifact review
- [x] T000f Open/update docs PR for LINUS review — merged as #743 (`b870e659`, squash from aecdd03c)

---

## Phase 1: Setup (implementation PR)

- [x] T001 Confirm branch rebased onto current `origin/main`; `CLAUDE_PROJECT_DIR=` documented in PR body
- [x] T002 [P] Add contract fixture stubs under `tests/` (or in-module) named per quickstart oracles — expect RED
- [x] T003 [P] Register tool name placeholder in `scripts/verify-tools.cjs` / tool list pins (will fail until handler exists — sequence carefully)

---

## Phase 2: Foundational

- [x] T004 Introduce `WITHHELD_META_KEY` and serde types in `src/protocol/result_status.rs` (or dedicated module) with no secret-byte fields
- [x] T005 Shape descriptor helper (synthetic-safe) reusable by meta attachment and tests
- [x] T006 Wire finding-id minting from admission scan identity (stable across preview→apply within the same scan generation)
- [x] T007 Ensure `refuse_by_policy` gains **no** new syscalls; attach meta only on lanes with existing finding evidence

**Checkpoint**: Types compile; refuse_by_policy syscall-free oracle still green.

---

## Phase 3: User Story 1 — Actionable Refusal Metadata (P1) 🎯

**Goal**: Withheld reads carry `_meta["symforge/withheld"]` without leaking bytes.  
**Independent Test**: quickstart oracles 1–3.

- [x] T008 [P] [US1] RED oracle `admission_refusal_carries_withheld_meta_without_secret_bytes`
- [x] T009 [P] [US1] RED oracle `clean_read_omits_withheld_meta`
- [x] T010 [US1] Attach withheld meta at admission refusal seam(s) using live findings + line ranges
- [x] T011 [US1] Path-rule / unscanned honest availability (no invented content ids)
- [x] T012 [US1] Update `format.rs` docs that currently claim "offers no remedy" to point at the separate write tool (no self-service bypass in refusal text)

**Checkpoint**: US1 oracles GREEN; secret string absent from serialized results.

---

## Phase 4: User Story 2 — Preview + Externalize (P1) 🎯 MVP with US1

**Goal**: `secret_remediate` preview/apply externalize with masked diff, atomic writes, re-scan report.  
**Independent Test**: quickstart oracles 4–6.

- [x] T013 [P] [US2] RED oracles for preview-no-write, apply+rescan, idempotent/rollback
- [x] T014 [US2] Implement tool handler skeleton in `src/protocol/secret_remediate.rs` (preview default true)
- [x] T015 [US2] Externalize rewriter for JS/TS, Python, Rust, `${X}` config
- [x] T016 [US2] `.env` create + `.gitignore` ensure; all paths via `resolve_repo_path`
- [x] T017 [US2] Reuse edit-lane atomic apply + idempotency patterns
- [x] T018 [US2] Honest apply report (written files, re-scan, history note)
- [x] T019 [US2] Register tool in `tools.rs` + verify-tools pins

**Checkpoint**: Externalize MVP demoable; US1+US2 green.

---

## Phase 5: User Story 3 — Encrypt (P2)

**Goal**: SOPS/age encrypt with public recipient; unavailable when missing.  
**Independent Test**: quickstart oracles 7–9.

- [ ] T020 [P] [US3] RED oracles unavailable / apply / spawn guard
- [ ] T021 [US3] Detect `sops` + recipient; mark action availability in meta + tool
- [ ] T022 [US3] Encrypt apply via `hidden_command` only; supported formats only
- [ ] T023 [US3] CI strategy: skip apply oracle when `sops` absent but keep unavailable oracle required

**Checkpoint**: Encrypt path honest about availability.

---

## Phase 6: User Story 4 — Dismiss (P2)

**Goal**: Content-digest-bound dismissal without the five rejected allowlist modes.  
**Independent Test**: quickstart oracles 10–13.

- [ ] T024 [P] [US4] RED oracles digest bind / revocation / symlink+size / health
- [ ] T025 [US4] Dismissal store format + loader invariants
- [ ] T026 [US4] Integrate dismissal into admission decision (read lane, not refuse_by_policy syscalls)
- [ ] T027 [US4] Ensure refusal text still offers no self-service bypass

**Checkpoint**: All US4 oracles green.

---

## Phase 7: Polish & Cross-Cutting

- [ ] T028 [P] Prompt-injection / write-gate documentation + oracle 14 strategy
- [ ] T029 Scope refusals oracle 15; syscall-free oracle 16
- [x] T030 FULL_SOURCE_PIN_V1 refresh if `src/` changes require it (after rustfmt)
- [ ] T031 Run full HANDOVER §4 gates; open implementation PR (do not merge without LINUS)

---

## Dependencies & Execution Order

- Phase 0 (spec) → review/merge of docs PR (Grove/LINUS process)
- Phase 1–2 block all implementation stories
- US1 before or parallel-hard with US2 (US2 needs finding ids from US1)
- US3 and US4 after US2 tool skeleton (T014)
- Polish last

## Parallel opportunities

- RED oracle authoring tasks marked [P] within a story
- US3/US4 after T014 can proceed in parallel if staffed

## Implementation strategy

1. Land docs PR (this track).
2. Implementation PR1: US1+US2 MVP.
3. Implementation PR2: US3+US4 (or same PR if small enough for review).
4. Never merge without independent review per HANDOVER §1 / Constitution VI.
