# SymForge — Work Ledger (post-v11.0.0)

**Audience**: an agent with nothing but a fresh clone. No MCP servers, no
agentmemory, no prior session, no chat history. Everything you need to pick up
work is either in this file or reachable from a path named in it.

**Drafted**: 2026-08-20, at the close of the Feature 020 Slice 4 activation cut
(shipped as the 11.0.0 breaking release).

**Scope**: this is a ledger of what is *owed*, not a status report. Volatile
state (SHAs, branches, open PRs, current version) is deliberately absent — see
§0 for the command that generates it.

---

## 0. Get current state — never trust a hand-written SHA

```
pwsh scripts/campaign-state.ps1          # human-readable
pwsh scripts/campaign-state.ps1 -Json    # machine bootstrap
```

It reads git, `gh`, and `Cargo.toml` on `origin/main` and filters untracked
files that are byte-identical to `main`. Anything in a doc that contradicts it
is stale, including anything in this file that looks like a fact about *right
now*.

Binding rule (repo `CLAUDE.md`): git SHAs, branch/worktree lists, "X is
uncommitted", open PR numbers, and the current version are **never** typed into
a document. Cite the command.

---

## 1. Traps that will cost you a day if you skip this section

### 1.1 `specs/020-repository-knowledge-index/` is frozen — its checkboxes are not a ledger

The tree is immutable post-T012, **including checkbox bytes**
(`specs/020-repository-knowledge-index/tasks.md:858-865`). The `- [ ]` / `- [x]`
marks are a snapshot from the moment of freeze. They do not track progress and
must never be edited.

Proof that reading them as progress is wrong: every Gate H and Gate I row is
unchecked, yet `src/knowledge/`, `src/protocol/knowledge_search.rs`,
`knowledge_review.rs`, `knowledge_curation.rs`, `knowledge_model.rs` and the
registered `search_knowledge` tool are all live in the shipped binary.

**Live progress lives in**, in descending authority:

| What | Where |
|---|---|
| As-executed record of the Slice 4 cut, with every deviation | `specs/028-preventive-activation-cut/activation-cut-execution-map.md` |
| Slice 4 evidence + adversarial review rounds + "what green does not prove" | `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` |
| Slice 3 / Slice 2 evidence | `docs/reviews/FEATURE-020-SLICE3-EVIDENCE-v11.md`, `…SLICE2-EVIDENCE-v11.md` |
| Machine-checked acceptance roster (the only self-verifying one) | `scripts/slice0-oracle-artifact.cjs` |
| Requirement↔oracle↔retirement traceability | `node scripts/validate-lifecycle-oracle-traceability.cjs` |
| Migration boundary and its open scopes | `docs/migrations/v11-index-lifecycle.md` |
| Project rules, gates, merge mechanics | `CLAUDE.md`, `AGENTS.md`, `.specify/memory/constitution.md` |

### 1.2 Lib unit-test paths carry an `internals::` prefix

The V11 cut mounts the server modules under `src/internals.rs` via `#[path]`.
Every `cargo test --lib … -- --exact` filter written before the cut silently
selects **nothing** and exits 0 having run nothing. Correct form:

```
cargo test --lib internals::daemon::tests::<name> -- --exact --ignored
```

This broke `scripts/slice0-oracle-artifact.cjs` in post-merge CI. Assume it
broke other pre-cut filters that nobody has run yet.

### 1.3 The default-feature gates cannot catch a feature-gated `cfg` mistake

Before pushing anything that adds a `#[cfg(test)]` helper, run exactly:

```
cargo test --no-default-features --features embed --lib -- --test-threads=1
```

A `#[cfg(test)]` item whose only consumer sits behind `feature = "server"` is an
unused import in that cell, and `-D warnings` fails the build — while every
default-feature gate passes. When the consumer is server-only, gate the helper
`#[cfg(all(test, feature = "server"))]`.

### 1.4 Long builds must not run through a 10-minute-capped tool

