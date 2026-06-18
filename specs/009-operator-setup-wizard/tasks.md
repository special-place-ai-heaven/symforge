---
description: "Task list for Operator Setup Wizard implementation"
---

# Tasks: Operator Setup Wizard & In-Harness Admin Command

**Input**: Design documents from `specs/009-operator-setup-wizard/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: REQUESTED — FR-017/018 (seams + fixtures-only), SC-003 (port regression),
the per-story acceptance scenarios. Test tasks included; drive flows through the seams.

**Organization**: by user story. US1 (collision-free port, the real bug) ships first and
its free-port probe is reused by the US2/US3 serve-start helper. Heavy reuse of 004/005/006
(see plan.md reuse map); 009 adds only the thin wizard/admin layer + new seams. Every
compile/gate runs through terminal-commander (error/stall comb rules).

## Format: `[ID] [P?] [Story] Description`
- **[P]**: parallelizable (different file, no incomplete-task dependency)
- Anchors are from the verified reuse map (plan.md); re-confirm at use.

## Path Conventions
Single Rust crate `symforge`; new modules under `src/cli/`, port probe in `src/server/serve.rs`,
MCP prompt in `src/protocol/prompts.rs`, tests under `tests/`. All 009 code `#[cfg(feature = "server")]`.

---

## Phase 1: Setup

