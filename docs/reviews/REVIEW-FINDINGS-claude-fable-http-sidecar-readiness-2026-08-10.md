# Review findings — claude-fable — HTTP sidecar readiness and publication fencing

Reviewer: Claude Fable (adversarial correctness review per
`REVIEW-REQUEST-http-sidecar-readiness-2026-08-10.md`).

## 0. Snapshot identity — the reviewed tree was a MOVING TARGET

Base commit: `71fb88429134462cc8bdf1022ee3037bcec5f65d` (= `origin/main`, 10.0.3).
All review targets were uncommitted working-tree diffs. Four distinct diff
states were observed during this single review pass
(`git diff --binary | git hash-object --stdin`):

| # | Diff blob hash | What happened at this state |
|---|---|---|
| S1 | `1fccccd9163df964384db5d01423b6b429035c62` | Full deep-read of the diff (2,974 insertions). `cargo fmt --check` FAIL; `cargo clippy -D warnings` FAIL (dead code); full test suite run: one failure (`batch_rename_perf`). |
| S2 | `86a94b5a68f60c235346b206aec187bbc0fadf39` | Re-read via interdiff (+334 net lines). Several S1 findings had been fixed in-flight by the implementer. |
| S3 | `59b6e82ec25e1b77097fe5b29dba68660da20fb0` | Second strict-gate run started here. `cargo fmt --check` still FAIL (9+ locations); `cargo clippy -D warnings` PASS. |
| S4 | `263c2aee7e353a87d7656497fe38c288b05c983a` | Tree had moved again while S3's gate was still running. |

Every finding below is stamped with the snapshot it was verified against.
Line numbers are from the stamped snapshot and will drift. **The strict gate
has never been observed green on any single frozen state of this diff.**

Baseline for comparative measurements: detached worktree at `71fb8842`
(`../symforge-baseline-review`), same machine, isolated runs.

## 1. Findings (P0/P1/P2/P3)

### P0 — none found.

### P1-1 — `batch_rename` dry-run wall clock regressed ~4x; H.7 perf gate fails (S1: proven, measured; S3 re-measure noted in §3)

1. **Confidence:** proven at S1 by direct A/B measurement on the same machine.
2. **Location:** entry point `src/protocol/edit_tools.rs:1965 batch_rename` →
   `prepare_project_wide_rename` (`edit_tools.rs:501`) →
   `watcher::reconcile_stale_files` (whole-repo freshen sweep inside the timed
   window), plus every receipt-bearing publish seam in
   `src/live_index/store.rs` / `src/live_index/single_file.rs` that the sweep
   drives.
3. **Sequence:** `cargo test --test batch_rename_perf -- --test-threads=1`.
   Diff'd tree (S1-era): best-of-3 **20,663 ms** (20,674 / 21,186 / 20,663 —
   consistent, not load spikes). Baseline `origin/main`, same machine, isolated
   worktree: **passes the 5,000 ms budget** (suite completed in 40.1 s
   including the ~35 s cold load; the test short-circuits when an attempt is
   under budget). The test asserts best-of-N precisely so machine load cannot
   explain a consistent 4x elevation.
4. **Consequence:** `cargo test --all-targets` (PR CI gate) fails; and every
   real `batch_rename`/project-wide-rename call pays the same multi-second
   regression in production.
5. **Smallest remediation:** profile the dry-run timed window on the diff vs
   main before landing. The only whole-repo loop inside the window is
   `reconcile_stale_files` (per-file `freshen_file_if_stale`, now returning
   through the receipt-bearing `read_and_index_with_receipt` machinery);
   suspicion also falls on the added per-publication work
   (`pre_update_snapshot` clone + `record_pre_update_snapshot_for_publication`
   + `published_generation()` re-loads) if the sweep publishes. This review
   measured and localized the window but did not isolate the exact line;
   per the repo's reporting invariant I am not claiming a root cause I did not
   observe.
6. **Regression test:** already exists and is the failing gate —
   `tests/batch_rename_perf.rs::batch_rename_health_dry_run_stays_under_h7_budget`.

### P1-2 — disk-absence ABA guard existed but was DEAD CODE; production removals fenced only by publication equality (S1: proven; **fixed in-flight by S2** — verify it stays wired)

