---
goal_id: SFB28
title: Implement first non-code repository intelligence family through existing surfaces
chain_id: symforge-live-code-backlog
phase: Phase 5 - non-code repository intelligence
status: "Completed"
depends_on: ["SFB27"]
target_branch: "backlog-implementation"
prohibited_branches: ["main", "master"]
worktree_hint: ".worktrees/backlog-implementation"
created_at: "2026-05-19"
started_at: "2026-05-22T00:15:56.5675647+02:00"
completed_at: "2026-05-22T00:37:21.5917799+02:00"
completion_commit: "ee418ebb0358555a32d7680d2779f6584859c80f"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "docs/live-code-backlog.md#20"
---
# SFB28 - Implement first non-code repository intelligence family through existing surfaces

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-live-code-backlog/SFB28-implement-first-non-code-repository-intelligence-family-through-existing-surfaces.md
```

## Goal File Workflow

0. Treat this markdown file as the whole prompt. Do not ask the user for extra instructions. If the task cannot be completed safely, mark it `Blocked` and explain exactly why in the final report.
1. Run the Branch Guard before editing this file, source code, tests, npm files, docs, generated artifacts, or Cargo metadata.
2. After Branch Guard passes, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
3. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`. Do not expand into adjacent backlog items unless this file explicitly says so.
4. If a stop condition is hit, stop implementation, set `status` to `Blocked`, set `blocked_reason`, leave `completion_commit` empty, and commit the status update if committing is safe.
5. When acceptance criteria pass, run the verification command exactly as written unless the command is impossible for a documented pre-existing reason.
6. Commit the verified implementation work first. Then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
7. Commit the goal-status update as a separate commit.

## Branch Guard

This goal belongs only to branch `backlog-implementation`.

Before making any change, run:

```bash
git branch --show-current
git status --short
```

If the branch is `backlog-implementation`, continue only if the working tree is clean or contains only this goal's already-started changes.

If the branch is `main`, `master`, or any other branch, do not edit there. Use or create the dedicated worktree:

```bash
if [ -d ".worktrees/backlog-implementation/.git" ] || [ -f ".worktrees/backlog-implementation/.git" ]; then
  cd .worktrees/backlog-implementation
else
  git fetch origin
  git worktree add -b backlog-implementation .worktrees/backlog-implementation origin/main || git worktree add .worktrees/backlog-implementation backlog-implementation
  cd .worktrees/backlog-implementation
fi
mkdir -p .agent/goals/symforge-live-code-backlog
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-live-code-backlog/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `backlog-implementation`.

## SymForge Goal Discipline

- Work from current code, not historical plans. Do not revive deleted historical docs, ADRs, RTK plans, old reports, or planning directories.
- Do not invent unrelated product features.
- Prefer small, reviewable Rust changes with focused tests.
- Preserve existing MCP behavior, public output contracts, npm packaging, CLI flags, tool names, schemas, and daemon behavior unless this goal explicitly changes one.
- Keep SymForge local-first: in-process `LiveIndex` and `.symforge/` local state remain the source of runtime truth.
- Maintain byte-exact source handling. Do not normalize line endings, rewrite source bytes casually, or serve stale spans silently.
- Never turn mock, stale, degraded, disabled, blocked, unavailable, or unknown state into success.
- If the target is already implemented, strengthen tests/evidence instead of duplicating code.
- If a public contract changes, add tests that pin the contract and note whether npm/client setup is affected.

## Dependency Guard

If `depends_on` is not empty, inspect the referenced goal file(s) under `.agent/goals/symforge-live-code-backlog/` when present. If a dependency is absent or not marked `Completed`, continue only if the code already contains the dependency's acceptance artifacts. Otherwise mark this goal `Blocked` with evidence.


## Mini-Spec

objective:
- Implement the first chosen non-code repository intelligence family from SFB27 so those files become searchable, resolvable, and explainable through existing SymForge surfaces without raw shell fallback for normal inspection.

non_goals:
- Do not implement both SQL and CI/YAML in one goal.
- Do not add broad parser rewrite.
- Do not invent a new public MCP tool unless SFB27 explicitly chose that route.
- Do not add edit behavior without dry-run/recovery evidence.

allowed_files_or_area:
- src/parsing/**
- src/live_index/**
- src/protocol/format.rs
- src/protocol/tools.rs
- src/protocol/search_format.rs
- tests/**
- Cargo.toml
- Cargo.lock only if the chosen parser dependency is justified by SFB27

forbidden_files:
- src/protocol/edit.rs except if SFB27 explicitly allowed dry-run edit behavior
- src/daemon.rs
- src/sidecar/**
- npm/**
- docs/**
- plans/**
- .planning/**
- openspec/**

contracts_or_interfaces:
- Chosen family uses existing search/outline/explain surfaces where possible.
- Normal, malformed, large, and empty/edge-case fixtures are tested.
- Malformed inputs degrade safely and explicitly.
- If edit behavior is added, dry-run/recovery evidence matches source edits.

invariants:
- Source-code indexing behavior remains unchanged.
- No raw shell fallback is needed for normal inspection.
- Large files remain bounded by existing budget/truncation policy.

acceptance_criteria:
- Chosen family files are searchable.
- Chosen family facts are resolvable/explainable through existing surfaces.
- Tests cover normal, malformed, large, and empty/edge-case files.
- No new public tool is added unless SFB27 explicitly required it.

evidence_required:
- SFB27 decision reference.
- Fixture list and test output.
- Sample search/outline/explain output.
- Default verification output.

stop_conditions:
- SFB27 decision is absent or ambiguous.
- Parser dependency or public tool decision exceeds this goal scope; block and create a foundation goal.
- Implementation requires changing edit semantics without dry-run/recovery evidence; block.

verification_command:

```bash
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
rg "sql|yaml|migration|config" src/parsing src/live_index src/protocol tests Cargo.toml
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


reviewer_checklist:
- Gate type is `implementation-ready` and was handled honestly.
- Branch evidence shows `backlog-implementation`.
- Changes stayed inside allowed files/areas.
- Forbidden historical docs/plans were not revived.
- Public MCP, CLI, npm, daemon, and output contracts did not regress unless this goal explicitly changed and tested them.
- Verification output is included in the final report.

## Task Prompt

Run only this goal. Follow the Branch Guard, update this file before and after work, keep edits inside the allowed files/areas, satisfy the mini-spec, run verification, commit verified work, then commit the status update. Report blockers instead of guessing.

## Final Report Format

Objective:
- <repeat this goal's objective>
Gate:
- <implementation-ready | evidence-gated | decision-gated>
Changes:
- <focused list of implementation changes>
Files changed:
- <paths>
Acceptance criteria:
- PASS/FAIL: <criterion> — <evidence>
Verification:
- PASS/FAIL: `<command>` — <summary>
Evidence:
- <branch evidence, test output summaries, rg output, before/after notes, status/output examples>
Commit:
- Verified work commit: `<hash>`
Known gaps / blockers:
- <none or explicit blocker with reason>
Next goal:
- none; pick the next uncompleted SFB goal by dependency order
