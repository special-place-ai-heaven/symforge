---
goal_id: SFR02
title: Synchronize health_compact across init allowlists and conformance
chain_id: symforge-code-review-2026-05-22
phase: Wave 1 - MCP surface correctness
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance"
created_at: "2026-05-22"
started_at: "2026-05-22T18:29:05.0590720+02:00"
completed_at: "2026-05-22T18:37:19.2405677+02:00"
completion_commit: "7551932"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "low"
source_refs:
  - "tests/conformance.rs EXPECTED_TOOLS includes health_compact as the 32nd canonical tool."
  - "src/cli/init.rs SYMFORGE_TOOL_NAMES, KILO_ALWAYS_ALLOW, and CLAUDE_ALWAYS_ALLOW omit health_compact in the fetched main snapshot."
  - "CLAUDE.md currently describes 31 tools."
---

# SFR02 - Synchronize health_compact across init allowlists and conformance

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR02-synchronize-health-compact-across-init-allowlists-and-conformance.md
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
goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance/.git" ] || [ -f ".worktrees/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance/.git" ]; then
  cd .worktrees/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance .worktrees/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance origin/main
  cd .worktrees/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance`.

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
- Findings covered: SF-007, SF-009, SF-030
- Current known state: tests/conformance.rs EXPECTED_TOOLS includes health_compact as the 32nd canonical tool.
- Desired end state: Make `health_compact` consistently available wherever generated client allowlists and conformance define the public SymForge tool surface.

## Code Evidence

- tests/conformance.rs EXPECTED_TOOLS includes health_compact as the 32nd canonical tool.
- src/cli/init.rs SYMFORGE_TOOL_NAMES, KILO_ALWAYS_ALLOW, and CLAUDE_ALWAYS_ALLOW omit health_compact in the fetched main snapshot.
- CLAUDE.md currently describes 31 tools.

## Mini-Spec

objective:
- Make `health_compact` consistently available wherever generated client allowlists and conformance define the public SymForge tool surface.

non_goals:
- Do not change health_compact behavior.
- Do not add new tools.
- Do not change daemon alias behavior.

allowed_files_or_area:
- src/cli/init.rs
- tests/conformance.rs
- tests/schema_roundtrip.rs
- tests/capability_status_integration.rs
- CLAUDE.md only if SFR01 has not already fixed the count

forbidden_files:
- src/protocol/tools.rs except if tool definition test evidence proves health_compact registration is stale
- src/daemon.rs
- npm/**

contracts_or_interfaces:
- Conformance expected tools, generated init allowlists, and documented count must agree.
- Client init behavior must remain additive/idempotent.

invariants:
- No retired tool is re-added to allowlists.
- `trace_symbol` stays excluded from generated allowlists unless SFR19 explicitly changes compatibility policy.

implementation_steps:
- Add `mcp__symforge__health_compact` to `SYMFORGE_TOOL_NAMES` if missing.
- Add `health_compact` to Kilo and Claude always-allow lists if missing.
- Add or update tests that assert the init lists match the conformance tool surface including health_compact.

acceptance_criteria:
- A test fails if health_compact is in conformance but missing from generated client allowlists.
- No retired/alias-only tools are added to client allowlists.
- Existing init merge tests remain green.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance`.
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
cargo test --test schema_roundtrip
cargo test init -- --test-threads=1
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

Run SFR02 only on branch `goal/sfr02-synchronize-health-compact-across-init-allowlists-and-conformance`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Make `health_compact` consistently available wherever generated client allowlists and conformance define the public SymForge tool surface.

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
