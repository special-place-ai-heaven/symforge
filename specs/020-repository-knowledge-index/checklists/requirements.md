> [!CAUTION]
> **The checked V10 checklist below is immutable historical evidence, not V11
> authorization.** Preserve every legacy line verbatim. Only the unchecked `CHKxxx`
> section after `END V10 HISTORICAL CHECKLIST` evaluates the V11 refreeze.

<!-- BEGIN V10 HISTORICAL CHECKLIST — PRESERVE EVERY LINE VERBATIM -->

# Specification Quality Checklist: Repository Knowledge Index

**Purpose**: Validate the feature contract before implementation<br>
**Created**: 2026-07-16<br>
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] User value and current failure pattern are explicit.
- [x] Code intelligence, knowledge retrieval, and catalog scope are separated.
- [x] “All files” and declared exclusions are defined without overclaiming.
- [x] Embeddings/vector DB/generative ingestion are explicitly non-foundational.
- [x] Assumptions and non-goals are explicit.

## Requirement Completeness

- [x] P0 safety, trust, value, freshness, recovery, and security stories exist.
- [x] Worktree/local-ref behavior has a separately testable P1 story.
- [x] Every story has an independent test and acceptance scenarios.
- [x] Functional and non-functional requirements are measurable.
- [x] Terminal failure states and coverage semantics are defined.
- [x] Resource accounting distinguishes catalog entries from ingested bytes.
- [x] Watcher, reconciliation, snapshot, verification, and publication are covered.
- [x] MCP input/output/no-match/error/security behavior is contracted.
- [x] Exact provenance and freshness authority are mandatory.
- [x] Indexed target state is invariant-bearing and cannot represent neither lane.
- [x] Source version has a closed working-tree state and is propagated through the
  manifest, snapshot identity, publication, and every per-source response envelope.
- [x] Gate E compiles only against core publication types; Gates G/H add bridge and
  authority state in order, and the displayed final shape is identified as post-H.
- [x] Search returns compact deterministic evidence plus stable IDs/bounded previews;
  full evidence arrays and bridge records remain available through review only.
- [x] Source-root authorization, project-state placement, and process-global control
  placement are separate and cover unbound rebind, explicit protected indexing,
  memory-only fallback, failed-retarget preservation, and nested-state exclusion.
- [x] Existing `.gitignore` hygiene and legacy team-artifact compatibility have one
  non-contradictory mutation contract.
- [x] Mental-model/bridge candidates, ambiguity, ownership provenance, context
  sections, cache identity, and one-generation capture are contracted.
- [x] Lifecycle, authority, aggregate code evidence, retrieval voice, review,
  idempotent ledger-only curation, and no-file-delete boundaries are contracted.
- [x] No `[NEEDS CLARIFICATION]` markers remain.

## Robustness Oracles

- [x] Every discovered path has exactly one terminal disposition.
- [x] Metadata-terminal giant artifacts receive zero content reads.
- [x] Stable-read mutation races fail closed.
- [x] Reconciliation covers creates/deletes/catalog-only files.
- [x] Snapshot round-trip preserves manifest/query parity.
- [x] Concurrent publication cannot mix generations.
- [x] Security fixtures prohibit secret-value leakage.
- [x] Worktree/ref variants remain labeled and deduplicated safely.
- [x] Unsafe automatic launch is non-fatal; explicit protected mode never touches
  source-local state and state failures cannot disable live queries.
- [x] Malformed/stale policy cannot suppress raw knowledge or poison code serving.
- [x] Full/compact surface counts are fixed at 39/3 with read/mutation annotations.

## Review Readiness

- [x] `plan.md`, `research.md`, `data-model.md`, `tasks.md`, `quickstart.md`, and contracts complete.
- [x] Fresh opposite-model Skeptic closure review complete.
- [x] Fresh opposite-model Architect closure review complete.
- [x] Fresh opposite-model Minimalist closure review complete.
- [x] Lead judgment recorded; all accepted high findings resolved locally.
- [x] Final corrected spec contains no known competing/contradictory authority.
- [x] Focused closure re-review completed with no HIGH and authorized one scoped delta
  pass after its five MEDIUM corrections.
