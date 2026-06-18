# Quickstart: Operator Setup Wizard — validation guide

How to prove 009 works end-to-end. Per-phase gate (mechanical) + per-story acceptance.
Implementation detail lives in `tasks.md`.

## Per-phase gate (run after EACH phase, via terminal-commander for error/stall signals)
```sh
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed   # FR-019 — wizard is server-gated
```

## Acceptance by story

### US1 — collision-free serve port (Phase 1)
- `serve_binds_free_port_when_default_occupied`: occupy 8787, run serve (no explicit addr) →
  binds a different reachable port, GET to the reported URL succeeds, no dead listener (SC-003).
- Control: 8787 free → binds 8787. Explicit-occupied → loud conflict error.

### US2 — setup wizard (Phase 2)
- Drive `symforge setup --non-interactive` (ScriptedSetupSink + NoopBrowserOpener) over a
  temp `home`/`working_dir` with **fixture** harness configs:
  - scan summary reports detected harnesses / OS / suggested free port, changes nothing (FR-004);
  - apply configures exactly the chosen harnesses, each with a timestamped restorable backup,
    re-run adds no duplicate (SC-002);
  - server mode → reachable dashboard URL reported (FR-010/020); browser open recorded as
    Skipped by the noop opener (FR-011);
  - profile persisted at `.symforge/operator-setup.json` (FR-012);
  - re-run detects the profile + running server → refresh/no-op, no duplicate (FR-013).
- Headless: no `DISPLAY` → URL printed, open skipped, no error.

### US3 — admin verb + affordance (Phase 3)
- With a server running on the remembered port, `symforge admin` reuses it (opens that URL,
  no second server) (SC-004). With none, it starts one on a free port and opens it.
- The `symforge-admin` MCP prompt returns the dashboard URL. The Claude Code command file is
  installed (against a fixture `~/.claude/commands/`); harnesses without a command-file format
  get the MCP prompt only (no broken file).

## Keystone (SC-001/006)
From a bare install, a single guided command reaches a configured state (chosen harnesses
set up; for server mode a reachable dashboard) with at most a handful of prompts and no
manual config editing — and the whole thing is proven with fixtures only, the full gate +
embed build green.

## Live dogfood (operator-perspective, after the gate)
Build the local binary, run `symforge setup` for real in a scratch project, confirm the
dashboard opens on the reported port; occupy 8787 first to confirm the fallback. (The
fixture tests prove the mechanism; this confirms the real OS opener + bind.)
