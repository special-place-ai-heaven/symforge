# SymForge Stability Hotfix — Phase H (2026-05-12)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Repair catastrophic and high-severity defects surfaced by three external evaluator reports on 2026-05-11. SymForge is the single most leverage-bearing MCP for the user's other projects; a broken release destroys cross-project agent efficiency. This phase ships before resuming Phase 2.3.

**Insertion point in master campaign sequence:**

```
Phase 0 [shipped]
-> Phase 1 [shipped]
-> Phase 2.1 [shipped]
-> Phase 2.2 [shipped]
-> Phase H [THIS DOC] — stability hotfix
-> Phase 2.3 [queued]
-> Phase 2.4 [queued]
-> Phase 3 [queued — CoChange T3.3 ranker]
-> Phase 4 [queued — RTK Tier 1]
```

**Source-of-truth references:**

- `docs/notes/external-evaluations/2026-05-11/SYMFORGE_TEST_REPORT_2026-05-11_01.md` — AAP MCP sweep (Kieran-style auditor)
- `docs/notes/external-evaluations/2026-05-11/SYMFORGE_EVALUATION_2026-05-11.md` — Kimi Code CLI, identified P0 index-destruction
- `docs/notes/external-evaluations/2026-05-11/SYMFORGE_TEST_REPORT_2026-05-11_02.md` — Codex, identified P1 reference-engine defects
- `docs/notes/external-evaluations/2026-05-11/INVESTIGATION_B-P0-1.md` — read-only verification of P0 (Mechanism A confirmed at code level)
- `docs/notes/external-evaluations/2026-05-11/INVESTIGATION_HEALTH_REFS.md` — read-only verification of P1 health + reference bugs
- `docs/plans/2026-05-08-symforge-improvements-master.md` — parent campaign plan; this hotfix inserts between Phase 2.2 and Phase 2.3

**Verification standard (E:\project\symforge\CLAUDE.md):**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo clippy -- -D warnings
```

Run after every task. `--test-threads=1` mandatory.

---

## Bug catalog (verified)

All claims verified against current HEAD `f804d21` via parallel-read spot checks of the 11 most load-bearing code anchors. Investigator quotes match source.

### Tier 1 — Catastrophic (P0): data loss

| Bug | Subsystem | One-liner |
|---|---|---|
| **B-P0-1** | watcher | After `index_folder` on a new root, doomed reconcile task removes B's files using A's `repo_root`. 1135 → 4 files in 2 min on Windows. Mechanism A confirmed. Mechanism B refuted. Mechanism C latent. |

### Tier 2 — Correctness (P1): MCP returns wrong answers

| Bug | Subsystem | One-liner |
|---|---|---|
| **B-P1-1** | edit | `batch_rename` (incl. `dry_run`) times out at 120s. Refactor tool unusable. |
| **B-P1-2** | live_index/query | `find_dependents` Pass 2 attributes any `Vec::new()`, `.clone()`, `.handle()` to the target file once a single import stem matches. Inflates 1612-ref false positives. |
| **B-P1-3** | live_index/query + parsing/xref | `find_references` keys reverse_index on simple name; `Type::new()` becomes `name="new"` and the type prefix is invisible. `batch_rename`'s `find_qualified_usages` byte-scan catches them; the two collectors are not unified. |
| **B-P1-4** | live_index/query | `get_symbol_context` on large files times out at 120s. `max_tokens=5000` + `verbosity=signature` ignored during gathering. |
| **B-P1-5** | live_index/query + format | `get_file_context` 97s wall on 269-symbol file. `max_tokens` applied after gathering, not during rendering. Nested test mod (16074-line span) not collapsed. |
| **B-P1-6** | protocol/mod + format | `health` and `health_compact` disagree on watcher state ("active" vs "off") and load time. Root cause is daemon-proxy-vs-local-fallback split: a single failed proxy call sets `daemon_degraded=true` (sticky), and subsequent local fallback renders `WatcherInfo::default()` which is `Off`. Phase 1's render-shape work (`34e97fb`) widened the divergence but did not cause it. |
| **B-P1-7** | protocol/tools | `search_text(structural=true)` envelope label says `Match type: constrained (literal)`. `search_text_match_type_label` at `src/protocol/tools.rs:1852-1875` has no `structural` branch. |

### Tier 3 — Diagnostic + minor (P2/P3)

| Bug | Subsystem | One-liner |
|---|---|---|
| **B-P2-1** | parsing | Tree-sitter Rust grammar pre-2024 — does not parse `&raw const` / `&raw mut`. Affects own `src/live_index/persist.rs` (line 1199) and `src/worktree.rs` (line 296). Symbol counts under-report. |
| **B-P2-2** | parsing | `validate_file_syntax` reports "syntax error near line 1 col 1" with byte_span covering whole file. Outermost ERROR-node position instead of deepest. |
| **B-P2-3** | sidecar | Hook adoption 68% (220/322). 102 fail-open. `health` says "fail-open here is mostly benign" — wording masks the real symptom. |
| **B-P2-4** | live_index/coupling | Wiki/.obsidian/ shows as strongest coupling pair. Personal-tooling noise leaks into git temporal output. |
| **B-P2-5** | live_index/query | Untracked file (`what_changed` sees it) invisible to `search_files`/`search_text` until manual `analyze_file_impact(new_file=true)`. No diagnostic guides the user. |
| **B-P3-1** | protocol/format | `search_text(group_by="usage")` does not exclude markdown docs and Rust doc-comments. |
| **B-P3-2** | protocol/format | Truncation phrasing inconsistent across tools — `(N more omitted)` (search_text) vs `(truncated, N more)` (get_repo_map). Cosmetic; affects scannability. (Added 2026-05-12 round-2 walk item 7.) |
| **B-P3-3** | docs | Structural-pattern (`search_text(structural=true)`) lacks a documented cookbook of known-good Rust ast-grep patterns. Broad pattern `fn $NAME($$$) -> Result<(), ActorError>$$$` returned 0 matches without explanation. Docs task. (Added 2026-05-12 round-2 walk item 7.) |

---

## Task sequencing

Phase H is split across three sequencing buckets reflecting blocking-criticality (revised 2026-05-12 round-2 walk item 4 + round-3 verification): catastrophe-fix (C-1) lands now, user-trust + correctness fixes (C-2) land before Phase 2.3 resumes, and diagnostic + minor + incidental items (C-4) defer to a 1-week sprint within 30 days of Phase 4 close. (Original C-3 "opportunistic" bucket was collapsed into C-2 on round-2 walk item 4.)

### C-1: Catastrophe-blockers — land NOW

Insertion point: between Phase 2.2 (shipped) and Phase 2.3 (queued). These six tasks are non-negotiable; the campaign cannot proceed past Phase 2.2 without them because B-P0-1 destroys the index on every `index_folder` against a new root. (Walk item 13 + round-3 verification added H.1f to close matrix-row-2 coupling residual.)

1. **H.1a** — `SharedIndexHandle` generation-fence API (foundation; pure additive, new tests)
2. **H.1b** — Cooperative cancellation token in `ProjectInstance` + watcher loop (consumes H.1a)
3. **H.1c** — Migrate watcher mutations to fenced API; add Layer 3 re-stat-on-NotFound
4. **H.1d** — Sibling leak audit: `SymForgeServer::index_folder`/`restart_watcher`, sidecar `freshen_sidecar_path_if_stale`, session rebind
5. **H.1e** — Generation-fence git_temporal publication path (added 2026-05-12 round-2 walk; closes matrix-row-5 obligation)
6. **H.1f** — Generation-fence coupling_refresh publication path (added 2026-05-12 round-2 walk item 13; closes matrix-row-2 residual exposure)

Ordering within C-1 is fixed by dependency. **Do NOT begin H.1b until H.1a is reviewed and merged**, because H.1b consumes the API surface that H.1a defines. H.1c depends on both. H.1d depends on all three. H.1e and H.1f depend on H.1a's fenced-method pattern but are otherwise independent of H.1b/c/d and of each other; both can land in parallel after H.1a closes.

### C-2: User-trust + correctness — INTERLEAVE between C-1 close and Phase 2.3 resume (revised 2026-05-12 round-2 walk item 4)

**Restructured 2026-05-12 (round-2 walk, item 4)** per product-lens review: original sequencing deferred these P1 correctness fixes to between Phase 3.2 and Phase 3.3 (H.4, H.5) or before Phase 4 (H.2, H.6). The deferral left user-side trust broken through Phase 2.3 + 2.4 + 3.1 + 3.2 — agents using SymForge for cross-project work hit false `find_dependents` blast-radius claims, undercounted `find_references`, and divergent `health` daily. Plan owner authorized re-bucketing: all four P1 correctness fixes + the trivial structural-label fix land BEFORE Phase 2.3 resumes. (Original C-2 "ranker substrate" and C-3 "opportunistic" buckets are now merged into this single user-trust + correctness bucket. C-4 deferred-to-post-Phase-4 bucket remains unchanged.)

Net cost: ~7-9 days added between C-1 close and Phase 2.3 resume. Net benefit: user trust fully restored before any further feature work; Phase 3.3 inherits clean ranker substrate naturally.

5. **H.2** — health source-of-truth unification (~1 day; touches daemon-proxy + format)
6. **H.3** — `search_text` structural label fix (~30 min; trivial)
7. **H.4** — `find_dependents` Pass 2 constraint (~2 days; ranker substrate)
8. **H.5** — `find_references` qualified-path coverage via shared collector (~2-3 days; refactor substrate)
9. **H.6** — `get_symbol_context` / `get_file_context` budget enforcement (~2-3 days)
10. **H.7** — `batch_rename` timeout fix (~1-5 days; profile-first; scope unknown until measured) — promoted to C-2 from C-4 on 2026-05-12 round-2 walk items 9+10. Profile-first uncertainty: if profile reveals scope >3 days, escalate to plan owner for re-bucket decision rather than blocking C-2 close.

Within C-2, parallel batching has constraints (round-3 feasibility F4 — CLAUDE.md §12 requires sequential dispatch when agents touch the same files): H.2 and H.6 BOTH touch `src/protocol/format.rs`, so they must run **sequentially**, not as a parallel pair. H.4 + H.5 may pair in parallel because H.4 touches `src/live_index/query.rs` and H.5 primarily touches `src/protocol/edit.rs` + `src/protocol/tools.rs` (no overlap with H.4's query.rs). H.3 is trivial and slots into any free batch but touches `src/protocol/tools.rs` so should not run in parallel with H.5. H.7 stands alone — must profile first before pairing decisions. Suggested order: (H.4, H.5) parallel → H.2 → H.6 → H.3 → H.7.

### C-4: Diagnostic + minor + incidental items from evaluation (post-Phase-4 stability followup)

Diagnostic + minor correctness issues + **incidental items** surfaced during external evaluation that don't directly serve the stated Phase H goal of "restore trust in index subsystem" but are real bugs worth fixing (added "incidental" framing 2026-05-12 round-2 walk item 11 per scope-guardian feedback that H.11 + H.12 are personal-workflow polish, not index-subsystem repair). Examples of incidental items: H.11 (sidecar wording in health diagnostic), H.12 (Obsidian vault path classifier).

**Calendar commitment** (added 2026-05-12 round-2 walk items 9+10): C-4 lands as a **1-week sprint within 30 days of Phase 4 close**. Carve into separate plan-doc `docs/plans/2026-06-XX-symforge-stability-followup.md` (date TBD per actual Phase 4 close date). If the sprint cannot start within the 30-day window, plan owner re-evaluates remaining items for scope expansion vs further deferral. H.7 (P1 batch_rename) was promoted out of C-4 to C-2 on 2026-05-12 round-2 walk items 9+10 since its severity matched user-trust bucket better than diagnostic followup.

11. **H.8** — tree-sitter-rust grammar bump for `&raw const` / `&raw mut`
12. **H.9** — `validate_file_syntax` walk ERROR nodes to deepest position
13. **H.10** — Untracked file search diagnostic + opportunistic indexing
14. **H.11** — Sidecar PID/alive surfacing in `health`
15. **H.12** — Add `.obsidian/`, `wiki/.obsidian/` to `NoisePolicy::classify_path`'s personal-tooling set
16. **H.13** — Regression-suite gap analysis (added 2026-05-12 round-2 walk item 5) — audit which test would catch each verified Phase H bug; propose test-surface investments
17. **ADR: watcher-subsystem-spawn-blocking-discipline** — write the ADR codifying the cancellation-token + generation-fence + fenced-API convention (carved here from H.1d closing rule per round-2 walk; not enforced by lint, reviewer responsibility at PR time)

### Revised campaign sequence (post-2026-05-12)

```
[shipped: Phase 0, 1, 2.1, 2.2]
-> Phase H C-1 (H.1a -> H.1b -> H.1c -> H.1d; H.1e + H.1f parallel after H.1a)  [CATASTROPHE FIX]
-> Phase H C-2 (H.2, H.3, H.4, H.5, H.6, H.7)    [USER-TRUST + CORRECTNESS; before Phase 2.3]
-> Phase 2.3, 2.4                                [resume; small]
-> Phase 3.1-3.6                                 [CoChange data plumbing + fusion; ranker substrate already clean]
-> Phase 4                                       [RTK Tier 1]
-> Phase H C-4 (H.8-H.13 + ADR; incidental items)  [dedicated 1-week sprint within 30d of Phase 4 close]
```

(Sequence restructured 2026-05-12 round-2 walk item 4: original C-2/C-3 split into ranker-substrate-prereq vs opportunistic was collapsed into a single C-2 user-trust + correctness bucket landing before Phase 2.3 resumes. Walk item 13: added H.1f to C-1 in parallel with H.1e. Walk items 9+10: H.7 promoted to C-2, C-4 reduced to H.8-H.13 + ADR.)

### Phase H C-1 close-out gate (before C-2 begins)

- [ ] H.1a, H.1b, H.1c, H.1d, H.1e, H.1f all committed and pushed.
- [ ] B-P0-1 has a regression test (`tests/watcher_reload_cancellation.rs::reload_cross_root_preserves_file_count`) that fails on `f804d21` and passes on HEAD.
- [ ] B-P0-1 Layer-3 test (`tests/watcher_layer3_restat.rs`) passes on Windows.
- [ ] All four `cargo` verification commands green on `--test-threads=1` and on default parallelism.
- [ ] No regressions on the 1640+ pre-existing lib tests.
- [ ] Manual re-run of Kimi's repro (index large repo, idle 5 min, file count unchanged) confirmed.
- [ ] No catastrophic surface remaining: `SymForgeServer::index_folder` no longer leaks watchers; sidecar freshen calls cannot drive cross-root removes.
- [ ] **Automated AAP-shaped smoke** `tests/watcher_aap_shaped_fixture.rs::aap_smoke_no_destruction` passes: synthetic tempdir with ~1100 files in nested-crate layout (multiple `Cargo.toml`, `src/lib.rs`, `tests/` dirs); `index_folder` against root A; idle 5 min; `index_folder` against root B (disjoint layout); idle 5 min; assert both retain their file counts within tolerance ±2. Added 2026-05-12 (round-2 walk, item 2) per dogfood-gate option C — CI-side capture of the workload shape that B-P0-1 actually manifested under.
- [ ] **User-side dogfood** — user runs SymForge against ≥1 non-symforge project for ≥30 min session including ≥1 `index_folder` call against a new root; reports no catastrophic state loss (file count stable across idle periods, no ghost entries on freshen, no false-positive removes during normal cross-project workflow). Added 2026-05-12 (round-2 walk, item 2) per dogfood-gate option C — author-side CI cannot prove the leverage-bearing-MCP property; this gate captures user-side trust before C-2 starts.

### Phase H C-2 close-out gate (before resuming Phase 2.3) — added 2026-05-12 round-2 walk item 4

User-trust + correctness fixes must close before Phase 2.3 resumes per the restructure.

- [ ] H.2, H.3, H.4, H.5, H.6, H.7 all committed and pushed; each has its own regression test. (H.7 added 2026-05-12 round-2 walk items 9+10.)
- [ ] B-P1-1 `batch_rename` (incl. `dry_run`) completes within 5s wall on the evaluator's 13-site repro (was: 120s timeout).
- [ ] B-P1-2 `find_dependents` no longer attributes inflated false-positive refs from common method-name collisions; AAP-shape repro reduces to <5 false positives (orchestrator-scale).
- [ ] B-P1-3 `find_references` for a fully-qualified Rust call returns the call site (was: missing).
- [ ] B-P1-4 + B-P1-5 `get_symbol_context` / `get_file_context` complete in < 5s on a 16k-line file with >250 symbols.
- [ ] B-P1-6 `health` and `health_compact` agree on watcher state AND load_duration_ms when called against the same `(PublishedIndexState, WatcherInfo)` pair.
- [ ] B-P1-7 `search_text(structural=true)` envelope reads "structural (ast-grep)".
- [ ] All four `cargo` verification commands green on `--test-threads=1` and on default parallelism.
- [ ] No regressions on the 1640+ pre-existing lib tests (post C-1 baseline).
- [ ] Master plan-doc updated with C-2 completion timestamp.

---

# Tier 1 — Catastrophic

## Failure-mode coverage matrix (Tier 1 design rationale)

The catastrophe-fix lands across three layers because each closes a distinct failure mode that the other two cannot fully cover. Added 2026-05-12 per adversarial-reviewer recommendation (ADV-03) that the "Either alone leaves a hole" claim was unfalsified at the plan level.

| Spawn site (file:line) | Mechanism it can hit | Layer 1 — cancellation token | Layer 2 — generation fence | Layer 3 — re-stat retry |
|---|---|---|---|---|
| Periodic reconcile sweep (`src/watcher/mod.rs:559-575`) | A (cross-root) + C (AV lock) | Token cancels loop entry; per-path check breaks before each `freshen_file_if_stale` | Doomed task post-`paths.read()` cannot remove files in B's index — fence rejects | Single transient NotFound retries before remove |
| Coupling refresh (`src/watcher/mod.rs:569-575` spawn + `src/live_index/coupling/lifecycle.rs::refresh_on_reconcile_tick` body) | A (stale workspace; doomed task corrupts coupling store with A-era data after reload(B)) | Token check before spawn; **closure body is non-cancellable mid-execute** — Layer 2 is best-effort | **Best-effort pre-flight check** owned by Task H.1f (REDESIGNED round-3): `refresh_on_reconcile_tick(root, expected_gen, &shared)` re-checks gen at function entry; doomed-just-spawned task aborts immediately. **Mid-walk reload corruption is accepted residual** (annoying not catastrophic; surfaces in `rejected_stale_mutations` telemetry for monitoring). Original commit-boundary fence design was infeasible — CouplingStore writes are streamed, not batched. | Not applicable — no `remove_file` call |
| `process_events` handler (`src/watcher/mod.rs:429-488`) | A + C | Token check per-event AND per-batch-entry; Layer 2 load-bearing for any check skipped under high-throughput batches | Generation fence on every remove triggered by Remove event with stale path | Re-stat applies on event-driven NotFound |
| Overflow-triggered reconcile (`src/watcher/mod.rs:627-634`) | A + C | Token cancels before overflow sweep | Fence on every remove during overflow sweep | Re-stat per path |
| `git_temporal::spawn_git_temporal_computation` (called from `ProjectInstance::reload` at `src/daemon.rs:1101-1104`) | A (stale temporal published for wrong root) | Not applicable — long-running git walks acceptable to let complete; fence prevents result corruption | Owned by **Task H.1e** — adds `update_git_temporal_at_generation` to `SharedIndexHandle`; rejects A-era publication after reload(B). Replaces the original "extend Layer 2 to cover `git_temporal::swap`" placeholder (note: `swap` was a misnomer for the actual `ArcSwap::store`-based API `update_git_temporal`) | Not applicable — no `remove_file` call |

**Why Layer 1 alone is insufficient:** A doomed reconcile that has already passed its loop-entry cancellation check and is mid-iteration when the parent signals can still write the next `remove_file` before the per-path check fires. The gap between `paths.read()` and execute-remove is non-zero. Layer 2 closes the race window at the index boundary.

**Why Layer 2 alone is insufficient:** Without Layer 1, doomed tasks run to completion across many reconcile cycles, holding spawn_blocking threads and accumulating fence-rejection telemetry per iteration. The fence rejects mutations but does not free resources. Layer 1 ensures doomed tasks exit promptly.

**Why Layer 3 alone is insufficient:** Re-stat retry catches Mechanism C (transient AV/lock) only. Without Layers 1+2, Mechanism A still destroys the index because the doomed task removes files that DO exist at their A-joined path even when re-stat passes.

**Note for future contributors (non-binding until ADR):** when adding a new `spawn_blocking` inside the watcher subsystem that performs index mutations, the safe pattern is (a) accept a `CancellationToken` clone, (b) capture `expected_gen` at task entry, (c) consume the fenced API (`*_at_generation`) for all mutations. Read-only spawn sites (telemetry, instrumentation, metrics) do not need (b) or (c) — the convention applies only to mutation sites. (Demoted from MUST to convention 2026-05-12 per round-2 reviews — product-lens, scope-guardian, adversarial all flagged the binding rule as exceeding hotfix scope and lacking enforcement mechanism.) The convention should be captured in an ADR — carved into C-4 as new task `ADR: watcher-subsystem-spawn-blocking-discipline` (out-of-scope for Phase H; lands in the post-Phase-4 stability sprint per Option C sequencing). The convention is not enforced by a lint or CI gate today; reviewers should check new watcher spawn sites at PR time and verify they follow the convention or document an explicit deviation.

---

## Task H.1a: SharedIndexHandle generation-fence API (additive)

**Severity:** P0 foundation. No behavior change yet; pure API addition + tests. Required before H.1b can land.

**Files (allowed):**

- Modify: `src/live_index/store.rs` — extend `SharedIndexHandle` with project-generation accessor + fenced-mutation methods.
- Modify: `tests/live_index_publish_atomicity.rs` (or new file) — add deterministic generation-fence tests.
- Forbidden: any file outside `src/live_index/store.rs` and the test file. No caller migration in this task.

**Context:** `SharedIndexHandle` at `src/live_index/store.rs:433-448` already carries `next_generation: AtomicU64`. Inspection: this field is internal to `PublishedIndexState` versioning; bumped per `swap_and_publish`. Therefore it is wrong to reuse for project identity (a single `update_file` would bump it and invalidate every doomed-task fence).

The fix introduces a SEPARATE `project_generation: AtomicU64` field that tracks project-identity, bumped ONLY on `SharedIndexHandle::reload`. Add fenced-mutation methods that re-read this generation under the existing `write_mutex` and short-circuit on mismatch.

**Spec (per CLAUDE.md §2.2):**

- **objective:** `SharedIndexHandle` exposes `current_project_generation() -> u64`, `remove_file_at_generation(path, expected_gen) -> bool`, `update_file_at_generation(path, indexed, expected_gen) -> bool`, `touch_mtime_at_generation(path, mtime, expected_gen) -> bool`. `reload` bumps `project_generation` after a successful index swap and before returning.
- **non_goals:** No watcher caller migration. No `process_events` change. No `maybe_reindex` change. No public API removal. Existing `remove_file`, `update_file`, `touch_mtime` keep their current behavior (used by non-watcher code).
- **allowed_files:** `src/live_index/store.rs`, the test file (`tests/live_index_publish_atomicity.rs` or a new dedicated `tests/live_index_generation_fence.rs`).
- **forbidden_files:** Everything else.
- **interfaces touched:** `SharedIndexHandle` public surface — adds 4 methods. No removal. No signature change on existing methods.
- **invariants:** Existing `remove_file`/`update_file`/`touch_mtime` callers see unchanged behavior. New fenced methods are no-ops on generation mismatch and return `false`. `project_generation` is monotonically increasing.
- **acceptance_criteria:**
  - [ ] `cargo check` clean.
  - [ ] `cargo clippy -- -D warnings` clean.
  - [ ] Test: `generation_fence_blocks_stale_remove` — capture `gen_a`, call `reload(root_b)`, call `remove_file_at_generation("a/file.rs", gen_a)`, assert result is `false` and index file count unchanged.
  - [ ] Test: `generation_fence_allows_current_remove` — capture `gen_b = current_project_generation()` after a reload, call `remove_file_at_generation(<path indexed under b>, gen_b)`, assert result is `true` and the file is removed.
  - [ ] Test: `generation_bumps_on_reload_only` — call `update_file`, assert `current_project_generation()` unchanged; call `reload`, assert `current_project_generation()` strictly increases.
  - [ ] Test: `rejected_stale_mutations_counter_increments_on_fence_rejection` — capture `gen_a = current_project_generation()`; assert `current_rejected_stale_mutations() == 0`; call `reload(root_b)` (bumps to `gen_b`); call `remove_file_at_generation(path, gen_a)`; assert it returns `false`; assert `current_rejected_stale_mutations() == 1`. Then call `update_file_at_generation(path, indexed, gen_a)`; assert returns `false`, counter is now `2`. Telemetry test added 2026-05-12 round-2 walk item 12.
  - [ ] Existing `tests/live_index_publish_atomicity.rs` tests still pass.
- **evidence_required:**
  ```
  cargo test -p symforge --test live_index_publish_atomicity -- --test-threads=1
  cargo test -p symforge --test live_index_generation_fence -- --test-threads=1   # if added as new file
  cargo clippy -- -D warnings
  ```
- **stop_conditions:** STOP if `project_generation` integration breaks `PublishedIndexState::capture` or any of the existing publish-atomicity invariants. STOP if the fenced-method signatures conflict with `ArcSwap` write-mutex discipline. STOP if benchmark shows write-mutex contention regression > 5%.

**Steps:**

- [ ] **Step 1: Invoke `superpowers:test-driven-development`.** Write the three new tests first against the current code (they should all fail to compile).
- [ ] **Step 2: Read `src/live_index/store.rs:433-720` carefully.** Confirm:
  - `next_generation: AtomicU64` semantics (bumped by `swap_and_publish`).
  - `write_mutex: Mutex<()>` discipline.
  - `reload` location and what it does today.
- [ ] **Step 3: Add `project_generation: AtomicU64` AND `rejected_stale_mutations: AtomicU64`** to `SharedIndexHandle`. Initialize `project_generation` to `0`, `rejected_stale_mutations` to `0`. Document three distinct atomic counters now on the struct: `next_generation` (publish versioning, internal), `project_generation` (project identity, used by fence), and `rejected_stale_mutations` (telemetry counter incremented on each fence rejection — added 2026-05-12 round-2 walk item 12 per product-lens silent-skip-risk concern). The counter is for observability only; never used in correctness paths.
- [ ] **Step 4: Implement `pub fn current_project_generation(&self) -> u64`** — `self.project_generation.load(Ordering::Acquire)`.
- [ ] **Step 5: Implement `pub fn remove_file_at_generation(&self, path: &str, expected_gen: u64) -> bool`:**
  - Take `_wg = self.write_mutex.lock()`.
  - Re-read `current = self.project_generation.load(Ordering::Acquire)` under the lock.
  - If `current != expected_gen`, **increment `self.rejected_stale_mutations` via `fetch_add(1, Ordering::Relaxed)`** and return `false` (trace-only log; do not log at warn level by default to avoid spamming on every doomed iteration). Telemetry increment added 2026-05-12 round-2 walk item 12.
  - Otherwise replicate the existing `remove_file` body and return `true`.
- [ ] **Step 6: Implement `update_file_at_generation` and `touch_mtime_at_generation`** analogously — each increments `rejected_stale_mutations` on generation mismatch. Also implement `pub(crate) fn current_rejected_stale_mutations(&self) -> u64` accessor that returns `self.rejected_stale_mutations.load(Ordering::Relaxed)` (used by H.2 to render in `health` output, and by H.1f for coupling-refresh telemetry). **Visibility is `pub(crate)`** (round-3 feasibility F3): this is internal observability, not a stable public API contract — could be removed in future without breaking external consumers.
- [ ] **Step 7: Modify `SharedIndexHandle::reload`** so it bumps `project_generation` AFTER the index swap succeeds and BEFORE returning. Use `.fetch_add(1, Ordering::AcqRel)`.
- [ ] **Step 8: Run the three new tests.** Confirm they pass.
- [ ] **Step 9: Run the full `cargo test --all-targets -- --test-threads=1`.** Confirm no regression.
- [ ] **Step 10: HALT for review.** Do not begin H.1b. Produce a verification report and request authorization to commit.

**Verification commands:**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

**Commit message:**

```
feat(live-index): add project-generation fence for SharedIndexHandle mutations

