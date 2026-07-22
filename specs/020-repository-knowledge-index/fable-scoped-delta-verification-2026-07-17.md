# Fable Scoped Delta Verification

Final freeze-gate pass per `fable-scoped-delta-verification-request-2026-07-17.md`.
Execution: the five semantic corrections were adjudicated directly by the lead
reviewer against the falsification sequences authored in
`fable-focused-rereview-2026-07-17.md` (ordinary-commit, branch-switch,
same-path-clone, dropped-history, immediate-replay, post-image-recovery,
budget-dropped-suppression, bytes-identical-commit, probe-ordering); an independent
audit pass verified LOW findings 6-10, the nine named red-test obligations, and
re-measured every mechanical claim. Read-only throughout; no repository state
modified other than this report, as instructed. No previously CLOSED finding was
reopened; no delta edit regressed one.

## Verdict

**PASS.**

## Five-correction adjudication

| Finding | Status | Evidence and reasoning |
|---|---|---|
| 1 — Curation identity and continuity | **SUSTAINED** | The binding is now identity-not-moving-state: `CurationSourceBinding { repository_id, source_id, continuity: CurationContinuityProof }` (data-model.md:1525-1540) with the digest fields removed. The predicate is explicit at every layer: "source sameness requires matching `RepositoryId`/`SourceId` plus the continuity predicate below, never equality of moving ref/tip/history, manifest, or policy state" (knowledge-authority-hygiene.md:216-222); "Current tip/ref/history movement alone is drift, never foreign identity" (data-model.md:1565); FR-037 (spec.md:602-613) carries the full predicate normatively, including "Guarded manifest/policy digests … MUST NOT participate in source-sameness comparison or block same-key/same-hash terminal replay". Falsification sequences: ordinary commit — anchor tip remains resolvable, passes; branch switch — `selected_ref_or_head` is not in the proof (`CurationContinuityProof::Git { object_format, anchor_tip_object_id }`), passes; unrelated same-path clone — anchor tip absent from the foreign object database, fails closed with typed `foreign_source_conflict`, quarantine, zero writes (hygiene:255-263); dropped-history rewrite — unresolvable anchor, conservatively fails closed; immediate replay after apply — digests excluded from the comparison, stored terminal result returned (hygiene:220-222, 247); post-image recovery after an intervening commit — continuity passes before ledger inspection, post-image finalizes (hygiene:255-267). Non-Git continuity is lineage-based, not digest equality (data-model.md:1563-1564) — see New finding 1 for a definitional residual. K-R18/K-R19/K-R20 (tasks.md:602-609) pin all three regression scenarios concretely. |
| 2 — Closed trust-store ownership | **SUSTAINED** | The edit-safety trust store is now explicitly owned by `ControlStateDir` in all six artifacts, including the two that were missing it: FR-050's owner sentence (spec.md:728), plan.md Phase-1 routing (plan.md:251), data-model.md:290, source-binding-and-state.md:119, B-R29 (tasks.md:60), quickstart step 16 (quickstart.md:92). No normative matrix permits self-resolution or `ProjectStateDir` placement; the "closed and exhaustive" claim is now true against its own inventory. |
| 3 — Representable fail-closed suppression | **SUSTAINED** | The fail-closed state is now `Suppressed`, never `NeedsReview`, and is encoded in the data model itself: "affected units fail closed to `Suppressed`, not `NeedsReview`; canonical `skipped_suppression_ids` plus `Truncated` coverage distinguish that unrepresentable state" (data-model.md:1348-1349), with `skipped_suppression_ids` a field whose non-emptiness marks exactly that state (data-model.md:1156, 1165). FR-039 (spec.md:637-641): reserved priority; out of default/current; retrievable through history/all; canonical skipped IDs + truncated coverage. This composes with the closed voice→scope projection (history is voice-based, exactly {`HistoryOnly`, `Suppressed`}, data-model.md:1331-1333) — the contradiction with FR-033's default definition is gone. hygiene:327 and quickstart:408 agree. H-R12 (tasks.md:383-386) asserts scope membership, not just a label. |
| 4 — Temporal marker, acceptance, republication coherence | **SUSTAINED** | One rule, stated identically in FR-031 (spec.md:568-574) and data-model.md:1284-1292: every scheduled job and coalesced pending-latest marker captures the live content generation plus exact live commit/tip at scheduling; a completion is accepted only when its analyzed target equals both that marker and the current live target; accepted derived-only republication advances publication generation only and carries the accepted commit/tip "consistently by the bundle, its manifest, `CodeSignalsSnapshot`, and response envelope". The three-divergent-targets ambiguity is eliminated. Keeping the manifest digest unchanged is intentional and implementable: the canonical digest exclusion list still excludes "captured source version, which is carried and verified separately" (data-model.md:716). Convergence: at most one running worker + one pending-latest marker per source with capped backoff (data-model.md:1292-1296); H-R15 (tasks.md:391-393) pins bounded acceptance AND envelope/temporal commit coherence, failing under both pre-correction readings. |
| 5 — Durability-probe ordering and destination coverage | **SUSTAINED** | Ordering is explicit: apply step 3 "first evaluates normal-current-worktree, writable-source, and durable replay/intent requirements; an unavailable requirement returns its typed reason with no durability-probe I/O. It then runs the required probes in both durable-record directories" (hygiene:223-229); FR-037: "Only after normal-current-worktree, writable-source, and durable replay/intent requirements are `Available` may first apply … probe each directory receiving durable curation records: the ledger parent and the `ProjectStateDir` replay/intent-journal parent. Either failed probe makes apply unavailable before reservation. Explicit-protected, read-only, ref, implicit-worktree, and memory-only bindings MUST return their typed reason with zero probe file operations anywhere under the source root" (spec.md:622-630). Both destinations covered, deduplicated when identical (hygiene:280-282) — this also closes prior LOW finding 9. K-R16 (whole-root spy, five binding fixtures, tasks.md:596-598) and K-R17 (journal-parent failure gates apply before reservation, tasks.md:599-601) are concrete; contract test 28 runs probes last in both directories (hygiene:371-374). |

