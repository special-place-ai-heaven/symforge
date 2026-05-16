# /goal Call-Time Capability Resolution Task 01: Contract And Docs

/goal establish the call-time capability-resolution product contract until README, ADR 0016, and the roadmap describe env vars as policy/default overrides rather than silent prerequisites without changing production behavior.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: Some advertised capabilities appear disabled by default through environment variables. Requested tool behavior should be available at call time or explicitly report why not.
- Relevant source material:
  - `AGENTS.md`
  - `README.md`
  - `docs/ideas/2026-05-16-capability-router-scoped-index-ideation-brief.md`
  - `docs/ideas/2026-05-16-call-time-capability-resolution-goal-task-authoring-prompt.md`
  - `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
  - `docs/decisions/0011-frecency-bump-policy.md`
  - `docs/decisions/0012-edit-and-ranker-hook-architecture.md`
  - `docs/decisions/0013-coupling-signal-contract.md`
  - `docs/decisions/0010-worktree-working-directory.md`
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-8`, `CCR-10`
- Depends on: none
- Expected files to modify:
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `README.md`
  - `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
- Files off limits:
  - `src/**/*.rs`
  - `tests/**/*.rs`
  - any generated release metadata unless the task explicitly discovers documentation tooling requires it

## Machine Metadata

- phase: `3g-call-time-capability-resolution`
- plan: `01`
- wave: `1`
- type: `docs-contract`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-8`, `CCR-10`
- must_haves:
  - ADR states that advertised capabilities must apply at call time or return capability evidence.
  - README environment-variable table describes policy/default override semantics.
  - Roadmap contains a bounded Wave 3g or equivalent entry that does not imply a multi-process router.

## Success Criteria - All Must Be True

1. `docs/decisions/0016-call-time-capability-resolution.md` exists and explicitly rejects a first-pass multi-process/multi-index router.
2. ADR 0016 defines the contract: requested capabilities are applied, prepared with evidence, marked unavailable, or marked disabled by policy.
3. README changes describe `SYMFORGE_FRECENCY`, `SYMFORGE_COUPLING`, `SYMFORGE_DEBUG_RANKING`, and `SYMFORGE_WORKTREE_AWARE` as operational policy/default knobs, not the only way an LLM can request advertised capability behavior.
4. `docs/plans/2026-05-15-symforge-post-h-roadmap.md` records a bounded call-time capability-resolution unit or wave after the current Wave 3 foundation work.
5. This task makes no production-code behavior changes.
6. Documentation does not promise behavior that later tasks are not explicitly assigned to implement.
7. Verification output proves no placeholder strings and no whitespace errors remain.

## Constraints

- Do not build a multi-process router or multi-tenant SymForge swarm in this task.
- Do not add a broad generic `scope` parameter.
- Do not change Rust production code.
- Do not claim call-time capability implementation is complete.
- Preserve local-first, in-process read-path performance as a stated requirement.
- Keep file writes inside the listed documentation ownership scope unless code inspection proves a small extra doc file is required.
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
- [ ] Inspect the source material listed in Context.
- [ ] Draft ADR 0016 with decision, rationale, non-goals, capability-state vocabulary, migration direction, and acceptance criteria.
- [ ] Update README environment-variable wording so env vars are operational policy knobs.
- [ ] Add a bounded roadmap entry for call-time capability resolution without implying that implementation is already complete.
- [ ] Run documentation-focused grep checks.
- [ ] Run `git diff --check`.
- [ ] Run `cargo check` only if a tool or CI policy requires it after documentation edits.
- [ ] Confirm every success criterion.

## Implementation Chunks

### Chunk 1: ADR 0016

Create `docs/decisions/0016-call-time-capability-resolution.md` with these sections:

- Status: accepted or proposed, depending on project convention.
- Problem: MCP clients cannot restart the server mid-task to set feature env vars.
- Decision: advertised capability parameters are resolved at call time; env vars become policy/default overrides.
- Non-goals: no multi-process router, no generic scope system, no cloud control plane.
- Capability outcomes: applied, preparing, unavailable, disabled by policy, fallback used, stale.
- Source-of-truth rule: `LiveIndex` remains authoritative; derived stores are advisory.
- Safety rule: write routing requires explicit call-time `working_directory` plus validation and response evidence.
- Performance rule: no heavy startup work unless policy opts into background warm.
- Migration order: shared evidence model, frecency, co-change, worktree/debug, health/integration.

### Chunk 2: README policy wording

Update the environment-variable section so each gated variable is described as policy/default control:

- Frecency: can be requested by `rank_by="frecency"`; env controls collection/persistence/default behavior once later implementation lands.
- Coupling: can be requested by `rank_by="path+cochange"`; env controls warm-on-start/disable behavior once later implementation lands.
- Worktree awareness: `working_directory` is intended to be explicit call-time routing consent; env can disable or default behavior once later implementation lands.
- Debug ranking: call-time explain/debug is the intended diagnostic path; env may default diagnostics on.

Use careful wording such as “planned call-time behavior” if production code is not yet updated.

### Chunk 3: Roadmap entry

Update `docs/plans/2026-05-15-symforge-post-h-roadmap.md` with a concise Wave 3g entry:

- Title: Call-Time Capability Resolution + Derived Store Policy.
- Dependency: after current trust and edit-safety foundation work.
- Scope: tasks 01-06 from this prompt pack.
- Gate: env-vars-unset call-time capability tests pass.

## Verification

Run:

```powershell
git diff --check
rg -n "multi-process router|call-time capability|SYMFORGE_FRECENCY|SYMFORGE_COUPLING|SYMFORGE_WORKTREE_AWARE|SYMFORGE_DEBUG_RANKING" README.md docs\decisions\0016-call-time-capability-resolution.md docs\plans\2026-05-15-symforge-post-h-roadmap.md
rg -n "placeholder marker|unresolved requirement id|silent prerequisite" README.md docs\decisions\0016-call-time-capability-resolution.md docs\plans\2026-05-15-symforge-post-h-roadmap.md
```

The second `rg` may match deliberate discussion of “silent prerequisites” only if the surrounding sentence says they are disallowed. It must not find unresolved placeholders.

## Quality Bar

- Wording is precise enough that future agents cannot interpret env vars as required startup gates for advertised requested behavior.
- ADR is concise but complete.
- README is honest about current behavior versus planned migration.
- Roadmap does not conflict with existing Wave 3/Wave 4 sequencing.
- The final output would survive a senior code review.

## Final Deliverable

Report:

- Confirmation for each success criterion.
- Every file created or modified.
- Verification commands run and results.
- The final ADR status chosen and why.
- Any implementation follow-ups discovered while editing docs.
