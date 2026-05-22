---
goal_id: SFR11
title: Quarantine corrupt index snapshots instead of dropping them
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - recovery artifacts
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them"
created_at: "2026-05-22"
started_at: "2026-05-22T22:08:13+02:00"
completed_at: "2026-05-22T22:25:39+02:00"
completion_commit: "0c05e43"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "src/live_index/persist.rs load_snapshot returns None on corrupt/truncated bytes after logging a warning."
  - "AGENTS.md says corruption should be quarantined, not silently served, and .symforge state should include quarantine artifacts."
  - "src/live_index/persist.rs already writes snapshots atomically via tmp to final rename."
---

# SFR11 - Quarantine corrupt index snapshots instead of dropping them

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR11-quarantine-corrupt-index-snapshots-instead-of-dropping-them.md
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
goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them/.git" ] || [ -f ".worktrees/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them/.git" ]; then
  cd .worktrees/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them .worktrees/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them origin/main
  cd .worktrees/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them`.

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
- Findings covered: SF-015, SF-020
- Current known state: src/live_index/persist.rs load_snapshot returns None on corrupt/truncated bytes after logging a warning.
- Desired end state: Move corrupt or version-incompatible snapshot files into a deterministic `.symforge/quarantine/` location with metadata instead of only returning None and losing evidence.

## Code Evidence

- src/live_index/persist.rs load_snapshot returns None on corrupt/truncated bytes after logging a warning.
- AGENTS.md says corruption should be quarantined, not silently served, and .symforge state should include quarantine artifacts.
- src/live_index/persist.rs already writes snapshots atomically via tmp to final rename.

## Mini-Spec

objective:
- Move corrupt or version-incompatible snapshot files into a deterministic `.symforge/quarantine/` location with metadata instead of only returning None and losing evidence.

non_goals:
- Do not change snapshot serialization format unless needed for metadata.
- Do not build full repair_index; SFR10 owns repair surface.
- Do not quarantine first-run missing snapshots.

allowed_files_or_area:
- src/live_index/persist.rs
- src/paths.rs
- tests/**persist*
- tests/**snapshot*
- AGENTS.md or README.md for one operator note

forbidden_files:
- src/protocol/tools.rs except if a test helper must expose state
- src/daemon.rs
- npm/**

contracts_or_interfaces:
- Corrupt `index.bin` is moved or copied to quarantine with a timestamp/content hash and reason.
- Valid snapshots still load exactly as before.
- Missing snapshot remains a normal first-run case.

invariants:
- Quarantine must not normalize or rewrite corrupt bytes.
- Quarantine failure must be logged and fail safely without serving corrupt data.

implementation_steps:
- Add quarantine path helpers under `.symforge/quarantine/index-snapshots/`.
- On postcard deserialize error and version mismatch, preserve the bad file and metadata before returning None.
- Add tests for corrupt bytes, version mismatch, missing file, and valid snapshot load.

acceptance_criteria:
- Corrupt snapshot test proves original bytes are preserved in quarantine.
- Version mismatch behavior is either quarantined or explicitly classified and tested.
- Valid snapshot and missing snapshot tests still pass.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them`.
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
cargo test snapshot quarantine -- --test-threads=1
cargo test persist -- --test-threads=1
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

Run SFR11 only on branch `goal/sfr11-quarantine-corrupt-index-snapshots-instead-of-dropping-them`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Move corrupt or version-incompatible snapshot files into a deterministic `.symforge/quarantine/` location with metadata instead of only returning None and losing evidence.

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
