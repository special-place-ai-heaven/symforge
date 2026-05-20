---
goal_id: SFB02
title: Add actionable untracked-file diagnostics to empty search results
chain_id: symforge-live-code-backlog
phase: Phase 1 - test hardening and diagnostics
status: "Completed"
depends_on: []
target_branch: "backlog-implementation"
prohibited_branches: ["main", "master"]
worktree_hint: ".worktrees/backlog-implementation"
created_at: "2026-05-19"
started_at: "2026-05-20T09:03:16.3605842+02:00"
completed_at: "2026-05-20T09:30:15.0748559+02:00"
completion_commit: "5594c785172d5582bc372b60c5bc6b524e6edd03"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "low"
source_refs:
  - "docs/live-code-backlog.md#2"
---
# SFB02 - Add actionable untracked-file diagnostics to empty search results

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-live-code-backlog/SFB02-add-actionable-untracked-file-diagnostics-to-empty-search-results.md
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
- When `search_files` or `search_text` returns zero hits but a matching untracked file exists, append a diagnostic that points users to `analyze_file_impact(path, new_file=true)` without auto-indexing.

non_goals:
- Do not auto-index untracked files by default.
- Do not change tracked-file search ranking or result shape except the zero-hit diagnostic suffix.
- Do not modify edit tools.

allowed_files_or_area:
- src/protocol/tools.rs
- src/protocol/format.rs
- src/protocol/search_format.rs
- src/live_index/search.rs
- src/live_index/query.rs
- src/git/**
- tests/**

forbidden_files:
- src/protocol/edit.rs except if a shared formatter import is unavoidable
- src/daemon.rs
- src/sidecar/**
- npm/**
- docs/**
- plans/**
- .planning/**
- openspec/**

contracts_or_interfaces:
- Diagnostic text must mention the untracked path count and the exact recovery call `analyze_file_impact("<path>", new_file=true)`.
- Diagnostic appears only when zero indexed hits and at least one matching untracked file are present.
- Tracked-file behavior remains unchanged.

invariants:
- No untracked file is indexed merely because search noticed it.
- Diagnostic must be deterministic for multiple matching untracked files.
- Existing search output remains human-readable.

acceptance_criteria:
- Regression test creates a tracked repo plus a matching untracked file and proves `search_files` emits the diagnostic on zero hits.
- Regression test proves `search_text` emits the diagnostic on zero hits.
- Existing tracked-file search tests still pass unchanged.

evidence_required:
- Test names and output for both search tools.
- Before/after sample output showing the diagnostic.
- `git diff --stat`.
- Default verification output.

stop_conditions:
- Untracked-file discovery requires scanning ignored or huge directories; stop and define a bounded matching rule first.
- Existing git helper cannot distinguish untracked from ignored files; stop and add a separate git-status helper task.
- The diagnostic would appear for non-empty indexed results; fix before closing.

verification_command:

```bash
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
rg "analyze_file_impact\(.*new_file=true|untracked file may match" src tests
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
- SFB03 - Surface sidecar PID and alive state in health output
