---
goal_id: SFR27
title: Final review burndown audit and release-readiness evidence
chain_id: symforge-code-review-2026-05-22
phase: Wave 5 - closure
status: "Completed"
depends_on: ["SFR01", "SFR02", "SFR03", "SFR04", "SFR05", "SFR07", "SFR08", "SFR09", "SFR10", "SFR11", "SFR12", "SFR13", "SFR14", "SFR15"]
target_branch: "goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr27-final-review-burndown-audit-and-release-readiness-evidence"
created_at: "2026-05-22"
started_at: "2026-05-23T03:02:53.6521203+02:00"
completed_at: "2026-05-23T03:15:38.6515299+02:00"
completion_commit: "3b7690c5bd8119ef632aa40518f720a4b5f7a225"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "low"
source_refs:
  - "docs/code-review-2026-05-22.md has a master list of 60 findings and recommended planning phases."
  - "The review says cargo check and npm tests passed, full cargo test was still running sidecar integration, and cargo build --release was not run."
---

# SFR27 - Final review burndown audit and release-readiness evidence

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR27-final-review-burndown-audit-and-release-readiness-evidence.md
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
goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr27-final-review-burndown-audit-and-release-readiness-evidence/.git" ] || [ -f ".worktrees/sfr27-final-review-burndown-audit-and-release-readiness-evidence/.git" ]; then
  cd .worktrees/sfr27-final-review-burndown-audit-and-release-readiness-evidence
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence .worktrees/sfr27-final-review-burndown-audit-and-release-readiness-evidence origin/main
  cd .worktrees/sfr27-final-review-burndown-audit-and-release-readiness-evidence
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence`.

## Dependency Guard

Depends on: `SFR01, SFR02, SFR03, SFR04, SFR05, SFR07, SFR08, SFR09, SFR10, SFR11, SFR12, SFR13, SFR14, SFR15`.

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
- Findings covered: SF-001..SF-060
- Current known state: docs/code-review-2026-05-22.md has a master list of 60 findings and recommended planning phases.
- Desired end state: Produce a final burndown artifact that proves which 2026-05-22 findings are fixed, deferred, superseded, or still blocked, with verification evidence suitable for release planning.

## Code Evidence

- docs/code-review-2026-05-22.md has a master list of 60 findings and recommended planning phases.
- The review says cargo check and npm tests passed, full cargo test was still running sidecar integration, and cargo build --release was not run.

## Mini-Spec

objective:
- Produce a final burndown artifact that proves which 2026-05-22 findings are fixed, deferred, superseded, or still blocked, with verification evidence suitable for release planning.

non_goals:
- Do not implement new fixes in this closure goal.
- Do not mark deferred items as completed.
- Do not hide partial verification.

allowed_files_or_area:
- .agent/goals/symforge-code-review-2026-05-22/**
- docs/code-review-2026-05-22.md
- docs/live-code-backlog.md
- CHANGELOG.md
- README.md

forbidden_files:
- src/**
- tests/**
- npm/**
- .github/**
- Cargo.toml
- Cargo.lock

contracts_or_interfaces:
- Every SF finding has one disposition: fixed with commit/evidence, deferred with reason, superseded, false positive, or blocked.
- Release-readiness claims must be backed by verification commands.

invariants:
- No implementation work in closure goal.
- No false 'all green' claim when a verification command was skipped or failed.

implementation_steps:
- Read all completed SFR goal files and collect completion commits/verification evidence.
- Update FINDING_STATUS_REGISTER.md with final dispositions.
- Run or cite full verification: cargo fmt, clippy, check, test, release build, npm test, or document exact blockers.
- Write a release-readiness summary and residual risk register.

acceptance_criteria:
- Every SF-001 through SF-060 row has a final disposition and evidence pointer.
- Residual risks are explicit and not buried in prose.
- No source code changed.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence`.
- The goal requires touching forbidden files.
- The goal expands into another goal's scope.
- A required interface, route, package, repository path, branch, runtime dependency, GitHub state, or source file contradicts this mini-spec.
- Verification cannot run for a reason that is not clearly pre-existing and documented.
- A security, credential, data-retention, privacy, production-safety, public API, backward-compatibility, or destructive-change question appears that is not answered by this goal file.
- The working tree contains unrelated dirty changes.

verification_command:

```bash
git diff --check
rg "SF-001|SF-060|Blocked|Deferred|Completed|Superseded|False positive" .agent/goals/symforge-code-review-2026-05-22 -n
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

Run SFR27 only on branch `goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Produce a final burndown artifact that proves which 2026-05-22 findings are fixed, deferred, superseded, or still blocked, with verification evidence suitable for release planning.

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