1. **Confidence:** proven at S1 (clippy `-D warnings` failed the build:
   ``method `remove_file_if_absent_at_publication_fence_with_receipt` is never
   used`` at `store.rs:2775`); `finalize_missing_file` (S1
   `single_file.rs:637`) called fence-only `remove_file_at_publication_fence`,
   and the watcher event lane called plain `remove_file_at_generation`.
2. **Location (S2+, fixed):** `src/live_index/single_file.rs:171
   finalize_missing_file`, `src/watcher/mod.rs:785,818 process_events`,
   `remove_file_if_scout_entry_at_generation` (now passes
   `expected_entry.absolute_path`).
3. **Sequence (S1):** delete `f.rs` → watcher `maybe_reindex` retries observe
   NotFound → recreate `f.rs` on disk after the last retry, before
   `finalize_missing_file`, with no intervening publication → publication
   fence still matches → entry removed while the file exists on disk (until a
   later create event heals it). This is exactly known-finding #7: an exact
   index-publication fence does not prove continued filesystem absence.
4. **Consequence (S1):** transient false-absence for a recreated file; plus a
   hard CI red (clippy) proving the guard was unreachable.
5. **Remediation:** already applied in S2 — `symlink_metadata` absence
   re-confirmation under the writer lock on all three removal paths. Keep it;
   the S2 tests (`stale_not_found_fence_preserves_recreated_file_and_reports_rejection`,
   `scout_reconciliation_removal_refuses_a_recreated_disk_path`) pin it: each
   fails if the disk recheck is removed, because the publication fence alone
   would admit the removal.
6. **Regression tests:** the two named above, present at S2+.

### P2-1 — impact 503-after-commit: a retried request reports "no structural changes" for an edit that changed symbols (S3/S4: proven by code reading; open)

1. **Confidence:** proven reachable by code reading; requires a same-project
   freshness flip mid-request.
2. **Location:** `src/sidecar/handlers.rs` `impact_handler` — the trailing
   `capture_queryable_sidecar_generation(&state, &fence)?` executed AFTER
   `impact_hook_text` has fully committed (S3 numbering ~line 999); the new
   freshness conjunct in `published_sidecar_index_is_queryable`
   (`handlers.rs:418-441`) makes the trailing check fail on freshness alone.
3. **Interleaving:** (A) `/impact?path=f.rs` passes `require_queryable`
   (freshness `Current`), takes the impact lock, `handle_edit_impact` commits:
   publishes v1→v2, drains the pre-update snapshot bound to its receipt,
   seeds the symbol cache with post-edit (v2) symbols, records write stats.
   (B) snapshot verification starts (or the watcher dies) → published
   freshness becomes `Verifying`/`Degraded`. (A) trailing capture fails →
   **503, empty body**, computed response discarded. Hook fails open; the
   caller retries once the index is `Current` again. The retry's baseline is
   now v2 (snapshot gone, cache = v2, index = v2), disk = v2 → the response
   claims **no symbol changes** for an edit that changed/added/removed
   symbols.
4. **Consequence:** invented un-change delivered to the caller — wrong output,
   not a refusal. All mutation side effects also stand behind a 503 the caller
   is invited to retry (the exact hazard Q3 names).
5. **Smallest remediation:** do not refuse an impact whose response is derived
   entirely from its own fenced immutable receipt. Either delete the trailing
   capture on the impact route (the pre-lock `require`, the in-handler
   `expected_generation` checks, and the receipt fencing already pin project
   identity), or restrict the trailing check to project-generation/source/root
   equality and exclude the freshness conjunct.
6. **Regression test:** rooted real-index fixture; run `impact_handler`
   through the public route; force freshness to `Verifying`
   (`mark_snapshot_verify_running`) after the publish seam (e.g. via a stable-read
   test seam or by invoking `handle_edit_impact`'s components around a direct
   freshness mark); assert the response is 200 with the correct diff — and
   that a follow-up impact does not report an empty diff. Fails on current
   code (503, then empty diff on retry).

### P2-2 — confirmed-absent removal is dropped forever on fence contention: deleted files linger in global answers (S2+: proven by code reading; open)

1. **Confidence:** proven by code reading (interleaving is ordinary watcher
   traffic); severity depends on write concurrency.
