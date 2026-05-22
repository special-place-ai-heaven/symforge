---
goal_id: SFR06
title: Introduce local idempotency replay substrate for mutating tools
chain_id: symforge-code-review-2026-05-22
phase: Wave 2 - idempotency and recovery
status: "Completed"
depends_on: ["SFR00"]
target_branch: "goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools"
landing_branch: "main"
prohibited_branches: ["main", "master", "backlog-implementation"]
worktree_hint: ".worktrees/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools"
created_at: "2026-05-22"
started_at: "2026-05-22T19:49:14.2404268+02:00"
completed_at: "2026-05-22T20:07:35.9169534+02:00"
completion_commit: "0dfd598c36501519772e23e08a5f4ca9e4e50d3f"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "AGENTS.md requires mutating tools to accept idempotency_key, store request_hash + status, replay same-key/same-hash, and fail same-key/different-hash."
  - "The review found no request hashing store and no idempotency_key on mutating MCP tools."
  - "Existing edit dry_run is not equivalent to idempotency; the review explicitly separates SF-046 from SF-002."
---

# SFR06 - Introduce local idempotency replay substrate for mutating tools

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-code-review-2026-05-22/SFR06-introduce-local-idempotency-replay-substrate-for-mutating-tools.md
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
goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools
```

Before making any change, run:

```bash
git fetch origin
git branch --show-current
git status --short
git rev-parse --short HEAD
git log --oneline -1
```

If the current branch is `main`, `master`, `backlog-implementation`, or any branch other than `goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools`, do not edit there. Use or create the dedicated worktree from updated `origin/main`:

```bash
if [ -d ".worktrees/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools/.git" ] || [ -f ".worktrees/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools/.git" ]; then
  cd .worktrees/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools
  git fetch origin
else
  git fetch origin
  git worktree add -b goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools .worktrees/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools origin/main
  cd .worktrees/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools
fi
mkdir -p .agent/goals/symforge-code-review-2026-05-22
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-code-review-2026-05-22/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools`.

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
- Findings covered: SF-002, SF-046
- Current known state: AGENTS.md requires mutating tools to accept idempotency_key, store request_hash + status, replay same-key/same-hash, and fail same-key/different-hash.
- Desired end state: Add a local-first idempotency substrate under `.symforge/` that can canonicalize mutating-tool requests, store replay records, and enforce same-key/same-hash semantics without wiring every tool yet.

## Code Evidence

- AGENTS.md requires mutating tools to accept idempotency_key, store request_hash + status, replay same-key/same-hash, and fail same-key/different-hash.
- The review found no request hashing store and no idempotency_key on mutating MCP tools.
- Existing edit dry_run is not equivalent to idempotency; the review explicitly separates SF-046 from SF-002.

## Mini-Spec

objective:
- Add a local-first idempotency substrate under `.symforge/` that can canonicalize mutating-tool requests, store replay records, and enforce same-key/same-hash semantics without wiring every tool yet.

non_goals:
- Do not apply idempotency to all tools in this goal.
- Do not change edit or index_folder public schemas except for internal tests around the substrate.
- Do not use an external database.

allowed_files_or_area:
- src/idempotency.rs
- src/lib.rs
- src/paths.rs
- src/hash.rs
- src/protocol/result_status.rs
- tests/idempotency*.rs
- AGENTS.md only to document the storage path if necessary

forbidden_files:
- src/protocol/tools.rs except compile-only import plumbing
- src/daemon.rs
- npm/**
- Cargo.toml unless a dependency is strictly necessary and justified

contracts_or_interfaces:
- Storage is local-first and rooted under `.symforge/` or SYMFORGE_HOME-equivalent project state.
- Canonical hash must be deterministic over semantically equivalent request JSON.
- Same key + different hash is a deterministic conflict, not a silent re-run.

invariants:
- No secrets or file contents are stored in idempotency records unless explicitly required and redacted.
- Replay records must not make a failed partial mutation look successful.

implementation_steps:
- Define IdempotencyKey, RequestHash, ReplayStatus, ReplayRecord, and a small file-backed store with atomic write semantics.
- Implement canonical JSON hashing using existing hash discipline where possible.
- Add unit tests for first execution, same-key/same-hash replay, same-key/different-hash conflict, and corrupt record handling.

acceptance_criteria:
- A reusable substrate exists and is tested without changing mutating tool behavior yet.
- Corrupt replay records fail safely or are quarantined; they are never served as success.
- The substrate API is narrow enough for SFR07 and SFR08 to consume.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools`.
- File paths changed.
- Verification command output summary.
- Any new public type, API, route, migration, feature flag, environment variable, event, status enum, npm behavior, or tool schema introduced.
- Explicit source-status notes for live, partial, degraded, disabled, test-only, mock, blocked, unknown, superseded, or deleted behavior touched.
- If a review finding is judged already fixed or false-positive, include concrete file paths and test names proving it.

stop_conditions:
- Current branch is not exactly `goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools`.
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
cargo test idempotency -- --test-threads=1
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

Run SFR06 only on branch `goal/sfr06-introduce-local-idempotency-replay-substrate-for-mutating-tools`. Complete the objective above, stay inside the allowed files/areas, respect all forbidden files and invariants, verify the work, commit only verified changes, update this goal file's status fields, and report blockers instead of guessing.

## Final Report Format

Objective:
- Add a local-first idempotency substrate under `.symforge/` that can canonicalize mutating-tool requests, store replay records, and enforce same-key/same-hash semantics without wiring every tool yet.

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