A cold `cargo test --all-targets` here takes ~25 minutes. Killing it mid-write
corrupts `target/` and produces errors that look like code defects and are not
(`E0786 found invalid metadata files`, rustc ICEs, `E0463 can't find crate`).
Run long cargo through a daemon that owns the process, and never interleave
feature sets in one target dir — run `--all-targets` and
`--no-default-features --features embed` one at a time.

Recovery, cheapest first: delete `target/debug/incremental` → `cargo clean -p
symforge` → full `cargo clean`.

### 1.5 The reporting invariant is binding, and it is what this project sells

> A component may not report success for an operation whose completion it did
> not observe.

Not "attempted", not "the code path was called" — observed. When you add any
status line, banner, envelope, or success return, answer in the PR: *what did
this observe, and what does it emit when the observation fails?* Six defects
fixed on 2026-08-06 were this one defect wearing different clothes, and all six
shipped green.

### 1.6 Merge mechanics

Squash by default, always with an explicit body:

```
gh pr merge <N> --squash --delete-branch \
  --subject "<conventional PR title> (#<N>)" \
  --body "<one short paragraph, no parentheses, no colon-bearing prose lines>"
```

The default squash body concatenates inner commit messages; any line shaped like
`word: text` impersonates a conventional header and one parse error makes the
entire commit invisible to release-please — no version bump, no changelog, no
error raised. Release PRs themselves stay `--merge`.

---

## 2. The ledger

Four tracks are live. Everything else in `specs/` is dormant (§3).

### Track A — eleven Slice 0 acceptance controls are still RED, and every owning slice has already shipped

This is the largest and most material item. Feature 020 Slice 0's product was
positive controls observed failing *before* the machinery existed. Eleven still
fail. Each names an owning slice in its `#[ignore]` prose; **all of those slices
have landed**. So these are eleven frozen-tree acceptance behaviors that are
owed and currently unowned by any remaining slice.

**Roster file (authoritative, fail-closed)**: `scripts/slice0-oracle-artifact.cjs`
— `RED_CASES` (11) and `RESOLVED_CASES` (1).
**Test bodies**: `tests/project_index_lifecycle_slice0.rs` (10) and
`src/daemon.rs` lib tests (1).
**Run it**: `node scripts/slice0-oracle-artifact.cjs` → writes
`target/ci/lifecycle-v11/slice-0-oracle-contract.json`. CI's `rust` job runs it.

**DISPOSITIONED 2026-08-21** by two independent read-only passes (LINUS,
HOLMES) on shipped `main`, reconciled with no `cargo` battery and no patches.
This is a **disposition table, not an implementation queue.**

The eleven split into two kinds, and the distinction decides who is wrong:

### Control-stale (3) — retarget the test body; do NOT change production to win

The property is still worth having, but the body asserts a *retired encoding*.
Rewriting production to satisfy the old assertion would move the code away from
the frozen oracle, not toward it.

| Control | Why the body is stale |
|---|---|
| `capacity_refused_open_creates_no_slot_and_no_watcher` | V11 answers a refused open with `Ok` + typed `SourceRefusal` + a non-ready slot, not `Err` + zero slots. FR-004 strict acquisition is the lease. Unmeasured residual: `activate` still starts a watcher (`daemon.rs:3398-3403`) |
| `whole_project_publication_preserves_latest_siblings` | Frozen oracle is pause A / publish B / rebase / tokens / one store. The body races V10 `LiveIndex::reload` against 1500 files in 150 ms |
| `configured_capacity_bounds_the_process_not_each_load` | FR-004 makes capacity a per-candidate catalog; SC-025 owns `ProcessCapacityPool`; `SYMFORGE_MAX_INDEX_FILES` is per discovery pass. Forcing it process-wide fights FR-004 and still misses SC-025 |

### Code-wrong (8) — keep ignored and fail-closed; do not un-ignore for green

