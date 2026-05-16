# /goal Call-Time Capability Resolution Task 05: Worktree Routing

/goal make edit-tool worktree routing explicitly requestable at call time until validated `working_directory` routes writes without `SYMFORGE_WORKTREE_AWARE=1`, unsafe paths fail before write, and every routed edit reports the resolved target.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Worktree-aware edits are advertised through `working_directory`, but the implementation is still process-env gated. A caller should be able to opt in at call time by supplying `working_directory`; env/config should only disable or default behavior.
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
  - `src/protocol/edit_format.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/mod.rs`
  - `tests/worktree_awareness.rs`
  - `tests/edit_hook_behavior.rs`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-5`, `CCR-9`, `CCR-10`
- Depends on:
  - `call_time_capability_resolution_task02_capability_evidence_foundation.md`
- Expected files to modify:
  - `src/worktree.rs`
  - `src/protocol/edit_hooks.rs`
  - `src/protocol/edit_format.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/mod.rs`
  - `tests/worktree_awareness.rs`
  - `tests/edit_hook_behavior.rs`
- Files off limits:
  - `src/live_index/frecency.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/live_index/search.rs`
  - ranking-debug tests and search ranking response code

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `05`
- wave: `3`
- type: `worktree-routing`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-5`, `CCR-9`, `CCR-10`
- must_haves:
  - Supplied `working_directory` is explicit per-call consent for worktree routing unless policy disables it.
  - Unknown or unsafe worktree paths fail before write.
  - Successful routed edits report the indexed path, resolved write target, and reroute state.

## Success Criteria - All Must Be True

1. Edit tools that already accept `working_directory` honor it at call time without requiring `SYMFORGE_WORKTREE_AWARE=1`, unless policy explicitly disables worktree routing.
2. Unknown, non-existent, unrelated, or unsafe `working_directory` values fail loudly before any write.
3. Successful routed edits include response evidence with `indexed_path`, `working_directory`, `wrote_to`, and `rerouted` or equivalent concise fields.
4. Tee snapshots still apply to the actual resolved write target before write.
5. Calls that omit `working_directory` preserve existing indexed-root write behavior and do not add response noise.
6. Health/conventions/help text no longer describes `SYMFORGE_WORKTREE_AWARE=1` as the prerequisite for call-time `working_directory` routing; it may describe env/config as policy disable/default behavior.
7. Tests prove env-vars-unset routing, policy-disabled routing or explicit disabled evidence, invalid worktree failure, omitted-parameter backward compatibility, and tee snapshot target correctness.
8. Verification output proves focused and shared tests pass.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not silently route writes; every reroute must be explicit in the response.
- Do not weaken `safe_repo_path`, canonicalization, known-worktree validation, or target-existence checks.
- Preserve local-first edit behavior.
- Keep file writes inside the listed ownership scope unless code inspection proves a small extra file is required.
- Do not implement ranking diagnostics in this task; Task 06 owns that.
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
- [ ] Inspect `src/worktree.rs`, `src/protocol/edit_hooks.rs`, `src/protocol/edit_format.rs`, edit-tool handlers, and existing worktree tests.
- [ ] Add or update focused tests for env-unset routing before changing the hook gate where practical.
- [ ] Convert worktree routing from env prerequisite to call-time `working_directory` opt-in with policy disable semantics.
- [ ] Preserve omitted-parameter behavior and response shape.
- [ ] Update worktree-related health/conventions/help text.
- [ ] Run focused worktree and edit-hook tests.
- [ ] Run `cargo check`.
- [ ] Run `cargo test --all-targets -- --test-threads=1` because edit behavior changed.
- [ ] Run `cargo build --release` if public docs or schemas changed.
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

### Chunk 2: Edit response evidence and tee target

Ensure every edit tool using the shared edit path appends or includes a concise resolved-target block when `working_directory` is supplied:

```text
Edit target: indexed_path=<path>; working_directory=<path>; wrote_to=<path>; rerouted=true
```

If the shared edit code already centralizes response formatting in `src/protocol/edit_format.rs`, implement this once there. Verify tee snapshots capture the resolved target path, not merely the indexed-root path.

### Chunk 3: Tests and stale wording

Update existing worktree tests that currently set `SYMFORGE_WORKTREE_AWARE=1` so at least one acceptance path proves env-vars-unset routing. Add or update tests for:

- env unset, valid worktree: routes and reports target.
- env/policy disabled: explicit disabled evidence or fail-safe behavior.
- invalid worktree: no write happens.
- omitted `working_directory`: byte-compatible indexed-root behavior.
- health/conventions text does not claim env flag is required for supplied `working_directory`.

## Verification

Run:

```powershell
cargo test --test worktree_awareness -- --test-threads=1
cargo test --test edit_hook_behavior -- --test-threads=1
cargo test --test edit_safety_tee -- --test-threads=1
cargo check
git diff --check
rg -n "working_directory|rerouted|wrote_to|indexed_path|WorktreeRouting|SYMFORGE_WORKTREE_AWARE" src tests README.md docs
```

Then run the shared suite because this touches edit behavior:

```powershell
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- Worktree routing is explicit, validated, and fail-safe.
- Public edit behavior is backward compatible when `working_directory` is omitted.
- Response evidence is concise and consistent across edit tools.
- The implementation does not treat env-var absence as a silent feature absence.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- Example routed-edit output with `working_directory`.
- Example invalid-worktree failure output.
- Decisions made about disabled-policy behavior.
- Known limitations and follow-ups.
