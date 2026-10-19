# Tasks: Containment and Durability Hardening

**Input**: Design documents from `/specs/033-containment-durability/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Required (spec FR-013, FR-014, constitution II). Every story has RED oracles with a positive control.

**Module placement (ponytail override of research.md new-file suggestion):** do **not** add crate-root modules (`bind_policy.rs`, `fs_persist.rs`). Those require `src/internals.rs` + `src/lib.rs` re-exports and Feature 020 darkness-test T051 pin churn. Put helpers in existing modules:
- no-follow: `src/paths.rs`
- atomic bytes: `src/idempotency.rs` next to `write_record_atomic` (`pub(crate) fn write_bytes_atomic`)
- loopback predicate: `pub(crate) fn is_loopback_bind_host` in `src/paths.rs`; daemon and sidecar call it

**Worktrees:** after Phase 2 commit, fan-out from that SHA. Disjoint `OWNS` as in research.md §7. Commit after each story. Do not mix US1 and US2. Recompute Feature 020 darkness-test T051 pins only in Phase 12 on the integration tree.

**Delivery:** behavior-changing 033 work lands as **one PR** after T050–gates. Story commits stay on `033-containment-durability`. Do not push until the operator asks.

**Analyze 2026-09-18:** Phase 2 helpers already exist on HEAD. Do not re-add them. Remaining RED is story lanes that still follow, bind, persist in-place, panic, or lie in docs.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no incomplete deps)
- **[Story]**: US1–US9
- File path in every task

## Phase 1: Setup

**Purpose**: Confirm branch and docs; no new project scaffold

- [x] T001 Confirm branch `033-containment-durability` and feature docs under `specs/033-containment-durability/` including contracts and GATES.md

---

## Phase 2: Foundational (BLOCKS all user stories) — DONE on HEAD

**Purpose**: Shared helpers in existing modules. Observed done: `path_must_be_regular_non_link`, `metadata_is_symlink_or_reparse`, `write_bytes_atomic`, `paths::is_loopback_bind_host`. Receipt: already green; do not pretend RED.

- [x] T002 Verify `path_must_be_regular_non_link` and `metadata_is_symlink_or_reparse` in `src/paths.rs` (do not duplicate)
- [x] T003 Verify unit tests in `src/paths.rs` for regular file (positive) vs symlink (and `cfg(windows)` junction via `mklink /J` as in `tests/physical_root_lease_v11.rs`)
- [x] T004 Verify `pub(crate) fn write_bytes_atomic` in `src/idempotency.rs` (tmp + flush + `sync_all` + persist + Unix parent `sync_all`) and the interrupt-leaves-previous test
- [x] T005 Verify `is_loopback_bind_host` is `pub(crate)` in `src/paths.rs` (daemon delegates); keep existing daemon bind tests in `src/daemon.rs` green
- [x] T006 Ran helper tests plus `test_daemon_bind_policy` via `cargo test --lib --bins --tests -- --test-threads=1` in `src/paths.rs` / `src/idempotency.rs` / `src/daemon.rs`; committed foundational helpers

**Checkpoint**: Helpers exist. Story machinery that is still defective is US1–US9 below.

---

## Phase 3: User Story 1 — Outside-workspace bytes never enter retrieval (P1) — F-001 🎯 MVP

**Goal**: Delete freshen join-fallback; no-follow on disk admission, worktree text, observe-disk, watcher freshen, stable read, reindex scout.

**Worktree OWNS**: `src/protocol/read_gate.rs`, `src/protocol/tools.rs` (freshen helper only), `src/sidecar/handlers.rs` (freshen helper + source-read tests), `src/watcher/mod.rs` (freshen stat/reindex open), `src/live_index/store.rs` (`read_pass` + `path_after` stamp only), `src/discovery/mod.rs` (`scout_single_path` metadata reader only), plus new tests in those files.

**Independent Test**: Temp repo, secret outside root, in-repo file symlink (Windows also junction). `get_file_content`, untracked `search_text`, sidecar `/workflows/source-read` after freshen do not disclose. Ordinary file still served.

### Tests (RED first)

- [x] T007 [P] [US1] RED in `src/protocol/tools.rs`: `safe_repo_path_for_freshen` join-fallback — symlink-out must not resolve to a readable outside path; ordinary relative file still resolves (positive)
- [x] T008 [P] [US1] Tests in `src/protocol/read_gate.rs` that `admit_disk_read` / `admit_worktree_text` refuse symlink/reparse, with a regular-file positive control. Machinery already calls `path_must_be_regular_non_link`; if these tests pass on HEAD, receipt already-green and do not revert
- [x] T009 [P] [US1] RED `outbound_link` tests in `src/protocol/tools.rs` and `src/sidecar/handlers.rs`: `get_file_content` + untracked `search_text` + sidecar `/workflows/source-read` after freshen; secret absent **and not indexed**. Unix file symlink. Windows `mklink /J` from in-repo dir to the secret’s parent, then `junc/secret` (junctions cannot target files). Ordinary in-repo file still returned (FR-013)
- [x] T010 [US1] Receipt in the same test modules as T007–T009: T007/T009 failing on current join-fallback/follow lanes before machinery; T008 already-green if HEAD already refuses at admit

### Implementation

- [x] T011 [US1] Delete `Err(_) => Ok(canon_root.join(relative))` in `src/protocol/tools.rs` `safe_repo_path_for_freshen` and the same shape in `src/sidecar/handlers.rs` `safe_sidecar_path_for_freshen`
- [x] T012 [US1] Call `path_must_be_regular_non_link` from `admit_disk_read` and replace `is_file()` in `admit_worktree_text` in `src/protocol/read_gate.rs`; delete the ponytail “never-follow is elsewhere” comment at `observe_disk_beneath` (observe-disk is covered by admit)
- [x] T013 [US1] Watcher freshen in `src/watcher/mod.rs` stats/opens via no-follow; skip/refuse links instead of `fs::metadata` follow. Nested `junc/secret` must run `safe_repo_path` (or equivalent canonicalize containment) before join+stat so a directory junction to an outside tree cannot present as a regular file
- [x] T014 [US1] `FilesystemStableReadAccess::read_pass` in `src/live_index/store.rs` refuses links before `File::open`; `path_after` uses the opened file’s metadata or `symlink_metadata`, never following `fs::metadata`. `scout_single_path` in `src/discovery/mod.rs` uses `symlink_metadata` / no-follow, not `std::fs::metadata`. `admit_worktree_text` in `src/protocol/read_gate.rs` must refuse nested paths that canonicalize outside the workspace (`safe_repo_path`), not only a leaf reparse
- [x] T015 [US1] Re-run T007–T009 in `src/protocol/tools.rs`, `src/protocol/read_gate.rs`, `src/sidecar/handlers.rs` until green; existing path tests still pass. Commit US1 only. Filter name: `outbound_link`

**Checkpoint**: F-001 closed independently. Do not include sidecar bind changes.

---

## Phase 4: User Story 2 — Sidecar non-loopback refuse (P2) — F-002

**Goal**: Sidecar uses `crate::paths::is_loopback_bind_host`; sibling opt-in env; token required off-loopback; Bearer on the whole non-loopback router.

**Worktree OWNS**: `src/sidecar/server.rs`, `src/sidecar/router.rs`, sidecar tests in those files. May *call* `crate::paths::is_loopback_bind_host` without rewriting daemon bind tests.

**Independent Test**: `SYMFORGE_SIDECAR_BIND=0.0.0.0` without opt-in fails spawn; with opt-in+token warns and starts; loopback starts without new token.

### Tests (RED first)

- [x] T016 [US2] RED: sidecar bind tests in `src/sidecar/server.rs` named `sidecar_bind`, mirroring `test_daemon_bind_policy_rejects_non_loopback_without_opt_in` / allow-with-opt-in; plus opt-in without `SYMFORGE_SIDECAR_TOKEN` fails; empty token counts as missing; loopback positive control (starts, no Bearer) **including when `SYMFORGE_SIDECAR_TOKEN` is set** — HTTP without `Authorization` still 200, not 401
- [x] T017 [US2] Receipt T016 failing in `src/sidecar/server.rs` before machinery

### Implementation

- [x] T018 [US2] `spawn_sidecar` in `src/sidecar/server.rs` resolves host via `crate::paths::is_loopback_bind_host` + `SYMFORGE_SIDECAR_ALLOW_NON_LOOPBACK`; refuse non-loopback without opt-in; refuse non-loopback without token; on opt-in+token MUST start and MUST record a warning
- [x] T019 [US2] When the listener is `NonLoopbackAuthenticated`, attach Bearer middleware to the whole router in `src/sidecar/router.rs` (every route). Optional documented `/health` exemption only; `/workflows/source-read` MUST be gated. Loopback has no Bearer layer. Do not copy daemon always-on auth
- [x] T020 [US2] Re-run T016 green in `src/sidecar/server.rs` / `src/sidecar/router.rs`. Commit US2 only (not US1 files)

**Checkpoint**: F-002 closed. Loopback hooks unchanged.

---

## Phase 5: User Story 3 — Trust hasher skips reparse (P2) — F-003

**Goal**: `collect_config_dir` does not recurse junctions/symlinks.

**Worktree OWNS**: `src/edit_safety/trust.rs` hasher walk only until US7 in the same worktree (sequential: US3 then US7).

**Independent Test**: Windows junction under `.symforge/config` pointing outside; hash input excludes target; Untrusted + warning. Unix symlink-to-dir skipped. Regular config file hashed (positive).

### Tests (RED first)

- [x] T021 [US3] RED: hasher test `trust_config_reparse` in `src/edit_safety/trust.rs` — junction/symlink under config not included; warning; Untrusted; positive regular file
- [x] T022 [US3] Receipt T021 failing in `src/edit_safety/trust.rs` before machinery

### Implementation

- [x] T023 [US3] In `collect_config_dir` in `src/edit_safety/trust.rs`, after `symlink_metadata`, skip recurse/hash when `metadata_is_symlink_or_reparse`; push warning; fail-closed Untrusted. Inline the predicate; do not add `config_entry_is_unsafe_to_walk`
- [x] T024 [US3] Re-run T021 green in `src/edit_safety/trust.rs`. Do not yet change `write_store` (US7)

**Checkpoint**: F-003 closed. Trust persist still in-place until US7.

---

## Phase 6: User Story 4 — Snapshot/artifact fsync (P2) — F-004

**Goal**: `sync_all` before rename on snapshot and artifact; artifact metadata atomic.

**Worktree OWNS**: `src/live_index/persist.rs` `write_snapshot` and `export_artifact` only.

**Independent Test**: helper observes sync before rename; existing checkpoint round-trips pass.

### Tests (RED first)

- [x] T025 [US4] RED `snapshot_sync_all` in `src/live_index/persist.rs`: observed `sync_all`-before-rename (spy, failpoint, or wrapper that records the sequence). A comment is not an oracle. Positive: completed persist leaves the new bytes. Must fail today because `std::fs::write`+rename has no `sync_all`
- [x] T026 [US4] Receipt T025 failing in `src/live_index/persist.rs` before machinery

### Implementation

- [x] T027 [US4] `write_snapshot` in `src/live_index/persist.rs`: write tmp via `File`, `sync_all`, rename; Unix parent sync
- [x] T028 [US4] `export_artifact` in `src/live_index/persist.rs`: same for zst; `artifact.json` via `write_bytes_atomic` in `src/idempotency.rs`
- [x] T029 [US4] Existing `checkpoint_now(verify_after_write=true)` persist tests in `src/live_index/persist.rs` still pass. Commit US4. Filter: `snapshot_sync_all`

**Checkpoint**: F-004 closed.

---

## Phase 7: User Story 5 — Panic-free splice/diff (P2) — F-005

**Goal**: `apply_splice` and `extract_symbols_for_diff` return/skip on illegal ranges.

**Worktree OWNS**: `src/protocol/edit.rs` (`apply_splice` + production call sites in that file), `src/protocol/edit_tools.rs` (direct `apply_splice` and builders that become `Result`), `src/parsing/mod.rs` (`extract_symbols_for_diff`). Callers of `extract_symbols_for_diff` in `src/protocol/format.rs` and `src/protocol/tools.rs` (`detect_impact`) treat skip like today’s `None` — edit those sites only if the signature forces it.

**Independent Test**: `end > len`, `end < start`, non-char-boundary start on multibyte `&str` — Err/skip, no panic. Valid splice positive control.

### Tests (RED first)

- [x] T030 [P] [US5] RED: `apply_splice` tests `splice_bounds` in `src/protocol/edit.rs` for inverted, OOB, and a valid insert (`start == end`)
- [x] T031 [P] [US5] RED: `extract_symbols_for_diff` test in `src/parsing/mod.rs` with a crafted symbol range on a multibyte string that would panic on `&source[mid_char..]`; a valid range still hashes (positive)
- [x] T032 [US5] Receipt T030–T031 failing (panic or wrap) in `src/protocol/edit.rs` and `src/parsing/mod.rs` before machinery

### Implementation

- [x] T033 [US5] `apply_splice` in `src/protocol/edit.rs` returns `Result<Vec<u8>, String>`; update call sites in `src/protocol/edit.rs` (`build_insert_before`, `build_insert_after`, `build_delete`, `build_edit_within`, `execute_batch_edit`, `execute_batch_rename`, `execute_batch_insert`) and `src/protocol/edit_tools.rs` (`replace_symbol_body`, `insert_symbol` via builders, `delete_symbol` via builders, `edit_within_symbol`)
- [x] T034 [US5] `extract_symbols_for_diff` in `src/parsing/mod.rs` checks bounds and `is_char_boundary`; skip illegal symbol ranges. If the return type changes, map skip like `None` in `src/protocol/format.rs` and `src/protocol/tools.rs`
- [x] T035 [US5] Re-run T030–T031 green in `src/protocol/edit.rs` and `src/parsing/mod.rs`; existing splice tests still pass. Commit US5. Filter: `splice_bounds`

**Checkpoint**: F-005 closed.

---

## Phase 8: User Story 6 — Windows token DACL (P2) — F-006

**Goal**: Owner-only DACL on daemon token file; Unix 0o600 unchanged.

**Worktree OWNS**: `src/daemon.rs` `write_daemon_token_file_at` and its tests; `Cargo.toml` windows features if required.

**Independent Test**: After write, Windows DACL has no Everyone/Users read ACE. Unix 0o600 positive.

### Tests (RED first)

- [x] T036 [US6] RED `daemon_token_acl` in `src/daemon.rs`: `cfg(windows)` after `write_daemon_token_file_at` inspecting DACL (including after replacing an existing token file); Unix mode test remains 0o600 (positive)
- [x] T037 [US6] Receipt T036 failing in `src/daemon.rs` on default `fs::write` ACL before machinery (Windows)

### Implementation

- [x] T038 [US6] Set owner-only DACL in `write_daemon_token_file_at` in `src/daemon.rs` using the existing `windows` crate after every write including replace; add `Win32_Security` features in `Cargo.toml` only if required. Token file is not required to use `write_bytes_atomic`
- [x] T039 [US6] Re-run T036 green on Windows in `src/daemon.rs`; Unix 0o600 still asserted. Commit US6. Filter: `daemon_token_acl`

**Checkpoint**: F-006 closed.

---

## Phase 9: User Story 7 — Atomic trust store (P2) — F-007

**Goal**: `write_store` / `write_cli_store` use `write_bytes_atomic`.

**Worktree OWNS**: `src/edit_safety/trust.rs` persist, `src/cli/trust.rs`. Same worktree as US3; run after T024.

**Independent Test**: Crash-before-rename leaves previous `trust.json`. Load still fail-closed on corrupt. Completed persist is the new bytes (positive).

### Tests (RED first)

- [x] T040 [US7] RED `trust_store_atomic` in `src/edit_safety/trust.rs` and/or `src/cli/trust.rs`: interrupting before replace keeps previous JSON parseable; completed persist is new bytes (positive)
- [x] T041 [US7] Receipt T040 failing on in-place `fs::write` in `src/edit_safety/trust.rs` / `src/cli/trust.rs`

### Implementation

- [x] T042 [US7] `write_store` in `src/edit_safety/trust.rs` and `write_cli_store` in `src/cli/trust.rs` call `write_bytes_atomic` from `src/idempotency.rs`
- [x] T043 [US7] Re-run T040–T021 still green in `src/edit_safety/trust.rs` and `src/cli/trust.rs` (hasher + persist). Commit US7

**Checkpoint**: F-003 and F-007 closed in one worktree, two commits preferred (US3 then US7).

---

## Phase 10: User Story 8 — Atomic init client configs (P2) — F-008

**Goal**: Production `cli/init.rs` writers use `write_bytes_atomic`. Test fixtures stay `fs::write`.

**Worktree OWNS**: `src/cli/init.rs` production write sites (as of plan date: `merge_hooks_into_settings`, `register_mcp_server`, `register_claude_desktop_mcp_server_with_home`, `create_desktop_wrapper_windows`, `register_codex_mcp_server`, `register_grok_mcp_server`, Gemini/Kilo/Cursor registers, markdown guidance writer — verify at implement time; leave `#[cfg(test)]`).

