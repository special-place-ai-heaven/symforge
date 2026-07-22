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

- [ ] K-R01 Preview writes/reserves nothing; apply accepts explicit action IDs and
  exactly one current-worktree review hash plus fresh manifest/policy/target guards,
  and mutates only `.symforge-knowledge.toml`.
- [ ] K-R02 Identical idempotency replay returns stored terminal success before now-
  stale freshness guards; same key/different canonical request conflicts.
- [ ] K-R03 Concurrent curators serialize under one per-project policy mutation lock
  and revalidate policy/manifest/document/action guards immediately before write.
- [ ] K-R04 Secret-positive input rejects before routing, echo, logging, idempotency
  reservation, evidence construction, temp write, or receipt.
- [ ] K-R05 Explicit-protected/read-only/user-local-without-durable-replay/memory-
  only/ref/implicit-worktree sources expose a reason-bearing unavailable capability
  before probe evaluation, with zero probe file operations beneath the source root.
- [ ] K-R06 Crash after durable intent reservation recovers one pending request and
  never mutates the ledger without completed validation.
- [ ] K-R07 Crash after validation but before temp-file sync leaves the previous
  complete ledger and a deterministically recoverable request.
- [ ] K-R08 Crash after temp-file `sync_all` but before atomic replace leaves either
  the old complete ledger or a recoverable validated temp, never partial policy.
- [ ] K-R09 Crash after atomic replace/parent-directory durability but before
  completion recording detects the exact post-state and terminalizes the request
  without applying it twice.
- [ ] K-R10 Crash after completion recording replays the stored result; startup
  recovery quarantines/blocks indeterminate state rather than guessing success.
- [ ] K-R11 Successful apply triggers ordinary watcher/reconciliation publication;
  the receipt reports applied/pending generation, an already captured reader keeps
  its old generation, and a later reader sees the new policy/voice atomically.
- [ ] K-R12 Move/delete/schema-invalid action, stale review/policy/manifest/target,
  unknown action, or any mixed batch failure causes zero policy mutation.
- [ ] K-R13 Same-path repository replacement between `pending_write` and recovery
  returns typed foreign-source conflict, quarantines attributable intent, and writes
  zero ledger bytes.
- [ ] K-R14 Same-key replay under a same-path replacement returns typed foreign-source
  conflict and never reports the old repository's result as applied.
- [ ] K-R15 Unix parent-sync and Windows write-through replacement capability probes
  gate apply; unsupported/failed probes return `AtomicDurabilityUnavailable` before
  reservation or ledger mutation.
- [ ] K-R16 `durability_probe_writes_nothing_into_non_available_sources`: a whole-root
  filesystem spy over explicit-protected, read-only, ref, implicit-worktree, and
  memory-only first-apply attempts observes zero probe operations.
- [ ] K-R17 `intent_journal_directory_durability_gates_apply`: when the ledger parent
  passes but the `ProjectStateDir` replay/intent-journal parent cannot meet the same
  durability contract, apply is typed unavailable before reservation.
- [ ] K-R18 `curation_replay_after_intervening_commit_is_not_foreign`: after apply and
  one ordinary commit, same-key/same-hash replay returns stored success. Its non-Git
  variant edits one file and requires a retained catalog-lineage transition to replay
  stored success; a missing transition fails closed.
- [ ] K-R19 `identical_replay_immediately_after_apply_matches_stored_binding`: the
  post-image policy digest cannot turn immediate terminal replay into a foreign-source
  conflict.
- [ ] K-R20 `curation_recovery_after_intervening_commit_terminalizes_post_image`:
  crash after replace, then ordinary commit or identical-byte branch switch; recovery
  accepts same-repository continuity and terminalizes the exact post-image.

### GREEN

- [ ] K-G01 Add preview-first `curate_knowledge` with canonical request hashing and
  explicit action/guard validation for one current project/source.
- [ ] K-G02 Reuse resolved `ProjectStateDir` durable replay/mutation intent and one
  per-project lock; disable apply when replay or atomic durability is unavailable.
- [ ] K-G03 Implement recoverable pre/post intent state, guarded temp write,
  `sync_all`, atomic replace, and the parent durability required by a tested platform
  contract, plus deterministic startup recovery/finalization; disable apply wherever
  that complete durability contract cannot be met. Use Unix parent-directory sync or
  Windows `FlushFileBuffers` plus write-through same-directory replacement, gated by
  last-evaluated first-use probes in both the ledger and replay/intent-journal
  directories plus platform crash tests.
- [ ] K-G04 Revalidate the on-disk ledger and all source guards under the lock, then
  let the ordinary watcher publish the new immutable generation.
- [ ] K-G05 Keep target documents immutable: no archive move, delete, or content edit;
  physical cleanup remains an evidence-only proposal requiring a later user action.
- [ ] K-G06 Expose exact capability/replay/recovery reason codes in tool receipts and
  health without treating live-query readiness as persistence readiness.
- [ ] K-G07 Bind durable replay and pending intent to verified `RepositoryId`/
  `SourceId` plus Git anchor-tip or non-Git root/catalog continuity. Keep manifest/
  policy digests as separate first-execution freshness guards; reject failed source
  continuity before ledger inspection, mutation, or stored-success replay.

### VERIFY

- [ ] K-V01 Curation/idempotency/concurrency/capability suites are green.
- [ ] K-V02 Crash injection at reservation, validation, temp sync, atomic replace,
  parent sync, and completion proves one complete ledger and one terminal receipt.
- [ ] K-V03 No review/curation path moves, deletes, or edits a target document.

## Gate L — Worktrees and local refs

### RED

