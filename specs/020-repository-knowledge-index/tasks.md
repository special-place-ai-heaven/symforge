> [!CAUTION]
> **Execution authority changed for V11.** The checked V10 gates below are immutable
> historical receipts only. They do not authorize V11 implementation, do not satisfy
> any V11 task, and must not be edited or reinterpreted. The only executable work in
> this file is the unchecked `Txxx` graph after `END V10 HISTORICAL RECEIPTS`.

<!-- BEGIN V10 HISTORICAL RECEIPTS — PRESERVE EVERY LINE VERBATIM -->

# Tasks: Repository Knowledge Index

**Branch**: `feat/repository-knowledge-index`
**Rule**: RED test -> observe intended failure -> minimal implementation -> focused
GREEN -> impact review. For a not-yet-defined type/tool, compile-fail is the initial
observed RED and MUST become a runnable behavioral RED before that gate closes. No
production step may precede its red oracle.

## Gate A — Spec and baseline

- [x] A-001 Complete all SpecKit artifacts.
- [x] A-002 Run opposite-model Skeptic/Architect/Minimalist review.
- [x] A-003 Resolve every accepted high finding and rerun review/checklist.
- [x] A-004 Capture clean focused baseline for discovery, watcher, persist, search,
  surface, and admission integration tests.

**STOP**: no product code before Gate A passes.

## Gate B — Metadata-first scout

### RED

- [x] B-R01 `scout_sparse_hard_skip_does_not_consume_ingest_budget`.
- [x] B-R02 `scout_unknown_metadata_never_defaults_to_zero`.
- [x] B-R03 `scout_manifest_is_total_and_deterministically_sorted`.
- [x] B-R04 `scout_binary_probe_never_exceeds_binary_sniff_bytes`.
- [x] B-R05 `scout_case_fold_pair_is_total_and_failure_is_per_entry`.
- [x] B-R06 `catalog_entry_ceiling_never_publishes_false_complete_manifest`.
- [x] B-R07 `non_utf8_path_is_opaque_catalog_only_without_lossy_collision`.
- [x] B-R08 `automatic_protected_roots_stay_unbound_before_source_or_project_state_io`.
- [x] B-R09 `unbound_bootstrap_rebinds_writable_project_without_restart`.
- [x] B-R10 `explicit_protected_root_requires_override_and_never_touches_local_state`.
- [x] B-R11 `explicit_protected_root_uses_user_local_then_memory_only`.
- [x] B-R12 `readable_unwritable_project_relocates_state_without_retargeting_source`.
- [x] B-R13 `root_state_key_coalesces_aliases_and_isolates_repos_and_worktrees`.
- [x] B-R14 Existing/absent/equivalent/concurrently changed `.gitignore` matrix for
  explicit normal `index_folder` and project-aware init: empty, BOM-only, CRLF, LF,
  final/no-final newline, effective rooted equivalent, ordered negation, global/info-
  exclude-only, hash race, and symlink/reparse cases; automatic paths are read-only.
- [x] B-R15 `.symforge` is hard-excluded under every state placement.
- [x] B-R16 `failed_retarget_preserves_previous_generation_and_watcher`.
- [x] B-R17 `state_placement_nested_global_dir_is_excluded_from_scout_and_watcher`.
- [x] B-R18 `device_or_uncanonicalizable_root_remains_nonindexable_with_override`.
- [x] B-R19 `global_state_identity_mismatch_is_never_loaded_or_overwritten`.
- [x] B-R20 Snapshot/reset/quarantine/checkpoint use resolved state placement, never
  reconstruct `<source>/.symforge`.
- [x] B-R21 `control_state_never_falls_back_to_launch_cwd_or_relative_symforge`.
- [x] B-R22 `team_artifact_export_refuses_non_project_local_or_protected_binding` and
  normal export reports exactly `already_tracked`, `untracked_visible`,
  `ignored_force_add_required`, or `git_visibility_unavailable`.
- [x] B-R23 `project_symforge_symlink_or_reparse_point_uses_global_without_following`.
- [x] B-R24 `catalog_metadata_budget_is_independent_and_never_publishes_partial_manifest`.
- [x] B-R25 `protected_membership_is_per_session_and_requires_each_direct_override`.
- [x] B-R26 `protected_membership_is_not_inherited_by_reconnect_alias_or_restart`.
- [x] B-R27 `index_folder_replay_reestablishes_live_binding_or_returns_live_postcondition_unavailable`;
  override/path changes under one key conflict before binding.
- [x] B-R28 `post_bind_state_write_failure_degrades_durability_not_live_readiness`.
- [x] B-R29 `every_state_consumer_requires_its_typed_owner`: snapshot/temp/quarantine/
  reset/checkpoint/replay/mutation intent/coupling/frecency/STEL/analytics/API-key/
  edit-safety TEE/cleanup use `ProjectStateDir`; edit-safety trust store, sidecar
  port/PID/session descriptors and status readers, daemon discovery/control, hook adoption/hints, operator
  profile, onboarding, runtime-startup coordination, cross-project replay/locks,
  version registry, and updater use `ControlStateDir`; none reconstruct source-local
  or launch-CWD state, and every reader uses the same resolver as its writer. Two
  concurrent project/daemon descriptor namespaces remain isolated and discoverable.
  Operator/onboarding state is intentionally global; legacy per-project files remain
  untouched and missing global state reruns onboarding once.
- [x] B-R30 Reindexing the same live `ProjectInstance` preserves its resolved placement;
  closing it and constructing a new instance re-runs placement and may recover to a
  newly available durable owner.
- [x] B-R31 `cold_start_budget_exhaustion_yields_distinct_typed_capacity_reasons`:
  entry- and metadata-budget exhaustion on a manifest-less cold start return distinct
  `FreshnessReason` values and publish no partial manifest or budget `ScoutIssue`.
- [x] B-R32 `parse_status_is_bounded_and_digest_stable`: reworded operational parser
  diagnostics do not change the manifest digest, and a knowledge-only file carries a
  knowledge-extractor status rather than a synthetic code-parse status.

### GREEN

- [x] B-G01 Add manifest/target/disposition/coverage types, including Gate-E core
  `HistoryLimit`/`HistoryCoverage`, with owned serializable access/reason enums in
  `src/domain/index.rs`.
- [x] B-G02 Implement metadata-first `scout_repository` in `src/discovery/mod.rs`.
- [x] B-G03 Apply metadata admission before admitted-byte accounting.
- [x] B-G04 Record walk/metadata issues without path omission.
- [x] B-G05 Canonically sort and validate manifest paths.
- [x] B-G06 Replace load/reload discovery entry points with the scout.
- [x] B-G07 Include repository-owned hidden knowledge paths; hard-exclude only
  declared internals and expose ignore-pruned coverage.
- [x] B-G08 Add lossless runtime path identity plus persisted safe UTF-8/opaque
  catalog projection.
- [x] B-G09 Add typed `RootResolution` with automatic/init refusal and explicit
  `allow_protected_root` authority; route every startup/session/index entry point.
- [x] B-G10 Add `StatePlacement` after binding: project-local, private user-local
  per root ID, then memory-only with explicit capability loss.
- [x] B-G11 Fence snapshot/quarantine/project-state consumers to selected placement;
  never probe protected-root `.symforge` or change the bound source on fallback.
- [x] B-G12 Keep unbound servers responsive and make later valid rebind clear prior
  bootstrap/state errors without process restart.
- [x] B-G13 Add one shared guarded/idempotent append to an existing root
  `.gitignore` for explicit normal `index_folder` and project-aware init; absent file
  remains absent, automatic paths stay read-only, and protected mode cannot mutate.
- [x] B-G14 Add dynamic absolute exclusion when selected user-local state lies under
  the source, plus snapshot root-ID validation and fail-safe identity mismatch.
- [x] B-G15 Preserve existing bindings on failed retarget and keep explicit-protected
  authorization private to the direct `index_folder` boundary.
- [x] B-G16 Separate global transport/replay control placement from project state;
  use process-local degraded coordination when no safe user-local base exists.
- [x] B-G17 Gate legacy team-artifact/`.gitattributes` mutation on normal writable
  project-local capability; never redirect it to fallback state.
- [x] B-G18 Enforce catalog-entry and metadata-byte budgets independently from
  admitted-content/in-flight budgets; retain typed attempt coverage and never
  publish a false Complete or partial candidate manifest.
- [x] B-G19 Make protected membership session-local; require a fresh direct exact
  override after reconnect/new session/restart while sharing an already authorized
  `ProjectInstance` only after the requesting session is admitted.
- [x] B-G20 Make `index_folder` replay prove the current live postcondition before
  returning stored success; preserve the historical receipt on typed failure.
- [x] B-G21 Move every durable consumer behind the typed state-owner APIs and expose
  reason-bearing capability loss after placement or later state-write failure.

### VERIFY

- [x] B-V01 Focused discovery/admission tests green.
- [x] B-V02 Existing admitted-file byte-cap behavior remains green.
- [x] B-V03 File impact review for domain/discovery/store.
- [x] B-V04 Root/state/init tests cover Windows, Unix, WSL, UNC, extended-prefix,
  symlink aliases, permission failures, and simulated protected roots without
  touching the host's real protected directories.
- [x] B-V05 Session/reconnect/restart/idempotency, exhaustive state-owner, root
  `.gitignore` byte, and four-state team-artifact matrices are green.

## Gate C — Stable bounded reads and total execution

### RED

- [x] C-R01 `stable_read_refuses_over_ceiling_before_allocation`.
- [x] C-R02 `stable_read_rejects_changed_manifest_stamp`.
- [x] C-R03 `read_failure_retains_unreadable_disposition`.
- [x] C-R04 `circuit_breaker_tail_retains_aborted_dispositions`.
- [x] C-R05 `inflight_permit_releases_at_staged_handoff_without_deadlock`.
- [x] C-R06 `stable_read_double_pass_rejects_same_stamp_torn_write`.
- [x] C-R07 `read_larger_than_inflight_budget_is_terminal_hard_skip` with exact
  `HardSkip(PerFileCeiling)` accounting and zero allocation/read.
- [x] C-R08 A circuit-breaker trip is isolated to one source/lane/stage, marks only
  its remaining entries aborted, degrades coverage, and schedules repair.

### GREEN

- [x] C-G01 Implement one bounded probe helper.
- [x] C-G02 Implement one bounded stable-read helper with double-pass hash and
  pre/post checks.
- [x] C-G03 Keep in-flight permits through parse/hand-off, then transfer bytes to
  independent staged admitted-content accounting.
