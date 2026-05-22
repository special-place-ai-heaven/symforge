---
goal_id: SFR16
title: Mitigate Windows libgit2 lockfile flake in git test helpers
chain_id: symforge-code-review-2026-05-22
phase: Wave 3 - test stability
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers"
created_at: "2026-05-22"
started_at: "2026-05-23T00:12:19.7875445+02:00"
completed_at: "2026-05-23T00:16:07.1238090+02:00"
completion_commit: "6f52074"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "docs/live-code-backlog.md item 1 describes intermittent Windows failures when libgit2 cannot rename `.git/refs/heads/*` lockfiles."
  - "The review maps this to git/test_helpers.rs and related frecency/persist tests."
---

# SFR16 - Mitigate Windows libgit2 lockfile flake in git test helpers

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers.md
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
goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers/.git" ] || [ -f ".worktrees/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers/.git" ]; then
  cd .worktrees/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers .worktrees/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers origin/main
  cd .worktrees/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers`.

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
- Findings covered: SF-037, docs/live-code-backlog.md#1
- Current known state: docs/live-code-backlog.md item 1 describes intermittent Windows failures when libgit2 cannot rename `.git/refs/heads/*` lockfiles.
- Desired end state: Add retry/backoff or process-git fallback around affected git test helpers so Windows lockfile flakiness does not require ignores or repeated manual reruns.

## Code Evidence

- docs/live-code-backlog.md item 1 describes intermittent Windows failures when libgit2 cannot rename `.git/refs/heads/*` lockfiles.
- The review maps this to git/test_helpers.rs and related frecency/persist tests.

## Mini-Spec

objective:
- Add retry/backoff or process-git fallback around affected git test helpers so Windows lockfile flakiness does not require ignores or repeated manual reruns.

non_goals:
- Do not change production git behavior unless proven affected.
- Do not add sleeps to unrelated tests.
- Do not hide real git failures as flake retries indefinitely.

allowed_files_or_area:
- src/git/test_helpers.rs
- tests/**git*
- tests/**frecency*
- tests/**persist*
- docs/live-code-backlog.md

forbidden_files:
- src/git/** production modules except test-only cfg sections
- src/protocol/**
- npm/**

contracts_or_interfaces:
- Retry budget is finite and logs/returns the final real error.
- Non-Windows behavior remains effectively unchanged.

invariants:
- No `#[ignore]` is added to avoid the flake.
- Retries must target known lockfile/rename failure modes, not all git errors.

implementation_steps:
- Identify helper functions that create many commits/refs and are used by flaking tests.
- Wrap only known libgit2 lockfile/rename operations with bounded retry/backoff or switch helper to process git where safer.
- Add tests or platform-gated assertions that exercise retry classification.

acceptance_criteria:
- Affected helpers have bounded retry or process fallback with clear comments.
- No new ignored test is introduced.
- Backlog item 1 is marked complete or narrowed with evidence.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers`.
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
cargo test git test_helpers frecency persist -- --test-threads=1
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

Run SFR16 only on branch `goal/sfr16-mitigate-windows-libgit2-lockfile-flake-in-git-test-helpers`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Add retry/backoff or process-git fallback around affected git test helpers so Windows lockfile flakiness does not require ignores or repeated manual reruns.

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