Adds project_generation: AtomicU64 to SharedIndexHandle, distinct from
the existing next_generation publish-versioning counter. Exposes:

- current_project_generation() -> u64
- remove_file_at_generation(path, expected_gen) -> bool
- update_file_at_generation(path, indexed, expected_gen) -> bool
- touch_mtime_at_generation(path, mtime, expected_gen) -> bool

reload() bumps project_generation atomically after a successful swap.
Fenced-mutation methods re-read the generation under write_mutex and
short-circuit on mismatch.

No existing caller is migrated in this commit. This is the foundation
for fixing B-P0-1 (index self-destruction by doomed watcher tasks).
See docs/plans/2026-05-12-symforge-stability-hotfix.md.
```

---

## Task H.1b: Cooperative cancellation token in ProjectInstance + run_watcher

**Severity:** P0. Consumes H.1a's `current_project_generation()` accessor as a layered defense. Must land in the same atomic shipping window as H.1c.

**Files (allowed):**

- Modify: `src/daemon.rs` — `ProjectInstance` struct, `reload`, `activate`, `abort_watcher_task`, `start_project_watcher`, `index_folder_for_session` removal site.
- Modify: `src/watcher/mod.rs` — `run_watcher` signature, inner loop polling, `reconcile_stale_files` signature, `process_events` signature, fire-and-forget reconcile sites.
- Modify: `Cargo.toml` if `tokio-util` crate addition is chosen (for `CancellationToken`); otherwise no Cargo change.
- Create: `tests/watcher_reload_cancellation.rs` — Shape-B integration test (reload-induced cross-root destruction).
- Forbidden: `src/live_index/store.rs` (no further changes here — H.1a covers it). `src/protocol/tools.rs::index_folder` (covered by H.1d).

**Context:** `ProjectInstance::reload` at `src/daemon.rs:1070-1107` calls `abort_watcher_task` (literal `task.abort()`, no cooperative signaling) then re-spawns. Old watcher's `spawn_blocking` reconcile children at `src/watcher/mod.rs:559-575` are fire-and-forget; the parent abort never reaches them. They hold `Arc<SharedIndexHandle>` and the captured `repo_root` (root A). Each child sweep walks `shared.read().all_files()` (which now contains B's paths after reload), constructs `root_A.join(path_from_B)`, gets `NotFound`, and calls `shared.remove_file`.

**Decision: use `tokio_util::sync::CancellationToken`** unless the dependency footprint is objectionable. Alternative: `Arc<AtomicBool>`. Decide in Step 2.

**Spec:**

- **objective:** A doomed watcher task observes a cooperative stop signal within ≤ 1 reconcile interval (default 30s, test-configurable to 1s) and exits its `reconcile_stale_files` loop. `ProjectInstance::reload` signals the stop token BEFORE swapping in the new watcher task. The new task captures a fresh token.
- **non_goals:** Do NOT replace `task.abort()` — keep it as a backstop. Do NOT change `SharedIndexHandle` API (H.1a covers that). Do NOT touch `SymForgeServer::index_folder` (H.1d).
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** `run_watcher` gains a `stop_token: CancellationToken` parameter. `reconcile_stale_files` gains a `should_stop: &dyn Fn() -> bool` parameter (or a token clone). `process_events` gains the same. Public `restart_watcher` and `start_project_watcher` change signatures or accept defaults.
- **invariants:** A doomed task that misses the cancellation signal must still be unable to corrupt the index (defense in H.1c). Cancellation is cooperative; in-flight `std::fs::read` calls complete before the loop exits.
- **acceptance_criteria:**
  - [ ] New test `tests/watcher_reload_cancellation.rs::reload_cross_root_preserves_file_count` — build two tempdirs A (50 files) and B (30 files), drive the equivalent of `ProjectInstance::reload(B)`, wait 3× the test-configured reconcile interval, assert `index.published_state().file_count == 30` and every B file is reachable via `get_file`.
  - [ ] New test `reload_signals_token_before_new_watcher` — capture an old-task token state, call reload, assert the old token observes cancellation strictly before the new task is spawned.
  - [ ] No existing test regresses.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:**
  ```
  cargo test -p symforge --test watcher_reload_cancellation -- --test-threads=1
  cargo test --all-targets -- --test-threads=1
  ```
- **stop_conditions:** STOP if `CancellationToken` introduces compile-time cost > 2s. STOP if cancellation polling adds measurable latency to steady-state event processing. STOP if `reconcile_stale_files`'s new signature breaks a non-watcher caller (unlikely — it's `pub(crate)` and called from one site).

**Steps:**

- [ ] **Step 1: Invoke `superpowers:test-driven-development`.** Write `tests/watcher_reload_cancellation.rs` with both tests; they should fail before implementation.
- [ ] **Step 2: Decide cancellation-primitive.** Read `Cargo.toml` for existing tokio-related deps; if `tokio-util` is not present, add it with feature `"sync"` only. Alternative: `Arc<AtomicBool>` with `Ordering::Acquire/Release`. Document the choice in the commit message.
- [ ] **Step 3: Add `stop_token` field to `ProjectInstance`** (`src/daemon.rs:82-97`). Initialize in `activate`. Bump-and-create a fresh token in `reload` BEFORE calling `abort_watcher_task`.
- [ ] **Step 4: Modify `abort_watcher_task`** so it signals the token before calling `task.abort()`. The task abort remains as a hard stop for non-cooperative work (synchronous blocking calls that never yield).
- [ ] **Step 5: Thread `stop_token` into `start_project_watcher`** and through `watcher::run_watcher`. Use `tokio::select!` against `stop_token.cancelled()` at the top of each inner-loop iteration.
- [ ] **Step 6: Modify `reconcile_stale_files`** to accept a closure-or-token and check it between each path. The loop at `src/watcher/mod.rs:344-349` becomes:
  ```rust
  for relative_path in &paths {
      if should_stop() { break; }
      let abs_path = repo_root.join(relative_path);
      if freshen_file_if_stale(...) { stale_count += 1; }
  }
  ```
- [ ] **Step 7: Thread the token into the three fire-and-forget `spawn_blocking` sites** at `src/watcher/mod.rs:559-575` and `:627-634`. Each clone the token before passing into the blocking closure.
- [ ] **Step 8: Modify `process_events`** to check the token between events AND between event-internal paths (per round-2 adversarial review ADV-R2-01). A batch carrying many events under buffer-overflow or coalesced-debounce conditions can fire many removes without the token-check ever firing if the check is only at batch entry. Per-event (and where the event has multiple paths, per-path) checks ensure mid-batch cancellation is honored. Layer 2's generation fence remains the load-bearing protection for any check that gets skipped under high throughput; the matrix correctly annotates Layer 2 as covering intra-batch removes.
- [ ] **Step 9: Modify `index_folder_for_session`** removal site (`src/daemon.rs:504-508`) to signal the removed project's token before letting the `ProjectInstance` drop.
- [ ] **Step 10: Run new test.** Confirm both pass.
- [ ] **Step 11: Run full test suite.** Confirm no regression.
- [ ] **Step 12: HALT for review.** Produce verification report. Do not commit until reviewed.

**Verification commands:**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo clippy -- -D warnings
```