- [X] T001 Baseline green (main@90f896c FF'd green; 009 added only docs). Gates run via Bash (terminal-commander daemon was unavailable mid-session).
- [X] T002 [P] `Commands::Setup`/`Commands::Admin` variants + main.rs dispatch — DONE (matched the structural `cfg(server)` gating at lib.rs:26-47; no redundant per-item cfg).
- [X] T003 [P] module skeletons (setup/admin/operator_profile/browser/harness_command), stubs bail loudly — DONE.

**Checkpoint**: surface compiles green; embed unaffected. ✅

---

## Phase 2: User Story 1 — Collision-free serve port (Priority: P1) 🎯 MVP

**Goal**: serve with no explicit address binds a verified-free port (prefer 8787, else
OS-assigned); explicit-occupied fails loudly; reported URL == bound URL.

**Independent Test**: occupy 8787 → serve (no addr) binds a different reachable port, GET
to the reported URL succeeds, no dead listener; 8787 free → binds 8787; explicit-occupied → loud error.

### Tests for US1
- [X] T004 [P] [US1] `tests/serve_port.rs` regression — DONE: occupy with an EXCLUSIVE listener (SO_REUSEADDR false-repro caught), probe falls back, starts a real axum server + reqwest GET→200 (reachable, SC-003/FR-020); control (8787-free→8787); explicit-occupied→loud error. Fails pre-fix.

### Implementation for US1
- [X] T005 [US1] `probe_free_listener` (zero-TOCTOU, returns the live listener) + `probe_free_port` (addr form, documented small window) in `src/server/serve.rs` — DONE (D1).
- [X] T006 [US1] `ServeCliArgs.listen: Option<String>` + `explicit_listen` carries intent: default→probe(prefer 8787 else :0); explicit→bind exactly, loud on occupied (FR-002/003); reported URL == bound URL — DONE.
- [X] T007 [US1] full gate green (test 90 ok/0 fail, clippy/fmt/embed clean, build --release). Commit Phase US1.

**Checkpoint**: US1 shippable alone — the real 8787 port bug fixed. ✅

---

## Phase 3: Foundational (shared US2/US3 infra)

**Purpose**: the seams + profile + serve-start helper US2 AND US3 both consume. Depends on
US1's `probe_free_port`. BLOCKS US2/US3.

- [X] T008 [P] `OperatorSetupProfile` (harnesses as `HarnessId::slug` strings — HarnessId has no serde), atomic write, `load`→None on missing/malformed, no secrets persisted — DONE + 4 tests.
- [X] T009 [P] `SetupSink` + `StderrSetupSink` + `ScriptedSetupSink` (+ `InstallationType` serde) — DONE + tests.
- [X] T010 [P] `BrowserOpener` + `OsBrowserOpener` (Command; headless guard via DISPLAY/WAYLAND → Skipped, never errors) + `NoopBrowserOpener`; kept `BrowserOpenOutcome` name (Phase-1 skeleton) — DONE + tests, no new dep.
- [X] T011 `ServerSessionDescriptor` + `operator_server_reachable` (reqwest sync block_on GET /api/v1/summary; any HTTP response→true) + `start_operator_server` (spawn OS thread w/ own runtime running serve::run on a pre-selected free addr; poll reachability). Reactor-bound `probe_free_port` panic from sync caller caught → reactor-free `select_free_addr_std`. Documented limit: no graceful-stop handle (D3 scope). DONE + live serve-start test.

**Checkpoint**: seams + profile + serve-start/reachability compile + unit-tested (12). US2/US3 can begin. ✅

---

## Phase 4: User Story 2 — Guided setup wizard (Priority: P1)

**Goal**: one guided command: scan → choose → restate → apply (reuse, backed-up) → serve-start → open → persist → idempotent re-run.

**Independent Test**: `symforge setup --non-interactive` (ScriptedSetupSink + NoopBrowserOpener) over a temp home + fixture harness configs: scan changes nothing; apply configures exactly the chosen harnesses each with a restorable backup, re-run no-duplicate; server mode → reachable dashboard URL; profile persisted; re-run → refresh/no-op.

### Tests for US2
- [ ] T012 [P] [US2] `tests/setup_wizard.rs`: drive `cli::setup::run` non-interactive over `HarnessRegistry::known_with(temp_home, temp_wd)` + fixture configs — assert scan summary (FR-004, no mutation), apply backs up + configures chosen harnesses + idempotent re-run no-duplicate (SC-002), profile persisted (FR-012), browser open recorded Skipped (FR-011), reported URL reachable (FR-020). Fixtures only (FR-018).
- [ ] T013 [P] [US2] Headless test: no `DISPLAY`/opener → URL printed, open skipped, no error (FR-011 edge).

### Implementation for US2
- [ ] T014 [US2] `SetupCliArgs` (clap: `--non-interactive`, `--installation-type`, `--port`, `--harnesses`, `--yes`) in `src/cli/setup.rs` (contracts/setup-cli.md).
- [ ] T015 [US2] Scan step: `HarnessRegistry::known()` + remembered-server reachability → read-only summary (OS, per-harness `HarnessState`, running server, suggested free port via `probe_free_port`) (FR-004).
- [ ] T016 [US2] Choose + restate: `SetupSink::ask_choice` for install type / harness subset / port (pre-filled free suggestion); `SetupSink::confirm` restates exact actions before any change (FR-005/006/008).
- [ ] T017 [US2] Apply: `harness_apply::plan` → `apply` (restorable backups, idempotent, BOM-safe via `read_config_text`) for the chosen harnesses; surface backup paths (FR-009).
- [ ] T018 [US2] Server mode: auth posture via `AuthConfig::refuse_to_start` (loopback no-key; network → generate/prompt key, pass via env not inline, FR-007); start via the serve-start helper; report dashboard + attach URLs; `BrowserOpener::open_url` offer (FR-010/011).
- [ ] T019 [US2] Persist `OperatorSetupProfile` (FR-012); idempotent re-run detection (existing profile + running server → refresh/no-op, FR-013).
- [ ] T020 [US2] Per-phase gate; confirm T012/T013 pass. Commit Phase US2.

**Checkpoint**: US1+US2 — bare install → configured + running in one command.

---

## Phase 5: User Story 3 — In-harness admin command (Priority: P2)

**Goal**: `symforge admin` (and the in-harness affordance) reuses a running server or starts one, then opens/returns the dashboard URL.

**Independent Test**: server running on remembered port → admin reuses it (opens that URL, no 2nd server); none running → starts one on a free port + opens; the `symforge-admin` MCP prompt returns the URL; Claude Code command file installed (fixture), others get the MCP prompt only.

### Tests for US3
- [ ] T021 [P] [US3] `tests/admin_verb.rs`: with a server reachable on the profile port, `cli::admin::run` reuses it (no 2nd server, SC-004); with none, starts one on a free port + reports reachable URL; uses NoopBrowserOpener. Fixtures only.
- [ ] T022 [P] [US3] Test the `symforge-admin` MCP prompt returns the dashboard URL; and the command-file installer writes the Claude Code file against a fixture `~/.claude/commands/` while harnesses without a format get no file (FR-016).

### Implementation for US3
- [ ] T023 [US3] `AdminCliArgs` + `cli::admin::run`: read `OperatorSetupProfile.port` → reachability check → reuse-if-running else serve-start (T011) → report + `BrowserOpener` open (FR-015, contracts/admin-cli.md).
- [ ] T024 [US3] Register a `symforge-admin` MCP prompt in `src/protocol/prompts.rs` that returns the running dashboard URL via the reachability→reuse/start path (universal affordance, Constitution II) (FR-016, D2).
- [ ] T025 [US3] `src/cli/harness_command.rs`: install the Claude Code `~/.claude/commands/symforge-admin.md` command file (reuse the restorable-backup write path); for harnesses with no documented command-file format, install nothing (MCP prompt is the floor — no guessed/broken file) (FR-016, D2, contracts/command-file.md). Wire installation into the setup/standard config path.
- [ ] T026 [US3] Per-phase gate; confirm T021/T022 pass. Commit Phase US3.

**Checkpoint**: all three stories independently functional.

---

## Phase 6: Polish & Cross-Cutting

- [ ] T027 [P] Run the full quickstart acceptance pass (SC-001..SC-006); confirm fixtures-only (no real config mutated).
- [ ] T028 Live operator dogfood: build the local binary, run `symforge setup` in a scratch project (occupy 8787 first to confirm fallback), confirm the dashboard opens on the reported port; `symforge admin` reuses it. Record evidence.
- [ ] T029 [P] Confirm Constitution VI/VII: `cargo check --no-default-features --features embed` green (009 fully server-gated); dashboard serve-only by design noted.
- [ ] T030 git-master: integrate onto the review branch; HARD-STOP before any push/merge (await human approval). Honest results doc under docs/reviews/.

---

## Dependencies & Execution Order

- **Setup (P1)** → **US1 (P2)** → **Foundational (P3, uses US1 probe)** → **US2 (P4)** + **US3 (P5)** (both depend on Foundational; US3 also reuses the US2 serve-start) → **Polish (P6)**.
- US1 is independent + ships first (the real bug). US2 and US3 share the Foundational seams; US3's admin-start reuses the US2 serve-start helper (T011, shared).
- Within a story: tests before impl; per-phase full gate (via terminal-commander) before the next; commit per phase.

### Parallel Opportunities
- Setup T002/T003 [P]; Foundational T008/T009/T010 [P] (distinct files); story test tasks [P].
- US2 and US3 implementation touch disjoint new files (setup.rs vs admin.rs/harness_command.rs/prompts.rs) → parallelizable after Foundational, but **serialize the heavy cargo gates** (one phase-gate at a time).

## Implementation Strategy
MVP = US1 (the port bug, shippable alone) → US2 (the wizard, the headline) → US3 (admin
convenience). Each a green, fixtures-tested increment; integrate to a review branch and
STOP for human approval before any push/merge. No new dependency, no new feature flag,
server-gated throughout.

## Notes
- Reuse 004/005/006 — do NOT reimplement scan/apply/backup/serve/admin (plan.md reuse map).
- No new dependency (browser-open via std::process::Command); all code `#[cfg(feature="server")]`.
- Fixtures only (FR-018): never mutate a real harness config; temp home via `known_with`.
- Reported URL == bound + reachable URL (FR-020). No push/merge without approval.
