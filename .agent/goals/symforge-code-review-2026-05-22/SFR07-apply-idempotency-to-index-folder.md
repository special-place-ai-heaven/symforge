---
goal_id: SFR07
title: Apply idempotency to index_folder
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - idempotency and recovery
status: "Completed"
depends_on: ["SFR06"]
target_branch: "goal/sfr07-apply-idempotency-to-index-folder"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr07-apply-idempotency-to-index-folder"
created_at: "2026-05-22"
started_at: "2026-05-22T20:08:55.0396442+02:00"
completed_at: "2026-05-22T20:35:04.0764372+02:00"
completion_commit: "cb8dac218b70f0ca4cce327b2a13b920c6251598"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "src/daemon.rs call_tool_handler has a special non-abortable path for index_folder."
  - "AGENTS.md names index_folder/index_repository as likely idempotent tools."
  - "The review recommends canonical hash + replay store per AGENTS rules for index/edit tools."
---

# SFR07 - Apply idempotency to index_folder

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR07-apply-idempotency-to-index-folder.md
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
goal/sfr07-apply-idempotency-to-index-folder
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr07-apply-idempotency-to-index-folder`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr07-apply-idempotency-to-index-folder/.git" ] || [ -f ".worktrees/sfr07-apply-idempotency-to-index-folder/.git" ]; then
  cd .worktrees/sfr07-apply-idempotency-to-index-folder
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr07-apply-idempotency-to-index-folder .worktrees/sfr07-apply-idempotency-to-index-folder origin/main
  cd .worktrees/sfr07-apply-idempotency-to-index-folder
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr07-apply-idempotency-to-index-folder`.

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
- Findings covered: SF-002, SF-003, SF-006
- Current known state: src/daemon.rs call_tool_handler has a special non-abortable path for index_folder.
- Desired end state: Add `idempotency_key` support to `index_folder` in local and daemon-backed execution paths, using the SFR06 substrate to prevent duplicate reindex runs and deterministic replay conflicts.

## Code Evidence

- src/daemon.rs call_tool_handler has a special non-abortable path for index_folder.
- AGENTS.md names index_folder/index_repository as likely idempotent tools.
- The review recommends canonical hash + replay store per AGENTS rules for index/edit tools.

## Mini-Spec

objective:
- Add `idempotency_key` support to `index_folder` in local and daemon-backed execution paths, using the SFR06 substrate to prevent duplicate reindex runs and deterministic replay conflicts.

non_goals:
- Do not add index_repository as a new alias unless SFR10 explicitly decides it.
- Do not implement full repair_index in this goal.
- Do not change the meaning of SYMFORGE_INDEX_FOLDER_RESET except to include it in the request hash.

allowed_files_or_area:
- src/protocol/tools.rs
- src/daemon.rs
- src/idempotency.rs
- tests/**index_folder*
- tests/conformance.rs
- tests/schema_roundtrip.rs

forbidden_files:
- src/protocol/edit.rs
- npm/**
- docs/** except a small schema note if public params changed

contracts_or_interfaces:
- `idempotency_key` is optional for backward compatibility.
- Same key + same index_folder request replays the stored result.
- Same key + different request fails deterministically with a clear message/status.

invariants:
- A failed or interrupted index_folder run must not be stored as completed success.
- Daemon and local paths must use the same request hash canonicalization.

implementation_steps:
- Add `idempotency_key: Option<String>` to IndexFolderInput with serde default and schema coverage.
- Wrap local `index_folder` execution with idempotency lookup/store.
- Wrap daemon special-case `index_folder` path with the same substrate and tests.

acceptance_criteria:
- Schema roundtrip accepts old payloads without idempotency_key.
- Regression tests prove same-key replay and conflict behavior for index_folder.
- Daemon special-case path is covered by a test or documented equivalent.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr07-apply-idempotency-to-index-folder`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr07-apply-idempotency-to-index-folder`.
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
cargo test index_folder idempotency -- --test-threads=1
cargo test --test schema_roundtrip
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

Run SFR07 only on branch `goal/sfr07-apply-idempotency-to-index-folder`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Add `idempotency_key` support to `index_folder` in local and daemon-backed execution paths, using the SFR06 substrate to prevent duplicate reindex runs and deterministic replay conflicts.

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
