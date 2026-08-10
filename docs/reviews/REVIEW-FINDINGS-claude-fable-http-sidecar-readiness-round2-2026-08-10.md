# Review findings — claude-fable — round 2 (frozen target)

Reviewer: Claude Fable. Adversarial correctness re-review per the updated
`REVIEW-REQUEST-http-sidecar-readiness-2026-08-10.md`. Round 1
(`REVIEW-FINDINGS-claude-fable-http-sidecar-readiness-2026-08-10.md`) is
historical; every claim below was re-verified against the frozen target only.

## 0. Review target identity — verified

- Base: `71fb88429134462cc8bdf1022ee3037bcec5f65d` (`origin/main`, 10.0.3).
- Frozen implementation commit: `71c14e309a9a45ad01d145b136ef556a6b86190e`
  ("fix: gate sidecar enrichment on trusted index state").
- Computed `git diff --binary 71fb8842 71c14e30 | git hash-object --stdin` =
  `920e3002030392732514f2d560839da05e214099` — **matches the expected hash**
  (71fb8842 is the merge-base, so two-dot and three-dot agree).
- Reviewed tree: branch `fix/http-sidecar-readiness` at HEAD
  `61f84672e2cf4a3d568f0cf80bda9e352c36425f` = 71c14e30 + one docs-only pin
  commit; `git status` clean for the entire review. No drift this round.
- Diff size: 13 files, 4,362 insertions, 618 deletions.

Method: full re-read of the frozen diff (delta-read against the round-1
snapshots I had already read line-by-line, then targeted in-situ reads of
every changed decision point), plus the full release-grade gate executed on
this exact tree (§3).

## 1. Round-1 blockers, re-tested against 71c14e30

| Round-1 finding | Status at 71c14e30 | Evidence |
|---|---|---|
| H.7 `batch_rename` perf regression (~20.6 s vs 5 s budget) | **CLOSED — passes** | `batch_rename_health_dry_run_stays_under_h7_budget` green in this review's own full-suite run on this machine (the same machine that measured the round-1 failure; baseline main also passes here). See §3 for the measured attempt time. |
| Rooted Empty first-file admission broken (S3 regression) | **CLOSED — passes** | `published_sidecar_index_is_queryable` (`handlers.rs:416-443`) now admits `Empty` + `Verifying` only when `snapshot_verify_state == NotNeeded` — the deliberate empty-repo initialization shape; `test_write_hook_confirms_index` (real HTTP: Empty serves, admits `src/new_module.rs`, and the follow-up edit diffs against the receipt-seeded baseline) green in this run. |
| Impact 503-after-commit → retry reports "no changes" | **CLOSED** | `finish_impact_response_at_fence` (`handlers.rs:1025-1038`) validates a successful impact response with `capture_sidecar_generation_at_fence`, which checks **identity only** (`published_matches_sidecar_fence`: project generation + source identity + indexed root — `handlers.rs:463-470`), not freshness/queryability. A freshness-only transition can no longer erase a committed response; a project rebind still refuses. Pinned both ways by `post_impact_fence_preserves_response_across_freshness_only_transition`. |
| Deletion convergence (confirmed-absent removal dropped forever on unrelated publication) | **CLOSED** | `remove_file_if_absent_at_publication_fence_with_receipt` (`store.rs:2870-2900`) now passes `expected_publication = None`: the deletion authority is project generation + under-write-lock `symlink_metadata` absence, exactly as known-finding #11 prescribes. Applied on all four lanes: `finalize_missing_file` (`single_file.rs:171-197`), both watcher event-lane calls (`watcher/mod.rs` `process_events`), scout reconciliation (`remove_file_if_scout_entry_at_generation` passes the entry's absolute path), and the snapshot verifier (`persist.rs remove_snapshot_deleted_file_if_still_absent`). Pinned by `missing_file_finalization_ignores_unrelated_same_project_publication` (removal proceeds past an unrelated publication), the two recreation-refusal tests, and `background_verify_deleted_file_removal_refuses_disk_recreation`. |
| `cargo fmt --check` red | **CLOSED — passes** (§3) | |
| `cargo clippy --all-targets -- -D warnings` red (dead-code ABA guard) | **CLOSED — passes** (§3); the formerly dead seam is now the production removal authority on all four lanes. | |
| Empty/whitespace legacy session id suppressed daemon fallback (round-1 P2-3) | **CLOSED** | `normalize_session_id` at descriptor read (`hook.rs:1068-1078`); pinned end-to-end by `run_hook_empty_descriptor_session_503_routes_locally_then_falls_back` and the whitespace/409 variant, which assert the recorded request routes on BOTH endpoints and root-pinning of both requests. |
| 404/500 from a live sidecar → `sidecar_port_stale` + restart hint (round-1 P2-4) | **CLOSED** | `EnrichmentHttpResult::HttpFailure(u16)` lane (`hook.rs:1332-1339` and the four handler arms); the stale-port lane is reserved for transport failure. The round-1 test gap is also closed: `run_hook_local_http_500_fails_open_as_http_failure_without_restart_hint`. |

