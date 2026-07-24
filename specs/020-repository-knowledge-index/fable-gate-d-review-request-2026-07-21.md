# Fable Gate D Scoped Review Request

Date: 2026-07-21

Branch: `feat/repository-knowledge-index`

Scope: SpecKit 020 Gate D only — watcher, reconciliation, and canonical manifest state

## Review objective

Adversarially verify that Gate D satisfies the frozen contracts in `spec.md`,
`plan.md`, `data-model.md`, and Gate D of `tasks.md`. This is a read-only review.
Do not modify production code or the frozen specification artifacts.

For every suspected finding, first try to refute it against the implementation,
the named behavioral oracle, and the complete serial verification receipt. Report
only sustained findings. Distinguish a contract defect from a later-gate omission.
Gate E owns snapshot fidelity and one-Arc publication; Gate F onward owns knowledge
extraction and retrieval.

## Required lenses

1. Generation and publication fencing under watcher/reconcile races.
2. Complete-manifest convergence, uncertainty retry, and bounded recovery.
3. Disposition authority: no stored `skipped_files` or independent skip state.
4. Removal and promotion symmetry across indexed bytes, derived indices, scout
   state, manifest entries, coverage, and compatibility projections.
5. Atomic-save/rename handling, transient access failures, and last-valid bytes.
6. Minimality: identify any unnecessary state or duplicate source of truth added
   by Gate D.

## Primary implementation seams

- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/live_index/health_view.rs`
- `src/live_index/persist.rs`
- `src/live_index/search.rs`
- `src/watcher/mod.rs`
- `src/protocol/tools.rs`
- `src/protocol/format/tests.rs`
- `src/protocol/investigation.rs`
- `src/protocol/prompts.rs`
- `src/protocol/resources.rs`
- `src/sidecar/handlers.rs`
- `src/sidecar/mod.rs`
- `tests/admission_acceptance.rs`
- `tests/impact_admission.rs`
- `tests/watcher_integration.rs`
- `tests/watcher_layer3_restat.rs`
- `tests/watcher_reload_cancellation.rs`

## Claims to falsify

- D-R01 through D-R10 and D-G01 through D-G09 are all closed exactly as checked
  in `tasks.md`; D-V01 through D-V03 have behavioral evidence.
- A watcher update observes through the same metadata-first admission and bounded
  stable-read policy as bulk load, then publishes all affected lanes once under
  project and publication fences.
- Reconciliation compares complete manifests, rebases paths changed after its
  off-lock build, and never overwrites a newer watcher publication.
- Equal Complete manifests are true no-ops. Degraded coverage always receives a
  bounded retry window, and a later uncertainty signal opens a new bounded window.
- `manifest_entries` is the sole disposition authority. Tier counts, admission
  lookup, and legacy skip output are projections; normalized-path upsert prevents
  duplicate catalog identities.
- Direct single-file update/removal cannot split indexed bytes from the canonical
  manifest. Transient unreadable/unstable observation preserves last-valid bytes
  while exposing a Degraded metadata-only disposition until recovery.
- Atomic-save batches coalesce each normalized path once and prioritize the live
  destination over vanished temporary hints, avoiding serial NotFound delays and
  stale-symbol retention.

## Verification evidence

- Store units: 74 passed.
- Watcher units: 53 passed.
- Query units: 146 passed.
- Health-view units: 4 passed.
- Admission acceptance: 5 passed.
- Impact admission: 3 passed.
- Layer-3 restat: 3 passed.
- Reload cancellation: 3 passed.
- `watcher_integration`: three consecutive focused runs, each 10 passed / 1 ignored.
- `cargo fmt --all -- --check`: exit 0.
- `cargo check -j 1 --tests`: exit 0 in 68.489s.
- Authoritative `cargo test -j 1 --all-targets -- --test-threads=1`: exit 0 in
  930.497s, Terminal Commander job
  `job_019f84910ba37c53921fa1b3672a3e19`.

The full gate initially exposed stale synthetic-manifest fixtures, unsynchronized
direct update/removal, catalog-path duplicate identities, and over-broad fixture
mtime pinning. Each was reproduced with a focused RED and corrected before the
final all-target receipt.

## Explicit non-findings / later work

- `src/protocol/tools.rs` is honestly classified by current SymForge as Tier 2
  metadata-only because of its size; `analyze_file_impact` therefore returns a
  typed limitation. Exact source reads and compiled behavioral tests are the
  evidence for that file.
- SF-AAP-001/002/003 are separately recorded release blockers and are not claimed
  as fixed by Gate D.
- Do not require Gate E snapshot/publication-bundle fields or Gate F-M knowledge
  behavior from this review unless Gate D made their later implementation
  impossible or contradicted the frozen contract.

## Requested output

Write the review to:

`specs/020-repository-knowledge-index/fable-gate-d-review-2026-07-21.md`

Return one of: `PASS`, `PASS WITH CHANGES`, or `CONTESTED`. For every sustained
finding include severity, exact file/line evidence, the violated frozen contract,
a falsification sequence, and the smallest correction. End with an explicit
decision: `GATE D SUSTAINED` or `GATE D REOPENED`.
