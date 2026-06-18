# Phase 0 Research: Operator Setup Wizard

Decisions resolving the two plan-level forks the spec deferred ("confirmed during
planning") plus the lifecycle/seam choices, each grounded in the verified reuse map.
Format: **Decision / Rationale / Alternatives rejected**.

## D1 — Free-port strategy (US1, FR-001/002/003)

**Decision**: when serve is started with **no explicit address**, prefer the historical
default port (`DEFAULT_LISTEN` = `127.0.0.1:8787`): probe it by attempting a bind; if
free, use it; if occupied, bind `127.0.0.1:0` (OS-assigned ephemeral) and use the port
the OS returns — always a verified-free port, never a dead second listener. An
**explicit** address is honored exactly when free and **fails loudly** when occupied
(no substitution).

**Rationale**: the probe-then-fallback gives operators a stable, predictable default
when nothing contends, and a guaranteed-free port when something does (the exact bug:
`wslrelay`/another service squatting 8787). Binding `:0` is the race-free way to get a
free port (the OS reserves it atomically); reusing `bind_listener` keeps SO_REUSEADDR
and the existing error path. Reported URL = the actually-bound address (FR-020).

**Alternatives rejected**:
- *Increment from 8787 (8788, 8789…)* — a check-then-bind TOCTOU race and can still
  collide; `:0` is atomic and simpler. Rejected.
- *Always ephemeral (`:0`)* — loses the stable default operators bookmark; the remembered
  port in the profile mitigates, but a predictable default is friendlier. Rejected as the
  default-first choice; `:0` is the fallback.
- *New fixed default port* — just relocates the collision risk. Rejected.

## D2 — Per-harness `symforge-admin` affordance (US3, FR-016)

**Decision**: install a command file where the harness supports one, else a
protocol-native MCP affordance. Concretely, across the live `HarnessId` set
(ClaudeCode, ClaudeDesktop, Codex, Gemini, KiloCode, Cursor):
- **Claude Code** supports markdown slash-command files → write
  `~/.claude/commands/symforge-admin.md` that invokes the `symforge admin` CLI verb
  (or, in stdio/no-CLI contexts, instructs calling the MCP affordance).
- **Harnesses without a command-file convention** (Codex/Gemini/Cursor/Kilo/Claude
  Desktop) → a **protocol-native MCP prompt** `symforge-admin` (registered in
  `protocol::prompts`) that returns the running dashboard URL (and starts/queries via
  the same reachability path). MCP prompts are already first-class
  (`src/protocol/prompts.rs`), so every harness that speaks MCP gets the affordance even
  without a command file.

**Rationale**: Constitution II (MCP-native) — the universal affordance is the MCP prompt,
present for all harnesses; the Claude Code command file is a convenience layer on top.
This avoids inventing per-harness command formats we can't verify. The exact set of
command-file-capable harnesses is confirmed against the harness configs during implement;
when unknown, default to the MCP prompt (never a broken command file).

**Alternatives rejected**:
- *Command file for every harness* — most MCP clients have no documented command-file
  format; guessing risks a broken affordance (a nonworking feature). Rejected.
- *CLI-only (`symforge admin`), no in-harness affordance* — fails FR-016 (in-harness
  one-action). Rejected.

## D3 — Serve lifecycle: start-on-demand + reuse-if-running (US2/US3, FR-010/013/015)

**Decision**: a non-blocking serve-start helper wraps `serve::run` on a background tokio
task bound to a verified-free port (D1), then polls reachability (D6 below) until the
listener serves or a short deadline elapses, returning the bound dashboard + attach URLs.
The admin verb and wizard first check reachability on the **remembered** port (from the
profile); if a server already serves there, they reuse it (open/return its URL) and start
nothing new (FR-015, no duplicate server).

**Rationale**: spec scopes this slice to start-on-demand + reuse; no OS service unit. The
helper reuses `serve::run` (no reimplementation) and returns control so the wizard can
report + open. Reachability before start is what prevents a duplicate server.

**Alternatives rejected**:
- *Always start a new server* — duplicates listeners, fails SC-004. Rejected.
- *Background OS service / daemonization* — explicitly out of scope. Rejected.

## D4 — Browser open (US2, FR-011)

**Decision**: a `BrowserOpener` seam; the real impl shells the OS opener via
`std::process::Command` — `cmd /c start "" <url>` (Windows), `open <url>` (macOS),
`xdg-open <url>` (Linux). Headless/no-opener (Linux with no `DISPLAY`, or the command
fails / is absent) → print the URL and skip (never an error). Tests use
`NoopBrowserOpener` (records the URL, opens nothing).

**Rationale**: no `open`/`webbrowser` crate exists and adding one for three `Command`
lines is the wrong trade (ponytail rung 4 / no new dep). The seam keeps the flow
side-effect-free in tests (FR-017) and the headless fallback satisfies FR-011.

**Alternatives rejected**:
- *Add the `open`/`webbrowser` crate* — a dependency for ~10 lines of `Command`. Rejected.
- *Always open (no headless guard)* — errors in CI/containers. Rejected.

## D5 — Operator setup profile persistence (FR-012, Key Entity)

**Decision**: `OperatorSetupProfile { installation_type, port, auth_posture, harnesses,
updated_ms }` serialized to `<project>/.symforge/operator-setup.json` via `load()/save()`,
mirroring `OnboardingState` (`onboarding.rs:20`, `state_path` → `resolve_symforge_dir`).
Missing/malformed → treat as "no prior setup" (fresh run), never a hard error.

**Rationale**: reuses the proven local-state pattern + `.symforge/` dir; operator
convenience state, not an index (Constitution I unaffected). Drives reuse-if-running +
idempotent re-run (FR-013).

**Alternatives rejected**:
- *Global ~/.symforge profile* — per-project is correct (the bound port + harnesses are
  project-scoped); the existing onboarding state is project-local too. Rejected.

## D6 — Auth posture + reachability (FR-007, FR-015/020)

**Decision**: reuse `AuthConfig::refuse_to_start(is_loopback)` (auth.rs:59) — loopback
bind requires no key; a non-loopback bind requires a key (operator supplies, or the
wizard generates one). A generated key for a network bind is passed via env/`--api-key-env`
(reuse `enforce_api_key_source_policy`, serve.rs:88), **never inline** (process-listing
leak). Reachability = an HTTP GET to `/api/v1/summary` with a short timeout (the
`sidecar_port_is_alive` pattern lifted to HTTP), confirming the bound URL actually serves
(FR-020) before reporting it.

**Rationale**: the secure-default refuse-to-start rule already exists and is the right
gate; the HTTP reachability check turns "bound" into "actually serving", killing the
advertised-but-dead URL class (the original 8787 failure mode).

**Alternatives rejected**:
- *Reimplement an auth policy* — the wizard orchestrates the existing one. Rejected.
- *TCP-connect-only reachability* — a bound-but-not-serving socket would pass; an HTTP GET
  proves the dashboard answers (FR-020). Rejected.

## D7 — Testability seams (FR-017/018, cross-cutting)

**Decision**: every operator-facing side effect behind an injectable seam —
`SetupSink` (terminal I/O: ask/confirm/summary/status; real `StderrSetupSink` +
`ScriptedSetupSink` for pre-supplied non-interactive answers), `BrowserOpener` (D4), and
a port-probe injection point so tests assert port selection without real binds where
possible. Harness apply/backup is already fixtures-driven (`harness_apply` + a temp
`home`/`working_dir` via `HarnessRegistry::known_with`). No test mutates a real harness
config (FR-018).

**Rationale**: mirrors the shipped `OnboardingSink` seam; makes the entire wizard/admin
flow drivable with scripted answers and asserted without terminal/network/browser/process
side effects (FR-014/017) — the SC-006 "fixtures only" bar.

**Alternatives rejected**:
- *Integration-only testing against real configs* — violates FR-018; flaky + destructive.
  Rejected.

## Cross-cutting

- **No new dependency, no new feature flag.** All 009 code is `#[cfg(feature = "server")]`
  (it configures/starts serve); embed compiles none of it (FR-019, Constitution VI).
- **Reported URL == bound URL** is enforced by binding first, then probing reachability,
  then reporting (FR-020).
- **Per-phase gate** (FR-019 of 010 carried as project norm): fmt/check/clippy/test/
  build --release/embed after each phase; run via terminal-commander with error/stall
  comb rules so a hung or failing compile is caught structurally.