- [x] C-G04 Replace dropped outcomes with terminal dispositions.
- [x] C-G05 Mark circuit-breaker tail entries rather than dropping them.
- [x] C-G06 Map requests larger than total in-flight budget to terminal
  `HardSkip(PerFileCeiling)` before allocation.
- [x] C-G07 Scope circuit breakers per source/lane/stage and convert every tripped tail
  into explicit degraded dispositions plus bounded reconciliation.

### VERIFY

- [x] C-V01 Stable-read/accounting tests green.
- [x] C-V02 CRLF/UTF-8/source-span regressions green.
- [x] C-V03 Peak-memory bounded fixture receipt captured.

## Gate D — Watcher and reconciliation

### RED

- [x] D-R01 `watcher_admits_sparse_gguf_before_read`.
- [x] D-R02 `watcher_file_change_publishes_all_lanes_once`.
- [x] D-R03 `reconcile_rescout_discovers_new_text_files`.
- [x] D-R04 `reconcile_rescout_tracks_catalog_only_shrink_and_delete`.
- [x] D-R05 `overflow_fresh_instance_repairs_missed_create_delete`.
- [x] D-R06 `stale_file_batch_cannot_mutate_any_lane`.
- [x] D-R07 `reconcile_racing_watcher_event_loses_neither_update`.
- [x] D-R08 `degraded_walk_retries_until_complete_even_when_digest_equal`.
- [x] D-R09 A transient `Unreadable`/`UnstableDuringRead` entry makes coverage
  Degraded, defeats equal-digest no-op, and converges after bounded re-observation.
- [x] D-R10 A new uncertainty signal re-triggers a source that previously settled in
  explicit degradation; repair never becomes a silent permanent stop.

### GREEN

- [x] D-G01 Remove extension filtering before scout/removal.
- [x] D-G02 Route single-file updates through shared admission/read.
- [x] D-G03 Add one generation-fenced batch update for all lane state.
- [x] D-G04 Make removal clear indexed and catalog state.
- [x] D-G05 Replace Tier-1 stale walk with complete manifest diff.
- [x] D-G06 Trigger authoritative reconciliation on uncertainty signals.
- [x] D-G07 Serialize commits under one writer boundary; stale off-lock builds
  rebase/retry or abort.
- [x] D-G08 Make the manifest the sole disposition authority; remove stored
  `skipped_files`, project compatibility output, and retire direct skip mutations.
- [x] D-G09 Keep bounded re-observation state for unreadable/unstable entries until a
  stable terminal disposition replaces them.

### VERIFY

- [x] D-V01 Watcher/reconcile focused suites green serially.
- [x] D-V02 Event-storm/rename/overflow test repeated without nondeterminism and
  with bounded debounced publication/digest cost.
- [x] D-V03 Equal Complete-manifest reconciliation publishes nothing; Degraded
  coverage always schedules bounded repair.

## Gate E — Snapshot and atomic publication

### RED

- [x] E-R01 `snapshot_round_trip_preserves_target_enum_and_catalog_dispositions`.
- [x] E-R02 `background_verify_uses_shared_admission_for_large_new_file`.
- [x] E-R03 `background_verify_cannot_mutate_after_project_retarget`.
- [x] E-R04 `published_generation_is_atomic_under_concurrent_reloads`.
- [x] E-R05 `failed_reload_preserves_previous_generation`.
- [x] E-R06 `failed_observation_publishes_degraded_last_valid_wrapper`.
- [x] E-R07 `verifying_snapshot_is_not_query_ready`.
- [x] E-R08 `same_path_repository_replacement_never_inherits_snapshot_or_temporal_state`.
- [x] E-R09 `manifest_publication_snapshot_and_response_envelope_preserve_source_version`.
- [x] E-R10 `background_verify_racing_watcher_update_rebases_or_aborts` using captured
  base publication/content/project generations.

### GREEN

- [x] E-G01 Version snapshot schema with canonical manifest/dispositions, Gate-E core
  `HistoryLimit`/`HistoryCoverage`, and captured source version including closed
  working-tree state.
- [x] E-G02 Restore/rebuild Gate-E core state only—live index, health, outline,
  resident search structures, and code signals—from one candidate snapshot.
- [x] E-G03 Route verification through scout/stable-read and fence it.
- [x] E-G04 Add one immutable `PublishedSourceSet` swap boundary containing
  per-source core `PublishedGeneration` bundles with captured source version.
- [x] E-G05 Build reload replacement directly instead of clone-overwrite.
- [x] E-G06 Preserve quarantine/source-rebuild behavior.
- [x] E-G07 Separate publication/content generations and keep freshness inside the
  captured immutable source bundle.
- [x] E-G08 Bind snapshot headers to stable repository/source/version identity plus
  manifest, resident-content, and applicable Git-history fingerprints; verify before
  Ready or overwrite instead of trusting placement path identity.
- [x] E-G09 Fence verifier commit to captured base publication/content/project
  generations so it cannot replace a newer watcher/reconciliation publication.

Gate E's bundle compiles with core fields through code signals only. Gate G adds
bridge state after bridge types exist; Gate H adds authority state after authority
types exist. The final data-model shape is post-H, not a Gate-E forward dependency.

### VERIFY

- [x] E-V01 Snapshot/recovery/publication focused tests green.
- [x] E-V02 Crash-injection points retain previous valid generation.
- [x] E-V03 Concurrent stress reports zero mixed generations.
- [x] E-V04 Same-path replacement, history mismatch, source drift, and corrupted
  identity fixtures never load or overwrite mismatched state; clean/dirty/not-
  applicable/unknown source versions round-trip without replacing exact digests.

## Gate F — Knowledge targets and extraction

### RED

- [x] F-R01 `knowledge_scope_finds_text_without_leaking_into_code_scope`.
- [x] F-R02 `config_targeted_to_both_scopes_is_findable_in_both`.
- [x] F-R03 `unknown_utf8_file_becomes_generic_knowledge`.
- [x] F-R04 `markdown_hit_preserves_section_and_line_pointer`.
- [x] F-R05 Markdown ATX/Setext/fence/frontmatter/table/link corpus.
- [x] F-R06 Sensitive files remain catalog-only with zero value leakage.
- [x] F-R07 `lfs_pointer_is_catalog_only_and_never_knowledge_searchable`.
- [x] F-R08 `detector_positive_hit_is_withheld_whole_in_direct_and_ccr_paths`.
- [x] F-R09 `sensitive_path_is_cataloged_without_content_read`.
- [x] F-R10 `safe_template_path_still_runs_content_detector`.
- [x] F-R11 `detector_failure_fails_closed_and_discards_transient_bytes`.
- [x] F-R12 `query_and_every_visible_hit_field_are_guarded_without_echo`.
- [x] F-R13 `non_utf8_text_is_catalog_only_without_lossy_evidence`.
- [x] F-R14 `text_format_matrix_routes_mdx_rst_asciidoc_org_extensionless_and_safe_configs`.
- [x] F-R15 `text_byte_matrix_handles_zero_lf_crlf_bom_multibyte_invalid_utf8_and_no_final_newline`.
- [x] F-R16 `cold_watch_reconcile_and_background_verify_have_identical_knowledge_units_and_dispositions`.

### GREEN

- [x] F-G01 Add overlapping invariant-bearing
  `IndexTargets::{Code, Knowledge, CodeAndKnowledge}` routing with no empty variant.
- [x] F-G02 Add `LanguageId::Text` and generic safe-text extraction.
- [x] F-G03 Convert unknown textual candidates from unsupported to knowledge.
- [x] F-G04 Classify prose as text/knowledge; retain config/schema overlap.
- [x] F-G05 Harden Markdown structural spans minimally to pass corpus.
- [x] F-G06 Add typed sensitive admission and one deterministic whole-hit output
  guard shared by direct and CCR paths.
- [x] F-G07 Project Markdown Section records; do not persist duplicate knowledge
  units and keep code doc-comments out of v1.
- [x] F-G08 Implement versioned high-precision byte rules with existing `regex`,
  compile once, and store only safe rule IDs/counts.
- [x] F-G09 Route the declared text-centric format matrix; accept UTF-8/BOM only
  and type unsupported encodings without conversion.

### VERIFY

- [x] F-V01 Parsing/search scope suites green.
- [x] F-V02 Existing code-language and config parser suites green.
- [x] F-V03 No parser dependency added unless corpus evidence requires it.
- [x] F-V04 Runtime-canary snapshot/CCR/analytics containment assertions pass
  without printing the canary on failure.

## Gate G — Evidence bridge core

### RED

- [x] G-R01 Exact repository path and unique code-spanned symbol resolve
  bidirectionally; bare prose similarity and external links create no code edge.
- [x] G-R02 Same-name/kind symbols at different spans remain `ambiguous`, never
  collapse to one guessed anchor.
- [x] G-R03 Missing path/symbol, supported structured value, and declared ownership
  selector retain typed resolution, source-local provenance, exact candidate count,
  and bounded samples.
- [x] G-R04 Document/code create/change/rename/remove through watcher and
  reconciliation repairs forward and reverse links in the same publication.
- [x] G-R05 A bridge build computed from an old content generation is rejected after
  a watcher publication; a call pinned before publication finishes entirely from
  its captured generation.
- [x] G-R06 Bridge candidate/selector/sample/metadata budget exhaustion leaves the
  extracted knowledge units intact and marks bridge coverage truncated/degraded.
- [x] G-R07 Source identity is part of every bridge key; equal paths/symbols from
  different `ProjectInstance` or source identities never satisfy one another.
- [x] G-R08 Historical contributors remain contributor evidence and never satisfy a
  declared owner/CODEOWNERS selector.

### GREEN

- [x] G-G01 Add closed-world bridge candidates only for internal links, exact paths,
  code-spanned unique symbols, supported structured values, and declared ownership
  selectors.
- [x] G-G02 Add exact/declared-set/ambiguous/missing forward and reverse state with
  stable link IDs, compact anchor IDs, independent limits, and explicit coverage.
- [x] G-G03 Resolve only against code from the same captured source/content
  generation and canonically order every bounded candidate set.
- [x] G-G04 Extend the Gate-E immutable `PublishedGeneration` with bridge and reverse-
  link state; stale off-lock derivations abort rather than overwrite.
- [x] G-G05 Route watcher/reconcile changes through one affected-bridge rebuild and
  atomic publication; keep bridge discovery code-scope and frecency neutral.
- [x] G-G06 Keep contributor history separate from declared ownership governance.

### VERIFY

- [x] G-V01 Bridge resolution/update/publication focused suites are green.
- [x] G-V02 Concurrent bridge/read stress observes zero mixed generations or
  cross-source resolutions.
- [x] G-V03 Repeated equal generations produce byte-identical bridge ordering and
  stable link IDs.

