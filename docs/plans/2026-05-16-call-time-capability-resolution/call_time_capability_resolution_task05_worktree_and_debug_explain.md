# /goal Call-Time Capability Resolution Task 05: Worktree Routing And Ranking Explain

/goal make worktree routing and ranking diagnostics explicitly requestable at call time until edit tools honor validated `working_directory` without `SYMFORGE_WORKTREE_AWARE=1` and `search_files` can show ranking explanation without `SYMFORGE_DEBUG_RANKING=1`.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Some advertised capabilities appear disabled by default through environment variables. Requested tool behavior should be available at call time or explicitly report why not.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/ideas/2026-05-16-call-time-capability-resolution-goal-task-authoring-prompt.md`
  - `docs/decisions/0010-worktree-working-directory.md`
  - `docs/decisions/0012-edit-and-ranker-hook-architecture.md`
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `src/capability/mod.rs`
  - `src/capability/policy.rs`
  - `src/capability/state.rs`
  - `src/worktree.rs`
  - `src/protocol/edit_hooks.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `src/live_index/search.rs`
  - `tests/worktree_awareness.rs`
  - `tests/edit_hook_behavior.rs`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-5`, `CCR-6`, `CCR-9`, `CCR-10`
- Depends on:
  - `call_time_capability_resolution_task02_capability_evidence_foundation.md`
  - prefer after `call_time_capability_resolution_task03_frecency_call_time_resolution.md`
  - prefer after `call_time_capability_resolution_task04_cochange_lazy_prepare.md`
- Expected files to modify:
  - `src/worktree.rs`
  - `src/protocol/edit_hooks.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `src/live_index/search.rs`
  - `tests/worktree_awareness.rs`
  - `tests/edit_hook_behavior.rs`
  - `tests/search_files_ranking_debug.rs`
  - `tests/schema_roundtrip.rs`
- Files off limits:
  - `src/live_index/frecency.rs` except for read-only inspection of ranking score helpers
  - `src/live_index/coupling/lifecycle.rs` except for read-only inspection of co-change evidence
  - `tests/frecency_ranking.rs`
  - `tests/cochange_fusion.rs` unless debug-ranking response shape requires a small coordinated update

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `05`
- wave: `3`
- type: `worktree-and-debug`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-5`, `CCR-6`, `CCR-9`, `CCR-10`
- must_haves:
  - Supplied `working_directory` is explicit per-call consent for worktree routing unless policy disables it.
  - Unknown or unsafe worktree paths fail before write.
  - `search_files` supports call-time ranking explanation without global debug env.

## Success Criteria - All Must Be True

1. Edit tools that already accept `working_directory` honor it at call time without requiring `SYMFORGE_WORKTREE_AWARE=1`, unless policy explicitly disables worktree routing.
2. Unknown, non-existent, or unrelated `working_directory` values fail loudly before any write.
3. Successful routed edits include response evidence with `indexed_path`, `working_directory`, `wrote_to`, and `rerouted` or equivalent concise fields.
4. Tee snapshots still apply to the actual resolved write target before write.
5. `search_files` accepts call-time ranking diagnostics through `debug_ranking=true` or `explain=["ranking"]` without requiring `SYMFORGE_DEBUG_RANKING=1`.
6. Ranking explanation is absent by default, concise when requested, and includes which signals applied or were unavailable.
7. Tests prove env-vars-unset worktree routing, policy-disabled routing, invalid worktree failure, and call-time ranking diagnostics.
8. Verification output proves focused and shared tests pass.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not silently route writes; every reroute must be explicit in the response.
- Do not weaken `safe_repo_path`, canonicalization, or known-worktree validation.
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
- [ ] Inspect `src/worktree.rs`, `src/protocol/edit_hooks.rs`, edit-tool response formatting, and `search_files` ranking output.
- [ ] Add or update focused tests for env-unset routing and call-time ranking explanation.
- [ ] Implement worktree call-time routing.
- [ ] Implement call-time ranking diagnostics.
- [ ] Run focused worktree and ranking-debug tests.
- [ ] Run `cargo check`.
- [ ] Run `cargo test --all-targets -- --test-threads=1` because edit and search response surfaces changed.
- [ ] Run `cargo build --release` if public schema or docs changed.
- [ ] Update docs if behavior, env vars, or response shapes changed.
- [ ] Confirm every success criterion.

## Implementation Chunks

### Chunk 1: Worktree routing as call-time opt-in

Refactor `src/worktree.rs` so `SYMFORGE_WORKTREE_AWARE` is not required for the feature hook to function when `working_directory` is supplied. Preferred behavior:

```text
working_directory absent: write indexed root exactly as before
working_directory present and valid: route to matching git worktree
working_directory present and invalid: fail before write
policy disabled: fail or pass through with explicit disabled evidence, according to ADR 0016
```

If the current process-wide hook registration remains, register the hook unconditionally and make policy checks happen inside the hook. Do not let the default hook silently ignore a supplied `working_directory`.

### Chunk 2: Edit response evidence

Ensure every edit tool using the shared edit path appends or includes a concise resolved-target block when `working_directory` is supplied:

```text
Edit target: indexed_path=<path>; working_directory=<path>; wrote_to=<path>; rerouted=true
```

If the shared edit code already centralizes response formatting, implement this once there. If response formatting is split across tools, add a small helper in `src/protocol/format.rs`.

### Chunk 3: Call-time ranking explanation

Add one of these public request shapes to `SearchFilesInput`:

```text
debug_ranking: bool
```

or:

```text
explain: Vec<String>
```

Prefer `explain: ["ranking"]` if the schema already has a pattern for explain arrays; otherwise use `debug_ranking: bool` for minimal change.

When requested, output should include concise per-signal information for the returned hits or top few hits:

- path/tier score
- frecency applied, unavailable, no history, or disabled
- co-change applied, unavailable, preparing, or disabled
- final ordering note

No global `SYMFORGE_DEBUG_RANKING=1` should be required for this output.

## Verification

Run:

```powershell
cargo test --test worktree_awareness -- --test-threads=1
cargo test --test edit_hook_behavior -- --test-threads=1
cargo test --test search_files_ranking_debug -- --test-threads=1
cargo test --test schema_roundtrip -- --test-threads=1
cargo check
git diff --check
rg -n "working_directory|rerouted|wrote_to|indexed_path|debug_ranking|explain|RankingDiagnostics|SYMFORGE_WORKTREE_AWARE|SYMFORGE_DEBUG_RANKING" src tests README.md docs
```

Then run the shared suite because this touches edit and search surfaces:

```powershell
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- Worktree routing is explicit, validated, and fail-safe.
- Ranking diagnostics are concise and only appear when requested or policy-defaulted.
- Public schema changes are backward compatible.
- The implementation does not turn debug output into default response noise.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- Example routed-edit output with `working_directory`.
- Example `search_files` ranking explanation output.
- Decisions made about `debug_ranking` versus `explain` request shape.
- Known limitations and follow-ups.
