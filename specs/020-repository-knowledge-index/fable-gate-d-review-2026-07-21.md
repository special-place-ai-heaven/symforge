# Fable Gate D Scoped Review — 2026-07-21

Date: 2026-07-21
Branch: `feat/repository-knowledge-index` (all Gate A–D work uncommitted in one working tree)
Scope: SpecKit 020 Gate D only — watcher, reconciliation, and canonical manifest state
Reviewer: Fable (main loop) + four independent adversarial reviewer agents (fencing/races,
disposition authority/symmetry, atomic-save/transient, claims/minimality), every sustained
finding re-verified against the source by the main loop before inclusion.
Method: read-only. No production code or frozen artifacts modified. Line numbers are
working-tree state at review time.

## Verdict

**CONTESTED.**

The core Gate D machinery is sound and mostly well-oracled: the single-writer commit
boundary with dual project/publication fences, the metadata-first single-file path, the
coalesce-then-sort-live-first atomic-save design, manifest-as-sole-authority on every read
surface, and removal/promotion symmetry on every scout-routed path all survived adversarial
refutation. But four HIGH findings are sustained, each falsifying either a frozen contract
clause (FR-011, FR-003) or a claim the review request states verbatim, and D-V03 does not
survive as checked. Three of the four HIGHs share one root cause — coverage is derived from
the scout plan while Gate D made the manifest the sole disposition authority — and are
fixable at two small choke points.

## Receipt verification (performed, not assumed)

- Focused suites independently re-run serially: `watcher_integration` 10 passed / 1 ignored
  (matches receipt), `admission_acceptance` 5, `impact_admission` 3, `watcher_layer3_restat`
  3, `watcher_reload_cancellation` 3 — all green, exact match.
- Terminal Commander job `job_019f84910ba37c53921fa1b3672a3e19` is **no longer known to the
  daemon** (`UnknownJob`); the authoritative all-target receipt is not independently
  replayable. The re-runs above are the compensating evidence.
- The full serial `--lib` suite is **not reproducibly green on this tree**: run 1 failed
  `live_index::persist::tests::snapshot_round_trip_preserves_target_enum_and_catalog_dispositions`
  (2833 passed / 1 failed), run 2 passed clean (2834 / 0), identical serial order. The test
  (persist.rs:1962) is **new in the uncommitted tree** (absent from HEAD) and is Gate E's
  E-R01 oracle name. Not a Gate D defect, but two consequences: (a) the claimed all-target
  exit-0 receipt cannot be current for the tree as reviewed; (b) E-R01's red oracle is
  unstable — it *mostly passes*, which per plan.md's red-oracle rule ("stop if a proposed
  red test already passes for the intended reason") is a Gate E problem to resolve before
  Gate E opens.

## Sustained findings

### F1 — HIGH — A transient metadata failure during rescout deletes last-valid state from every lane

- Evidence: `src/discovery/mod.rs:797-816` (root cause), `src/watcher/mod.rs:772-801`
  (consequence).
- Violated contract: FR-011 (spec.md:486-490) — an `Unreadable` entry must make coverage
  Degraded and **retain** a bounded re-observation trigger "until it is replaced by a stable
  disposition"; data-model.md:1610-1611 — "delete is represented by absence from the next
  **complete** manifest".
- Falsification sequence: `a.rs` is Tier-1 indexed. A periodic rescout runs
  `scout_entries_with_io`; `std::fs::metadata(a.rs)` fails transiently (Windows AV/EPERM, or
  a rename window). The metadata-failure arm records a `DirectoryEntryUnreadable` issue and
  then `continue`s — **the entry is dropped from the plan** (contrast the probe-failure arm,
  which retains an `Unavailable{Probe}` entry). The reconcile diff computes
  `removed_paths = previous − fresh` with **no coverage gate**; the scout-entry equality
  fence in `remove_file_if_scout_entry_at_generation` passes (nothing changed the entry), and
  live bytes, manifest entry, and plan entry are all removed. A transient stat error deletes
  a healthy file from search/symbols/outline/catalog with no retained `Unreadable`
  disposition. The same hole applies wholesale to an unreadable *directory* subtree. Default
  config self-heals on the next 30s periodic pass (the file returns as a new entry), but the
  contract requires retained-Degraded, not delete-then-restore, and with
  `SYMFORGE_RECONCILE_INTERVAL=0` the loss persists.