## Gate H — Knowledge authority foundation

### RED

- [ ] H-R01 Lifecycle, authority domain, aggregate code evidence, and retrieval voice
  remain independent for old-correct, new-wrong, mixed-section, intent, ADR,
  governance, changelog, generated, and unknown fixtures.
- [ ] H-R02 Age/mtime/birth time/later churn can only produce review signals; exact
  linked-code changes after the document commit can produce only
  `RelevantCodeChangedSinceDocument`.
- [ ] H-R03 Temporal provenance distinguishes complete-to-root, shallow,
  bounded-window, rename-follow-limited, divergent, dirty/new working tree,
  clock-skewed, and unavailable evidence without upgrading any timestamp to proof.
- [ ] H-R04 Broken anchor, deterministic structured mismatch, implementation gap,
  suspected conflict, relevant-code change, unresolved, and not-evaluated states retain
  bounded typed aggregate evidence together.
- [ ] H-R05 Malformed/unsupported policy, native-policy conflict, supersession cycle,
  and stale file/unit hash remove suppression, block unsafe curation eligibility,
  and leave raw safe code/knowledge available.
- [ ] H-R06 Intent/ADR/governance divergence is an implementation gap; only a
  deterministic current-implementation claim may be code-diverged.
- [ ] H-R07 One conflicting unit never suppresses unaffected units in the same file;
  whole-file summaries retain all unit states.
- [ ] H-R08 Cold load, watcher update, reconciliation, and background verification
  derive byte-identical units/lifecycle/evidence/voice for the same source bytes.
- [ ] H-R09 Watcher/policy/code changes publish bridge, authority, voice, and
  temporal coverage together; stale async temporal completion is rejected, queues at
  most one coalesced recomputation for the latest source, and an accepted completion
  advances publication but not content generation or manifest/content digests while
  carrying one coherent source-version tip through bundle and envelope.
- [ ] H-R10 Authority/temporal/policy budget exhaustion is explicit and cannot hide
  raw safe units or claim complete review coverage.
- [ ] H-R11 Policy/rule/secret-policy/snapshot version mismatch recomputes derived
  authority before Ready.
- [ ] H-R12 `budget_dropped_suppression_state_is_representable_and_scope_consistent`:
  a superseded/proven-divergent unit beyond the authority-record cutoff fails closed
  to voice `Suppressed`, is absent from default/current, remains retrievable through
  history/all, and exposes canonical skipped IDs plus truncated coverage.
- [ ] H-R13 A commit/ref-tip change with identical content generation rejects temporal
  completion by source-version mismatch and schedules bounded latest-state recompute.
- [ ] H-R14 Mixed evidence uses the normative compact display precedence without
  hiding stronger conflict, broken-anchor, implementation-gap, or uncertainty state.
- [ ] H-R15 `bytes_identical_commit_temporal_recompute_converges`: after one bytes-
  identical commit and quiescence, bounded recomputation accepts the new tip and the
  published source version and temporal evidence name that same commit.

### GREEN

- [ ] H-G01 Add invariant-bearing unit lifecycle, authority-domain, aggregate code-
  evidence, temporal-provenance, and derived-voice types.
- [ ] H-G02 Implement only versioned closed deterministic proof rules; keep lexical/
  model judgment advisory and retain unresolved semantics.
- [ ] H-G03 Parse one versioned `.symforge-knowledge.toml` ledger with exact whole-
  file or zero-based half-open unit byte/hash targets; stale entries become findings
  and lose suppression authority.
- [ ] H-G04 Add bounded Git/filesystem temporal evidence with explicit history and
  rename coverage; compare commit topology before clocks.
- [ ] H-G05 Derive authority/voice from the existing extracted unit/bridge state and
  extend the immutable generation with authority state and versioned coverage.
- [ ] H-G06 Reuse one derivation path for cold, watcher, reconcile, verification, and
  later blob adapters; do not create a second authority index.
- [ ] H-G07 Keep malformed/stale policy fail-open for raw safe retrieval and fail-
  closed for suppression or mutation authority.
- [ ] H-G08 Each temporal job and coalesced pending-latest marker captures the live
  content generation and exact source-version commit/tip at scheduling; accept only
  when analyzed target, marker, and current live target agree, then republish that tip
  consistently with one running worker and one pending marker per source.
- [ ] H-G09 Reserve derived-budget priority for hash-valid suppression and
  proven-divergence evidence; fail affected units closed if representation still
  cannot fit.

### VERIFY

- [ ] H-V01 Authority/policy/temporal parity suites are green.
- [ ] H-V02 A fixture matrix proves code is authoritative only for supported current-
  implementation claims and cannot erase intent/governance/history.
- [ ] H-V03 Temporal shallow/rename/clock/dirty/unavailable receipts expose exact
  provenance and never infer stale/archived/deletion from time alone.

## Gate I — `search_knowledge`

### RED

- [ ] I-R01 Exact eight-field schema (`query`, `path_prefix`, `source_scope`,
  `authority_scope`, `project`, `projects`, `limit`, `max_tokens`), read-only
  annotations, capability advertisement, and one-tool full-surface delta.
- [ ] I-R02 Every hit carries exact evidence/heading/unit/hash/source plus its own
  publication/content generation, compact authority display, stable finding/provenance/
  link IDs, bounded bridge-anchor previews, and coverage; each source envelope also
  carries the captured source version.
- [ ] I-R03 All successful no-match shapes are distinct and deterministic:
  `no_evidence_complete`, `no_evidence_degraded`, `evidence_withheld`,
  `evidence_noncurrent`, and `query_too_weak`.
- [ ] I-R04 Error/readiness matrix covers empty query; invalid path/source/authority;
  mutually exclusive or unknown project selector; unsupported source scope; scout/
  verify state; degraded last-valid; corrupt/no-valid snapshot; evicted CCR
  generation; and too-small output budget without false complete no-evidence.
- [ ] I-R05 Deterministic phrase/heading/distinct-term/source-precedence/canonical-tie
  ranking; source precedence and document authority remain independent.
- [ ] I-R06 CCR/truncation preserves full provenance, compact authority display,
  stable finding/provenance/link IDs, bounded bridge previews, source identity/version/
  generations, and whole-hit withholding without copying full evidence arrays.
- [ ] I-R07 Secret-positive query rejects before routing, echo, logging, cache,
  analytics, or CCR; each visible hit field is independently guarded.
- [ ] I-R08 Search and `ask` discovery remain frecency-neutral and knowledge intent
  never captures symbol/reference/code-text intent.
- [ ] I-R09 Compact facade preserves every successful no-match shape and never turns
  it into an MCP error; compact CCR footer/retrieval round-trips through the
  `symforge` facade and the surface remains three.
- [ ] I-R10 Default/current/intent/history/all authority scopes return distinct,
  deterministic sets; budget-failed-closed `Suppressed` units are absent from
  default/current and present in history/all, and filtered noncurrent evidence is not
  false no-evidence.
- [ ] I-R11 Two open projects prove `project`, explicit `projects`, and `projects=["*"]`
  selection, session-scoped wildcard expansion, deterministic envelope ordering,
  project/projects mutual exclusion, unknown selector error, and zero cross-project
  hit/bridge/cache leakage.
- [ ] I-R12 A search paused across watcher publication formats only its captured
  source set; the next call sees the new publication and per-hit generation.
- [ ] I-R13 Until Gate L, `current` is the only advertised source scope and each
  worktree/ref/all request returns typed unsupported-scope, never no-evidence.
- [ ] I-R14 Finding/provenance IDs survive a derived-only republication and record
  reorder, resolving through `review_knowledge` to the same source-local dossier.
- [ ] I-R15 Repeating `get_file_content` after watcher publication serves the new
  project/source/publication/content generation, never a stale repeat-cache result.
- [ ] I-R16 Formatters derive every hit/envelope source identity, source version, and
  generation from the captured bundle; mismatched copies are unrepresentable, and
  every `line`/`line_range` is one-based with a half-open range end.

### GREEN

- [ ] I-G01 Add and validate only the exact eight-field `SearchKnowledgeInput`.
- [ ] I-G02 Reuse existing resident text retrieval over knowledge targets and the
  Gate H unit-authority filter; add no embeddings/vector database/duplicate corpus.
- [ ] I-G03 Add only the corpus-proven deterministic post-ranking chain.
- [ ] I-G04 Add one full-surface tool with daemon multi-project dispatch that captures
  one immutable source set per selected `ProjectInstance` before searching.
- [ ] I-G05 Add safe exact formatter, compact evidence/link previews, CCR envelope,
  per-source version plus per-hit generation, all five no-match classes, and the
  complete typed validation/readiness error mapping.
- [ ] I-G06 Route focused knowledge through `ask` only after facade mapping tests;
  keep symbol/reference/code-text routing and compact-3 unchanged.
- [ ] I-G07 Advertise only current-source capability until Gate L expands it.

### VERIFY

- [x] I-V01 Protocol/format/surface/daemon/no-match/error/selector suites are green.
- [x] I-V02 One-call real-repository queries return exact pointers with measured
  token reduction and deterministic repeated output.
- [x] I-V03 Existing code symbol/reference/text suites contain no prose-only leakage.

## Gate J — Repository mental model and read-only review

### RED

- [x] J-R01 `get_repo_map` returns bounded current/intent role cards, missing roles,
  code topology, hygiene counts, and uncertainty from one captured source set.
- [x] J-R02 `get_file_context` and `get_symbol_context` accept
  `sections=["knowledge"]`; omitted/empty/default/bundle/budget behavior and exact
  backlink caps match the contract.
- [x] J-R03 Context repeat-cache identity includes project/source/publication/content
  generation so a watcher rebuild cannot serve prior backlinks.
- [x] J-R04 `review_knowledge` summary/document/remediation modes return exact,
  bounded, source-local dossiers with the full aggregate evidence arrays and bridge
  records referenced by search IDs, temporal provenance, blockers, and smallest
  allowed proposals.
- [x] J-R05 Per-source `review_hash` and top-level result hash are byte-stable across
  repeated equal generations and independent of `limit`, `max_tokens`, formatting,
  truncation, and CCR storage because they cover the complete untruncated plan.
- [x] J-R06 Two-project review selectors return isolated per-source plans/hashes in
  canonical order; wildcard selection is session-scoped and unknown/mixed selectors
  fail with the same typed selector contract as search.
- [x] J-R07 Map/ask/bridge/review discovery leaves frecency unchanged; only a directly
  requested code context may retain its existing commitment bump.
- [x] J-R08 Map/context/review calls paused across watcher or async temporal
  publication finish from their captured generation; concurrent calls observe no
  mixed live/outline/temporal/bridge/authority state.
