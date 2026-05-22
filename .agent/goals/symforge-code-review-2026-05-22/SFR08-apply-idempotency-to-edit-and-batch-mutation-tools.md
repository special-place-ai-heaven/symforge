---
goal_id: SFR08
title: Apply idempotency to edit and batch mutation tools
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - idempotency and recovery
status: "Completed"
depends_on: ["SFR06", "SFR07"]
target_branch: "goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools"
created_at: "2026-05-22"
started_at: "2026-05-22T20:37:55.3785961+02:00"
completed_at: "2026-05-22T21:27:07+02:00"
completion_commit: "c8254953a4d8acb817d4008228c2d4d8014fac37"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "AGENTS.md says mutating operations must support idempotency and describes replay rules."
  - "The review lists edits and batch_* tools as missing idempotency while noting dry_run and edit tee snapshots are separate existing safety layers."
  - "src/protocol/tools.rs contains structural edit tools such as replace_symbol_body, edit_within_symbol, insert_symbol, delete_symbol, batch_edit, batch_insert, and batch_rename."
---

# SFR08 - Apply idempotency to edit and batch mutation tools

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR08-apply-idempotency-to-edit-and-batch-mutation-tools.md
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
goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools/.git" ] || [ -f ".worktrees/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools/.git" ]; then
  cd .worktrees/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools .worktrees/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools origin/main
  cd .worktrees/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools`.

## Dependency Guard

Depends on: `SFR06, SFR07`.

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
- Findings covered: SF-002, SF-046, SF-047
- Current known state: AGENTS.md says mutating operations must support idempotency and describes replay rules.
- Desired end state: Add optional `idempotency_key` support to structural edit and batch mutation tools so retries cannot duplicate edits or silently replay incompatible requests.

## Code Evidence

- AGENTS.md says mutating operations must support idempotency and describes replay rules.
- The review lists edits and batch_* tools as missing idempotency while noting dry_run and edit tee snapshots are separate existing safety layers.
- src/protocol/tools.rs contains structural edit tools such as replace_symbol_body, edit_within_symbol, insert_symbol, delete_symbol, batch_edit, batch_insert, and batch_rename.

## Mini-Spec

objective:
- Add optional `idempotency_key` support to structural edit and batch mutation tools so retries cannot duplicate edits or silently replay incompatible requests.

non_goals:
- Do not change dry_run semantics.
- Do not remove pre-edit tee snapshots.
- Do not change edit operation formats beyond adding optional idempotency_key where appropriate.

allowed_files_or_area:
- src/protocol/tools.rs
- src/protocol/edit.rs
- src/protocol/edit_format.rs
- src/idempotency.rs
- tests/edit_*.rs
- tests/batch_*.rs
- tests/conformance.rs
- tests/schema_roundtrip.rs

forbidden_files:
- src/live_index/query.rs
- src/daemon.rs except if daemon forwarding needs param passthrough tests
- npm/**

contracts_or_interfaces:
- Optional idempotency_key is backward-compatible.
- Dry-run requests are either excluded from replay storage or stored distinctly from committed mutations.
- Batch operation replay preserves per-operation result_status metadata.

invariants:
- No request may be marked completed until the verified mutation path has completed.
- Same-key/different-hash must fail before touching files.

implementation_steps:
- Add optional idempotency_key to mutating input structs.
- Create a shared helper for mutation replay around existing edit execution paths.
- Add tests for single edit, batch edit, dry-run, and conflict semantics.

acceptance_criteria:
- Same-key/same-hash replay returns the original edit result without applying a second mutation.
- Same-key/different-hash returns deterministic conflict before file changes.
- Dry-run behavior remains unchanged and does not poison committed replay state.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools`.
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
cargo test edit idempotency -- --test-threads=1
cargo test batch idempotency -- --test-threads=1
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

Run SFR08 only on branch `goal/sfr08-apply-idempotency-to-edit-and-batch-mutation-tools`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Add optional `idempotency_key` support to structural edit and batch mutation tools so retries cannot duplicate edits or silently replay incompatible requests.

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