---

## Task H.1c: Migrate watcher mutations to fenced API + Layer 3 re-stat-on-NotFound

**Severity:** P0 defense-in-depth. Even with H.1b in place, an already-running blocking call can race; H.1c rejects stale mutations at the index boundary.

**Files (allowed):**

- Modify: `src/watcher/mod.rs` — capture project generation at watcher spawn, pass through to `maybe_reindex` and `process_events`, call fenced mutation API. Add Layer 3 re-stat retry.
- Create: `tests/watcher_layer3_restat.rs` — Shape A test (single-watcher false-positive removal under transient lock).
- Forbidden: `src/daemon.rs`, `src/live_index/store.rs`, `src/protocol/tools.rs`, sidecar files.

**Context:** Doomed `spawn_blocking` tasks may have read `paths` from the shared handle before the cancellation arrived, and then start removing. The fence at the index boundary (H.1a) ensures stale producers cannot mutate; H.1c is the consumer side.

Layer 3 addresses Mechanism C: AV/IDE/build-tool exclusive lock returns `NotFound` instead of `PermissionDenied`. A short re-stat retry eliminates the spurious removal.

**Spec:**

- **objective:** Every `shared.remove_file` / `shared.update_file` / `shared.touch_mtime` call originating from `run_watcher` consumes the fenced API (`*_at_generation`) with the generation captured at task spawn time. `maybe_reindex` retries the stat+read+hash+parse+update pipeline up to 3 times with bounded backoff (50/200/500ms, ~750ms total) before falling through to the removal path on persistent `NotFound`. (Objective updated 2026-05-12 per round-2 coherence review — was "retries once with 50ms" which contradicted Step 6.)
- **non_goals:** No change to non-watcher callers of `shared.remove_file` (sidecar callers, tools.rs `freshen_exact_path_for_targeted_retrieval`, etc. — those go to H.1d).
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** `maybe_reindex` signature gains an `expected_gen: u64` parameter. `process_events` signature gains the same. `reconcile_stale_files` already receives the token from H.1b; now also receives `expected_gen`.
- **invariants:** A doomed task that slipped past H.1b's cancellation signal is rejected at the fenced API and emits a single trace-level event per task lifetime (not per call — that would spam). Layer 3's retry adds at most 3 extra pipeline executions per persistent-NotFound event (4 attempts total), totaling at most ~750ms of `std::thread::sleep` time per NotFound. (Invariant updated 2026-05-12 per round-2 coherence review — was "at most one extra metadata call" which contradicted Step 6.)
- **acceptance_criteria:**
  - [ ] New test `watcher_layer3_restat::transient_av_lock_does_not_remove_file` — Windows-gated. Open a file with `FILE_SHARE_NONE`, trigger reconcile, release lock, assert file still indexed. (On non-Windows, mark the test ignored with a documented reason.)
  - [ ] New test `watcher_layer3_restat::permanent_deletion_still_removes` — delete a file, trigger reconcile, assert the file is removed. Tests Layer 3 does not break legitimate removal.
  - [ ] New test `watcher_layer3_restat::bulk_deletion_storm_completes_within_baseline` — added 2026-05-12 per round-2 product-lens review: trigger synchronized deletion of 100+ files (simulating cargo clean / git checkout / rm -rf), measure wall-clock time to index convergence. Assert convergence within 2x baseline (where baseline is the pre-Layer-3 implementation, measured by setting `delays_ms = [0, 0, 0]`). Protects against `tokio::spawn_blocking` pool starvation from N × 750ms blocked-thread time under legitimate bulk-delete workflows. Use cases this catches: cross-project `cargo clean` from the user's other repos that destroy thousands of `target/` files in seconds.
  - [ ] Augment the H.1b test `reload_cross_root_preserves_file_count` to also catch the doomed-task-slips-past-cancellation case: spawn the doomed task such that it already read `paths` before cancellation; assert fence rejects its removes.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:**
  ```
  cargo test -p symforge --test watcher_layer3_restat -- --test-threads=1
  cargo test -p symforge --test watcher_reload_cancellation -- --test-threads=1
  cargo test --all-targets -- --test-threads=1
  ```
- **stop_conditions:** STOP if the 750ms total backoff measurably degrades steady-state reconcile throughput. STOP if a deletion storm regression test shows wall-time worse than 2x baseline. STOP if fenced API integration breaks any existing watcher test. STOP if the doomed-task slips-past-cancellation test cannot be made deterministic. (Stop conditions updated 2026-05-12 per round-2 coherence + product reviews — was scoped to "50ms delay" which is no longer the implementation.)

**Steps:**

- [ ] **Step 1: Invoke `superpowers:test-driven-development`.**
- [ ] **Step 2: Modify `run_watcher`** to capture `let expected_gen = shared.current_project_generation();` at task entry. Pass into all three `spawn_blocking` closures and into `process_events`.
- [ ] **Step 3: Modify `reconcile_stale_files`** signature to accept `expected_gen: u64`. Pass into `freshen_file_if_stale`.
- [ ] **Step 4: Modify `freshen_file_if_stale`** signature to accept `expected_gen`. Pass into `maybe_reindex`.
- [ ] **Step 5: Modify `maybe_reindex`** signature to accept `expected_gen`. Replace all `shared.remove_file(...)`, `shared.update_file(...)`, `shared.touch_mtime(...)` calls with their `*_at_generation` counterparts.
- [ ] **Step 6: Add `ReindexResult::NotFound` variant + Layer 3 retry** in `src/watcher/mod.rs` (variant at lines 146-155; retry-loop replaces current 233-244 NotFound arm). Use bounded exponential backoff via an extracted helper so the retry loop has a clear control-flow target and the delay is not an unmeasurable fixed-50ms guess. (Rewritten 2026-05-12 per round-1 reviews ADV-02 + feasibility on control-flow + round-2 reviews FEAS-R2-01 enum-variant + ADV-R2-05 metadata-stat + coherence-R2 Layer-3 scope drift + feasibility-R2-04 TOCTOU.)

  **First, declare a new `ReindexResult::NotFound` variant** at `src/watcher/mod.rs:146-155`, documented as "ENOENT observed by `read_and_index`; caller decides whether to retry or treat as confirmed-absent." The existing `Removed` variant retains semantics of "removed from index after confirmed absence."

  **Then extract the stat+read+hash+parse+update pipeline** (current `maybe_reindex` body at lines 222-279, INCLUDING the `fs::metadata` stat at lines 227-232) into a helper `read_and_index(relative_path, abs_path, shared, language, expected_gen) -> ReindexResult`. The helper re-runs `std::fs::metadata` on each call so the TOCTOU mtime-before-content invariant documented at lines 222-226 is preserved across retries — this is intentional. The helper's `NotFound` branch (returned when either `fs::metadata` OR `std::fs::read` produces ENOENT) returns the new `ReindexResult::NotFound` variant WITHOUT calling `remove_file_at_generation`, leaving the removal decision to the retry-loop caller.

  **Note on Layer-3 scope:** the matrix column, this task's heading, the test names (`watcher_layer3_restat::*`), and the section headers use "re-stat retry" as terminology. The actual implementation retries the full stat+read+hash+parse+update pipeline, not just `fs::metadata`. This intentional broadening (round-2 coherence + adversarial reviews) catches Mechanism C transient locks on either syscall. Terminology "re-stat retry" remains for continuity with the task heading and matrix; treat it as shorthand for "post-NotFound retry."

  Replace the NotFound arm of `maybe_reindex` with a bounded backoff loop:

  ```rust
  let delays_ms = [50u64, 200, 500];
  for &delay_ms in delays_ms.iter() {
      match read_and_index(relative_path, abs_path, shared, language, expected_gen) {
          ReindexResult::NotFound => {
              std::thread::sleep(std::time::Duration::from_millis(delay_ms));
              continue;
          }
          other => return other,
      }
  }
  // After 50ms + 200ms + 500ms (= 750ms total backoff), the file is still NotFound.
  // Treat as confirmed absent.
  shared.remove_file_at_generation(relative_path, expected_gen);
  warn!("watcher: file not found after retries, removed from index: {relative_path}");
  ReindexResult::Removed
  ```

  Three retries cap transient-handling at ~750ms total backoff (4 attempts total: initial + 3 retries). If AV/IDE lock holds longer, the file is treated as absent and removed — surface this hold-time threshold as a regression-test parameter so the choice is explicit rather than implicit. The extracted helper ensures the pipeline runs once per attempt without duplicating the body across the original NotFound arm and the retry arm.
- [ ] **Step 7: Modify `process_events`** so removal-event handling uses `remove_file_at_generation`. Update sites: `src/watcher/mod.rs:429-488`.
- [ ] **Step 8: Run new tests.** Confirm pass.
- [ ] **Step 9: Run full test suite + clippy.** Confirm no regression.
- [ ] **Step 10: HALT for review.**

**Verification commands:**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo clippy -- -D warnings
```

---

## Task H.1d: Sibling leak surfaces — index_folder, restart_watcher, sidecar freshen

**Severity:** P0. With H.1a-c shipped, the watcher subsystem is safe, but three other code paths still create the same race:

1. `SymForgeServer::index_folder` at `src/protocol/tools.rs:4438-4444` calls `restart_watcher` WITHOUT aborting the prior watcher.
2. `restart_watcher` at `src/watcher/mod.rs:676-686` spawns a fresh `run_watcher` but does not stop the previous one.
3. Sidecar handlers and tools.rs callers of `freshen_file_if_stale` (`src/protocol/tools.rs:1659-1728`, `src/sidecar/handlers.rs:180`) build `abs_path = server.capture_repo_root().join(rel)` per request; if a concurrent `index_folder` reassigned the server's `repo_root`, the stale captured root drives the same NotFound→remove chain.

**Files (allowed):**

- Modify: `src/protocol/tools.rs` — `SymForgeServer::index_folder` path; `freshen_exact_path_for_targeted_retrieval`, `prepare_exact_path_for_edit`, `prepare_batch_paths_for_edit`. Define `EditError` enum (derives `thiserror::Error`) near the existing freshen helpers (~line 1659).
- Modify: `src/protocol/mod.rs` — added 2026-05-12 after round-3 adversarial + architecture review. Add ONE `watcher_handle: Arc<Mutex<Option<crate::watcher::WatcherHandle>>>` field to `SymForgeServer` struct (line 46); initialize as `Arc::new(Mutex::new(None))` in `new` (line 79) and `new_daemon_proxy` (line 102). No other changes in mod.rs. Test helpers `make_server` / `make_server_with_root` in `tools.rs:7771/7785`, `resources.rs:422/481`, `prompts.rs:477` already call `SymForgeServer::new(...)` (audit 2026-05-12) so they remain untouched as long as `::new` signature stays the same.
- Modify: `src/watcher/mod.rs` — `restart_watcher` signature so it can abort the prior watcher cooperatively. Define new public newtype `WatcherHandle { task: JoinHandle<()>, stop_token: Arc<AtomicBool> }` co-located with the existing watcher infrastructure; this is the ownership-pair type returned by `restart_watcher` and stored by callers.
- Modify: `src/sidecar/handlers.rs` — `freshen_sidecar_path_if_stale`.
- Create: `tests/watcher_index_folder_leak.rs` — repro for SymForgeServer leak.
- Forbidden: `src/daemon.rs` (already covered by H.1b), `src/live_index/store.rs`. `ProjectInstance`-side unification of the (task, stop_token) pair into `WatcherHandle` is deferred to a follow-up task — H.1d only refactors the new server-side surface.

**Spec:**

- **objective:** No code path holds two concurrent watchers writing into the same `SharedIndexHandle`. Per-request `freshen_file_if_stale` callers either reject the request on root-mismatch or use the fenced API.
- **non_goals:** Do NOT remove `restart_watcher` entirely — that breaks compatibility. Modify it to accept a prior `WatcherHandle` (newtype bundling task + stop_token) so it can signal-and-bounded-await before spawning. Do NOT unify the `ProjectInstance` (daemon.rs) task+token pair with `WatcherHandle` in this task — that is a follow-up (forbidden file).
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** `src/watcher/mod.rs` gains public newtype `WatcherHandle { task: JoinHandle<()>, stop_token: Arc<AtomicBool> }` (Arc<AtomicBool> chosen for consistency with the H.1b/H.1c watcher stop-token plumbing already shipped — NOT `tokio_util::sync::CancellationToken`). `restart_watcher` signature gains `prev: Option<WatcherHandle>` and returns `WatcherHandle`. `SymForgeServer` (in `src/protocol/mod.rs`) gains ONE field `watcher_handle: Arc<Mutex<Option<WatcherHandle>>>` (single field bundling the pair, not two; `Arc<Mutex<...>>` mandatory because `SymForgeServer` is `#[derive(Clone)]` and `Mutex<Option<JoinHandle>>` is not Clone). `tools.rs` gains `EditError` enum with `thiserror::Error` derive, two variants `PathNotFound { path: PathBuf }` + `SessionStale { path: PathBuf }`. Helper-boundary contract preserved: `prepare_exact_path_for_edit`, `prepare_batch_paths_for_edit`, `freshen_exact_path_for_targeted_retrieval` continue to return `Result<_, String>` to callers — internal `EditError` is `format!("{e}")`-converted at the helper boundary; the 11 existing callsites at `tools.rs:6708/6895/7029/7160/7398/7523` and retrieval callsites at `3079/4764/4857` are untouched (audit 2026-05-12 confirmed all use `Err(e) => return e` pure-passthrough, no substring matching).
- **invariants:** At most one active watcher per `SharedIndexHandle`. Per-request freshen calls cannot drive a remove without consulting the current project generation.
- **acceptance_criteria:**
  - [ ] New test `watcher_index_folder_leak::repeated_index_folder_preserves_file_count` — call `SymForgeServer::index_folder` twice on different roots, wait > reconcile interval, assert second root's file count is intact.
  - [ ] Augment one of the sidecar integration tests (or add a new one) to assert that a concurrent root reassign during `freshen_sidecar_path_if_stale` does not remove a valid file.
  - [ ] New tests added 2026-05-12 per round-2 adversarial review ADV-R2-04, verifying each of the three callsites preserves its distinct success-path contract after fence migration: `prepare_exact_path_for_edit_returns_err_on_confirmed_absent`; `freshen_exact_path_for_targeted_retrieval_removes_on_confirmed_absent_with_fence`; `prepare_batch_paths_for_edit_partial_succeeds_with_skipped_paths`. Plus `freshen_helper_returns_generation_mismatch_when_gen_stale` to exercise the new `FreshenResult::GenerationMismatch` variant.
  - [ ] **Session-rebind surface audit** (added 2026-05-12 round-2 walk item 8): cross-check that H.1b Step 9's modification of `index_folder_for_session` at `src/daemon.rs:504-508` has correctly signaled the removed project's stop token before the `ProjectInstance` drops. Add new test `index_folder_for_session_signals_token_before_drop` (lives in H.1d's test file rather than H.1b's, so H.1d's audit is the cross-task safety net): drive `index_folder_for_session` against an existing project, intercept the token signal, assert the removed project's doomed `spawn_blocking` reconcile children observe the cancellation before their next iteration. If H.1b's Step 9 modification is missing or incomplete when H.1d starts, escalate to plan owner at HALT (do NOT silently extend H.1d's allowed_files to cover daemon.rs).
  - [ ] **Exhaustive-match guardrail** (added 2026-05-12 round-3 adversarial review ADV-H1D-06): the `FreshenResult` enum carries `#[must_use]`, and every callsite in the three public helpers matches all four variants explicitly. No `_ =>` wildcard arms, no `if let Some(...) = ...` shortcuts that silently drop unhandled variants. Verify by grepping the post-implementation diff for `FreshenResult::` — each callsite must list all four variants. This compile-time guardrail prevents future drift from silently degrading the per-callsite contract.
  - [ ] **`#[derive(Clone)]` preservation** (added 2026-05-12 round-3 adversarial review ADV-H1D-02): post-implementation `cargo check` must compile `SymForgeServer`'s existing `#[derive(Clone)]` without modification. The new `watcher_handle` field MUST be `Arc<Mutex<Option<WatcherHandle>>>` (Arc-wrapped) because `Mutex<...>` is not Clone. Plain `Mutex<Option<JoinHandle<()>>>` would compile-fail and any hand-implemented Clone would silently lose shared-state semantics.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:**
  ```
  cargo test -p symforge --test watcher_index_folder_leak -- --test-threads=1
  cargo test -p symforge --test sidecar_integration -- --test-threads=1
  cargo test --all-targets -- --test-threads=1
  ```