- Sub-finding F1a (MEDIUM, same root): on the single-path route, `scout_single_path`
  metadata failure returns `Err` and the watcher publishes **nothing**
  (`src/watcher/mod.rs:313-325`) — no `Unreadable` disposition, coverage untouched — the
  only failure stage that silently skips FR-011 accounting.
- Smallest correction (one place fixes both): in `scout_entries_with_io`, replace the
  metadata-failure `continue` with a retained `ScoutedEntry` whose decision is
  `ScoutDecision::Unavailable { stage: AccessStage::Metadata, kind }` (the variant already
  exists; bulk admission already emits `Unreadable{Metadata}`). For never-enumerated
  subtrees, additionally gate `removed_paths` on the fresh plan having Complete coverage.
  Add one RED: metadata failure for an existing indexed file during rescout → bytes and
  manifest entry survive with `Unreadable{Metadata}` + Degraded coverage.

### F2 — HIGH — Degraded coverage from read-stage transients is a moment-in-time flag that any later plan refresh erases

- Evidence: `src/discovery/mod.rs:97-107` (`refresh_scout_plan` derives coverage from walk
  issues + `Unavailable` decisions only), `src/live_index/store.rs:1071-1104`
  (`scout_plan_with_entry_locked` — Degraded exists only via the `force_degraded` argument),
  `src/watcher/mod.rs:932-934` (retry loop reads plan coverage).
- Violated contract: FR-011 — "Any `Unreadable`/`UnstableDuringRead` entry makes coverage
  Degraded … until it is replaced by a stable disposition"; data-model.md:704-707; D-V03
  "Degraded coverage always schedules bounded repair"; US4's "never silently serves a stale
  unit as current".
- Falsification sequence (deterministic, two files): `a.rs` and `b.rs` both fail the stable
  read → both manifest entries `UnstableDuringRead`, plan coverage Degraded (forced),
  last-valid bytes retained. `b.rs` recovers via a real update →
  `publish_indexed_file_at_generation` → `scout_plan_with_entry_locked(b, false)` →
  `refresh_scout_plan` recomputes coverage from the plan alone — `a.rs`'s plan entry still
  says `Ingest` (the scout succeeded; the read failed) → **coverage stored Complete** while
  `terminal_dispositions()` still reports `(a.rs, UnstableDuringRead)` and `get_file("a.rs")`
  serves retained last-valid bytes with no degraded signal. Reconcile route: the same
  recompute inside `publish_reconciled_scout_plan_at_generation` closes the FR-011 bounded
  backoff window after one attempt while the failure persists. Convergence itself survives
  only because the re-observation set is rebuilt from the manifest each pass.
- Smallest correction: one store-level guard where plans are stored under `write_mutex` —
  after `refresh_scout_plan`, force `plan.coverage = Degraded` when any live manifest entry
  has disposition `Unreadable`/`UnstableDuringRead` (both storing sites have `self.live` in
  scope). One RED: persistent read-stage transient across a reconcile pass keeps coverage
  Degraded; sibling recovery does not clear it.

### F3 — HIGH — Circuit-breaker degradation is a silent permanent stop: the scheduled repair has no executor, and the first reconcile erases its coverage signal

- Evidence: `src/live_index/store.rs:929-932` (`reconciliation_repairs` — doc comment claims
  "Gate D's reconciliation worker drains this queue"; grep proves **no consumer exists**:
  all production references are fills/carries/clears; `src/watcher/` never touches it);
  `src/watcher/mod.rs:720-730` (`transient_paths` filter covers `Unreadable |
  UnstableDuringRead` only — `AbortedCircuitBreaker` excluded); `src/live_index/store.rs:
  3332-3334` (trip degrades stored plan coverage as a direct field override not represented
  in plan issues/entries).
- Violated contract: data-model.md:1605-1608 — a trip "schedules bounded reconciliation for
  that lane; it cannot … become a silent terminal stop"; FR-007; D-V03 second half.
- Falsification sequence: a parser-lane trip marks N files `AbortedCircuitBreaker`, coverage
  Degraded, repair objects queued. The next reconcile rescouts: each aborted file's fresh
  scout entry **equals** its previous entry (disk unchanged) and is not in `transient_paths`
  → skipped, never re-read. `publish_reconciled_scout_plan_at_generation` →
  `refresh_scout_plan` recomputes coverage from a clean fresh walk → **Complete**; the
  bounded retry loop exits after attempt 1. The queue is never drained; the aborted files
  stay unindexed indefinitely; `freshness_status` (set Degraded at reload) is never
  recovered, so freshness and coverage permanently disagree. The named oracle
  (`circuit_breaker_trip_is_scoped_degraded_and_schedules_repair`, store.rs:5760-5824)
  asserts only that the repair *object* exists — it passes for the wrong reason.
