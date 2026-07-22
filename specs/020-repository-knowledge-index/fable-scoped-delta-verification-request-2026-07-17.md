# Fable Scoped Delta Verification Request: Repository Knowledge Index

## Assignment

Perform one independent, read-only delta verification of the five MEDIUM corrections
authorized by `fable-focused-rereview-2026-07-17.md`. This is the final SpecKit freeze
gate. Do not edit code, specifications, tasks, or repository state.

Read `AGENTS.md` first. Never inspect, print, quote, or reproduce a secret value.
Security evidence must use safe `file:line` locations and synthetic pattern names.

Write the result to:

`specs/020-repository-knowledge-index/fable-scoped-delta-verification-2026-07-17.md`

## Authority and scope

Read these inputs completely:

1. `specs/020-repository-knowledge-index/fable-focused-rereview-2026-07-17.md`
2. `specs/020-repository-knowledge-index/spec.md`
3. `specs/020-repository-knowledge-index/plan.md`
4. `specs/020-repository-knowledge-index/data-model.md`
5. `specs/020-repository-knowledge-index/tasks.md`
6. `specs/020-repository-knowledge-index/quickstart.md`
7. `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
8. `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`
9. `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`
10. `specs/020-repository-knowledge-index/checklists/requirements.md`

Use `research.md` only to verify LOW finding 6 if needed. Inspect the current source
seams cited by the focused report when necessary to determine whether a corrected
contract is implementable. Do not rerun the broad 24-finding campaign or reopen a
previously CLOSED finding unless one of these delta edits regressed it.

The focused report explicitly permits this scoped pass because it found no HIGH. A
passing mechanical check is evidence of document integrity, not proof that a state
transition is coherent.

## Five mandatory correction checks

### 1. Curation identity and continuity

Adjudicate focused finding 1 against FR-037, `CurationSourceBinding`, authority-hygiene
step 2 and recovery, quickstart curation cases, and K-R02/K-R13/K-R14/K-R18/K-R19/K-R20.

Verify all of the following:

- source sameness is identity-not-moving-state: exact `RepositoryId`/`SourceId` plus
  an explicit continuity proof;
- Git continuity accepts an ordinary commit or branch switch when the stored anchor
  commit remains resolvable in the live object database, while an unrelated same-path
  clone or dropped-history rewrite fails closed;
- non-Git continuity uses stable root-object identity plus durable catalog lineage,
  not equality of a digest that changes with ordinary file edits;
- manifest and policy digests are first-execution freshness guards, not fields in the
  source-binding equality predicate;
- same-key/same-hash replay returns the stored terminal result before now-stale
  freshness guards, including immediately after apply and after an intervening commit;
- crash recovery after the post-image exists terminalizes for the same source without
  reopening foreign-source writes.

Falsify this correction with the focused report's exact ordinary-commit, branch-switch,
same-path-clone, dropped-history, immediate-replay, and post-image recovery sequences.

### 2. Closed trust-store ownership

Adjudicate focused finding 2 against FR-050, the Phase-1 state-routing matrix in
`plan.md`, the data model, source-binding contract, B-R29, and quickstart step 16.

Verify that the edit-safety trust store is explicitly and consistently owned by
`ControlStateDir`; no normative closed/exhaustive matrix permits it to self-resolve or
fall under `ProjectStateDir`.

### 3. Representable fail-closed suppression

Adjudicate focused finding 3 across FR-033/FR-039, the authority-hygiene contract,
`KnowledgeAuthorityView`, H-R12, I-R10, and the quickstart authority cases.

Verify that a hash-valid suppression dropped by the reserved derivation budget:

- has voice `Suppressed`, never `NeedsReview`;
- is absent from default and current scopes but retrievable through history/all;
- exposes canonical skipped-suppression IDs and truncated coverage;
- remains consistent with the closed voice-to-scope projection and deterministic
  scope-set tests.

### 4. Temporal marker, acceptance, and republication coherence

Adjudicate focused finding 4 against FR-031, the publication rule in `plan.md`,
`PublishedGeneration`/`CodeSignalsSnapshot`, the mental-model contract, H-R09/H-R13/
H-R15/H-G08, and the quickstart temporal sequence.

Verify all of the following:

- every job and pending-latest marker captures live content generation plus the exact
  live commit/tip at scheduling;
- acceptance requires the completion's analyzed target to equal both that marker and
  the current live target;
- accepted derived-only republication advances publication generation, preserves
  content generation and manifest/content digests, and carries the accepted tip
  coherently through the bundle source version, manifest metadata, temporal snapshot,
  and response envelope;
- a bytes-identical commit/ref-tip change converges within bounded attempts rather
  than deadlocking or returning temporal evidence for a different reported commit.

Confirm that keeping the manifest digest unchanged is intentional and implementable:
the canonical digest definition excludes captured `SourceVersion`.

### 5. Durability-probe ordering and complete destination coverage

Adjudicate focused finding 5 and LOW finding 9 against FR-037, the capability model,
authority-hygiene durability steps, K-R05/K-R15/K-R16/K-R17, SC-019, and quickstart.

Verify all of the following:

- every non-probe apply requirement is evaluated first;
- protected, explicit-protected, read-only, ref, and memory-only sources return their
  typed unavailable reason with zero probe I/O anywhere beneath the source root;
- first apply probes both the ledger parent and the `ProjectStateDir` replay/intent
  journal parent, deduplicating only when they resolve to the same directory;
- either destination's failure returns typed Unavailable before idempotency reservation
  or partial ledger/journal state;
- the filesystem spy observes the entire protected root, not only `.symforge`.

## LOW cleanup and red-oracle audit

Verify the focused report's LOW findings 6-10:

1. no deleted baseline-state terminology remains in canonical authority artifacts;
2. dead `EntryBudgetExceeded`/`MetadataBudgetExceeded` `ScoutIssueKind` variants are
   absent, with capacity represented by typed freshness/health outcomes;
3. unused manifest/policy digest fields are absent from `CurationSourceBinding`;
4. the intent/replay journal directory participates in the durability gate;
5. `project_generation` has one explicit reset/rebind advance rule and remains stable
   across same-project P0/P1 publication churn.

Confirm these nine named red-test obligations exist exactly once and state a concrete
must-fail oracle:

- `cold_start_budget_exhaustion_yields_distinct_typed_capacity_reasons`
- `parse_status_is_bounded_and_digest_stable`
- `budget_dropped_suppression_state_is_representable_and_scope_consistent`
- `bytes_identical_commit_temporal_recompute_converges`
- `durability_probe_writes_nothing_into_non_available_sources`
- `intent_journal_directory_durability_gates_apply`
- `curation_replay_after_intervening_commit_is_not_foreign`
- `identical_replay_immediately_after_apply_matches_stored_binding`
- `curation_recovery_after_intervening_commit_terminalizes_post_image`

## Local mechanical evidence to independently falsify

The correction campaign currently reports:

- 75 unique requirement/success-criterion definitions;
- 314 unique task IDs;
- Gates A-M exactly once;
- all nine named red-test obligations exactly once;
- zero broken local Markdown links;
- 12 focused correction assertions passing;
- clean `git diff --check` apart from existing line-ending warnings.

Re-measure or falsify these claims, but do not substitute them for the five semantic
checks above.

## Required output

Return Markdown only and save a new report at the required path. Use this structure:

```text
# Fable Scoped Delta Verification

## Verdict
PASS | PASS WITH CHANGES | FAIL

## Five-correction adjudication
| Finding | SUSTAINED | REFUTED | REGRESSION | Evidence and reasoning |

## LOW cleanup and red-oracle audit
- Findings 6-10
- Nine named tests
- Mechanical measurements

## New in-scope findings
1. [HIGH|MEDIUM|LOW] Short title
   - Evidence: exact file:line references
   - Failure scenario: concrete runtime/user-visible sequence
   - Violated invariant: exact requirement or design rule
   - Smallest correction: precise specification/task change

## Freeze decision
- READY TO FREEZE only if no unrefuted HIGH or MEDIUM remains.
- Otherwise: NOT READY TO FREEZE, with the exact blocker list.
```

Do not report style preferences. Do not accept a claim because a MUST sentence or test
name exists; trace the modeled transition end to end. If all five corrections hold and
no new HIGH/MEDIUM exists, state **READY TO FREEZE** explicitly.
