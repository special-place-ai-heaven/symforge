# Prompt: Author /goal-Ready Tasks For Call-Time Capability Resolution

Use this document as the prompt for GPT-5.5 Pro. Its job is to create the task files that future agents can run with `/goal path/to/file.md`. It should not implement the feature itself.

## What Was Inspected

`/goal` is installed/enabled as a Codex goals feature.

Observed local evidence:

- `C:\Users\poslj\.codex\config.toml` has `[features] goals = true`.
- No separate local `/goal` `SKILL.md` file was found; this appears to be a built-in goal runner, not a normal skill directory.
- The expected prompt convention is the `/goal` mega-prompt shape:
  - one clear final outcome,
  - explicit context,
  - measurable success criteria,
  - constraints / operating rules,
  - a checklist or progress log,
  - final deliverables and proof.
- The local GSD PLAN schema at `.codex/get-shit-done/templates/phase-prompt.md` is still useful as a secondary reference for dependency/wave thinking, `must_haves`, file ownership, and verification discipline, but the generated files must be native `/goal` prompts first.

Therefore, create files that are both:

1. Self-contained enough for `/goal path/to/file.md`.
2. Written in the `/goal` mega-prompt style, so an agent can run autonomously without hand-holding.
3. Structured enough that progress, file ownership, dependencies, and verification are unambiguous.

## Your Assignment

Create a task prompt pack for implementing **Call-Time Capability Resolution + Derived Store Policy** in SymForge.

Do not change production code. Do not implement the feature. Only create planning/task files.

Recommended output directory:

```text
docs/plans/2026-05-16-call-time-capability-resolution/
```

Create:

- `README.md` - overview, execution order, dependency graph, and how to run each task with `/goal`.
- `call_time_capability_resolution_task01_contract_and_docs.md`
- `call_time_capability_resolution_task02_capability_evidence_foundation.md`
- `call_time_capability_resolution_task03_frecency_call_time_resolution.md`
- `call_time_capability_resolution_task04_cochange_lazy_prepare.md`
- `call_time_capability_resolution_task05_worktree_routing.md`
- `call_time_capability_resolution_task06_ranking_explain.md`
- `call_time_capability_resolution_task07_health_visibility_and_integration.md`

You may split or merge if repo inspection proves a better boundary, but keep each file small enough for one agent to finish in one focused session.

## Source Material To Read First

Read these repo files before authoring the tasks:

- `AGENTS.md`
- `README.md`
- `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
- `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
- `.codex/get-shit-done/templates/phase-prompt.md`
- `.codex/get-shit-done/workflows/execute-plan.md`

Treat the GSD files as planning references, not as the primary output schema.

Also inspect relevant implementation areas before naming task file ownership:

- `src/protocol/tools.rs`
- `src/live_index/frecency.rs`
- `src/live_index/persist.rs`
- `src/live_index/coupling/lifecycle.rs`
- `src/worktree.rs`
- `src/protocol/edit_hooks.rs`
- `src/protocol/ranking.rs` or the current ranking-response equivalent, if present
- tests touching `search_files`, frecency, coupling, worktree edits, ranking debug output, and health output

Use SymForge/code-intelligence tooling first where available. Fall back to raw reads only for exact source or docs.

## Product Direction To Encode

The task pack must implement the GPT-5.5 Pro recommendation:

- Do **not** build a multi-process router or multi-tenant SymForge swarm yet.
- Do **not** add a broad generic `scope` parameter in the first slice.
- Implement call-time capability resolution inside the existing in-process SymForge server.
- Environment variables should be policy overrides or defaults, not silent prerequisites for advertised tool behavior.
- If a tool call requests a capability, SymForge must do one of four things:
  - apply it,
  - prepare it and say so,
  - explain why it is unavailable,
  - explain that policy disabled it.

Core capabilities:

- Frecency: collect lightweight bumps by default where safe; use frecency ranking only when `rank_by="frecency"` or policy default requests it. Env/config can disable or change persistence policy.
- Co-change: do not build the co-change store eagerly on daemon start by default. On `rank_by="path+cochange"`, lazily prepare or report fallback state with evidence.
- Worktree-aware edits: `working_directory` should be a call-time opt-in with strict validation. Env/config should only disable policy if necessary.
- Debug ranking: expose call-time explain/debug output, such as `debug_ranking=true` or `explain=["ranking"]`. Env/config can default it on, but should not be the only way.
- Health/capabilities: make capability status visible in `health` or an equivalent capability-status surface.

## Requirements

