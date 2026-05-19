---
goal_id: SRTK04
title: inline language test macro for Rust and Python
phase: Wave B - parser test foundation
status: "Pending"
depends_on: ["SRTK01"]
target_branch: "symforge-rtk-surgical"
prohibited_branches: ["main"]
started_at: ""
completed_at: ""
completion_commit: ""
---

# SRTK04 - inline language test macro for Rust and Python

Use this file directly with `/goal`:

    /goal .agent/goals/symforge-rtk-surgical/SRTK04-inline-language-test-macro-for-rust-and-python.md

## Goal File Workflow

0. Use the Branch Guard below before editing this goal file, status fields, source code, docs, tests, fixtures, or migrations.
1. After Branch Guard passes, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
2. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`, and stop if a stop condition is hit.
3. When acceptance criteria pass, run the verification command, commit the verified goal work, then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
4. Commit the status update as well. If blocked, set `status` to `Blocked`, leave `completion_commit` empty, and record the blocker in the final report.

## Branch Guard

This goal belongs only to the `symforge-rtk-surgical` branch.

Before changing anything, run:

```bash
git branch --show-current
git status --short
```

The branch output must be exactly:

```text
symforge-rtk-surgical
```

If this checkout is `main` or any other branch, do not edit there. Create or pivot to a dedicated worktree for `symforge-rtk-surgical`, then rerun this Branch Guard. Stop if the target worktree is unavailable, dirty with unrelated work, or still does not print `symforge-rtk-surgical`.

## Selection Rationale

Create a small co-located inline test macro for language extractors using the existing
`parse_source` entry point, then add one Rust and one Python example test.

## Mini-Spec

objective:
- Create a small co-located inline test macro for language extractors using the existing `parse_source` entry point, then add one Rust and one Python example test.

non_goals:
- Do not create a parallel tree-sitter parser path.
- Do not add language autodetection to the macro.
- Do not implement all remaining languages in this goal.

allowed_files_or_area:
- src/parsing/inline_tests.rs
- src/parsing/mod.rs
- src/parsing/languages/rust.rs
- src/parsing/languages/python.rs
- wiki/todos/** if the repo has the referenced todo file

forbidden_files:
- Do not vendor, copy, or import RTK as a SymForge runtime dependency.
- Do not import RTK shell hooks, hook installers, command rewriting, Claude permission parsing, CLI output filters, OpenClaw plugin code, Homebrew formula code, or HTTP telemetry.
- Do not add `lazy_static`, `ureq`, `flate2`, `quick-xml`, `which`, or `getrandom` for this task.
- Do not add `panic = "abort"` or change the release profile beyond the exact task scope.
- Do not convert SymForge from an MCP code-intelligence server into an RTK-style CLI-output filter.
- Do not change unrelated MCP tool names, aliases, or response contracts.
- Do not write raw prompts, secrets, `.env` contents, provider credentials, private keys, or unbounded source blobs to logs, analytics, memory, or persistent state.
- Do not silently report degraded, skipped, unavailable, disabled, stale, partial, or blocked behavior as success.

contracts_or_interfaces:
- SymForge stays a local-first MCP server for code intelligence. RTK is only a source of selectively borrowed Rust patterns.
- Tool handlers return helpful plain text `String` responses unless an existing SymForge contract explicitly says otherwise.
- Do not use MCP error codes for normal not-found or degraded cases; follow the existing helpful-text style.
- Never hold `RwLock` guards across await points; extract owned data, drop the lock, then format.
- Capability and policy decisions that are user/operator-controlled must resolve at call time unless this goal explicitly proves another model is safe.
- Discovery-only tools must not create new persistent DB files or durable side effects.
- New live behavior must be gated, observable, and reversible until a later goal explicitly enables default rollout.

invariants:
- Current SymForge code is the implementation truth. If planning docs disagree with code, document the gap and stop rather than pretending the plan is implemented.
- A goal that finds the target already implemented must strengthen tests, documentation, or status instead of duplicating code.
- Persistent state must be versioned or migration-safe, and missing/corrupt local state must degrade safely rather than crash the daemon.
- Evidence-gated tasks may close as `Completed` with a no-patch investigation note when measurement shows no worthwhile implementation.
- Each sprint should remain one focused code/doc commit plus the goal-status commit unless the owner redirects.

code_evidence_to_inspect_before_editing:
- `src/parsing/inline_tests.rs` is absent.
- `parse_source` constructs and uses the current parser flow; the macro must call it rather than duplicating parser setup.
- Language modules already contain language-specific extraction logic.

implementation_steps:
- Create `src/parsing/inline_tests.rs` under `#[cfg(test)]` with an `inline_test!` macro.
- Register the test-only module from `src/parsing/mod.rs`.
- Macro inputs must include test name, explicit `LanguageId`, source, and expected `(kind, name)` symbols.
- Add one Rust inline test in `languages/rust.rs`.
- Add one Python inline test in `languages/python.rs`.
- Add a short todo for remaining languages if the project todo path exists.

acceptance_criteria:
- Macro expands to a normal `#[test]` function.
- Macro calls existing `parse_source`.
- Rust and Python tests pass.
- Remaining languages are left as todo/follow-up, not silently skipped as complete.

risks_and_mitigation:
- Tree-sitter parser setup already exists; duplicating it would create drift.
- If visibility blocks access to `parse_source`, choose the smallest visibility/test-module adjustment and document it.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `symforge-rtk-surgical`.
- File paths changed.
- Verification command output summary.
- Any new public type, port, route, migration, event, feature flag, environment variable, or status enum introduced.
- Explicit source-status notes for live, partial, degraded, disabled, blocked, unknown, test-only, or mock behavior touched.
- A short note explaining why this is a selective SymForge enhancement rather than RTK bulk integration.

stop_conditions:
- Current branch is not exactly `symforge-rtk-surgical`.
- The task would require importing RTK runtime code or changing SymForge's product shape.
- The target code path is missing or contradicts this mini-spec.
- The goal would require touching forbidden files or expanding into a later goal's scope.
- Verification cannot run for a reason that is not clearly pre-existing and documented.

verification_command:
```bash
test "$(git branch --show-current)" = "symforge-rtk-surgical"
git diff --check
cargo test --lib parsing::languages::rust -- --test-threads=1
cargo test --lib parsing::languages::python -- --test-threads=1
cargo test --lib parsing -- --test-threads=1
```

## Task Prompt

Run SRTK04 only on branch `symforge-rtk-surgical`. Create a small co-located inline test macro for language extractors using the existing `parse_source` entry point, then add one Rust and one Python example test. Keep the sprint surgical, prove the current code surface before editing, avoid RTK bulk import, commit only verified work, and report blockers instead of guessing.

## Final Report Format

Objective:
- <repeat this goal's objective>
Changes:
- <focused list of implementation changes>
Files changed:
- <paths>
Verification:
- PASS/FAIL: `<command>` — <summary>
Evidence:
- <source-status notes, test output summaries, route/status evidence, and any benchmark/profiling output required by the goal>
Commit:
- Verified work commit: `<hash>`
Known gaps / blockers:
- <none or explicit blocker>
Next goal:
- SRTK05 - ADR 0015 project config trust design (SRTK05-adr-0015-project-config-trust-design.md)