- [ ] L-R01 Current worktree outranks divergent linked worktree/ref without changing
  document authority labels.
- [ ] L-R02 Identical Git blob is parsed once with multiple source mappings.
- [ ] L-R03 Ref movement invalidates old mappings deterministically.
- [ ] L-R04 Giant Git blob is catalog-only without materialization.
- [ ] L-R05 A process-spawn spy fails on any Git/LFS child process while offline
  local-ref ingestion proves no network fetch callback or object materialization.
- [ ] L-R06 Mixed-freshness all-source envelope reports each source publication/
  content generation, digest, coverage, review hash, and worst overall coverage.
- [ ] L-R07 Local-ref entry/blob/derived mapping budgets cannot block current-source
  readiness and cannot publish a false Complete ref scope.
- [ ] L-R08 Bridge, authority, policy, state placement, cache, and review identities
  never cross worktree/ref/source boundaries.
- [ ] L-R09 Current/worktrees/local_refs/all filters, two-project selectors, and
  `projects=["*"]` compose deterministically from one captured source set per selected
  `ProjectInstance`; unavailable/empty/degraded source lanes retain typed outcomes.
- [ ] L-R10 The same Markdown/text/config blob produces lifecycle/extraction/secret/
  bridge/authority results identical to filesystem ingestion, except for explicit
  source and temporal-provenance differences.
- [ ] L-R11 A second session cannot address an explicit-protected worktree through a
  wildcard/ref mapping without its own direct exact override.
- [ ] L-R12 P1 bundle churn racing a long P0 build cannot force unbounded P0 retry or
  abort; the P0 commit fences only its own source and reaches readiness.
- [ ] L-R13 Concurrent P0/P1 commits retain both updates. Every source-map change
  advances `registry_generation`, while a P1-only swap leaves current-worktree
  publication/content/project generations unchanged.
- [ ] L-R14 The same object ID under Markdown and text/config paths shares raw bytes
  but re-derives classification-specific units, extraction, and secret outcome.

### GREEN

- [ ] L-G01 Reuse worktree project ownership for checked-out sources.
- [ ] L-G02 Add bounded local-ref tree/blob scout through existing `git2`.
- [ ] L-G03 Deduplicate immutable raw blob bytes by object ID; key parse/extraction by
  classification/route/extractor version and secret scans by path-policy inputs/
  policy version while retaining source-local identity/generation/policy mappings.
- [ ] L-G04 Route blobs through the shared scout/extraction/secret/bridge/authority
  adapters; do not create a second prose parser or search index.
- [ ] L-G05 Reconcile ref/worktree topology and source mappings atomically.
- [ ] L-G06 Add source filters/labels/per-source review envelopes and expand advertised
  source-scope capabilities only after their focused tests pass.
- [ ] L-G07 Commit every source lane through the owning `ProjectInstance` publication
  lock, copying the current map under lock and replacing only the lane's source entry.

### VERIFY

- [ ] L-V01 Worktree/ref/source-scope/parity focused tests are green.
- [ ] L-V02 Current-only default latency/memory is measured and unchanged within the
  declared budget.
- [ ] L-V03 All-sources query/review remains bounded, deterministic, and source-local.
- [ ] L-V04 Local-ref lane failure leaves current-worktree P0 Ready/queryable.

## Gate M — Health, corpus, surface, and release

- [ ] M-001 Add health manifest/target/disposition/metadata/admitted/in-flight byte/
  coverage/retry, binding/session-membership, state-owner/placement, replay/
  persistence-capability, source-set, bridge, temporal, and authority-hygiene fields.
- [ ] M-002 Assert terminal-disposition equality, no-partial-manifest budget behavior,
  unbound -> bound transition, per-session protected membership, and post-bind
  durability degradation independently from query readiness.
- [ ] M-003 Assert full surface is exactly 39 tools and compact surface remains 3;
  annotations/resources/prompts/catalog/docs match shipped behavior.
- [ ] M-004 Run every quickstart scout/format/lifecycle/search/error/bridge/authority/
  root-state/curation-crash/worktree fixture; record exact results, token reductions,
  and degraded limitations. Corpus median returned tokens must be at least 50% below
  the recorded broad-discovery-plus-direct-read baseline.
- [ ] M-005 Run secret-safety scan/report by file:line only.
- [ ] M-006 Prove policy mismatch forces re-scout/recompute and serialized snapshot,
  CCR, analytics, logs, diagnostics, review, and curation contain no runtime canary.
- [ ] M-007 Assert memory-only `checkpoint_now` is a successful typed
  `persistence_unavailable` result with `applied=false`, not an MCP/protocol error.
- [ ] M-008 Run `cargo fmt --check`.
- [ ] M-009 Run `cargo check --features server`.
- [ ] M-010 Run `cargo clippy --all-targets --features server -- -D warnings`.
- [ ] M-011 Run focused suites and serial server all-target suite.
- [ ] M-012 Run exact embed gate.
- [ ] M-013 Run adversarial implementation review; resolve accepted blockers.
- [ ] M-014 Update `tasks/todo.md` review/evidence and measured limitations.
- [ ] M-015 Verify every delegated worker is stopped and left no child process tree.

## Dependencies

```text
A -> B -> C -> D -> E -> F -> G -> H -> I -> J -> K -> L -> M
```

Gate G establishes bridge evidence before Gate H derives authority. Gate I can then
filter/search that authority without a forward dependency; Gate J consumes both for
orientation and read-only review, and Gate K alone owns mutation. Safe parallel work
is limited to independent red fixtures/review after shared contracts freeze. Shared
domain/store/watcher/persist edits remain serialized.
