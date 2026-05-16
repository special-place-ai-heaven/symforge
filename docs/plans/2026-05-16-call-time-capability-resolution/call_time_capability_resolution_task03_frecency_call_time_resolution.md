# /goal Call-Time Capability Resolution Task 03: Frecency Call-Time Resolution

/goal convert frecency from a silent environment-gated ranking feature into call-time requested behavior until `search_files(rank_by="frecency")` with env vars unset returns deterministic frecency evidence or explicit no-history/policy evidence without bumping discovery tools.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Some advertised capabilities appear disabled by default through environment variables. Requested tool behavior should be available at call time or explicitly report why not.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/ideas/2026-05-16-call-time-capability-resolution-goal-task-authoring-prompt.md`
  - `docs/decisions/0011-frecency-bump-policy.md`
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `src/capability/mod.rs`
  - `src/capability/policy.rs`
  - `src/capability/state.rs`
  - `src/live_index/frecency.rs`
  - `src/live_index/persist.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `tests/frecency_ranking.rs`
  - `tests/edit_hook_behavior.rs`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-3`, `CCR-9`, `CCR-10`
- Depends on: `call_time_capability_resolution_task02_capability_evidence_foundation.md`
- Expected files to modify:
  - `src/live_index/frecency.rs`
  - `src/live_index/persist.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `tests/frecency_ranking.rs`
  - `tests/call_time_frecency.rs`
  - `tests/edit_hook_behavior.rs`
- Files off limits:
  - `src/live_index/coupling/lifecycle.rs`
  - `src/worktree.rs`
  - `tests/cochange_fusion.rs`
  - `tests/worktree_awareness.rs`

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `03`
- wave: `2`
- type: `frecency-conversion`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-3`, `CCR-9`, `CCR-10`
- must_haves:
  - `rank_by="frecency"` no longer silently depends on `SYMFORGE_FRECENCY=1`.
  - Discovery tools still never bump frecency.
  - Capability evidence distinguishes applied, no-history, disabled-by-policy, and unavailable states.

## Success Criteria - All Must Be True

1. `search_files(rank_by="frecency")` with `SYMFORGE_FRECENCY` unset returns path results plus explicit frecency capability evidence.
2. If frecency history exists, requested frecency ranking applies deterministically and says how many returned candidates had frecency scores.
3. If no frecency history exists, output explicitly says frecency history is empty or not yet useful and path ranking was returned.
4. If policy disables frecency, output explicitly says disabled by policy and path ranking was returned.
5. Commitment tools still record bumps through the intended frecency path, but `search_files`, `search_text`, and `search_symbols` still do not bump.
6. The implementation preserves discovery-only sessions that do not request frecency: no frecency database is created merely by ordinary search.
7. Tests prove env-vars-unset behavior, policy-disabled behavior, existing-history behavior, and discovery-no-bump behavior.
8. Verification output proves focused and shared tests pass.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not change co-change behavior in this task except to avoid merge conflicts in shared helper calls.
- Preserve ADR 0011: discovery tools deliberately do not bump frecency.
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
- [ ] Inspect `src/live_index/frecency.rs`, `src/protocol/tools.rs`, `tests/frecency_ranking.rs`, and `tests/edit_hook_behavior.rs`.
- [ ] Add focused env-unset and policy-disabled tests before broad refactoring where practical.
- [ ] Implement frecency capability resolution and response evidence.
- [ ] Run focused frecency tests.
- [ ] Run `cargo check`.
- [ ] Run `cargo test --all-targets -- --test-threads=1` because shared ranking and edit-hook behavior changed.
- [ ] Run `cargo build --release` if README or public behavior is updated in this task.
- [ ] Update docs if behavior, env vars, or response shapes changed.
- [ ] Confirm every success criterion.

## Implementation Chunks

### Chunk 1: Define frecency policy behavior

Use the capability foundation from Task 02 to define frecency policy outcomes. The preferred behavior is:

```text
ordinary search without rank_by="frecency": no database creation, no frecency evidence noise
commitment tool on a known path: safe lightweight bump collection unless policy disables it
search_files rank_by="frecency": open existing history or session history, apply if useful, otherwise report no-history fallback
```

Keep env/config as policy override. Do not let unset env mean “feature silently absent” for the requested rank mode.

### Chunk 2: Convert ranking branch

In `src/protocol/tools.rs`, replace the current strict `SYMFORGE_FRECENCY=1` branch with call-time resolution:

- Requested `rank_by="frecency"` asks the frecency capability resolver for history.
- Existing store/history is used read-only for ranking.
- Missing history returns explicit evidence and preserves path ranking.
- Policy-disabled returns explicit evidence and preserves path ranking.
- Unreadable/corrupt store returns explicit unavailable evidence and preserves path ranking.

If persistent bump collection is still policy-controlled, make the output honest about whether the current session has history.

### Chunk 3: Preserve bump contract

In `src/live_index/frecency.rs`, preserve these invariants:

- Discovery tools do not call bump.
- Batch tools deduplicate bump paths.
- Errors never break commitment tools.
- Corrupt store or policy-disabled state is quarantined and reported only when requested by `rank_by="frecency"` or health/status.

Add tests for env unset and policy disabled. Use existing `FlagGuard` patterns if present, but adapt names so the tests prove call-time behavior rather than old feature-gate behavior.

## Verification

Run:

```powershell
cargo test --test frecency_ranking -- --test-threads=1
cargo test --test call_time_frecency -- --test-threads=1
cargo test --test edit_hook_behavior -- --test-threads=1
cargo check
git diff --check
rg -n "rank_by.*frecency|FrecencyRanking|frecency capability|SYMFORGE_FRECENCY" src tests README.md docs
```

Then run the shared suite if any shared ranking, edit-hook, or protocol code changed:

```powershell
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- Requested frecency behavior is honest even when no history exists.
- Ordinary discovery remains footprint-free.
- The implementation does not make startup heavier.
- Output is concise enough for MCP use but explicit enough for trust.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- Example output for env-unset `search_files(rank_by="frecency")`.
- Decisions made about session versus persistent frecency collection.
- Known limitations and follow-ups.
