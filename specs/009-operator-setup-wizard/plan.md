# Implementation Plan: Operator Setup Wizard & In-Harness Admin Command

**Branch**: `009-operator-setup-wizard` | **Date**: 2026-06-18 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/009-operator-setup-wizard/spec.md`

**Grounding**: this plan ORCHESTRATES shipped capabilities (004 serve, 005 harness
scan/apply/backup, 006 admin GUI). The reuse map below is verified against live code
on `main@90f896c` (8.1.0); 009 adds a thin wizard/admin layer + a small set of new
seams, and must not reimplement what 004/005/006 already provide.

## Summary

Take an operator from a bare `npm install -g symforge` to a configured, running
SymForge in one guided command, plus a `/symforge-admin` in-harness affordance that
opens the running dashboard. Three independently-shippable slices: **US1** a
collision-free default serve port (a real bug — the fixed `8787` default binds a dead
second listener when occupied), **US2** the guided wizard (scan → choose → apply with
restorable backups → running dashboard), **US3** the admin verb + command-file install
(reuse a running server or start one). Everything operator-facing (terminal, port
probe, browser, process spawn) sits behind injectable seams (FR-017) so the whole flow
is fixture-tested with zero real side effects. Server-feature-gated; the embed build
stays network-free (FR-019).

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge`.

**Primary Dependencies**: existing only — `socket2` (free-port probe, already used in
`bind_listener`), `clap` (subcommands), `serde`/`serde_json` (profile + harness configs),
`tokio` (serve runtime), `axum`/`rmcp` (server). **No new dependency** (browser-open uses
`std::process::Command` to the OS opener, not a crate).

**Storage**: a new operator-local `OperatorSetupProfile` at `<project>/.symforge/operator-setup.json`
(mirrors the existing `OnboardingState` load/save pattern, `onboarding.rs`). Not an
index; operator convenience state (Constitution I unaffected).

