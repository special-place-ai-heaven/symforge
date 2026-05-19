---
goal_id: SRTK01
title: automod config extractors
phase: Wave A - low-risk hygiene
status: "Completed"
depends_on: []
target_branch: "symforge-rtk-surgical"
prohibited_branches: ["main"]
started_at: "2026-05-19T17:34:06.0248279+02:00"
completed_at: "2026-05-19T17:44:26.0352997+02:00"
completion_commit: "70206000c976d9a9d5723b825ed7da7927942c62"
---

# SRTK01 - automod config extractors

Use this file directly with `/goal`:

    /goal .agent/goals/symforge-rtk-surgical/SRTK01-automod-config-extractors.md

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

Replace the five manual config extractor module declarations with the same `automod::dir!()` pattern
already used by language extractors, without changing extractor APIs or behavior.

## Mini-Spec

objective:
- Replace the five manual config extractor module declarations with the same `automod::dir!()` pattern already used by language extractors, without changing extractor APIs or behavior.

non_goals:
- Do not change `extractor_for`, `is_config_language`, edit capability logic, parser behavior, or dependencies.
- Do not attempt config-extractor registry optimization in this goal.

allowed_files_or_area:
- src/parsing/config_extractors/mod.rs

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
- `Cargo.toml` already has `automod = "1"`.
- `src/parsing/languages/mod.rs` uses `automod::dir!("src/parsing/languages")`.
- `src/parsing/config_extractors/mod.rs` currently declares `env`, `json`, `markdown`, `toml_ext`, and `yaml` manually.

implementation_steps:
- Run preflight `cargo test --lib config_extractors -- --test-threads=1` if practical.
- Replace the five manual module declarations with `automod::dir!("src/parsing/config_extractors");`.
- Check the module remains usable by `src/parsing/mod.rs` and existing tests.
- Do not touch registry allocation or extractor internals.

acceptance_criteria:
- The five config extractor modules remain accessible.
- `extractor_for` has the same signature and behavior.
- `cargo test --lib config_extractors -- --test-threads=1` passes.
- `cargo check` passes.

risks_and_mitigation:
- Low risk; the exact pattern is already used for language modules.
- If `automod` exposes a visibility issue, stop and report the exact compiler error rather than expanding scope.

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
cargo test --lib config_extractors -- --test-threads=1
cargo check
```

## Task Prompt

Run SRTK01 only on branch `symforge-rtk-surgical`. Replace the five manual config extractor module declarations with the same `automod::dir!()` pattern already used by language extractors, without changing extractor APIs or behavior. Keep the sprint surgical, prove the current code surface before editing, avoid RTK bulk import, commit only verified work, and report blockers instead of guessing.

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
- SRTK02 - compression ratio regression test (SRTK02-compression-ratio-regression-test.md)
