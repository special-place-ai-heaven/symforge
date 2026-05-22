---
goal_id: SFR22
title: Split live_index query disambiguation, bundle, and health view helpers
chain_id: symforge-code-review-2026-05-22
phase: Wave 4 - maintainability refactors
status: "Completed"
depends_on: ["SFR05", "SFR12"]
target_branch: "goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers"
created_at: "2026-05-22"
started_at: "2026-05-23T01:32:31.1162729+02:00"
completed_at: "2026-05-23T01:49:29.7687322+02:00"
completion_commit: "ee6c3d2314089b91609bdb90ef07edeb3b131a9a"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "The review identifies src/live_index/query.rs as a roughly 7k-line module centralizing symbol/file resolution."
  - "SF-005 ambiguous symbol behavior and SFR12 verification status both depend on clear query/status boundaries."
---

# SFR22 - Split live_index query disambiguation, bundle, and health view helpers

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR22-split-live-index-query-disambiguation-bundle-and-health-view-helpers.md
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
goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers/.git" ] || [ -f ".worktrees/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers/.git" ]; then
  cd .worktrees/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers .worktrees/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers origin/main
  cd .worktrees/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers`.

## Dependency Guard

Depends on: `SFR05, SFR12`.

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
- Findings covered: SF-013
- Current known state: The review identifies src/live_index/query.rs as a roughly 7k-line module centralizing symbol/file resolution.
- Desired end state: Decompose `src/live_index/query.rs` into focused helper modules for disambiguation, context bundles, and health/status views without behavior changes.

## Code Evidence

- The review identifies src/live_index/query.rs as a roughly 7k-line module centralizing symbol/file resolution.
- SF-005 ambiguous symbol behavior and SFR12 verification status both depend on clear query/status boundaries.

## Mini-Spec

objective:
- Decompose `src/live_index/query.rs` into focused helper modules for disambiguation, context bundles, and health/status views without behavior changes.

non_goals:
- Do not change query ranking or symbol resolution behavior.
- Do not change public MCP outputs.
- Do not move protocol-level formatters.

allowed_files_or_area:
- src/live_index/query.rs
- src/live_index/query/**
- src/live_index/disambiguation.rs
- src/live_index/context_bundle.rs
- src/live_index/health_view.rs
- src/live_index/mod.rs
- tests/**query*
- tests/**symbol*
- tests/**health*

forbidden_files:
- src/protocol/tools.rs except import path updates
- src/protocol/format.rs
- npm/**

contracts_or_interfaces:
- Existing tests for search, symbol lookup, context bundles, and health remain green.
- Module split must not alter scoring/ranking semantics.

invariants:
- No behavior change in maintainability goal.
- No output change without a failing pre-existing test and explicit justification.

implementation_steps:
- Identify cohesive internal helper groups in query.rs.
- Move one group at a time, preserving function signatures where practical.
- Run focused tests after each move and full cargo check at the end.

acceptance_criteria:
- query.rs is smaller and new modules have focused names/responsibilities.
- No public behavior or output changes occur.
- Tests cover the moved functions through existing public paths.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers`.
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
cargo test query search symbol context health -- --test-threads=1
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

Run SFR22 only on branch `goal/sfr22-split-live-index-query-disambiguation-bundle-and-health-view-helpers`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Decompose `src/live_index/query.rs` into focused helper modules for disambiguation, context bundles, and health/status views without behavior changes.

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
