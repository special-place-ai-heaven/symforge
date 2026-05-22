---
goal_id: SFR19
title: Decide and implement trace_symbol alias lifecycle
chain_id: symforge-code-review-2026-05-22
phase: Wave 3 - API compatibility
status: "Completed"
depends_on: ["SFR01", "SFR05"]
target_branch: "goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr19-decide-and-implement-trace-symbol-alias-lifecycle"
created_at: "2026-05-22"
started_at: "2026-05-23T00:29:30.8487483+02:00"
completed_at: "2026-05-23T00:42:13.1321439+02:00"
completion_commit: "70214e7b3ca0dfc663dfe25467747574d29430d4"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "src/daemon.rs still routes trace_symbol to get_symbol_context with a deprecation banner."
  - "src/cli/init.rs tests/allowlists exclude retired trace_symbol from generated client guidance."
  - "docs/live-code-backlog.md item 14 asks whether to keep daemon compatibility one final release or remove it."
---

# SFR19 - Decide and implement trace_symbol alias lifecycle

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR19-decide-and-implement-trace-symbol-alias-lifecycle.md
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
goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr19-decide-and-implement-trace-symbol-alias-lifecycle/.git" ] || [ -f ".worktrees/sfr19-decide-and-implement-trace-symbol-alias-lifecycle/.git" ]; then
  cd .worktrees/sfr19-decide-and-implement-trace-symbol-alias-lifecycle
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle .worktrees/sfr19-decide-and-implement-trace-symbol-alias-lifecycle origin/main
  cd .worktrees/sfr19-decide-and-implement-trace-symbol-alias-lifecycle
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle`.

## Dependency Guard

Depends on: `SFR01, SFR05`.

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
- Findings covered: SF-008, SF-029, SF-041, docs/live-code-backlog.md#14
- Current known state: src/daemon.rs still routes trace_symbol to get_symbol_context with a deprecation banner.
- Desired end state: Make the retired `trace_symbol` alias lifecycle explicit: keep with tested deprecation until a named version boundary or remove it cleanly from daemon compatibility and docs.

## Code Evidence

- src/daemon.rs still routes trace_symbol to get_symbol_context with a deprecation banner.
- src/cli/init.rs tests/allowlists exclude retired trace_symbol from generated client guidance.
- docs/live-code-backlog.md item 14 asks whether to keep daemon compatibility one final release or remove it.

## Mini-Spec

objective:
- Make the retired `trace_symbol` alias lifecycle explicit: keep with tested deprecation until a named version boundary or remove it cleanly from daemon compatibility and docs.

non_goals:
- Do not re-add trace_symbol to generated client allowlists.
- Do not change get_symbol_context behavior except compatibility routing.
- Do not remove changed_with deprecation unless explicitly scoped.

allowed_files_or_area:
- src/daemon.rs
- src/cli/init.rs
- tests/daemon_aliases.rs
- tests/conformance.rs
- CHANGELOG.md
- README.md
- docs/live-code-backlog.md

forbidden_files:
- src/protocol/tools.rs except if deprecation text constants live there
- npm/**

contracts_or_interfaces:
- Generated client allowlists must not grant retired aliases by default.
- If daemon alias remains, response includes clear deprecation text and tests pin it.
- If alias is removed, tests and docs must reflect the removal.

invariants:
- No stale alias path may silently behave as a first-class current tool.
- Source search for trace_symbol must show only deliberate compatibility/history references.

implementation_steps:
- Use review register to choose keep-until-v8 or remove-now policy.
- Update daemon alias code/tests and docs accordingly.
- Ensure get_symbol_context is documented as the replacement.

acceptance_criteria:
- Policy is explicit in CHANGELOG or README.
- Tests prove generated init allowlists do not include trace_symbol.
- If alias remains, deprecation banner is tested; if removed, request fails in a tested way.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle`.
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
cargo test trace_symbol daemon_aliases init -- --test-threads=1
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

Run SFR19 only on branch `goal/sfr19-decide-and-implement-trace-symbol-alias-lifecycle`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Make the retired `trace_symbol` alias lifecycle explicit: keep with tested deprecation until a named version boundary or remove it cleanly from daemon compatibility and docs.

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