2. **Location:** `src/live_index/single_file.rs:171-197 finalize_missing_file`
   (fence captured at the last NotFound observation), and the silent drops:
   `watcher/mod.rs process_events` (`ReindexResult::PublicationRejected => {}`)
   and the event lane's one-shot
   `remove_file_if_absent_at_publication_fence_with_receipt` calls
   (`watcher/mod.rs:785,818` — no retry on `None`).
3. **Interleaving:** delete `a.rs` → `maybe_reindex` retries (~750 ms) observe
   absence, fence F captured → any concurrent publication of unrelated `b.rs`
   advances the publication fence → `remove_file_with_fences` rejects (exact
   fence mismatch) → `PublicationRejected` is discarded; the delete event is
   consumed and no further events will ever fire for `a.rs`.
4. **Consequence:** `repo-map`, search, symbol-context and every global-absence
   claim keep serving the deleted file until something else touches that path
   (freshen-on-read heals it only if someone reads it). `origin/main` removed
   at the project-generation fence and was immune to concurrent publishes —
   this is a staleness-convergence regression, worst in write-heavy repos
   (where concurrent publications are the norm, making rejection likely).
5. **Smallest remediation:** on exact-fence rejection in the absence path,
   recapture the fence and retry the removal a bounded number of times — the
   `symlink_metadata` re-confirmation under the writer lock is what makes this
   safe. Equivalently: for the disk-confirmed-absent path, fence on
   project-generation + disk absence and drop the exact publication-equality
   conjunct (the disk check is the authority known-finding #7 asked for).
6. **Regression test:** index `a.rs` and `b.rs`; simulate: delete `a.rs`,
   capture `publication_fence()`, publish an update to `b.rs`, then call
   `finalize_missing_file(shared, "a.rs", abs, gen, stale_fence)`; assert the
   file is (eventually) removed. Fails today: returns `PublicationRejected`
   and `a.rs` remains indexed with no retry scheduled.

### P2-3 — empty/whitespace legacy session id suppressed the daemon fallback for a local sidecar (S1: proven; **fixed in-flight by S2** — keep the normalization)

At S1, `read_sidecar_endpoint`'s legacy lane returned `Some("")`
(`port_file.rs:716-719`), `proxy_path` routed that as LOCAL, but
`initial_endpoint_is_daemon = effective_session_id.is_some()` said daemon →
on any failure from a local sidecar the daemon fallback was suppressed, and
outcomes were attributed to a phantom session. Exactly known-finding #4
("normalize once, one predicate"), which was NOT closed at S1. S2 added
`normalize_session_id` at descriptor read plus trimming/filtering in
`try_daemon_fallback`. Verify it survives to the final diff; Q5's
"empty legacy session ID" row depends on it.

### P2-4 — 404/500 from a live sidecar produced `sidecar_port_stale` + `restart_sidecar` (S1: proven; **fixed in-flight by S3** — one matrix cell still untested)

At S1, `classify_enrichment_response` mapped every non-2xx/409/503 status to
`Unavailable`, so a live sidecar answering 404 (routine for a not-indexed
path) or 500 fell into the dead-transport lane: `NoSidecar`,
`sidecar_port_stale`, `restart_sidecar` hint — calling a live sidecar dead
(Q5's 404/500 column). S3 added `EnrichmentHttpResult::HttpFailure(status)`
with its own live-refusal arms (`hook.rs:589-598`), reserving the stale lane
for transport failure only. **Gap:** as of S3 I found no subprocess test
pinning the 404/500 lane (the new tests cover 503/409); add one mirroring
`run_hook_index_not_ready_fails_open_as_sidecar_error` with a 500 mock —
without it, the smallest reversion (dropping the `HttpFailure` arm) goes
green.

### P3 — smaller items

- **P3-1 (S3: proven).** `cargo fmt --check` is STILL red at S3 — 9+
  locations (`hook.rs:1193`, `tools.rs:4878/8725/27700`,
  `handlers.rs:14/3486/3494`, both hook test files). CI's first job fails.
- **P3-2 (S3).** Every routine 404 read (unindexed path) now records
  `SidecarError` in the adoption log and burns one daemon-fallback round-trip
  (≤500 ms) before failing open. Honest, but it pollutes the error-rate signal
  with an expected condition; consider a distinct outcome for request-level
  404 vs 5xx.
- **P3-3 (S2+, design consequence to confirm).** The new freshness conjunct
  means `Degraded { WatcherUnavailable }` blacks out ALL sidecar enrichment
  indefinitely, even though freshen-on-read exists precisely to serve
  per-file-current answers without a watcher. This is what Q2/invariant 1
  demand (Degraded must refuse), so it is spec-conformant — but it is a
  meaningful availability tradeoff the author should confirm was intended for
  the watcher-dead steady state, not only for transient verification.
- **P3-4 (S2+).** `SYMBOL_CACHE_GENERATIONS` (`handlers.rs:501`) is a
  process-global mutex + map keyed by `Arc::as_ptr as usize`. The Weak
  upgrade + `ptr_eq` guard does defeat address-reuse, and dead entries are
  purged — correct, but it serializes all sidecars' cache ops on one lock and
  is a lot of machinery for "cache is scoped to one project generation".
  Note only; no defect found.
- **P3-5 (process).** Four diff states in one review pass. Findings and gate
  results above are snapshot-stamped; nothing here should be assumed true of
  the final diff until the strict gate runs green on a frozen state.

## 2. Required questions

### Q1 — Complete route sweep

Local Axum router (`src/sidecar/router.rs:23-56`), all behind
`caller_root_guard` middleware except the two diagnostics:

| Route | Class | Verdict |
|---|---|---|
| `/health`, `/stats` | diagnostic | deliberately unguarded (root-agnostic, no index reads that assert global truth); `/stats` counters shown not to advance on refusals (matrix test). |
| `/outline`, `/workflows/source-read` | read + freshen | guarded — `require_queryable_sidecar_index` → fence-pinned freshen → `capture_queryable_sidecar_generation`. |
| `/impact`, `/workflows/post-edit-impact` | read + freshen + admit + cache + stats | guarded + single-flighted (`lock_impact_analysis`); P2-1 applies to its trailing capture. |
| `/symbol-context`, `/workflows/search-hit-expansion` | read + freshen | guarded. |
| `/repo-map`, `/workflows/repo-start` | global read | guarded. |
| `/prompt-context`, `/workflows/prompt-context` | global read + freshen | guarded (hints, nested symbol-context, and repo-map body all pinned to the captured generation). |

All five workflow aliases are verified thin delegations to the canonical
handlers (`workflow_source_read_handler` → `outline_handler`, etc.) — no
duplicated logic to drift.

Daemon proxy (`src/daemon.rs:3584-3628`): 12 `/v1/sessions/{id}/sidecar/*`
routes; each authorizes, resolves the session runtime,
`guard_session_caller_root` (409 parity), then `block_on`s the SAME canonical
handlers with `repo_root = runtime.canonical_root` — behaviorally identical
to local routes. Verified `SessionSummary` serializes `project_id` =
`active_project_id` (`daemon.rs:705-712`, `1484-1491`), so the hook's new
active-project session filter works against the real daemon, not just the
mock.

One non-identical entry point, deliberately outside the sidecar fence: MCP
`analyze_file_impact` (`tools.rs:5306`) → `impact_tool_text` — guarded by
`loading_guard!` (health only) + generation checks + the shared impact lock,
but NOT by source-binding/root/freshness. Under `Degraded` freshness the MCP
tool still mutates/serves while the sidecar refuses. Consistent with the MCP
surface's trust-banner philosophy, but it is the one lane where invariant-1
symmetry does not hold; flagging for an explicit decision.

### Q2 — Readiness and root fence

No unqueryable state passes: `published_sidecar_index_is_queryable`
(`handlers.rs:418-441`) requires status ∈ {Ready, Empty} ∧ `source.is_some()`
∧ `indexed_root.is_some()` ∧ freshness `Current` (or Empty ∧ `Verifying`).
Loading, Degraded, source-unbound Ready (`from_source_files` shape), and
rootless Empty all 503 — each conjunct pinned independently by
`sidecar_queryability_requires_status_source_and_root_independently` (+
freshness variants) and `ready_but_source_unbound_index_is_not_queryable_by_sidecar`.
A rooted, source-bound Empty serves `/repo-map` and admits its first file —
pinned end-to-end by `test_write_hook_confirms_index`, which now also proves
the first post-admission edit diffs against the seeded baseline instead of
reporting every symbol `[Added]`.

Validate-A-serve-B after rebind: prevented twice — `require_queryable`
compares `state.repo_root` to the fence's `indexed_root` (409 on mismatch),
and every render re-captures against the fence (project generation + source
identity value + root), while all mutations CAS on the captured
project generation. The residual failure mode is fail-closed (P2-1), not
wrong-project data. Cross-project mutation is additionally pinned by
`stale_project_generation_cannot_remove_from_rebound_project` and
`stale_impact_generation_cannot_consume_or_overwrite_rebound_project_state`.

### Q3 — Mutation and receipt linearizability

Linearization point: the winning store publication
(`publish_indexed_file_at_generation` / `publish_hash_skip_at_generation` /
terminal-disposition seams), CAS'd under `write_mutex` on
(project generation, `published_state().generation`) and returning the exact
`Arc<PublishedGeneration>` while the writer lock still owns it. The impact
response (symbols, skipped-text, callers) is rendered from
`receipt.published` — a later watcher publication cannot alter it
(`reindex_receipt_remains_exact_after_later_publication`). A rejected
publication is typed (`PublicationRejected`) and maps to 503, never success.
Concurrent impacts are serialized by the shared `impact_mutex` across the
HTTP route, workflow alias, daemon proxy, and MCP lane (all reach one of the
two locking entry points; `impact_entry_points_share_the_index_single_flight_lock`
pins both). Snapshot consumption is publication-bound
(`take_pre_update_snapshot_for_publication_at_generation`): an older receipt
cannot drain a newer publication's baseline even when content hashes collide
(`publication_bound_snapshot_take_preserves_same_hash_aba_update`), the
project retarget clears the side table under the writer lock, and the S2
`SharedIndexWriteGuard::drop` change invalidates path tokens after arbitrary
guard mutations. **Defect found:** the one remaining Q3 item is P2-1 —
side effects committed before a trailing 503 whose retry produces a wrong
"no changes" answer.

### Q4 — Single-file publication semantics

Audited every branch of `read_and_index_with_stable_read_receipt` (S1 diff,
re-verified S2): exclusion-eviction, metadata-terminal, unavailable-scout,
generated-output, hard-skip, unreadable, unstable-read, content-policy,
hash-skip, full index, NotFound, and the 4-attempt CAS loop. All publish
seams take `expected_index_state_generation = base.health.generation` — the
health/live domain — and compare against `published_state.load().generation`;
bridge-/authority-only publications advance only the full
`publication_generation` and no longer poison the CAS
(`bridge_only_publication_does_not_invalidate_live_index_reindex_cas` — the
deterministic 4-loss failure from known-finding #3 is closed). Project
generation is checked first and independently in every seam. Winning
publications are returned as receipts under the writer lock. `Removed` is
reported only when `remove_file_if_absent_at_publication_fence_with_receipt`
actually published (project gen + exact fence + `symlink_metadata` absence
under the lock — S2), `Skipped`/`HashSkip` always carry the publication that
occurred, and every no-publication outcome is `PublicationRejected`.
Delete→recreate cannot authorize removal of the recreated path (disk
re-confirmation; pinned). Residuals: P2-2 (rejected removals never retried)
and a pre-existing (unchanged by this diff) quirk where an excluded-path
eviction with no prior record still clones-and-publishes.

### Q5 — Hook behavior matrix

Traced at S3 (all lanes end in valid fail-open JSON; request counts bounded;
`outcome_session_id` is the endpoint that actually answered):

| Endpoint | Response | Behavior |
|---|---|---|
| local descriptor | 200 | `Routed`, 1 request. |
| local descriptor | 503 | one daemon fallback: 200→`DaemonFallback`@daemon-session; 503/409/4xx/5xx→`SidecarError` (`index_not_ready`/`root_conflict`/`http_failure`)@daemon-session; none/transport→`SidecarError index_not_ready`@initial session. No probe, no stale hint. |
| local descriptor | 409 | same shape, `root_conflict`. Fallback re-resolves BY ROOT and filters sessions to `project_id == matching.project_id`, so it cannot loop back into the retargeted session. |
| local descriptor | 404/500 | `HttpFailure` lane (S3): fallback attempt, then `SidecarError http_failure`; never `sidecar_port_stale`. (Untested cell — see P2-4.) |
| local descriptor | transport | liveness probe (verbose only) + fallback; if no daemon: `NoSidecar sidecar_port_stale` + restart hint — correct, the thing is actually dead. |
| daemon-backed descriptor | 503 | authoritative: NO rediscovery (`initial_endpoint_is_daemon && initial_index_not_ready`), `SidecarError`@that session, exactly 1 enrichment request — pinned by a mock that COUNTS requests (`run_hook_daemon_descriptor_index_not_ready_is_not_retried`). |
| daemon-backed descriptor | 409 / 404/500 / transport | one rediscovery EXCLUDING the failed session (`try_daemon_fallback(root, excluded)`); alternate active session may answer; bounded at 2 enrichment requests total, every alternate-result arm terminal — no loop. |
| missing descriptor | — | step-1 daemon fallback (`DaemonFallback` on success; `NoSidecar sidecar_port_missing` otherwise). |
| stale descriptor (dead port) | — | transport lane above. |
| empty legacy session id | — | normalized to `None` at descriptor read (S2 `normalize_session_id`); routes local, fallback-eligible — the S1 defect (P2-3) is closed. |

The enrichment mocks now match the daemon route only when the query carries
`caller_root=`, pinning that the fallback request stays root-pinned (the S1
change that made the daemon request pinned too).

### Q6 — Public API compatibility

No breaks found (checked across `store.rs`, `single_file.rs`,
`sidecar/mod.rs`, `watcher/mod.rs`, `daemon.rs`, not just the embed facade):

- `ReindexResult` (public, embed) keeps exactly six variants; contention is
  carried on crate-private `ReindexOutcome`, folded at the boundary by
  `into_public_compat` (`PublicationRejected` → `Skipped`, preserving main's
  observable behavior for embedders). External exhaustive matches compile
  unchanged.
- `SidecarState::symbol_cache` — the field type is now the alias
  `SymbolSnapshotCache = HashMap<String, Vec<SymbolSnapshot>>`: type-identical,
  source-compatible; S2's redesign moved generation tracking OUT of the key
  format into a private registry, so embedder-visible cache shape is unchanged.
- `update_file_from_disk`, `remove_file_at_generation`,
  `remove_file_at_publication_fence`, `take_pre_update_snapshot` — signatures
  unchanged (`take_pre_update_snapshot` now also takes the writer lock; no
  public caller can hold that lock, so no deadlock surface).
- Additive only: `pub take_pre_update_snapshot_at_generation`, the alias, and
  assorted `pub(crate)` machinery. `from_indexed_files`/`from_source_files`
  pre-exist on main. `read_and_index_with_stable_read` became `#[cfg(test)]`
  but was `pub(crate)`.

### Q7 — Tests that can fail

High-value tests and the smallest reversion each catches:

- `loading_sidecar_refuses_all_content_routes_without_mutation` — real Axum
  server, all 10 content routes, asserts 503 + EMPTY body + no
  publication/content/project-generation movement + no admission of
  `src/new.rs` + all four stat counters flat. Fails if any single handler
  drops `require_queryable_sidecar_index`, if a refusal body leaks, or if a
  refused request mutates anything.
- `sidecar_queryability_requires_status_source_and_root_independently` (+ S2
  freshness variants) — each conjunct reverted individually flips an assert.
- `ready_but_source_unbound_index_is_not_queryable_by_sidecar` — kills
  status-only readiness (the original cold-start defect shape).
- `test_write_hook_confirms_index` (extended) — source-bound Empty admits its
  first file over real HTTP AND the follow-up edit diffs against the receipt-
  seeded baseline (fails if new-file impact reverts to seeding an empty
  baseline, or if Empty is refused: invariant 3 both halves).
- `reindex_receipt_remains_exact_after_later_publication` — fails if the
  response/publication is sampled from current state instead of the receipt,
  or if an old receipt can drain a later snapshot.
- `bridge_only_publication_does_not_invalidate_live_index_reindex_cas` —
  fails if any seam CAS-es on the full publication generation again
  (known-finding #3).
- `stale_not_found_fence_preserves_recreated_file_and_reports_rejection` and
  `scout_reconciliation_removal_refuses_a_recreated_disk_path` (S2) — fail if
  the `symlink_metadata` absence re-confirmation is removed; the publication
  fence alone would admit both removals.
- `stale_project_generation_cannot_remove_from_rebound_project`,
  `stale_impact_generation_cannot_consume_or_overwrite_rebound_project_state`,
  `publication_bound_snapshot_take_preserves_same_hash_aba_update`,
  `generic_write_guard_invalidates_pre_update_snapshot_tokens` (S2) — the
  cross-project and ABA fences, each with a one-line reversion target.
- `impact_entry_points_share_the_index_single_flight_lock` — holds the lock
  and asserts BOTH entry points block then complete; fails if either drops it.
- `test_impact_handler_edit_preserves_index_when_file_unreadable` (rewritten)
  — fails if impact resumes deleting index entries on a single missing-file
  observation.
- Hook subprocess suite — the daemon-503 no-retry test asserts the REQUEST
  COUNT (not just output), the fallback test's mock only matches the daemon
  route when `caller_root=` is present, and the session-filter mock lists a
  NEWER wrong-project session that would be selected (and 404) without the
  filter — all genuinely revert-sensitive.

Mock quality: mock servers are deadline-bounded (no unbounded joins),
tolerate probe connections, and 404 unexpected routes rather than accepting
the wrong path. Gaps: (a) no test for the 404/500 `HttpFailure` lane (P2-4);
(b) nothing pins P2-1 or P2-2 (they are open defects); (c) the enrichment
matrix test depends on process-CWD juggling under `CWD_LOCK` — brittle under
future parallelization but sound at `--test-threads=1`.

### Q8 — Anything else materially wrong

- The measured H.7 perf regression (P1-1) — the only candidate for
  "materially wrong" outside the fencing logic itself.
- The moving-target process issue (P3-5): three of the S1 findings in this
  report were fixed by the implementer WHILE the review ran. Nothing wrong
  with the fixes — but it means no reviewer statement, including "the suite
  passes", has yet been true of the diff that will actually land.
- No path returning wrong-project data, global absence from partial state,
  stale exact-path content, or an infinite wait was found in S2+ beyond the
  items above. No patch-release API break found.

## 3. Validation record (this review's runs)

| Check | Snapshot | Result |
|---|---|---|
| `cargo fmt --check` | S1 | **FAIL** (3 locations, `tools.rs`) |
| `cargo clippy --all-targets -- -D warnings` | S1 | **FAIL** (dead code: the unwired ABA guard, P1-2) |
| `cargo test --all-targets -- --test-threads=1` | ~S1 | **FAIL** — 1 failure: `batch_rename_health_dry_run_stays_under_h7_budget` (20,663 ms best-of-3 vs 5,000 ms budget); everything else green (3,125 lib + all integration suites) |
| Baseline `batch_rename_perf` | `71fb8842` worktree | **PASS** (<5,000 ms) — same machine, isolated |
| `cargo fmt --check` | S3 | **FAIL** (9+ locations) |
| `cargo clippy --all-targets -- -D warnings` | S3 | **PASS** |
| `cargo test --all-targets -- --test-threads=1` | S3 | **FAIL** — `hook_enrichment_integration::test_write_hook_confirms_index`: the new-file `/impact` against a rooted, source-bound EMPTY index returned an empty refusal body ("new_file response must contain 'Indexed'; body: <empty>"). **At S3, invariant 3 (rooted empty repo must admit its first file) is itself broken** — most plausibly by the S2/S3 freshness conjunct or the reworked new-file impact flow. The suite fail-fasted here, so `batch_rename_perf` was not re-measured on S3; P1-1 stands on the S1 measurement. |
| `node scripts/verify-tools.cjs` (both surfaces) | — | NOT RUN (blocked on a green build of a frozen snapshot) |

## 4. Verdict

**Block** — until (1) the diff is frozen, (2) the H.7 `batch_rename`
regression (P1-1) is root-caused and resolved or measurably refuted on the
final snapshot, (3) `cargo fmt` is green, (4) the S3 regression that refuses
first-file admission into a rooted Empty index (invariant 3; caught by the
author's own `test_write_hook_confirms_index`) is fixed, and (5) P2-1 and
P2-2 are fixed or explicitly accepted with rationale. The readiness/fencing architecture itself
is sound and well-tested — the S2/S3 in-flight fixes closed every structural
hole S1 had — so once the perf regression is explained and the gate runs
green on one frozen diff, the remaining items are small, and this lands.
