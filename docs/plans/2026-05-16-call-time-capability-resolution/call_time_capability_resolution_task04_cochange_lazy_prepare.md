# /goal Call-Time Capability Resolution Task 04: Co-Change Lazy Prepare

/goal convert co-change ranking from startup/env-prepared behavior into call-time lazy capability resolution until `search_files(rank_by="path+cochange", anchor_path=...)` with env vars unset uses ready evidence, starts bounded preparation, or returns explicit fallback evidence.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Some advertised capabilities appear disabled by default through environment variables. Requested tool behavior should be available at call time or explicitly report why not.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/ideas/2026-05-16-call-time-capability-resolution-goal-task-authoring-prompt.md`
  - `docs/decisions/0013-coupling-signal-contract.md`
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `src/capability/mod.rs`
  - `src/capability/policy.rs`
  - `src/capability/state.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/live_index/coupling/mod.rs`
  - `src/live_index/store.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `tests/cochange_fusion.rs`
  - `tests/coupling_refresh_generation_fence.rs`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-4`, `CCR-9`, `CCR-10`
- Depends on: `call_time_capability_resolution_task02_capability_evidence_foundation.md`
- Expected files to modify:
  - `src/live_index/coupling/lifecycle.rs`
  - `src/live_index/store.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `tests/cochange_fusion.rs`
  - `tests/call_time_cochange.rs`
  - `tests/coupling_refresh_generation_fence.rs`
- Files off limits:
  - `src/live_index/frecency.rs`
  - `src/worktree.rs`
  - `tests/frecency_ranking.rs`
  - `tests/worktree_awareness.rs`

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `04`
- wave: `2`
- type: `cochange-conversion`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-4`, `CCR-9`, `CCR-10`
- must_haves:
  - `rank_by="path+cochange"` no longer requires `SYMFORGE_COUPLING=1` to explain or prepare capability state.
  - No heavy coupling build runs on startup by default.
  - First-use behavior is bounded, explicit, and test-covered.

## Success Criteria - All Must Be True

1. `search_files(rank_by="path+cochange", anchor_path=...)` with `SYMFORGE_COUPLING` unset returns explicit co-change capability evidence.
2. If an existing coupling store is ready and fresh enough, co-change ranking applies and reports applied evidence.
3. If no coupling store exists, the first request starts lazy bounded preparation or records a clear unavailable/preparing fallback without blocking unboundedly.
4. Ordinary daemon/server startup does not eagerly build `.symforge/coupling.db` unless policy explicitly asks for warm-on-start.
5. Coupling store corruption, non-git repos, missing anchors, and empty neighbor sets each produce precise evidence rather than silent path-only behavior.
6. Generation fencing and existing watcher safety discipline remain intact for refresh or background work.
7. Tests prove env-vars-unset behavior, ready-store behavior, preparing/fallback behavior, and startup no-heavy-work behavior.
8. Verification output proves focused and shared tests pass.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not change frecency behavior in this task except to share evidence formatting.
- Do not make default startup analyze git history unless explicit policy says warm-on-start.
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
6. No placeholders: no stubs, fake-success responses, unfinished behavior, or silent fallbacks.
7. Keep a progress log: update the checklist below as work proceeds.
8. Stay scoped: if you discover adjacent work, record it as follow-up instead of expanding the task.
9. Check success before stopping: re-read every success criterion and confirm it is satisfied.

## Implementation Checklist

- [ ] Re-read this task and list the plan.
- [ ] Inspect `src/live_index/coupling/lifecycle.rs`, `src/live_index/store.rs`, `src/protocol/tools.rs`, and existing co-change tests.
- [ ] Add or update focused tests for env-unset and missing-store behavior first where practical.
- [ ] Implement lazy prepare/status behavior.
- [ ] Run focused co-change tests.
- [ ] Run `cargo check`.
- [ ] Run `cargo test --all-targets -- --test-threads=1` because shared search and coupling lifecycle behavior changed.
- [ ] Run `cargo build --release` if release-facing behavior changed.
- [ ] Update docs if behavior, env vars, or response shapes changed.
- [ ] Confirm every success criterion.

## Implementation Chunks

### Chunk 1: Split coupling policy from env gate

Refactor `src/live_index/coupling/lifecycle.rs` so `SYMFORGE_COUPLING` is no longer the only way to prepare or use the store. Preserve an operational policy override with states such as:

- disabled by policy
- lazy on request
- warm on start

The default for this task should be lazy on request, not warm on start.

### Chunk 2: Lazy first-use prepare

On `search_files(rank_by="path+cochange", anchor_path=...)`:

- Try to open an existing store.
- If store exists and has usable evidence, apply ranking and report ready/applied evidence.
- If store is missing and policy permits lazy prepare, start bounded preparation or schedule background preparation.
- Return path-ranked results immediately if preparation cannot finish within the chosen bounded budget.
- Append capability evidence that says preparing or unavailable.

Do not let the handler block indefinitely on git history analysis.

### Chunk 3: Test state transitions

Add tests that cover:

- env unset, no store: fallback evidence says preparing or unavailable.
- existing store with qualifying neighbor rows: co-change applies and rows show shared-commit evidence.
- non-git repo: unavailable evidence.
- disabled policy: disabled evidence.
- repeated first-use calls do not spawn duplicate unbounded builders.

## Verification

Run:

```powershell
cargo test --test cochange_fusion -- --test-threads=1
cargo test --test call_time_cochange -- --test-threads=1
cargo test --test coupling_refresh_generation_fence -- --test-threads=1
cargo check
git diff --check
rg -n "CoChangeRanking|path\+cochange|coupling store|SYMFORGE_COUPLING|lazy" src tests README.md docs
```

Then run the shared suite because this touches search and lifecycle paths:

```powershell
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- Co-change evidence is advisory and never silently overrides current file bytes.
- First-use behavior is bounded and honest.
- Startup remains light by default.
- The implementation preserves generation-fence discipline for background refreshes.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- Example output for env-unset missing-store `path+cochange`.
- Example output for ready-store applied co-change ranking.
- Decisions made about bounded warmup duration and background scheduling.
- Known limitations and follow-ups.
