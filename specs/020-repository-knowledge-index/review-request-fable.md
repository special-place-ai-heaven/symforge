# Fable Review Request: Repository Knowledge Index

## Assignment

Perform a read-only, adversarial specification review of SymForge feature 020.
Do not edit code, specifications, tasks, configuration, or repository state. Do
not run mutating commands. Return a review only.

Before reviewing, read `AGENTS.md` and follow it. In particular, never inspect,
print, quote, or reproduce a secret value. Security findings must cite safe
`file:line` locations and synthetic pattern names only.

## Product intent

SymForge must gain a repository-knowledge lane that lets an LLM find exact,
source-backed facts in prose/config/schema/spec/plan/runbook files without broad
file discovery and repeated reads. This lane remains separate from code
intelligence, stays in the existing local in-memory engine, and must not add an
embedding/vector database or second search store in v1.

One metadata-first scout must account for every in-scope regular file. Giant
models, archives, datasets, databases, and other artifacts are cataloged without
full read/hash/map/decompression/deserialization and cannot prevent useful code or
knowledge from becoming ready. Watcher, reconciliation, snapshot verification,
worktree/ref sources, and queries must share the same scope and generation truth.

An exact, source-local bridge must organize code topology plus current/intent
knowledge into a bounded repository mental model. Deterministic authority review
must stop superseded or proven-divergent current-implementation prose from speaking
as current while preserving plans, ADRs, governance, and north-star intent. Review
is read-only; approved curation changes only one hash-bound repo policy ledger.

Source authorization and writable state are separate. Unsafe automatic launch roots
stay responsive but unbound and accept a later project selection. Direct explicit
protected-root indexing is allowed only with `allow_protected_root=true`; it never
touches `<source>/.symforge`, uses private user-local per-root state, and falls back
to a live memory-only index when persistence is unavailable.

## Canonical artifacts to read completely

1. `specs/020-repository-knowledge-index/spec.md`
2. `specs/020-repository-knowledge-index/plan.md`
3. `specs/020-repository-knowledge-index/research.md`
4. `specs/020-repository-knowledge-index/data-model.md`
5. `specs/020-repository-knowledge-index/tasks.md`
6. `specs/020-repository-knowledge-index/quickstart.md`
7. `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
8. `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`
9. `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
10. `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`
11. `specs/020-repository-knowledge-index/checklists/requirements.md`

Use the current source to falsify assumptions, especially:

- `src/discovery/mod.rs`
- `src/paths.rs`
- `src/daemon.rs`
- `src/cli/init.rs`
- `src/idempotency.rs`, `src/sidecar/`, `src/edit_safety/tee.rs`, and root-derived
  persistence consumers
- `src/protocol/mod.rs`, `src/cli/analytics.rs`, `src/cli/hook.rs`,
  `src/cli/operator_profile.rs`, `src/cli/onboarding.rs`, and
  `src/server/serve.rs`
- `src/live_index/store.rs`
- `src/live_index/persist.rs`
- `src/live_index/search.rs`
- `src/live_index/query.rs`, `graph.rs`, and `git_temporal.rs`
- `src/watcher/mod.rs`
- `src/domain/index.rs`
- `src/protocol/search_tools.rs`
- `src/protocol/tools.rs`
- `src/protocol/ccr.rs`
- `src/worktree/` and `src/git.rs`

## Non-negotiable invariants to attack

- Every in-scope regular file has exactly one representable terminal disposition.
- Every indexed disposition carries exactly one closed `Code`, `Knowledge`, or
  `CodeAndKnowledge` target; no empty target state can be constructed.
- Metadata-terminal artifacts receive zero probes/full reads/admitted-byte charge.
- Catalog-entry, in-flight, admitted-content, per-file, source, and output budgets
  have distinct semantics and cannot deadlock one another.
- Stable reads fail closed under concurrent mutation and preserve exact bytes.
- One logical update cannot expose mixed live/catalog/search/health generations.
- Long off-lock builds cannot overwrite a concurrent watcher/reconciliation update.
- Degraded coverage self-heals and can never be reported as complete no-evidence.
- Current worktree, linked worktrees, and admitted local refs stay separately
  labeled; all-source responses report captured source version (including closed
  working-tree state), generation/digest/coverage per source. Branch/timestamp/state
  labels never substitute for exact manifest/content identity.
