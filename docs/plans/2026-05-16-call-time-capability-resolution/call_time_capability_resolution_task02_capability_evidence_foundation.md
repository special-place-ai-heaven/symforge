# /goal Call-Time Capability Resolution Task 02: Capability Evidence Foundation

/goal add the minimal shared capability evidence and policy model until Rust code can express applied, preparing, unavailable, disabled, stale, and fallback states without wiring any feature conversion yet.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Some advertised capabilities appear disabled by default through environment variables. Requested tool behavior should be available at call time or explicitly report why not.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/ideas/2026-05-16-call-time-capability-resolution-goal-task-authoring-prompt.md`
  - `docs/decisions/0016-call-time-capability-resolution.md` if Task 01 has landed
  - `src/lib.rs`
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs`
  - `src/live_index/frecency.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/worktree.rs`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-7`, `CCR-10`
- Depends on: none, but prefer running after `call_time_capability_resolution_task01_contract_and_docs.md` if ADR 0016 exists
- Expected files to modify:
  - `src/lib.rs`
  - `src/capability/mod.rs`
  - `src/capability/state.rs`
  - `src/capability/policy.rs`
  - `src/protocol/format.rs`
  - `tests/capability_evidence.rs`
- Files off limits:
  - `src/live_index/frecency.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/worktree.rs`
  - edit-tool behavior files except for read-only inspection

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `02`
- wave: `1`
- type: `foundation`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-7`, `CCR-10`
- must_haves:
  - Public Rust enums or structs represent capability names, statuses, freshness, cost, safety, and policy.
  - Formatting helper can render concise capability evidence lines for tool responses.
  - Focused tests cover display text and default policy behavior.

## Success Criteria - All Must Be True

1. A small `src/capability/` module exists and is exported from `src/lib.rs`.
2. Capability evidence can represent at least: `Applied`, `Ready`, `Preparing`, `Unavailable`, `DisabledByPolicy`, `FallbackUsed`, and `Stale` or equivalent names.
3. Capability policy can express env/config defaults without reading environment variables inside the evidence type itself.
4. `src/protocol/format.rs` or a narrowly scoped helper can render one-line evidence suitable for appending to tool outputs.
5. Tests prove the default policy is deterministic and evidence rendering is stable.
6. This task does not convert frecency, co-change, worktree, or debug-ranking behavior yet.
7. Verification output proves the foundation compiles and tests pass.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not change behavior of any existing tool beyond adding reusable formatting helpers that are not called yet.
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
- [ ] Inspect `src/protocol/format.rs`, `src/live_index/frecency.rs`, `src/live_index/coupling/lifecycle.rs`, and `src/worktree.rs` to confirm naming conventions.
- [ ] Add the new `src/capability/` module and export it from `src/lib.rs`.
- [ ] Add focused tests for evidence rendering and default policy shape.
- [ ] Implement the behavior.
- [ ] Run `cargo test --test capability_evidence -- --test-threads=1`.
- [ ] Run `cargo check`.
- [ ] Run `cargo test --all-targets -- --test-threads=1` if shared Rust behavior changed beyond inert types/helpers.
- [ ] Confirm every success criterion.

## Implementation Chunks

### Chunk 1: Capability state types

Create compact types such as:

```rust
CapabilityName
CapabilityStatus
CapabilityFreshness
CapabilityCost
CapabilitySafety
CapabilityPolicy
CapabilityEvidence
```

Expected capability names for this slice:

- `FrecencyRanking`
- `CoChangeRanking`
- `WorktreeRouting`
- `RankingDiagnostics`

Keep the API deliberately small. Do not create a central dispatcher. Do not introduce project tenants or multi-index routing.

### Chunk 2: Policy defaults

Define policy values that future tasks can consume:

- Frecency collection: session or persistent policy, with a disabled override.
- Coupling prepare: lazy-on-request by default, with warm-on-start and disabled variants available.
- Worktree routing: explicit call-time routing unless disabled by policy.
- Ranking diagnostics: call-time explain unless environment or config defaults it on.

The policy model should not decide behavior yet. It should only make future behavior expressible without each handler inventing its own strings.

### Chunk 3: Formatting and tests

Add a helper that renders a concise line such as:

```text
Capability: co-change ranking preparing — coupling store warming; path ranking returned.
```

Focused tests should assert stable rendering for applied, preparing, unavailable, disabled, stale, and fallback states.

## Verification

Run:

```powershell
cargo test --test capability_evidence -- --test-threads=1
cargo check
git diff --check
rg -n "CapabilityEvidence|CapabilityStatus|CapabilityPolicy|FrecencyRanking|CoChangeRanking|WorktreeRouting|RankingDiagnostics" src tests
```

Run full tests if the implementation touches shared formatting called by existing tools:

```powershell
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- Evidence types are small, typed, and deterministic.
- Display strings are concise enough for MCP output.
- The design does not require all tools to route through a heavy central abstraction.
- No feature conversion happens before the common vocabulary exists.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- The exact evidence/status names chosen.
- Any follow-up changes required for Tasks 03-06.
