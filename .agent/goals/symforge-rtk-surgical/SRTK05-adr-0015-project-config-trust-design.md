---
goal_id: SRTK05
title: ADR 0015 project config trust design
phase: Wave B - trust decision before code
status: "Pending"
depends_on: []
target_branch: "symforge-rtk-surgical"
prohibited_branches: ["main"]
started_at: ""
completed_at: ""
completion_commit: ""
---

# SRTK05 - ADR 0015 project config trust design

Use this file directly with `/goal`:

    /goal .agent/goals/symforge-rtk-surgical/SRTK05-adr-0015-project-config-trust-design.md

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

Write the architecture decision for SymForge `.symforge/` project-config trust gating before
implementing trust code, explicitly choosing the minimum SymForge-native control surface and
rejecting RTK hook bulk.

## Mini-Spec

objective:
- Write the architecture decision for SymForge `.symforge/` project-config trust gating before implementing trust code, explicitly choosing the minimum SymForge-native control surface and rejecting RTK hook bulk.

non_goals:
- Do not implement trust code in this goal.
- Do not add CLI or MCP trust commands before the ADR chooses the surface.
- Do not add integrity sidecars before the ADR decides whether `.symforge/` currently has behavior that needs them.

allowed_files_or_area:
- docs/decisions/0015-rtk-trust-gating-symforge-config.md
- docs/decisions/README.md only if the repo maintains an ADR index

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
- `src/edit_safety/mod.rs` currently exports only `tee`.
- `src/edit_safety/trust.rs` is absent.
- `src/hash.rs` already provides SHA-256 helpers.
- Prior “trust gate” language in the repo refers to search/context trust, not `.symforge/` config trust.

implementation_steps:
- Create ADR 0015 at the exact reserved path.
- Open with a disambiguation between search/context trust gates and `.symforge/` project-config trust.
- State that RTK is not a dependency and only patterns are borrowed.
- Define TrustStatus: Trusted, Untrusted, ContentChanged { expected, actual }, EnvOverride.
- Document fail-secure behavior for missing/corrupt trust store.
- Document TOCTOU-safe trust recording from a precomputed hash.
- Use canonical path keys with `std::fs::canonicalize` plus `dunce` normalization.
- Choose `dirs::data_local_dir()/symforge/trust.json` for store placement.
- Define CI-gated override semantics for `SYMFORGE_TRUST_PROJECT_CONFIG=1` only under known CI vars.
- Choose either CLI subcommands, MCP tools, or no user surface yet; justify the choice against SymForge’s current daemon/CLI shape.
- State default LOG_ONLY and opt-in ENFORCE only if/when implemented.
- Decide whether integrity sidecar is now, later, or rejected until executable `.symforge/` behavior exists.
- Reference ADR 0011, 0012, 0014, and 0016.

acceptance_criteria:
- ADR 0015 exists and is concrete enough for SRTK06/SRTK07.
- ADR does not claim RTK runtime integration.
- ADR settles control surface enough to unblock or block SRTK07.
- ADR explicitly defers or authorizes integrity sidecar scope.
- No source implementation is done.

risks_and_mitigation:
- Premature trust implementation can create the wrong surface; this is deliberately doc-first.
- If ADR index conventions are unclear, inspect existing ADR files and follow the local style.

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
cargo check
```

## Task Prompt

Run SRTK05 only on branch `symforge-rtk-surgical`. Write the architecture decision for SymForge `.symforge/` project-config trust gating before implementing trust code, explicitly choosing the minimum SymForge-native control surface and rejecting RTK hook bulk. Keep the sprint surgical, prove the current code surface before editing, avoid RTK bulk import, commit only verified work, and report blockers instead of guessing.

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
- SRTK06 - trust core pure module (SRTK06-trust-core-pure-module.md)