**Independent Test**: Interrupted init leaves previous editor JSON parseable. Existing init tests pass. Completed persist is new bytes (positive).

### Tests (RED first)

- [x] T044 [US8] RED `init_atomic_write` in `src/cli/init.rs`: interrupted replace leaves previous JSON parseable (same bar as T040). A helper-name assert is not enough. Positive: completed persist is the new bytes
- [x] T045 [US8] Receipt T044 failing on in-place write in `src/cli/init.rs`

### Implementation

- [x] T046 [US8] Replace production `std::fs::write` in `src/cli/init.rs` (non-test) with `write_bytes_atomic`; leave `#[cfg(test)]` fixtures
- [x] T047 [US8] Existing init tests in `src/cli/init.rs` green. Commit US8. Filter: `init_atomic_write`

**Checkpoint**: F-008 closed.

---

## Phase 11: User Story 9 — Honest lifecycle docs (P3) — F-009

**Goal**: Parent module docs name live call edges. Submodule darkness kept only if still true. Feature 020 darkness-test T051 pin refresh is Phase 12 (merge US9 docs with pin refresh on integration so a pushed T049-alone cannot be CI-red).

**Worktree OWNS**: `src/index_lifecycle/mod.rs` docs, and submodule docs that are stale (`observer.rs`, `query.rs` if production type uses exist). Do not edit `tests/preventive_runtime_dark_v11.rs` here.