- Local-ref P1 failure or memory pressure cannot block current-worktree P0.
- No fetch, checkout, archive expansion, LFS smudge, or giant blob materialization.
- Prose never enters code symbol/reference/text scope; overlapping config/schema
  targets remain possible.
- Knowledge hits are exact excerpts with file and 1-based line provenance.
- Sensitive paths are metadata-only before reads; detector-positive/indeterminate
  stable bytes are discarded before publication for both targets; defense-in-depth
  whole-hit withholding prevents any policy-detected value reaching MCP, CCR,
  snapshots, analytics, logs, diagnostics, tests, or review output.
- No second database, service, persisted unit store, or speculative parser stack.
- Automatic protected roots perform no project walk/state I/O; later valid rebind
  works in the same process and failed retarget preserves any prior project.
- Explicit protected indexing authorizes only the exact source, skips local state
  probes, and remains queryable through global or memory-only placement.
- Protected-root authority belongs to one live session request: another session,
  reconnect, snapshot, selector, or process restart cannot inherit it. An
  idempotency replay must re-establish the live membership postcondition or return
  a typed unavailable result; a historical receipt is not a successful rebind.
- A nested global state directory cannot self-index or feed watcher loops; snapshot,
  reset, quarantine, checkpoint, and idempotency paths never reconstruct state from
  the source root.
- Every state reader and writer (snapshot, idempotency, analytics, API key, TEE,
  sidecar coordination, hook state, operator profile, onboarding, quarantine, and
  checkpoint) must use the same resolved state-placement oracle; neither current
  working directory nor source root may be an implicit fallback.
- Lifecycle, authority domain, aggregate code evidence, and retrieval voice remain
  independent; age and model judgment never become deterministic contradiction.
- Role cards/bridge/authority/backlinks/temporal evidence come from one captured
  generation and discovery/review never bumps frecency.
- Gate E implements only the core immutable publication bundle; Gate G adds bridge
  state and Gate H adds authority state after their types exist. The displayed final
  `PublishedGeneration` is explicitly post-H.
- Search hits contain compact deterministic authority state, stable finding/
  provenance/link IDs, and bounded previews only. Full aggregate evidence arrays and
  bridge records are available through `review_knowledge`, not duplicated into CCR.
- Physical document move/delete is not a feature-020 mutation.

## Required review questions

1. Is every type/state transition implementable in Rust without illegal or
   unrepresentable states?
2. Do bulk load, watcher, reconciliation, snapshot restore/verify, and Git-ref
   ingestion converge on one authority?
3. Can any size/path/error/race/cancellation/circuit-breaker case make a path
   disappear or poison unrelated indexing?
4. Can any reader observe stale evidence labeled current, mixed generations, or a
   singular envelope for multiple independently updating sources?
5. Are hidden documentation/instruction paths and declared exclusions honest?
6. Does the contract match the implementation gates, existing project dispatch,
   compact facade behavior, CCR, and frecency invariants?
7. Which proposed concepts can be deleted without weakening user value or safety?
8. Which red tests are still missing a concrete runtime failure oracle?
9. Can any startup/index/session path either inherit protected-root authority or let
   state-placement failure disable later reindexing/query readiness?
10. Can stale/malformed policy, async temporal completion, context caching,
    ambiguous symbols, or contributor-history labels create false authority?
11. Can a same-path repository replacement pass snapshot verification, or can any
    idempotency replay report success without the requested project being live in
    the requesting session?
12. Do target, source-version, staged-publication, and compact-search types preserve
    the four corrected invariants without reintroducing an illegal state, forward
    dependency, unbounded vector, unstable ID, or evidence duplication?

## Output format

Return Markdown only:

```text
# Fable Adversarial Review

## Verdict
PASS | PASS WITH CHANGES | FAIL

## Findings
1. [HIGH|MEDIUM|LOW] Short title
   - Evidence: exact file:line references
   - Failure scenario: concrete runtime/user-visible sequence
   - Violated invariant: exact requirement or design rule
   - Smallest correction: precise specification/task change

## What withstands scrutiny
- Decisions that should not be weakened

## Missing tests
- Exact proposed red-test names and must-fail behavior

## Re-review trigger
- Whether accepted HIGH findings require a full repeat review
```

Do not report style preferences. Do not accept a claim because a unit test exists;
trace the real state transition. Do not propose implementation edits in this pass.