- Smallest correction: add `| FileDisposition::AbortedCircuitBreaker` to the
  `transient_paths` filter (the already-wired reconcile then re-reads those paths and their
  successful publication replaces the aborted dispositions), and delete the unconsumed
  `reconciliation_repairs` queue — or actually drain it; either way the field comment must
  stop describing a worker that does not exist. F2's manifest-derived coverage guard keeps
  the Degraded signal alive meanwhile.
- Minimality (folded from lens 6): `reconciliation_repairs` +
  `ScopedReconciliationRepair` (store.rs:2478-2498) duplicate the retry policy already
  hardcoded in `reconcile_for_cause_with` (5 attempts / 50ms) and encode a trigger fully
  derivable from manifest `AbortedCircuitBreaker` dispositions — dead duplicate state
  outside plan.md's complexity budget.

### F4 — HIGH — The impact seam mints duplicate canonical catalog identities from non-normalized spellings

- Evidence: `src/protocol/tools.rs:4919-4923` (`analyze_file_impact` forwards
  `params.0.path` verbatim — contrast `get_file_content` at tools.rs:7966 and
  `validate_file_syntax` at tools.rs:8165, which apply `normalize_exact_path`);
  `src/sidecar/handlers.rs:820-865` (`impact_text`, no normalization; exact-string
  `get_file` miss routes a non-normalized spelling of an **indexed** file into
  `handle_new_file_impact`); `src/live_index/store.rs:3435-3448` (`LiveIndex::update_file`
  fabricates a `CatalogPath` from the verbatim key when no manifest entry matches);
  `store.rs:3183-3196` (upsert dedup by exact catalog-path string).
- Violated contract: FR-003 (exactly one terminal disposition per file), FR-025 (lossless
  single path identity), and the review request's verbatim claim "normalized-path upsert
  prevents duplicate catalog identities".
- Falsification sequence: catalog holds `src/lib.rs` (Indexed). A caller invokes
  `analyze_file_impact` with `./src/lib.rs` (or a backslash/absolute spelling —
  `cli/hook.rs:580-587` emits absolute paths when `strip_prefix` fails). The admission gate
  passes (`root.join` tolerates the spelling), the exact-string `get_file` lookup misses,
  `should_auto_index_new_file` fires, and `update_file("./src/lib.rs", …)` mints a second
  files-map key **and a second manifest entry** for one on-disk file — doubled dispositions,
  inflated tier counts, doubled search hits. Reconciliation never repairs it: the diff base
  is the scout plan, which never contained the `./` spelling; the duplicate persists until
  full reload.
- Smallest correction: normalize once at the shared choke point — apply the existing
  `normalize_path_query`/`normalize_exact_path` to the key inside
  `LiveIndex::update_file`/`remove_file`, or at `impact_text` entry (covers the MCP tool and
  the sidecar HTTP route in one edit).

### F5 — MEDIUM — The impact auto-index seam bypasses scout scope policy and mints Indexed entries reconciliation cannot repair

- Evidence: `src/sidecar/handlers.rs:874-913` — `impact_admission_refusal` applies
  `classify_admission` (size/lockfile/binary) but not gitignore scope, generated-output
  demotion, or hard-scope/source exclusions, all of which the watcher's shared seam enforces
  (`src/watcher/mod.rs:296-306, 1069-1075`); `handlers.rs:843-861` then admits via raw
  `update_file`, and `store.rs:3428-3432` will flip an existing `MetadataOnly` disposition to
  `Indexed`.
- Violated contract: FR-001 (a **single** metadata-first scout defines repository scope);
  D-G02 as checked ("route single-file updates through shared admission/read" — this
  single-file update is not).
- Falsification sequence: gitignored `scratch/tool.js` or generated `dist/bundle.js`
  (demoted `MetadataOnly(GeneratedOrVendor)` by bulk scout) is edited; the impact hook fires;
  `classify_admission` returns Normal; the file is force-admitted `Indexed` into the
  canonical manifest. The watcher skips gitignored paths, and the reconcile diff never
  consults `manifest_entries` for residents absent from both scout plans — the out-of-policy
  entry persists until full reload.
- Smallest correction: route the admit branch through the same seam the refusal branch
  already uses (`freshen_file_if_stale` → `read_and_index` with the generation fence)
  instead of raw `update_file`.