- **stop_conditions:** STOP if `SymForgeServer` lacks a place to hold the prior watcher handle. Round-3 adversarial review (2026-05-12) confirmed via grep audit that no test files perform struct-literal `SymForgeServer { ... }` init — all paths go through `SymForgeServer::new(...)` — so the field addition is contained to `src/protocol/mod.rs` with `::new` signature preserved. STOP if any sidecar test that previously relied on the racy behavior breaks. STOP if the `WatcherHandle` newtype placement in `src/watcher/mod.rs` collides with an existing symbol of the same name (grep first; rename to `WatcherTaskHandle` if necessary). STOP if `parking_lot::Mutex` cannot be used for `watcher_handle` because some code path requires holding the lock across `.await` — switch to `tokio::sync::Mutex` and document the deviation.

**Steps:**

- [ ] **Step 1: Invoke `superpowers:systematic-debugging`.** This is partly investigative — confirm via Read that `SymForgeServer` does not currently retain its watcher JoinHandle, then design the minimal addition.
- [ ] **Step 2a: Define `WatcherHandle` newtype** in `src/watcher/mod.rs`. Public visibility. Two fields: `task: tokio::task::JoinHandle<()>` and `stop_token: Arc<AtomicBool>`. No Clone derive (JoinHandle is `!Clone`). Doc-comment names the invariant: "Owned together: signal stop_token, then bounded-await task. See H.1b's `abort_watcher_task` for the canonical shutdown sequence." This newtype is the ownership-pair type for any caller spawning a watcher.
- [ ] **Step 2b: Modify `SymForgeServer`** in `src/protocol/mod.rs` to add ONE new field: `watcher_handle: Arc<Mutex<Option<crate::watcher::WatcherHandle>>>`. `Arc<Mutex<...>>` is mandatory — `SymForgeServer` is `#[derive(Clone)]` and a bare `Mutex<Option<JoinHandle>>` is not Clone. The pattern matches the existing `watcher_info: Arc<Mutex<WatcherInfo>>` field at mod.rs:51. Initialize as `Arc::new(parking_lot::Mutex::new(None))` in both constructors (`new` at line 79 and `new_daemon_proxy` at line 102). **Add an inline doc comment at the field** stating "MUST NOT be held across `.await`. Use `.lock().take()` and `.lock().replace(...)` around any async work — parking_lot Mutex is non-async and held-across-await would deadlock the runtime." Also document Some-vs-None semantics: "Some only in local-stdio mode where SymForgeServer owns its own watcher. None in daemon-proxy mode and daemon-degraded mode."
- [ ] **Step 3: Modify `SymForgeServer::index_folder`** at `tools.rs:4394` to sequence: (1) `.lock().take()` on `watcher_handle` to extract `Option<WatcherHandle>`, drop the guard immediately; (2) if `Some`, signal the stop_token via `stop_token.store(true, Ordering::Release)`; (3) `tokio::time::timeout(Duration::from_secs(2), handle.task).await` for bounded-await join — log at warn-level on timeout but continue (don't block reload on a hung watcher); (4) call `self.index.reload(canonical_root)?` (which bumps project_generation per H.1a); (5) call new `restart_watcher(root, shared, watcher_info, prev=None)` (always None here — we already aborted the prior handle in step 1-3); (6) `.lock().replace(Some(new_handle))` to store. Verify with `grep -n` that no other path in tools.rs's index_folder mutates `watcher_handle` — the 2-second timeout and signal-await order are unguarded against re-entry. The 2-second timeout matches the H.1b convention for cooperative shutdown.
- [ ] **Step 4: Modify `restart_watcher`** signature in `src/watcher/mod.rs`. New shape: `pub fn restart_watcher(repo_root: PathBuf, shared: SharedIndex, watcher_info: Arc<Mutex<WatcherInfo>>, prev: Option<WatcherHandle>) -> WatcherHandle`. Internally: if `prev` is `Some`, signal `prev.stop_token` and bounded-await `prev.task` with `tokio::time::timeout(Duration::from_secs(2), ...)` before spawning the new task — log at warn on timeout, abort the task as backstop. Spawn the new task via `run_watcher_with_stop(repo_root, shared, watcher_info, new_token)` from H.1b. Return `WatcherHandle { task: new_handle, stop_token: new_token }`. The no-prev callers (server-side index_folder always passes None now that it pre-aborts in step 1-3 above; any other callers in daemon.rs are forbidden territory) get today's behavior unchanged. Verify backward-compat wrapper: H.1b kept a 0-arg `restart_watcher` wrapper for non-forbidden callers. Audit those callers; if any exist, either update the wrapper to pass `prev=None` to the new signature OR introduce `restart_watcher_with_prev` and keep the old wrapper unchanged. Prefer the wrapper-update path if no daemon.rs callers reference the old shape.
- [ ] **Step 5: Modify the three `freshen_exact_path_for_targeted_retrieval`-family callers** as follows. **Capture order matters** (round-2 feasibility review FEAS-R2-03): capture `expected_gen = server.index.current_project_generation()` FIRST, then call `server.capture_repo_root()` SECOND. The reverse order creates a race where a concurrent `index_folder` sets a new repo_root and bumps generation between the two reads, producing a `(stale_root, current_gen)` pair the fence cannot reject. Capturing gen first means any concurrent reload between the two reads produces `(current_root, stale_gen)`, which the fence correctly rejects. Thread `expected_gen` through `freshen_file_if_stale` (helper signature gains `expected_gen: u64`), and call `remove_file_at_generation` for confirmed-absent paths. **Do NOT silently skip remove on NotFound** — that alternative (originally proposed and rejected 2026-05-12 per round-1 adversarial review ADV-01 + feasibility review) would mask legitimate user-driven deletions until the next reconcile sweep (up to 30s later) and allow the same tool call that deleted a file to return ghost entries. Generation-fencing addresses both root-mismatch safety (per-request roots stale across `index_folder`) and legitimate-deletion correctness; the refuse-on-NotFound alternative addresses only the former.

  **Enriched `FreshenResult` shape** (round-2 adversarial review ADV-R2-04): the shared internal helper `freshen_file_if_stale` returns an enriched enum rather than a single boolean to prevent future refactors from accidentally flattening the three callsites' distinct success-path contracts:

  ```rust
  #[must_use]
  pub(crate) enum FreshenResult {
      Fresh,                          // indexed mtime matches disk
      StaleReindexed,                 // re-read + re-parsed; index updated
      StaleRemoved,                   // confirmed absent; remove_file_at_generation succeeded
      GenerationMismatch,             // fence rejected; caller's gen is stale
  }
  ```

  Variants renamed to canonical UpperCamelCase (PascalCase) per Rust style; the original spec language `Stale_Reindexed` etc. used snake_case which clippy would flag. `#[must_use]` is mandatory — round-3 adversarial review ADV-H1D-06 flagged that without it, a future caller forgetting an arm on the 4-variant enum compile-passes silently. Visibility is `pub(crate)` so test modules can match on variants; outside the crate the enum is invisible.

  **`EditError` enum** (defined in `tools.rs` near line 1659):

  ```rust
  #[derive(Debug, thiserror::Error)]
  pub(crate) enum EditError {
      #[error("Error: file not found at {path}")]
      PathNotFound { path: std::path::PathBuf },
      #[error("Error: session stale at {path} — call index_folder to refresh repo_root")]
      SessionStale { path: std::path::PathBuf },
  }
  ```

  `thiserror::Error` derive provides `Display` so `format!("{e}")` at the helper boundary yields the user-facing string. `thiserror` is already in the codebase (referenced in test fixtures at tools.rs:12091/12126). Helper-boundary contract: the three public helpers (`prepare_exact_path_for_edit`, `prepare_batch_paths_for_edit`, `freshen_exact_path_for_targeted_retrieval`) continue to return `Result<_, String>` — internal `EditError` is `format!("{e}")`-converted at the helper boundary. The 11 existing callsites at `tools.rs:6708/6895/7029/7160/7398/7523/3079/4764/4857` are untouched.

  **Per-callsite contracts** — each public helper exhaustively matches on `FreshenResult` (no `_ =>` wildcard arms; the `#[must_use]` + exhaustive-match discipline is an acceptance criterion below):

  - `prepare_exact_path_for_edit`: `Fresh`/`StaleReindexed` → continue with the edit; `StaleRemoved` → return `Err(format!("{}", EditError::PathNotFound { path }))` so the edit tool aborts; `GenerationMismatch` → return `Err(format!("{}", EditError::SessionStale { path }))` indicating the session needs to refresh its repo_root.
  - `freshen_exact_path_for_targeted_retrieval`: any of the four variants → continue (returns `()` or its existing return type); the side-effect of `StaleRemoved` is the desired index-consistency outcome. Exhaustive match still required (compile-time guardrail), even though every arm body is `()`.
  - `prepare_batch_paths_for_edit`: `Fresh`/`StaleReindexed` → include path in the batch; `StaleRemoved` → skip path with a soft warning (continue iterating); `GenerationMismatch` → abort the whole batch with `Err(format!("{}", EditError::SessionStale { path }))`.
- [ ] **Step 6: Modify `freshen_sidecar_path_if_stale`** at `src/sidecar/handlers.rs:180` analogously.
- [ ] **Step 7: Run new tests.**
- [ ] **Step 8: Run full test suite + clippy.**
- [ ] **Step 9: HALT for review.**

**Verification commands:**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo clippy -- -D warnings
```

---

---

## Task H.1e: Generation-fence git_temporal publication path

**Severity:** P0. Closes the matrix-row-5 obligation that round-1 LFG identified and round-2 review surfaced as unowned (4-persona convergence, conf 0.98). Lands in C-1 bucket after H.1a closes; parallel to H.1b/c/d (no dependency on them).

**Files (allowed):**

- Modify: `src/live_index/store.rs` — add `update_git_temporal_at_generation(index, expected_gen) -> bool` method on `SharedIndexHandle`.
- Modify: `src/live_index/git_temporal.rs` — `spawn_git_temporal_computation` accepts `expected_gen: u64` and consumes the fenced API.
- Modify: `src/daemon.rs` — call-site updates only at `activate` (line 1064) and `reload` (line 1106). Capture `expected_gen = index.current_project_generation()` immediately after the reload's project_generation bump and pass through. No other daemon.rs changes permitted.
- Modify: `src/main.rs` — call-site update only at `run_local_mcp_server_async` (line 287). Capture and pass `expected_gen`. No other main.rs changes permitted.
- Modify: `src/protocol/mod.rs` — call-site update only at `ensure_local_index` (line 362). Capture and pass `expected_gen`. No other protocol/mod.rs changes permitted (do NOT touch `SymForgeServer` struct, `new`, `new_daemon_proxy`, or any other symbol).
- Modify: `src/protocol/tools.rs` — call-site update only at `index_folder` (line 4524). Capture and pass `expected_gen`. No other tools.rs changes permitted (do NOT touch any other tool handler, freshen helper, or sibling code paths).
- Create: `tests/git_temporal_generation_fence.rs` — deterministic generation-fence tests for the git_temporal publication path.
- Forbidden: `src/watcher/mod.rs`, sidecar files, all other source files. Within the 4 caller-files-with-extended-access, the ONLY permitted change is the single-line `expected_gen` capture + pass-through at the named call site; treat the rest of each file as forbidden.

**Context:** `ProjectInstance::reload` (`src/daemon.rs:1101-1104`) calls `spawn_git_temporal_computation` after `index.reload` completes. The git_temporal computation is async + long-running (git log/diff walks) and publishes results via `SharedIndexHandle::update_git_temporal` at `src/live_index/store.rs:680-682`, which is a single atomic `ArcSwap::store`. A doomed computation that started under root A continues running after `ProjectInstance::reload` swaps in root B, then publishes A's temporal data into B's `SharedIndexHandle::git_temporal` field. Result: temporal data is stale-for-wrong-root for one update cycle until the new computation publishes. Affects: `analyze_file_impact` churn scores, `search_files(rank_by="path+cochange")` (Phase 3), `health` git-temporal output.

Severity rationale: this is degraded ranker output (annoying), not destroyed file index (catastrophic). But the matrix's "Either alone leaves a hole" premise depends on this fifth site being covered, so closing the gap is needed for the matrix's logical integrity. Also: Phase 3 ranker fusion (CoChange T3.3) reads from git_temporal — leaving this for C-4 would risk Phase 3.3 inheriting a known-stale annotation surface, compounding wrong answers.

**Spec (per CLAUDE.md §2.2):**

- **objective:** `SharedIndexHandle::update_git_temporal_at_generation(index, expected_gen) -> bool` is exposed (write-mutex-protected, re-reads `project_generation` under lock, no-ops on mismatch returning `false`). `spawn_git_temporal_computation` captures `expected_gen = shared.current_project_generation()` at task entry and consumes the fenced API for its single publication. A doomed temporal task that runs to completion after a reload publishes nothing (fence rejects).
- **non_goals:** No change to git_temporal computation logic itself (git log/diff walks unchanged). No change to `analyze_file_impact` or other temporal consumers — they continue reading from the same `ArcSwap`. No CancellationToken integration at git_temporal level (a long-running git walk is acceptable to let complete; the fence prevents its result from corrupting state).
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** `SharedIndexHandle` public surface gains 1 method. `spawn_git_temporal_computation` signature gains `expected_gen: u64` parameter.
- **invariants:** Existing `update_git_temporal` keeps current behavior (used by direct non-watcher-spawned consumers, if any). Fenced method is no-op on generation mismatch and returns `false`. After `reload(B)`, no A-era temporal data lands in B's `git_temporal` field.
- **acceptance_criteria:**
  - [ ] `cargo check` clean.
  - [ ] `cargo clippy -- -D warnings` clean.
  - [ ] Test `git_temporal_generation_fence::stale_temporal_publication_rejected` — capture `gen_a`, run a temporal computation against root A that delays publication; call `reload(root_b)`; assert the A-era publication is rejected by the fence (returns `false`); assert `git_temporal` field still holds B-era data (or `pending` if B's computation hasn't completed).
  - [ ] Test `git_temporal_generation_fence::current_temporal_publication_allowed` — capture `gen_b = current_project_generation()` after reload to B; call `update_git_temporal_at_generation(b_data, gen_b)`; assert returns `true` and `git_temporal` field is updated.
  - [ ] Existing temporal tests still pass.
- **evidence_required:** `cargo test -p symforge --test git_temporal_generation_fence -- --test-threads=1` green; full test suite green.
- **stop_conditions:** STOP if `update_git_temporal_at_generation` integration breaks any existing temporal test. STOP if `spawn_git_temporal_computation` lacks a clean place to capture `expected_gen` at task entry. STOP if the deterministic stale-publication test cannot be made reliable.

**Steps:**

- [ ] **Step 1: Invoke `superpowers:test-driven-development`.**
- [ ] **Step 2: Read `src/live_index/store.rs:670-690`** (the existing `update_git_temporal` body) and `src/live_index/git_temporal.rs:32-73` (the spawn function).
- [ ] **Step 3: Add `pub fn update_git_temporal_at_generation(&self, index: super::git_temporal::GitTemporalIndex, expected_gen: u64) -> bool`** to `SharedIndexHandle`. Body: take write_mutex, re-read project_generation under lock, short-circuit on mismatch returning `false`, else call existing `update_git_temporal` body, return `true`.
- [ ] **Step 4: Modify `spawn_git_temporal_computation`** to accept `expected_gen: u64` at the top-level signature. The caller (`daemon.rs::reload`) captures the generation immediately after the reload's project_generation bump.
- [ ] **Step 5: Replace** the internal `shared.update_git_temporal(...)` call in `git_temporal.rs` with `shared.update_git_temporal_at_generation(..., expected_gen)`. Log at trace-level (not warn) on `false` return — matches the H.1a pattern of avoiding warn-spam per doomed-task iteration.
- [ ] **Step 6: Write the two acceptance tests.**
- [ ] **Step 7: Run new test + full suite + clippy.**
- [ ] **Step 8: HALT for review.** Produce verification report; await authorization to commit.

**Verification commands:**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo clippy -- -D warnings
```

**Commit message:**

```
feat(live-index): generation-fence git_temporal publication path

Adds update_git_temporal_at_generation method to SharedIndexHandle,
mirroring the *_at_generation pattern from H.1a but extended to the
git_temporal ArcSwap publication.

spawn_git_temporal_computation captures expected_gen at task entry and
consumes the fenced API for its publication. After reload, a doomed
temporal task publishes nothing -- fence rejects.

Closes the matrix-row-5 obligation surfaced by round-2 review's
4-persona convergence (P0, conf 0.98).
```

---

## Task H.1f: Best-effort pre-flight generation check for coupling_refresh (REDESIGNED 2026-05-12 round-3 verification; round-3.5 patched 2026-05-13 after CODEX2 dispatch defect + adversarial review)

**Severity:** P2. Round-2 3-persona convergence on coupling-refresh row issues. Round-3 feasibility verification killed the original commit-boundary fence design (CouplingStore writes are streamed throughout the walk, no batch commit point). Round-3.5 (this version) corrects spec defects surfaced by a failed CODEX2 dispatch + two adversarial reviewers: (i) no exposed counter-increment accessor for `rejected_stale_mutations`; (ii) stale spawn-site line refs (569-575 is pre-H.1c; real site is 706-719); (iii) "mid-walk corruption" framing was imprecise — ground-truth: db is per-workspace path, doomed task writes to A's own db harmlessly while B's new tick writes to B's own db independently; the "residual" is wasted CPU completing a walk against a dormant workspace, not data corruption.

Severity remains P2: COUPLING flag (`SYMFORGE_COUPLING`) is opt-in default-off through Phase 3 deployment. If/when the flag flips default-on, re-evaluate severity at that gate.

Lands in C-1 bucket after H.1a closes; parallel to H.1b/c/d/e.

**Files (allowed):**

- Modify: `src/live_index/coupling/lifecycle.rs` — `refresh_on_reconcile_tick` accepts `expected_gen: u64` and `shared: &SharedIndex` parameters; performs a pre-flight gen-check at function entry (BEFORE the `flag_on()` and `is_git_repo()` short-circuits at lines 108-113, so counter telemetry fires on doomed tasks regardless of repo state); if mismatch, calls the new `shared.note_rejected_stale_mutation()` method, logs `tracing::trace!` only (extend the existing `use tracing::debug;` import to `use tracing::{debug, trace};`), returns early without touching disk. Update all 5 in-crate test call sites (lines 257, 400, 426, 459, 470 — note 470 is the SECOND call inside `guard_skips_tick_when_held`) to construct a minimal `SharedIndex` via `Arc::new(SharedIndexHandle::new_for_test(...))` and pass dummy steady-state gen.
- Modify: `src/live_index/coupling/mod.rs` — only if the new signature requires a re-export update (signature change alone usually does not break `pub use` at line 14; verify and adjust if needed).
- Modify: `src/watcher/mod.rs:706-719` spawn site ONLY — at the inner reconcile branch's coupling-refresh `spawn_blocking` (existing local bindings: `root_for_coupling` at line 710, `stop_for_coupling` at line 711). Add `let expected_gen_for_coupling = expected_gen;` (REUSE the outer-scope `expected_gen` already captured at watcher line ~614 / passed as `expected_gen_for_reconcile` at line 691 — do NOT re-sample `shared.current_project_generation()` inside or near the coupling closure; re-sampling defeats the fence) and `let shared_for_coupling = shared.clone();` BEFORE the spawn closure at line 712. Pass both into the `refresh_on_reconcile_tick(...)` invocation at line 716-718. NO other watcher/mod.rs changes — H.1b/c/d territory closed.
- Modify: `src/live_index/store.rs` — SURGICAL CARVE-OUT, ONLY for: (1) add new `pub(crate) fn note_rejected_stale_mutation(&self)` method on `SharedIndexHandle` that calls `self.rejected_stale_mutations.fetch_add(1, Ordering::Relaxed)` (mirror the internal increment at line 596/676/753); (2) bump `current_rejected_stale_mutations` from `pub(crate)` to `pub` (line 525-527) so the integration test in `tests/` can read it. NO other store.rs changes permitted; treat the rest of the file as forbidden.
- Create: `tests/coupling_refresh_generation_fence.rs` — deterministic generation-fence tests for the pre-flight check. Mirror the unit-test scaffold pattern from `src/live_index/store.rs:1976 rejected_stale_mutations_counter_increments_on_fence_rejection`: construct a real `SharedIndex` via `LiveIndex::empty()` or the test helper used by H.1a's counter test; bump `project_generation` directly via the existing test helper rather than spinning up a real watcher. Init `root_a` and `root_b` as real git repos with `init_repo_with_root_commit`-style helpers; set `SYMFORGE_COUPLING=1` for the test scope (note: tests/ is an external crate so it does NOT share the in-crate `COUPLING_ENV_LOCK`; document this and either run this test serially via project-wide `--test-threads=1` policy or use a unique env-var manipulation guard).
- Forbidden: `src/daemon.rs`, `src/protocol/tools.rs`, sidecar files, and deep CouplingStore-internal files (`run_init`, `cold_build`, `apply_head_delta` bodies — those would require rewriting streamed-write semantics out of scope for Phase H). Within `src/live_index/store.rs`, the rest of the file outside the two named surgical edits is forbidden.

**Context:** Coupling refresh is invoked from the periodic reconcile tick via fire-and-forget `spawn_blocking` at `src/watcher/mod.rs:706-719` (post-H.1c shape; the pre-H.1c line range 569-575 in earlier plan-doc drafts is stale). The closure body (`refresh_on_reconcile_tick` in `coupling/lifecycle.rs:107-126`) walks git delta data via `run_init` → `cold_build`/`apply_head_delta` (in `walker.rs:88, :129`) and writes results to a **per-workspace** SQLite store at `project_root.join(SYMFORGE_COUPLING_DB_PATH)` (lifecycle.rs:120). Per-workspace db means: a doomed task spawned with `root_for_coupling = A` opens A's db on disk, walks A's git data, and writes A-era data to A's db. B's new tick opens B's db (different path) and writes B-era data there. NO cross-workspace contamination occurs at the file-system level; the guard at lifecycle.rs:115-118 is per-`project_root` so inter-root tasks do not block each other. Round-3 verification confirmed writes are streamed throughout the walk — there is no batch commit point to gate.

**Failure-mode coverage achieved by the redesigned H.1f:**
- **Doomed-just-spawned**: a refresh task that hasn't yet started its walk when `reload(B)` fires — pre-flight check at task entry detects the gen mismatch and aborts immediately without touching disk. The new `note_rejected_stale_mutation()` increment makes the rejection observable in health output. ✓ Covered.
- **Mid-walk doomed completion**: a refresh task already partway through `run_init` when `reload(B)` fires — the task continues writing A-era data to **A's per-workspace db** until walk completes. No cross-contamination with B's db. The doomed completion is wasted CPU + dormant-result-on-disk under A's path, not data corruption. If the user later revisits root A, A's db reflects A's HEAD at the time of the doomed walk (correct A-era data — the walk was honest, just unnecessary). ✗ **Accepted residual**. Severity rationale: wasted CPU on a workspace the user moved away from is annoying, not unsafe. Lesser severity than B-P0-1's destruction.

Severity rationale (revised round-3.5): the original "fence at commit boundary" design would have ALSO covered the doomed-just-spawned case but is infeasible without API rewrite. The redesigned pre-flight covers doomed-just-spawned at low complexity. Mid-walk doomed-completion is documented as wasted-CPU residual.

**Telemetry attribution non-goal:** `SharedIndexHandle::rejected_stale_mutations` is a single shared counter incremented by all fence surfaces (H.1a indexed-file writes, H.1a mtime touches, H.1e git_temporal, this H.1f coupling). Per-surface attribution is OUT OF SCOPE for H.1f. The per-surface signal is the trace-only log message (`coupling: pre-flight gen-check rejected; ...`). If per-surface counters are needed later, that's a separate task; do not bloat H.1f scope.

**Spec:**

- **objective:** `refresh_on_reconcile_tick(project_root, expected_gen, shared)` performs a pre-flight `shared.current_project_generation() == expected_gen` check at function entry, BEFORE the `flag_on()` and `is_git_repo()` short-circuits. On mismatch: call `shared.note_rejected_stale_mutation()`, log `trace!` only, return without touching disk. On match: proceed with existing flag/git/guard short-circuits then `run_init` body unchanged. A doomed coupling-refresh task that hasn't started its walk yet aborts before any disk work; a doomed task already in `run_init` continues to completion on its own per-workspace db (accepted wasted-CPU residual).
- **non_goals:** No change to `run_init`, `cold_build`, `apply_head_delta`, or `CouplingStore` internals. No per-write fence inside the streamed walk. No CouplingStore API rewrite. No CancellationToken integration at the coupling layer (Layer 1 = pre-flight check, Layer 2 = pre-flight check, Layer 3 = not applicable). No per-surface telemetry attribution.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** `refresh_on_reconcile_tick` signature gains `expected_gen: u64` and `shared: &SharedIndex` parameters. `SharedIndexHandle` gains 1 new `pub(crate) fn note_rejected_stale_mutation(&self)` method. `current_rejected_stale_mutations` getter visibility goes `pub(crate)` → `pub`. Spawn site at `src/watcher/mod.rs:706-719` captures and passes both new arguments using H.1c's already-captured `expected_gen`.
- **invariants:** Pre-flight check calls `shared.note_rejected_stale_mutation()` on rejection so the doomed-task rejection increments the shared `rejected_stale_mutations` counter (observable in health output, attribution via trace log only). Existing coupling-store consumers see no change in API. Per-workspace db isolation is preserved (no cross-contamination between A's and B's coupling dbs). The wasted-CPU residual is documented in matrix row 2.
- **acceptance_criteria:**
  - [ ] `cargo check` clean.
  - [ ] `cargo clippy -- -D warnings` clean (note: `tracing::trace!` must be in scope — extend import).
  - [ ] Test `coupling_refresh_generation_fence::stale_refresh_aborts_pre_flight` — bump `shared.project_generation` to simulate a reload; call `refresh_on_reconcile_tick(root_a, stale_gen, &shared)`; assert the function returns early (no disk write — coupling DB file at `root_a.join(SYMFORGE_COUPLING_DB_PATH)` does NOT exist post-call); assert `current_rejected_stale_mutations()` incremented by exactly 1.
  - [ ] Test `coupling_refresh_generation_fence::current_refresh_proceeds_normally` — steady-state (no reload, `expected_gen == shared.current_project_generation()`); assert refresh proceeds and writes the coupling DB to disk (file exists at expected path). Requires `SYMFORGE_COUPLING=1` and a real git repo via `init_repo_with_root_commit`-style helper.
  - [ ] Existing coupling-store tests still pass (all 5 in-crate `refresh_on_reconcile_tick` test call sites updated for new signature, semantics preserved).
  - [ ] Existing H.1a counter test at `store.rs:1976` still passes (no regression on the `note_rejected_stale_mutation` increment path).
- **evidence_required:** `cargo test -p symforge --test coupling_refresh_generation_fence -- --test-threads=1` green; full test suite green; clippy strict-warn green.
- **stop_conditions:** STOP if `refresh_on_reconcile_tick` cannot accept `&SharedIndex` due to circular imports (unlikely: `coupling/` is a child module of `live_index/`; the import `use crate::live_index::store::SharedIndex` is safe). STOP if the deterministic stale-refresh test cannot be made reliable. STOP if any required edit falls outside the named allowed-files or outside the surgical scope within `src/live_index/store.rs`. STOP if `SharedIndexHandle::new_for_test` or equivalent test helper does not exist — surface for guidance rather than constructing a complex test scaffold ad-hoc.

**Steps:**

- [ ] **Step 1: Invoke `superpowers:test-driven-development`.**
- [ ] **Step 2: Read** `src/live_index/coupling/lifecycle.rs:107-126` (target function), `src/live_index/store.rs:444, :525-527, :596, :676, :753, :1976-1998` (counter field + getter + existing internal increment sites + H.1a counter test for scaffold), `src/watcher/mod.rs:614-720` (the post-H.1c run_watcher loop, including the `expected_gen` capture site and the coupling spawn at 706-719).
- [ ] **Step 3a: In `src/live_index/store.rs`** — add `pub(crate) fn note_rejected_stale_mutation(&self)` method on `SharedIndexHandle` near the existing `current_rejected_stale_mutations` getter (~line 525-527). Body: `self.rejected_stale_mutations.fetch_add(1, Ordering::Relaxed);`. Bump the getter visibility from `pub(crate) fn current_rejected_stale_mutations` to `pub fn current_rejected_stale_mutations`. No other store.rs changes.
- [ ] **Step 3b: In `src/live_index/coupling/lifecycle.rs`** — modify `refresh_on_reconcile_tick` signature to `pub fn refresh_on_reconcile_tick(project_root: &Path, expected_gen: u64, shared: &SharedIndex)`. Add `use crate::live_index::store::SharedIndex;` to the imports (and bump `use tracing::debug;` to `use tracing::{debug, trace};`). At the VERY TOP of the function body (BEFORE the `if !flag_on()` and `if !is_git_repo()` short-circuits), insert:
    ```rust
    let current_gen = shared.current_project_generation();
    if current_gen != expected_gen {
        shared.note_rejected_stale_mutation();
        trace!(
            "coupling: pre-flight gen-check rejected; expected={expected_gen} current={current_gen}; not refreshing"
        );
        return;
    }
    ```
  Then leave the existing `flag_on`/`is_git_repo`/`guard_for`/`try_acquire`/`run_init` chain unchanged.
- [ ] **Step 4: Modify spawn site at `src/watcher/mod.rs:706-719`** — at lines 710-711 (existing `root_for_coupling`/`stop_for_coupling` captures), add `let expected_gen_for_coupling = expected_gen;` (REUSE the outer-scope `expected_gen` already captured at watcher line ~614 — do NOT re-sample) and `let shared_for_coupling = shared.clone();`. Update the `refresh_on_reconcile_tick(&root_for_coupling)` call at line 716-718 to `refresh_on_reconcile_tick(&root_for_coupling, expected_gen_for_coupling, &shared_for_coupling)`. NO other watcher/mod.rs changes.
- [ ] **Step 4.5: Update 5 in-crate test call sites in lifecycle.rs** (lines ~257, 400, 426, 459, 470 — note 470 is the SECOND call inside `guard_skips_tick_when_held`). For each, construct a minimal `SharedIndex` via `Arc::new(SharedIndexHandle::new_for_test(...))` (or whichever test-helper exists; if none, mirror the construction at `store.rs:1976` setup), capture `let gen = shared.current_project_generation()`, and pass `(tmp.path(), gen, &shared)`. Semantics-preserving — existing tests assert pre-existing flag/guard/HEAD behavior; the steady-state gen-match means the new pre-flight check passes and execution proceeds to the existing code path.
- [ ] **Step 5: Write the two acceptance tests** in `tests/coupling_refresh_generation_fence.rs`. Mirror the scaffold from `src/live_index/store.rs:1976`. For the stale test, bump `shared.project_generation` synchronously via the existing test helper (NO real watcher spawn) so the function-level test is deterministic.
- [ ] **Step 6: Run full verification gates** — `cargo check`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, `cargo clippy -- -D warnings`, then `cargo clean` at task boundary.
- [ ] **Step 7: HALT for review.** Produce verification report; await authorization to commit. Note the accepted wasted-CPU residual (mid-walk doomed-completion to A's per-workspace db) explicitly in the commit message so the limitation is captured in agentmemory for future contributors.

**Verification commands:**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo clippy -- -D warnings
```

**Commit message:**

```
feat(live-index): pre-flight generation fence for coupling_refresh

Adds expected_gen + shared parameters to refresh_on_reconcile_tick;
pre-flight gen-check at function entry (before flag/git/guard
short-circuits) aborts the doomed-just-spawned case without disk
writes, incrementing the shared rejected_stale_mutations counter
via the new pub(crate) SharedIndexHandle::note_rejected_stale_mutation
accessor (added in this commit). Getter visibility bumped pub(crate)
-> pub to allow integration tests in tests/ crate.

Closes matrix-row-2 doomed-just-spawned exposure flagged by round-2
3-persona convergence (coherence + scope + adversarial). Round-3.5
patch consumes feasibility + adversarial reviewer findings:
- spawn-site line refs updated 569-575 -> 706-719 (post-H.1c)
- expected_gen REUSED from H.1c's outer-scope capture (not re-sampled)
- store.rs surgical carve-out (1 new pub(crate) method + 1 visibility
  bump) replaces the original blanket forbid that contradicted the
  counter-increment requirement
- gen-check placed BEFORE flag/is_git_repo short-circuits so telemetry
  fires regardless of repo state

Accepted residual (wasted-CPU, not data corruption): a refresh task
already inside run_init when reload(B) fires continues writing
A-era data to A's per-workspace coupling db until the walk completes.
Ground-truth verified 2026-05-13: coupling db is per-workspace path
(project_root.join(SYMFORGE_COUPLING_DB_PATH)), so doomed completion
writes to A's own db with no cross-contamination to B's db. The
result sits dormant on disk under A's path; on future revisit to A,
run_init reads correct A-era data. P2 severity contingent on
SYMFORGE_COUPLING=1 remaining opt-in default-off through Phase 3.
```

---

# Tier 2 — Correctness

## Task H.2: Health source-of-truth unification

**Severity:** P1. The two health surfaces disagree on watcher state and load time. Root cause: daemon-degraded sticky flag + local fallback rendering a process that has no watcher.

**Files (allowed):**

- Modify: `src/protocol/mod.rs` — `daemon_degraded` opportunistic-clear logic; `ensure_local_index` adds a marker on `WatcherInfo` indicating "no watcher attached".
- Modify: `src/protocol/format.rs` — `health_report_compact_from_published_state` to surface the "local fallback" sentinel distinctly from `Off`; align idle-arm guard with `health_report_from_stats`.
- Modify: `src/protocol/format/tests.rs` — add cross-handler conformance test.
- Modify: `src/watcher/mod.rs` (minor) — add a `WatcherInfo::detached_local_fallback() -> Self` constructor or equivalent, OR add a field `is_local_fallback: bool`.
- Forbidden: Daemon-internal code.

**Spec:**

- **objective:** Both `health` and `health_compact` render the same `Watcher:` state AND the same `load_duration_ms` value (after rendering normalization) when called against the same `(PublishedIndexState, WatcherInfo)` pair. Local fallback is labeled distinctly from a dead daemon-side watcher. (Load-time scope added 2026-05-12 round-2 walk item 3 per B-P1-6 catalog promise.)
- **non_goals:** Do NOT change daemon-side watcher behavior. Do NOT remove `daemon_degraded`.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** Internal: a new boolean (or marker) on `WatcherInfo`. Optional API: `is_local_fallback()` query on `WatcherInfo`.
- **invariants:** Local fallback state never claims `Active`. Sticky `daemon_degraded` is cleared on next successful daemon call.
- **acceptance_criteria:**
  - [ ] New test `health_report_consistency::both_paths_agree_on_watcher_state_and_load_time` — render both `health_report_from_stats` and `health_report_compact_from_published_state` on 6 distinct `(WatcherInfo, PublishedIndexState)` combos; assert parsed watcher-state classification agrees AND parsed `load_duration_ms` agrees byte-for-byte. (Load-time clause added 2026-05-12 round-2 walk item 3 — closes B-P1-6's load-time half. The fix is already implicit in H.2's `daemon_degraded` clearing because both surfaces then read from daemon's `published_state`; this test makes that explicit so Gate 2 can verify it.)
  - [ ] **Render `rejected_stale_mutations` counter in health output** (added 2026-05-12 round-2 walk item 12): both `health` and `health_compact` include a line `Stale-mutation rejections: N` reading from `SharedIndexHandle::current_rejected_stale_mutations()` (counter added by H.1a per round-2 walk item 12). New test `health_renders_rejected_stale_mutations_counter` asserts the line appears in both surfaces with the correct value. This counter is the user-visible telemetry surface for detecting silent-skip races introduced by future Phase 2.3/3/4 work — closes the "cancellation token leaves silent skip risk" round-1 finding.
  - [ ] New test `daemon_degraded_clears_on_next_success` — set degraded, run a proxied call that succeeds, assert flag is cleared.
  - [ ] Manual repro from evaluator: in-process health + health_compact return matching watcher state.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`, plus an in-process repro showing `health` and `health_compact` agree on watcher state.
- **stop_conditions:** STOP if the local-fallback marker addition cascades into > 5 file changes.

**Steps:**

- [ ] **Step 1: Read the investigator's Part A** at `docs/notes/external-evaluations/2026-05-11/INVESTIGATION_HEALTH_REFS.md` — confirms the exact divergence sites.
- [ ] **Step 2: Decide marker shape.** Option A: add `is_local_fallback: bool` to `WatcherInfo`. Option B: introduce a sentinel `WatcherState::LocalFallback` enum variant. Option B is cleaner type-wise but requires match-arm coverage everywhere. Default to A unless a strong reason emerges.
- [ ] **Step 3: Set the marker in `ensure_local_index`** at `src/protocol/mod.rs`.
- [ ] **Step 4: Render the marker in both format paths.** Compact: `Watcher: local-fallback (no watcher attached)`. Full: `Watcher: local-fallback (no watcher attached; daemon proxy unavailable)`.
- [ ] **Step 5: Clear `daemon_degraded` on next successful proxy call** — in `proxy_tool_call`, on the success branch after a retry, call `self.daemon_degraded.store(false, Ordering::Relaxed)`.
- [ ] **Step 6: Write conformance test.** Spawn the same render on a matrix of inputs; assert match.
- [ ] **Step 7: Run tests + clippy.**
- [ ] **Step 8: HALT for review.**

**Verification commands:**

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.3: search_text structural label fix

**Severity:** P1 (trivial). `search_text(structural=true)` envelope says `Match type: constrained (literal)`. Fix is ~10 lines.

**Files (allowed):**

- Modify: `src/protocol/tools.rs` — `search_text_match_type_label` signature + branch + 1 caller.
- Modify: existing structural-search tests (if any) or add `tests/search_text_structural_label.rs`.
- Forbidden: Everything else.

**Spec:**

- **objective:** `search_text(structural=true, ...)` returns envelope `Match type: structural (ast-grep)`.
- **non_goals:** No change to actual structural-search matching logic.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** `search_text_match_type_label` gains `structural: bool` parameter.
- **invariants:** Non-structural callers see identical output to today.
- **acceptance_criteria:**
  - [ ] New test asserting structural label on a representative input.
  - [ ] No regression on existing search_text tests.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`.
- **stop_conditions:** None expected.

**Steps:**

- [ ] **Step 1: Read `src/protocol/tools.rs:1852-1875`** to confirm the function shape.
- [ ] **Step 2: Add a `structural: bool` parameter** as the first arg; on `structural`, return `"structural (ast-grep)"`. Otherwise existing behavior.
- [ ] **Step 3: Update the single caller** in the `search_text` handler.
- [ ] **Step 4: Add test + run.**
- [ ] **Step 5: Commit.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.4: find_dependents Pass 2 constraint

**Severity:** P1. `find_dependents` attributes 1612 false-positive refs because Pass 2 promotes every reference whose simple name matches any target symbol name, once a single import stem matches.

**Files (allowed):**

- Modify: `src/live_index/query.rs` — `find_dependents_for_file` Pass 2 filter at `src/live_index/query.rs:2741-2778`.
- Modify: tests — `tests/find_dependents_*.rs` or equivalent. Add fixture exhibiting common method-name collision.
- Forbidden: tool-handler files; xref query files.

**Spec:**

- **objective:** A file `A` is reported as depending on file `B` only when `A` references a symbol in `B` via either (a) a `ReferenceKind::TypeUsage` whose name is in `B`'s symbol set, or (b) a `ReferenceKind::Call` whose `qualified_name` suffix-matches `B`'s module path (the existing Pass 3 check), or (c) an explicit import edge.
- **non_goals:** No change to import detection. No change to re-export chain BFS (Pass 4).
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** None public. Internal: Pass 2 filter logic.
- **invariants:** Pre-existing find_dependents true-positives (cases where `A` actually does depend on `B`) still appear in results.
- **acceptance_criteria:**
  - [ ] New test `find_dependents::constructor_name_collision_no_false_positive` — fixture: file A defines `TypeA::new`, file B calls `Vec::new`, `String::new`, `unrelated::Other::new`, with one matching import for an unrelated module from A. Assert find_dependents(A) does not report B.
  - [ ] New test `find_dependents::real_qualified_call_dependent_still_reported` — file C calls `A::TypeA::new()`, no import. Assert find_dependents(A) reports C (via Pass 3).
  - [ ] New test `find_dependents::cross_language_method_name_collision` — added 2026-05-12 per adversarial review (ADV-04): fixtures for C# (`obj.Equals(other)` and `string.Equals(...)` across types that override `Equals`) and Python (`from module import foo; foo()` alongside unrelated `from baz import bar2; bar2()`) demonstrating Pass 2 narrowing does not regress non-Rust semantics. Overloaded methods sharing names across types must not promote to dependent edges absent qualified-name suffix match against the target's module path. If the Pass 2 rewrite gates on `target_language` for cross-language correctness, this test is the gate. Phase 4 RTK Tier 1 inherits Pass 2; this fixture protects the cross-language path before Phase 4 ships.
  - [ ] Cross-check on a synthetic large fixture: the orchestrator-scale false-positive count drops to near-zero (target: <5 false positives across a 1000-file Rust repo with method-name-collision patterns).
  - [ ] No regression on existing dependent tests.
- **evidence_required:** `cargo test -p symforge -- --test-threads=1`.
- **stop_conditions:** STOP if Pass 2 narrowing causes a documented true-positive scenario to be missed. STOP if `ReferenceRecord` lacks the `kind` discrimination needed (it does — `ReferenceKind::TypeUsage` exists per the query file inspection).

**Steps:**

- [ ] **Step 1: Read** `src/live_index/query.rs:2700-2825` to confirm the current Pass 2/3 split.
- [ ] **Step 2: Read** `src/domain/` (or wherever `ReferenceKind` is defined) to confirm available variants.
- [ ] **Step 3: Tighten Pass 2 filter:**
  ```rust
  reference.kind != ReferenceKind::Import
      && target_symbol_names.contains(reference.name.as_str())
      && Self::has_pub_symbol(target_file, &reference.name)
      && (
          reference.kind == ReferenceKind::TypeUsage
          || reference.qualified_name.as_deref().is_some_and(|qn|
              matches_exact_symbol_qualified_name(
                  &target_language, qn, &reference.name, module_path.as_deref()
              )
          )
      )
  ```
- [ ] **Step 4: Add the three new tests** (round-2 coherence review caught this — Step 4 originally said "two new tests" but H.4 acceptance_criteria was expanded to three tests including the cross-language fixture).
- [ ] **Step 5: Run tests + clippy.**
- [ ] **Step 6: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.5: find_references qualified-path coverage via shared collector

**Severity:** P1. `find_references` undercounts fully-qualified Rust uses; `batch_rename`'s `find_qualified_usages` byte-scan catches them. Promote the better collector.

**Files (allowed):**

- Modify: `src/protocol/edit.rs` — extract `find_qualified_usages` and surrounding logic into a shared submodule (or `src/live_index/qualified_usages.rs`).
- Modify: `src/protocol/tools.rs` — `find_references` calls the shared collector and merges with reverse-index results, deduplicating on `(file_path, byte_range)`.
- Create or modify: tests — at least one test demonstrating `find_references("MemoryStoreKnowledgeUpsertAdapter")` returns fully-qualified calls.
- Forbidden: `src/parsing/xref.rs` (grammar query change is a separate, higher-risk task; not in this hotfix).

**Spec:**

- **objective:** `find_references(name)` returns the union of reverse-index hits and byte-scanned qualified-path hits, deduplicated by `(file, byte_range)`, with a clear confidence label per hit.
- **non_goals:** Do NOT change the tree-sitter Rust xref query. Do NOT change `batch_rename`'s behavior.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** New `pub(crate) fn collect_qualified_usages(name, file_contents) -> Vec<QualifiedUsage>` in shared module. `find_references` handler invokes it.
- **invariants:** Pre-existing find_references hits still appear. Performance: the byte-scan adds O(files × content_bytes) per query, comparable to `grep` over the workspace; document this in the function doc-comment and gate behind a sane default.
- **acceptance_criteria:**
  - [ ] New test `find_references::qualified_call_via_full_path_returned` — fixture file `A` defines `struct TypeA`, file `B` calls `crate::module::TypeA::new()`. Assert find_references("TypeA") returns the call site in B.
  - [ ] Existing `batch_rename` tests pass unchanged.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`.
- **stop_conditions:** STOP if extracting `find_qualified_usages` from `src/protocol/edit.rs` breaks `batch_rename`. STOP if `find_references` performance regresses > 30% on a large fixture.

**Steps:**

- [ ] **Step 1: Read** `src/protocol/edit.rs:1585-2400` to map the `find_qualified_usages` API surface.
- [ ] **Step 2: Extract** to `src/live_index/qualified_usages.rs` or `src/protocol/qualified_usages.rs`. Preserve the existing API for `batch_rename`. Re-export from the original module if needed for backwards-compat.
- [ ] **Step 3: Modify `find_references` handler** at `src/protocol/tools.rs:4926+` to: (a) call reverse-index, (b) call the shared collector across all files, (c) merge with dedup on `(file_path, byte_range)`, (d) label combined hits.
- [ ] **Step 4: Add the new test.**
- [ ] **Step 5: Run tests + clippy.**
- [ ] **Step 6: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.6: get_symbol_context / get_file_context budget enforcement

**Severity:** P1. Both tools time out (97-120s) on large files; `max_tokens` is applied after gathering, not during rendering.

**Files (allowed):**

- Modify: `src/live_index/query.rs` — `capture_symbol_context_view` and `capture_file_context_view` (or wherever the gather-then-render split lives). Enforce token budget during outline expansion.
- Modify: `src/protocol/format.rs` — outline renderers; collapse test modules unless explicitly requested.
- Modify: `src/protocol/tools.rs` — handler-side timeout wrap (already at MCP layer; verify but do not weaken).
- Create or modify: tests — add a large-file fixture and an explicit budget-respect test.
- Forbidden: parsing.

**Spec:**

- **objective:** `get_symbol_context` and `get_file_context` complete in < 5s wall on a 16k-line file with > 250 symbols. The response respects `max_tokens` to within ±20%. Test modules with > 100 functions collapse by default to a count summary unless `include_tests=true` or the section is explicitly named.
- **non_goals:** Do NOT remove dependents/siblings expansion entirely — make it budget-aware.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** New input field on both tools: `include_tests: bool` (default `false`). Internal: token-counter threaded through outline expansion.
- **invariants:** Smaller files render identically to today.
- **acceptance_criteria:**
  - [ ] New test `get_symbol_context::large_file_respects_budget` — synthetic 16k-line file fixture with > 250 symbols; assert response under 5s wall and within budget.
  - [ ] New test `get_file_context::nested_test_module_collapsed_by_default` — file with 100+ test functions; assert outline does not enumerate every test.
  - [ ] Augmented existing test asserting `include_tests=true` restores full outline.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`.
- **stop_conditions:** STOP if the budget threading requires invasive refactors of > 3 modules. Carve a smaller subtask if so.

**Steps:**

- [ ] **Step 1: Read** the current implementation at `src/live_index/query.rs` (capture_*_view functions) and `src/protocol/format.rs` (outline rendering).
- [ ] **Step 2: Identify the gather-then-render boundary.** Where do dependents/siblings get loaded? That is the budget-enforcement point.
- [ ] **Step 3: Implement** a `RenderBudget` struct (or extend an existing one) that the renderer increments as it appends; bail out with "section-truncated (N more not shown)" when exceeded.
- [ ] **Step 4: Add** `include_tests: bool` input. Test-module collapse logic in the renderer.
- [ ] **Step 5: Add tests.**
- [ ] **Step 6: Run tests + clippy.**
- [ ] **Step 7: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.7: batch_rename timeout fix (profile-first)

**Severity:** P1. Even `dry_run=true` times out at 120s. Cause unconfirmed — could be lock-order inversion, unbounded reference traversal, or git-temporal blocking. Investigation must precede patching.

**Files (probable):**

- Modify: `src/protocol/edit.rs` — `execute_batch_rename` and surrounding helpers.
- Possibly modify: `src/live_index/query.rs` — if reference traversal lacks cycle detection.
- Forbidden: Until profile completes, no edits.

**Spec:**

- **objective:** `batch_rename(dry_run=true)` for a struct with 13 sites across 6 files returns in < 5s wall. Removes any lock-order inversion or unbounded traversal.
- **non_goals:** Do NOT change rename semantics. No `find_qualified_usages` rewrite.
- **allowed_files:** TBD post-profile.
- **forbidden_files:** TBD post-profile.
- **interfaces touched:** TBD.
- **invariants:** Pre-existing batch_rename behavior preserved except for completion time.
- **acceptance_criteria:**
  - [ ] Profile report committed to `docs/notes/external-evaluations/2026-05-11/PROFILE_BATCH_RENAME.md` identifying the root cause.
  - [ ] Targeted patch landed with regression test asserting wall < 5s for the evaluator's repro.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:** Flame graph or `RUST_LOG=trace` capture confirming the bottleneck; test wall-time assertion.
- **stop_conditions:** STOP if profile reveals the bottleneck is in a third-party crate (e.g. git-temporal blocking on `libgit2`); pivot to a smaller scope or carve out as separate task.

**Steps:**

- [ ] **Step 1: Invoke `superpowers:systematic-debugging`.**
- [ ] **Step 2: Reproduce the timeout** on a known fixture. Confirm wall > 60s before profiling.
- [ ] **Step 3: Capture** `RUST_LOG=symforge=trace cargo test --test batch_rename_perf -- --test-threads=1 --nocapture` output. Save the captured log to disk; summarize only.
- [ ] **Step 4: Identify** the slowest segment. Lock-order vs traversal vs I/O.
- [ ] **Step 5: Write profile report** at the path above.
- [ ] **Step 6: HALT for review of profile findings** before any code change. Authorize scope based on profile.
- [ ] **Step 7: Patch the identified bottleneck.**
- [ ] **Step 8: Add regression test.**
- [ ] **Step 9: Run tests + clippy.**
- [ ] **Step 10: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

# Tier 3 — Diagnostic + minor

## Task H.8: tree-sitter-rust grammar bump for &raw

**Severity:** P2. SymForge's own `src/live_index/persist.rs` and `src/worktree.rs` parse as partial because vendored tree-sitter-rust predates Rust 2024 raw references. Symbol counts under-report; downstream tools see incomplete outlines.

**Files (allowed):**

- Modify: `vendor/tree-sitter-rust/` — bump to a release with `raw_reference_expression` support. May require regenerating parser files.
- Modify: `Cargo.toml` — if the dependency is declared in Cargo rather than vendored, bump the version.
- Modify: tests — `validate_file_syntax` should report `Status: ok` on `persist.rs` and `worktree.rs`.

**Spec:**

- **objective:** `cargo test --all-targets` passes against the bumped grammar. `validate_file_syntax(src/live_index/persist.rs)` reports `ok`.
- **non_goals:** Do NOT switch to a different grammar source. Do NOT vendor other languages in this task.
- **allowed_files:** as listed.
- **forbidden_files:** other languages' vendor dirs.
- **interfaces touched:** None.
- **invariants:** Pre-existing successfully-parsed files still parse.
- **acceptance_criteria:**
  - [ ] `validate_file_syntax` reports `ok` on the two known partials.
  - [ ] Partial-parse count in `health` drops by at least 2.
  - [ ] No regression on the conformance suite for other languages.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:** Conformance suite output; `health` partial count delta.
- **stop_conditions:** STOP if a grammar bump breaks an unrelated parse case. STOP if regenerating the parser requires installing `tree-sitter-cli` not currently in the toolchain.

**Steps:**

- [ ] **Step 1: Investigate** how `tree-sitter-rust` is integrated — vendored vs Cargo dependency. Read `Cargo.toml` and `vendor/tree-sitter-rust/`.
- [ ] **Step 2: Identify the smallest version** that adds `raw_reference_expression`.
- [ ] **Step 3: Apply the bump.**
- [ ] **Step 4: Verify** `validate_file_syntax` on the two known partial files.
- [ ] **Step 5: Run full conformance suite.**
- [ ] **Step 6: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.9: validate_file_syntax diagnostic localization

**Severity:** P2. Reports "syntax error near line 1 col 1" with byte_span 0..101894. Outermost ERROR-node, not deepest.

**Files (allowed):**

- Modify: `src/parsing/` — `validate_file_syntax` (or equivalent) walks the tree-sitter parse tree, collects ERROR nodes, reports the deepest one with line:col + symbol-extracted-count.
- Modify: tests — add a fixture with a known mid-file syntax error, assert diagnostic localizes correctly.
- Forbidden: format/render files.

**Spec:**

- **objective:** `validate_file_syntax` reports the deepest ERROR-node position. If the file has multiple errors, report the first (in source order) and a count of others.
- **non_goals:** No change to parsing strictness. No new lint rules.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** Diagnostic message shape.
- **invariants:** Files that parse cleanly still report `Status: ok`.
- **acceptance_criteria:**
  - [ ] New test `validate_file_syntax::valid_inner_doc_comment_not_error_at_line_1` — file starting with `//!`, otherwise valid. Assert no diagnostic at line 1.
  - [ ] New test `validate_file_syntax::deepest_error_reported_on_partial` — fixture with one syntax error at line 50 of 200. Assert diagnostic reports line 50.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`.
- **stop_conditions:** STOP if tree-sitter ERROR-walk requires API not currently used.

**Steps:**

- [ ] **Step 1: Read** the current `validate_file_syntax` implementation.
- [ ] **Step 2: Walk ERROR nodes.** Tree-sitter's `Cursor::is_error()` or `Node::has_error()` may be used; verify exact API in current binding.
- [ ] **Step 3: Update diagnostic format** to `near <token> (line L, col C); N more errors below`.
- [ ] **Step 4: Add tests.**
- [ ] **Step 5: Run tests + clippy.**
- [ ] **Step 6: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.10: Untracked file search diagnostic

**Severity:** P2. `what_changed` sees untracked files; `search_files`/`search_text` don't. No diagnostic guides the user to `analyze_file_impact(new_file=true)`.

**Files (allowed):**

- Modify: `src/protocol/tools.rs` — `search_files`, `search_text` handlers. When the query returns 0 hits AND `what_changed(uncommitted=true)` has untracked matches matching the query, surface a diagnostic line.
- Modify: tests.

**Spec:**

- **objective:** Empty-result responses for search tools include an actionable diagnostic when an untracked file matches the query. Format: `Note: 1 untracked file may match this query. Run analyze_file_impact("<path>", new_file=true) to index it.`
- **non_goals:** Do NOT auto-index untracked files (that violates idempotency). Do NOT broaden search to untracked files by default.
- **allowed_files:** as listed.
- **forbidden_files:** Other tool handlers.
- **interfaces touched:** Search-tool envelope strings.
- **invariants:** Tracked-file search behavior unchanged.
- **acceptance_criteria:**
  - [ ] New test asserting the diagnostic appears for an untracked-file query.
  - [ ] No regression on existing search tests.
  - [ ] `cargo clippy -- -D warnings` clean.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`.
- **stop_conditions:** STOP if `what_changed` is not cheaply callable from the search-tool path.

**Steps:**

- [ ] **Step 1: Identify** the cheapest way to get an untracked-file list from inside `search_files`/`search_text`. Likely a cached snapshot from the last `what_changed` call or a single-shot git invocation.
- [ ] **Step 2: Add the diagnostic.**
- [ ] **Step 3: Add tests.**
- [ ] **Step 4: Run tests + clippy.**
- [ ] **Step 5: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.11: Sidecar PID/alive surfacing in health

**Severity:** P2. `health` says "Fail-open here is mostly benign" while 102/322 events fail-open. Surface sidecar PID + alive/dead.

**Files (allowed):**

- Modify: `src/protocol/format.rs` — `health_report_*` add a sidecar line.
- Modify: `src/sidecar/` — expose sidecar PID + alive state to the daemon.
- Modify: tests.

**Spec:**

- **objective:** `health` includes a `Sidecar:` line with PID and alive/dead state. `health_compact` includes `sidecar: up` or `sidecar: down`.
- **non_goals:** No change to sidecar restart logic.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** Health output strings.
- **invariants:** Pre-existing health fields unchanged.
- **acceptance_criteria:**
  - [ ] Test asserting sidecar line presence in both health outputs.
  - [ ] Test asserting `down` is rendered when sidecar is killed.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`.
- **stop_conditions:** STOP if exposing sidecar PID requires invasive sidecar-side refactor.

**Steps:**

- [ ] **Step 1: Identify** sidecar state already exposed to the daemon.
- [ ] **Step 2: Render.**
- [ ] **Step 3: Add tests.**
- [ ] **Step 4: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

## Task H.12: NoisePolicy classify_path covers .obsidian + wiki/.obsidian

**Severity:** P2. Phase 2 work covered `vendor`, `third_party`, `node_modules`, `.venv`, `venv`, `site-packages`, `pods`, `bower_components`, `.claude/gsd-*`, `.claude/get-shit-done/`. Did not cover Obsidian vault internals. Result: `.obsidian/core-plugins.json ↔ .obsidian/plugins/dataview/styles.css` shows as strongest git coupling.

**Files (allowed):**

- Modify: `src/discovery/mod.rs` — `NoisePolicy::classify_path` (or equivalent) adds Obsidian paths to the personal-tooling set.
- Modify: tests for `NoisePolicy::classify_path`.

**Spec:**

- **objective:** `.obsidian/` and `wiki/.obsidian/` (anywhere in path) classify as `PersonalTooling`. Default-excluded from coupling output and from search unless `include_personal_tooling=true`.
- **non_goals:** Don't reorganize the vendor set. Don't exclude markdown content outside `.obsidian/`.
- **allowed_files:** as listed.
- **forbidden_files:** Coupling code.
- **interfaces touched:** `NoisePolicy::classify_path`'s personal-tooling list.
- **invariants:** Phase 2's personal-tooling set still classifies correctly.
- **acceptance_criteria:**
  - [ ] New test cases in `NoisePolicy::classify_path` tests covering `.obsidian/`, `wiki/.obsidian/`, `.obsidian/plugins/dataview/styles.css`.
- **evidence_required:** `cargo test --all-targets -- --test-threads=1`.
- **stop_conditions:** None expected.

**Steps:**

- [ ] **Step 1: Read** the existing `NoisePolicy::classify_path` implementation.
- [ ] **Step 2: Extend** the personal-tooling list.
- [ ] **Step 3: Add tests.**
- [ ] **Step 4: HALT for review.**

**Verification commands:**

```
cargo test --all-targets -- --test-threads=1
cargo clippy -- -D warnings
```

---

---

## Task H.13: Regression-suite gap analysis (added 2026-05-12 round-2 walk item 5)

**Severity:** P2 (deferred to C-4 post-Phase-4 stability sprint). Per round-2 product-lens review: 13 defects surfaced by 3 evaluators in 1 day indicates a test-surface gap. This task captures the meta-analysis in actionable form.

**Files (allowed):**

- Create: `docs/notes/regression-suite-gap-2026-XX-XX.md` — audit document.
- Possibly create new tests under `tests/` based on audit findings (proposed as follow-on tasks, not landed by H.13 itself).
- Forbidden: production source changes — those follow from audit results as separate tasks.

**Context:** Pre-Phase-H test surface allowed 16 verified defects (1 P0, 7 P1, 5 P2, 3 P3) to ship to main. Three independent evaluators using SymForge as a black box surfaced them within a day. (Catalog extended with B-P3-2 and B-P3-3 on 2026-05-12 round-2 walk item 7; H.13 context updated round-3 verification.) The catastrophe-fix regression tests added by H.1a-e + the user-trust tests added by H.2-H.6 close known bug surfaces, but the underlying "our tests would have missed these" question remains open. H.13 audits the gap.

**Spec:**

- **objective:** For each verified Phase H bug (B-P0-1, B-P1-1 through B-P1-7, B-P2-1 through B-P2-5, B-P3-1 through B-P3-3 — 16 total), document (a) what test would have caught it pre-shipping, (b) whether the project's existing test infrastructure could have housed that test, (c) what's missing in the test surface to enable similar bugs to be caught in the future. Output: a single audit doc + a prioritized list of test-surface investments.
- **non_goals:** No production code changes. No new test implementations within H.13 itself — recommendations land as separate follow-on tasks.
- **allowed_files:** as listed.
- **forbidden_files:** as listed.
- **interfaces touched:** None (analysis-only task).
- **invariants:** Audit considers only verified bugs (those with regression tests in H.1a-H.6). Does not speculate on hypothetical un-found bugs.
- **acceptance_criteria:**
  - [ ] Audit doc committed at `docs/notes/regression-suite-gap-2026-XX-XX.md`.
  - [ ] Each of the 16 verified bugs has a "test that would have caught it" entry naming the test category (unit / integration / fuzz / property / end-to-end / manual dogfood) and the rough effort estimate.
  - [ ] Top 3 test-surface investments identified with effort estimates.
  - [ ] Recommendations either committed as new tasks in the C-4 followup plan-doc OR explicitly deferred with rationale.
- **evidence_required:** Audit doc reviewed and accepted by plan owner.
- **stop_conditions:** STOP if the audit reveals fundamental tooling gap requiring its own multi-task investigation — carve that into a separate plan-doc rather than continuing H.13.

**Steps:**

- [ ] Step 1: Read each external evaluator report's repro section in `docs/notes/external-evaluations/2026-05-11/`.
- [ ] Step 2: For each verified bug, write a "test surface" entry describing the test that would catch it, the test category, and the rough effort estimate.
- [ ] Step 3: Group findings by test-surface category. Identify cross-cutting gaps (e.g., if 5 bugs would have been caught by Windows-AV-lock simulation, that's a gap worth investing in).
- [ ] Step 4: Identify top 3 highest-ROI test-surface investments. Effort estimate for each.
- [ ] Step 5: Land follow-on task entries in the C-4 stability followup plan-doc.
- [ ] Step 6: HALT for plan owner review.

**Verification commands:**

```
ls -la docs/notes/regression-suite-gap-2026-*.md
```

**Commit message:**

```
docs: regression-suite gap analysis for Phase H verified defects

Audits which test surface would have caught each of the 13 defects
surfaced by external evaluators on 2026-05-11. Identifies top 3
test-surface investments for C-4 followup sprint.

Per Phase H plan-doc round-2 walk item 5.
```

---

# Phase H — Closeout (two gates)

## Gate 1: Phase H C-1 close-out (before C-2 begins)

This is the catastrophe-fix gate. Restored trust in the index subsystem is the precondition for any further campaign work.

- [ ] H.1a, H.1b, H.1c, H.1d, H.1e, H.1f committed and pushed; each has its own regression test.
- [ ] All four `cargo` verification commands green on `--test-threads=1`.
- [ ] No regressions on the 1640+ pre-existing lib tests.
- [ ] Manual repro: index a large repo (1000+ files), idle 5 minutes, file count unchanged.
- [ ] **Automated smoke** `tests/watcher_aap_shaped_fixture.rs::aap_smoke_no_destruction` passes — see top-of-doc Phase H C-1 close-out gate for full criterion (added 2026-05-12 round-2 walk item 2).
- [ ] **User-side dogfood** — SymForge runs ≥30 min session on a non-symforge project including ≥1 cross-root `index_folder`; no catastrophic state loss reported (added 2026-05-12 round-2 walk item 2).
- [ ] `agentmemory memory_save` for the project-generation fence pattern decision.
- [ ] Master plan-doc updated with C-1 completion timestamp.
- [ ] Begin Phase H C-2.

## Gate 1.5: Phase H C-2 close-out (before resuming Phase 2.3) — added 2026-05-12 round-2 walk item 4

User-trust + correctness fixes restore the index-querying surface to a state where cross-project agent work is reliable. See Phase H C-2 close-out gate section in Task sequencing for full criteria.

- [ ] H.2, H.3, H.4, H.5, H.6, H.7 all committed and pushed with regression tests. (H.7 added 2026-05-12 round-2 walk items 9+10.)
- [ ] Each B-P1-* surfaced by external evaluators has a passing regression test.
- [ ] `agentmemory memory_save` for: daemon-degraded sticky-flag policy, find_dependents Pass 2 narrowing rationale, qualified-path collector unification rationale.
- [ ] Master plan-doc updated with C-2 completion timestamp.
- [ ] Resume Phase 2.3.

## Gate 2: Full Phase H close-out (after Phase 4 + C-4 followup sprint)

This is the campaign-end gate. Fires after Phase 4 and the C-4 stability followup sprint complete.

- [ ] All Phase H tasks (C-1, C-2, C-4) committed and pushed. (C-3 was eliminated 2026-05-12 round-2 walk item 4 — collapsed into C-2.)
- [ ] Each verified bug (B-P0-1, B-P1-1 through B-P1-7, B-P2-1 through B-P2-5, B-P3-1 through B-P3-3) has a regression test asserting the original repro no longer reproduces (or, for P3 polish items, an explicit acceptance-style assertion). (Updated round-3 verification — catalog has 16 entries after walk item 7 added B-P3-2/3.)
- [ ] Manual external-evaluator repro plan re-run end-to-end on HEAD — none of the 16 verified bugs surface.
- [ ] Plan-doc `docs/plans/2026-06-XX-symforge-stability-followup.md` (for C-4) marked complete or carved into yet another follow-on doc with rationale.
- [ ] `agentmemory memory_save` entries for: daemon-degraded sticky-flag policy, find_dependents Pass 2 semantics, qualified-path collector unification rationale.
- [ ] Knowledge-extractor pass to `docs/solutions/` for the durable patterns surfaced by this hotfix.

---

## Out-of-scope / deferred

The following are NOT in Phase H. Carve out to follow-up tasks before campaign close:

- **B-P3-1**: `search_text(group_by="usage")` markdown/doc filter — minor polish.
- **B-P3-2**: Truncation phrasing inconsistency — cosmetic.
- **B-P3-3**: Structural-pattern docs cookbook — docs task.
- Reconcile churn investigation beyond H.1 — once H.1 lands and we re-measure, decide if 18-repairs-per-3-min is now low or still high.
- Grammar bumps for non-Rust languages — separate hygiene pass.
- Obsidian vault internal git-coupling exclusion — H.12 addresses the path classifier; deeper temporal-subsystem path filtering is a separate task if H.12 doesn't fully resolve it.

---

## Deferred / Open Questions

### From 2026-05-12 review

- **C-1 close-out gate omits user dogfood signal** — Phase H C-1 close-out gate (P0, product-lens, confidence 0.86) — **RESOLVED 2026-05-12 (round-2 walk, item 2): Added both an automated AAP-shaped smoke (`tests/watcher_aap_shaped_fixture.rs::aap_smoke_no_destruction`) AND a user-side dogfood gate (≥30 min session on non-symforge project) to Gate 1 close-out. Belt-and-suspenders: CI captures workload shape, user-side captures real trust.**

  Original concern: stated goal is "restored trust in the index subsystem" for a tool whose user declined to install. Gate as written passes on author's machine and may still leave SymForge unusable for the user. Trust is user-side property. Resolution: added option C from the walk-through — both automated smoke (AAP-shaped fixture, dual-root, idle/idle) AND user-side dogfood (≥30 min session, ≥1 cross-root index_folder) are now Gate 1 criteria.

  <!-- dedup-key: section="phase h c1 closeout gate before resuming phase 23" title="c1 closeout gate omits user dogfood signal" evidence="RESOLVED via automated smoke + user dogfood added to Gate 1" -->

- **H.2 drops 'load time' half of B-P1-6** — Task H.2 spec (P1, coherence, confidence 0.83) — **RESOLVED 2026-05-12 (round-2 walk, item 3): Extended H.2 objective + conformance test to include load-time agreement assertion. The fix is already implicit in H.2's `daemon_degraded` sticky-clear (both surfaces then read from daemon's `published_state` so load_duration_ms converges naturally); making the assertion explicit closes B-P1-6 against Gate 2.**

  Original concern: B-P1-6 catalog cites watcher state AND load time disagreement; H.2 only addressed state. Resolution: extend conformance test name to `both_paths_agree_on_watcher_state_and_load_time` and add explicit `load_duration_ms` byte-for-byte agreement assertion. No new implementation work — the fix was already in H.2's spec implicitly via sticky-flag clearing.

  <!-- dedup-key: section="task h2 spec" title="h2 drops load time half of bp16" evidence="RESOLVED via H.2 test extension" -->

- **Tier 2 health/refs scheduled after Phase 3 ranker work** — Task sequencing (P1, product-lens, confidence 0.82) — **RESOLVED 2026-05-12 (round-2 walk, item 4): Restructured C-2 to include H.2 + H.3 + H.4 + H.5 + H.6 all landing BEFORE Phase 2.3 resumes. Original C-2 "ranker-substrate prereq" and C-3 "opportunistic" merged into new C-2 user-trust + correctness bucket. New Gate 1.5 (C-2 close) added between C-1 close and Phase 2.3.**

  Original concern: all P1 correctness fixes were deferred past Phase 2.3-3.2, leaving user-side trust broken through that whole window. Resolution: re-bucket to land before Phase 2.3 (+7-9 days added to critical path; net benefit user-trust restored end-to-end before any feature work).

  <!-- dedup-key: section="task sequencing" title="tier 2 healthrefs scheduled after phase 3 ranker work" evidence="RESOLVED via C-2 restructure adds H2 H3 H4 H5 H6 before Phase 23" -->

- **Stated goal narrower than 'usable build' problem** — Goal / Phase H bug catalog (P1, product-lens, confidence 0.72) — **RESOLVED 2026-05-12 (round-2 walk, item 5): Added Task H.13 (regression-suite gap analysis) to C-4 bucket. Captures the meta-question "why didn't our tests catch these" in actionable form; concrete test-surface investments deferred to C-4 followup sprint. Round-2 walk also added user-side dogfood gate (item 2), AAP-shaped automated smoke (item 2), and pulled all P1 correctness fixes before Phase 2.3 (item 4) — which together implicitly address the user-trust framing concern.**

  <!-- dedup-key: section="goal phase h bug catalog" title="stated goal narrower than usable build problem" evidence="RESOLVED via H.13 regression-suite gap analysis added to C-4" -->

- **B-P3-2 and B-P3-3 referenced but never defined in catalog** — Out-of-scope / deferred (P2, coherence, confidence 0.92)

  Readers cannot match the deferral list against the bug inventory. The catalog defines exactly one P3 row (B-P3-1). The deferral list cites B-P3-2 ('Truncation phrasing inconsistency') and B-P3-3 ('Structural-pattern docs cookbook') as if they were catalogued bug IDs, but neither appears in the catalog. This means either the catalog is incomplete (two real bugs are missing from the verified inventory) or the IDs are inventions. Either way the deferral list cannot be audited against the catalog.

  **RESOLVED 2026-05-12 (round-2 walk, item 7):** Added B-P3-2 (truncation phrasing inconsistency) and B-P3-3 (structural-pattern docs cookbook) as proper P3 catalog entries. Audit trail now complete; out-of-scope references resolve to real catalog rows.

  <!-- dedup-key: section="outofscope deferred" title="bp32 and bp33 referenced but never defined" evidence="RESOLVED via catalog entries added for B-P3-2 and B-P3-3" -->

- **H.1d title lists 'session rebind' not covered in spec** — C-1 task list vs Task H.1d (P2, coherence, confidence 0.82) — **RESOLVED 2026-05-12 (round-2 walk, item 8): Added explicit session-rebind surface audit to H.1d acceptance_criteria. Session-rebind code change lives in H.1b Step 9 (`index_folder_for_session` removal site at `src/daemon.rs:504-508`); H.1d adds a cross-task safety-net test `index_folder_for_session_signals_token_before_drop` so the surface is double-verified at H.1d HALT-for-review even though the code change is owned by H.1b.**

  Implementers reading the C-1 enumeration will believe H.1d covers four surfaces: index_folder, restart_watcher, sidecar freshen, and 'session rebind'. The H.1d task body never names a session-rebind surface in its Files, Spec, or Steps. Either the summary promises scope that the spec drops, or H.1d is silently missing a fourth surface and ships incomplete. The two readings have very different consequences.

  <!-- dedup-key: section="c1 task list vs task h1d" title="h1d title lists session rebind not covered in spec" evidence="H1d Sibling leak audit SymForgeServerindex_folderrestart_watcher sidecar freshen_sidecar_path_if_stale session rebind" -->

- **C-3 'independent of Phase 3 scope' contradicts revised sequence** — C-3 prose vs Revised campaign sequence (P2, coherence, confidence 0.78) — **RESOLVED 2026-05-12 (round-2 walk, item 4 collateral): C-3 bucket eliminated; H.2/H.3/H.6 moved into restructured C-2 (user-trust + correctness, lands before Phase 2.3). Contradiction no longer applies.**

  <!-- dedup-key: section="c3 prose vs revised campaign sequence" title="c3 independent of phase 3 scope contradicts revised sequence" evidence="RESOLVED via C-3 bucket elimination in C-2 restructure" -->

- **C-4 framed 'minor' but contains P1 task (H.7)** — C-4 description vs H.7 severity (P2, coherence + scope-guardian + product-lens, confidence 0.74) — **RESOLVED 2026-05-12 (round-2 walk, items 9+10): H.7 promoted out of C-4 to C-2 user-trust + correctness bucket. Profile-first uncertainty handled via escalation note (>3 days profile scope → escalate to plan owner). C-4 reframed to "Diagnostic + minor stability followup (post-Phase-4)" matching its actual contents (H.8-H.13 + ADR).**

  C-4 is described as 'non-blocking diagnostic + minor correctness issues' to be deferred to a post-Phase-4 stability sprint. H.7 (batch_rename timeout) is bucketed in C-4 but is labeled Severity P1 in its own task header. Sequencing buckets are meant to communicate severity-to-deferral mapping, so reading C-4's framing will mislead someone planning the followup sprint into treating it as low-priority cleanup when one of its tasks is a P1 correctness defect that makes the refactor tool unusable. Three personas independently flagged variations of this concern (coherence on framing, scope on per-task justification, product-lens on lack of calendar owner).

  <!-- dedup-key: section="c4 description vs h7 severity" title="c4 framed minor but contains p1 task" evidence="C4 Defer to post-Phase-4 stability sprint Non-blocking diagnostic minor correctness issues" -->

- **C-4 deferred sprint has no scheduled owner / calendar bound** — Task sequencing / C-4 / Revised campaign sequence (P2, product-lens, confidence 0.72) — **RESOLVED 2026-05-12 (round-2 walk, items 9+10): Added calendar commitment to C-4 — 1-week sprint within 30 days of Phase 4 close. Plan-doc filename `2026-06-XX-symforge-stability-followup.md` resolves to actual Phase 4 close date. If sprint cannot start within 30-day window, plan owner re-evaluates remaining items.**

  C-4 (H.7-H.12) is named 'defer to post-Phase-4 stability sprint' and is supposed to land in 'docs/plans/2026-06-XX-symforge-stability-followup.md'. B-P1-1 (batch_rename timeout) is by the plan's own classification a P1 'MCP returns wrong answers' tier item — and the primary refactoring tool. Putting it after Phase 3 + Phase 4 + the carve-out of a brand new plan-doc creates a momentum-carry risk: Phase 4 closes, the campaign feels done, and the dedicated stability sprint becomes the kind of follow-up that gets perpetually re-prioritized. The plan should commit a calendar window (or block Phase 4 close on it), not a TBD plan-doc filename.

  <!-- dedup-key: section="task sequencing c4 revised campaign sequence" title="c4 deferred sprint has no scheduled owner" evidence="Plan L110 C-4 Defer to post-Phase-4 stability sprint Carve into separate plan-doc docsplans2026-06-XX-symforge-stability-followup" -->

- **H.11 and H.12 don't serve stated Phase H goal** — Bug catalog vs Phase H goal (P2, scope-guardian, confidence 0.72) — **RESOLVED 2026-05-12 (round-2 walk, item 11): Reframed C-4 to explicitly include "incidental items from evaluation" alongside diagnostic + minor correctness. H.11 (sidecar wording) and H.12 (Obsidian vault classifier) are now honestly labeled as incidental items rather than implicitly miscategorized as index-subsystem repair.**

  The plan's stated goal is 'Repair catastrophic and high-severity defects' to restore trust in the index subsystem. H.11 adds a diagnostic line to health output for sidecar PID/alive; H.12 reclassifies `.obsidian/` paths as personal-tooling noise. Neither involves a defect in the index subsystem — they are personal-workflow polish that surfaced incidentally in the evaluator reports. H.11's parent bug B-P2-3 is wording in a health diagnostic. H.12's parent bug B-P2-4 is that personal vault files appear in coupling output for the user's own setup. These are legitimate items but the scope rationale 'stability hotfix to restore trust in index subsystem' does not cover them. They belong in a separate noise-policy/diagnostic-polish micro-batch.

  <!-- dedup-key: section="bug catalog vs phase h goal" title="h11 and h12 dont serve stated phase h goal" evidence="Plan line 5 Goal Repair catastrophic and high-severity defects surfaced by three external evaluator reports" -->

- **Cancellation token leaves silent skip risk (no telemetry counter)** — H.1a fence design / H.1c stop_conditions (P2, product-lens, confidence 0.68) — **RESOLVED 2026-05-12 (round-2 walk, item 12): Added `rejected_stale_mutations: AtomicU64` counter to H.1a (alongside `project_generation`); fenced-mutation methods now increment on rejection. Added `current_rejected_stale_mutations()` accessor. Added new test `rejected_stale_mutations_counter_increments_on_fence_rejection`. H.2 acceptance extended to render the counter in `health` output (`Stale-mutation rejections: N` line). Counter is the user-visible telemetry surface for catching silent-skip races introduced by future Phase 2.3/3/4 work.**

  The three-layer defense is well-designed for the destroy-the-index failure mode. But the inversion question 'what would make this fail?' has a quiet answer the plan does not name: the fenced API silently no-ops on generation mismatch ('return false; do not log at warn level by default; trace-only to avoid spamming on every doomed iteration'). After Phase H ships, if a NEW race is introduced (Phase 2.3, 3, or 4 work) that causes legitimate removes to fire against the wrong generation, those removes vanish silently and the user-visible symptom becomes 'index doesn't reflect deletes' rather than 'index destroyed' — a quieter, slower-to-detect failure mode. A counter (`rejected_stale_mutations`) surfaced in health would let the regression-test surface and the dogfood loop both detect new variants — without the spam cost.

  <!-- dedup-key: section="h1a fence design h1c stop_conditions" title="cancellation token leaves silent skip risk" evidence="Plan L194-196 Step 5 of H1a If current expected_gen return false do not log at warn level by default trace-only" -->

- **C-3 'opportunistic' bucket buries P1 budget enforcement (H.6)** — Task sequencing / C-3 / H.6 (P2, product-lens, confidence 0.65) — **RESOLVED 2026-05-12 (round-2 walk, item 4 collateral): C-3 bucket eliminated; H.6 moved into restructured C-2 (user-trust + correctness, lands before Phase 2.3) alongside other P1 fixes. No longer 'opportunistic' framing.**

  <!-- dedup-key: section="task sequencing c3 h6" title="c3 opportunistic bucket buries p1 budget enforcement" evidence="RESOLVED via C-3 bucket elimination in C-2 restructure" -->

- **Failure-mode matrix row 5 promises git_temporal coverage no C-1 task implements** — Failure-mode coverage matrix (Tier 1 design rationale) (P0, 4-persona convergence: coherence + feasibility + product + scope, confidence 0.98 after cross-persona boost) — **RESOLVED 2026-05-12 (round-2 walk): Added Task H.1e for git_temporal generation-fencing. Matrix row 5 amended to reference H.1e as owner. See Task H.1e spec for details.**

  Original concern: Round-1 LFG inserted a five-row defense matrix that names `git_temporal::spawn_git_temporal_computation` as a fifth spawn site requiring 'Generation guard on swap'. Symbol `git_temporal::swap` does not exist; actual API is `SharedIndexHandle::update_git_temporal`. Neither H.1a-d touched the git_temporal publication path. Resolution: add Task H.1e to C-1 bucket, with allowed_files = `src/live_index/store.rs` + `src/live_index/git_temporal.rs` + tests, owning the fenced-API extension to git_temporal publication.

  <!-- dedup-key: section="failuremode coverage matrix tier 1 design rationale" title="matrix row 5 promises git_temporal coverage no c1 task implements" evidence="RESOLVED via H.1e" -->

- **process_events line range mismatch between matrix and H.1c Step 7** — Failure-mode matrix vs Task H.1c Step 7 (P1, coherence, confidence 0.85) — **RESOLVED 2026-05-12 (round-2 walk, item 6): Source verification via `grep -n "fn process_events" src/watcher/mod.rs` returns line 429; function body ends at line 488. H.1c Step 7's `:429-488` was correct; matrix's `:587-604` was wrong (likely confused with `run_watcher` inner loop). Matrix row updated to `:429-488`.**

  <!-- dedup-key: section="failuremode matrix vs task h1c step 7" title="process_events line range mismatch between matrix and h1c step 7" evidence="RESOLVED via source verification - process_events is at 429-488" -->

- **Coupling-refresh matrix row: mechanism A claimed but Layer 2+3 N/A; closure body non-cancellable mid-execute** — Failure-mode matrix (row 2) (P2, 3-persona convergence: coherence + scope + adversarial, confidence 0.72 after cross-persona boost) — **RESOLVED 2026-05-12 (round-2 walk, item 13): Added Task H.1f for coupling-refresh generation-fencing. Matrix row 2 updated to reference H.1f as Layer-2 owner with explicit "closure body is not cancellable mid-execute — Layer 2 load-bearing" annotation. H.1f threads expected_gen through refresh_on_reconcile_tick and gates store-write commit on current_project_generation() match.**

  Three personas converged on different concerns about matrix row 2. (1) Coherence: row 2 asserts Mechanism A is in scope but both Layer 2 ('coupling store is workspace-scoped already') and Layer 3 ('no remove_file call') are 'Not applicable.' Only Layer 1 is listed as active defense — yet the document elsewhere argues "Why Layer 1 alone is insufficient." Either this row is a counterexample to the dual-layer rationale or Mechanism A is not actually reachable here. (2) Scope: row 2 is filler — both rows 1 and 2 reference overlapping line ranges (`:559-575` vs `:569-575`) and row 2's only-Layer-1 coverage adds visual completeness without adding analytic content. (3) Adversarial: 'Token check cancels before refresh' is misleading — the token check fires before `spawn_blocking` is scheduled, but once `refresh_on_reconcile_tick` is running (potentially walking thousands of files), the closure body has no cancellation surface. A doomed coupling-refresh task that started just before the cancellation signal will run to completion writing to a workspace-scoped store. If the workspace store ownership changed (root A's task writing into root B's store), the doomed task corrupts B's coupling data — a weaker variant of B-P0-1 in a different subsystem. Open question: (a) drop matrix row 2 as filler; (b) thread `expected_gen` into `refresh_on_reconcile_tick` so its store writes are fenced; (c) document this as accepted lesser-severity residual exposure rather than 'Not applicable.' Plan owner decision required.

  <!-- dedup-key: section="failuremode matrix row 2" title="couplingrefresh matrix row mechanism a claimed but layer 23 na closure body noncancellable midexecute" evidence="Coupling refresh srcwatchermodrs569-575 A stale workspace Token check cancels before refresh Not applicable coupling store is workspacescoped already Not applicable no remove_file call" -->

---

End of plan-doc.
