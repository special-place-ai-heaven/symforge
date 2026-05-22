---
goal_id: SFR15
title: Add CI fmt, clippy, release build, and performance smoke gates
chain_id: symforge-code-review-2026-05-22
phase: Wave 3 - quality gates
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates"
created_at: "2026-05-22"
started_at: "2026-05-22T23:51:47.8255398+02:00"
completed_at: "2026-05-23T00:10:41.3503816+02:00"
completion_commit: "38f3a92"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - ".github/workflows/ci.yml currently runs cargo check and cargo test, plus npm tests, but not cargo fmt, cargo clippy, or cargo build --release."
  - "CLAUDE.md says verification includes cargo check, cargo test --all-targets, cargo build --release, and npm tests when relevant."
  - "tests/live_index_integration.rs has ignored 1000-file load perf coverage and tests/coupling_calibration.rs has an ignored real-repo calibration harness."
  - "Cargo.toml uses Rust edition 2024 and rust-toolchain is 1.94.0 per review."
---

# SFR15 - Add CI fmt, clippy, release build, and performance smoke gates

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates.md
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
goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates/.git" ] || [ -f ".worktrees/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates/.git" ]; then
  cd .worktrees/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates .worktrees/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates origin/main
  cd .worktrees/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates`.

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
- Findings covered: SF-022, SF-023, SF-024, SF-025
- Current known state: .github/workflows/ci.yml currently runs cargo check and cargo test, plus npm tests, but not cargo fmt, cargo clippy, or cargo build --release.
- Desired end state: Strengthen CI so formatting, clippy, release build, and selected performance/calibration evidence are checked in an appropriate cadence without making normal PR CI impractical.

## Code Evidence

- .github/workflows/ci.yml currently runs cargo check and cargo test, plus npm tests, but not cargo fmt, cargo clippy, or cargo build --release.
- CLAUDE.md says verification includes cargo check, cargo test --all-targets, cargo build --release, and npm tests when relevant.
- tests/live_index_integration.rs has ignored 1000-file load perf coverage and tests/coupling_calibration.rs has an ignored real-repo calibration harness.
- Cargo.toml uses Rust edition 2024 and rust-toolchain is 1.94.0 per review.

## Mini-Spec

objective:
- Strengthen CI so formatting, clippy, release build, and selected performance/calibration evidence are checked in an appropriate cadence without making normal PR CI impractical.

non_goals:
- Do not run heavyweight real-repo calibration on every push unless runtime is proven acceptable.
- Do not change Rust edition/toolchain unless explicitly required.
- Do not suppress clippy warnings globally to make CI pass.

allowed_files_or_area:
- .github/workflows/ci.yml
- rust-toolchain.toml
- Cargo.toml
- tests/live_index_integration.rs
- tests/coupling_calibration.rs
- README.md
- CLAUDE.md

forbidden_files:
- src/** except tiny cfg/test annotations required by clippy with justification
- npm/** except if CI npm command changes

contracts_or_interfaces:
- Regular CI remains deterministic and not excessively slow.
- Nightly/manual jobs can run heavier ignored performance/calibration tests.
- Release build is run before release claims.

invariants:
- No clippy/fmt gate may be added unless the repo is made to pass it or the failure is explicitly scoped to a follow-up blocker.
- No test should rely on absolute local paths in standard CI.

implementation_steps:
- Add cargo fmt --check and cargo clippy --all-targets -- -D warnings gates if they pass or fix narrow warnings.
- Add cargo build --release as CI or a manual/release workflow gate.
- Add a separate scheduled/manual perf job for ignored tests or lower the perf test to a bounded smoke that can run in CI.
- Document toolchain/edition rationale.

acceptance_criteria:
- CI config includes fmt, clippy, and release build in appropriate jobs.
- Ignored performance/calibration tests have a documented runnable CI/manual path.
- No standard CI job references local-only paths such as C:/AI_STUFF.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates`.
- The goal requires touching forbidden files.
- The goal expands into another goal's scope.
- A required interface, route, package, repository path, branch, runtime dependency, GitHub state, or source file contradicts this mini-spec.
- Verification cannot run for a reason that is not clearly pre-existing and documented.
- A security, credential, data-retention, privacy, production-safety, public API, backward-compatibility, or destructive-change question appears that is not answered by this goal file.
- The working tree contains unrelated dirty changes.

verification_command:

```bash
git diff --check
python execution/version_sync.py check
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo test --all-targets -- --test-threads=1
cd npm && npm test
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

Run SFR15 only on branch `goal/sfr15-add-ci-fmt-clippy-release-build-and-performance-smoke-gates`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Strengthen CI so formatting, clippy, release build, and selected performance/calibration evidence are checked in an appropriate cadence without making normal PR CI impractical.

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
