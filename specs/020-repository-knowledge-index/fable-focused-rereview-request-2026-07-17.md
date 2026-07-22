# Fable Focused Re-review Request: Repository Knowledge Index

## Assignment

Perform a read-only Architect/Skeptic/Minimalist re-review of the corrected SymForge
feature-020 SpecKit. This is the freeze gate after the review in
`fable-adversarial-review-2026-07-17.md`; do not edit code, specifications, tasks, or
repository state.

Read `AGENTS.md` first. Never inspect, print, quote, or reproduce a secret value.
Security evidence must use safe `file:line` locations and synthetic pattern names.

## Inputs to read completely

1. `specs/020-repository-knowledge-index/fable-adversarial-review-2026-07-17.md`
2. `specs/020-repository-knowledge-index/spec.md`
3. `specs/020-repository-knowledge-index/plan.md`
4. `specs/020-repository-knowledge-index/research.md`
5. `specs/020-repository-knowledge-index/data-model.md`
6. `specs/020-repository-knowledge-index/tasks.md`
7. `specs/020-repository-knowledge-index/quickstart.md`
8. `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
9. `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`
10. `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
11. `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`
12. `specs/020-repository-knowledge-index/checklists/requirements.md`

Use the current source files cited by the original report to falsify the corrected
contracts. Do not treat this correction ledger or a passing mechanical check as proof.

## Required three lenses

- **Architect**: attack ownership, identity, generation fencing, commit boundaries,
  recovery, and cross-gate type placement.
- **Skeptic**: construct concrete race/crash/replacement/budget/cache sequences that
  would still permit stale, foreign, mixed-generation, or falsely current evidence.
- **Minimalist**: identify machinery that remains dead, duplicative, forward-dependent,
  or removable without weakening a user-visible invariant.

Each lens must independently inspect the publication/commit seam and curation replay
seam. Reconcile the lenses only after their independent conclusions are recorded.

## Correction ledger to verify, not assume

| Original finding | Intended correction | Primary artifacts |
|---|---|---|
| 1 | Curation intent and replay persist a strong `CurationSourceBinding`; same-path foreign repositories fail before ledger inspection or stored-success replay. | data model, authority contract, spec, tasks K-R13/K-R14, quickstart |
| 2 | One per-`ProjectInstance` publication writer lock is the commit boundary; each commit copies the current source map and replaces one source; P1-only swaps do not advance P0 generations. | plan, data model, spec FR-008, tasks L-R12/L-R13, quickstart |
| 3 | Background snapshot verification fences captured source identity plus base publication/content/project generations. | data model, spec FR-012, tasks E-R10, quickstart |
| 4 | A rejected stale temporal completion coalesces to one pending-latest recomputation per source with capped backoff. | data model, plan, tasks H-G08, quickstart |
| 5 | Edit-safety trust state is owned by `ControlStateDir`. | data model, source-binding contract, tasks B-R29, quickstart |
| 6 | Sidecar/daemon descriptors are namespaced by `ProjectId` and daemon/process instance; readers use the same namespace. | data model, source-binding contract, tasks B-R29, quickstart |
| 7 | `Unreadable`/`UnstableDuringRead` makes coverage Degraded, defeats equal-digest no-op, and retains bounded re-observation. | data model, spec FR-011, tasks D-R09, quickstart |
| 8 | A read larger than the total in-flight budget terminates as `HardSkip(PerFileCeiling)` before allocation. | data model, spec FR-005, tasks C-R07, quickstart |
| 9 | Durable apply is gated by an executable same-directory platform probe; Unix includes parent sync and Windows includes flushed temp plus write-through replacement or a documented tested equivalent. | authority contract, data model, plan, spec, tasks K-R15, quickstart |
| 10 and 16 | Dead verification-baseline types and fields were deleted; relevant-code-change/review-due signals carry the required behavior directly. | data model, authority contract, research, tasks, quickstart |
| 11 | Entry and metadata capacity failures have distinct typed reasons and never publish a partial manifest. | data model, spec FR-004, tasks B-R06/B-R24, quickstart |
| 12 | `HistoryLimit`/`HistoryCoverage` are Gate-E core types used by later temporal authority. | data model, plan, tasks |
| 13 | Canonical parse status is the closed `Parsed`/`PartialParse`/`Failed` enum; diagnostics remain operational only. | data model, spec FR-007, tasks B-R01/B-G01, quickstart |
| 14 | Finding/provenance IDs exclude record order, publication generation, and resolution state. | data model, authority contract, tasks I-R14 |
| 15 | Explicit history scope maps exactly to `HistoryOnly` and `Suppressed`. | data model, authority contract |
| 17 | Temporal completion fences both content generation and the exact captured source-version commit/tip. | data model, tasks H-R13/H-G08, quickstart |
| 18 | `KnowledgeReviewSourceResult` carries `source_version`. | data model, authority contract, quickstart |
| 19 | `CodeEvidenceDisplay` has one explicit normative precedence, mirrored by enum order. | data model |
| 20 | Document authority remains an independent filter/label and no contract mandates an undeclared ranking factor. | authority and search contracts, tasks I-R05 |
| 21 | `get_file_content` performs freshness/generation capture before its generation-aware repeat-cache lookup. | mental-model contract, tasks I-R15, quickstart |
| 22 | Compact CCR footers name a `symforge` facade retrieval intent and hash, not an unavailable fourth tool. | search contract, tasks I-R09, quickstart |
| 23 | Object ID alone shares raw bytes only; parse/extraction/secret results include classification/route and versioned policy inputs, while source-derived state is rebuilt per source. | data model, research, tasks L-R14/L-G03, quickstart |
| 24 | Hash-valid suppressions and proven divergence receive reserved derivation priority; if still unrepresentable, affected units fail closed outside default/current voice with explicit skipped IDs/coverage. | authority contract, data model, spec, tasks H-R12, quickstart |

