# Tasks: SymForge Harness Onboarding & Config Hub (v8 8.1)

**Feature dir**: `specs/005-v8-harness-onboarding/`. Inputs: [plan.md](./plan.md), [spec.md](./spec.md).
**Gates each phase**: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`. Verify against fixture configs only — never mutate real user configs.

## Phase 1: Setup
- [ ] T001 Create `tests/fixtures/harness/` with sample client configs: `claude_populated.json`, `claude_empty.json`, `claude_stale_symforge.json`, `codex.toml`, `gemini_settings.json`, `malformed.json`, `bom_utf8.json`.
- [ ] T002 [P] Read `src/cli/init.rs` `register_*_mcp_server` functions; document each known client's config path + attach-entry shape in `specs/005-v8-harness-onboarding/research.md` (replace pointers) and `data-model.md` (HarnessTarget catalog).

## Phase 2: Foundational — HarnessRegistry
- [ ] T003 Implement `src/cli/harness.rs`: `HarnessTarget { id, config_path(s), Format(Json{pointer}|Toml|GeminiSettings), attach-entry builder }` and `HarnessRegistry::known()` populated from the existing `init.rs` client knowledge (factor, don't duplicate — call into shared helpers).
- [ ] T004 Add `HarnessRegistry::scan() -> Vec<HarnessStatus>` reporting per client: NotInstalled / Absent / PresentCurrent / PresentStale (compare existing SymForge entry's URL+key to the target attach entry). BOM-safe read via the existing `init.rs` parser.
- [ ] T005 [P] Unit tests for scan against fixtures (each status reachable).

## Phase 3: US1 — Hands-free attach (P1) 🎯 MVP
- [ ] T006 [US1] Implement add/refresh-in-place + de-dup of SymForge entries per `Format` in `src/cli/harness.rs` (pure function: config-in → config-out, no I/O), preserving the client's structure/format.
- [ ] T007 [P] [US1] Tests: absent→added; stale→refreshed (no dup); double-entry→de-duped; valid output per client format; on fixtures.
- [ ] T008 [US1] Wire `init --scan` (report) and `init --scan --apply <serve-url> <key>` in `src/cli/init.rs`/`mod.rs` to scan + (apply path) call the writer (Phase 4).

## Phase 4: US2 — Safe writes: backup, dry-run, idempotent (P2)
- [ ] T009 [US2] Implement `src/cli/harness_apply.rs`: `plan(targets, attach) -> ApplyPlan` (dry-run report, no I/O); `apply(plan)` = for each change, write timestamped backup `<config>.<ts>.bak` THEN atomic-write new content; `restore(backup)`; idempotency check (skip when PresentCurrent).
- [ ] T010 [P] [US2] Tests `tests/harness_apply_backup.rs`: dry-run writes nothing; apply creates backup; restore reproduces prior bytes exactly; second apply is a no-op; malformed/permission-denied target is reported and does not abort the run or corrupt the file.
- [ ] T011 [US2] **GATE** repo gates green; manual fixture walkthrough (scan → dry-run → apply → restore).

## Phase 5: US3 — First-run / post-update onboarding (P3)
- [ ] T012 [US3] Implement `src/cli/onboarding.rs`: `OnboardingState { last_shown_version }` persisted as JSON in the SymForge data dir; `maybe_show_banner(current_version, attach_url)` shows once per version (and an open-browser offer), records state.
- [ ] T013 [P] [US3] Tests `tests/onboarding_state.rs`: fresh→shows+records; same version→suppressed; version change→re-surfaces. (Inject state path + version; no real browser.)
- [ ] T014 [US3] Hook `maybe_show_banner` into the install/update and `serve` startup paths (banner only — no behavior change to serve itself).

## Phase 6: Polish
- [ ] T015 [P] Update `docs/v8-gap-closure-plan.md`: mark G-040, G-041 closed; link this feature.
- [ ] T016 [P] Add onboarding/scan usage to `docs/stel-server.md` (or a new operator doc).
- [ ] T017 **GATE** Final: all repo gates green; embed build clean; fixture-based acceptance for SC-001..006 recorded in `specs/005-v8-harness-onboarding/validation.md`. No push/merge without human approval.

## Dependencies
```text
Setup(T001-2) → Registry(T003-5) → US1(T006-8) → US2(T009-11) ; US3(T012-14) independent after Setup ; Polish(T015-17) last
```

## Out of scope
GUI/`/api/v1` (006), AAP panel (007), per-harness distinct keys / rotation / hashed key store (006). No new STEL tools.
