---
goal_id: SFR12
title: Expose snapshot verification mismatches and background verify progress
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - recovery artifacts
status: "Completed"
depends_on: ["SFR11"]
target_branch: "goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress"
created_at: "2026-05-22"
started_at: "2026-05-22T22:27:08+02:00"
completed_at: "2026-05-22T22:54:06+02:00"
completion_commit: "c1bf125"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "src/live_index/persist.rs spot_verify_sample returns mismatched paths from deterministic sample verification."
  - "src/main.rs loads snapshots on warm start and spawns background_verify."
  - "The review recommends exposing mismatch lists and verify progress in health or repair tools."
---

# SFR12 - Expose snapshot verification mismatches and background verify progress

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR12-expose-snapshot-verification-mismatches-and-background-verify-progress.md
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
goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress/.git" ] || [ -f ".worktrees/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress/.git" ]; then
  cd .worktrees/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress .worktrees/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress origin/main
  cd .worktrees/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress`.

## Dependency Guard

Depends on: `SFR11`.

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
- Findings covered: SF-021, SF-032, SF-034
- Current known state: src/live_index/persist.rs spot_verify_sample returns mismatched paths from deterministic sample verification.
- Desired end state: Surface snapshot verification state, mismatch paths, and background verify progress in health/health_compact or a dedicated status resource so agents can see stale/corrupt recovery state.

## Code Evidence

- src/live_index/persist.rs spot_verify_sample returns mismatched paths from deterministic sample verification.
- src/main.rs loads snapshots on warm start and spawns background_verify.
- The review recommends exposing mismatch lists and verify progress in health or repair tools.

## Mini-Spec

objective:
- Surface snapshot verification state, mismatch paths, and background verify progress in health/health_compact or a dedicated status resource so agents can see stale/corrupt recovery state.

non_goals:
- Do not implement full repair_index unless SFR10 selects that path.
- Do not make health output noisy for clean states.
- Do not reindex automatically just because a mismatch is listed unless existing code already does so.

allowed_files_or_area:
- src/live_index/store.rs
- src/live_index/persist.rs
- src/protocol/tools.rs
- src/protocol/resources.rs
- src/protocol/format.rs
- tests/**health*
- tests/**persist*
- tests/conformance.rs

forbidden_files:
- src/protocol/edit.rs
- src/daemon.rs except daemon health passthrough tests
- npm/**

contracts_or_interfaces:
- Health text remains readable and compact health remains compact.
- Machine-readable result_status remains correct for health tools if used.
- Mismatch paths are bounded/truncated with a count when many exist.

invariants:
- Do not claim verification complete while a background verify task is still running.
- Do not hide mismatch count if path list is truncated.

implementation_steps:
- Find existing SnapshotVerifyState fields and extend them if needed to carry progress/mismatch summary.
- Render verification status in health and health_compact with clean-state minimalism.
- Add tests for in-progress, clean, and mismatch states.

acceptance_criteria:
- Health surfaces snapshot load source and verify state in a way evaluation harnesses can assert.
- Mismatch count and at least bounded example paths appear when mismatches exist.
- Clean state output remains concise.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress`.
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
cargo test health snapshot verify -- --test-threads=1
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

Run SFR12 only on branch `goal/sfr12-expose-snapshot-verification-mismatches-and-background-verify-progress`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Surface snapshot verification state, mismatch paths, and background verify progress in health/health_compact or a dedicated status resource so agents can see stale/corrupt recovery state.

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
