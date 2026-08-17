# Feature Specification: Preventive Lifecycle Activation Cut (Feature 020 Slice 4)

**Feature Branch**: `feature-020-slice-4-candidates`

**Created**: 2026-08-17

**Status**: Draft

**Input**: User description: "Slice 4 of Feature 020 (repository-knowledge-index): the preventive-lifecycle activation cut. Enable the dark-landed preventive index lifecycle (supervisor, candidate pipeline, observer, verification, query leases, snapshot V11 migration) everywhere in one indivisible enablement unit, per the frozen Feature 020 roster T053–T073 and the campaign plan docs/reviews/FEATURE-020-SLICE4-CAMPAIGN-v11.md. The frozen spec tree specs/020-repository-knowledge-index/ (contracts v10-authority-retirement-v11.md, public-api-v11.json, lifecycle-oracle-traceability-v11.md) is immutable input — this new spec derives from it and must not contradict it."

## Relationship to the frozen Feature 020 tree *(binding)*

This specification is an **execution spec**: it scopes and governs the delivery of
Slice 4 (tasks T053–T073) of the frozen Feature 020 specification. It introduces
no new product behavior beyond what the frozen tree already specifies.

- **Normative source of truth**: `specs/020-repository-knowledge-index/`
  (spec, plan, tasks roster lines 931–956, and the frozen contracts
  `v10-authority-retirement-v11.md`, `public-api-v11.json`,
  `lifecycle-oracle-traceability-v11.md`). That tree is immutable — including
  checkbox bytes — and is **never edited** by this feature.
- **Conflict rule**: if anything in this spec tree (028) is discovered to
  contradict the frozen 020 tree, the frozen tree wins and the 028 artifact is
  amended. 028 is a living execution document, not a second authority.
- **Evidence home**: execution evidence goes under `docs/reviews/`, continuing
  the Slice 0–3 convention (`FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` per
  T072).
- Quoted identifiers such as "FR-049" and "FR-051" inside user stories and
  requirements refer to the **frozen 020 spec's** requirement numbering, not to
  the FR numbering of this document.

## Clarifications

### Session 2026-08-18

- Q: How should Wave 1 (the five dark, behavior-neutral RED+machinery pairs) land on main? → A: One PR per pair — five short-lived branches, each merged when CI is green and the darkness seal + census are reconciled.
- Q: When a Wave 1 PR is CI-green and reviewed, may I merge it without pausing for approval? → A: Auto-merge Wave 1; pause for explicit operator approval only on the Wave 2 activation-cut PR.
- Q: What review depth does each Wave 1 pair get before merging? → A: One independent code-review pass per pair, mandatorily including the cfg-lens sweep; full multi-round adversarial review reserved for the Wave 2 cut.

## User Scenarios & Testing *(mandatory)*

The five stories below are the frozen 020 tree's own per-story acceptance
statements ("Independent acceptance by feature story", tasks.md:1003–1009),
restated as prioritized journeys. Their wording is anchored to the frozen text
so acceptance remains mechanically traceable.

### User Story 1 - Safety: one bad observation never poisons the index (Priority: P1)

An agent (or human operator) works in a repository where one file is
pathological — unreadable, unstable during read, non-UTF-8-named, truncated, or
parse-failing. The index keeps serving correct answers for everything else: the
pathological observation blocks only its own candidate promotion and never
publishes partial, stale, mixed, or false-current state.

**Why this priority**: This is the reason the preventive lifecycle exists.
symforge's product is being trustworthy about what it knows; a confidently
wrong answer is unrecoverable.

**Independent Test**: Feed the closed promotion matrix inputs (`Unreadable`,
`UnstableDuringRead`, `AbortedCircuitBreaker`, `ParseStatus::Failed`, unknown
ordering, truncated required derivations, `PartialParse`) into the candidate
pipeline and verify each blocks promotion without affecting sibling sources;
verify a non-UTF-8 path retains a distinct stable native identity, stays
catalog-only with zero content probes, and never persists a lossy spelling.

**Acceptance Scenarios**:

1. **Given** a healthy indexed project, **When** one source becomes unreadable
   mid-refresh, **Then** only that source's promotion is blocked and every
   other source's answers remain current and correct.
2. **Given** two paths whose lossy display spellings collide, **When** both are
   observed, **Then** each keeps a distinct stable native identity and neither
   is content-probed or persisted under a lossy spelling.
3. **Given** a candidate build that fails or panics, **When** the supervisor
   accounts for the attempt, **Then** the candidate is discarded and no
   capability certificate can authorize a partial promotion.

---

### User Story 2 - Trust: promoted state is total, diagnostics never impersonate it (Priority: P1)

An agent queries index health and sees exactly what has been committed:
promoted manifests are total, and attempt diagnostics (retries, bounded
attempts, in-flight candidates) can never masquerade as committed dispositions.

**Why this priority**: Equal-first with safety — the reporting invariant
("a component may not report success for an operation whose completion it did
not observe") is this repository's binding rule, and Slice 4 is where the
lifecycle starts making live promises.

**Independent Test**: Drive health, health_compact, status, and the health
resources while candidates are in flight and verify committed-generation
accounting is reported separately from bounded-attempt accounting in every
surface.

**Acceptance Scenarios**:

1. **Given** a source with three failed candidate attempts and one committed
   generation, **When** any health surface is queried, **Then** the committed
   disposition and the attempt history are reported as distinct facts and
   neither is presented as the other.

---

### User Story 3 - Bounded retrieval: answers come from an exact all-Current selection or a typed refusal (Priority: P2)

An agent's query succeeds — or reports "no match" — only when every selected
source is `Current` under a strict lease; anything else produces a typed
refusal naming what was stale, missing, extra, or mismatched, never a silently
degraded answer.

**Why this priority**: Depends on P1/P2 machinery existing; it is the
user-visible contract of every query lane.

**Independent Test**: Exercise strict-query leases across atomic multi-source
capture, empty/missing/extra/mismatched `SelectedAggregate` rejection, stale
finalization, and retarget races; verify no-match is only reachable when every
selected source is `Current`.

**Acceptance Scenarios**:

1. **Given** a selection in which one source is not `Current`, **When** the
   query executes, **Then** the response is a typed refusal, not a partial
   answer and not a no-match.
2. **Given** a complete strict lease, **When** post-lease rendering truncates
   output, **Then** the response may add `OutputCoverage::Truncated` but the
   source-truth, candidate, cache, and CCR identities are unchanged.

---

### User Story 4 - Convergence: every edit class converges through bounded deltas (Priority: P2)

A developer (or agent) edits, adds, deletes, or renames files — singly or in
bursts — and the index converges through bounded delta work without a
refusal-per-edit full-rebuild availability cliff: the observed refresh gate
holds p95 ≤ 2 s and maximum ≤ 5 s from completed write burst to the first
strict lease carrying that byte identity.

**Why this priority**: The activation cut is only shippable if day-to-day
editing stays fast; this story carries the release-blocking performance gate.

**Independent Test**: Run the registered `ObservedRefreshGateV1` benchmark with
its fixed add/modify/delete/rename/terminal-classification and burst workloads
against baseline `1521abb0` and verify p95 ≤ 2 s, max ≤ 5 s, p95 ≤ 1.25×
baseline, and no single-path full rebuild outside Gap/ScopeDirty.

**Acceptance Scenarios**:

1. **Given** a completed write burst, **When** the observer coalesces and the
   candidate pipeline promotes, **Then** the first strict lease carrying that
   byte identity arrives within the gate bounds.
2. **Given** a delta candidate for one changed source, **When** it promotes,
   **Then** it exact-validates only its changed source token and patches the
   latest whole project root without reallocating unrelated newer siblings.

---

### User Story 5 - Recovery: restart trusts nothing it did not prove in this process (Priority: P3)

After a restart, crash, or upgrade, the system treats every legacy V10 cache
record, CCR handle, and snapshot byte as an untrusted seed: it promotes to
`Current` only after complete current-process proof, quarantines what fails
verification, and recovers cold-start curation read-only until `Current` —
without ever minting a source-mutation permit during recovery.

**Why this priority**: Recovery correctness protects the other four stories
across the upgrade boundary; it is exercised less often but fails
catastrophically when wrong.

**Independent Test**: Seed a process with apparently valid V10 snapshots, cache
records, and CCR handles (including root/digest mismatches and concurrent V10
writers), restart under V11, and verify bounded untrusted-seed restore,
quarantine, preserved rollback, and read-only recovery until `Current`.

**Acceptance Scenarios**:

1. **Given** a V10 snapshot whose root digest mismatches, **When** V11
   restores, **Then** the snapshot is quarantined under `.symforge/v11/`
   namespace isolation and a rebuild fallback proceeds; runtime secret-canary
   bytes never appear in snapshots, quarantine metadata, receipts, or
   diagnostics.
2. **Given** a cold start with pending curation state, **When** recovery runs,
   **Then** it cannot mint a `SourceMutationPermit` and stays read-only until
   the project reaches `Current`.

---

### Edge Cases

- Non-UTF-8 path whose lossy display collides with another path (US1 scenario 2
  — identity must stay lossless and catalog-only).
- The verification deadline boundary: just-before the 15-minute monotonic
  deadline remains eligible; at/after latches `VerificationOverdueLatched`
  before any strict acquisition; partial/cancelled/resumed work never extends
  the deadline (frozen FR-049).
- Capacity exhaustion during observer handoff: exhausted-capacity safety
  transitions must land in a safe latched state, never a silent drop.
- A cancelled, non-abortable `index_folder` run: outcome resolves via an
  activation epoch or an authoritative ACTIVE re-sync — never a half-published
  root (carried residual from Slice 3 evidence).
- Concurrent V10 writers alive during snapshot migration: V11 restore must not
  race them into mixed authority.
- Policy-version mismatch at verification time: forces non-Current
  authoritative re-scout before any new `Current` promotion.
- Retarget race between lease acquisition and project retarget: strict lease
  must reject rather than serve the wrong target.
- Team-artifact writes when git visibility is unavailable: the frozen FR-051
  four-state receipt-and-refusal matrix (`already_tracked`,
  `untracked_visible`, `ignored_force_add_required`,
  `git_visibility_unavailable`) must be exact.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The activation cut MUST ship as one indivisible enablement unit:
  no subset of T053–T073's activation behavior is independently shippable, and
  no merge or release may ship a refusal-per-edit full-rebuild phase, a legacy
  fallback, or mixed authority (tasks.md:933–934, 999–1001).
- **FR-002**: Dark preparatory work (the RED test tasks T053–T058 paired with
  machinery tasks T059–T063, T065) lands incrementally on `main` before the
  cut as **one PR per RED+machinery pair** from a short-lived branch; each
  landing MUST be behavior-neutral: the darkness seal
  (`tests/preventive_runtime_dark_v11.rs`) and the retirement census MUST be
  extended to cover it, and CI MUST stay green on every landing.
- **FR-003**: Every RED test MUST be observed failing before its minimal GREEN
  machinery is built, and no acceptance spec may be reported executed until its
  production seam exists (tasks.md:836–838).
- **FR-004**: The two publication roots MUST never be simultaneously
  authoritative, and PreventiveV1 has no in-place fallback
  (quickstart.md:724–730).
- **FR-005**: At the cut, every ingress lane in the frozen inventory — writers,
  callbacks, publication roots, cache/CCR lanes, snapshot paths, tools,
  resources, prompts, sidecar/hook lanes, raw embed bypasses — MUST resolve to
  exactly one typed authority branch of the T050 matrix
  (`GenerationLeased`, `DiskObserved`, `WorktreeScopeObserved`, `GitObserved`,
  `RuntimeHealthObserved`, `MutationPermitted`, `StateWriteAuthorized`,
  `Refused`), proven by `all_ingress_uses_exact_typed_authority_branch`.
- **FR-006**: Only SymForge-owned structural edit/curation and
  init/root-ignore/`.gitattributes`/hygiene source-byte writes may hold a fresh
  `SourceMutationPermit`, which MUST first publish non-Current; all external
  observations route through the isolated candidate pipeline without a permit
  (T064).
- **FR-007**: The activation mode machine `LegacyOpen → LegacyClosing →
  PreventiveV1Open` MUST be process-wide and non-configurable, with every
  tool/resource/prompt query, cache/CCR/retrieval, sidecar/hook, and
  finalization lane registered (T066).
- **FR-008**: In the same activation change, only the attested V11 replacement
  API and `EmbeddedSourceHandle` are exposed, and every inventoried V10
  constructor, writer, callback, secondary publication root, legacy fallback,
  handler bypass, and raw embed update/remove export is retired — all 244
  inventory members across the 13 frozen categories, with exact-graph equality
  across the 26 configuration cells (T067; frozen retirement contract).
- **FR-009**: The observed refresh gate MUST hold p95 ≤ 2 s, maximum ≤ 5 s,
  p95 ≤ 1.25× baseline `1521abb0`, with no single-path full rebuild outside
  Gap/ScopeDirty, recorded in `docs/reviews/OBSERVED-REFRESH-GATE-v1.md`
  (T068/T070); capacity accounting MUST show retained-plus-candidate peaks and
  no unaccounted residency (T069).
- **FR-010**: Every advertised edit class MUST produce the same canonical
  manifest, required artifact digests, and representative query results as a
  clean full rebuild (T071).
- **FR-011**: Snapshot migration MUST bump the format, treat V10 bytes as
  untrusted seeds, quarantine failures under `.symforge/v11/`, preserve
  rollback, and keep excluded team-artifact bytes `ProjectStateDir`-only
  without source-mutation authority; the frozen FR-051 four-state
  receipt-and-refusal matrix MUST be exact (T065).
- **FR-012**: Runtime secret-canary bytes MUST never enter snapshots,
  quarantine metadata, receipts, or diagnostics (T057).
- **FR-013**: The frozen spec tree `specs/020-repository-knowledge-index/`
  MUST NOT be modified; all execution evidence lands under `docs/reviews/`,
  closing with the T072 activation campaign and adversarial review in
  `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` and the T073
  migration documentation in `docs/migrations/v11-index-lifecycle.md`.
- **FR-014**: Test names, file paths, and pattern bases MUST match the frozen
  oracle registry (`contracts/lifecycle-oracle-traceability-v11.md`) exactly;
  the traceability validator MUST pass after every landing.
- **FR-015**: Merge gates: a Wave 1 PR merges without further operator
  approval once CI is fully green and its review pass is clean; the Wave 2
  activation-cut PR MUST NOT merge without explicit operator approval, after
  all gates and adversarial review rounds have closed.
- **FR-016**: Review cadence: each Wave 1 PR receives one independent
  code-review pass that MUST include a cfg-lens sweep (never-executed
  `cfg`-gated bodies treated as unverified claims); the Wave 2 cut receives
  fresh multi-round adversarial review per the Slice 3 discipline.

### Key Entities

- **Candidate**: an isolated, capacity-reserved build (full or delta) of a
  source or project root, carrying complete artifact certificates; commits at
  exactly one runtime-store point or is discarded whole.
- **Promotion matrix**: the closed set of terminal candidate outcomes; only a
  fully successful candidate promotes, and no certificate can authorize a
  partial promotion.
- **Observer cut**: a monotonic invalidation boundary produced by the bounded
  coalescing accumulator, with scope-dirty/gap latches and a stable handoff to
  a full successor baseline.
- **Strict lease**: the only read authority at and after the cut — an atomic,
  exact-bijection capture of selected `Current` sources; completed leases own
  render authority.
- **SourceMutationPermit**: the only write authority for SymForge-owned source
  mutations; publishing non-Current precedes the write; external observations
  never hold one.
- **VerificationScopeReceipt / VerificationWorkBound /
  VerificationFeasibilityReceipt**: sealed objects governing rolling
  verification — scope may never silently narrow, computed work never exceeds
  the 712-second reachable default, and a lost reservation forces non-Current
  rather than extending the 15-minute deadline.
- **Activation mode machine**: the process-wide `LegacyOpen → LegacyClosing →
  PreventiveV1Open` state machine; non-configurable, registered over every
  lane.
- **Untrusted seed**: any pre-existing V10 snapshot/cache/CCR byte at restart;
  usable only to accelerate re-proof, never as authority.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The T072 activation campaign passes
  `all_ingress_uses_exact_typed_authority_branch` across daemon, stdio, serve,
  embed, every tool/resource/prompt handler, sidecar/hook lanes, snapshot,
  observer, mutation, local-ref, derived, cache, CCR, and retrieval — zero
  unmatched ingress lanes.
- **SC-002**: `ObservedRefreshGateV1` records p95 ≤ 2 s, max ≤ 5 s, and
  p95 ≤ 1.25× baseline `1521abb0` in
  `docs/reviews/OBSERVED-REFRESH-GATE-v1.md`, with no single-path full rebuild
  outside Gap/ScopeDirty.
- **SC-003**: Delta/full-rebuild equivalence holds for 100% of advertised edit
  classes (identical canonical manifest, artifact digests, and representative
  query results).
- **SC-004**: Every Wave 1 landing merges with CI fully green (including the
  `embed-build` no-default-features job) and the whole-source seal plus
  retirement census reconciled; zero darkness-seal violations reach `main`.
- **SC-005**: At the cut, the only previously-`#[ignore]`d stand-ins
  (T058's four in `tests/activation_cut_v11.rs`) gain observing bodies and go
  live in the same range; no ignored lifecycle test remains after T072.
- **SC-006**: The post-slice adversarial review in
  `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` closes with
  zero unresolved P0/P1/P2 findings, including a dedicated cfg-lens sweep
  (Windows-invisible `cfg(unix)` code was the Slice 3 escape class).
- **SC-007**: `node scripts/validate-lifecycle-oracle-traceability.cjs` passes
  on the final Slice 4 tree with every T053–T073 requirement row green.
- **SC-008**: The frozen tree `specs/020-repository-knowledge-index/` is
  byte-identical before and after the slice (`git diff --stat` empty for that
  path across the whole slice range).

## Scope

**In scope**: Feature 020 tasks T053–T073 — RED oracle suites, supervisor,
candidate pipeline, observer, verification, query leases, snapshot V11
migration, ingress rerouting, activation mode machine, V10 retirement/V11
exposure, performance and capacity gates, activation campaign, migration docs.

**Out of scope**: Slice 5 mechanical removal (T074–T077) and the release/
adversarial closure train (T078–T090); any edit to the frozen 020 tree; any
new product surface not in the frozen inventory.

## Assumptions

- The dark-machinery inventory and two-wave structure recorded in
  `docs/reviews/FEATURE-020-SLICE4-CAMPAIGN-v11.md` (committed `7a476058`) are
  accurate as of `main` = `81dc7d67`; the planning phase re-verifies the
  inventory before relying on any single row.
- The Wave 2 indivisible cut ships as one PR from a dedicated branch, using
  the Slice 3 evidence discipline (frozen-source seals, TC-receipted gates,
  immutable candidates); PR granularity, merge gates, and review cadence are
  now decided in FR-002/FR-015/FR-016 (see Clarifications).
- The existing `symforge-slice4` worktree on `feature-020-slice-4-candidates`
  is the working area; heavy cargo gates run through Terminal Commander with
  serial cargo discipline, per the repository's binding build rules.
- Baseline commit `1521abb0` referenced by T070 exists and remains reachable;
  if it does not, the gate is re-anchored per the frozen tasks text before any
  benchmark result is recorded (verified during planning, not assumed at
  measurement time).
- This spec's FR/SC numbering is local to feature 028; quoted "FR-049"/
  "FR-051" identifiers are the frozen 020 spec's numbering.