| Control | Live miss |
|---|---|
| `empty_placeholder_publication_refuses_watcher_mutation` | `add_file` (`store.rs:2820-2831`) has no EmptyBootstrap gate; the default-suite check at `store.rs:6402-6412` is a paper-over |
| `failed_reload_retains_the_recovery_observer` | aborts the watcher then `?`; no replacement on `Err` |
| `observer_replacement_gap_is_latched_as_non_current` | `recompute_freshness_locked` drops the historical gap → `Current` |
| `old_observer_delivery_after_promotion_is_not_current` | same rederive; no token fence |
| `snapshot_seed_is_not_queryable_before_verification` | persist hydrates files immediately; `get_file` has no Pending gate; `is_ready()` is status-only |
| `same_path_root_replacement_is_not_silently_adopted` | path-keyed map; publishes `Current` |
| `concurrent_first_open_performs_exactly_one_cold_load` | load happens outside the lock then `or_insert`; `admit_project` does not skip bootstrap |
| `watcher_mutation_during_candidate_build_is_not_discarded` | `store.rs:2403-2436` still reaches `swap_and_publish`; `IsolatedCandidate` appears **zero** times in `store.rs` |

### Seam map — the eight code-wrong controls are SIX seams, not eight bugs

**Established 2026-08-21** by two independent read-only passes (LINUS, HOLMES)
on `main` @ `fd4de8dc`, briefed under the sealed protocol so neither saw the
other's table or the requester's guess. They agree.

| Seam | Mechanism | Closes |
|---|---|---|
| 1 | `EmptyBootstrap` / `add_file` gate | `empty_placeholder_publication_refuses_watcher_mutation` |
| 2 | `reload_with` abort-then-`?` (no replacement on the error path) | `failed_reload_retains_the_recovery_observer` |
| 3 | **Store reload trunk** — `recompute_freshness_locked` + `swap_and_publish` | `observer_replacement_gap…`, `old_observer_delivery…`, `watcher_mutation_during_candidate_build…`, **and the Current-half of** `same_path_root_replacement…` |
| 4 | persist hydrate + `get_file` Pending gate | `snapshot_seed_is_not_queryable_before_verification` |
| 5 | path-keyed `PROJECT_AUTHORITIES` (`activation.rs:894`, `HashMap<PathBuf, _>`) | the **identity-half of** `same_path_root_replacement…` |
| 6 | `ensure_project_slot` / `or_insert` in `src/daemon.rs` | `concurrent_first_open_performs_exactly_one_cold_load` |

**Three things this changes:**

1. **Seam 3 is the prize.** One owner closes four of the eight. It is also the
   seam where `IsolatedCandidate` appears **zero** times in `store.rs` —
   independently re-verified — so the candidate pipeline was never wired into
   the publication trunk at all.
2. **`same_path_root_replacement…` is two problems wearing one name.** It needs
   seams 3 *and* 5. Any plan that treats it as one item will half-fix it.
3. **Owning any single seam leaves leftovers.** There is no ordering that
   discharges Track A incrementally without carrying residue, and a plan that
   claims otherwise has mis-grouped something.

### Seam 3 is one function with TWO jobs — and 3b alone leaves three of four RED

**Established 2026-08-22** (HOLMES design read on `fd4de8dc`), verified here
against source.

| Job | What it is | Closes |
|---|---|---|
| **3a — freshness latch** | `recompute_freshness_locked` (`store.rs:1883-1890`) explicitly filters out `ObservationFailed`, `ReconciliationPending` and `SnapshotVerificationFailed`; and `store.rs:2417-2424` writes `FreshnessStatus::Current` unconditionally when coverage is not degraded | `observer_replacement_gap…`, `old_observer_delivery…`, Current-half of `same_path…` |
| **3b — candidate wiring** | the reload trunk reaches `swap_and_publish` and never touches the candidate pipeline | `watcher_mutation_during_candidate_build…` |

**The trap**: "wire `IsolatedCandidate` into publication" sounds like the whole
of seam 3. It is only 3b. `IsolatedCandidate` does not mention
`FreshnessStatus` at all, so owning it without the two 3a edits leaves **three
of seam 3's four controls red** — while looking like the seam was closed.

The 3a filter is worth seeing, because it is deliberate code rather than an
omission:

