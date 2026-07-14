# Codex V2 `close_agent` live-namespace diagnostic

Date: 2026-07-14<br>
Scope: read-only deployment diagnostic plus disposable process smoke; no new Codex product edit<br>
Candidate branch: `fix/multi-agent-v2-close-agent`<br>
Pinned base: `rust-v0.144.4` / `8c68d4c87dc54d38861f5114e920c3de2efa5876`<br>
Candidate SHA-256: `86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00`

## Executive result

The reviewed V2 handler and teardown bridge work, but the default deployment is
not currently usable against the live ChatGPT-backed API. Before model
execution, the API rejects the newly advertised function with HTTP 400:

> `Function 'collaboration.close_agent' is not allowed in reserved namespace 'collaboration'.`

This is an integration contract outside the open-source handler. The same
binary, model, auth route, prompt, and MCP sentinel are accepted when Codex's
already-supported V2 setting changes only the tool namespace to `agents`:

```toml
[features.multi_agent_v2]
tool_namespace = "agents"
```

That A/B result isolates the failure to the reserved namespace allowlist. It
does not justify changing all V2 tools' source default in this patch.

## Code and test state before the live smoke

- Independent source review verdict:
  `research/token-cost/claude-report-codex-v2-close-agent-code-review-a019.md`
  → `APPROVE_CONTINUE`.
- Focused V2 family: 65/65 passed.
- Default namespace, configured namespace, code-mode exposure, and
  close/follow-up/list behavior tests passed.
- A helper-complete `just test -p codex-core` ran 2,594 tests: 2,582 passed,
  11 failed, 1 timed out, and 46 skipped. No V2 close/spec-plan test failed.
- Residual failures were outside the seven-file patch: Windows symlink and
  elevation behavior, an unrelated missing `codex-command-runner` helper,
  network/mock timing, hook-log flakiness, CLI-stream leakage, and a remote
  compaction timeout. The package gate therefore remains red, not green.
- Generated stable-tag `Cargo.lock` churn was restored from `HEAD` after the
  Cargo commands.

## Live A/B protocol

Both meaningful attempts used:

- standalone candidate `codex.exe` 0.144.4, not the installed Codex;
- model `gpt-5.6-sol` with low reasoning;
- `features.multi_agent_v2=true`;
- an isolated empty workspace and isolated `CODEX_HOME` containing only a
  copied auth file;
- `target/debug/test_stdio_server.exe` as one stdio MCP process per resident
  agent;
- a 250 ms external process-transition monitor;
- a separately owned unrelated Node sentinel, checked alive after worker
  close and stopped explicitly afterward.

The controller sequence initialized the root MCP, spawned canonical task
`/root/sentinel_worker`, waited for `WORKER_READY`, held before close, called
`close_agent`, attempted a forbidden post-close follow-up, held after close,
and checked that the worker was absent.

An earlier attempt with the UI alias `fable` is excluded: the API rejected the
unsupported model alias before treatment. Its sole root MCP process exited
with the failed root session.

## Default reserved namespace: fail before treatment

With the source default `tool_namespace = "collaboration"`:

- root MCP PID 31296 started;
- the live API returned HTTP 400 on the tool declaration before the model
  could call a tool;
- PID 31296 exited with the failed root session;
- no worker was spawned, so this attempt provides no close-path evidence.

## Configured `agents` namespace: mechanism succeeds

With only `features.multi_agent_v2.tool_namespace='agents'` added:

- the API accepted the tool set and the candidate exited 0;
- root MCP PID 62076 started;
- V2 worker MCP PID 59356 appeared, producing a 1 → 2 owned-process
  transition;
- `close_agent` resolved `/root/sentinel_worker`, completed, and returned the
  worker's previous status as `completed` with message `WORKER_READY`;
- at that close, PID 59356 disappeared and PID 62076 remained: 2 → 1;
- `followup_task` then failed with `live agent path '/root/sentinel_worker' not
  found`, proving post-close unavailability;
- the root MCP survived and accepted the post-close call sequence;
- PID 62076 exited only when the root session ended: 1 → 0;
- the unrelated sentinel job remained running through worker close and root
  exit, then was stopped explicitly.

The model's final self-grade was `smoke=FAIL` because both 15-second MCP
`sync` holds returned `user cancelled MCP tool call`. Those calls did provide
the intended observation windows, but their cancellation makes this a
diagnostic mechanism proof, not the final passing smoke. The final run should
use 5-second holds, below the observed client-cancellation boundary.

## Cleanup and custody

- Both failed candidate root sessions reaped their sole MCP child.
- The configured-namespace run reaped the worker at close and root MCP at
  root exit.
- The transition monitor and unrelated sentinel were stopped explicitly.
- The isolated smoke home was deleted immediately after the diagnostic because
  it contained a copied auth file; no credential value was read or persisted
  into repository evidence.
- The empty smoke workspace was deleted.
- The Codex worktree and Cargo target remain intentionally retained for the
  next review/final-smoke checkpoint.

## Decision options

1. **Supported config override for local deployment (recommended now).** Keep
   the reviewed seven-file product patch unchanged and configure V2's existing
   `tool_namespace` setting to `agents` when installing this local candidate.
   This is the smallest reversible change and has live A/B evidence.
2. **Change Codex's source default from `collaboration` to `agents`.** This is
   mechanically small but renames all seven V2 functions, not only the new
   one. It is a broader compatibility/product decision and should not be
   smuggled into the lifecycle bridge.
3. **Wait for the live API allowlist to add `collaboration.close_agent`.** This
   preserves the intended reserved namespace but leaves the user's process
   leak unresolved and has no known delivery date.
4. **Split only `close_agent` into another namespace.** This preserves six
   existing names but creates a mixed collaboration surface and special-case
   registration. It is less coherent than the supported whole-surface
   namespace setting.

Recommended next checkpoint: independent review of this diagnosis and option
1. If approved, run the final 5-second-hold sentinel smoke through `agents`,
run the required `just fix -p codex-core`/format checks, install the pinned
binary plus the reversible config setting, restart Codex, and verify the live
tool surface before committing and cleaning the disposable target/worktree.
