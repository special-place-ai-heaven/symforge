---
goal_id: SFR14
title: Complete result_status coverage for public read, search, and edit outcomes
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - public contract semantics
status: "Completed"
depends_on: ["SFR05"]
target_branch: "goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes"
created_at: "2026-05-22"
started_at: "2026-05-22T23:34:07.8095072+02:00"
completed_at: "2026-05-22T23:50:10.9402597+02:00"
completion_commit: "56b1287"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "src/protocol/result_status.rs defines found, not_found, ambiguous, invalid_request, empty_result, and internal_failure outcomes."
  - "src/protocol/tools.rs already has helper functions statused_tool_result and statused_edit_tool_result."
  - "docs/live-code-backlog.md asks for stable machine-level outcomes and a replayable public contract conformance suite."
---

# SFR14 - Complete result_status coverage for public read, search, and edit outcomes

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes.md
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
goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes/.git" ] || [ -f ".worktrees/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes/.git" ]; then
  cd .worktrees/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes .worktrees/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes origin/main
  cd .worktrees/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes`.

## Dependency Guard

Depends on: `SFR05`.

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
- Findings covered: SF-030, SF-058, docs/live-code-backlog.md#9, docs/live-code-backlog.md#11
- Current known state: src/protocol/result_status.rs defines found, not_found, ambiguous, invalid_request, empty_result, and internal_failure outcomes.
- Desired end state: Audit and close remaining gaps where public tool responses lack stable result_status metadata, prioritizing read/search/edit/dry-run outcomes and conformance corpus coverage.

## Code Evidence

- src/protocol/result_status.rs defines found, not_found, ambiguous, invalid_request, empty_result, and internal_failure outcomes.
- src/protocol/tools.rs already has helper functions statused_tool_result and statused_edit_tool_result.
- docs/live-code-backlog.md asks for stable machine-level outcomes and a replayable public contract conformance suite.

## Mini-Spec

objective:
- Audit and close remaining gaps where public tool responses lack stable result_status metadata, prioritizing read/search/edit/dry-run outcomes and conformance corpus coverage.

non_goals:
- Do not rewrite human-readable output unless needed to match status.
- Do not add new outcome classes without updating the contract version plan.
- Do not apply status metadata to internal helper functions that are not MCP tools.

allowed_files_or_area:
- src/protocol/tools.rs
- src/protocol/result_status.rs
- tests/conformance.rs
- tests/schema_roundtrip.rs
- tests/**result_status*

forbidden_files:
- src/live_index/** except tests fixtures
- src/daemon.rs except alias passthrough status tests
- npm/**

contracts_or_interfaces:
- Existing human text remains understandable.
- Machine status truth must not contradict visible text.
- Any contract version change must be explicit.

invariants:
- Not found is not an MCP transport error.
- Ambiguous selectors are classified ambiguous, not found or success.

implementation_steps:
- Inventory public tools and classify whether each response path already emits result_status.
- Patch priority gaps for get_symbol, get_file_content, search_*, find_references, replace_symbol_body, batch_edit, and batch_insert.
- Extend conformance corpus with found/not_found/ambiguous/invalid_request/empty/dry-run cases.

acceptance_criteria:
- Conformance tests cover all six OutcomeClass values.
- At least the prioritized tools have status metadata on public success/error-like paths.
- No human-visible contract regression occurs without an explicit test update.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes`.
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
cargo test --test conformance
cargo test result_status -- --test-threads=1
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

Run SFR14 only on branch `goal/sfr14-complete-result-status-coverage-for-public-read-search-and-edit-outcomes`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Audit and close remaining gaps where public tool responses lack stable result_status metadata, prioritizing read/search/edit/dry-run outcomes and conformance corpus coverage.

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