- [x] J-R09 Contributor history is labeled contributor evidence and never rendered
  as ownership; missing roles remain unknown rather than inferred.
- [x] J-R10 Role/review/remediation budget exhaustion retains exact counts, stable
  IDs/hashes, and degraded coverage without inlining the prose corpus.
- [x] J-R11 Secret-positive content/input creates no card, link, dossier, proposal,
  hash input, analytics, CCR, diagnostic, or echoed field.

### GREEN

- [x] J-G01 Add fixed role cards using declared spans, versioned heading rules, and
  path conventions only; reuse existing code topology/hotspot evidence.
- [x] J-G02 Extend existing map/ask/context surfaces per contract; add no new mental-
  model tool and never generate or persist a repository summary.
- [x] J-G03 Add generation-aware context cache identity and non-bumping internal
  helpers for discovery routes.
- [x] J-G04 Add read-only `review_knowledge` over captured Gate H records with
  canonical complete-plan hashes and bounded remediation dossiers.
- [x] J-G05 Add exact duplicate/backlink/protected-role/unique-content eligibility
  checks; keep deletion evidence-only and unsupported semantics advisory.
- [x] J-G06 Add `symforge-knowledge-hygiene` prompt through review/evidence/proposal/
  approval/preview; it cannot approve or mutate by itself.

### VERIFY

- [x] J-V01 Role/map/context/review/hash/publication suites are green.
- [x] J-V02 One-call real-repository orientation and deep review return exact entry
  points/dossiers plus uncertainty without invented ownership/status/architecture.
- [x] J-V03 Review/map/context repeatability is byte-identical for equal captured
  generations and no read-only path mutates a document or policy.

## Gate K — Guarded logical curation

### RED

- [x] K-R01 Preview writes/reserves nothing; apply accepts explicit action IDs and
  exactly one current-worktree review hash plus fresh manifest/policy/target guards,
  and mutates only `.symforge-knowledge.toml`.
- [x] K-R02 Identical idempotency replay returns stored terminal success before now-
  stale freshness guards; same key/different canonical request conflicts.
- [x] K-R03 Concurrent curators serialize under one per-project policy mutation lock
  and revalidate policy/manifest/document/action guards immediately before write.
- [x] K-R04 Secret-positive input rejects before routing, echo, logging, idempotency
  reservation, evidence construction, temp write, or receipt.
- [x] K-R05 Explicit-protected/read-only/user-local-without-durable-replay/memory-
  only/ref/implicit-worktree sources expose a reason-bearing unavailable capability
  before probe evaluation, with zero probe file operations beneath the source root.
- [x] K-R06 Crash after durable intent reservation recovers one pending request and
  never mutates the ledger without completed validation.
- [x] K-R07 Crash after validation but before temp-file sync leaves the previous
  complete ledger and a deterministically recoverable request.
- [x] K-R08 Crash after temp-file `sync_all` but before atomic replace leaves either
  the old complete ledger or a recoverable validated temp, never partial policy.
- [x] K-R09 Crash after atomic replace/parent-directory durability but before
  completion recording detects the exact post-state and terminalizes the request
  without applying it twice.
- [x] K-R10 Crash after completion recording replays the stored result; startup
  recovery quarantines/blocks indeterminate state rather than guessing success.
- [x] K-R11 Successful apply triggers ordinary watcher/reconciliation publication;
  the receipt reports applied/pending generation, an already captured reader keeps
  its old generation, and a later reader sees the new policy/voice atomically.
- [x] K-R12 Move/delete/schema-invalid action, stale review/policy/manifest/target,
  unknown action, or any mixed batch failure causes zero policy mutation.
- [x] K-R13 Same-path repository replacement between `pending_write` and recovery
  returns typed foreign-source conflict, quarantines attributable intent, and writes
  zero ledger bytes.
- [x] K-R14 Same-key replay under a same-path replacement returns typed foreign-source
  conflict and never reports the old repository's result as applied.
- [x] K-R15 Unix parent-sync and Windows write-through replacement capability probes
  gate apply; unsupported/failed probes return `AtomicDurabilityUnavailable` before
  reservation or ledger mutation.
- [x] K-R16 `durability_probe_writes_nothing_into_non_available_sources`: a whole-root
  filesystem spy over explicit-protected, read-only, ref, implicit-worktree, and
  memory-only first-apply attempts observes zero probe operations.
- [x] K-R17 `intent_journal_directory_durability_gates_apply`: when the ledger parent
  passes but the `ProjectStateDir` replay/intent-journal parent cannot meet the same
  durability contract, apply is typed unavailable before reservation.
- [x] K-R18 `curation_replay_after_intervening_commit_is_not_foreign`: after apply and
  one ordinary commit, same-key/same-hash replay returns stored success. Its non-Git
  variant edits one file and requires a retained catalog-lineage transition to replay
  stored success; a missing transition fails closed.
- [x] K-R19 `identical_replay_immediately_after_apply_matches_stored_binding`: the
  post-image policy digest cannot turn immediate terminal replay into a foreign-source
  conflict.
- [x] K-R20 `curation_recovery_after_intervening_commit_terminalizes_post_image`:
  crash after replace, then ordinary commit or identical-byte branch switch; recovery
  accepts same-repository continuity and terminalizes the exact post-image.

### GREEN

- [x] K-G01 Add preview-first `curate_knowledge` with canonical request hashing and
  explicit action/guard validation for one current project/source.
- [x] K-G02 Reuse resolved `ProjectStateDir` durable replay/mutation intent and one
  per-project lock; disable apply when replay or atomic durability is unavailable.
- [x] K-G03 Implement recoverable pre/post intent state, guarded temp write,
  `sync_all`, atomic replace, and the parent durability required by a tested platform
  contract, plus deterministic startup recovery/finalization; disable apply wherever
  that complete durability contract cannot be met. Use Unix parent-directory sync or
  Windows `FlushFileBuffers` plus write-through same-directory replacement, gated by
  last-evaluated first-use probes in both the ledger and replay/intent-journal
  directories plus platform crash tests.
- [x] K-G04 Revalidate the on-disk ledger and all source guards under the lock, then
  let the ordinary watcher publish the new immutable generation.
- [x] K-G05 Keep target documents immutable: no archive move, delete, or content edit;
  physical cleanup remains an evidence-only proposal requiring a later user action.
- [x] K-G06 Expose exact capability/replay/recovery reason codes in tool receipts and
  health without treating live-query readiness as persistence readiness.
- [x] K-G07 Bind durable replay and pending intent to verified `RepositoryId`/
  `SourceId` plus Git anchor-tip or non-Git root/catalog continuity. Keep manifest/
  policy digests as separate first-execution freshness guards; reject failed source
  continuity before ledger inspection, mutation, or stored-success replay.

### VERIFY

- [x] K-V01 Curation/idempotency/concurrency/capability suites are green.
- [x] K-V02 Crash injection at reservation, validation, temp sync, atomic replace,
  parent sync, and completion proves one complete ledger and one terminal receipt.
- [x] K-V03 No review/curation path moves, deletes, or edits a target document.

## Gate L — Worktrees and local refs

### RED

- [x] L-R01 Current worktree outranks divergent linked worktree/ref without changing
  document authority labels. (test `all_scope_lists_current_lane_before_ref_lanes`)
- [x] L-R02 Identical Git blob is parsed once with multiple source mappings.
  (test `identical_blob_is_parsed_once_across_same_classification_paths`)
- [x] L-R03 Ref movement invalidates old mappings deterministically. (tests
  `ref_movement_replaces_the_lane_and_bumps_registry`,
  `reconcile_deletion_pass_runs_despite_a_branch_publish_failure`)
- [x] L-R04 Giant Git blob is catalog-only without materialization. (tests
  `giant_blob_is_catalog_only_without_materialization`,
  `catalog_only_blob_bytes_are_not_materialized`)