### F6 — MEDIUM — A fully failed rescout terminates the bounded-backoff window immediately by reading stale Complete coverage

- Evidence: `src/watcher/mod.rs:731-740, 813-826` (walk error → `fresh_plan = None` → only
  Tier-1 fallback, **no plan published**) with `src/watcher/mod.rs:932-934` (retry loop reads
  the *previous* plan's Complete coverage and breaks after attempt 1).
- Violated contract: FR-011 — "Degraded walks MUST retry with bounded backoff"; a walk that
  errors outright is the most degraded walk possible.
- Smallest correction: surface walk failure from `reconcile_stale_files_with_stop` (bool or
  small enum alongside the count) and treat it as Degraded for retry purposes in
  `reconcile_for_cause_with`.

### F7 — MEDIUM — Test-evidence gaps on claims recorded as closed

1. **D-V03 first half has no behavioral oracle.** No test reconciles an unchanged *active*
   project and asserts `published_state().generation` did not move; the no-publication
   property is structural only (`publish_reconciled_scout_plan_at_generation` never calls
   `swap_and_publish`). One test closes it: load → `reconcile_for_cause(Periodic)` with zero
   disk changes → assert `repaired == 0`, generation unchanged, coverage Complete.
2. **D-R08's fixture shape cannot catch the F1 class.** It degrades coverage by appending an
   *issue* while keeping entries identical (`src/watcher/mod.rs:2374-2384`); real degraded
   walks degrade by dropping entries. The retry mechanics it proves are real; the dangerous
   case is untestable in that shape. Same fixture reused at :2587-2596.
3. **D-R01's "before read" half is not instrumented.** `watcher_admits_sparse_gguf_before_read`
   (:1825) would pass identically for a read-then-hard-skip implementation; read-avoidance
   is asserted nowhere on the watcher path.
4. **The overflow trigger leg is inspection-only.** `src/watcher/mod.rs:1380-1413` (notify
   error → `reconcile_for_cause(Overflow)`) is exercised by no test; D-R05/D-R10 drive the
   engine directly. Engine proven, wiring not.

### Advisory notes (LOW / FYI — no gate action required)

- `update_file_at_generation` (store.rs:1438-1503) is a live trap: production-visible,
  project-fenced only (no publication fence, no scout-plan refresh), referenced today only by
  one unit test. It *does* sync the manifest (via `LiveIndex::update_file`), so the split
  risk is plan-only — but the obvious-looking name invites reintroducing the exact drift
  Gate D retired. Delete or `#[cfg(test)]` it.
- `test_watcher_ignores_non_source_files` (tests/watcher_integration.rs:616-656) still
  narrates the deleted extension-filter model; it now passes for a different reason than its
  comments claim. Rename and add the positive assertion (path present in
  `terminal_dispositions()`, absent from content).
- `remove_file_with_fences` and repeated failed re-observations clone-and-publish even when
  nothing changed — O(index) per vanished temp hint. Correct, bounded (D-V02 holds), worth a
  ceiling note.
- Case-only rename (`foo.rs` → `Foo.rs`) leaves the old-case identity for ≤ one reconcile
  interval (`symlink_metadata` resolves case-insensitively on Windows); by-design per
  data-model.md:503-504, noting the window.
- Snapshot restore leaves `manifest_entries` empty and `background_verify` repopulates
  per-file (persist.rs:1239, 1564-1610) — transiently partial manifest lane. Gate E
  attribution (E-G01/E-G02/E-R02 own it); Gate D did not make it impossible. Recorded so it
  is not lost.

## Claim table

| Item | Verdict |
|---|---|
| D-R01 | CLOSED, weak on the "before read" half (F7.3) |
| D-R02 | CLOSED — genuine single-publication oracle (generation +1, all lanes from one update) |
| D-R03 | CLOSED |
| D-R04 | CLOSED — asserts files, compat projection, dispositions, AND plan lanes |
| D-R05 | CLOSED for the engine; overflow wiring untested (F7.4) |
| D-R06 | CLOSED — exemplary all-lane before/after snapshot + rejected-counter |
| D-R07 | CLOSED — deterministic barrier race through a production hook; rebase honestly oracled |
| D-R08 | CLOSED for its fixture class; structurally blind to the F1 class (F7.2) |
| D-R09 | CLOSED for its fixture class; the persistent-failure coverage carve-out is F2 |
| D-R10 | CLOSED for walk-level degradation; the CB class violates the same principle (F3) |
| D-G01 | CLOSED — no extension gate before scout/removal; `supported_language` is a hint only |
| D-G02 | CLOSED for watcher/freshen/reconcile; **breached by the impact admit branch** (F5) |
| D-G03 | CLOSED — all lanes under one writer/generation/publication fence, one swap |
| D-G04 | CLOSED — removal clears files, manifest, plan under fences |
| D-G05 | CLOSED — complete manifest diff; Tier-1 walk only as commented failed-rescout fallback |
| D-G06 | CLOSED with caveats — F3 (CB uncertainty), F7.4 (overflow wiring) |
| D-G07 | CLOSED — write_mutex everywhere, dual fences, plan rebase, bounded abort |
| D-G08 | CLOSED — zero stored skip state; projections only; snapshot version bump quarantines old state. Sole-*writer* discipline breached by F4/F5 |
| D-G09 | CLOSED for its literal text (unreadable/unstable); the aborted-class analogue is F3 |
| D-V01 | CLOSED — receipts reproduced independently this review (see Receipt verification) |
| D-V02 | CLOSED with caveat — storm/rename behavioral; "repeated without nondeterminism" receipt-only |
| D-V03 | **NOT CLOSED** — first half has no behavioral oracle (F7.1); second half falsified for the CB class (F3) and the persistent-transient class (F2) |

## What was refuted (held under attack)

- No check-then-swap anywhere: every `publish_*_at_generation` re-clones under `write_mutex`
  and re-validates both fences; off-lock work is fenced by the captured publication
  generation and retried from disk (bounded, 4 attempts).
- The reconcile plan rebase (`store.rs:1170-1189`) folds every current-vs-baseline delta
  under the lock — the racing-watcher test genuinely fails on regression.
- `stale_file_batch_cannot_mutate_any_lane` is a model negative oracle (all lanes + counter).
- Manifest is the sole stored disposition state: no `skipped_files` anywhere, tier counts /
  admission lookup / compat skip output / metadata-only listing are all live projections;
  HEAD archaeology confirms the field, its mutators, and its bulk population are gone.
- Atomic-save handling: one `PendingPath` per normalized path, live destinations sorted
  first, vanished temp hints removed with zero retries in the event lane (the [50,200,500]ms
  sleeps live only in `maybe_reindex`, off the event path); rename-over purges old symbols.
- Transient failures retain last-valid bytes (`retains_last_valid_content`) with the tier
  projection following the manifest, and recovery restores `Indexed` — including under a
  real `FILE_SHARE_NONE` lock (`transient_av_lock_does_not_remove_file`).
- Watcher stable-read is byte-identically the bulk helper (`stable_read_with_retries`,
  double-pass hash, same limits); watcher scout is the shared `scout_single_path`.
- Bytes and manifest never split on any direct update/remove path: `LiveIndex::update_file`
  inlines the manifest upsert; `LiveIndex::remove_file` clears both plus derived indices.
- Explicit non-findings honored: no Gate E snapshot/one-Arc-bundle or Gate F+ knowledge
  behavior was demanded; `src/protocol/tools.rs` Tier-2 classification accepted as declared.

## Root-cause synthesis

Two roots explain five of the seven findings:

1. **Coverage authority split (F1, F2, F3, F6):** Gate D made `manifest_entries` the sole
   disposition authority, but `CoverageStatus` is still derived from the scout plan
   (`refresh_scout_plan`: walk issues + `Unavailable` decisions only). Transient read-stage
   and aborted dispositions live in the manifest, so every plan refresh forgets them, and a
   degraded walk deletes what it failed to enumerate. The fix is the same move Gate D
   already made for dispositions: derive coverage from the manifest at the plan-store choke
   points, and retain metadata-failure entries as `Unavailable{Metadata}`.
2. **Impact seam outside the shared seam (F4, F5):** one entry path still writes into the
   canonical manifest with caller-controlled spellings and its own partial admission gate.
   The fix is one normalization at the choke point plus routing the admit branch through
   `freshen_file_if_stale` like its own refusal branch already does.

## Decision

**GATE D REOPENED.**

Reopen scope is narrow: F1–F6 with their named smallest corrections and the F7 oracles, plus
re-capture of the authoritative serial receipt once the tree (including the unstable E-R01
oracle) is settled. The fencing, coalescing, authority-projection, and symmetry work — the
bulk of Gate D — survived adversarial review and does not need rework.
