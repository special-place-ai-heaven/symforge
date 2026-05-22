---
goal_id: SFR00
title: Reconcile review state and existing backlog goals
chain_id: symforge-code-review-2026-05-22
phase: Wave 0 - reconciliation
status: "Completed"
depends_on: []
target_branch: "goal/sfr00-reconcile-review-state-and-existing-backlog-goals"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr00-reconcile-review-state-and-existing-backlog-goals"
created_at: "2026-05-22"
started_at: "2026-05-22T17:57:34.5939056+02:00"
completed_at: "2026-05-22T18:14:56.6060372+02:00"
completion_commit: "4dd4a4a"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "low"
source_refs:
  - "docs/code-review-2026-05-22.md: review says some backlog items are already implemented and SymForge MCP was not indexed during that review."
  - "Repository branches observed: main and backlog-implementation; backlog-implementation is behind main with no ahead commits."
  - "Existing goal example .agent/goals/symforge-live-code-backlog/SFB02 is completed and uses branch backlog-implementation."
---

# SFR00 - Reconcile review state and existing backlog goals

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR00-reconcile-review-state-and-existing-backlog-goals.md
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
goal/sfr00-reconcile-review-state-and-existing-backlog-goals
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr00-reconcile-review-state-and-existing-backlog-goals`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr00-reconcile-review-state-and-existing-backlog-goals/.git" ] || [ -f ".worktrees/sfr00-reconcile-review-state-and-existing-backlog-goals/.git" ]; then
  cd .worktrees/sfr00-reconcile-review-state-and-existing-backlog-goals
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr00-reconcile-review-state-and-existing-backlog-goals .worktrees/sfr00-reconcile-review-state-and-existing-backlog-goals origin/main
  cd .worktrees/sfr00-reconcile-review-state-and-existing-backlog-goals
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr00-reconcile-review-state-and-existing-backlog-goals`.

## Dependency Guard

Depends on: `none`.

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
- Findings covered: SF-010, SF-011, SF-060, existing .agent/goals/symforge-live-code-backlog
- Current known state: docs/code-review-2026-05-22.md: review says some backlog items are already implemented and SymForge MCP was not indexed during that review.
- Desired end state: Create an evidence register that reconciles the 2026-05-22 review findings against current main, existing SFB goal files, and completed/obsolete backlog items before any code-changing goal runs.

## Code Evidence

- docs/code-review-2026-05-22.md: review says some backlog items are already implemented and SymForge MCP was not indexed during that review.
- Repository branches observed: main and backlog-implementation; backlog-implementation is behind main with no ahead commits.
- Existing goal example .agent/goals/symforge-live-code-backlog/SFB02 is completed and uses branch backlog-implementation.

## Mini-Spec

objective:
- Create an evidence register that reconciles the 2026-05-22 review findings against current main, existing SFB goal files, and completed/obsolete backlog items before any code-changing goal runs.

non_goals:
- Do not change production source code.
- Do not mark review findings fixed without file/line evidence.
- Do not revive backlog-implementation as a target branch.

allowed_files_or_area:
- .agent/goals/symforge-code-review-2026-05-22/**
- docs/code-review-2026-05-22.md only for an optional errata note if evidence proves a false positive

forbidden_files:
- src/**
- tests/**
- npm/**
- .github/**
- Cargo.toml
- Cargo.lock

contracts_or_interfaces:
- The output must distinguish live issues, already-fixed issues, false positives, deferred strategic items, and blocked items.

invariants:
- No code changes in this reconnaissance goal.
- Every status change must cite a concrete path and, when possible, a line or test name.

implementation_steps:
- Run `git fetch origin`, inspect `origin/main`, and compare any existing non-main branch before editing.
- Inventory `.agent/goals/symforge-live-code-backlog/` and note any completed SFB work that overlaps SF-010/SF-011/SF-045/SF-030.
- Create or update `FINDING_STATUS_REGISTER.md` in this chain with one row per SF finding and a suggested next goal or disposition.
- Create `EXISTING_GOAL_OVERLAP.md` listing SFB goals that should not be duplicated by this chain.

acceptance_criteria:
- `FINDING_STATUS_REGISTER.md` exists and classifies SF-001 through SF-060.
- `EXISTING_GOAL_OVERLAP.md` lists all overlapping SFB goals discovered in the repo and says whether to skip, supersede, or use as evidence.
- No source-code files are changed.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr00-reconcile-review-state-and-existing-backlog-goals`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr00-reconcile-review-state-and-existing-backlog-goals`.
- The goal requires touching forbidden files.
- The goal expands into another goal's scope.
- A required interface, route, package, repository path, branch, runtime dependency, GitHub state, or source file contradicts this mini-spec.
- Verification cannot run for a reason that is not clearly pre-existing and documented.
- A security, credential, data-retention, privacy, production-safety, public API, backward-compatibility, or destructive-change question appears that is not answered by this goal file.
- The working tree contains unrelated dirty changes.

verification_command:

```bash
git diff --check
rg "SF-001|SF-060|SFB02|backlog-implementation" .agent/goals/symforge-code-review-2026-05-22 -n
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

Run SFR00 only on branch `goal/sfr00-reconcile-review-state-and-existing-backlog-goals`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Create an evidence register that reconciles the 2026-05-22 review findings against current main, existing SFB goal files, and completed/obsolete backlog items before any code-changing goal runs.

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
