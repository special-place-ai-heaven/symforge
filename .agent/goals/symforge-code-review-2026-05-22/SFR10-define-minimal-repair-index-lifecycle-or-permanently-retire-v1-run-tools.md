---
goal_id: SFR10
title: Define minimal repair_index lifecycle or permanently retire v1 run tools
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - idempotency and recovery
status: "Blocked"
depends_on: ["SFR09", "SFR11", "SFR12"]
target_branch: "goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools"
created_at: "2026-05-22"
started_at: "2026-05-22T22:06:57+02:00"
completed_at: ""
completion_commit: ""
blocked_reason: "Dependency guard failed: SFR11 and SFR12 are still Pending, and current code lacks their required snapshot quarantine and background verification mismatch/progress acceptance artifacts. SFR10 should resume after SFR11 and SFR12 complete."
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "AGENTS.md describes deterministic repair paths and likely tools repair_index, get_index_run, cancel_index_run, get_index_run."
  - "tests/live_index_integration.rs asserts old v1 function definitions are absent in protocol/tools.rs."
  - "src/live_index/persist.rs has spot_verify_sample and snapshot load paths that can feed repair/checkpoint reporting."
---

# SFR10 - Define minimal repair_index lifecycle or permanently retire v1 run tools

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools.md
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
goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools/.git" ] || [ -f ".worktrees/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools/.git" ]; then
  cd .worktrees/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools .worktrees/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools origin/main
  cd .worktrees/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools`.

## Dependency Guard

Depends on: `SFR09, SFR11, SFR12`.

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
- Findings covered: SF-003, SF-006, SF-015, SF-021
- Current known state: AGENTS.md describes deterministic repair paths and likely tools repair_index, get_index_run, cancel_index_run, get_index_run.
- Desired end state: Close the repair/run-lifecycle gap by either implementing a minimal, current-contract repair surface or updating AGENTS/tests to permanently retire the old v1 run tools with an explicit replacement workflow.

## Code Evidence

- AGENTS.md describes deterministic repair paths and likely tools repair_index, get_index_run, cancel_index_run, get_index_run.
- tests/live_index_integration.rs asserts old v1 function definitions are absent in protocol/tools.rs.
- src/live_index/persist.rs has spot_verify_sample and snapshot load paths that can feed repair/checkpoint reporting.

## Mini-Spec

objective:
- Close the repair/run-lifecycle gap by either implementing a minimal, current-contract repair surface or updating AGENTS/tests to permanently retire the old v1 run tools with an explicit replacement workflow.

non_goals:
- Do not build a full durable run scheduler unless this goal is explicitly expanded in a new WorkSpec.
- Do not add fake run IDs that cannot be queried/resumed.
- Do not bypass SFR09 checkpoint semantics.

allowed_files_or_area:
- AGENTS.md
- README.md
- docs/live-code-backlog.md
- src/protocol/tools.rs
- src/protocol/resources.rs
- src/live_index/persist.rs
- tests/conformance.rs
- tests/live_index_integration.rs
- tests/**repair*

forbidden_files:
- src/daemon.rs except for daemon alias passthrough if a repair tool is implemented
- npm/**

contracts_or_interfaces:
- If a repair tool is implemented, it must expose machine-readable status and never treat skipped/unknown verification as success.
- If repair tools are retired, docs and tests must state the explicit replacement workflow.

invariants:
- No 'repair_index' tool may exist as a no-op placeholder.
- No run-lifecycle resource may claim resumability unless it is actually durable.

implementation_steps:
- Use SFR00 status register to choose implementation or retirement path.
- If implementing: build a minimal repair/check surface over snapshot quarantine and verify mismatch outputs.
- If retiring: update docs and tests so the current checkpoint/reset/verify workflow is explicit.

acceptance_criteria:
- The gap between AGENTS recovery rules and actual tool surface is closed by code or explicit retirement docs/tests.
- Conformance includes at least one repair/retirement contract case.
- No placeholder tool or stale doc claim remains.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools`.
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
cargo test repair checkpoint snapshot -- --test-threads=1
cargo test --test conformance
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

Run SFR10 only on branch `goal/sfr10-define-minimal-repair-index-lifecycle-or-permanently-retire-v1-run-tools`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Close the repair/run-lifecycle gap by either implementing a minimal, current-contract repair surface or updating AGENTS/tests to permanently retire the old v1 run tools with an explicit replacement workflow.

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