## 2. Findings (P0/P1/P2/P3) against the frozen target

### P0 — none found.

### P1 — none found.

### P2 — none found.

The round-1 P2s are all closed (§1). I attempted to break the replacement
mechanisms and could not:

- **Deletion authority without the exact fence.** Attack: delete → absence
  fence F → recreate → watcher admits recreated content → delete again →
  stale-fence `finalize_missing_file`. The removal is *correct* (disk is
  absent at removal time, under the writer lock, same project), and the second
  delete's own event converges identically. Attack: recreate between the
  under-lock `symlink_metadata` check and the publish — impossible; both
  happen under `write_mutex`, and a disk write cannot be fenced by an index
  lock in ANY design; the create event re-admits (the same window main had).
- **Identity-only post-impact fence.** Attack: rebind to project B between
  commit and `finish_impact_response_at_fence` — refused (project generation
  differs). Attack: rebind to a *same-root, same-source* project — project
  generation is monotonic and never reused, so the fence still differs.
  Attack: freshness flip — response preserved by design; the NEXT request is
  gated by `require_queryable_sidecar_index`, which is the correct boundary.
- **Freshness recompute (`recompute_freshness_locked`, `store.rs:1829-1921`).**
  Attack: does healing ObservationFailed skip a pending snapshot verification?
  No — with no reasons left it publishes `Verifying` while
  `SnapshotVerifyState::{Pending,Running}`, `Current` only otherwise; pinned
  by `healed_observation_does_not_bypass_pending_snapshot_verification`.
  Attack: does a snapshot-verify transition clobber unrelated reasons? No —
  Observation/Reconciliation/SnapshotVerification reasons are recomputed from
  live state; watcher/capacity/derived reasons are preserved verbatim; pinned
  by `snapshot_verify_transitions_preserve_other_degraded_reasons`. Attack:
  degraded-at-construction race (known-finding #8) — the constructor now
  computes degraded coverage inline (`store.rs:1420-1435`), so the published
  bundle is never falsely `Current`; pinned by
  `degraded_initial_scout_is_published_in_the_trust_bundle`. The
  `set_freshness_status` bypass seam has exactly one production caller — the
  daemon catalog-capacity cold refusal (`daemon.rs:3526`) — whose capacity
  reason is in recompute's preserved set, so later publications keep the
  refusal sticky, which is the §5 requirement for that out-of-scope state.
- **Reconcile publication minting.** `publish_reconciled_scout_plan_at_generation`
  (`store.rs:1970`) takes the writer lock, CAS-es the project generation, and
  republishes only when the trust bundle actually changed — an unchanged
  periodic reconcile can no longer mint publication generations (pinned by
  `reconciled_coverage_publishes_freshness_once_per_transition` and the
  reconcile no-mint assertion in the watcher tests). This also removes the
  standing supply of unrelated publications that made exact-fence CAS lanes
  livelock-prone.

### P3-1 — one unreadable in-scope file degrades freshness and blacks out the whole sidecar surface (likely; design consequence to confirm, not a contract violation)

1. **Confidence:** likely (mechanism proven from code; frequency depends on
   repo contents and OS file-locking behavior).
2. **Location:** `recompute_freshness_locked` — `observation_failed` is true if
   ANY manifest entry has `FileDisposition::Unreadable`/`UnstableDuringRead`
   (`store.rs:1856-1862`); `published_sidecar_index_is_queryable` refuses all
   Degraded freshness.
3. **Sequence:** one in-scope source file is persistently locked/unreadable
   (on Windows, a file held open with a deny-share handle). Any publication
   recomputes freshness → `Degraded { ObservationFailed }` → every `/outline`,
   `/impact`, `/symbol-context`, `/repo-map`, `/prompt-context` request 503s
   project-wide until that one path is successfully re-observed — which for a
   persistently locked file is never. Healing exists and works
   (`healed_observation_restores_current_freshness`) but requires the path to
   become readable.
4. **Consequence:** availability, not correctness — hooks fail open, honestly.
   The contract says Degraded must refuse, so this is the specified behavior;
   I am flagging the radius (one file → whole-surface blackout, indefinitely).
5. **Smallest remediation, if unintended:** none required for this patch. If
   the radius is unwanted later, scope `ObservationFailed` refusal to the
   affected paths or to a count/proportion threshold — as a follow-up, since
   any change here alters the trust contract.
6. **Regression test:** n/a while the behavior is intended; the two healing
   tests already pin the recovery direction.

### P3-2 — MCP `analyze_file_impact` remains outside the sidecar queryability fence (likely; pre-existing surface split, unchanged risk)

`impact_tool_text` (`handlers.rs:1048-1072`) is reached from MCP
`analyze_file_impact` (`tools.rs:5306`) behind `loading_guard!` (health only)
plus generation checks and the shared impact single-flight lock — but not
behind source-binding/root/freshness queryability. Under `Degraded` freshness
the MCP tool still serves/mutates while every sidecar HTTP route refuses.
Invariant 1 only binds the HTTP routes, their aliases, and the daemon proxy —
all of which are symmetric (verified §Q1) — so this is consistent with the
request as written and with the MCP surface's trust-banner philosophy. Flagged
so the asymmetry is a decision, not an accident. No change requested for this
patch.

### P3-3 — global symbol-cache generation registry (note only)

`SYMBOL_CACHE_GENERATIONS` (`handlers.rs:519-527`): process-global mutex +
map keyed by `Arc::as_ptr as usize`. The `Weak::upgrade` + `Arc::ptr_eq`
check defeats address reuse and dead entries are purged each call — I could
not construct a mis-association. It does serialize all sidecars' cache ops on
one lock; acceptable at hook rates. Note only.

## 3. Validation — full release-grade gate on the frozen tree

Executed by this review on the clean checkout at `61f84672` (implementation
identical to 71c14e30):

| Check | Result |
|---|---|
| `cargo fmt --check` | **PASS** |
| `cargo clippy --all-targets -- -D warnings` | **PASS** |
| `cargo test --all-targets -- --test-threads=1` | **PASS** — 115 test binaries/suites, zero failures, including `batch_rename_health_dry_run_stays_under_h7_budget` and every suite named in the request §6 |
| `cargo build --release` | **PASS** |
| `node scripts/verify-tools.cjs --bin target/release/symforge.exe` | **PASS** — 8 PASS, 0 REVIEW, 0 FAIL |
| `node scripts/verify-tools.cjs --fixture verify-tools-real --bin target/release/symforge.exe` | **PASS** — 11 PASS, 0 REVIEW, 0 FAIL |

Harness-invocation caveat (request erratum, not a code defect): the request
§6 command `node scripts/verify-tools.cjs --fixture --surface compact` does
not match the script's CLI — `--fixture` takes a value and no `--surface`
flag exists (`scripts/verify-tools.cjs:32-46`); run literally, it fails with
ENOENT on `tests/fixtures/--surface/cases.jsonl`. The compact surface is
exercised *inside* the default fixture: the harness relay sets
`SYMFORGE_SURFACE=compact` per compact-marked case
(`verify-tools.cjs:131-140`). I therefore ran the two CI-canonical
invocations (`.github/workflows/ci.yml:127-128`, `.exe` suffix for Windows),
which are the commands the release gate actually enforces. Fix the command
lines in the request/runbook so a future operator's literal run does not
false-alarm.

H.7 note: this is the same machine on which round 1 measured the 20.6 s
best-of-3 failure against the 5 s budget (and on which `origin/main` passed
in an isolated baseline worktree), so this pass is a like-for-like
refutation of the round-1 regression, not a different-hardware artifact.

## 4. Required questions

### Q1 — Complete route sweep

Local Axum router (`src/sidecar/router.rs:23-56`), `caller_root_guard`
middleware on everything except the diagnostics:

| Route(s) | Class | Verdict |
|---|---|---|
| `/health`, `/stats` | deliberately diagnostic | root-agnostic; no global claims; stats counters proven flat across refusals. |
| `/outline` + `/workflows/source-read` | read + fence-pinned freshen | guarded. |
| `/impact` + `/workflows/post-edit-impact` | read + freshen + admit + cache + stats | guarded + single-flighted; post-commit validation identity-only (§1). |
| `/symbol-context` + `/workflows/search-hit-expansion` | read + fence-pinned freshen | guarded. |
| `/repo-map` + `/workflows/repo-start` | global read | guarded. |
| `/prompt-context` + `/workflows/prompt-context` | global read + freshen | guarded; hints, nested symbol-context and repo-map body all render from the captured generation. |

All five aliases are one-line delegations to the canonical handlers. Daemon
proxy (`daemon.rs:3584-3628`): twelve `/v1/sessions/{id}/sidecar/*` routes —
authorize → resolve runtime → `guard_session_caller_root` (409 parity) →
`block_on` the same canonical handlers with `repo_root =
runtime.canonical_root`. Behaviorally identical; no divergent copy of any
handler exists. The only asymmetric entry point is MCP `analyze_file_impact`
(P3-2, deliberate surface split). No unsafe route found.

### Q2 — Readiness and root fence

No source-unbound Ready, rootless Empty, Loading, or Degraded publication
passes: status ∈ {Ready, Empty} ∧ source ∧ root ∧ freshness-queryable, where
`Verifying` is accepted only for Empty with `SnapshotVerifyState::NotNeeded` —
a pending/running empty snapshot verification refuses (pinned by the
`snapshot_verifying_empty` variant). Each conjunct is independently pinned.
Rooted Empty serves and admits its first file over real HTTP, and the
admission receipt seeds the next edit's baseline. Validate-A-serve-B: refused
by the repo-root/indexed-root 409 in `require_queryable_sidecar_index`, the
per-render fence re-capture (project generation + source identity + root),
generation-CAS'd mutations, and the caller_root guards on both surfaces.
Remaining failure direction is fail-closed. No additional finding.

### Q3 — Mutation and receipt linearizability

Linearization point: the winning store publication under `write_mutex`
(generation-domain CAS), returned as an immutable
`Arc<PublishedGeneration>` receipt while the lock is held. The response —
including skipped-text and callers — renders from the receipt, never from
re-sampled current state (`reindex_receipt_remains_exact_after_later_publication`).
Rejected mutations are typed and surface as 503, never success. Concurrent
impacts are serialized across all four entry lanes by `lock_impact_analysis`
(pinned). Snapshot consumption is publication-bound: an older receipt cannot
drain a newer same-hash replacement's baseline
(`publication_bound_snapshot_take_preserves_same_hash_aba_update`), retarget
clears the table, generic write-guard drops invalidate stale tokens, and the
baseline priority is snapshot → indexed content → symbols-only cache last
(known-finding #12; pinned by the `[Changed]`-precision asserts in
`test_write_hook_confirms_index`). The round-1 side-effects-before-503 defect
is closed by identity-only post-commit validation (§1). No additional
finding.

### Q4 — Single-file publication semantics

Re-audited every branch on the frozen code. Store CAS uses the health/live
generation domain everywhere (`bridge_only_publication_does_not_invalidate_live_index_reindex_cas`);
project generation is checked first and independently; winning publications
are returned under the writer lock; `Removed` is reported only when the
project-generation + under-lock-disk-absence removal actually published;
`Skipped`/`HashSkip`/`Reindexed` always carry their publication; every
no-publication path is `PublicationRejected`. Disk delete→recreate cannot
authorize removal on any of the four removal lanes (all pinned). Confirmed
convergence: unrelated same-project publications no longer starve confirmed
deletions (§1). Residual (pre-existing on main, unchanged): an
excluded-path eviction with no prior record still clones-and-publishes.
No additional finding.

### Q5 — Hook behavior matrix

Re-traced on frozen code; every row lands in valid fail-open JSON, bounded
request counts, honest attribution:

| Endpoint | Response | Behavior |
|---|---|---|
| local descriptor | 200 | `Routed`, 1 request. |
| local descriptor | 503 / 409 / 404-500 | one daemon fallback (root-pinned query, active-project-filtered session); success → `DaemonFallback`@daemon session; live failure → `SidecarError` with typed reason (`index_not_ready` / `root_conflict` / `http_failure`); no daemon → `SidecarError`@initial session. Never a restart hint, never `sidecar_port_stale`. |
| local descriptor | transport | fallback; if none, `NoSidecar sidecar_port_stale` + restart hint — correct, actually dead. |
| daemon descriptor | 503 | authoritative: zero rediscovery, exactly 1 request (request-count pinned). |
| daemon descriptor | 409 / 404-500 / transport | one rediscovery EXCLUDING the failed session; recovery through a different active session or typed fail-open; ≤2 enrichment requests, no loop (route-recording mocks pin the exact request sequences on both endpoints, including root-pinning of each request). |
| missing descriptor | — | step-1 daemon fallback; `NoSidecar sidecar_port_missing` otherwise. |
| stale descriptor | — | transport lane. |
| empty/whitespace legacy session id | — | normalized to `None` at read; routes locally, fallback-eligible — pinned end-to-end for both the 503 and 409 initial responses. |

`SessionSummary` serializes `project_id` from `active_project_id`
(`daemon.rs:705-712`, `1484-1491`), so the active-project filter holds against
the real daemon, not only the mocks. No additional finding.

### Q6 — Public API compatibility

Re-diffed the public surface across all changed files: `ReindexResult` keeps
exactly six variants (internal `ReindexOutcome` folds `PublicationRejected` →
`Skipped` at the embed boundary — main-compatible for exhaustive matchers);
`SidecarState::symbol_cache` is the type-identical alias
`SymbolSnapshotCache`; `take_pre_update_snapshot` keeps its signature (now
writer-locked; no public caller can hold that lock); additions are purely
additive (`take_pre_update_snapshot_at_generation`, the alias, fence-suffixed
snapshot-verify markers); everything else is `pub(crate)`.
`from_indexed_files`/`from_source_files` pre-exist on main. No break found.

### Q7 — Tests that can fail

The round-1 catalog stands (10-route loading matrix with no-mutation
asserts; independent queryability conjuncts; source-unbound-Ready refusal;
Empty first-file admission; exact receipts after later publication; same-hash
snapshot ABA; cross-project removal/publish/consume fences; shared impact
lock; unreadable-file baseline preservation; hook 503/409 no-loop/attribution
/no-hint suite). New at the frozen commit, each with a real smallest
reversion:

- `missing_file_finalization_ignores_unrelated_same_project_publication` —
  fails if the exact publication fence is reinstated on the absence path.
- `background_verify_deleted_file_removal_refuses_disk_recreation` and
  `scout_reconciliation_removal_refuses_a_recreated_disk_path` — fail if the
  under-lock disk-absence check is dropped from those lanes.
- `post_impact_fence_preserves_response_across_freshness_only_transition` —
  fails in one direction if the post-commit check re-includes freshness, and
  in the other if it stops rejecting a rebind.
- `snapshot_verify_transitions_preserve_other_degraded_reasons`,
  `healed_observation_restores_current_freshness`,
  `healed_observation_does_not_bypass_pending_snapshot_verification`,
  `degraded_initial_scout_is_published_in_the_trust_bundle`,
  `reconciled_coverage_publishes_freshness_once_per_transition` — pin the
  freshness engine's four properties (atomic construction, reason
  preservation, healing, no-mint); each fails against the naive direct-store
  implementations they replaced.
