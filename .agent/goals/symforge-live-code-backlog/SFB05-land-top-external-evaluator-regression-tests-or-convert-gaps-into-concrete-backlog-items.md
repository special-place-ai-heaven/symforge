---
goal_id: SFB05
title: Land top external-evaluator regression tests or convert gaps into concrete backlog items
chain_id: symforge-live-code-backlog
phase: Phase 1 - test hardening and diagnostics
status: "Completed"
depends_on: []
target_branch: "backlog-implementation"
prohibited_branches: ["main", "master"]
worktree_hint: ".worktrees/backlog-implementation"
created_at: "2026-05-19"
started_at: "2026-05-20T10:50:59.3843216+02:00"
completed_at: "2026-05-20T11:18:49.2345851+02:00"
completion_commit: "5ac3e3959db88ef837ac9b6bde3178c42303eaaf"
blocked_reason: ""
gate: "evidence-gated"
risk_level: "medium"
source_refs:
  - "docs/live-code-backlog.md#5"
---
# SFB05 - Land top external-evaluator regression tests or convert gaps into concrete backlog items

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-live-code-backlog/SFB05-land-top-external-evaluator-regression-tests-or-convert-gaps-into-concrete-backlog-items.md
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
- Audit already-fixed external-evaluator bugs, map each to the test category that should have caught it, and land at least three focused regression tests or convert untestable gaps into concrete implementation backlog items.

non_goals:
- Do not revive historical evaluation reports or planning directories.
- Do not add doc-only retrospectives.
- Do not fix unrelated bugs found during the audit unless a new regression test requires a tiny code fix.

allowed_files_or_area:
- tests/**
- src/** only for tiny fixes required by landed regression tests
- docs/live-code-backlog.md only if converting a gap into a concrete implementation item

forbidden_files:
- plans/**
- .planning/**
- openspec/**
- old reports
- historical ADRs
- npm/** unless a regression is specifically npm-related

contracts_or_interfaces:
- Each selected bug maps to one test category: parser, search, edit, git/ranking, daemon/session, protocol contract, npm/client setup, or sidecar.
- At least three new regression tests must land unless each top gap is represented as a concrete backlog item with file targets and verification.

invariants:
- No broad audit-only commit with no executable evidence.
- No historical docs are restored.
- New tests are deterministic and do not require network access.

acceptance_criteria:
- A short decision note in the final report lists audited bugs and selected top gaps.
- At least three regression tests are added and passing, OR concrete backlog entries are added for every top gap not implemented.
- No deleted/historical planning directories are restored.

evidence_required:
- List of audited bug/gap candidates.
- Names of added tests or exact backlog entries added.
- `git diff --stat`.
- Default verification output.

stop_conditions:
- Available evidence is insufficient to identify three top gaps without historical docs; stop and mark blocked rather than inventing.
- A selected regression requires broad feature work; convert it to a concrete backlog item instead.

verification_command:

```bash
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
git diff --name-only HEAD~1..HEAD
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
- Gate type is `evidence-gated` and was handled honestly.
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
- SFB06 - Make current partial-parse hygiene distinguish expected vendor noise from unexpected repo partials
