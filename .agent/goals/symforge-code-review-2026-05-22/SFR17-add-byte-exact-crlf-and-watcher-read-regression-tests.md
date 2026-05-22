---
goal_id: SFR17
title: Add byte-exact CRLF and watcher read regression tests
chain_id: symforge-code-review-2026-05-22
phase: Wave 3 - storage correctness
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests"
created_at: "2026-05-22"
started_at: "2026-05-23T00:17:25.6931825+02:00"
completed_at: "2026-05-23T00:22:26.0462297+02:00"
completion_commit: "51ab5c7"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "low"
source_refs:
  - "src/live_index/persist.rs snapshot structs store `content: Vec<u8>` and content_hash, and write snapshots using postcard and atomic rename."
  - "The review notes watcher reads file bytes with fs::read and recommends a regression from the historical Python newline bug."
  - "AGENTS.md says write bytes exactly as read and never normalize line endings."
---

# SFR17 - Add byte-exact CRLF and watcher read regression tests

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR17-add-byte-exact-crlf-and-watcher-read-regression-tests.md
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
goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests/.git" ] || [ -f ".worktrees/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests/.git" ]; then
  cd .worktrees/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests .worktrees/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests origin/main
  cd .worktrees/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests`.

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
- Findings covered: SF-020, SF-033
- Current known state: src/live_index/persist.rs snapshot structs store `content: Vec<u8>` and content_hash, and write snapshots using postcard and atomic rename.
- Desired end state: Add focused regression tests proving indexing, watcher reads, and snapshot persistence preserve CRLF/source bytes exactly and do not normalize line endings.

## Code Evidence

- src/live_index/persist.rs snapshot structs store `content: Vec<u8>` and content_hash, and write snapshots using postcard and atomic rename.
- The review notes watcher reads file bytes with fs::read and recommends a regression from the historical Python newline bug.
- AGENTS.md says write bytes exactly as read and never normalize line endings.

## Mini-Spec

objective:
- Add focused regression tests proving indexing, watcher reads, and snapshot persistence preserve CRLF/source bytes exactly and do not normalize line endings.

non_goals:
- Do not change snapshot format unless tests reveal a bug.
- Do not change parser semantics.
- Do not add broad Windows-only logic unless needed.

allowed_files_or_area:
- src/live_index/persist.rs
- src/watcher/mod.rs
- tests/**persist*
- tests/**watcher*
- tests/**crlf*

forbidden_files:
- src/protocol/tools.rs
- src/daemon.rs
- npm/**

contracts_or_interfaces:
- CRLF bytes survive index load, snapshot serialize/load, and watcher update paths.
- Content hashes are computed on raw bytes.

invariants:
- Tests must compare bytes, not strings normalized by platform APIs.
- No line-ending normalization helper is introduced.

implementation_steps:
- Add a CRLF fixture created with explicit bytes.
- Test LiveIndex load and snapshot roundtrip preserve content bytes and hash.
- Test watcher or incremental update path reads raw bytes with no normalization.

acceptance_criteria:
- At least one test would fail if CRLF became LF during persistence.
- At least one test would fail if watcher path decoded/re-encoded content.
- Existing persist/watcher tests remain green.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests`.
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
cargo test crlf persist watcher -- --test-threads=1
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

Run SFR17 only on branch `goal/sfr17-add-byte-exact-crlf-and-watcher-read-regression-tests`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Add focused regression tests proving indexing, watcher reads, and snapshot persistence preserve CRLF/source bytes exactly and do not normalize line endings.

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
