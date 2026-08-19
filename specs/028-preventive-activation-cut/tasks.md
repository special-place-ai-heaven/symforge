# Tasks: Preventive Lifecycle Activation Cut (Feature 020 Slice 4)

**Input**: Design documents from `specs/028-preventive-activation-cut/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: RED-first tests are MANDATORY here (spec FR-003; frozen tasks.md:836–838),
using the exact frozen oracle names from research.md R4. Every "(020:TNNN)" tag
maps a task to its frozen Feature 020 roster owner — that roster is the
normative source; these tasks are its execution decomposition.

**Organization**: user-story phases are the five Wave 1 dark pairs (spec
clarifications: one PR per pair, auto-merge on green CI + clean review). The
indivisible Wave 2 cut and closure are integration phases — they serve all five
stories at once and carry no story labels. **Story acceptance is two-stage by
design**: each story's oracles go GREEN dark in its Wave 1 phase; live
end-to-end acceptance (quickstart spot-checks) completes only at the cut.
That is the frozen "Slice 4 is one enablement unit" constraint, not a gap.

## Format: `[ID] [P?] [Story] Description`

## Path Conventions

Single Rust crate at repo root: `src/`, `tests/`, `benches/` (new). Work
happens in the `symforge-slice4` worktree; pair branches fork from
current `main` after the previous pair merges (seal/census files serialize).

---

## Phase 1: Setup

**Purpose**: land the execution-spec docs so every pair PR can reference them

- [x] T001 Commit `specs/028-preventive-activation-cut/*` + the CLAUDE.md plan-pointer update on `feature-020-slice-4-candidates`, open a docs-only PR to `main`, and on green CI squash-merge with explicit `--subject "docs(feature-020): Slice 4 execution spec (speckit 028) (#N)" --body "..."` per the repo squash rules

---

## Phase 2: Foundational

**No foundational tasks.** Slices 0–3 are the foundation and are complete:
the darkness seal, retirement census, contract chain, and twelve hardened
`src/index_lifecycle/` modules are live on `main` (research.md R2). Each pair
extends them; nothing blocks before Phase 3.

---

## Phase 3: User Story 1 — Safety: candidate pipeline (Priority: P1) 🎯 MVP — Wave 1 pair 1

**Goal**: the closed promotion matrix and lossless opaque-path identity exist
as dark machinery (020:T053 + T059 + T060)

**Independent Test**: `cargo test --test index_candidate_lifecycle_v11 -- --test-threads=1`
GREEN; darkness seal proves the new modules unreachable from production.

- [x] T002 [US1] Branch `feature-020-s4-p1-candidate` from `main`; author RED `tests/index_candidate_lifecycle_v11.rs` with frozen names `closed_candidate_promotion_matrix` and `opaque_non_utf8_path_identity_is_lossless` plus the full 020:T053 case list (isolated build, publish-before-prune, retry supersession, all seven matrix outcomes, metadata-terminal exclusions, certificate-cannot-authorize-partial, failed/panicked discard); run only this file and record the observed RED output in the PR description (020:T053)
- [x] T003 [US1] Implement `src/index_lifecycle/supervisor.rs` — loader ownership, cancellation, attempt accounting, classified failure, retry triggers moved from the existing loader seams (020:T059)
- [x] T004 [US1] Implement `src/index_lifecycle/candidate.rs` — capacity-reserved isolated full/delta candidates, complete artifact certificates, one runtime-store commit point, `CatalogPath` native/opaque identity end-to-end, delta exact-validation of only the changed source token, no-allocation whole-root patch, same-source drift retry/abort, epochs-never-authorize; register both modules in `src/index_lifecycle/mod.rs` (020:T060)
- [x] T005 [US1] Extend `tests/preventive_runtime_dark_v11.rs` (constructor unreachability + census) to cover supervisor+candidate; refresh `FULL_SOURCE_PIN_V1` via the Rust oracle only; reconcile the contract chain if census categories shift (quickstart step 3)
- [x] T006 [US1] Run the full gate battery serially via Terminal Commander (fmt, clippy -D warnings, all-targets serial suite, `--no-default-features --features embed --lib`, release build, verify-tools, npm) plus `node scripts/validate-lifecycle-oracle-traceability.cjs`; `cargo clean` if `target/` grew heavy
- [x] T007 [US1] One independent code review including the mandatory cfg-lens sweep; open PR; on green CI auto-squash-merge with explicit subject/body (spec FR-015/FR-016)

**Checkpoint**: candidate pipeline GREEN dark on `main`; the Wave 1 workflow
(RED → GREEN → seal → gates → review → merge) is proven end to end.

---

## Phase 4: User Story 2 — Trust: rolling verification (Priority: P1) — Wave 1 pair 2

**Goal**: what is `Current` is provably current — the 15-minute monotonic
deadline machine and sealed receipts exist dark (020:T055 + T062)

**Independent Test**: `cargo test --test rolling_verification_v11 -- --test-threads=1`
GREEN dark.

- [x] T008 [P] [US2] Branch `feature-020-s4-p2-verification` from updated `main`; author RED `tests/rolling_verification_v11.rs` with frozen name `rolling_passes_are_fair_resumable_and_fenced` plus the full 020:T055 case list (scope discovery, entry obligations, same-stamp rewrites, exact deadline boundary with `VerificationOverdueLatched`, no-extension-by-partial-work, overdue acquisition refusal, fenced proof refresh, sealed `VerificationScopeReceipt`, `VerificationWorkBound` ≤ 712 s, `VerificationFeasibilityReceipt` lost-reservation → non-Current, policy-mismatch re-scout); record observed RED (020:T055)
- [x] T009 [US2] Implement `src/index_lifecycle/verification.rs` from nothing — racy-clean entry obligations, scope-discovery deadlines, resumable rolling passes, immutable proof refresh, the exact frozen-FR-049 monotonic overdue predicate; register in `mod.rs` (020:T062)
- [x] T010 [US2] Extend darkness seal + census for verification.rs; refresh `FULL_SOURCE_PIN_V1` via the Rust oracle
- [x] T011 [US2] Full gate battery via Terminal Commander + traceability validator
- [x] T012 [US2] Review (cfg-lens) → PR → green CI → auto-squash-merge

**Checkpoint**: verification machine GREEN dark on `main`.

---

## Phase 5: User Story 3 — Bounded retrieval: strict query leases (Priority: P2) — Wave 1 pair 3

**Goal**: exact all-`Current` selection or typed refusal, and the US2 health
split, exist dark behind the factory (020:T056 + T063)

**Independent Test**: `cargo test --test project_query_lease_v11 -- --test-threads=1`
GREEN dark.

- [x] T013 [P] [US3] Branch `feature-020-s4-p3-query` from updated `main`; author RED `tests/project_query_lease_v11.rs` with frozen name `strict_selection_is_atomic_and_complete` plus the 020:T056 lease cases (atomic multi-source capture, empty/missing/extra/mismatched `SelectedAggregate` rejection, exact bijection, no-match-only-all-Current, stale finalization, retarget races, post-lease `OutputCoverage::Truncated` identity preservation, SC-019 protected-root promotion with zero below-root probe I/O); record observed RED (020:T056)
- [x] T014 [US2] In the same RED file (after T013), author the frozen-named `committed_generation_and_attempt_health_are_separate` cases across health, health_compact, status, and health resources (the US2 acceptance oracle) (020:T056)
- [x] T015 [US3] Implement `src/index_lifecycle/query.rs` — project/single-source strict leases, exact multi-project selections, separate ranking snapshots, sealed completed-lease render authority, `SourceRefusal` transport mapping, and the committed-vs-attempt health projection seam (all four surfaces modeled INSIDE the dark module — the call-edge sweep forbids the index_lifecycle token in live files, so the `src/live_index/health_view.rs` and `src/protocol/` wiring named by frozen T063 is cut work under T064/T066); register in `mod.rs` (020:T063)
- [x] T016 [US3] Extend darkness seal + census for query.rs (no health_view/protocol seams — see the T015 scope correction); refresh `FULL_SOURCE_PIN_V1` via the Rust oracle
- [x] T017 [US3] Full gate battery via Terminal Commander + traceability validator
- [x] T018 [US3] Review (cfg-lens) → PR → green CI → auto-squash-merge

**Checkpoint**: query-lease machinery GREEN dark on `main`; US2's oracle exists.

---

## Phase 6: User Story 4 — Convergence: observer (Priority: P2) — Wave 1 pair 4

**Goal**: bounded coalescing, monotonic cuts, gap latches, and stable handoff
exist dark (020:T054 + T061); the performance gate itself runs at the cut

**Independent Test**: `cargo test --test observer_handoff_v11 -- --test-threads=1`
GREEN dark.

- [x] T019 [P] [US4] Branch `feature-020-s4-p4-observer` from updated `main`; author RED `tests/observer_handoff_v11.rs` with frozen name `stable_token_cut_latches_every_gap` plus the 020:T054 case list (stable-token cuts, gap latching, predecessor drain, post-barrier baseline, ingress unwind retention, exhausted-capacity safety transitions); record observed RED (020:T054)
- [x] T020 [US4] Implement `src/index_lifecycle/observer.rs` — bounded coalescing accumulator, monotonic invalidation cuts, scope-dirty/gap latches, stable observer handoff, full successor baseline, adapting the live `src/watcher/` event vocabulary without touching its live routing; register in `mod.rs` (020:T061)
- [x] T021 [US4] Extend darkness seal + census for observer.rs; refresh `FULL_SOURCE_PIN_V1` via the Rust oracle
- [x] T022 [US4] Full gate battery via Terminal Commander + traceability validator
- [x] T023 [US4] Review (cfg-lens) → PR → green CI → auto-squash-merge

**Checkpoint**: observer GREEN dark on `main`.

---

## Phase 7: User Story 5 — Recovery: snapshot V11 migration (Priority: P3) — Wave 1 pair 5

**Goal**: untrusted-seed restore, quarantine, rollback, and the frozen-FR-051
matrix exist dark; the version bump stays unreachable until activation
(020:T057 + T065)

**Independent Test**: `cargo test --test snapshot_v11_migration -- --test-threads=1`
GREEN dark.

- [x] T024 [P] [US5] Branch `feature-020-s4-p5-snapshot` from updated `main`; author RED `tests/snapshot_v11_migration.rs` with frozen name `snapshot_seed_requires_complete_current_proof` plus the 020:T057 case list (untrusted V10 seeds, pre-decode capacity, root/digest mismatch, quarantine, rollback, concurrent V10 writers, `.symforge/v11/` namespace isolation, secret-canary bytes never persisted); record observed RED (020:T057)
- [x] T025 [US5] Implement dark V11 snapshot migration in `src/index_lifecycle/snapshot.rs` (NOT `src/live_index/persist.rs` — the darkness sweep forbids the module token in live files, same ruling as T015/T016; the persist.rs wiring and version bump are cut work) — bounded untrusted-seed restore, complete re-observation, quarantine, atomic V11 replacement, preserved rollback, rebuild fallback, `ProjectStateDir`-only team-artifact persistence, the exact frozen-FR-051 four-state receipt/refusal matrix; the `CURRENT_VERSION` bump and V11 write path stay production-unreachable until T030 activation (020:T065)
- [x] T026 [US5] Extend darkness seal + census for snapshot.rs (the dark module the T025 correction landed in; persist.rs stays untouched until the cut); refresh `FULL_SOURCE_PIN_V1` via the Rust oracle
- [x] T027 [US5] Full gate battery via Terminal Commander + traceability validator
- [x] T028 [US5] Review (cfg-lens) → PR → green CI → auto-squash-merge

**Checkpoint**: all five pairs GREEN dark on `main`; the only ignored
lifecycle tests left are T058's four stand-ins. Wave 2 may open.

---

## Phase 8: The indivisible activation cut (Wave 2 — one branch, one PR)

**Purpose**: enable everything at once; no story labels because no subset is
independently shippable (spec FR-001). Branch
`feature-020-slice-4-activation` from `main`; all tasks land in this one PR.

- [x] T029 Route every ingress per the permit/pipeline split — external watcher/sidecar/hook/temporal/bridge/authority/local-ref/derived observations through the isolated candidate pipeline permit-free; SymForge-owned edit/curation/init/root-ignore/`.gitattributes`/hygiene writes through a fresh `SourceMutationPermit` that first publishes non-Current — in `src/watcher/mod.rs`, `src/sidecar/`, `src/cli/init.rs`, `src/cli/hook.rs`, `src/protocol/edit.rs`, `src/protocol/edit_hooks.rs`, `src/live_index/` (020:T064)
- [x] T030 Implement `src/index_lifecycle/activation.rs` — `LegacyOpen → LegacyClosing → PreventiveV1Open`, register every tool/resource/prompt, cache/CCR/retrieval, sidecar/hook, and finalization lane; process-wide, non-configurable (020:T066)
- [x] T031 In the same change, expose only the attested V11 replacement API and `EmbeddedSourceHandle`, retire all 244 inventoried V10 members per the frozen category dispositions, hold exact-graph equality across the 26 configuration cells, and flip the pre-plotted keywords (`server_api` `pub(crate)`→`pub`, `#[path]` mount → private `lib.rs` mod with `embed.rs` re-exports) in `src/daemon.rs`, `src/main.rs`, `src/sidecar/`, `src/cli/hook.rs`, `src/embed.rs`, `src/lib.rs` (020:T067)
- [x] T032 Give T058's four stand-ins observing bodies and remove their `#[ignore]` attributes at `tests/activation_cut_v11.rs:2120–2154` (`preventive_v1_is_the_only_live_mode`, `embedded_source_has_one_handle_and_no_raw_bypass`, `every_source_write_requires_current_mutation_permit`, `state_owners_and_team_artifact_are_exact`) plus focused `src/cli/init.rs` and persistence tests for cold-recovery-cannot-mint-permit and the frozen-FR-051 matrix (020:T058)
- [x] T033 Add `criterion` dev-dependency and the `[[bench]]` target `observed_refresh_gate_v1` (frozen registration `criterion_group:observed_refresh_gate_v1_group->observed_refresh_gate_v1`); build `benches/observed_refresh_gate_v1.rs` with the fixed workloads, trigger matrices, campaigns, corpora digests, host/cache/quiescence controls, completion receipts, and clean-rebuild equivalence fixtures in `tests/fixtures/observed-refresh-v1/`; emit the code-owned benchmark receipt (020:T068)
- [x] T034 Add retained-plus-candidate peak accounting, burst convergence, capacity fairness, and no-unaccounted-residency measurements — frozen-named `whole_runtime_capacity_is_conserved_under_activation` in `tests/process_capacity_pool_v11.rs` plus `benches/observed_refresh_gate_v1.rs` additions (020:T069)
- [x] T035 Run `ObservedRefreshGateV1` against baseline `1521abb0` and the candidate; require p95 ≤ 2 s, max ≤ 5 s, p95 ≤ 1.25× baseline, no single-path full rebuild outside Gap/ScopeDirty; record exact results in `docs/reviews/OBSERVED-REFRESH-GATE-v1.md` (020:T070)
- [x] T036 Author and pass `tests/delta_full_rebuild_equivalence_v11.rs` with frozen names `every_edit_matches_clean_full_rebuild` and `knowledge_artifacts_match_clean_full_rebuild` — identical canonical manifest, artifact digests, and representative query results per advertised edit class (020:T071)
- [ ] T037 Run the indivisible activation campaign through `all_ingress_uses_exact_typed_authority_branch` across daemon, stdio, serve, embed, every tool/resource/prompt handler, sidecar/hook lanes, snapshot, observer, mutation, local-ref, derived, cache, CCR, retrieval, including the frozen-FR-051 four-state matrix AND the carried Slice 3 residual families — cancelled non-abortable `index_folder` (activation epoch or authoritative ACTIVE re-sync), D16 cross-process publication atomicity, D14 live-observer invalidation proof, the replay-authority forbidden shortcut, and the repeat-cache/CCR publication-identity fence; then the full gate battery via Terminal Commander (020:T072, campaign half)

**Checkpoint**: the cut branch is functionally complete and gate-green;
live acceptance for all five user stories verified via quickstart spot-checks.

---

## Phase 9: Polish & closure

- [ ] T038 Complete the post-slice multi-round adversarial code review (fresh reviewer rounds + refuters, mandatory cfg-lens, explicit verdicts on each T037 residual family) and close `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` with zero unresolved P0/P1/P2 (020:T072, evidence half)
- [ ] T039 [P] Write the breaking lifecycle/embed migration boundary, removed exports, replacement APIs, and rollback constraints in `docs/migrations/v11-index-lifecycle.md` (020:T073)
- [ ] T040 Verify `git diff --stat <merge-base> HEAD -- specs/020-repository-knowledge-index/` is empty (spec SC-008); present the cut PR to the operator and **wait for explicit approval** before merging (spec FR-015); after approval, squash-merge with explicit subject/body and watch the release-please cycle per repo rules
- [ ] T041 [P] Save durable findings to agentmemory with the `[symforge]` prefix; run `cargo clean` in the worktree; update `docs/reviews/FEATURE-020-SLICE4-CAMPAIGN-v11.md` status line via a follow-up docs commit if the campaign deviated from plan

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none — start immediately.
- **Foundational (Phase 2)**: empty.
- **Pairs (Phases 3–7)**: each pair depends only on Setup. Pairs are
  logically `[P]`-independent (frozen roster) **but** the darkness-seal pin,
  census, and `mod.rs` are shared files — so pair *merges* serialize: each
  pair branches from `main` after the previous pair's merge. Recommended
  order: P1 candidate → P2 verification → P3 query → P4 observer → P5
  snapshot (candidate first; the others attach to its vocabulary).
- **Cut (Phase 8)**: requires ALL five pairs merged (checkpoint after
  Phase 7). Tasks T029–T031 are one logical change (mixed authority is
  forbidden mid-branch history is fine; the PR is the unit); T032 follows
  T029–T031; T033/T034 may proceed in parallel with T029–T032 on separate
  files; T035–T037 strictly after everything else.
- **Closure (Phase 9)**: T038 after T037; T039 parallel with T038;
  T040 after both; T041 last.

### Parallel Opportunities

- The five RED-authoring tasks (T002, T008, T013+T014, T019, T024) touch
  disjoint new files and may be *drafted* in parallel; their machinery,
  seal, and merge steps serialize per the shared-file rule.
- Within Phase 8: T033 then T034 (T034 appends to the bench file T033
  creates), both parallel to T029–T032 (src rerouting).
- T039 (migration docs) parallel to T038 (adversarial review).

---

## Implementation Strategy

**MVP = Phase 3 (US1 candidate pair).** It delivers the highest-priority
story's machinery AND proves the entire Wave 1 workflow (RED → GREEN → seal →
census → gates → cfg-review → auto-merge) on the smallest scope. Stop and
validate after T007 before fanning out.

**Incremental delivery**: one pair PR at a time onto `main`, always green,
always dark. The activation cut opens only at the Phase 7 checkpoint and
ships as a single operator-approved PR. Rollback story: any Wave 1 PR is
independently revertable (behavior-neutral); the cut PR is the only
non-trivial revert and is why it waits for explicit approval.

**Standing discipline** (every task): serial cargo via Terminal Commander for
anything >10 min; the embed-build no-default-features gate before every push;
seal recomputes only via the Rust oracle; explicit `--subject`/`--body` on
every squash-merge.