**Independent Test**: Parent docs no longer claim production has no callers. Submodules that remain uncalled may keep the phrase.

### Tests / docs

- [x] T048 [US9] Rewrite darkness paragraph in `src/index_lifecycle/mod.rs` to name `project_source_authority` / confined-write edges. Rewrite `src/index_lifecycle/observer.rs` and `src/index_lifecycle/query.rs` if they still claim production darkness while `ObserverId` / `HealthProjection` are used from watcher/persist/health_view. Keep the phrase on `candidate.rs` / `supervisor.rs` / `verification.rs` only if still accurate
- [x] T049 [US9] Confirm `rg "Nothing in production calls this module" src/index_lifecycle/mod.rs` is empty; submodule hits only if still accurate. Commit US9 docs in `src/index_lifecycle/`. Do not push this commit alone; pin refresh is T060 on integration

**Checkpoint**: Docs honest. Suite not fully green until pins.

---

## Phase 12: Polish & integration

**Purpose**: Merge worktrees, recompute seals, full Principle IV gates, embed cell. Land as one PR.

- [x] T050 Merge all story worktrees onto `033-containment-durability` (git worktree paths under `.worktrees/033-us*`); resolve non-pin conflicts by judgment (never blind `--theirs`)
- [x] T060 After `cargo fmt` on the integration tree, recompute `EXCLUDED_RUNTIME_SOURCE_PIN_V1` and `FULL_SOURCE_PIN_V1` in `tests/preventive_runtime_dark_v11.rs` via the rust oracle only (Feature 020 darkness test T051 must pass after this; 033 task id is T060)
- [x] T061 `cargo fmt --check` (repo rustfmt via `Cargo.toml`)
- [x] T062 `cargo clippy --all-targets -- -D warnings` with constitution `-j 4` (one cargo at a time)
- [x] T063 `cargo test --lib --bins --tests -- --test-threads=1` via Terminal Commander if long (`Cargo.toml` suite)
- [x] T064 `cargo test --no-default-features --features embed --lib -- --test-threads=1` (after T063, not interleaved)
- [x] T065 Adversarial review recorded under `docs/reviews/` (cfg-lens: unix 0o600, windows DACL, windows junction, sidecar Bearer only off-loopback)
- [x] T066 Mark evidence in `specs/033-containment-durability/GATES.md`; `cargo clean` after the heavy session; remove story worktrees under `.worktrees/033-us*`
- [x] T067 `cargo build --release` then `node scripts/verify-tools.cjs --bin target/release/symforge`
- [x] T068 `npm test` in `npm/package.json` (`cd npm && npm test`)

