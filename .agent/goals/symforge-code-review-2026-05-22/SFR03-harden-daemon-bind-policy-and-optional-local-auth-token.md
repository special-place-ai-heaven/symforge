---
goal_id: SFR03
title: Harden daemon bind policy and optional local auth token
chain_id: symforge-code-review-2026-05-22
phase: Wave 1 - daemon hardening
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token"
created_at: "2026-05-22"
started_at: "2026-05-22T18:39:25.6338811+02:00"
completed_at: "2026-05-22T19:03:09.4372809+02:00"
completion_commit: "b21b24b"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "src/daemon.rs build_router exposes /health, /v1/projects, /v1/sessions/open, /v1/sessions/{session_id}/tools/{tool_name}, and sidecar routes without an auth layer."
  - "src/daemon.rs spawn_daemon reads SYMFORGE_DAEMON_BIND and binds `{resolved_host}:0`."
  - "The review flags unauthenticated daemon HTTP plus configurable bind as a P1 security risk."
---

# SFR03 - Harden daemon bind policy and optional local auth token

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR03-harden-daemon-bind-policy-and-optional-local-auth-token.md
```

## Goal File Workflow

0. Treat this markdown file as the whole prompt. Do not ask the user for extra instructions. If the task cannot be completed safely, mark it `Blocked` and explain exactly why in the final report.
1. Run the Branch Guard before editing this file, source code, tests, npm files, docs, generated artifacts, or Cargo metadata.
2. After Branch Guard passes, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
3. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`. Do not expand into adjacent review findings unless this file explicitly says so.
4. If a stop condition is hit, stop implementation, set `status` to `Blocked`, set `blocked_reason`, leave `completion_commit` empty, and commit the status update if committing is safe.
5. When acceptance criteria pass, run the verification command exactly as written unless the command is impossible for a documented pre-existing reason.
6. Commit the verified implementation work first. Then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
7. Commit the goal-status update as a separate commit.
8. After squash-merging this sprint to `main`, archive the sprint branch according to operator policy.

## Branch Guard

This goal belongs only to branch:

```text
goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token/.git" ] || [ -f ".worktrees/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token/.git" ]; then
  cd .worktrees/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token .worktrees/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token origin/main
  cd .worktrees/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token`.

## Dependency Guard

Depends on: `SFR00`.

If `depends_on` is not empty, inspect the referenced goal file(s) under `.agent/goals/symforge-code-review-2026-05-22/` when present. If a dependency is absent or not marked `Completed`, continue only if current code already contains that dependency's acceptance artifacts. Otherwise mark this goal `Blocked` with evidence.

## SymForge Goal Discipline

- Work from current code and the committed `docs/code-review-2026-05-22.md`, not from historical plans alone.
- Preserve the local-first architecture: in-process `LiveIndex` and `.symforge/` local state remain runtime truth.
- Preserve byte-exact source handling. Do not normalize line endings or rewrite source bytes casually.
- Preserve MCP tool names, schemas, result-status contracts, npm packaging, and daemon behavior unless this goal explicitly changes one and adds tests.
- Do not turn mock, stale, degraded, disabled, blocked, unavailable, or unknown state into success.
- If a finding is already implemented, add evidence/tests or mark it in the register instead of duplicating code.
- If a public contract changes, update conformance/schema tests and document compatibility impact.

## Mission Context

- Target project: `special-place-administrator/symforge`
- Goal chain: `symforge-code-review-2026-05-22`
- Review source: `docs/code-review-2026-05-22.md`
- Findings covered: SF-004, SF-040
- Current known state: src/daemon.rs build_router exposes /health, /v1/projects, /v1/sessions/open, /v1/sessions/{session_id}/tools/{tool_name}, and sidecar routes without an auth layer.
- Desired end state: Make the daemon safe by default for local use: enforce localhost-only default binding, warn or reject non-loopback binds unless explicitly allowed, and add optional bearer-token protection for tool/session routes without breaking default local workflows.

## Code Evidence

- src/daemon.rs build_router exposes /health, /v1/projects, /v1/sessions/open, /v1/sessions/{session_id}/tools/{tool_name}, and sidecar routes without an auth layer.
- src/daemon.rs spawn_daemon reads SYMFORGE_DAEMON_BIND and binds `{resolved_host}:0`.
- The review flags unauthenticated daemon HTTP plus configurable bind as a P1 security risk.

## Mini-Spec

objective:
- Make the daemon safe by default for local use: enforce localhost-only default binding, warn or reject non-loopback binds unless explicitly allowed, and add optional bearer-token protection for tool/session routes without breaking default local workflows.

non_goals:
- Do not redesign MCP stdio mode.
- Do not expose the daemon for remote production use.
- Do not change tool handlers except to pass through authenticated requests.

allowed_files_or_area:
- src/daemon.rs
- src/cli/**
- tests/daemon*.rs
- tests/sidecar*.rs
- README.md
- AGENTS.md
- CLAUDE.md

forbidden_files:
- src/protocol/tools.rs
- src/live_index/**
- npm/** except if npm launcher must pass a new env var and this is explicitly justified

contracts_or_interfaces:
- Default daemon bind is loopback.
- Non-loopback bind requires an explicit opt-in variable and a visible warning.
- If a token is configured, all mutating/session/tool routes require it; health may remain unauthenticated only if tests document why.

invariants:
- No auth token is printed in logs.
- Existing local daemon-backed client still works with no token in default localhost mode.

implementation_steps:
- Introduce a narrow daemon auth config struct/function that reads env vars and validates bind host.
- Apply middleware or per-handler checks to session-opening, session-closing, heartbeat, project, tool, and sidecar routes according to the chosen contract.
- Add tests for default localhost/no-token, non-loopback rejection or explicit opt-in warning, token-required success, and token-missing failure.
- Document the daemon threat model and safe bind behavior.

acceptance_criteria:
- Daemon rejects or loudly gates non-loopback binds without explicit operator opt-in.
- When a token env var is set, protected routes reject missing/wrong token and accept the right token.
- No test captures token values in logs or response bodies.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token`.
- The goal requires touching forbidden files.
- The goal expands into another goal's scope.
- A required interface, route, package, repository path, branch, runtime dependency, GitHub state, or source file contradicts this mini-spec.
- Verification cannot run for a reason that is not clearly pre-existing and documented.
- A security, credential, data-retention, privacy, production-safety, public API, backward-compatibility, or destructive-change question appears that is not answered by this goal file.
- The working tree contains unrelated dirty changes.

verification_command:

```bash
git diff --check
cargo fmt --check
cargo test daemon -- --test-threads=1
cargo test --test daemon_aliases
cargo check
```

Default full verification, when task-specific verification passes and time permits:

```bash
git branch --show-current
git diff --check
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
```

If this goal changes `npm/**`, also run:

```bash
cd npm && npm test
```

## Task Prompt

Run SFR03 only on branch `goal/sfr03-harden-daemon-bind-policy-and-optional-local-auth-token`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Make the daemon safe by default for local use: enforce localhost-only default binding, warn or reject non-loopback binds unless explicitly allowed, and add optional bearer-token protection for tool/session routes without breaking default local workflows.

Changes:
- <focused list of implementation changes>

Files changed:
- <paths>

Verification:
- PASS/FAIL: `<command>` — <summary>

Evidence:
- <source-status notes, test output summaries, route/status evidence, screenshots only if rendered UI changed>

Commit:
- Verified work commit: `<hash or none>`
- Goal status commit: `<hash or none>`

Known gaps / blockers:
- <none or explicit blocker>

Next goal:
- <next goal ID and filename, or none>
