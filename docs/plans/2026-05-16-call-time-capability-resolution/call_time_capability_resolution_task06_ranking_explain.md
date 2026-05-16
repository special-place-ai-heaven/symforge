# /goal Call-Time Capability Resolution Task 06: Ranking Explain

/goal make `search_files` ranking diagnostics explicitly requestable at call time until ranking explanation is available through a request parameter without `SYMFORGE_DEBUG_RANKING=1`, absent by default, and covered by schema and behavior tests.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: ranking debug output exists behind a process env flag, but an LLM caller should be able to ask for ranking evidence on a specific `search_files` call without restarting the MCP server.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/ideas/2026-05-16-call-time-capability-resolution-goal-task-authoring-prompt.md`
  - `docs/decisions/0012-edit-and-ranker-hook-architecture.md`
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `src/capability/mod.rs`
  - `src/capability/policy.rs`
  - `src/capability/state.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `src/live_index/search.rs`
  - `src/live_index/query.rs`
  - `src/live_index/rank_signals.rs`
  - `tests/schema_roundtrip.rs`
  - `tests/rank_signal_behavior.rs`
  - `tests/frecency_ranking.rs`
  - `tests/cochange_fusion.rs`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-6`, `CCR-9`, `CCR-10`
- Depends on:
  - `call_time_capability_resolution_task02_capability_evidence_foundation.md`
  - prefer after `call_time_capability_resolution_task03_frecency_call_time_resolution.md`
  - prefer after `call_time_capability_resolution_task04_cochange_lazy_prepare.md`
- Expected files to modify:
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `src/live_index/search.rs`
  - `tests/search_files_ranking_debug.rs`
  - `tests/schema_roundtrip.rs`
  - `tests/rank_signal_behavior.rs`
- Files off limits:
  - `src/worktree.rs`
  - `src/protocol/edit_hooks.rs`
  - `src/protocol/edit_format.rs`
  - worktree acceptance tests
  - feature-specific frecency/co-change internals except for read-only inspection or tiny compatibility updates required by the public ranking explanation shape

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `06`
- wave: `3`
- type: `ranking-explain`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-6`, `CCR-9`, `CCR-10`
- must_haves:
  - `search_files` supports call-time ranking explanation without global debug env.
  - Ranking explanation is absent by default.
  - Explanation states which signals applied, were unavailable, were disabled, or fell back.

## Success Criteria - All Must Be True

1. `search_files` accepts call-time ranking diagnostics through `debug_ranking=true` or `explain=["ranking"]` without requiring `SYMFORGE_DEBUG_RANKING=1`.
2. Ranking explanation is absent by default and does not add noise to ordinary `search_files` responses.
3. When requested, ranking explanation is concise and includes which signals applied, were unavailable, were disabled, were preparing, or fell back.
4. Explanation covers the returned ordering for default path ranking, frecency-requested ranking, and path+cochange-requested ranking where those signals are available from earlier tasks.
5. Env/config may default diagnostics on, but env-var absence never prevents call-time requested diagnostics.
6. Public schema changes are backward compatible and covered by roundtrip tests.
7. Tests prove call-time diagnostics with env vars unset, default absence, schema roundtrip, and compatibility with existing rank-signal behavior.
8. Verification output proves focused and shared tests pass.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not implement worktree routing in this task; Task 05 owns that.
- Do not make detailed per-row score dumps the default response.
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

- [x] Re-read this task and list the plan.
- [x] Inspect `SearchFilesInput`, current `SYMFORGE_DEBUG_RANKING` call sites, rank-signal helpers, and search_files formatting.
- [x] Choose `explain=["ranking"]` or `debug_ranking=true`; prefer the smaller backward-compatible request shape if no explain-array precedent exists.
- [x] Add schema/roundtrip tests before changing handler behavior where practical.
- [x] Add focused behavior tests for default absence and call-time requested diagnostics with env vars unset.
- [x] Implement call-time ranking diagnostics.
- [x] Run focused ranking-debug tests.
- [x] Run `cargo check`.
- [x] Run `cargo test --all-targets -- --test-threads=1` because search response surfaces changed.
- [x] Run `cargo build --release` if public schema or docs changed.
- [x] Update docs if behavior, env vars, or response shapes changed.
- [x] Confirm every success criterion.

## Implementation Chunks

### Chunk 1: Request shape

Add one of these public request shapes to `SearchFilesInput`:

```text
debug_ranking: bool
```

or:

```text
explain: Vec<String>
```

Prefer `explain: ["ranking"]` if the codebase already has an explain-array convention. Otherwise use `debug_ranking: bool` for minimal schema change. In either case, deserialize missing values to default-off and keep existing calls compatible.

### Chunk 2: Ranking evidence rendering

When requested, append a compact section after the normal `search_files` output. It should include enough information for an LLM to understand the ordering without flooding the response:

- requested rank mode,
- path/tier score or tier family,
- frecency applied, unavailable, no history, disabled, or not requested,
- co-change applied, unavailable, preparing, disabled, or not requested,
- final ordering note.

Reuse capability evidence formatting from Task 02 where possible. If current rank breakdown helpers only exist behind `SYMFORGE_DEBUG_RANKING=1`, move the useful rendering behind the call-time request while preserving env as a default-on policy option.

### Chunk 3: Tests and docs

Add focused tests for:

- default search output does not contain ranking explanation.
- `debug_ranking=true` or `explain=["ranking"]` shows explanation with env vars unset.
- schema roundtrip preserves the new request field.
- ranking explanation composes with `rank_by="frecency"` and `rank_by="path+cochange"` outputs without hiding capability fallback evidence.

Update README/env-var wording if needed so `SYMFORGE_DEBUG_RANKING` is described as an operational default-on/debug override, not the only way to ask for ranking diagnostics.

## Verification

Run:

```powershell
cargo test --test search_files_ranking_debug -- --test-threads=1
cargo test --test schema_roundtrip -- --test-threads=1
cargo test --test rank_signal_behavior -- --test-threads=1
cargo test --test frecency_ranking -- --test-threads=1
cargo test --test cochange_fusion -- --test-threads=1
cargo check
git diff --check
rg -n "debug_ranking|explain|RankingDiagnostics|SYMFORGE_DEBUG_RANKING|ranking explanation|rank_by" src tests README.md docs
```

Then run the shared suite because this touches search surfaces:

```powershell
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- Ranking diagnostics are concise, useful, and explicitly requested.
- Ordinary `search_files` output remains quiet.
- Public schema change is backward compatible.
- Env var absence does not block call-time diagnostics.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- Example default `search_files` output showing no explanation.
- Example requested ranking explanation output.
- Decision made about `debug_ranking` versus `explain`.
- Known limitations and follow-ups.