---

## Dependencies & Execution Order

### Phase Dependencies

- Phase 1 → Phase 2 (foundational commit) **blocks** all stories
- US1–US9 after Phase 2, in **separate worktrees** except US3→US7 sequential in one worktree
- US9 docs before Feature 020 T051 pin refresh (033 T060)
- Phase 12 after all stories merged

### User Story Dependencies

- US1: foundational paths helper
- US2: foundational `is_loopback_bind_host` in `src/paths.rs`
- US3: foundational reparse helper
- US4: foundational `write_bytes_atomic` (artifact.json)
- US5: none beyond crate compile
- US6: `windows` crate features
- US7: after US3 in same files; `write_bytes_atomic`
- US8: `write_bytes_atomic`
- US9: docs only; pins in Phase 12

### Parallel after T006

```text
wt-f001: T007–T015
wt-f002: T016–T020
wt-f003-007: T021–T024 then T040–T043
wt-f004: T025–T029
wt-f005: T030–T035
wt-f006: T036–T039
wt-f008: T044–T047
wt-f009: T048–T049
```

Then T050–T068 on integration.

## Parallel example: after foundational

```text
Task rust-pro worktree .worktrees/033-us1  OWNS US1 files  T007–T015
Task rust-pro worktree .worktrees/033-us2  OWNS US2 files  T016–T020
Task rust-pro worktree .worktrees/033-us4  OWNS persist.rs T025–T029
Task rust-pro worktree .worktrees/033-us5  OWNS edit/parse T030–T035
Task rust-pro worktree .worktrees/033-us6  OWNS daemon token T036–T039
Task rust-pro worktree .worktrees/033-us8  OWNS cli/init.rs T044–T047
Task rust-pro worktree .worktrees/033-us3  OWNS trust.rs sequential US3 then US7
Task rust-pro worktree .worktrees/033-us9  OWNS index_lifecycle docs T048–T049
```

