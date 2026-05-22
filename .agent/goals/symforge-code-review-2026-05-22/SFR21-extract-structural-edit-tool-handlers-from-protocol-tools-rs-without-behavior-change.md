---
goal_id: SFR21
title: Extract structural edit tool handlers from protocol/tools.rs without behavior change
chain_id: symforge-code-review-2026-05-22
phase: Wave 4 - maintainability refactors
status: "Completed"
depends_on: ["SFR08", "SFR20"]
target_branch: "goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change"
created_at: "2026-05-22"
started_at: "2026-05-23T01:07:37.3191847+02:00"
completed_at: "2026-05-23T01:28:44.5770047+02:00"
completion_commit: "7ca22cac0df939f155f5732ea875470ba718eafc"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "The review recommends splitting tools.rs by read/search/edit/index/health domains."
  - "Existing edit safety and tee snapshot tests cover structural edit behavior and must remain the guardrail."
---

# SFR21 - Extract structural edit tool handlers from protocol/tools.rs without behavior change

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change.md
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
goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change/.git" ] || [ -f ".worktrees/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change/.git" ]; then
  cd .worktrees/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change .worktrees/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change origin/main
  cd .worktrees/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change`.

## Dependency Guard

Depends on: `SFR08, SFR20`.

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
- Findings covered: SF-012, SF-046, SF-047
- Current known state: The review recommends splitting tools.rs by read/search/edit/index/health domains.
- Desired end state: Move structural edit tool input structs and handlers into a focused protocol edit-tools module without changing edit behavior, dry-run semantics, tee snapshots, or public MCP schemas.

## Code Evidence

- The review recommends splitting tools.rs by read/search/edit/index/health domains.
- Existing edit safety and tee snapshot tests cover structural edit behavior and must remain the guardrail.

## Mini-Spec

objective:
- Move structural edit tool input structs and handlers into a focused protocol edit-tools module without changing edit behavior, dry-run semantics, tee snapshots, or public MCP schemas.

non_goals:
- Do not change edit semantics.
- Do not add idempotency here unless SFR08 did not land and this goal is explicitly updated.
- Do not refactor live_index query logic.

allowed_files_or_area:
- src/protocol/tools.rs
- src/protocol/edit_tools.rs
- src/protocol/edit.rs
- src/protocol/edit_format.rs
- src/protocol/mod.rs
- tests/edit_*.rs
- tests/batch_*.rs
- tests/schema_roundtrip.rs
- tests/conformance.rs

forbidden_files:
- src/live_index/query.rs
- src/daemon.rs except compile-only import path updates
- npm/**

contracts_or_interfaces:
- Edit schemas and outputs stay compatible.
- Dry-run and pre-edit tee snapshots remain intact.
- Result_status metadata from SFR14 is preserved.

invariants:
- No broad behavior change in a refactor goal.
- All moved functions retain tests.

implementation_steps:
- Extract edit-related structs/functions to a new module.
- Keep tool_router integration compiling with minimal visibility changes.
- Run edit, batch, schema, and conformance tests.

acceptance_criteria:
- tools.rs line count decreases; edit_tools module owns structural edits.
- No public edit schema/output regression.
- Edit safety and tee tests remain green.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change`.
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
cargo test edit batch tee -- --test-threads=1
cargo test --test schema_roundtrip
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

Run SFR21 only on branch `goal/sfr21-extract-structural-edit-tool-handlers-from-protocol-tools-rs-without-behavior-change`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Move structural edit tool input structs and handlers into a focused protocol edit-tools module without changing edit behavior, dry-run semantics, tee snapshots, or public MCP schemas.

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