- [x] Scoped delta verification confirms no unrefuted HIGH/MEDIUM finding and
  authorizes SpecKit freeze.

The initial broad external review and complete focused three-lens re-review are both
preserved. The focused report found no HIGH, so its freeze decision permits one
read-only verification limited to the five corrected seams. Production implementation
was blocked until `fable-scoped-delta-verification-2026-07-17.md` declared READY TO
FREEZE; Gate A is now complete.

<!-- END V10 HISTORICAL CHECKLIST -->

---

# V11 Lifecycle-Prevention Requirements Quality Checklist

**Purpose**: Formal release-gate review of the Feature 020 V11 refreeze<br>
**Created**: 2026-08-11<br>
**Audience**: Refreeze author, independent reviewer, and release approver<br>
**Authority**: [Lifecycle-prevention design](../../../docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md) and the exact hash-pinned refreeze corpus

All V11 items begin unchecked deliberately. An item may be checked only against the
exact frozen corpus after the manifest-aware validator supplies objective evidence;
the old V10 checks and an in-tree assertion cannot satisfy these items.

## A01-A19 amendment completeness

- [ ] CHK001 Is A01 explicit that failed work leaves the retained verified generation unchanged while immutable runtime state carries work/failure, with no degraded wrapper publication? [Completeness, Design §3 A01]
- [ ] CHK002 Is A02 explicit that cold start without a verified generation is non-queryable on every strict lane? [Coverage, Design §3 A02]
- [ ] CHK003 Is A03 explicit that all existing public consumers are strict-`Current` and retained last-known-good state remains internal and non-queryable? [Clarity, Design §3 A03]
- [ ] CHK004 Does A04 enumerate every promotion-blocking incomplete disposition, including unreadable/unstable input, circuit-breaker abort, parse failure, unknown ordering, truncated derived coverage, and `PartialParse`, while preserving complete metadata-terminal exclusions? [Completeness, Design §3 A04]
- [ ] CHK005 Is A05 unambiguous that V11 capability certificates attest only complete generations and cannot authorize partial promotion? [Clarity, Design §3 A05]
- [ ] CHK006 Is A06 explicit that circuit-breaker failure cancels and discards the candidate without producing any partially published state? [Exception Flow, Design §3 A06]
- [ ] CHK007 Does A07 define NFR-003 so one bad file blocks candidate promotion/current acquisition without changing or making queryable the retained generation? [Consistency, Design §3 A07]
- [ ] CHK008 Is A08 explicit that aborted attempts expose bounded attempt diagnostics but no canonical committed manifest? [Clarity, Design §3 A08]
- [ ] CHK009 Does A09 remove every compatibility claim that permits a partial parse to be served as `Current`? [Consistency, Design §3 A09]
- [ ] CHK010 Does A10 require temporal, bridge, authority, and mental-model derived work either to complete inside the advertised strict scope before promotion or to be absent from that runtime's protocol surface? [Completeness, Design §3 A10]
- [ ] CHK011 Is A11 explicit that truncation in a required derived scope blocks promotion unless a separately refrozen capability contract defines closed proof, disclosure, and invalidation rules? [Edge Case, Design §3 A11]
- [ ] CHK012 Does A12 preserve protected-root readiness under user-local or memory-only placement without any state/durability-probe I/O below the protected source root? [Security, Design §3 A12]
- [ ] CHK013 Does A13 define no-match as valid only when every selected required source is `Current`, with all other cases expressed as per-source readiness refusal? [Acceptance Criteria, Design §3 A13]
- [ ] CHK014 Does A14 restrict first-contact coverage and role evidence to `Current` generations and prohibit partial orientation from a non-current selection? [Consistency, Design §3 A14]
- [ ] CHK015 Does A15 distinguish exact promoted-manifest accounting from bounded aborted-attempt accounting without allowing an attempt to claim canonical repository coverage? [Measurability, Design §3 A15]
- [ ] CHK016 Does A16 require health to separate committed-generation accounting from attempt diagnostics in every representation? [Consistency, Design §3 A16]
- [ ] CHK017 Does A17 keep persistence/durability orthogonal to query readiness while making gapped or incomplete observer coverage sufficient to refuse strict-current acquisition? [Clarity, Design §3 A17]
- [ ] CHK018 Does A18 define `ObservedRefreshGateV1` with measurable edit-convergence latency, clean-rebuild equivalence, burst behavior, and admitted-memory criteria rather than qualitative “fast” language? [Measurability, Design §3 A18]
- [ ] CHK019 Does A19 replace every public degraded/last-verified contract row with typed `SourceRefusal`, keep retention internal, and define `authority_scope` solely as `KnowledgeVoiceFilter` rather than consistency selection? [Consistency, Design §3 A19]

