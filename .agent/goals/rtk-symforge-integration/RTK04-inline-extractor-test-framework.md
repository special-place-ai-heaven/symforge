---
goal_id: RTK04
title: Inline extractor test framework
phase: Wave B - foundation
status: "Queued"
depends_on: ["RTK01"]
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK04 - Inline extractor test framework

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK04-inline-extractor-test-framework.md

## Goal File Workflow

0. Use the Branch Guard below before editing this goal file, status fields, source code, docs, tests, or migrations. A wrong branch in the repository root means pivot to the dedicated worktree; a wrong branch inside that target worktree means stop before changing anything.
1. Use the Dependency Guard below. If a required predecessor goal is not completed, stop and report the missing dependency.
2. After Branch Guard and Dependency Guard pass, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
3. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`, and stop if a stop condition is hit.
4. When acceptance criteria pass, run the verification command, commit the verified goal work, then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
5. Commit the status update as well. If blocked, set `status` to `Blocked`, leave `completion_commit` empty, and record the blocker in the final report.

## Branch Guard

This goal belongs only to the `rtk-symforge-integration` branch, based from `main`.

Before changing this file's frontmatter, source code, docs, tests, migrations, or generated fixtures, run:

```bash
git branch --show-current
git status --short
```

The branch output must be exactly:

```text
rtk-symforge-integration
```

If this checkout is `main` or anything else, do not edit there. Pivot to an existing worktree for `rtk-symforge-integration`, or create one from `main` before continuing, for example:

```bash
git fetch origin main
git worktree add -b rtk-symforge-integration ../symforge-rtk-symforge-integration origin/main
cd ../symforge-rtk-symforge-integration
git branch --show-current
git status --short
```

If branch `rtk-symforge-integration` already exists but has no worktree, use:

```bash
git worktree add ../symforge-rtk-symforge-integration rtk-symforge-integration
cd ../symforge-rtk-symforge-integration
```

Stop only if the target worktree is unavailable, dirty with unrelated SymForge work, or still does not print `rtk-symforge-integration`. Do not move unrelated work into this branch and do not perform this goal on `main` unless the owner explicitly edits this goal file to say so.

## Dependency Guard

Required predecessor goals:
- `RTK01`

Before editing, confirm each predecessor goal file has `status: "Completed"` and a non-empty `completion_commit`:

```bash
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK01-automod-config-extractors.md
```

If any required predecessor is missing, blocked, still queued, or completed without a commit hash, stop and report the dependency blocker.

## Shared Project Constraints

forbidden_project_wide:
- Do not add SymForge runtime coupling to RTK. This sprint cherry-picks proven Rust idioms and patterns only.
- Do not reopen settled items: `panic = "abort"`, RTK build-time `.scm` embedding, `match_output`, shell lexer, telemetry HTTP ping, OpenClaw plugin, Homebrew packaging, or `RegexSet` replacement without benchmark evidence.
- Do not add forbidden dependencies: `lazy_static`, `ureq`, `flate2`, `quick-xml`, `which`, or `getrandom`.
- Do not log provider credentials, secrets, `.env` contents, private keys, raw query text, or unbounded source blobs.
- Do not silently convert missing, stale, degraded, disabled, blocked, or unknown data into success.
- Do not modify unrelated roadmap, release, frontend, npm, or vendored files unless this goal explicitly names them.

contracts_or_interfaces:
- SymForge remains a Rust MCP server. These goals must not make RTK a dependency or shared runtime component.
- Existing public tool surfaces and aliases must remain backward compatible unless an ADR explicitly authorizes a change.
- Persistent local state must be platform-correct and privacy-preserving, with opt-out or inert defaults where specified.
- Trust/config behavior must resolve at call time when the ADR or goal mentions ADR 0016; do not cache policy decisions at boot.
- Discovery-only tools must not create persistent frecency or analytics stores when their feature is disabled.
- Every goal remains one focused, reviewable implementation commit plus a separate goal-status commit unless the owner redirects.

invariants:
- If the current code and this goal disagree, inspect the code first, preserve the current implementation truth, and report the mismatch instead of guessing.
- A goal that finds the target already implemented must strengthen tests, status, or documentation instead of duplicating code.
- Any new status enum, CLI/MCP surface, environment variable, persistent schema, or feature flag must be named in the final report.
- Any broad `#[allow(...)]` is prohibited. A narrow `allow` must be local and include a one-line reason.

## Mini-Spec

objective:
- Add a co-located inline extractor test framework and first Rust/Python example tests that call the existing `parse_source` entry point.

non_goals:
- Do not create a parallel tree-sitter parser path.
- Do not add language autodetection inside the macro.
- Do not implement all remaining language tests in this goal.

allowed_files_or_area:
- src/parsing/inline_tests.rs
- src/parsing/mod.rs
- src/parsing/languages/rust.rs
- src/parsing/languages/python.rs
- wiki/todos/Todos — SymForge.md

forbidden_files:
- Do not modify parser behavior outside tests.
- Do not touch config extractor modules except if RTK01 revealed a small compatibility issue.

goal_specific_contracts_or_interfaces:
- Macro should be test-only with `#[cfg(test)]`.
- Macro must call existing `parse_source`.
- Caller must specify `LanguageId`; no detection logic.

implementation_steps:
- Inspect `parse_source` and current Rust/Python language extractor tests.
- Create `src/parsing/inline_tests.rs` with `inline_test!` macro.
- Register the test-only module from `src/parsing/mod.rs`.
- Add one Rust inline symbol extraction test.
- Add one Python inline symbol extraction test.
- Append a todo for the remaining 17 languages.

acceptance_criteria:
- `#[cfg(test)] inline_test!(name, language = LanguageId::..., source = "...", expected_symbols = [...])` compiles.
- The macro generates a `#[test]` function.
- The macro asserts extracted symbol names and kinds.
- One Rust inline test passes.
- One Python inline test passes.
- Remaining languages are recorded as a todo, not implemented here.

evidence_required:
- Macro signature.
- Example tests added.
- Todo entry path.
- Targeted parser test output summary.

stop_conditions:
- `parse_source` cannot be called from the intended module without changing production visibility.
- The macro requires behavior changes to parser code.
- The task expands into all language coverage.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --lib parsing::languages::rust -- --test-threads=1
cargo test --lib parsing::languages::python -- --test-threads=1
cargo test --lib parsing -- --test-threads=1
```

## Task Prompt

Run RTK04 only on branch `rtk-symforge-integration`. Install the inline extractor test pattern with two examples only, preserve the existing parser entry point, and leave the full language rollout as a documented follow-up.

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
- <source-status notes, test output summaries, schema/route/status evidence, screenshots only if this goal changes rendered UI>
Commit:
- Verified work commit: `<hash>`
Known gaps / blockers:
- <none or explicit blocker>
Next goal:
- RTK05 - ADR 0015 for RTK `.symforge` trust gate (RTK05-adr-0015-rtk-symforge-trust-gate.md)