For Finding 21, correct the original scenario while preserving the underlying defect
test: current source checks the generation-blind repeat cache before freshening the
target. The risk is suppression of the required current reread, not direct replay of
cached file bytes. Verify that the new contract and red oracle close that actual seam.

The LOW findings 25-36 were also addressed: process-global operator/onboarding
semantics are explicit without legacy merging; placement changes only with a new
`ProjectInstance`; breakers are source/lane/stage scoped; degraded repair re-triggers;
compile-fail is only the initial RED for absent types; the offline assertion names a
process-spawn spy; the corpus threshold is numeric; dead enum variants were deleted;
response identity is derived from captured bundles; and line ranges are one-based and
half-open. Report any regression, but HIGH/MEDIUM closure is the freeze decision.

## Mandatory attack sequences

1. Race continuous P1 source-set swaps against a long P0 build and a watcher commit.
2. Pause background verification, publish a newer watcher generation, then resume.
3. Move a Git commit/ref without changing tracked bytes while temporal work is active.
4. Replace a repository at the same canonical path between curation reservation,
   pending intent recovery, and same-key replay.
5. Exhaust policy-entry/authority-record budgets beyond a hash-valid suppression.
6. Deep-read, publish a watcher edit, and repeat `get_file_content` in one session.
7. Reuse one object ID under paths with different classification/extraction routes.
8. Fail each durability step and prove no idempotency reservation or partial ledger is
   created when the platform contract is unavailable.

## Evidence already available

Mechanical checks currently report 75 unique requirement/success definitions, 306
unique task definitions, all 13 gates exactly once, zero broken local links across 17
Markdown files, 73 targeted cross-artifact consistency assertions passing, and clean
`git diff --check`. Re-run or independently falsify these; they are not substitutes for
the state-transition review.

## Required output

Return Markdown only and write a new report; do not overwrite the original review.

```text
# Fable Focused Re-review

## Verdict
PASS | PASS WITH CHANGES | FAIL

## Original finding closure
| Finding | CLOSED | OPEN | REGRESSION | Evidence and reasoning |

## New findings
1. [HIGH|MEDIUM|LOW] Short title
   - Evidence: exact file:line references
   - Failure scenario: concrete runtime/user-visible sequence
   - Violated invariant: exact requirement or design rule
   - Smallest correction: precise specification/task change

## Lens conclusions
### Architect
### Skeptic
### Minimalist

## Missing tests
- Exact red-test name and must-fail behavior

## Freeze decision
- READY TO FREEZE only if no unrefuted HIGH or MEDIUM remains.
```

Do not report style preferences. Do not accept a claim merely because a test name or
MUST sentence exists; trace whether the modeled state transition is coherent and
implementable against the current source.