**Testing**: `cargo test --all-targets -- --test-threads=1`; the wizard/admin flows are
driven through the `SetupSink` + `BrowserOpener` + port-probe seams with scripted answers
(FR-014/017), validated against harness-config **fixtures** only (FR-018 — never the
developer's real configs).

**Target Platform**: local CLI, Windows/Linux/macOS; serve over loopback (default) or a
routable address (keyed).

**Project Type**: single Rust project (CLI + server feature).

**Performance Goals**: none beyond responsiveness; port probe is O(1) TCP binds.

**Constraints**: server-feature-gated (FR-019, Constitution VI — embed stays
network-free); no real terminal/network/browser/process side effects in tests (FR-017);
fixtures-only verification (FR-018); reported URLs must be the URLs actually bound
(FR-020); idempotent re-runs (FR-013, Constitution IV).

**Scale/Scope**: ~5 new small modules (`cli/setup.rs`, `cli/admin.rs`,
`cli/operator_profile.rs`, a browser-open seam, a free-port helper) + 2 `Commands`
variants + a per-harness command-file installer; heavy reuse of 004/005/006.

## Reuse map (verified live — 004/005/006)

| Need | Reuse (file:line) | 009 role |
|------|-------------------|----------|
| Bind a listener | `server::serve::bind_listener` (serve.rs:141) | bind the chosen/probed port |
| Default address | `serve::DEFAULT_LISTEN = "127.0.0.1:8787"` (serve.rs:29) | preferred port, probe if occupied |
| Harness list + state | `HarnessRegistry::known()/scan()` (harness.rs:129/188), `HarnessState` (harness.rs:98) | scan summary (US2 FR-004) |
| Plan/apply/backup | `harness_apply::{plan,apply,write_backup,restore,atomic_write}` (harness_apply.rs:82/116/208/233/239) | apply harness entries with restorable backups, idempotent (FR-009) |
| BOM-safe read | `init::read_config_text` (init.rs:377) | already in the apply path |
| Output/seam pattern | `OnboardingSink` (onboarding.rs:56), `OnboardingState` load/save (onboarding.rs:20) | model for `SetupSink` + `OperatorSetupProfile` |
| Dashboard + API | `ADMIN_PATH="/admin"` (admin/mod.rs:33), `build_admin_router` (admin/mod.rs:116), `/api/v1/*` | the dashboard the wizard opens |
| Liveness check | `sidecar::port_file::sidecar_port_is_alive` (port_file.rs:174) | pattern for operator-server reachability |
| Auth policy | `AuthConfig::{requires_auth,refuse_to_start}` (auth.rs:37/59) | loopback no-key / network requires-key (FR-007) |
| `.symforge/` dir | `paths::resolve_symforge_dir` (paths.rs:45) | profile location |
| CLI surface | `cli::Commands` (cli/mod.rs:40), `main.rs` dispatch, `ServeCliArgs` (cli/serve.rs) | add `Setup`/`Admin` verbs |

## New surface (the only code 009 adds)

1. **Free-port probe** (US1) — `serve::probe_free_port`/`bind_with_fallback`: bind
   `127.0.0.1:0` (OS-assigned) or test the preferred port, return a verified-free
   `SocketAddr`. Wire the no-explicit-address serve path through it; an explicit
   occupied address still fails loudly (FR-002/003).
2. **`OperatorSetupProfile`** (`cli/operator_profile.rs`) — `{installation_type, port,
   auth_posture, harnesses}` with `load()/save()` at `.symforge/operator-setup.json`
   (mirror `OnboardingState`).
3. **`SetupSink`** seam (`cli/setup.rs`) — `ask_choice`, `confirm`, `summary`, `status`;
   `StderrSetupSink` (real) + `ScriptedSetupSink` (test, pre-supplied answers) (FR-014/017).
4. **`BrowserOpener`** seam — `open_url(url) -> Result<Opened|Skipped>` via
   `std::process::Command` (`cmd /c start` | `open` | `xdg-open`), headless → print+skip;
   `NoopBrowserOpener` for tests (FR-011/017).
5. **Non-blocking serve-start helper** — start `serve` on a background task bound to a
   verified-free port, wait until reachable, return the bound dashboard+attach URLs
   (FR-010/020). The wizard/admin reuse this; it wraps `serve::run` rather than
   reimplementing it.
6. **Operator-server reachability check** — HTTP GET `/api/v1/summary` (or `/admin`)
   with a short timeout (the `sidecar_port_is_alive` pattern over HTTP) → reuse-if-running
   (US3 FR-015).
7. **Per-harness command-file install** — write a `symforge-admin` command file where the
   harness supports one (e.g. Claude Code `~/.claude/commands/symforge-admin.md`), else a
   protocol-native affordance (an MCP prompt/resource returning the dashboard URL) for
   harnesses without command files (FR-016). Exact per-harness format resolved in
   research.md against the `HarnessId` set.
8. **`Commands::Setup` / `Commands::Admin`** + `main.rs` dispatch.

## Constitution Check

*Against `.specify/memory/constitution.md` v1.0.0.*

| # | Principle | Verdict | Note |
|---|-----------|---------|------|
| I | Local-First In-Process Index | **PASS** | No index touched. The profile is operator state, not a query store. |
| II | MCP-Native Surface | **PASS** | The in-harness admin affordance is an MCP prompt/resource (or a harness command file), not chat injection; no client-tool shadowing. |
| III | Trust Envelopes | **N/A→PASS** | No ranked/truncated tool results; the wizard reports exact bound URLs (FR-020) — the honest-surface ethos applies (no advertised-but-dead URL). |
| IV | Determinism & Recovery | **PASS** | Idempotent re-run (FR-013); every config write has a restorable backup (FR-009, reused); start-on-demand + reuse-if-running. |
| V | Frecency Invariant | **PASS** | No frecency interaction. |
| VI | Embed Isolation (G-045) | **PASS** | All 009 code is `#[cfg(feature = "server")]` (it configures/starts serve); embed compiles none of it. `cargo check --no-default-features --features embed` in the per-phase gate. |
| VII | Transport Parity | **PASS (scoped)** | The dashboard is serve-only **by design** (spec Assumption; consistent with the existing serve-only dashboard) — a transport-specific capability explicitly scoped, not a tool-result parity regression. |
| VIII | Verification Before Done | **PASS** | Per-phase full gate; fixtures-only (FR-018); seams make the flow testable without side effects. |

**Result**: no violations. **Complexity Tracking**: empty.

**Re-check post-design**: still PASS — no new index, no new cross-cutting feature flag,
no new dependency, no chat injection, server-gated.

## Project Structure

```text
specs/009-operator-setup-wizard/
├── plan.md · research.md · data-model.md · quickstart.md
├── contracts/  (setup-cli.md, admin-cli.md, operator-profile.md, free-port.md,
│                command-file.md, seams.md)
└── tasks.md  (/speckit-tasks)
```

### Source (new + touched), by user story

```text
US1  src/server/serve.rs            free-port probe + no-explicit-address fallback
US2  src/cli/setup.rs        (NEW)  wizard orchestrator + SetupSink + SetupCliArgs
     src/cli/operator_profile.rs (NEW)  OperatorSetupProfile load/save
     src/cli/browser.rs      (NEW)  BrowserOpener seam (std::process::Command)
     (reuse harness.rs / harness_apply.rs / init.rs::read_config_text — unchanged)
US3  src/cli/admin.rs        (NEW)  admin verb: reachability check -> reuse/start -> open
     src/cli/harness_command.rs (NEW)  per-harness symforge-admin command-file install
     (reuse server/admin, auth.rs, port liveness pattern)
xcut src/cli/mod.rs, src/main.rs    Commands::Setup / Commands::Admin + dispatch
     tests/  setup_wizard.rs, serve_port.rs, admin_verb.rs (fixtures-only)
```

**Structure Decision**: a thin `cli::setup`/`cli::admin` layer over the 004/005/006
engine; the only genuinely new mechanisms are the free-port probe, the profile, and the
two seams (SetupSink, BrowserOpener) — everything else is reuse.

## Phase mapping (user stories → phases)

| Phase | Story | Pri | Core | New code |
|-------|-------|-----|------|----------|
| **1** | US1 collision-free port | P1 | probe a verified-free port; explicit-occupied fails loud | free-port probe in serve.rs + regression |
| **2** | US2 setup wizard | P1 | scan -> choose -> confirm -> apply (reuse) -> serve-start -> open | setup.rs, operator_profile.rs, browser.rs, serve-start helper |
| **3** | US3 admin verb + command file | P2 | reuse-if-running / start; install per-harness `symforge-admin` | admin.rs, harness_command.rs, reachability check |

US1 ships first (foundational, the real bug, unblocks US2/US3 server flows). Each phase
runs the full gate (incl. embed) before the next.

## Complexity Tracking

No constitution violations. Table intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| — | — | — |