- `run_hook_local_http_500_fails_open_as_http_failure_without_restart_hint`,
  the empty/whitespace-session pair, and the descriptor-404/409
  rediscovery pair — the hook mocks now RECORD request routes and assert the
  exact sequences (`assert_request_routes`), 404 unexpected routes, and are
  deadline-bounded: the mock-accepts-wrong-path and unbounded-join failure
  modes are structurally addressed.

False-green audit: the queryability variant tests build synthetic
`PublishedGeneration` values — acceptable because the same conjuncts are also
exercised through the real Axum server in the loading matrix and
Empty-admission tests; the direct-helper tests are the decomposition, not the
only coverage. The CWD-lock env juggling in the enrichment suite remains
brittle-but-sound at `--test-threads=1`. No false-green fixture found.

### Q8 — Anything else materially wrong

No additional finding. Specifically re-checked after the freshness engine
landed: no path serves wrong-project data, claims global absence from partial
state, serves stale exact-path content after a rejected freshen (rejection
propagates as refusal on every read lane), invents success from a rejected
publication, waits unboundedly (all retry loops are bounded; all mocks and
locks deadline- or scope-bounded), or breaks the 10.0.x public surface.

## 5. Verdict

**Clear to land.** Every round-1 blocker is closed on the frozen commit and
re-verified here by both code reading and the full release-grade gate run on
the exact tree (fmt, clippy `-D warnings`, full `--all-targets` suite
including the H.7 perf gate on the machine that originally caught the
regression, release build, and both CI-canonical `verify-tools.cjs` harness
passes on the release binary — all green). The three P3 notes are design
consequences to confirm, not landing blockers; the only action item outside
the code is the §3 erratum in the request's own verify-tools command lines.
