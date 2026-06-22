---
description: "Task list for STEL Predictor Calibration implementation"
---

# Tasks: STEL Predictor Calibration

**Input**: Design documents from `specs/013-stel-predictor-calibration/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: REQUESTED — the spec mandates the named regression tests (`stel_ledger_persistence`,
`stel_calibration_tuning`, `surface_honesty`, `stel_golden_replay`) plus the per-story gate.
Test tasks are included and written BEFORE the fix where practical (red → green).

**Organization**: by user story. Foundational lands the durable-layer primitives the stories
consume; then US1=Phase 3 (durable ledger reaches stdio/embed), US2=Phase 4 (predictor learns
from observed error), US3=Phase 5 (calibration state reported honestly). Each story is an
independently shippable increment; the full gate runs after each.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different file, no dependency on an incomplete task)
- **[Story]**: US1–US3; Setup/Foundational/Polish carry no story label
- Anchors (file:line) are live as of authoring; **re-confirm against live code at use** (line
  numbers drift). Harness MCP may be any version; correctness is proven by `cargo`.

## Path Conventions

Single Rust crate `symforge`; sources under `src/`, tests under `tests/`, docs under `docs/`.

## Single-owner decisions (resolved before task numbering — read first)

These were contradictions across the draft slices; resolved here so no two tasks fight:

- **Durable store is EMBED-REACHABLE.** FR-001 requires durability in stdio/embed, so the
  durable store is un-gated to `any(feature="server", feature="embed")` (rusqlite is already an
  UNCONDITIONAL dep — Cargo.toml:110 — so this adds zero new deps and pulls in no server/network
  stack). The earlier "prove the gate must NOT move" stance is rejected. Owner: **US1 (T018)**.
- **Schema v2 migration + `estimator_version` column**: single owner = **Foundational (T009)**.
  US1/US2/US3 depend on it; no story re-migrates.
- **Bounded prune-on-write retention (`LEDGER_RETENTION_MAX`)**: single owner =
  **Foundational (T011)**. Closes P3-B (ledger_store.rs:39). No story re-adds it.
- **`store_active_tuning` / `load_active_tuning` / `samples_for_estimator`**: storage primitives
  owned by **Foundational (T012, T013)**. US2 owns only the derivation/validation math that
  consumes them.
- **Drop `calibration_auto_tune` from `DEFERRED_ITEMS` (status.rs:19)**: single owner = **US2
  (T038)** — only once the `tuned` state is actually reachable. US3 does NOT re-edit it.
- **FR-011 reset/clear**: explicit owner = **US2 (T037)**, delivered as an MCP tool mode (not
  injected context), with a test that state returns to `Deferred` after reset.

---

## Phase 1: Setup (Baseline — goal PHASE 0)

**Purpose**: establish a green baseline and re-confirm anchors before touching code.

- [ ] T001 Capture baseline green gate at HEAD on branch `013-stel-predictor-calibration` BEFORE any edit: run `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, `cargo build --no-default-features --features embed --lib`, `cargo clippy --no-default-features --features embed --lib -- -D warnings`; record pass/fail in a scratch note. Do NOT blanket-kill `symforge*` (the user's live MCP is a global binary; cargo builds to `target/`, no conflict).
- [ ] T002 [P] Re-confirm live anchors against `src/` before editing: `SqliteStelLedgerStore::open(&Path, session_id)` ledger_store.rs:302; dir-entry `StelLedgerStore::open(dir, session_id)` ledger_store.rs:201 (degrades to `Disabled`); WAL+busy_timeout pragma ledger_store.rs:313; `record()` ledger_store.rs:391 (INSERT 418-458); SCHEMA_V1 ledger_store.rs:43-68, `CURRENT_SCHEMA_VERSION=1` :36, `migrate()` :351 (v1-only, P3-A no-forward-guard); P3-B unbounded-growth comment :39-41; `summarize_calibration` calibration.rs:30; `tuning_sufficiency_note` "auto-tuning still deferred" calibration.rs:72-80; `TUNING_REVIEW_MIN_EVENTS=5` calibration.rs:9; `estimate_economics` controller.rs:319; 400/800 floor `grounded_step_tokens` controller.rs:359-361; `COMPACT_SCHEMA_TOKENS=45`/`COMPACT_INVOKE_TOKENS=80` controller.rs:14,16 (added :331-332); `DurableLedgerState` status.rs:41; calibration section under detail:full status.rs:189; `DEFERRED_ITEMS` status.rs:19; `ledger_store` mod gate mod.rs:27-28; `stel_ledger_store` field + `with_stel_ledger_store` protocol/mod.rs:255-258,285-288,299-306; the ONLY `.with_stel_ledger_store` call serve.rs:352-355; serve durable open serve.rs:324-340; stdio local bootstrap main.rs:418-424 (+ `transport::stdio()` serve_server :426); daemon-proxy stdio main.rs:257 (`new_daemon_proxy`); inert `types::CalibrationState` (per-tool EMA) types.rs:344. Note any drift in the scratch note.
- [ ] T003 [P] Confirm `target/` warm and stays warm (no `cargo clean` until campaign end).

**Checkpoint**: anchors verified live; baseline gate green (T001) — the SC-004 zero-regression measurement starts from this known-green state.

---

## Phase 2: Foundational (Durable-layer storage primitives)

**Purpose**: the v2 schema, bounded retention, tuned-constant storage, and estimator-version
sample read path that US1/US2/US3 all depend on. BLOCKS the story phases that consume durable
state. Storage primitives ONLY — no stdio wiring (US1), no tuning math (US2), no honest-surface
rendering (US3).

**⚠️ CRITICAL**: no story phase can finish until these primitives exist. All work stays inside
`src/stel/ledger_store.rs` (server-gated module for now) plus the new test file. The existing
WAL + busy_timeout + non-blocking `record` machinery is reused, not rewritten. All schema work
is additive and migration-gated.

**Naming-collision constraint**: `types::CalibrationState` (types.rs:344) is the pre-existing
inert per-tool EMA struct; the data-model's honest `Deferred/Accumulating/Tuned` state machine
is a DIFFERENT type and lands later under a NON-colliding name (US3 `CalibrationVerdict`). The
foundation introduces NO new public surface type — only DB-layer storage primitives.

### Tests for Foundational (write FIRST — red before impl)

- [ ] T004 Write `tests/stel_ledger_persistence.rs` (server-gated, `#![cfg(feature="server")]`) asserting the v2-migration + retention + tuned-constant contract BEFORE the impl lands: (a) a fresh store opens at `schema_version==2` with an `estimator_version` column present; (b) a row written before the column-add reads back `estimator_version=='pre-013'` (backfill sentinel); (c) inserting `LEDGER_RETENTION_MAX+N` events leaves exactly `LEDGER_RETENTION_MAX` rows, newest retained, oldest pruned (monotonic newest-kept); (d) `store_active_tuning` then `load_active_tuning` round-trips an identical `TunedEstimateConstants` after reopen; (e) a second `store_active_tuning` for the same `(project, estimator_version)` REPLACES (not appends) the active set. Tests will fail to compile until T009–T013 land. (FR-001, FR-002, FR-012, SC-003)
- [ ] T005 [P] Determinism / corruption-recovery tests in `tests/stel_ledger_persistence.rs`: (a) `migrate()` is idempotent — calling it twice leaves `schema_version==2`, does not duplicate the column, and does not re-backfill non-sentinel rows; (b) a store whose open/migrate fails (corrupt/non-DB file path) degrades to `Disabled` and is NOT served (Principle IV corruption-quarantined), distinct from `Unavailable`; (c) store/load tuned constants is byte-stable across reopen (Principle IV byte-exact persistence). (Principle IV, FR-003, FR-012)
- [ ] T006 [P] Crash-durability test (constitution IV "shutdown is NOT a safe persistence boundary") in `tests/stel_ledger_persistence.rs`: record an event, then reopen the same db WITHOUT a clean `Drop`/checkpoint (drop the connection handle abruptly / force-close without a graceful shutdown path) and assert the event survived — the WAL append is durable mid-write, not only after a clean drop. (Principle IV, FR-001, SC-003)

### Implementation for Foundational

- [ ] T007 No-code constraint note: confirm `types::CalibrationState` (types.rs:344, per-tool EMA, exported mod.rs) still compiles untouched and record in `specs/013-stel-predictor-calibration/data-model.md` that the foundation adds NO new public type — only DB primitives — so the honest state machine lands later under `CalibrationVerdict` (US3), never overloading `CalibrationState`. (`cargo check` only; no edit to types.rs.) (FR-009)
- [ ] T008 [P] Add a server-gated `TunedEstimateConstants` POD to `src/stel/ledger_store.rs` (fields: `response_floor`, `manual_floor`, `schema_tokens`, `invoke_tokens`, `estimator_version`, `sample_size`, `error_before`, `error_after`, `tuned_at`). Pure type, no I/O, no derivation. (data-model TunedEstimateConstants)
- [ ] T009 [US-shared] Bump schema to v2 in `src/stel/ledger_store.rs`: add `estimator_version TEXT` to `stel_ledger_events` and backfill the `pre-013` sentinel via an idempotent ALTER guard. Edit the SCHEMA_V1 const region (ledger_store.rs:43-68) and `migrate()` (ledger_store.rs:351); raise `CURRENT_SCHEMA_VERSION` (line 36) to 2. In `migrate()`: run v1 DDL, then add the column ONLY if absent (probe `PRAGMA table_info` / catch duplicate-column), then `UPDATE stel_ledger_events SET estimator_version='pre-013' WHERE estimator_version IS NULL`, then write `schema_version=2`. Migration stays idempotent and never-panics (keep the poisoned-mutex recovery, FR-011). Update the `record()` INSERT (ledger_store.rs:418-458) to write the CURRENT estimator-version constant for new rows, NOT the sentinel. **Single owner of the migration — no story re-migrates.** (FR-001; data-model PredictionErrorSample; R3 estimator-version invalidation) (depends on T004)
- [ ] T010 [P] Persist tuned constants: add a `stel_calibration` table (single active set per `(project, estimator_version)`) to SCHEMA_V1 and the methods `store_active_tuning(&self, c: &TunedEstimateConstants) -> Result<()>` (INSERT OR REPLACE, audited per FR-008) and `load_active_tuning(&self, estimator_version) -> Result<Option<TunedEstimateConstants>>` on `SqliteStelLedgerStore`. Read/write under the existing poisoned-mutex recovery; corruption/open-failure degrades to `Disabled`, never serves a bad tuning. NO derivation/validation here (that is US2). This struct is the data store only; it must NOT bump frecency (no discovery/search call). (data-model TunedEstimateConstants persisted+audited; FR-008; FR-011) (depends on T008, T009)
- [ ] T011 Bounded prune-on-write retention in `record()`: define `pub const LEDGER_RETENTION_MAX: usize = 2000;` (data-model R2 default, count-based/deterministic). After the INSERT in `record()` (ledger_store.rs:418-458), within the SAME held connection lock, delete rows beyond the most-recent cap (e.g. `DELETE FROM stel_ledger_events WHERE id NOT IN (SELECT id FROM stel_ledger_events ORDER BY id DESC LIMIT ?cap)`), keeping the prune idempotent and non-blocking (no extra lock round-trip; reuse the poisoned-mutex recovery). Replace the P3-B deferred comment (ledger_store.rs:39-41) to record that retention now ships. `record()` stays off the request hot path — no change to its `spawn_blocking` caller contract. **Single owner of retention.** (FR-002, SC-003; R2; closes P3-B) (depends on T009)
- [ ] T012 Add a server-gated read accessor `samples_for_estimator(&self, version, limit) -> Result<Vec<StoredLedgerRecord>>` filtering `stel_ledger_events` to the current `estimator_version` (EXCLUDES `pre-013`-tagged rows from the active tuning population per R3) ordered newest-first. Pure read path, NO frecency bump (Principle V). This is the seam US2 consumes; the foundation provides the filtered read only, not the tuning math. (R3; FR-012; Principle V) (depends on T009)
- [ ] T013 Foundational green-gate: run the full rust + embed + embed-musl matrix and confirm no golden-replay / surface-honesty regression (calibration storage changed the BACKING store, not any routing/policy decision): `cargo fmt --check && cargo check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1 && cargo build --release && cargo build --no-default-features --features embed --lib && cargo clippy --no-default-features --features embed --lib -- -D warnings && cargo build --no-default-features --features embed --target x86_64-unknown-linux-musl --lib && cargo test --test stel_ledger_persistence -- --test-threads=1 && cargo test --test surface_honesty -- --test-threads=1`. Commit the Foundational slice. (SC-004; Principle VI; Principle VIII) (depends on T009, T010, T011, T012)

**Checkpoint**: durable primitives exist (v2 schema, retention, tuned-constant storage, filtered sample read); module still server-gated; `cargo check --no-default-features --features embed --lib` green; primitives proven by tests, not compilation.

---

## Phase 3: User Story 1 — Calibration data survives across stdio sessions (Priority: P1) 🎯 MVP

**Goal**: the durable, restart-surviving stel-ledger.db — today serve-only (010 FR-004) — reaches
the default STDIO/embed dispatch path. Events accumulate cumulatively across restarts (SC-003),
are bounded by the documented cap (FR-002, owned by Foundational), and degrade honestly to
in-memory `Disabled` when the store cannot open (FR-003).

**Independent Test**: run several stdio MCP sessions in sequence against the same project
`.symforge` dir across ≥3 process restarts; the calibration/ledger surface (`status detail:full`
`DurableLedgerState`) shows `total_events` CUMULATIVE (monotonic, non-reset), row count never
exceeds the cap, and a forced open failure reads `Disabled{reason}` distinguishably — never a
silent durable-accumulation claim. Encoded in `tests/stel_ledger_persistence.rs` (cross-restart
accumulation + degrade-to-Disabled + transport parity) using `StelLedgerStore::open` against a
tempdir re-opened across simulated sessions. `cargo check --no-default-features --features embed
--lib` stays green (embed isolation).

**Load-bearing constraint (Principle VI)**: today `ledger_store` is `#[cfg(feature="server")]`
(mod.rs:27-28) and the `stel_ledger_store` field + builder are server-gated (protocol/mod.rs),
so the durable store does not exist in embed. US1 un-gates the durable-open path to
`any(server, embed)` WITHOUT introducing server/network deps (rusqlite is unconditional,
Cargo.toml:110) and brings stdio↔serve to parity (Principle VII).

### Tests for User Story 1 (write FIRST — red before impl)

- [ ] T014 [P] [US1] Embed-isolation guard in `tests/stel_ledger_persistence.rs`: assert the durable ledger module + the protocol attach path COMPILE and are reachable under `--no-default-features --features embed`. Verification: `cargo check --no-default-features --features embed --lib`. (FR-001; Principle VI)
- [ ] T015 [P] [US1] Cross-session accumulation + degrade-to-Disabled tests in `tests/stel_ledger_persistence.rs`: open `StelLedgerStore::open(tempdir,..)`, record events, drop, re-open the same dir ≥3 times and assert `total_events` is cumulative/monotonic (non-reset); and assert a forced open failure (unwritable path) yields `StelLedgerStore::Disabled` with a distinguishable reason — never a panic or silent zero. (FR-001; FR-003; SC-003; Principle IV)
- [ ] T016 [P] [US1] Transport-parity test in `tests/stel_ledger_persistence.rs`: the SAME durable db opened via the dir-based entry (`StelLedgerStore::open(dir, session_id)`, ledger_store.rs:201) and reopened under a second `session_id` sees the cumulative cross-session event count (one db, one session-spanning count) — the stdio↔serve parity invariant US1 must satisfy. (Principle VII; SC-003)
- [ ] T017 [P] [US1] Frecency-non-bump test in `tests/stel_ledger_persistence.rs`: recording a durable ledger event does NOT bump discovery/search frecency (assert the frecency/discovery counter before vs after `record()`). (Principle V)

### Implementation for User Story 1

- [ ] T018 [US1] Un-gate the durable store module for embed: change `#[cfg(feature="server")] pub mod ledger_store;` (mod.rs:27-28) to `#[cfg(any(feature="server", feature="embed"))]` WITHOUT introducing server/network deps. **Sole owner of the embed-gating decision.** Verification: `cargo check --no-default-features --features embed --lib && cargo check`. (FR-001; Principle VI) (depends on T014)
- [ ] T019 [US1] Un-gate the `stel_ledger_store` field on `SymForgeServer`, the `with_stel_ledger_store` builder, and the `finalize_symforge_with_ledger` write-through so a durable handle can be held + written on the embed/stdio path: broaden the three `#[cfg(feature="server")]` cfgs at protocol/mod.rs:255-258, 285-288, 299-306 to `any(server, embed)`; keep `LedgerWriteTracker` behavior intact. Verification: `cargo check --no-default-features --features embed --lib && cargo clippy --all-targets -- -D warnings`. (FR-001; FR-003 durable handle can hold Disabled; Principle VI) (depends on T018)
- [ ] T020 [US1] Wire durable open into the LOCAL stdio bootstrap: in `run_local_mcp_server_async` (main.rs:279), before `serve_server` on `transport::stdio()` (main.rs:426), when `watcher_root` is `Some(root)` call `paths::ensure_symforge_dir(root)` then `StelLedgerStore::open(&dir, format!("stdio-{}", std::process::id()))` and attach via `.with_stel_ledger_store(Arc::new(store))` on the `SymForgeServer` built at main.rs:418-424 — mirroring serve.rs:324-355. On dir/open `Err`, log + proceed in-memory (degrade, FR-003). (FR-001; FR-003) (depends on T019)
- [ ] T021 [US1] Resolve the DEFAULT daemon-backed stdio path so the operator's real deployment is not a silent gap: the default route is `run_remote_mcp_server_async` → `new_daemon_proxy` (main.rs:223,257), and the DAEMON WORKER process (daemon.rs `new_daemon_proxy` build sites) is where dispatch actually executes tool calls — NOT the proxy. Open `StelLedgerStore::open` under the daemon's resolved project root and thread it into the worker dispatch SO durable accumulation works in the daemon default; OR conclusively document AND test (doc comment at the `new_daemon_proxy` attach site + a test) that the daemon already owns persistence. Pick one and make it explicit — the proxy must not be the only persistence story. (FR-001 no silent stdio gap; SC-001; SC-003; Principle III trust-envelope honesty) (depends on T020)
- [ ] T022 [US1] HTTP-sidecar coexistence guard in `tests/stel_ledger_persistence.rs`: the local stdio path already spawns an HTTP sidecar on the SAME project root (main.rs:408-410). Assert the durable `StelLedgerStore` opened on stdio coexists with the sidecar without double-opening / contending on `stel-ledger.db` — i.e. single-process two-opener WAL concurrency is confirmed (R4), not just the multi-process case. If a second opener is unsafe, share the single `Arc` instead. (R4; FR-001; Principle IV) (depends on T020)
- [ ] T023 [US1] Extend `tests/surface_honesty.rs` (server-gated): assert the `DurableLedgerState` rendered on the stdio path under `status detail:full` distinguishes `Durable` / `Disabled{reason}` / `Unavailable` honestly (broken vs never-configured), and that NO durable-accumulation figure is presented as measured when the store is `Disabled` (010 honesty contract, extended to the stdio durable surface). (FR-003; SC-005; Principle III) (depends on T021)

**Checkpoint — full gate**: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1 && cargo check --no-default-features --features embed --lib && cargo build --no-default-features --features embed --target x86_64-unknown-linux-musl --lib && cargo build --release`. US1 shippable alone — durable ledger reaches stdio AND the daemon default, no server deps leak into embed, no routing/golden-replay/policy regression (SC-004). MVP done. (SC-004; Principle VI; Principle VIII)

---

## Phase 4: User Story 2 — The predictor improves from observed error (Priority: P1)

**Goal**: after enough current-estimator-version samples accumulate, an auto-tune consumer
DERIVES candidate replacements for the planner's static constants (400/800 plan floors,
45/80 schema/invoke), VALIDATES each on a held-out slice NOT used to derive it, REJECTS any
candidate that does not strictly reduce mean absolute prediction error versus the constants in
force, WIRES accepted constants into `estimate_economics`, and AUDITS every applied adjustment.
Replaces the hard-coded `auto-tuning still deferred` seam (calibration.rs:72-80) with a truthful
`Deferred → Accumulating → Tuned` machine; `tuned` reads ONLY when backed by the held-out
error-reduction artifact; every served figure stays an estimate; routing/policy/safety untouched.

**Depends on**: US1 supplying durable, estimator-version-tagged samples (Foundational T009/T012
`estimator_version` column + `samples_for_estimator` + `load_active_tuning`/`store_active_tuning`).

**Independent Test**: build a deterministic in-memory corpus of `PredictionErrorSample` rows
(all current `estimator_version`) systematically biased (e.g. response tokens under-predicted by
a known factor), split into train + held-out, run `derive_tuning_candidate(train)` then
`validate_candidate(candidate, held_out, in_force)`: assert the accepted constants reduce
held-out MAE versus the static 400/800+45/80 floors AND, fed into `estimate_economics` via
`load_active_tuning`, move the predicted figure toward actuals; an unbiased corpus applies NO
candidate (state stays `accumulating`); a deliberately worse candidate is REJECTED; every applied
adjustment produced an audit record; the rendered surface reads `tuned (before% → after%)` ONLY
when the artifact exists. Commands: `cargo test --test stel_calibration_tuning -- --test-threads=1`;
plus `cargo test --test surface_honesty -- --test-threads=1` and `cargo check --no-default-features
--features embed --lib`.

**Quantitative bar**: SC-002 requires the tuned constants beat the static floors by at least the
"meaningful margin" set in plan.md/research.md (T024 sets it). The accept-path test asserts that
specific margin, not merely "strictly drops".

### Tests for User Story 2 (write FIRST — red before impl)

- [ ] T024 [P] [US2] Set the numeric SC-002 "meaningful margin" in `specs/013-stel-predictor-calibration/plan.md` (or research.md), then write `tests/stel_calibration_tuning.rs` (deterministic): a biased corpus reduces held-out MAE by AT LEAST that margin vs the static 400/800+45/80 floors (accept path, FR-005/SC-002); an unbiased corpus produces NO adjustment; a worse-than-baseline candidate is REJECTED; tuning is reproducible (same corpus → same constants). Tests fail until derive/validate land. (FR-004, FR-005, FR-012, SC-002)
- [ ] T025 [P] [US2] Version-mismatch fallback test in `tests/stel_calibration_tuning.rs`: `estimate_economics` falls back to the static 400/800+45/80 floors when the active `TunedEstimateConstants.estimator_version` does NOT match the current estimator (R3 in-force rule) — a stale-version tuned set must NOT silently apply — in addition to the present-and-matching case. (FR-006; R3 in-force rule)

### Implementation for User Story 2

- [ ] T026 [US2] Anchor the US1 dependency: confirm the `estimator_version`-tagged sample read path (Foundational `estimator_version` column + `samples_for_estimator` + `load_active_tuning`/`store_active_tuning`) exists on `ledger_store.rs` before US2 consumes it. Verification: `cargo check --features server && cargo check --no-default-features --features embed --lib`. (FR-001 precondition; precondition for FR-004) (depends on Foundational T009/T010/T012)
- [ ] T027 [US2] Add the `CalibrationVerdict` state machine + reuse `TunedEstimateConstants` in `src/stel/calibration.rs` (`Deferred` / `Accumulating { n, min }` / `Tuned { sample_size, error_before, error_after }`); pure types, no I/O. Name it distinctly from the inert `types::CalibrationState` (per-tool EMA, types.rs:344) to avoid the collision. (FR-009, FR-010) (depends on T024)
- [ ] T028 [US2] Implement pure deterministic `derive_tuning_candidate(samples) -> Option<TunedEstimateConstants>` in `calibration.rs`: derive candidate `response_floor`/`manual_floor`/`schema_tokens`/`invoke_tokens` from accumulated predicted-vs-actual error of CURRENT-estimator-version samples only, respecting the minimum-sample gate (`TUNING_REVIEW_MIN_EVENTS` / documented minimum). (FR-004, FR-012) (depends on T027)
- [ ] T029 [US2] Implement pure `validate_candidate(candidate, held_out, in_force) -> bool` in `calibration.rs`: compute held-out MAE for candidate vs constants currently in force; return `true` ONLY if held-out MAE strictly drops (by the SC-002 margin from T024). This is the REJECT gate — non-improving and worse candidates return `false`. (FR-005, SC-002) (depends on T028)
- [ ] T030 [US2] Replace the deferred seam: rewrite `tuning_sufficiency_note` / `summarize_calibration` (calibration.rs:72-80) to compute the real `CalibrationVerdict` (deferred/accumulating/tuned) via `derive_tuning_candidate` + `validate_candidate` over a held-out split, instead of the hard-coded `auto-tuning still deferred` string; add bounded-step + hysteresis so re-tuning converges, not oscillates. (FR-004, FR-005, FR-009) (depends on T029)
- [ ] T031 [US2] Persist + audit accepted constants via the Foundational `store_active_tuning` (audited gated action: old value, new value, sample_size, error_before/after, tuned_at) and read via `load_active_tuning`; corruption/open-failure degrades to `Disabled`, never serves a bad tuning (idempotent, non-blocking write). Calibration write must NOT bump frecency. (FR-008 gated-action audit; FR-003 honest degrade; Principle V) (depends on T029, Foundational T010)
- [ ] T032 [US2] Wire accepted constants into `estimate_economics` (controller.rs:319): when an active `TunedEstimateConstants` MATCHING the current `estimator_version` is in force, use the tuned response/manual floor (replacing the 400/800 plan-only fallback at `grounded_step_tokens` controller.rs:359-361) and tuned schema/invoke (replacing `COMPACT_SCHEMA_TOKENS`/`COMPACT_INVOKE_TOKENS` controller.rs:331-332); fall back to static floors otherwise (T025 covers the mismatch). Byte-grounded path unchanged; figure stays labeled `(est.)`. (FR-006, FR-010) (depends on T031)
- [ ] T033 [US2] Surface honest tuned state: update `format_calibration_section` (calibration.rs:85-110) and the `status detail:full` path (status.rs:189) to render `deferred` / `accumulating (n/min)` / `tuned (error before% → after%)` from `CalibrationVerdict`, never `validated`/`saved`/`active` and never `tuned` without the error artifact. (FR-009, FR-010, SC-005) (depends on T030)
- [ ] T034 [US2] EXTEND `tests/surface_honesty.rs` (server-gated): assert every `CalibrationVerdict` (deferred, accumulating, tuned) renders honestly — `tuned` carries before/after error and sample size, no surface reads `validated`/`saved`/`active`, the served figure stays `(est.)` under tuned constants. (FR-009, FR-010, SC-005) (depends on T033)
- [ ] T035 [US2] Routing/golden-replay non-regression in `tests/stel_golden_replay.rs`: assert routing/policy/deny + golden-replay byte-exact output are unchanged across deferred/accumulating/tuned states (calibration changes estimates, not decisions); confirm calibration writes never bump frecency. (FR-007, SC-004; Principle V, Principle VII) (depends on T032)
- [ ] T036 [US2] Embed-isolation gate: confirm `derive_tuning_candidate`/`validate_candidate`/`CalibrationVerdict` compile in embed (pure, no server/network deps) while `store_active_tuning`/`load_active_tuning` stay reachable behind `any(server, embed)`; embed `--lib` AND embed-musl `--lib` stay green and pull no server/networking crate. Verification: `cargo check --no-default-features --features embed --lib && cargo clippy --no-default-features --features embed --lib -- -D warnings && cargo build --no-default-features --features embed --target x86_64-unknown-linux-musl --lib`. (Principle VI / G-045; supports FR-006 reachability) (depends on T032)
- [ ] T037 [US2] FR-011 operator reset/clear: implement clearing accumulated calibration (clear tuned constants and/or samples for the current `estimator_version`) WITHOUT rebuilding the index, delivered as an MCP tool MODE/param (constitution II MCP-native surface + tool-consolidation: sync `SYMFORGE_TOOL_NAMES` in `cli/init.rs` and the `daemon.rs` aliases), plus a test asserting the state returns to `Deferred` after reset (data-model `* → Deferred (operator reset)`). NOT injected context. (FR-011; Principle II) (depends on T031)
- [ ] T038 [US2] Drop `calibration_auto_tune` from `DEFERRED_ITEMS` (status.rs:19) and update its compact-status test assertion — **single owner of this edit; US3 does not touch it** — only now that the `tuned` state is actually reachable (mirrors how `ledger_persistence` was removed under 010 FR-004). (FR-009) (depends on T033)

**Checkpoint — full gate**: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1 && cargo check --no-default-features --features embed --lib && cargo build --no-default-features --features embed --target x86_64-unknown-linux-musl --lib && cargo build --release`. US1+US2 work independently; predictor learns from observed error and applies only validated, audited gains. Commit Phase US2. (SC-002, SC-004, SC-005; Principle VIII)

---

## Phase 5: User Story 3 — The calibration state is reported honestly (Priority: P2)

**Goal**: surface the truthful `Deferred → Accumulating(n/min) → Tuned(before% → after%)` state
machine in `status detail:full` and the opt-in full trust envelope. `Tuned` renders ONLY when
backed by a before/after held-out error-reduction artifact (from US2); otherwise `Accumulating`
or `Deferred`. A `Disabled`/`Unavailable` durable store pins the state at `Deferred` (in-memory
only), reported distinguishably. Extends the `surface_honesty` regression so every state renders
honestly and no surface reads `tuned`/`validated`/`saved`/`active` without the artifact. Adds NO
new measured-saving claim and changes NO routing/policy/economics behavior.

**Depends on**: US2 supplying the tuning artifact and `CalibrationVerdict`.

**Independent Test**: drive the state from cold (no events) through accumulating to tuned with a
deterministic corpus; at each stage render `status detail:full` (`format_stel_status` with
`StelStatusDetail::Full`) and the full trust envelope (`format_trust_envelope` with
`SYMFORGE_STEL_FULL=1`) and assert: (a) zero/sub-threshold events read `deferred` then
`accumulating (n/min)`, never `tuned`; (b) a tuning that reduced held-out error reads
`tuned (error: before% → after%)` with sample size + before/after figures; (c)
`tuned`/`validated`/`saved`/`active` never appear without the backing artifact; (d) a
`Disabled`/`Unavailable` durable store pins `deferred` distinguishably. Run:
`cargo test --all-targets -- --test-threads=1` and `cargo check --no-default-features --features
embed --lib`.

### Tests for User Story 3 (write FIRST — red before impl)

- [ ] T039 [P] [US3] Failing unit tests in `src/stel/calibration.rs` for the honest `CalibrationVerdict` renderer (deferred / accumulating (n/min) / tuned (error: before% → after%)). Verification: `cargo test --all-targets -- --test-threads=1 calibration::` (expected RED: the render fn does not exist yet). (FR-009)
- [ ] T040 [P] [US3] Failing `status.rs` unit tests asserting `status detail:full` renders `deferred`, then `accumulating (n/min)`, then `tuned (error: before% → after%)` for each verdict, and that a `Disabled`/`Unavailable` `durable_ledger` pins the rendered calibration state at `deferred` (never `tuned`). Verification: `cargo test --all-targets -- --test-threads=1 status::` (expected RED). (FR-009)
- [ ] T041 [P] [US3] Failing `envelope.rs` unit tests asserting the opt-in full trust envelope `calibration:` line renders the three honest states and never reads `tuned` without before/after figures. Verification: `cargo test --all-targets -- --test-threads=1 envelope::` (expected RED: calibration field is still `&'static str`). (FR-009)
- [ ] T042 [P] [US3] Failing deterministic corpus-driven state-transition test in `tests/stel_calibration_tuning.rs`: replay a known biased corpus and assert the SURFACED `CalibrationVerdict` transitions cold→`Deferred`, gathering→`Accumulating(n/min)`, validated-reduction→`Tuned(before→after)`, and a worse-than-baseline candidate stays `Accumulating` (never `Tuned`). (FR-009, FR-012, SC-001)
- [ ] T043 [P] [US3] Transport-parity + frecency guard test in `tests/stel_ledger_persistence.rs`: the surfaced `CalibrationVerdict` text is identical for the stdio and serve renderings of `detail:full` given the same ledger/tuning inputs, and rendering the calibration surface performs zero frecency bumps (read-only). (SC-004, FR-007; Principle VII, Principle V)

### Implementation for User Story 3

- [ ] T044 [US3] Implement the `CalibrationVerdict` render helper + deterministic constructor from sample count + the optional US2 tuning artifact in `src/stel/calibration.rs`; `Tuned` renders only when the before/after artifact is present, otherwise `accumulating`/`deferred`. Replace the hard-coded `auto-tuning still deferred` string residue and extend `StelCalibrationSummary` so `format_calibration_section` renders the verdict line under `status detail:full`. (FR-009) (depends on T039, US2 T027/T030)
- [ ] T045 [US3] Wire the `CalibrationVerdict` through `StelStatusContext` into `format_full_status` (status.rs): thread the verdict and, when `durable_ledger` is `Disabled`/`Unavailable`, FORCE `Deferred` to honor the in-memory-only honesty invariant, so the `detail:full` calibration section reflects real state; keep compact status free of the calibration section (existing test asserts absence). (FR-009) (depends on T040, T044)
- [ ] T046 [US3] Change `TrustEnvelopeInput.calibration` from `&'static str` to an owned honest rendering of the `CalibrationVerdict` and update `format_trust_envelope_inner` full-block to print the state-aware line; the compact one-liner is unchanged (never carries calibration). Keep served/predicted figures explicitly `(est.)`/heuristic-labeled even when `tuned` (grounding is not measurement). (FR-009, FR-010) (depends on T041, T044)
- [ ] T047 [US3] Update `envelope_for_decision` in `handler.rs` to pass the live `CalibrationVerdict` (derived from the in-force tuning artifact, falling back to `Deferred` when no durable store / no tuning) instead of the hard-coded `calibration: "deferred"` literal. (FR-006, FR-010) (depends on T046)
- [ ] T048 [US3] Export `CalibrationVerdict` + any render helper from `src/stel/mod.rs` so integration tests and the status/envelope/handler callers resolve it (and so it does NOT shadow the existing `types::CalibrationState` re-export). Verification: `cargo check --all-targets && cargo check --no-default-features --features embed --lib`. (FR-009) (depends on T044)
- [ ] T049 [US3] Extend the `surface_honesty` regression (`tests/surface_honesty.rs`): drive each `CalibrationVerdict` state and assert `detail:full` status AND the full trust envelope render honestly — deferred/accumulating never read `tuned`/`validated`/`saved`/`active`; `tuned` appears only with before/after error figures; a `Disabled`/`Unavailable` durable store pins `deferred` distinguishably. (SC-005, FR-009) (depends on T047)

**Checkpoint — full gate**: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1 && cargo check --no-default-features --features embed --lib && cargo build --no-default-features --features embed --target x86_64-unknown-linux-musl --lib && cargo build --release`. The calibration state machine is legible, auditable, and honest across status + envelope + both transports; no new measured claim; no routing/policy change. Commit Phase US3. (SC-005, FR-009; Principle VI; Principle VIII)

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: campaign-level verification and integration.

- [ ] T050 Run the full quickstart acceptance pass for all three stories (SC-001..SC-005), including the ≥3-restart cross-session dogfood on a real stdio session AND the daemon-default path.
- [ ] T051 [P] Confirm Constitution VI/VII at campaign scope: `cargo build --no-default-features --features embed --lib`, `cargo build --no-default-features --features embed --target x86_64-unknown-linux-musl --lib`, and `cargo clippy --no-default-features --features embed --target x86_64-unknown-linux-musl --lib -- -D warnings` all green; stdio↔serve parity holds for every touched formatter.
- [ ] T052 git-master: integrate all phase commits onto a review branch; HARD-STOP before any push/merge (await explicit human approval).
- [ ] T053 Write the honest results doc (objective / changes / verification / evidence / known gaps) under `docs/reviews/`; `cargo clean` only now (campaign end).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no deps — start immediately.
- **Foundational (Phase 2)**: depends on Setup — BLOCKS US1/US2/US3 (they consume the v2 schema, retention, tuned-constant storage, and `samples_for_estimator`).
- **US1 (Phase 3)**: after Foundational. Un-gates the durable store for embed/stdio and wires both the local AND daemon-default stdio paths. The MVP.
- **US2 (Phase 4)**: after US1 — needs durable, estimator-version-tagged samples + the storage primitives reachable on the dispatch path. Owns the derivation/validation/audit + the `DEFERRED_ITEMS` drop + FR-011 reset.
- **US3 (Phase 5)**: after US2 — needs the tuning artifact + `CalibrationVerdict`. Pure honest-surface rendering; no economics change.
- **Polish (Phase 6)**: after all stories.

### Within Each Story

- Tests written before the fix (assert they fail), then implement, then the per-phase full gate, then commit.
- Each story ends green on the full gate before the next begins.

### Cross-story dependency enforcement

- US2 → US1 (schema + samples reachable on dispatch) and US3 → US2 (artifact + verdict) are HARD edges, not prose: do NOT start US2 before US1's durable-open + Foundational's `estimator_version` column land, and do NOT start US3 before US2's `CalibrationVerdict`/artifact land. Each story's independent test presumes its predecessor shipped.

### Parallel Opportunities

- Setup T002/T003 [P]; Foundational T005/T006/T008/T010 [P] (T009/T011/T012 serialize on `record()`/`migrate()` in the same file).
- Within a story, `[P]` test tasks (different files / no dep) run together: US1 T014-T017; US2 T024/T025; US3 T039-T043.
- US2 and US3 cannot run in parallel (US3 consumes US2's artifact); US1 must precede both.
- **Throttle**: each phase's full gate is a heavy `cargo` run — serialize the gates (do not run phase-gates concurrently); keep `target/` warm until T053.

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories: v2 schema, retention, tuned-constant storage, filtered sample read).
3. Complete Phase 3: US1 — durable ledger reaches stdio AND the daemon default, degrades honestly, embed isolation intact.
4. **STOP and VALIDATE**: ≥3-restart cross-session accumulation on a real stdio session and the daemon-default path; embed `--lib` + embed-musl green.
5. Ship if ready — US1 is the highest-leverage increment (durability where the operator actually runs).

### Incremental Delivery

US1 → US2 → US3 → Polish. Each story is a green, independently-testable increment; integrate to
a review branch and STOP for human approval before any push/merge (T052).

---

## Notes

- Anchors are live-at-authoring line numbers; re-confirm live before editing (Step-0 / EDIT INTEGRITY).
- Honesty is load-bearing: never render `tuned` without the before/after held-out error-reduction artifact; storing constants alone never promotes the surface.
- Estimate-only: calibration changes ESTIMATES, never routing/policy/safety. Served figures stay `(est.)`/heuristic-labeled even when tuned (grounding is not measurement).
- Embed isolation (Principle VI/G-045): the durable store is embed-reachable via `any(server, embed)` and rusqlite (unconditional), but NO server/networking crate enters the embed build — `cargo check/build --no-default-features --features embed` (and the musl target) must stay green.
- Single owners are fixed: migration = T009, retention = T011, embed-gating = T018, `DEFERRED_ITEMS` drop = T038, FR-011 reset = T037. Do not duplicate these across stories.
- No push/merge without explicit human approval — commit to a review branch and stop.
