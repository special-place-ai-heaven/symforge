---
goal_id: RTK05
title: ADR 0015 for RTK `.symforge` trust gate
phase: Wave B - foundation
status: "Queued"
depends_on: []
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK05 - ADR 0015 for RTK `.symforge` trust gate

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK05-adr-0015-rtk-symforge-trust-gate.md

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

This goal has no required predecessor goal. Still inspect the current repo state before editing, because previous manual work may already have implemented the target.

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
- Write ADR 0015 documenting RTK-style `.symforge/` trust gating, including state model, persistence, migration, call-time policy, and user audit/revoke surface, without implementing code.

non_goals:
- Do not implement trust code in this goal.
- Do not add CLI or MCP commands yet.
- Do not alter Wave 0/search-context trust code.

allowed_files_or_area:
- docs/decisions/0015-rtk-trust-gating-symforge-config.md
- docs/decisions/README.md if the repo maintains an ADR index

forbidden_files:
- src/**
- Cargo.toml
- tests/**

goal_specific_contracts_or_interfaces:
- ADR 0015 is intentionally reserved even though ADR 0016 exists.
- ADR must disambiguate RTK `.symforge/` trust gating from Wave 0/search-context trust gates.
- ADR must cite ADR 0011, 0012, 0014, and 0016.

implementation_steps:
- Inspect existing ADR style and index.
- Create `docs/decisions/0015-rtk-trust-gating-symforge-config.md`.
- Open with the naming-collision disambiguation paragraph.
- Document four-state `TrustStatus`: `Trusted`, `Untrusted`, `ContentChanged { expected, actual }`, `EnvOverride`.
- Document fail-secure corrupt/missing store behavior.
- Document TOCTOU-safe precomputed-hash recording.
- Document canonical path keying with `std::fs::canonicalize` plus existing `dunce` normalization and Windows test requirement.
- Document store path `dirs::data_local_dir()/symforge/trust.json`.
- Document CI-gated `SYMFORGE_TRUST_PROJECT_CONFIG=1` override and CI env list.
- Document `chrono = "0.4"` use for `trusted_at`.
- Document `symforge trust`, `symforge trust --list`, and `symforge untrust` or justified MCP equivalents.
- Document default LOG_ONLY and opt-in ENFORCE behavior.
- Update ADR index if present.

acceptance_criteria:
- ADR exists at the exact reserved path.
- ADR references ADR 0011, 0012, 0014, and 0016.
- ADR explicitly says no watcher is added for live `.symforge/` hashing.
- ADR documents corrupt-store and missing-store behavior.
- ADR documents how users audit and revoke trust.
- ADR chooses a clear status: Proposed, Accepted, or equivalent.

evidence_required:
- ADR status chosen and reason.
- Any index update.
- Implementation tasks that remain.
- Confirmation no code implementation was done.

stop_conditions:
- Existing ADR numbering convention contradicts using 0015.
- The trust gate would need a watcher or boot-time cached policy.
- The ADR would need to reopen a settled out-of-scope item.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo check
```

## Task Prompt

Run RTK05 only on branch `rtk-symforge-integration`. Create the ADR-only trust-gate decision record. Keep it implementation-free and explicit enough for the following trust-core goal to execute without redesign.

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
- RTK06 - Trust core module and tests (RTK06-trust-core-module-and-tests.md)