## Refreeze authority and tamper resistance

- [ ] CHK020 Does the refreeze manifest classify every Feature 020 artifact plus bound `CONTEXT.md`, pin exact hashes, and close every A01-A19 predecessor-to-successor mapping? [Completeness, Design §3, Gap]
- [ ] CHK021 Is the amendment-set ID specified as a canonical domain-separated digest over sorted amendment/replaced-clause/replacement records, with no operator-selected label ambiguity? [Clarity, Design §3]
- [ ] CHK022 Does the detached attestation pin the manifest, design, context, amendment set, and API allowlist digests without claiming that the mutable in-tree attestation is its own trust anchor? [Traceability, Design §3]
- [ ] CHK023 Is the trusted signed append-only `RefreezeApprovalRecordV11` explicitly outside the mutable repository, bound to the exact target commit/tree plus detached-attestation digest and trusted release identity, with rejection requirements for a coordinated in-tree rewrite that retains an older record? [Security, Recovery, Design §3]
- [ ] CHK024 Does `contracts/public-api-v11.json` specify the exact keep/replace/remove set, supported target/cfg/feature domain, unknown-configuration refusal, generated graph cover, and the rule that later slices cannot expand the Interface without refreeze? [Completeness, Design §3]

## Cross-artifact consistency and activation proof

- [ ] CHK025 Are `Current`, internal retained generation, immutable work/failure state, and typed `SourceRefusal` defined consistently across spec, plan, data model, contracts, tasks, quickstart, and health semantics? [Consistency, Design §§3, 5]
- [ ] CHK026 Is the promotion predicate complete and closed across manifest, observations, derived artifacts, mutation epoch/permits, root authority, observer cut, racy-clean proof, capacity, and advertised scope, with no partial-success escape hatch? [Completeness, Design §§5-8]
- [ ] CHK027 Are successful multi-source/no-match requirements tied to one atomic selected-source receipt and an exact all-`Current` bijection, with unavailable members preserved as refusal evidence? [Consistency, Design §§5, 7]
- [ ] CHK028 Does the activation contract prove one process-wide mode, drain every legacy query/cache/CCR/retrieval/finalization registration, invalidate legacy state, retire raw embed/secondary writers, and make simultaneous V10/PreventiveV1 authority impossible? [Completeness, Design §11 Slice 4]
- [ ] CHK029 Is Slice 4 specified as indivisible across candidates, deltas, verification, capacity, provenance, embed/public-API migration, activation, and `ObservedRefreshGateV1`, with no shippable refusal-per-edit intermediate? [Consistency, Design §11 Slice 4]
- [ ] CHK030 Are V10 positive controls distinguished from future-seam acceptance specifications, including the target slice and the prohibition on reporting an unimplemented oracle as executed? [Traceability, Design §11 Slice 0]
- [ ] CHK031 Is V11 explicitly defined as the breaking embed/lifecycle boundary with consumer migration, restart, rollback, and live-V10-writer constraints? [Coverage, Design §§3, 11]
- [ ] CHK032 Are release criteria objectively frozen for causal oracles, models, all-target tests, memory, refresh convergence, delta equivalence, racy-clean detection, provenance round trips, activation races, API inventory, and external approval ancestry? [Acceptance Criteria, Design §13]