```rust
for reason in reason_codes.iter().copied().filter(|reason| {
    !matches!(reason,
        FreshnessReason::ObservationFailed
        | FreshnessReason::ReconciliationPending
        | FreshnessReason::SnapshotVerificationFailed)
}) {
```

### 3b is a data-plane cut, not a helper call — the types do not meet

Verified: `IsolatedCandidate::commit(self, root: &ProjectArtifactRoot) ->
Result<Arc<ProjectArtifacts>, PromotionRefusal>` (`candidate.rs:373-376`), and
**`LiveIndex` appears zero times in `candidate.rs`**. There is no overload that
publishes a `LiveIndex`.

So the store trunk cannot call into the candidate pipeline and keep publishing
through `swap_and_publish`. Doing both is not wiring — it is a second dark run
beside the same V10 swap. Real 3b means ending the mid-cut split: query necks
stop reading `ProjectRuntimeHandle::data_plane()` and start reading the
publication root. That is a census of `data_plane()` call sites (the C4/C5
work), not a one-function edit.

### Activation mode is NOT at risk, and that matters for scoping

`reload_for_binding_with_exclusions` does not call `ActivationCut`; neither does
`IsolatedCandidate::commit`. Mode changes only by typed transition evidence and
has no reverse edge. The process is already `PreventiveV1Open`. **Seam 3 work
cannot flip it.**

But the corollary is the scoping finding:

> The frozen Slice 5 constraint is "do not change runtime authority, public
> behavior, writer reachability, or activation mode." Seam 3 satisfies the
> activation-mode half — and **violates the public-behaviour half by design**,
> because closing those four controls *is* a behaviour change.

**Therefore Track A seam work is not Slice-5-shaped and must not ride under
Slice 5's neutrality bracket.** They are different kinds of work: Slice 5
removes what is provably dead and changes nothing; seam work changes behaviour
on purpose. Anyone tempted to fold them into one campaign should stop here.

### PR #609 touches none of the eight

**Established 2026-08-22** (LINUS read on the same merge-base, `fd4de8dc`).
#609 is an orphan-admission cleanup on failed daemon *bootstrap*: it retires a
pending admission when bootstrap returns a non-capacity `Err` and the opener is
the last holder. Real, local, and unrelated to the map.

- **S5 (`PROJECT_AUTHORITIES`, `activation.rs:894-911`): not in the diff.**
- **S6 (`ensure_project_slot…/or_insert`, `daemon.rs:1137-1142`): not in the diff.**
- `registry.rs` (+93) adds `AdmissionJoin` / `AdmissionAttempt` / `stop_if_unheld`,
  keyed by `ProjectKey` — adjacent types, not a missed seam.
- All eight controls remain red; #609 adds a ninth, already-aimed cleanup.

**Its pin refresh is honest.** LINUS could not recompute (no local tree) and
recorded it as an unverified residual. Recomputed here from the git blobs, and
the method was validated by reproducing `main`'s pins exactly first:

```
main @ fd4de8dc  FULL     96b7a77f… 196 9300142   claimed identical   MATCH
#609 @ 65f96c97  FULL     258ee682… 196 9307806   claimed identical   MATCH
#609 @ 65f96c97  EXCLUDED b4c9f548…  20  393840   claimed identical   MATCH
```

Note the file set is **all** files under `src/`, not just `*.rs` — three
non-Rust assets under `src/server/admin/assets/` are included, which is why a
`.rs`-only recompute yields 193 files and the wrong digest.

### Track A and Track B are not disjoint

The seam map collapses part of the board, which nobody had connected before:

| Seam | Is also Track B residual |
|---|---|
| 4 — persist hydrate (`persist.rs:2121-2129`, `SnapshotStore` unwired) | **B4**. LINUS draws a line HOLMES did not: `get_file` (`query.rs:1220`) is the same *family* but is not B4 itself. Treat B4 as the hydrate half. |
| 5 — path-keyed `PROJECT_AUTHORITIES` (`activation.rs:894-911`) | **B1** — retarget-in-place admission identity |