- [x] L-R05 A process-spawn spy fails on any Git/LFS child process while offline
  local-ref ingestion proves no network fetch callback or object materialization.
  (tests `reconcile_ingests_offline_with_no_remote_configured` +
  `process_spawn_spy_confirms_offline_ingestion_never_shells_out` — plants canary
  git/git-lfs stubs on PATH/GIT_EXEC_PATH, runs the full offline ingest, asserts the
  lane published AND the canary never fired = pure libgit2 FFI, no child process. The
  spy is `#[ignore]` (run via `--ignored`, like the perf smokes) because it mutates
  process-global PATH; run isolated so `set_var` never races another test's env read.)
- [x] L-R06 Mixed-freshness all-source envelope reports each source publication/
  content generation, digest, coverage, review hash, and worst overall coverage.
  (multi-source envelope + P1 per-source generations `ref_tip_move_advances_lane_generations…`)
- [x] L-R07 Local-ref entry/blob/derived mapping budgets cannot block current-source
  readiness and cannot publish a false Complete ref scope. (tests
  `entry_budget_degrades_coverage…`, `degraded_scout_publishes_degraded_ref_manifest_coverage`)
- [x] L-R08 Bridge, authority, policy, state placement, cache, and review identities
  never cross worktree/ref/source boundaries. (tests `source_isolation_never_crosses…`,
  `source_isolation_holds_through_scoped_query_composition`; policy/state/cache identities
  are per-source by construction of the ref bundle builders.)
- [x] L-R09 Current/worktrees/local_refs/all filters, two-project selectors, and
  `projects=["*"]` compose deterministically from one captured source set per selected
  `ProjectInstance`; unavailable/empty/degraded source lanes retain typed outcomes.
  (`search_scoped`/`review_scoped` + cross-project `cross_project_*_source_scope_composes…`)
- [x] L-R10 The same Markdown/text/config blob produces lifecycle/extraction/secret/
  bridge/authority results identical to filesystem ingestion, except for explicit
  source and temporal-provenance differences. (secret-path parity fix +
  `sensitive_path_blob_is_withheld_by_path_even_when_content_is_clean`)
- [x] L-R11 A second session cannot address an explicit-protected worktree through a
  wildcard/ref mapping without its own direct exact override. (tests
  `l_r11_second_session_cannot_reach…`, `l_r11_tool_dispatch_blocks_wildcard…`)
- [x] L-R12 P1 bundle churn racing a long P0 build cannot force unbounded P0 retry or
  abort; the P0 commit fences only its own source and reaches readiness. (P0 fences on
  per-lane generations, not `registry_generation`; `publish_ref_source` leaves current byte-identical)
- [x] L-R13 Concurrent P0/P1 commits retain both updates. Every source-map change
  advances `registry_generation`, while a P1-only swap leaves current-worktree
  publication/content/project generations unchanged. (tests
  `p0_publishes_preserve_published_ref_lanes`, `publishing_and_removing_a_ref_source…`)
- [x] L-R14 The same object ID under Markdown and text/config paths shares raw bytes
  but re-derives classification-specific units, extraction, and secret outcome.
  (parse cache keyed by `(object_id, classification, language, is_tsx, is_c_header)`)

### GREEN

- [x] L-G01 Reuse worktree project ownership for checked-out sources. (reconcile excludes
  checked-out worktree HEADs + main HEAD from P1; fail-closed on unresolved HEAD)
- [x] L-G02 Add bounded local-ref tree/blob scout through existing `git2`. (`scout_local_ref`)
- [x] L-G03 Deduplicate immutable raw blob bytes by object ID; key parse/extraction by
  classification/route/extractor version and secret scans by path-policy inputs/
  policy version while retaining source-local identity/generation/policy mappings.
  (`materialize_ingest_blobs` dedup + `route_catalog_files` parse cache + sensitive-path rule)
- [x] L-G04 Route blobs through the shared scout/extraction/secret/bridge/authority
  adapters; do not create a second prose parser or search index. (`route_ref_blob`/`classify_ref_blob`)
- [x] L-G05 Reconcile ref/worktree topology and source mappings atomically.
  (`reconcile_local_ref_topology` + single-flight guard; gated production caller `spawn_local_ref_reconcile`)
- [x] L-G06 Add source filters/labels/per-source review envelopes and expand advertised
  source-scope capabilities only after their focused tests pass. (`search_scoped`/`review_scoped`,
  cross-project dispatch, advertised 4 scopes)
- [x] L-G07 Commit every source lane through the owning `ProjectInstance` publication
  lock, copying the current map under lock and replacing only the lane's source entry.
  (`publish_ref_source`/`remove_ref_source`/`next_after_current_publish`)

### VERIFY

- [x] L-V01 Worktree/ref/source-scope/parity focused tests are green. (full lib suite 2995/0/2)
- [x] L-V02 Current-only default latency/memory is measured and unchanged within the
  declared budget. (default path inert — `SYMFORGE_LOCAL_REF_LANES` OFF by default returns
  before any git/scout work; measurement `local_ref_gate_default_path_cost_is_negligible`
  (`#[ignore]` perf smoke) records the sole added cost — one env read — at ~133 ns/call,
  ~376x under the 50us tripwire; gate-OFF publishes no lane, registry unchanged —
  `local_ref_lanes_gate_off_publishes_no_ref_lane`)
- [x] L-V03 All-sources query/review remains bounded, deterministic, and source-local.
  (scoped composition tests; per-source captured set)
- [x] L-V04 Local-ref lane failure leaves current-worktree P0 Ready/queryable. (test
  `failed_ref_ingestion_leaves_the_current_lane_untouched`; reconcile errors never touch P0)

## Gate M — Health, corpus, surface, and release

- [x] M-001 Add health manifest/target/disposition/metadata/admitted/in-flight byte/
  coverage/retry, binding/session-membership, state-owner/placement, replay/
  persistence-capability, source-set, bridge, temporal, and authority-hygiene fields.
  (extends `health`/`health_compact` via `format::format_repository_knowledge_health[_compact]`;
  fields surfaced from published-generation data. authorization is a closed set
  {normal|explicit_protected} for every bound placement — derived from
  `StatePlacement::MemoryOnly.failures` (ProjectLocal failure => normal; only-UserLocal =>
  explicit_protected), NO SourceAccessMode plumbing (fixed at 9ee4df0 per Cursor's M-001 review).
  Accepted proxies (M-014): `retry` via freshness reason-codes + watcher reconcile-repairs counter
  (data-model excludes a retry counter from the digest); bridge "version" proxied by
  content_generation; in-flight = configured ceiling (live usage not retained past cold load).
  No new tool — surface stays 39/3.)
- [x] M-002 Assert terminal-disposition equality, no-partial-manifest budget behavior,
  unbound -> bound transition, per-session protected membership, and post-bind
  durability degradation independently from query readiness. (tests
  `m002_terminal_disposition_is_rendered_identically…`, `m002_budget_degraded_manifest_reports_bounded…`,
  `m002_health_reflects_unbound_to_bound_transition`, `m002_health_shows_per_session_protected_membership`,
  `m002_post_bind_durability_degradation_is_independent_of_query_readiness`)
- [x] M-003 Assert full surface is exactly 39 tools and compact surface remains 3;
  annotations/resources/prompts/catalog/docs match shipped behavior. (tests
  `surface_default::full_surface_advertises_exactly_39_tools` [added — pins full == tool_definitions()
  minus `symforge` facade == 39] + `surface_compact_resolves_compact_and_advertises_three_tools`;
  prompts `prompts::test_prompt_router_lists_expected_prompts`; resources/catalog
  `resources::test_resource_definitions_include_repo_surfaces` + `test_read_tools_catalog_resource`)
- [x] M-004 Run every quickstart scout/format/lifecycle/search/error/bridge/authority/
  root-state/curation-crash/worktree fixture; record exact results, token reductions,
  and degraded limitations. Corpus median returned tokens must be at least 50% below
  the recorded broad-discovery-plus-direct-read baseline. (≥50% MET: Gate I corpus test
  `search_knowledge::real_repository_corpus_returns_exact_deterministic_non_fixture_pointers`
  PASSED [fresh corpus median 945.5 vs recorded direct-read baseline median 3214.5 = ~70%
  reduction, larger vs the full broad-discovery+direct-read baseline]. DOCUMENTED LIMITATION:
  measurement-only — the harness emits the corpus-side token numbers + asserts determinism/
  pointer/trust, but the baseline and the ≥50% comparison are operator-computed; no CI oracle
  auto-fails on a future regression. The A019 token-surface shakedown 0/20 is a DIFFERENT
  experiment [compact-vs-full whole-task totals, oracle-design failure], not a token-economics failure.)
- [x] M-005 Run secret-safety scan/report by file:line only. (ran the repo's own
  `scan_secret_bytes`+`sensitive_path_rule` over all 954 tracked content files via a throwaway
  harness [deleted]: 0 path-rule/credential-file hits, 0 indeterminate; 30 content hits are all
  token-SHAPED strings in the scanner's own machinery, deliberate security-test canaries, bearer-token
  handling code, or doc curl examples — tree CLEAN. Scanner exposes only rule_id+count, never values.)
- [x] M-006 Prove policy mismatch forces re-scout/recompute and serialized snapshot,
  CCR, analytics, logs, diagnostics, review, and curation contain no runtime canary. (existing
  coverage: re-scout `persist::secret_policy_mismatch_forces_rescout_before_snapshot_ready`,
  CCR `ccr::knowledge_ccr_is_policy_tagged_and_mismatch_fails_closed`; no-canary in snapshot/CCR/
  analytics/diagnostics/review/curation each cited. "no canary in LOGS" holds by construction —
  sensitive bytes never become resident [`watcher::watcher_content_policy_withholds_sensitive_bytes_before_publication`],
  so nothing to log; a per-callsite log-capture test would be strictly weaker.)
- [x] M-007 Assert memory-only `checkpoint_now` is a successful typed
  `persistence_unavailable` result with `applied=false`, not an MCP/protocol error. (test
  `tools::tests::test_checkpoint_now_memory_only_is_typed_persistence_unavailable_not_error` — bound
  root, no state placement; asserts `persistence_unavailable`+`applied=false`, not the hard-error branch;
  the `-> String` handler makes "not an Err/protocol error" structural.)
- [x] M-008 Run `cargo fmt --check`. (green throughout; last confirmed at HEAD 40d6250)
- [x] M-009 Run `cargo check --features server`. (subsumed by clippy --all-targets --features server, clean)
- [x] M-010 Run `cargo clippy --all-targets --features server -- -D warnings`. (clean throughout, last at HEAD 40d6250)
- [x] M-011 Run focused suites and serial server all-target suite. (single clean run: 113 test binaries, 0 failed, 0 panics across lib + all integration targets at HEAD 40d6250; also a push-CI gate.)
- [x] M-012 Run exact embed gate. (GREEN: `cargo test --no-default-features --features embed
  --lib` = 1283 passed/0 failed (incl. the embed freshness regression test); was RED with 13
  compile errors. Watcher/protocol-dependent paths in `persist.rs`/`local_ref_scout.rs` gated
  behind `#[cfg(feature="server")]` — server byte-identical, embed = principled no-watcher mode.)
- [x] M-013 Run adversarial implementation review; resolve accepted blockers. (Cursor Gate-M/AAP
  review — briefs `GATE-M-REVIEW.md` + `gate-m-review-cursor-2026-07-24.md`; accepted blockers
  resolved: edit_plan ambiguous-path determinism + bare-symbol cascade [AAP-001], `from_path`
  parity [AAP-002], M-002/AAP-003 test strengthening. Additional cross-model pass — Kimi K3,
  `gate-m-review-kimi-k3-2026-07-24.md`: 1 blocker (embed `background_verify` freshness mislabel
  → false `Current`) FIXED (embed folds stat-changed/new into the mismatch set → `Degraded`) +
  regression-tested; all other risks cleared.)
- [x] M-014 Update `tasks/todo.md` review/evidence and measured limitations. (`tasks/todo.md`
  "Gate M evidence + measured limitations (M-014)"; accepted proxies for retry/bridge-version/
  in-flight documented; remaining measured limitation = M-004 token reduction is measurement-only.)
- [x] M-015 Verify every delegated worker is stopped and left no child process tree.
  (as_of 2026-07-24: `git worktree list` = main only, no stray worktrees; no orphaned
  cargo/rustc from any delegated agent; the running `symforge.exe`/`node.exe` are the live
  MCP daemon + Claude Code infrastructure, not agent child processes.)

## Dependencies

```text
A -> B -> C -> D -> E -> F -> G -> H -> I -> J -> K -> L -> M
```

Gate G establishes bridge evidence before Gate H derives authority. Gate I can then
filter/search that authority without a forward dependency; Gate J consumes both for
orientation and read-only review, and Gate K alone owns mutation. Safe parallel work
is limited to independent red fixtures/review after shared contracts freeze. Shared
domain/store/watcher/persist edits remain serialized.

<!-- END V10 HISTORICAL RECEIPTS -->

---

# Executable V11 tasks: Preventive project-index lifecycle

**Authority**: The V11 lifecycle-prevention design and the hash-pinned Feature 020
refreeze manifest govern this graph. Work follows RED -> observed failure -> minimal
GREEN -> focused verification. A versioned acceptance specification is not reported
as an executed test until the slice that introduces its production seam.

## Phase 1 — Refreeze prerequisite (implementation-blocking)

**Goal**: Freeze one internally consistent Feature 020 authority set before any V11
product code or Slice 0 oracle is added.

- [ ] T001 Inventory every file in `specs/020-repository-knowledge-index/` plus the bound `CONTEXT.md`, record its exact SHA-256 and authority classification, and create `specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md` with only the declared non-recursive self-hash exclusion
- [ ] T002 Map amendment IDs A01-A19 to every replaced clause hash and successor requirement, contract, task, and test ID, then compute the amendment-set ID as the domain-separated SHA-256 of canonical sorted records rather than an operator label in `specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md`
- [ ] T003 Apply A01-A19 consistently, create and complete the normative `contracts/lifecycle-oracle-traceability-v11.md`, `contracts/lifecycle-acceptance-oracles-v11.md`, and `contracts/v10-authority-retirement-v11.md` before inventory freeze, and record V11 as the breaking embed/lifecycle release boundary before Slice 0 across `specs/020-repository-knowledge-index/GOAL.md`, `specs/020-repository-knowledge-index/spec.md`, `specs/020-repository-knowledge-index/plan.md`, `specs/020-repository-knowledge-index/data-model.md`, `specs/020-repository-knowledge-index/tasks.md`, `specs/020-repository-knowledge-index/quickstart.md`, `specs/020-repository-knowledge-index/contracts/`, `specs/020-repository-knowledge-index/checklists/requirements.md`, and bound `CONTEXT.md` wherever its mapped authority lives
- [ ] T004 Mark all completed degraded-publication material as superseded historical evidence without changing its receipt bytes in `specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md`
- [ ] T005 [P] Define the canonical V11 public Interface, supported target/cfg/feature domain, and V10 keep/replace/remove matrix in `specs/020-repository-knowledge-index/contracts/public-api-v11.json`
- [ ] T006 Add generated all-cfg inventory, graph-cover, dependent-crate positive, and compile-fail fixtures for the allowlist in `tests/fixtures/public-api-v11-consumer/`
- [ ] T007 Write RED tests for unclassified files, unmapped clauses/requirements, hash drift, noncanonical amendment ordering, operator-label substitution, contradictory degraded language, unsupported cfgs, API expansion, coordinated in-tree digest rewrites, and missing FR/SC implementation-or-test traceability in `execution/test_refreeze_v11.py` and `scripts/validate-lifecycle-oracle-traceability.test.cjs`
- [ ] T008 Implement canonical sorted-record/domain-separated amendment-set recomputation plus manifest-aware replacement, API-allowlist, exact-hash, external-anchor validation, and the exhaustive frozen traceability checker until T007 is GREEN in `execution/refreeze_v11.py` and `scripts/validate-lifecycle-oracle-traceability.cjs`, with exact internal gates `python execution/refreeze_v11.py verify-internal --target-ref HEAD` and `node scripts/validate-lifecycle-oracle-traceability.cjs`
- [ ] T009 After every T003-T006 corpus/API edit is complete, regenerate all final file hashes, classifications, A01-A19 replacement mappings, and the amendment-set ID in `specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md`, then require a clean no-drift rerun before attestation
- [ ] T010 Pin the final T009 manifest, lifecycle-design, bound-context, amendment-set, and public-API digests in `docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md`; run the manifest validator, exhaustive lifecycle traceability checker, and cross-artifact analysis against those final bytes, record the exact bounded results, and rerun after the final attestation write without treating the mutable file as its own trust anchor
- [ ] T011 Freeze one candidate target commit/tree and obtain independent review; if any finding changes the corpus, preserve the review evidence, regenerate T001-T010, and repeat until the exact target is clear in `docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md`
- [ ] T012 Obtain a trusted signed append-only `RefreezeApprovalRecordV11` outside the repository that binds the exact T011 commit/tree, final detached-attestation digest, and trusted release identity; rerun `python execution/refreeze_v11.py verify-internal --target-ref HEAD`, then use `execution/refreeze_v11.py` to prove the external record accepts only that target and rejects any coordinated in-tree rewrite retaining it

> [!IMPORTANT]
> **HARD STOP — NO SLICE 0 OR PRODUCT CODE MAY BEGIN UNTIL T001-T012 ARE COMPLETE.**
> The external approval record is immutable input held outside the repository. It must
> never be fabricated, inferred from an in-tree file, or copied into this tree.
> After T012, every file under the Feature 020 root—including these checkbox bytes—is
> immutable. Record execution status and evidence under `docs/reviews/`; do not check
> boxes or edit a frozen contract. Any normative Feature 020 change requires a new
> manifest, attestation, exact-target review, and trusted external approval.

## Phase 2 — Slice 0: causal RED oracles and acceptance specifications

**Goal**: Preserve working positive controls for every known V10 defect and freeze
future-seam acceptance contracts without pretending that unimplemented tests ran.

- [ ] T013 Validate the frozen `contracts/lifecycle-oracle-traceability-v11.md` table against every `FR-001` through `FR-052` and `SC-001` through `SC-026`, then record the immutable table digest, bounds, fairness assumptions, inherited-test resolution, and intended Slice 0 CI artifacts in `docs/reviews/FEATURE-020-SLICE0-CAUSAL-ORACLES-v11.md` without modifying the frozen contract
- [ ] T014 Add and observe the smallest real-seam RED oracle `generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b` in `src/watcher/mod.rs::tests`, pausing after generation advance and before root publication; run exactly `cargo test --lib watcher::tests::generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b -- --exact --nocapture`
- [ ] T015 Add and observe RED positive controls for simultaneous first open, refusal without slot/watcher mutation, mutable-empty placeholder publication, and failed/panicked load retention in `tests/project_index_lifecycle_slice0.rs`
- [ ] T016 Add and observe RED positive controls for old-observer delivery after promotion, watcher mutation of a candidate, and observer replacement gaps in `tests/project_index_lifecycle_slice0.rs`
- [ ] T017 Add and observe RED positive controls for hybrid ArcSwap reads, generation-labeled live-disk bytes, same-stamp rewrites, and incomplete worktree derivations; pause a prepared source-A delta, publish source B, resume A, and prove latest B survives, same-source drift retries or aborts, equal numeric epochs cannot replace the opaque source-publication token, and exactly one whole-project root store occurs in `tests/project_index_lifecycle_slice0.rs`
- [ ] T018 Add and observe RED positive controls for same-path physical-root replacement, multi-loader close/rebind ordering, query/capacity starvation, charge conservation, raw embed bypass, and live V10 snapshot writers in `tests/project_index_lifecycle_slice0.rs`
- [ ] T019 Materialize RED test stubs from the frozen `contracts/lifecycle-acceptance-oracles-v11.md` mutation, ingress, observer, capacity, registry, query, provenance, verification, activation, embed, and migration specifications at their declared target slices without editing the frozen contract
- [ ] T020 Extend and run the pre-refreeze `scripts/validate-lifecycle-oracle-traceability.cjs` checker against Slice 0 execution evidence so it rejects a missing requirement row, implementation owner, executable-or-inherited test, positive-control result, a not-yet-runnable oracle mislabeled as executed, or an unmapped invariant; add a fail-closed `--require-materialized --evidence <release-evidence.json>` mode whose code-owned resolvers require every planned Rust case and benchmark registration to exist and every T078-T089 receipt to bind the same release tree
- [ ] T021 Run the exhaustive traceability checker and V10 positive-control commands, preserve expected failures as bounded CI artifacts, and obtain the required adversarial architecture review before Slice 1 in `docs/reviews/FEATURE-020-SLICE0-CAUSAL-ORACLES-v11.md`

## Phase 3 — Slice 1: atomic mutation authority

**Goal**: Make cross-root mutation and publication impossible before introducing the
larger lifecycle runtime.

- [ ] T022 Write and observe RED authority tests for exact whole-authority validation, mutation-permit terminality, epoch mismatch, and root-A writes after root-B install; add the grant-provenance matrix that accepts only a consumed exact live-`Current` authority and rejects `Loading`, `Refreshing`, `Blocked`, `Stopping`, candidate, snapshot, retained-generation, and stale-publication inputs without advancing the mutation epoch or creating a permit record in `tests/project_index_authority_v11.rs`
- [ ] T023 Write and observe RED primitive tests for no-follow handle-relative I/O, symlink/reparse escapes, replacement before temp creation, and a mutation permit refusing `start_side_effect` unless its exact grant has already published non-Current in `tests/physical_root_lease_v11.rs` and `tests/project_index_authority_v11.rs`; production writer integration remains a Slice 4 activation test
- [ ] T024 Define `BindingAuthority`, `ObserverToken`, `CandidateAuthority`, generation-bound `MutationAuthority`, and sealed `CurrentMutationGrantAuthority` consumable only from an exact live-`Current` publication, with never-reused identity and checked exhaustion in `src/index_lifecycle/authority.rs`
- [ ] T025 Implement owning `PhysicalRootLease` plus beneath-confined, handle-relative destructive I/O until T023 is GREEN in `src/index_lifecycle/physical_root.rs`
- [ ] T026 Implement non-cloneable mutation permits whose grant consumes `CurrentMutationGrantAuthority`, with grant/start/commit/no-side-effect/drop terminal paths until T022 is GREEN in `src/index_lifecycle/mutation.rs`
- [ ] T027 Implement writer-validated Freeze -> Drain -> Install for reload, rebind, and physical-root replacement in `src/index_lifecycle/transition.rs`
- [ ] T028 Replace separate-field fence inference at the watcher/store mutation seam with the whole mutation authority in `src/watcher/mod.rs` and `src/live_index/store.rs`
- [ ] T029 Run the Slice 1 focused tests, record RED-to-GREEN evidence and impact analysis, and complete the post-slice adversarial code review in `docs/reviews/FEATURE-020-SLICE1-EVIDENCE-v11.md`

## Phase 4 — Slice 2: registry tombstones and process-wide capacity

**Goal**: Establish single-flight admission, non-revivable close/reopen, and exact
process memory ownership before retained-plus-candidate overlap exists.

- [ ] T030 Write and observe RED registry tests for pending admission, concurrent join, protected-membership refusal, late-grant refund, close/reopen coalescing, process shutdown races, and SC-019 authorized protected roots selecting user-local/memory-only placement and reaching `PendingProjectAdmission` with zero state or durability-probe I/O below the source root; Slice 2 does not construct or claim lifecycle `Current` in `tests/project_registry_lifecycle_v11.rs`
- [ ] T031 Write and observe RED embedded-source tests for one handle/close authority, `SourceAlreadyOpen`, close/Drop coalescing, final-owner shutdown, and `WouldSelfWait` in `tests/embed_lifecycle_v11.rs`
- [ ] T032 Write and observe RED capacity tests for fixed safety precharge, oldest-satisfiable scheduling, drain barriers, resize cleanup-before-requeue, detached owners, and exact conservation in `tests/process_capacity_pool_v11.rs`
- [ ] T033 Implement `PendingProjectAdmission`, `LiveProjectSlot`, stopping tombstones, never-reused slot identity, and atomic install/cancel transfer in `src/index_lifecycle/registry.rs`
- [ ] T034 Implement the shared process runtime, persistent factory-incarnation registry, and stable capacity domain for daemon, stdio, serve, and embed in `src/index_lifecycle/process_runtime.rs`
- [ ] T035 Implement hierarchical capacity owners, immutable grants, allocation construction guards, charged residency groups, and query-response reservation in `src/index_lifecycle/capacity.rs`
- [ ] T036 Implement out-of-lock dispatch, revocation/refund, oldest-satisfiable drain barriers, pin-aware parking, replacement headroom, and cleanup-before-requeue resize until T032 is GREEN in `src/index_lifecycle/capacity.rs`
- [ ] T037 Implement the internal embedded registration, sole-handle ownership, close receipt, and independent finalizer foundation behind production-unreachable constructors until T031 is GREEN in `src/index_lifecycle/embedded.rs`, leaving the V10 public embed lane unchanged
- [ ] T038 Wire pending admission, fixed revocation-package charging, and SC-019 protected-root state placement with no source-local state/durability probe into dark daemon/stdio/serve adapters in `src/index_lifecycle/adapters.rs` without replacing any V10 production admission path before Slice 4
- [ ] T039 Prove refusal or cancellation cannot construct `Current`, leak a slot, double-refund, or release live blocking memory in `tests/project_registry_lifecycle_v11.rs` and `tests/process_capacity_pool_v11.rs`
- [ ] T040 Run the Slice 2 focused and model tests, record accounting/identity evidence, and complete the post-slice adversarial code review in `docs/reviews/FEATURE-020-SLICE2-EVIDENCE-v11.md`

## Phase 5 — Slice 3: behavior-neutral seams, provenance, and dark runtime

**Goal**: Type every response authority and build the preventive runtime behind
production-unreachable constructors without changing V10 behavior.

- [ ] T041 Write and observe RED claim-attribution and `OperationContractV1` Cartesian-negative tests for `Generation`, `DiskObservation`, `WorktreeScopeObservation`, `GitObservation`, `Comparison`, n-ary `Derivation`, `SelectedAggregate`, `EvaluationProvenance`, every typed refusal basis/retry/status combination, and `KnowledgeVoiceFilter` never selecting consistency; add compile-fail/private-constructor cases proving `OutputCoverage::Truncated` cannot exist before a completed strict lease and cannot enter candidate, attempt, cache, CCR, or persistence identity in `tests/claim_provenance_v11.rs`
- [ ] T042 Write and observe RED cross-authority tests proving generation mode never reads unmatched disk bytes; `DiskObservation::PathMissing` proves only path-local absence at its observation time, complete `WorktreeScopeObservation` only its sealed scope/interval, and `GitObservation::NotInTree` only the exact object tree; none proves generation/repository-wide absence, rebinds refuse mixed-root derivations, and a failed pure observation returns typed refusal while preserving `Current` unless it independently proves lifecycle invalidation through the observer seam in `tests/read_gate_authority_v11.rs`
- [ ] T043 Define sealed `OperationReceipt`, `ClaimContext`, provenance, scope-certificate, selection-receipt, typed `SourceRefusal`, and post-lease-only `OutputCoverage` constructors; require a sealed completed-lease render authority to construct `OutputCoverage::Truncated` in `src/protocol/claim_provenance.rs`
- [ ] T044 Split generation-byte resolution from beneath-confined disk observation and make authority choice explicit in `src/protocol/read_gate.rs`
- [ ] T045 Migrate raw fallback, untracked search, validation, each diff mode, worktree impact, text/structured formatting, cache, CCR, persistence, and retrieval through typed provenance in `src/protocol/`
- [ ] T046 Consolidate legacy production reads on one captured published source set without naming any V10 fact product `Current` in `src/live_index/view.rs`
- [ ] T047 Implement the closed `SourceRuntimeState`, immutable project runtime root, lifecycle supervisor, strict lease constructors, and V11 `EmbeddedSourceHandle` behind a dark-only factory in `src/index_lifecycle/runtime.rs`
- [ ] T048 Implement the attested V11 replacement items behind production-unreachable seams and generate the exact future export delta in `src/index_lifecycle/public_api.rs`; do not remove, replace, expose, or widen any live V10 `src/lib.rs` export in Slice 3
- [ ] T049 Run the generated all-cfg inventory plus dependent-crate positive/compile-fail fixtures against the dark API adapter and record the future activation consumer result in `docs/reviews/AAP-MIGRATION-RECEIPT-v11.md` without claiming the V11 exports are live
- [ ] T050 Generate the failing `tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch` reachability test from the frozen `contracts/v10-authority-retirement-v11.md` inventory and prove every V10 writer, callback, publication root, cache/CCR lane, snapshot path, tool, resource, prompt, sidecar/hook query/freshening/finalization lane, and raw embed bypass has an exact Slice 4 owner; the matrix must distinguish `GenerationLeased`, `DiskObserved`, `WorktreeScopeObserved`, `GitObserved`, `RuntimeHealthObserved`, `MutationPermitted`, `StateWriteAuthorized`, and `Refused` without modifying the frozen inventory
- [ ] T051 Prove all Slice 3 preventive constructors remain unreachable from daemon, stdio, serve, embed, snapshot, observer, and mutation entry points in `tests/preventive_runtime_dark_v11.rs`
- [ ] T052 Run the Slice 3 provenance round trips, cfg matrix, public-API harness, and unchanged-V10 behavior gates, then complete the post-slice adversarial code review in `docs/reviews/FEATURE-020-SLICE3-EVIDENCE-v11.md`

## Phase 6 — Slice 4: candidate, invalidation, delta, and activation (indivisible)

**Goal**: Enable preventive lifecycle everywhere in one cut. No merge or release may
ship a refusal-per-edit full-rebuild phase, a legacy fallback, or mixed authority.

- [ ] T053 [P] Write and observe RED candidate tests for isolated build, publish-before-prune, retry supersession, and the closed promotion matrix: `Unreadable`, `UnstableDuringRead`, `AbortedCircuitBreaker`, `ParseStatus::Failed`, unknown ordering, truncated required derivations, and `PartialParse` block promotion; add exact `tests/index_candidate_lifecycle_v11.rs::opaque_non_utf8_path_identity_is_lossless`, proving lossy-display collisions retain distinct stable native identities, remain catalog-only with zero content probes, and never persist a lossy spelling; metadata-terminal exclusions remain complete; capability certificates cannot authorize partial promotion; failed/panicked candidates are discarded
- [ ] T054 [P] Write and observe RED observer tests for stable-token cuts, gap latching, predecessor drain, post-barrier baseline, ingress unwind retention, and exhausted-capacity safety transitions in `tests/observer_handoff_v11.rs`
- [ ] T055 [P] Write and observe RED verification tests for scope discovery, entry obligations, same-stamp rewrites, the exact 15-minute monotonic deadline boundary (just-before remains eligible; at/after latches `VerificationOverdueLatched` before strict acquisition), partial/cancelled/resumed work never extending the deadline, overdue acquisition refusal, fair resumable rolling passes, fenced proof refresh, and every policy-version mismatch forcing non-Current authoritative re-scout before any new `Current` promotion in `tests/rolling_verification_v11.rs`
- [ ] T056 [P] Write and observe RED strict-query tests for atomic multi-source capture, empty/missing/extra/mismatched `SelectedAggregate` rejection, exact selected-source bijection, no-match only when every selected source is `Current`, stale finalization, retarget races, post-lease rendering that may add `OutputCoverage::Truncated` only after a complete strict lease without changing source-truth/candidate/cache/CCR identity, SC-019 authorized protected roots reaching `Current` only after full candidate promotion with zero state/durability-probe I/O below the source root, and committed-generation versus bounded-attempt accounting across health, health_compact, status, and health resources in `tests/project_query_lease_v11.rs`
- [ ] T057 [P] Write and observe RED snapshot tests for untrusted V10 seeds, pre-decode capacity, root/digest mismatch, quarantine, rollback, concurrent V10 writers, `.symforge/v11/` namespace isolation, and runtime secret-canary bytes never entering snapshots, quarantine metadata, receipts, or diagnostics in `tests/snapshot_v11_migration.rs`
- [ ] T058 [P] Write and observe RED activation tests for one process mode, legacy-gate drain, cache/CCR invalidation, response finalization, raw-embed retirement, and never-simultaneous publication roots; prove cold/restart curation recovery cannot mint a permit and stays read-only until `Current`, exact post-image receipt finalization and excluded team-artifact state writes remain `ProjectStateDir`-only without a permit, the FR-051 `already_tracked`/`untracked_visible`/`ignored_force_add_required`/`git_visibility_unavailable` receipt-and-refusal matrix is exact, and every pre-image retry/probe/cleanup and init/root-ignore/`.gitattributes`/hygiene/curation source write refuses unless a fresh `SourceMutationPermit` already published non-Current in `tests/activation_cut_v11.rs` plus focused `src/cli/init.rs` and persistence tests
- [ ] T059 Move loader ownership, cancellation, attempt accounting, classified failure, and retry triggers into the per-source supervisor in `src/index_lifecycle/supervisor.rs`
- [ ] T060 Implement capacity-reserved isolated full and delta candidates with complete artifact certificates and one runtime-store commit point; preserve `CatalogPath` native/opaque identity through scout, candidate, manifest, and promotion without lossy reconstruction; every prepared source delta exact-validates only its changed source token and no-allocation patches the latest whole project root so unrelated newer membership/source siblings survive, while same-source drift retries or aborts and numeric epochs never authorize publication, in `src/index_lifecycle/candidate.rs`
- [ ] T061 Implement the bounded coalescing accumulator, monotonic invalidation cuts, scope-dirty/gap latches, stable observer handoff, and full successor baseline in `src/index_lifecycle/observer.rs`
- [ ] T062 Implement racy-clean entry obligations, scope-discovery deadlines, resumable rolling verification, immutable proof refresh, and the exact FR-049 monotonic overdue predicate: only a complete exact-identity whole-scope `VerificationRecord` advances the fixed 15-minute deadline, while deadline expiry atomically latches non-Current before any strict lease in `src/index_lifecycle/verification.rs`
- [ ] T063 Implement project/single-source strict leases, exact multi-project selections, separate ranking snapshots, sealed completed-lease render authority and post-lease `OutputCoverage`, `SourceRefusal` transport mapping, and committed-generation-versus-attempt health projections in `src/index_lifecycle/query.rs`, `src/live_index/health_view.rs`, and `src/protocol/`
- [ ] T064 Route external watcher observations, targeted and sidecar/hook freshening, temporal, bridge, authority, local-ref, and derived observations directly through the isolated candidate pipeline without a mutation permit; route only SymForge-owned structural edit/curation, init/root-ignore/`.gitattributes`/hygiene source-byte writes through a fresh `SourceMutationPermit` that first publishes non-Current and then returns through the isolated candidate pipeline; keep non-Current recovery read-only until `Current` and keep exact post-image receipt finalization state-only in `src/watcher/mod.rs`, `src/sidecar/`, `src/cli/init.rs`, `src/cli/hook.rs`, `src/protocol/edit.rs`, `src/protocol/edit_hooks.rs`, and `src/live_index/`
- [ ] T065 Bump the snapshot format and implement bounded untrusted-seed restore, complete re-observation, quarantine, atomic V11 replacement, preserved rollback, rebuild fallback, and excluded team-artifact bytes/metadata as `ProjectStateDir` persistence-only without source mutation authority; implement the exact FR-051 `already_tracked`/`untracked_visible`/`ignored_force_add_required`/`git_visibility_unavailable` receipt-and-refusal matrix, and route only a companion in-scope `.gitattributes` change through T064's permit path in `src/live_index/persist.rs`
- [ ] T066 Implement `LegacyOpen -> LegacyClosing -> PreventiveV1Open`, register every tool/resource/prompt query, cache/CCR/retrieval, sidecar/hook, and finalization lane, and make mode selection process-wide and non-configurable in `src/index_lifecycle/activation.rs`
- [ ] T067 In the same activation change, expose only the attested V11 replacement API and `EmbeddedSourceHandle`, then retire every inventoried V10 constructor, writer, callback, secondary publication root, legacy fallback, tool/resource/prompt handler bypass, sidecar/hook bypass, and raw embed update/remove export in `src/daemon.rs`, `src/main.rs`, `src/sidecar/`, `src/cli/hook.rs`, `src/embed.rs`, and `src/lib.rs`
- [ ] T068 Make `ObservedRefreshGateV1` executable as the registered `benches/observed_refresh_gate_v1.rs::observed_refresh_gate_v1` benchmark with fixed add/modify/delete/rename/terminal-classification and burst workloads; daemon/stdio/serve managed-observer-plus-authoritative-poll and embed-contract trigger matrices; delivered-event, gap/need-rescan, and suppressed-notification campaigns; exact completed-write-burst or SymForge-mutation-commit to the first strict lease carrying that byte identity; corpora digests, host/cache/quiescence controls, completion receipts, pre-granted capacity vector plus scratch/headroom, and clean-rebuild equivalence in `tests/fixtures/observed-refresh-v1/`; emit the code-owned benchmark receipt consumed by release materialization validation
- [ ] T069 Add retained-plus-candidate peak accounting, burst convergence, capacity fairness, and no-unaccounted-residency measurements in `tests/process_capacity_pool_v11.rs` and `benches/observed_refresh_gate_v1.rs`
- [ ] T070 Run `ObservedRefreshGateV1` against baseline `1521abb0` and the candidate with p95 <=2 seconds, maximum <=5 seconds, p95 <=1.25x baseline, no single-path full rebuild outside Gap/ScopeDirty, and record exact results in `docs/reviews/OBSERVED-REFRESH-GATE-v1.md`
- [ ] T071 Prove every advertised edit class has the same canonical manifest, required artifact digests, and representative query results as a clean full rebuild in `tests/delta_full_rebuild_equivalence_v11.rs`
- [ ] T072 Run the indivisible activation campaign across daemon, stdio, serve, embed, every tool/resource/prompt handler, sidecar/hook query/freshening/finalization, snapshot, observer, mutation, local-ref, derived, cache, CCR, and retrieval through `all_ingress_uses_exact_typed_authority_branch`; include the exact four-state FR-051 team-artifact receipt/refusal matrix, then complete the post-slice adversarial code review in `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md`
- [ ] T073 Update the breaking lifecycle/embed migration boundary, removed exports, replacement APIs, and rollback constraints in `docs/migrations/v11-index-lifecycle.md`

## Phase 7 — Slice 5: mechanical removal

**Goal**: Delete only code already proven unreachable in Slice 4; do not change runtime
authority, public behavior, writer reachability, or activation mode.

- [ ] T074 Capture a pre-cleanup public API, authority-reachability, behavior, and activation baseline in `docs/reviews/FEATURE-020-SLICE5-BASELINE-v11.md`
- [ ] T075 Remove unreachable placeholder storage, bootstrap/circuit-breaker lifecycle fields, legacy mode branches, secondary publication roots, obsolete tests, and compatibility comments in `src/`
- [ ] T076 Remove dead V10 embed implementation only after the allowlist negative suite proves it unnameable in `src/embed.rs`
- [ ] T077 Re-run the T074 baseline, prove Slice 5 changed no runtime authority, public behavior, writer reachability, or activation result, and complete the post-slice adversarial code review in `docs/reviews/FEATURE-020-SLICE5-EVIDENCE-v11.md`

## Phase 8 — Release and adversarial closure

**Goal**: Land only after causal, model, performance, memory, provenance, migration,
surface, and operational gates all pass on one frozen tree.

- [ ] T078 Run formatting and Clippy with warnings denied and record exact commands/results in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T079 Run focused lifecycle, capacity, watcher, snapshot, provenance, embed, migration, and activation suites in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T080 Add and run four separate pure proptest command models in `tests/model/`, four separate TLA+ specifications for process ownership, registry identity, source promotion/invalidation, and capacity admission in `formal/v11/`, and the shared production transition kernel through its `cfg(loom)` adapter in `src/index_lifecycle/loom_tests.rs`, with adjacent-interface assumptions, bounds, fairness, and traceability recorded
- [ ] T081 Run the serial all-target suite, release build, canonical full/compact tool, resource, and prompt fixtures, and the SC-006 representative-workflow token comparator with at least 50% median reduction as a hard gate in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T082 Run cold-start race, same-stamp/suppressed-notification, rolling-deadline, observer-handoff, and root-replacement campaigns with working positive controls in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T083 Run measured concurrent-project memory coverage for retired query-pinned generations, retained-plus-candidate overlap, snapshot scratch, and accumulators in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T084 Run the complete provenance/refusal matrix through text, structured, HTTP, cache, CCR, persistence, and retrieval, including `OperationContractV1` Cartesian negatives, exact-bijection `SelectedAggregate` cases, cross-query/filter/consistency cache-confusion negatives, equal-shape nonexistent-versus-unauthorized `InvalidSelection`, `KnowledgeVoiceFilter`-not-consistency proofs, separate `RankingSnapshot` identity/order algebra, post-lease-only `OutputCoverage::Truncated` round trips that cannot alter candidate/cache/CCR identity, and runtime secret-canary plus policy-mismatch campaigns across output, logs, analytics, diagnostics, cache/CCR, snapshots, persistence, retrieval, review, and curation that report only rule IDs and file:line locations in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T085 Run same-process activation and restart campaigns seeded with apparently valid V10 cache records, CCR handles, snapshots, and live legacy writers in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T086 Run the generated V11 public-API allowlist, all-cfg graph cover, dependent-crate fixtures, and unknown-configuration rejection in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T087 Run the secret-safety scan reporting only rule IDs and file:line locations, never values, in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T088 Freeze the exact release commit/tree and all gate digests, obtain an independent adversarial review, and resolve every accepted P0/P1/P2 in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T089 Re-run `execution/refreeze_v11.py` with the trusted external approval record, prove the approved refreeze remains the immutable ancestor of the release tree, and assemble the canonical same-tree T078-T089 execution receipt at `target/ci/lifecycle-v11/release-evidence.json` for `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`
- [ ] T090 Run exactly `node scripts/validate-lifecycle-oracle-traceability.cjs --require-materialized --evidence target/ci/lifecycle-v11/release-evidence.json` on the final frozen release tree and record the clear-to-land decision only when every planned Rust case and benchmark is materialized, every requirement row plus T078-T089 is green, and every receipt binds that same tree in `docs/reviews/FEATURE-020-V11-RELEASE-GATE.md`

## V11 dependencies

```text
T001 -> T002 -> T003/T004
T005 -> T006
T003/T004/T006 -> T007 -> T008 -> final manifest regeneration T009 -> T010 -> T011 -> T012
HARD STOP: T012 -> Slice 0
Slice 0 (T013-T021) -> Slice 1 (T022-T029) -> Slice 2 (T030-T040)
Slice 2 -> Slice 3 (T041-T052) -> indivisible Slice 4 (T053-T073)
Slice 4 -> mechanical Slice 5 (T074-T077) -> release closure (T078-T090)
```

Parallel work is restricted to tasks marked `[P]`. Shared registry, lifecycle,
capacity, watcher, persistence, activation, and public-export edits remain serialized.
Slice 4 is one enablement unit: no subset is independently shippable.

## Independent acceptance by feature story

- **US1 / safety**: one pathological observation blocks only candidate promotion; it never publishes partial, stale, mixed, or false-current state.
- **US2 / trust**: promoted manifests are total and attempt diagnostics cannot masquerade as committed dispositions.
- **US3 / bounded retrieval**: success and no-match require an exact all-`Current` selection; otherwise the response is typed refusal.
- **US4 / convergence**: every edit class converges through bounded deltas under `ObservedRefreshGateV1` without a refusal-per-edit availability cliff.
- **US5 / recovery**: restart treats legacy/snapshot bytes as untrusted seeds and promotes only after complete current-process proof.
- **US6 / temporal scope**: Git, generation, disk, and worktree authorities remain explicit and root-compatible across comparisons.
- **US7 / security**: protected/sensitive selectors and disk observations cannot leak bytes, identities, or generation endorsement.
- **US8 / first contact**: orientation is available only from complete `Current` sources; a mixed selection refuses with exact evidence.
- **US9 / authority hygiene**: retrieval voice never selects consistency, and stale documents cannot acquire generation authority.

## V11 implementation strategy

The safety MVP is the refreeze plus Slices 0-2: it closes the proven cross-root and
single-flight/capacity foundations while leaving production on V10 authority. Slice 3
adds behavior-neutral typed seams and a dark runtime. Slice 4 is the only activation
point and must include deltas, verification, capacity, provenance, embed migration,
public API retirement, and `ObservedRefreshGateV1` together. Slice 5 is deletion only.
