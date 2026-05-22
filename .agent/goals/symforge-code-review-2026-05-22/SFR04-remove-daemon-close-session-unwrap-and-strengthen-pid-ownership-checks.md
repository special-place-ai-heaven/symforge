---
goal_id: SFR04
title: Remove daemon close-session unwrap and strengthen PID ownership checks
chain_id: symforge-code-review-2026-05-22
phase: Wave 1 - daemon correctness and process safety
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks"
created_at: "2026-05-22"
started_at: "2026-05-22T19:04:52.4840919+02:00"
completed_at: "2026-05-22T19:24:03.1535195+02:00"
completion_commit: "63badac545b5477667352141e84a4e3871189c65"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "src/daemon.rs close_session finds an owning project and then uses `projects.get_mut(&pid).unwrap()`."
  - "src/daemon.rs terminate_process uses libc::kill on Unix under an unsafe-code allowance and taskkill on Windows."
  - "The review recommends avoiding theoretical unwrap races and documenting/ensuring PID-file ownership checks."
---

# SFR04 - Remove daemon close-session unwrap and strengthen PID ownership checks

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks.md
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
goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks/.git" ] || [ -f ".worktrees/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks/.git" ]; then
  cd .worktrees/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks .worktrees/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks origin/main
  cd .worktrees/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks`.

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
- Findings covered: SF-017, SF-018, SF-040
- Current known state: src/daemon.rs close_session finds an owning project and then uses `projects.get_mut(&pid).unwrap()`.
- Desired end state: Make daemon session close and daemon process termination robust against stale project/session state and unsafe PID-file misuse.

## Code Evidence

- src/daemon.rs close_session finds an owning project and then uses `projects.get_mut(&pid).unwrap()`.
- src/daemon.rs terminate_process uses libc::kill on Unix under an unsafe-code allowance and taskkill on Windows.
- The review recommends avoiding theoretical unwrap races and documenting/ensuring PID-file ownership checks.

## Mini-Spec

objective:
- Make daemon session close and daemon process termination robust against stale project/session state and unsafe PID-file misuse.

non_goals:
- Do not implement daemon auth; SFR03 owns auth.
- Do not change public daemon HTTP routes.
- Do not remove libc::kill if it remains the correct Unix primitive.

allowed_files_or_area:
- src/daemon.rs
- tests/daemon*.rs
- tests/sidecar*.rs
- README.md or AGENTS.md for threat-model documentation

forbidden_files:
- src/protocol/tools.rs
- src/live_index/**
- npm/**

contracts_or_interfaces:
- Closing an unknown or orphan session returns the existing not-found/orphan semantics.
- Terminating a process must not blindly trust a PID file that belongs to a different user or unrelated executable when that can be checked.

invariants:
- No panic on stale project/session state.
- Process termination remains idempotent when the process is already gone.

implementation_steps:
- Replace the unwrap in close_session with explicit stale-state handling and tests.
- Audit daemon runtime file readers/writers and add ownership/path/executable checks before terminate_process where feasible on the platform.
- Keep platform-specific behavior explicit and documented.

acceptance_criteria:
- A regression test proves close_session does not panic if the owning project disappears between lookup and mutation, or equivalent stale state is simulated.
- PID termination code has documented safety checks or explicit rationale where checks are impossible.
- Existing daemon lifecycle tests pass.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks`.
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

Run SFR04 only on branch `goal/sfr04-remove-daemon-close-session-unwrap-and-strengthen-pid-ownership-checks`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Make daemon session close and daemon process termination robust against stale project/session state and unsafe PID-file misuse.

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