**Not B2, and the distinction is worth keeping.** The requester initially
recorded seam 5 as "plausibly B2 as well"; both reviewers rejected that and
were right to. The subtlety: `activation.rs:894-911` is simultaneously the
`PathBuf`-keyed map (seam 5's mechanism) *and* `project_source_authority`
(B2's subject) — same file, same function, different claim. B1 is the identity
behaviour the control asserts; B2 is a separate observation about the lookup
staying a per-root static after the flip. Sharing a code site does not make
them the same residual.

So two "unowned Track A controls" already have named Track B owners, and
closing B1/B2/B4 discharges Track A work as a side effect. Plan them as one
body of work, not two tracks.

### The "precondition window unreachable" claim was false

Two controls carried `#[ignore]` text asserting their precondition window was
unreachable. On `watcher_mutation_during_candidate_build_is_not_discarded` that
is **false on the shipped tree** — the window is reachable; the seam simply
never routes through the candidate pipeline. The other, 
`whole_project_publication_preserves_latest_siblings`, is control-stale rather
than unreachable. Both strings were corrected in the tree on 2026-08-21.

That claim originated from a process that ran the controls un-ignored, observed
failure, and wrote down *that* they failed — then described *why* without
having established it. It stood unchallenged for a day and would have
misdirected whoever picked this up next.

### Owed, not done

`src/daemon.rs`'s `#[ignore]` string still predicts "remove this attribute in
Slice 2", a slice that shipped. The correction is written and was reverted
unapplied: `daemon.rs` is inside `FULL_SOURCE_PIN_V1`'s file set, so editing it
moves a seal that only the Rust oracle may recompute, which needs a full gate
run. Do it in a change that can afford one.

### Track A residuals from the reviewers

- No `cargo test` battery was run against the ignored roster in these passes.
- `src/server_api` was not fetched; MCP `is_ready` vs `get_file` is unaudited.
- Do not reopen PR #603 to discharge any of this.

**The roster's fail-closed contract, so you do not fight it:**

- A `RED_CASES` control that *starts passing* is a **CI error**, not a success.
  Either the defect was fixed without its slice removing the `#[ignore]`, or the
  control went vacuous. Both need a human.
- To land a fix, move the control into `RESOLVED_CASES` with `slice`, `tasks`,
  `defect`, and `fix` fields, delete its `#[ignore]`, and let it run in the
  default suite. Never delete a control — that deletes the regression guard and
  the evidence of the RED→GREEN transition.
- A resolved suite must **not** pass `--ignored` (with the attribute gone it
  would select nothing and exit 0 having run nothing).

The one already-resolved row is the pattern to copy:
`internals::watcher::tests::generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b`
(Slice 1, T028, defect 2.8).

### Track B — six recorded design residuals from the Slice 4 cut

Adjudicated open with named owners across three adversarial review rounds, all
assessed sub-P2. Full text in
`docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` §5 and §7a–7c, and
in the C-group records of
`specs/028-preventive-activation-cut/activation-cut-execution-map.md`.

| # | Residual | Where it lives |
|---|---|---|
| B1 | Retarget-in-place admission identity — a root physically replaced at the same path keeps its admission identity | `src/index_lifecycle/` admission + transitions (C4b/C5 records) |
| B2 | `project_source_authority` remains a per-root static convergence lookup after the flip | `src/index_lifecycle/activation.rs:899` — **corrected 2026-08-21**; the previously recorded `authority.rs` path is stale, that file holds no such function and no `PathBuf` map |
| B3 | Serve access-mode threading — the serve path surfaces no `RootBinding` and presents `NormalProject` | serve loader path |
| B4 | `SnapshotStore` per-entry verify-state wiring into the live restore path | `src/live_index/persist.rs`; scope stated in `docs/migrations/v11-index-lifecycle.md` §4 |
| B5 | Supersede multi-party heal residual — a multi-party interleave following a crash-orphan can still strip a live marker (bounded to a microsecond window by a double-checked re-read; no name-based marker scheme closes it fully) | stale-marker heal path, comment states the bound in code |
| B6 | Kilo init classification | `src/cli/init.rs` |

Also recorded, and cheap: the `OBSERVED-REFRESH-GATE-v1.md` sub-millisecond
lanes quantize 1→2 ms against a ratio gate below the measurement quantum. The
honest bound is recorded; **microsecond receipts** are the named follow-up
(`docs/reviews/OBSERVED-REFRESH-GATE-v1.md`).

### Track C — Slice 5: mechanical removal (T074–T077) — not started

Roster: `specs/020-repository-knowledge-index/tasks.md` Phase 7 (lines 958–966).
Goal, verbatim: *"Delete only code already proven unreachable in Slice 4; do not
change runtime authority, public behavior, writer reachability, or activation
mode."*

| Task | Deliverable | Artifact status |
|---|---|---|
| T074 | Capture pre-cleanup public API / authority-reachability / behavior / activation baseline | `docs/reviews/FEATURE-020-SLICE5-BASELINE-v11.md` — **does not exist** |
| T075 | Remove unreachable placeholder storage, bootstrap/circuit-breaker lifecycle fields, legacy mode branches, secondary publication roots, obsolete tests, compatibility comments | `src/` |
| T076 | Remove dead V10 embed implementation — **only after** the allowlist negative suite proves it unnameable | `src/embed.rs` |
| T077 | Re-run the T074 baseline, prove Slice 5 changed nothing, complete the post-slice adversarial review | `docs/reviews/FEATURE-020-SLICE5-EVIDENCE-v11.md` — **does not exist** |

This is the next sequential slice and it is unblocked. It is also the *safest*
remaining track: a pure-deletion slice whose acceptance criterion is "the
baseline is bit-identical afterward".

### Track D — Phase 8: release and adversarial closure (T078–T090) — not started

Roster: `specs/020-repository-knowledge-index/tasks.md` Phase 8 (lines 968–985).

**Read this first**: 11.0.0 already shipped to npm, crates.io, and GitHub
Releases at the Slice 4 cut, because Slice 4 *is* the V11 breaking lifecycle/embed
boundary. The Phase 8 formal closure gate did **not** run. The version is out;
the gate is still owed. Nothing in Phase 8 blocks users — it is the evidence
obligation the feature carries, and it is the largest single body of unbuilt work
in the repo.

Central artifact for all of T078–T090:
`docs/reviews/FEATURE-020-V11-RELEASE-GATE.md` — **does not exist**.

### The switch that arms Phase 8's release gates

**`.github/release-evidence-requirements-v11.json`** — found 2026-08-21 while
tracing why two V11 gates never appear in a Release run.

Two steps in `.github/workflows/release.yml` are guarded by
`if: needs.resolve-release-ref.outputs.lifecycle_gate_active == 'true'`:

- *Verify Feature 020 V11 refreeze integrity* (`refreeze_v11.py verify-internal`)
- *Test and verify Feature 020 V11 lifecycle traceability*

`lifecycle_gate_active` is derived from that JSON file, which currently reads:

```json
{ "phase": "pre_activation",
  "required_oracle_receipts": [],
  "required_review_documents": [],
  "required_task_receipts": [] }
```

So both steps **skip on every Release run**, and have since the file was
frozen. The workflow asserts the inverse too — `gate-release-ref` fails with
*"The pre-activation Feature 020 lifecycle gate unexpectedly ran"* if the phase
is `pre_activation` and the gate did not skip. This is intentional and
self-consistent, not a hole.

**Two things a Phase 8 executor needs from this:**

1. **Flipping `phase` to `active` is Track D work.** Once flipped, those two
   Release gates arm, and the three `required_*` arrays stop being empty — they
   become the receipt roster T089/T090 must satisfy. The file is the switch; do
   not treat it as configuration to be tidied.
2. **The skip does not mean those paths are unexercised.** `ci.yml` runs both
   gates **unconditionally** on every PR, with no `if:` guard — the steps named
   *Verify Feature 020 V11 refreeze integrity* and *Test and verify Feature 020
   V11 lifecycle traceability*, inside the `rust` job. Grep the step names; do
   not trust a line number here, they move. They ran on
   every PR that reached `main`. The Release-side skip means only that *that
   run* did not exercise them.

The distinction matters because the two statements read almost identically and
only one is true: *"the gates did not run on that Release run"* is correct;
*"those paths were not exercised"* is not.

There is a real oddity to sit with, though, and it is not a defect: the tree
**activated** in V11 — 11.0.0 shipped, `PreventiveV1Open` is the only live mode
— while the release-evidence phase still says `pre_activation`. That is
correct today, because the phase tracks Phase 8's *evidence* obligation rather
than the runtime's mode, and Phase 8 has not started. It will read as a
contradiction to whoever finds it cold, which is why it is written down here.

Never-materialized code artifacts:

| Path | Owed by | Status |
|---|---|---|
| `tests/model/` — four separate pure proptest command models | T080 | **missing** |
| `formal/v11/` — four TLA+ specs (process ownership, registry identity, source promotion/invalidation, capacity admission) | T080 | **missing** |
| `src/index_lifecycle/loom_tests.rs` — shared production transition kernel through its `cfg(loom)` adapter | T080 | **missing** |
| `benches/observed_refresh_gate_v1.rs` | T068 | exists |
| `tests/delta_full_rebuild_equivalence_v11.rs` | T071 | exists |
| `docs/migrations/v11-index-lifecycle.md` | T073 | exists (B4 scope still open) |

Task shape, condensed — each records exact commands and results into the release-gate doc:

- **T078** fmt + clippy, warnings denied.
- **T079** focused lifecycle/capacity/watcher/snapshot/provenance/embed/migration/activation suites.
- **T080** the proptest models + TLA+ specs + loom adapter above, with assumptions, bounds, fairness, traceability recorded.
- **T081** serial all-target suite, release build, canonical full/compact tool + resource + prompt fixtures, and the **SC-006 token comparator with ≥50 % median reduction as a hard gate**.
- **T082** cold-start race, same-stamp/suppressed-notification, rolling-deadline, observer-handoff, root-replacement campaigns — *with working positive controls*.
- **T083** measured concurrent-project memory coverage (retired query-pinned generations, retained-plus-candidate overlap, snapshot scratch, accumulators).
- **T084** the full provenance/refusal matrix through text, structured, HTTP, cache, CCR, persistence, retrieval — including `OperationContractV1` Cartesian negatives, exact-bijection `SelectedAggregate` cases, cache-confusion negatives, equal-shape nonexistent-vs-unauthorized `InvalidSelection`, `KnowledgeVoiceFilter`-not-consistency proofs, `RankingSnapshot` identity/order algebra, post-lease-only `OutputCoverage::Truncated` round trips, and secret-canary + policy-mismatch campaigns that **report only rule IDs and `file:line`, never values**.
- **T085** same-process activation and restart campaigns seeded with apparently-valid V10 cache records, CCR handles, snapshots, and live legacy writers.
- **T086** generated V11 public-API allowlist, all-cfg graph cover, dependent-crate fixtures, unknown-configuration rejection.
- **T087** secret-safety scan — rule IDs and `file:line` only.
- **T088** freeze the exact release commit/tree and all gate digests, obtain an **independent** adversarial review, resolve every accepted P0/P1/P2.
- **T089** re-run `execution/refreeze_v11.py` with the trusted external approval record, prove the approved refreeze is still the immutable ancestor of the release tree, assemble `target/ci/lifecycle-v11/release-evidence.json`.
- **T090** run exactly:
  ```
  node scripts/validate-lifecycle-oracle-traceability.cjs \
    --require-materialized \
    --evidence target/ci/lifecycle-v11/release-evidence.json
  ```
  Clear-to-land only when every planned Rust case and benchmark is materialized,
  every requirement row plus T078–T089 is green, and every receipt binds that
  same tree.

Note the dependency: **T090 is why Track A matters to Track D**. `--require-materialized`
will not pass while planned cases are unmaterialized, and the slice-0 roster is
part of what it checks.

### Track E — bookkeeping leftovers (minutes, not days)

**Done 2026-08-20** — `specs/028-preventive-activation-cut/tasks.md` T040/T041 are
marked complete (SC-008 re-verified first: `git diff` over the frozen tree is empty
from the approved ancestor through the merge and through `main` today), and the
`FEATURE-020-SLICE4-CAMPAIGN-v11.md` status line now reads CLOSED with the carried
work named.

**Deliberately left open**, and this is a decision rather than an oversight:

| Item | Where | Why untouched |
|---|---|---|
| Trailing "merge the PR" / "run the full gate" checkboxes: 002 T053, 018 T027, 025 T252 | those `tasks.md` files | Ticking them asserts a verification nobody living observed. Those specs are dormant (§3) and their work landed through PRs months ago, but "the gate was run and was green" is a claim about an event, and the reporting invariant (§1.5) does not get suspended for checkboxes. Anyone who can reconstruct the evidence should tick them and cite it; nobody should tick them to tidy the file. |

---

## 3. Dormant — do not start these without asking

These carry unchecked tasks but no live campaign. Last touched (by any commit
under that directory):

| Spec | Last touched | Note |
|---|---|---|
| `specs/011-ccr-output-compression` | 2026-08-17 | touched only by an unrelated session-cache fix; 0 of 41 tasks ever checked |
| `specs/027-answer-identity-disclosure` | 2026-08-07 | docs-only landing |
| `specs/021-admission-coverage-honesty` | 2026-08-06 | 0 of 75 checked |
| `specs/026-serve-snapshot-restore` | 2026-08-04 | delivered via PR, checkboxes never closed |
| `specs/016-perl-parser-hardening` | 2026-07-06 | 0 of 60 checked |
| `specs/015-cbm-capability-ports` | 2026-07-06 | 43 of 149 checked |
| `specs/013-stel-predictor-calibration` | 2026-06-22 | 0 of 53 checked |
| `specs/003`, `004`, `005`, `008`, `009`, `010` | ≤ 2026-06-18 | v8-era, superseded |

`specs/024-optimization-backlog` has no `tasks.md`; it is a backlog document, not
a live roster.

---

## 4. Verification gates — the exact commands

Run all of these before claiming anything is done. Backend changes need every
row; `npm/`-only changes need the last row alone; mixed changes need both.

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo test --no-default-features --features embed --lib -- --test-threads=1
cargo build --release
node scripts/slice0-oracle-artifact.cjs
node scripts/validate-lifecycle-oracle-traceability.cjs
cd npm && npm test
```

CI runs a superset (version sync, conventional-commit title check, release
build + `verify-tools.cjs --bin target/release/symforge`, darwin/musl/embed
cells). `--test-threads=1` is a correctness gate, not a performance choice — do
not remove it. `cargo clippy --all-targets` is deliberately run **without** a
preceding `cargo check`; clippy is a strict superset and running both compiles
the graph twice for one answer.

Do **not** re-add the four-key `Swatinem/rust-cache` configuration. It was tried
and measured slower on four jobs; the measurement and the likely cause (four
distinct shared-keys against a 10 GB repo-wide cap) are recorded in `CLAUDE.md`.

**Clean up after yourself.** `target/` lands beside the checkout and has reached
180 GB on this repo. Run `cargo clean` when you finish a heavy local session.

---

## 5. Suggested order for a fresh agent

1. **Track E** (minutes) — clears stale prose so the next reader is not misled.
2. **Track C** (Slice 5) — sequential next slice, pure deletion, baseline-pinned,
   lowest risk of introducing a defect.
3. **Track A** — the real acceptance debt. Start with the four rows whose named
   owner already shipped (they have the clearest defect statements), and rewrite
   the stale `#[ignore]` prose as you go.
4. **Track B** — small, adjudicated, well-localized; good filler between the
   larger tracks.
5. **Track D** — the largest. T080's models/TLA+/loom are green-field and can be
   built in parallel with anything above; T088–T090 must run last, on one frozen
   tree, and T090 depends on Track A being materialized.

Whatever you pick: RED first, observe the failure, then the minimal green, then
the focused verification. An acceptance spec is not reported executed until its
production seam exists.
