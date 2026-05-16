# /goal Call-Time Capability Resolution Task 06: Health Visibility And Integration

/goal close the call-time capability-resolution slice until health or equivalent status output reports capability states and installed/runtime tests prove env-vars-unset requested behavior or explicit evidence for frecency, co-change, worktree routing, and ranking diagnostics.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Some advertised capabilities appear disabled by default through environment variables. Requested tool behavior should be available at call time or explicitly report why not.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/ideas/2026-05-16-call-time-capability-resolution-goal-task-authoring-prompt.md`
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
  - `src/capability/mod.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/mod.rs`
  - `src/protocol/format.rs`
  - `src/daemon.rs`
  - `src/live_index/frecency.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/worktree.rs`
  - `tests/schema_roundtrip.rs`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-7`, `CCR-8`, `CCR-9`, `CCR-10`
- Depends on:
  - `call_time_capability_resolution_task01_contract_and_docs.md`
  - `call_time_capability_resolution_task02_capability_evidence_foundation.md`
  - `call_time_capability_resolution_task03_frecency_call_time_resolution.md`
  - `call_time_capability_resolution_task04_cochange_lazy_prepare.md`
  - `call_time_capability_resolution_task05_worktree_and_debug_explain.md`
- Expected files to modify:
  - `src/protocol/tools.rs`
  - `src/protocol/mod.rs`
  - `src/protocol/format.rs`
  - `src/daemon.rs`
  - `README.md`
  - `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
  - `docs/notes/2026-05-16-call-time-capability-resolution-close-out.md`
  - `tests/capability_status_integration.rs`
  - `tests/schema_roundtrip.rs`
- Files off limits:
  - feature-specific internals from Tasks 03-05 unless integration tests reveal a defect that must be fixed to close the gate

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `06`
- wave: `4`
- type: `integration-closeout`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-7`, `CCR-8`, `CCR-9`, `CCR-10`
- must_haves:
  - Health or equivalent status output reports capability states compactly.
  - Integration tests prove env-vars-unset requested behavior or explicit fallback evidence.
  - Docs and roadmap match final behavior.

## Success Criteria - All Must Be True

1. `health`, `health_compact`, or an equivalent status surface reports frecency, co-change, worktree routing, and ranking diagnostics capability state without excessive output noise.
2. Capability status includes enough state to distinguish ready, preparing, unavailable, disabled by policy, stale, and fallback-used where relevant.
3. Integration tests prove env-vars-unset `rank_by="frecency"` behavior.
4. Integration tests prove env-vars-unset `rank_by="path+cochange"` behavior for ready or preparing/fallback state.
5. Integration tests prove `working_directory` call-time routing or explicit disabled evidence.
6. Integration tests prove call-time ranking diagnostics without `SYMFORGE_DEBUG_RANKING=1`.
7. README and roadmap reflect final actual behavior, not planned behavior.
8. A close-out note records commands, result summaries, known limitations, and any accepted residual risk.
9. Full verification passes or any skipped check is explicitly justified with evidence.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not expand feature behavior beyond closing integration gaps from Tasks 03-05.
- Preserve local-first, in-process read-path performance.
- Keep capability status compact enough not to make health noisy.
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
- [ ] Inspect health output paths, schema roundtrip tests, and feature-specific tests from Tasks 03-05.
- [ ] Add or update capability status integration tests.
- [ ] Add health/status output for capability states.
- [ ] Update README and roadmap to match final behavior.
- [ ] Create close-out evidence note.
- [ ] Run focused integration tests.
- [ ] Run `cargo check`.
- [ ] Run `cargo test --all-targets -- --test-threads=1`.
- [ ] Run `cargo build --release`.
- [ ] Run installed-runtime smoke if available on the local machine.
- [ ] Confirm every success criterion.

## Implementation Chunks

### Chunk 1: Capability status surface

Add compact status reporting to the existing health surface or a clearly named equivalent. Preferred health wording:

```text
Capabilities:
  frecency: ready/session/persistent/disabled/no-history
  co-change: ready/preparing/unavailable/stale/disabled
  worktree routing: explicit-call enabled/disabled
  ranking diagnostics: call-time explain available/default-on/default-off
```

Avoid verbose per-row ranking dumps in health. Keep detailed ranking explanation inside `search_files` when requested.

### Chunk 2: Integration tests

Create `tests/capability_status_integration.rs` or extend an existing integration test suite. Cover at least:

- env vars unset, `rank_by="frecency"` requested: explicit evidence appears.
- env vars unset, `rank_by="path+cochange"` requested: explicit applied/preparing/unavailable evidence appears.
- `working_directory` supplied: routing or disabled evidence appears.
- `debug_ranking=true` or `explain=["ranking"]`: ranking diagnostics appear without global env.
- health/status: capability states appear and are compact.

### Chunk 3: Documentation and close-out

Update README and roadmap to final behavior. Create `docs/notes/2026-05-16-call-time-capability-resolution-close-out.md` with:

- Implementation commit chain placeholder to fill after commit.
- Test commands and result summaries.
- Installed-runtime smoke checklist.
- Residual risks and follow-ups.
- Statement that no multi-process router was implemented.

Use “pending commit SHA” only inside the close-out note if the agent has not committed yet; replace it before final completion if the workflow requires a commit.

## Verification

Run:

```powershell
cargo test --test capability_status_integration -- --test-threads=1
cargo test --test schema_roundtrip -- --test-threads=1
cargo test --test frecency_ranking -- --test-threads=1
cargo test --test cochange_fusion -- --test-threads=1
cargo test --test worktree_awareness -- --test-threads=1
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
git diff --check
rg -n "Capabilities:|frecency:|co-change:|worktree routing:|ranking diagnostics:|call-time capability" src tests README.md docs
```

If installed-runtime smoke is available locally, run the built binary against a real repo and record:

```text
symforge --version
health or health_compact
search_files rank_by="frecency"
search_files rank_by="path+cochange" anchor_path=<known path>
search_files debug_ranking=true or explain=["ranking"]
edit tool with working_directory in a safe fixture
```

## Quality Bar

- Health/status is trust-building, not noisy.
- Integration tests prove real public behavior rather than only private helper behavior.
- Documentation no longer contradicts runtime behavior.
- Close-out evidence is sufficient for a later release decision.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- Health/status sample output.
- Installed-runtime smoke result if available.
- Final docs changed.
- Known limitations and follow-ups.