## LOW cleanup and red-oracle audit

**Findings 6-10 — all CLOSED** (independent audit, evidence spot-confirmed by lead):

- 6: zero canonical hits for the deleted baseline identifiers; the hygiene axis list (knowledge-authority-hygiene.md:35-38) now matches `CodeEvidenceSummary`'s eight retained sets 1:1 (data-model.md:955-967), with "suspected conflicts" added; research.md:237-239 dropped "baseline state". Remaining `baseline` word-hits are pre-existing different senses (SC-006 token baseline, Gate-A performance baseline, rejected Gitleaks suppression baselines) — not the deleted machinery.
- 7: `ScoutIssueKind` carries no budget variants (data-model.md:628-634); capacity is typed `FreshnessReason::{CatalogEntryCapacityExceeded, CatalogMetadataCapacityExceeded}` (data-model.md:610-611) with the explicit no-manifest rule (data-model.md:645-648).
- 8: `CurationSourceBinding` carries no digest fields (data-model.md:1536-1540); freshness guards live only in `CurateKnowledgeInput` and first-execution validation.
- 9: both durable-record directories participate in the gate (hygiene:280-282, FR-037 spec.md:624-628, K-R17, K-G03, contract test 28).
- 10: `project_generation` has one definition site — "the owning `ProjectInstance` epoch: it advances only when that instance is reset or rebound and remains stable across same-project P0 content/derived publications and all P1 registry churn" (data-model.md:1271-1273); all other uses are consumers, not competing rules.

**Nine named red-test obligations** — each exists exactly once with a concrete must-fail oracle: B-R31, B-R32 (Gate B); H-R12, H-R15 (Gate H); K-R16, K-R17, K-R18, K-R19, K-R20 (Gate K). Gate placement respects type dependencies. Two implementation-realism notes (no task-text change needed): B-R32's fixture must actually vary the operational diagnostic text (a no-op rewording would pass vacuously), and K-R16's filesystem spy must wrap the real first-apply probe path, not a stubbed probe result — the same class of caveat the focused report recorded for K-R15.

**Mechanical measurements** — all claims independently re-measured and matched: 75 unique FR+SC definitions (52 FR + 23 SC; 8 NFRs outside the count), 314 unique task IDs (delta of +8 over the prior 306 equals exactly the new obligations), gates A-M each exactly once, the nine test names each exactly once, 0 broken local links (noting only 2 local links exist; the artifacts cross-reference by prose citation). Clean.

## New in-scope findings

1. **[LOW] The non-Git continuity arm's terms are used but never defined: `root_object_identity` has no derivation rule and the "unbroken durable catalog lineage" has no named recorder or storage location**
   - Evidence: `CurationContinuityProof::NonGit { root_object_identity, catalog_identity_digest }` (data-model.md:1530-1533); "a non-Git proof requires unchanged platform root-object identity and an unbroken durable catalog lineage from the recorded digest to the current one" (data-model.md:1563-1564; hygiene:258-260; spec.md:605-606; quickstart:460) — no artifact defines how `root_object_identity` is derived (platform volume/file identity of the canonical root?) or which component durably records the catalog-digest lineage, where (the natural home is the `ProjectStateDir` replay store), and at which points (each publication? each apply?). The Git arm is fully specified by contrast.
   - Failure scenario: an implementer who never records lineage (nothing tasks it) makes every non-Git recovery/replay fail the continuity proof after any ordinary edit — typed `foreign_source_conflict` on the user's own repository, the K-R18/K-R02 regression class confined to non-Git sources. Fail direction is safe (no writes), so the impact is availability and mislabeling, not safety.
   - Violated invariant: the delta-request's own check-1 bullet ("non-Git continuity uses stable root-object identity plus durable catalog lineage, not equality of a digest that changes with ordinary file edits") — mandated outcome specified, mechanism unnamed.
   - Smallest correction: two sentences in data-model (echoed in hygiene): `root_object_identity` is the bounded platform file-identity encoding of the canonical source root (the `PlatformFileId` mechanism, used here as continuity evidence, not logical identity); the catalog lineage is recorded in the `ProjectStateDir` replay store as an appended digest-chain entry at each successful publication and apply, and an absent chain fails closed. Optionally extend K-R18 with a non-Git variant.

## Freeze decision

**READY TO FREEZE.**

All five MEDIUM corrections are SUSTAINED with none refuted and none regressed; LOW
findings 6-10 are CLOSED; the nine red-test obligations exist exactly once with
concrete oracles; every mechanical claim was independently re-measured and matched.
No unrefuted HIGH or MEDIUM remains anywhere in the delta scope. The single new
finding is LOW (a definitional gap in the non-Git continuity arm whose failure
direction is safe); it does not block freeze under the stated rule and can land with
the two fixture-realism notes at the campaign's discretion — before or during Gate B,
since it touches no frozen invariant.

Per the focused report's condition, this delta pass satisfies the re-review
requirement: the SpecKit may freeze and Gate A may close. Implementation (Gate B
onward) proceeds under the standing rule that any contradiction discovered between a
frozen contract and code reality stops work for re-planning rather than silent
requirement mutation.