## Implementation Strategy

### MVP

Phase 1 + 2 + US1 (F-001). Stop and validate containment before other stories if time-boxed.

### Incremental

Each worktree commits its story. Integration merges, pins, full gates. One PR.

### Suggested MVP scope

US1 only is the confidentiality MVP. This campaign implements **all nine** findings.

## Notes

- Commit after each story (user request) and after foundational.
- Rejected review items R-001–R-007 stay untouched.
- No Bearer on loopback sidecar.
- Do not fix `select_runtime_data_base`.
- Embed unused-import trap on new non-test helpers in `src/paths.rs` / `src/idempotency.rs` (always compiled — good).
- `apply_splice` signature change is US5-only; do not drive-by reformat `edit.rs`.
- CHECK parallelism is constitution `-j 4`. CLAUDE.md’s host `-j 8` note is not the campaign CHECK.
- Ponytail ceiling: metadata-then-read, not `O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT`, unless a test shows a swap.

## Phase 13: Convergence

- [x] T069 [US4] Add a `cfg(test)` failpoint so a simulated `sync_all` failure in `write_tmp_file_synced` (`src/live_index/persist.rs`) returns `Err`, does not rename, and leaves previous dest bytes; filter `snapshot_sync_all`. Positive control remains `snapshot_sync_all_is_observed_before_rename`. Refresh `FULL_SOURCE_PIN_V1` via the rust oracle only if that source set changes. per SC-004 (`partial`)
