# Phase 0 Research: Preventive Lifecycle Activation Cut

**Date**: 2026-08-18 | **Tree verified at**: `main` = `81dc7d67` (worktree
`symforge-slice4`, branch `feature-020-slice-4-candidates`)

Every row below was verified against the live tree during planning, not copied
from prior documents. One prior-document correction was found (R3).

## R1 — Benchmark baseline `1521abb0` is reachable

- **Decision**: T070's baseline commit `1521abb0` is usable as-is.
- **Evidence**: `git cat-file -t 1521abb0` → `commit`.
- **Alternatives considered**: re-anchoring per frozen tasks text — not needed.

## R2 — Dark-machinery inventory (campaign table re-verified)

- **Decision**: Wave 1 builds exactly six new modules; twelve existing modules
  are adapted, not rebuilt.
- **Evidence**: `src/index_lifecycle/` contains `adapters.rs`, `authority.rs`,
  `capacity.rs`, `embedded.rs`, `mod.rs`, `mutation.rs`, `physical_root.rs`,
  `process_runtime.rs`, `public_api.rs`, `registry.rs`, `runtime.rs`,
  `transition.rs`. Missing (to be created): `supervisor.rs` (T059),
  `candidate.rs` (T060), `observer.rs` (T061), `verification.rs` (T062),
  `query.rs` (T063), `activation.rs` (T066).
- **Test files**: `tests/activation_cut_v11.rs` and
  `tests/process_capacity_pool_v11.rs` exist; the five Wave 1 RED files do not
  yet exist (correct — they are the work).

## R3 — Correction to the campaign doc's inventory

- **Finding**: `docs/reviews/FEATURE-020-SLICE4-CAMPAIGN-v11.md` lists
  "`claim_provenance.rs` sealed types" as a skeleton file in
  `src/index_lifecycle/`. **No such file exists.** The sealed
  claim-provenance types live in `src/index_lifecycle/public_api.rs`,
  `src/lifecycle_identity.rs`, and `src/protocol/format.rs`.
- **Impact**: none on scope — the types exist and are production-unreachable
  as described; only the file attribution was wrong. Do not create a
  `claim_provenance.rs`; extend the real locations.

## R4 — Frozen oracle registry, Slice 4 rows (authoritative RED names)

Extracted verbatim from
`specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md`.
These names are contract identifiers; RED tests MUST use them exactly.

| Oracle ID | Target | Owner |
|---|---|---|
| TEST-CANDIDATE | `tests/index_candidate_lifecycle_v11.rs::closed_candidate_promotion_matrix` | T053 |
| TEST-OPAQUE-PATH | `tests/index_candidate_lifecycle_v11.rs::opaque_non_utf8_path_identity_is_lossless` | T053 |
| TEST-OBSERVER | `tests/observer_handoff_v11.rs::stable_token_cut_latches_every_gap` | T054 |
| TEST-ROLLING-VERIFICATION | `tests/rolling_verification_v11.rs::rolling_passes_are_fair_resumable_and_fenced` | T055 |
| TEST-QUERY | `tests/project_query_lease_v11.rs::strict_selection_is_atomic_and_complete` | T056 |
| TEST-HEALTH | `tests/project_query_lease_v11.rs::committed_generation_and_attempt_health_are_separate` | T056 |
| TEST-SNAPSHOT | `tests/snapshot_v11_migration.rs::snapshot_seed_requires_complete_current_proof` | T057 |
| TEST-ACTIVATION | `tests/activation_cut_v11.rs::preventive_v1_is_the_only_live_mode` | T058 |
| TEST-EMBED | `tests/activation_cut_v11.rs::embedded_source_has_one_handle_and_no_raw_bypass` | T058 |
| TEST-MUTATION | `tests/activation_cut_v11.rs::every_source_write_requires_current_mutation_permit` | T058 |
| TEST-STATE | `tests/activation_cut_v11.rs::state_owners_and_team_artifact_are_exact` | T058 |
| TEST-CAPACITY-INTEGRATION | `tests/process_capacity_pool_v11.rs::whole_runtime_capacity_is_conserved_under_activation` | T069 |
| TEST-DELTA | `tests/delta_full_rebuild_equivalence_v11.rs::every_edit_matches_clean_full_rebuild` | T071 |
| TEST-KNOWLEDGE | `tests/delta_full_rebuild_equivalence_v11.rs::knowledge_artifacts_match_clean_full_rebuild` | T071 |
| TEST-PERFORMANCE | `benches/observed_refresh_gate_v1.rs::observed_refresh_gate_v1` (registration `criterion_group:observed_refresh_gate_v1_group->observed_refresh_gate_v1`) | T068 |

## R5 — T058 stand-ins confirmed

- **Evidence**: the four `#[ignore]` stand-ins exist at
  `tests/activation_cut_v11.rs:2120–2154`, each with an ignore reason naming
  its oracle ID and "remove this attribute in Slice 4 (T058)". Their names
  already match R4's TEST-ACTIVATION/EMBED/MUTATION/STATE exactly.

## R6 — Benchmark infrastructure does not exist yet

- **Decision**: T068 adds `criterion` as a dev-dependency plus a `[[bench]]`
  target named `observed_refresh_gate_v1` with
  `criterion_group` = `observed_refresh_gate_v1_group`.
- **Evidence**: `Cargo.toml` contains no `criterion` and no `[[bench]]`.
- **Rationale**: the frozen registration string mandates criterion's group
  registration shape; this is a contract-required dependency, not a
  convenience.
- **Alternatives considered**: hand-rolled `#[bench]`/harness=false timing —
  rejected: the frozen registration string names `criterion_group`.

## R7 — Delivery workflow (from spec clarifications, session 2026-08-18)

- Wave 1 = five PRs, one per RED+machinery pair; auto-merge on green CI +
  clean review (one pass, mandatory cfg-lens sweep).
- Wave 2 = one activation-cut PR; full multi-round adversarial review; merges
  only on explicit operator approval.
- Standing build rules (binding, from repo CLAUDE.md): cargo runs > 10 min go
  through Terminal Commander, one cargo at a time, `--test-threads=1`;
  run `cargo test --no-default-features --features embed --lib` before every
  push that touches `#[cfg(test)]` helpers; `cargo clean` after heavy
  sessions; never hand-recompute the source seal without validating the
  recompute against the Rust oracle on a known input.