Use these IDs in every task file. Each task must reference at least one requirement in a `Requirements covered` section.

- `CCR-1`: Requested capabilities are honored at call time or return explicit unavailable/disabled evidence.
- `CCR-2`: Env vars are policy/default overrides, not silent feature gates for normal requested behavior.
- `CCR-3`: Frecency has safe default bump collection and deterministic `rank_by="frecency"` behavior.
- `CCR-4`: Co-change ranking uses lazy bounded preparation or clear fallback evidence on first use.
- `CCR-5`: Worktree routing works from `working_directory` without requiring `SYMFORGE_WORKTREE_AWARE=1`, unless policy disables it.
- `CCR-6`: Ranking debug information is available via call-time request without requiring `SYMFORGE_DEBUG_RANKING=1`.
- `CCR-7`: Health/capability visibility reports enabled, disabled, unavailable, preparing, ready, stale, and fallback states where relevant.
- `CCR-8`: Documentation explains env vars as operational policy knobs, including disable/default-on/persistence semantics.
- `CCR-9`: Tests prove call-time behavior for requested capabilities with env vars unset.
- `CCR-10`: The design preserves local-first, in-process read-path performance and avoids startup-heavy derived-store work.

## Required Task File Format

Each task file must be directly runnable as:

```text
/goal docs/plans/2026-05-16-call-time-capability-resolution/<task-file>.md
```

Each file must use this native `/goal` structure. Do not make YAML frontmatter the primary contract.

```markdown
# /goal Call-Time Capability Resolution Task NN: Short Name

/goal [final outcome in one line] until [measurable end state] without [explicit constraints].

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Some advertised capabilities appear disabled by default through environment variables. Requested tool behavior should be available at call time or explicitly report why not.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
  - exact source files this task owns
- Requirements covered: `CCR-X`, `CCR-Y`
- Depends on: `none` or exact prior task filename(s)
- Expected files to modify:
  - `path/to/file.rs`
  - `tests/path_to_test.rs`
- Files off limits:
  - any file owned by another same-wave task

## Success Criteria - All Must Be True

1. [Specific measurable outcome.]
2. [Specific measurable outcome.]
3. Requested capability behavior is proven with env vars unset, or the response explicitly reports disabled/unavailable/fallback evidence.
4. Final implementation has no fake-success path, TODO-as-behavior, silent fallback, or placeholder architecture.
5. Verification output proves the result: test output, command output, diff evidence, or health/tool response evidence.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter unless the task explicitly says to.
- Preserve local-first, in-process read-path performance.
- Keep file writes inside the listed ownership scope unless code inspection proves a small extra file is required.
- Do not ask clarifying questions unless genuinely blocked; inspect the repo and proceed.
- Preserve user changes and unrelated worktree changes.

## Operating Rules - Non-Negotiable

1. Plan first: output a short numbered plan before editing.
2. Inspect first: use SymForge/code-intelligence tooling before raw source reads when available.
3. Work autonomously: continue until the task is complete or genuinely blocked.
4. Self-verify: after each meaningful implementation step, run the narrowest relevant check.
5. Debug failures: if verification fails, diagnose and fix before stopping.
6. No placeholders: no stubs, fake-success responses, TODO behavior, or silent fallbacks.
7. Keep a progress log: update the checklist below as work proceeds.
8. Stay scoped: if you discover adjacent work, record it as follow-up instead of expanding the task.
9. Check success before stopping: re-read every success criterion and confirm it is satisfied.

## Implementation Checklist

- [ ] Re-read this task and list the plan.
- [ ] Inspect the listed source files and tests.
- [ ] Add or update focused tests first when practical.
- [ ] Implement the behavior.
- [ ] Run task-specific tests.
- [ ] Run `cargo check`.
- [ ] Run `cargo test --all-targets -- --test-threads=1` if shared Rust behavior changed.
- [ ] Run `cargo build --release` if release-facing behavior changed.
- [ ] Update docs if behavior, env vars, or response shapes changed.
- [ ] Confirm every success criterion.

## Quality Bar

- Code is clean, typed, deterministic, and consistent with existing SymForge conventions.
- Errors and unavailable states are explicit, not hidden.
- New behavior is covered by focused tests.
- Documentation matches actual behavior.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- Proof of behavior, such as test names, health output, or response evidence.
- Decisions made.
- Known limitations and follow-ups.
```

Do not create vague task files. Each file must name exact expected source/test files after inspecting the repo. If exact files cannot be known before implementation, list the smallest likely ownership scope and require the implementing agent to update the progress log with any extra files it touches.

