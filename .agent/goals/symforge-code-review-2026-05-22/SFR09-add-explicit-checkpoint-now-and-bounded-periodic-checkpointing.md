---
goal_id: SFR09
title: Add explicit checkpoint_now and bounded periodic checkpointing
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - idempotency and recovery
status: "Completed"
depends_on: ["SFR06"]
target_branch: "goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing"
created_at: "2026-05-22"
started_at: "2026-05-22T21:32:19+02:00"
completed_at: "2026-05-22T22:05:03+02:00"
completion_commit: "1c5f896"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "AGENTS.md says shutdown is not a safe persistence boundary and likely foundation tools include checkpoint_now."
  - "src/main.rs serializes index only on clean MCP server shutdown."
  - "src/main.rs already warm-starts by loading .symforge/index.bin and spawning background verification when a snapshot exists."
  - "tests/live_index_integration.rs currently asserts v1 tool names like checkpoint_now are not function definitions in protocol/tools.rs."
---

# SFR09 - Add explicit checkpoint_now and bounded periodic checkpointing

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing.md
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
goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing/.git" ] || [ -f ".worktrees/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing/.git" ]; then
  cd .worktrees/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing .worktrees/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing origin/main
  cd .worktrees/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing`.

## Dependency Guard

Depends on: `SFR06`.

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
- Findings covered: SF-003, SF-006, SF-032
- Current known state: AGENTS.md says shutdown is not a safe persistence boundary and likely foundation tools include checkpoint_now.
- Desired end state: Add a deliberately scoped checkpoint path that can serialize the current index on demand and optionally on a bounded interval, while preserving explicit tool-consolidation rules and not reviving old v1 run lifecycle names blindly.

## Code Evidence

- AGENTS.md says shutdown is not a safe persistence boundary and likely foundation tools include checkpoint_now.
- src/main.rs serializes index only on clean MCP server shutdown.
- src/main.rs already warm-starts by loading .symforge/index.bin and spawning background verification when a snapshot exists.
- tests/live_index_integration.rs currently asserts v1 tool names like checkpoint_now are not function definitions in protocol/tools.rs.

## Mini-Spec

objective:
- Add a deliberately scoped checkpoint path that can serialize the current index on demand and optionally on a bounded interval, while preserving explicit tool-consolidation rules and not reviving old v1 run lifecycle names blindly.

non_goals:
- Do not add get_index_run/cancel_index_run unless SFR10 explicitly defines run lifecycle.
- Do not add WAL in this goal.
- Do not make shutdown persistence the only recovery story.

allowed_files_or_area:
- src/protocol/tools.rs
- src/protocol/result_status.rs
- src/main.rs
- src/live_index/persist.rs
- tests/conformance.rs
- tests/live_index_integration.rs
- tests/**checkpoint*
- AGENTS.md
- README.md

forbidden_files:
- src/protocol/edit.rs
- npm/**

contracts_or_interfaces:
- If the tool is named `checkpoint_now`, update or replace INFR-05 no-v1-tools tests with an explicit current-contract test.
- Checkpoint writes use the existing atomic snapshot path.
- Periodic checkpointing is opt-in or conservatively configured and visible in health/docs.

invariants:
- No checkpoint result may claim success when serialize_shared_index failed.
- Checkpointing must not hold locks across await or block the async runtime unexpectedly.

implementation_steps:
- Decide whether `checkpoint_now` is a revived current tool or a differently named current v7 tool; document the decision in tests.
- Implement explicit snapshot checkpointing via existing persist::serialize_shared_index.
- Add optional bounded periodic checkpointing controlled by config/env or a documented default that does not surprise users.
- Update tests that previously asserted the old name was absent so they assert intentional current semantics instead.

acceptance_criteria:
- A checkpoint command/path produces `.symforge/index.bin` and reports success/failure accurately.
- A test proves checkpoint failure is not reported as success.
- The old-v1-name guard is updated honestly, not simply deleted.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing`.
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
cargo test checkpoint -- --test-threads=1
cargo test --test live_index_integration
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

Run SFR09 only on branch `goal/sfr09-add-explicit-checkpoint-now-and-bounded-periodic-checkpointing`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Add a deliberately scoped checkpoint path that can serialize the current index on demand and optionally on a bounded interval, while preserving explicit tool-consolidation rules and not reviving old v1 run lifecycle names blindly.

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
