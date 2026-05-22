---
goal_id: SFR01
title: Align AGENTS, CLAUDE, README, and backlog with the shipped v7 surface
chain_id: symforge-code-review-2026-05-22
phase: Wave 1 - low-risk contract clarity
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface"
created_at: "2026-05-22"
started_at: "2026-05-22T18:17:47.3516717+02:00"
completed_at: "2026-05-22T18:24:54.8914721+02:00"
completion_commit: "2b844a5"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "low"
source_refs:
  - "AGENTS.md lists v1/future foundation tools such as index_repository, repair_index, checkpoint_now, get_repo_outline, and invalidate_cache as likely tools."
  - "src/cli/init.rs has canonical v7 client allowlists centered on index_folder, get_repo_map, get_file_context, and concrete edit/search tools."
  - "CLAUDE.md says 31 tools while tests/conformance.rs expects 32 including health_compact."
  - "docs/live-code-backlog.md still contains items the review says are already implemented, including untracked-file search diagnostics and sidecar PID health output."
---

# SFR01 - Align AGENTS, CLAUDE, README, and backlog with the shipped v7 surface

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface.md
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
goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface/.git" ] || [ -f ".worktrees/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface/.git" ]; then
  cd .worktrees/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface .worktrees/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface origin/main
  cd .worktrees/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface`.

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
- Findings covered: SF-001, SF-009, SF-010, SF-011, SF-014, SF-027, SF-028, SF-031, SF-036, SF-039, SF-043, SF-044, SF-047, SF-048, SF-050
- Current known state: AGENTS.md lists v1/future foundation tools such as index_repository, repair_index, checkpoint_now, get_repo_outline, and invalidate_cache as likely tools.
- Desired end state: Make the planning and operator-facing docs accurately describe the current SymForge v7.13.x tool/resource/prompt surface, shipped module layout, and completed backlog items without adding new product behavior.

## Code Evidence

- AGENTS.md lists v1/future foundation tools such as index_repository, repair_index, checkpoint_now, get_repo_outline, and invalidate_cache as likely tools.
- src/cli/init.rs has canonical v7 client allowlists centered on index_folder, get_repo_map, get_file_context, and concrete edit/search tools.
- CLAUDE.md says 31 tools while tests/conformance.rs expects 32 including health_compact.
- docs/live-code-backlog.md still contains items the review says are already implemented, including untracked-file search diagnostics and sidecar PID health output.

## Mini-Spec

objective:
- Make the planning and operator-facing docs accurately describe the current SymForge v7.13.x tool/resource/prompt surface, shipped module layout, and completed backlog items without adding new product behavior.

non_goals:
- Do not add, remove, or rename MCP tools.
- Do not edit source code except tests only if needed to prove a doc claim is stale.
- Do not create new architecture doctrine beyond what current code supports.

allowed_files_or_area:
- AGENTS.md
- CLAUDE.md
- README.md
- docs/live-code-backlog.md
- CHANGELOG.md only if a migration note is required

forbidden_files:
- src/**
- tests/** except optional doc-link grep tests if already present
- npm/**
- .github/**

contracts_or_interfaces:
- Docs must preserve the local-first architecture, byte-exact storage rules, and result_status terminology.
- Backlog must not contain already-completed items as implementation work.

invariants:
- Future/aspirational items must be labeled future/deferred, not described as shipped.
- No doc may claim 31 tools if conformance expects 32 including health_compact.

implementation_steps:
- Update the AGENTS MCP Surface and module guidance to distinguish shipped names from future/deferred names.
- Update CLAUDE.md tool count and verification guidance if it is stale.
- Update README feature summary and tool/resource/prompt references if they conflict with conformance or resources/prompts code.
- Refresh live-code-backlog by marking completed items closed or narrowing them to the remaining verified gap.

acceptance_criteria:
- Docs include a migration table mapping old/future names to current names or explicit deferred status.
- Backlog no longer asks agents to implement work that the register or code proves is already complete.
- README/CLAUDE/AGENTS agree on tool count and the presence of resources/prompts.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface`.
- The goal requires touching forbidden files.
- The goal expands into another goal's scope.
- A required interface, route, package, repository path, branch, runtime dependency, GitHub state, or source file contradicts this mini-spec.
- Verification cannot run for a reason that is not clearly pre-existing and documented.
- A security, credential, data-retention, privacy, production-safety, public API, backward-compatibility, or destructive-change question appears that is not answered by this goal file.
- The working tree contains unrelated dirty changes.

verification_command:

```bash
git diff --check
rg "31 tools|32 tools|health_compact|index_repository|repair_index|checkpoint_now|get_repo_outline|invalidate_cache" AGENTS.md CLAUDE.md README.md docs/live-code-backlog.md -n
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

Run SFR01 only on branch `goal/sfr01-align-agents-claude-readme-and-backlog-with-the-shipped-v7-surface`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Make the planning and operator-facing docs accurately describe the current SymForge v7.13.x tool/resource/prompt surface, shipped module layout, and completed backlog items without adding new product behavior.

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