Optional: add a small `## Machine Metadata` section with `phase`, `plan`, `wave`, `depends_on`, `requirements`, `files_modified`, and `must_haves` if useful. Keep it readable Markdown, not the primary prompt contract.

## Scoping Rules

Each task file should contain one primary `/goal` objective plus a concrete implementation checklist. If there are substeps, keep them to 1-3 implementation chunks.

Do not make one giant plan. Avoid overlapping write sets between same-wave tasks.

Use dependencies deliberately:

- Task 01 can be docs/ADR/product contract only.
- Task 02 should create shared capability evidence/policy types before feature-specific conversions.
- Task 03 should depend on Task 02 if it consumes shared evidence types.
- Task 04 should depend on Task 02 if it consumes shared evidence types.
- Task 05 may depend on Task 02 if shared evidence/policy applies to worktree routing.
- Task 06 should depend on Task 02, and preferably Tasks 03-04 if ranking explanation needs final frecency/co-change evidence wording.
- Task 07 should depend on all implementation tasks and focus on health/status/docs/integration verification.

Suggested waves:

- Wave 1: Task 01 and, if disjoint enough, Task 02.
- Wave 2: Task 03 and Task 04 in parallel if their write sets do not overlap.
- Wave 3: Task 05 and Task 06, sequential if they both touch shared protocol response code.
- Wave 4: Task 07.

If repo inspection shows the files overlap, make them sequential.

## Task Content Expectations

Task 01 should ask the implementing agent to:

- Add or update a decision record, preferably `docs/decisions/0016-call-time-capability-resolution.md` if numbering is available.
- Update README/env-var wording so env vars are described as policy/default overrides.
- Add roadmap entry if appropriate.
- Avoid code behavior changes unless needed for docs tests.

Task 02 should ask the implementing agent to:

- Add a small capability evidence/policy model in the appropriate Rust module.
- Prefer existing project module boundaries.
- Define statuses such as `ready`, `disabled`, `unavailable`, `preparing`, `fallback`, and `stale` if they fit the codebase.
- Add focused unit tests for serialization/response shaping if exposed.

Task 03 should ask the implementing agent to:

- Convert frecency from env-gated advertised behavior to call-time resolution.
- Ensure `rank_by="frecency"` with env vars unset produces deterministic behavior or explicit evidence.
- Keep collection cheap and local-first.
- Test env-unset behavior and policy-disabled behavior.

Task 04 should ask the implementing agent to:

- Convert co-change ranking to lazy bounded prepare/fallback behavior.
- Ensure `rank_by="path+cochange"` with env vars unset does not silently ignore the requested signal.
- Test fallback/preparing/ready evidence.

Task 05 should ask the implementing agent to:

- Let edit tools honor validated `working_directory` at call time without requiring `SYMFORGE_WORKTREE_AWARE=1`, unless policy disables it.
- Keep routed write response evidence explicit and fail before write for invalid worktrees.
- Keep the response shape backward compatible unless the task explicitly documents a versioned change.

Task 06 should ask the implementing agent to:

- Add call-time ranking explain/debug output without requiring `SYMFORGE_DEBUG_RANKING=1`.
- Keep ranking explanation absent by default and concise when requested.
- Cover the new request shape in schema/roundtrip tests.

Task 07 should ask the implementing agent to:

- Surface capability policy/status in health or equivalent status output.
- Add integration tests that prove env vars unset still allow call-time requested behavior or explicit unavailable/disabled evidence.
- Run full Rust verification.
- Update docs to match actual final behavior.

## Validation Of Your Output

After creating the task files, validate the prompt pack yourself:

```powershell
git diff --check
rg -n "TODO|TBD|\\[specific measurable outcome\\]|path/to/file|CCR-X" docs\plans\2026-05-16-call-time-capability-resolution
```

The `rg` command must return no placeholder matches in the generated task files.

If you include optional GSD-compatible metadata or XML task blocks, also run:

```powershell
node .\.codex\get-shit-done\bin\gsd-tools.cjs verify plan-structure <generated-task-file>
node .\.codex\get-shit-done\bin\gsd-tools.cjs verify references <generated-task-file>
```

If validation fails, fix the task files before reporting completion.

## Final Response Expected From You

Report:

- Files created.
- The dependency/wave order.
- Any assumptions made because a code path or `/goal` parser was not discoverable.
- The validation commands you ran and their results.

Do not claim the implementation is done. The output of this assignment is the task prompt pack only.
