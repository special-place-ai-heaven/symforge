---
goal_id: SFR05
title: Return Ambiguous status for get_symbol_context without disambiguating path
chain_id: symforge-code-review-2026-05-22
phase: Wave 1 - correctness
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path"
created_at: "2026-05-22"
started_at: "2026-05-22T19:25:31.8244477+02:00"
completed_at: "2026-05-22T19:47:40.5854069+02:00"
completion_commit: "0d95de72b21aa8afb555f71480a4c2c94f2c00fa"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "medium"
source_refs:
  - "src/protocol/tools.rs get_symbol_context auto-resolves multiple candidate paths to candidates[0] and only appends a note saying to specify path."
  - "src/protocol/result_status.rs already defines OutcomeClass::Ambiguous and ResultStatus metadata."
  - "tests/conformance.rs includes public contract machinery for outcome_class checks."
---

# SFR05 - Return Ambiguous status for get_symbol_context without disambiguating path

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path.md
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
goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path/.git" ] || [ -f ".worktrees/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path/.git" ]; then
  cd .worktrees/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path .worktrees/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path origin/main
  cd .worktrees/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path`.

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
- Findings covered: SF-005, SF-030, SF-058
- Current known state: src/protocol/tools.rs get_symbol_context auto-resolves multiple candidate paths to candidates[0] and only appends a note saying to specify path.
- Desired end state: Change `get_symbol_context` so ambiguous symbol selectors without `path`/`file` return an explicit ambiguous outcome instead of showing the first candidate as if it were authoritative.

## Code Evidence

- src/protocol/tools.rs get_symbol_context auto-resolves multiple candidate paths to candidates[0] and only appends a note saying to specify path.
- src/protocol/result_status.rs already defines OutcomeClass::Ambiguous and ResultStatus metadata.
- tests/conformance.rs includes public contract machinery for outcome_class checks.

## Mini-Spec

objective:
- Change `get_symbol_context` so ambiguous symbol selectors without `path`/`file` return an explicit ambiguous outcome instead of showing the first candidate as if it were authoritative.

non_goals:
- Do not change `get_symbol` or `find_references` unless tests prove their contracts depend on this behavior.
- Do not remove bundle/trace modes.
- Do not change symbol search ranking.

allowed_files_or_area:
- src/protocol/tools.rs
- src/protocol/format.rs
- src/protocol/result_status.rs
- tests/conformance.rs
- tests/**symbol*
- tests/**context*

forbidden_files:
- src/live_index/query.rs except if a tiny helper extraction is required
- src/daemon.rs except if daemon alias needs to preserve trace_symbol semantics
- npm/**

contracts_or_interfaces:
- Explicit `path` or `file` keeps existing found/not-found behavior.
- Ambiguous no-path request returns machine-readable `OutcomeClass::Ambiguous` where the MCP protocol supports metadata.
- Human text must list candidate paths and ask the caller to pass `path` or `file`.

invariants:
- No first-candidate fallback for ambiguous selectors in default context mode.
- Trace mode still requires path as it currently does.

implementation_steps:
- Create a small ambiguous-candidate formatter if needed.
- Update get_symbol_context default path resolution so candidates.len() > 1 stops before sidecar lookup and returns ambiguous text/status.
- Add conformance or integration cases with duplicate symbol names in different files.

acceptance_criteria:
- Duplicate symbol names without path produce an Ambiguous result_status and candidate list.
- The same request with `path` succeeds and returns the intended symbol context.
- No ambiguous request bumps frecency for an arbitrary first candidate.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path`.
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
cargo test --test conformance get_symbol_context
cargo test get_symbol_context -- --test-threads=1
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

Run SFR05 only on branch `goal/sfr05-return-ambiguous-status-for-get-symbol-context-without-disambiguating-path`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Change `get_symbol_context` so ambiguous symbol selectors without `path`/`file` return an explicit ambiguous outcome instead of showing the first candidate as if it were authoritative.

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
