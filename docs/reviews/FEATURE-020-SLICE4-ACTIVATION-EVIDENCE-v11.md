# Feature 020 Slice 4 — Activation Campaign Evidence (v11)

**Scope**: the campaign half of frozen 020:T072 (execution spec 028, T037),
executed 2026-08-20 on branch `feature-020-slice-4-activation`. The review
half — T038's multi-round adversarial review, including the cfg-lens sweep —
is OPEN; this document is its input and is closed only by its recorded
rounds. Nothing here is a merge authorization: T040's SC-008 check and the
explicit operator approval gate (FR-015) stand between this evidence and any
merge.

Authority chain: frozen spec tree `specs/020-repository-knowledge-index/`
(immutable input) → execution spec `specs/028-preventive-activation-cut/`
(`spec.md`, `tasks.md`, and the amended-as-executed
`activation-cut-execution-map.md`, which is the detailed per-commit record
this summary cites rather than repeats).

## 1. Commit lineage (executed, pushed)

| Group | Commit | Landed |
|---|---|---|
| Wave-2 map | `604f6053` | execution map for the cut |
| C1 (fixture doors) | `5105053d` | activation precondition discharged on fixture doors |
| C2 (T066 mode machine) | `bebdb8f8` | activation mode machine, dark |
| C2b (T064 writer lane) | `8cf5e754`, `f393d0a3` | permits gate edit-tool writes; hygiene lanes take the permit |
| C3/C3b (T029 observation lane) | `39f4d3dc`, `c80e162c` | watcher + facade admissions drive the candidate pipeline; policy permit lane; callbacks census closed |
| C4a (T030 roots) | `66faf7e2` | `ProjectRuntimeHandle` owns every publication root |
| C4b (T030 bootstrap) | `6e8ea006` | bootstrap flows through the activation machine and process registry |
| C4c (T030 sweeps + neck) | `fc3f306d`, `a377ee36` | sweeps join the observation lane; daemon neck acquires through its admission slot; `WorktreeCache` fenced per indexed root |
| C5 prep | `27608a88` | binary dispatcher hoisted into `cli::entry` |
| C5 (T031 exposure flip) | `4823ad6a` | V11 public surface, `server_api` wired, raw modules retired |
| C6 (T032/020:T058) | `4af9216e` | the four frozen T058 oracles observe; cold recovery + init writer lane pinned |
| C7 (T033/020:T068) | `4bc9b923` | `ObservedRefreshGateV1` benchmark, receipts, fixtures |
| C8 (T034/020:T069) | `4bd26d7e` | whole-runtime capacity conservation observed |
| C9 (T035/020:T070) | `b57ae5e5` | gate vs baseline `1521abb0`: all gates pass (see §5) |
| C9 addendum | `8607a5df` | repeat-cache publication-identity fence proven; C9 mis-attribution corrected |
| C10 (T036/020:T071) | `3d6ff54e` | delta/full-rebuild equivalence for every advertised edit class |
| C11a (T037 fences) | `3d620cf1` | CCR publication-identity + replay-authority fences, RED-first |
| C11b (T037 observation residuals) | `dce1cddc` | D14 falsifiable (RED-first), cancelled `index_folder` resolution, D16 adjudication |

## 2. The campaign oracle

```
cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact
```

Observed 2026-08-20 at `dce1cddc`: `1 passed; 0 failed` (exit 0). The oracle
is TEST-SURFACE (020:T050), name-pinned by
`contracts/lifecycle-oracle-traceability-v11.md`: it closes the three-way
surface split over the 116 slots (102 `SURFACE_OVERLAY` rows, 3
`NON_INGRESS_EXCEPTIONS`, 11 `AUTHORITY_FREE_INGRESS`) against the frozen
inventory's categories, owner tasks, and production seams. Per the Slice 3
caveat that still applies: it proves the partition closes, not that any
individual member's set is exactly right — per-member correctness rests on
the cited bases and on T038's review.

## 3. Gate battery observed at `dce1cddc`

All via Terminal Commander (`outcome_trust: observed`) on 2026-08-20, except
fmt/validator (foreground, output verbatim):

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — exit 0 on final bytes
  ("Checking symforge" observed; not a cache-instant pass).
- Dark seal `preventive_runtime_dark_v11` — 9/9. FULL source pin refreshed
  from oracle actuals after fmt; the EXCLUDED-runtime pin was UNCHANGED
  through all of C10–C11b, as predicted for edits outside the 13 sealed
  sources.
- Embed cell `cargo test --no-default-features --features embed --lib --
  --test-threads=1` — exactly 1336 passed, exit 0.
- Full serial suite `cargo test --lib --bins --tests -- --test-threads=1` —
  exit 0 (723 s).
- Bench smoke `cargo bench --bench observed_refresh_gate_v1 -- --test` —
  exit 0.
- `node scripts/validate-lifecycle-oracle-traceability.cjs` — OK
  (78 requirements, 24 acceptance oracles, 13 retirement categories),
  postactivation branch.
- Campaign oracle (§2) — 1/1 exact.

## 4. The five carried Slice 3 residual families — dispositions

1. **Repeat-cache / CCR publication-identity fence.** Read-tools half:
   `session_cache_hit.rs::stale_publication_never_satisfies_the_repeat_read_cache`
   (C9 addendum `8607a5df`). CCR half (C11a `3d620cf1`, RED observed —
   identical bytes under a moved publication collided onto one handle):
   `CcrPublicationIdentity` is an input to handle minting and stored on the
   blob; foreign-source blobs refuse typed; superseded renderings are served
   with an explicit "CCR replay: rendering bound to content generation N"
   label; evicted/unknown handles keep typed unavailability. All three
   frozen `ccr` category assertions hold; the retained disposition is the
   frozen one: a generation-bound rendering cache downstream of the query
   lease, never fresh authority.
2. **Replay-authority forbidden shortcut.** C11a (RED observed — a stored
   edit success replayed byte-identical after an external overwrite):
   `ReplayRecord` carries a post-image receipt (absolute written paths +
   content digests, read back at the single completion funnel; the batch
   executors return their written paths). A stored result replays only
   while every receipt target still holds its bytes; v1 records and failed
   records never replay through the verified lanes; unverified completed
   records are superseded at begin. The `index_folder`/daemon control-state
   replay lanes are deliberately untouched (state-dir idempotency
   contracts, not source-byte claims) — recorded for T038.
3. **D14 live-observer invalidation.** C11b (RED observed AND real): a read
   of a never-indexed missing path published — the fenced-removal seam
   swapped-and-published unconditionally, in three faces (missing-path
   read, source-scope eviction of never-held paths, embed facade observing
   removals that never happened). Fixed with the typed `FencedRemoval`
   outcome; `NothingHeld` publishes nothing and observes nothing. The
   falsifiable oracle
   (`a_failed_read_observation_preserves_the_fence_the_observer_moves`)
   contrasts the same live fence moving under the observer lane and
   holding across failed observations, before and after movement. The
   model-level Slice 3 test stays; its unfalsifiability is repaired by the
   live contrast, not by deletion.
4. **Cancelled non-abortable `index_folder`.** C11b, green on first run:
   `a_cancelled_activation_never_governs_until_an_observed_resync`
   (`src/daemon.rs`) proves the frozen 028 edge case — an unobserved
   daemon-side activation never governs the adapter's unqualified reads
   (body and receipt agree on the last OBSERVED project; a control proves
   the daemon ACTIVE really moved), and the caller's observed retry is the
   authoritative ACTIVE re-sync after which both sides converge. No
   half-published root is observable at any point.
5. **D16 cross-process publication atomicity.** C11b, adjudicated:
   the structured boundary is per-response — the typed evidence receipt
   names the exact immutable publication that rendered the body (not the
   pre-dispatch seed; not a publication landing between render and attach),
   and the wire `_meta` round-trips as typed `ProjectEvidence`
   (`the_evidence_receipt_names_the_publication_that_rendered_the_body`).
   Product-wide cross-process atomicity under arbitrary concurrent
   publication is deliberately NOT claimed, matching the Slice 3 record.

## 5. Adjudications and recorded residuals on T038's roster

- **Sub-millisecond 2.00× bench lanes (C9)**: two lanes (embed commit,
  freshen-on-read) quantize 1→2 ms against a ratio gate below the
  measurement quantum; adjudicated PASS with the deviation recorded openly
  in `docs/reviews/OBSERVED-REFRESH-GATE-v1.md`. T038 must challenge the
  adjudication.
- **Retarget-in-place admission identity (C4b)**: a root physically
  replaced at the same path keeps its admission identity until the C5
  transitions own rebinding.
- **Serve path without `RootBinding` (C4b)**: the serve path surfaces no
  `RootBinding` and presents `NormalProject`; its loader resolves protected
  roots upstream.
- **`project_source_authority` static (C4b/C5)**: remains the per-root
  convergence lookup after the flip.
- **Replay control-state lanes untouched (C11a)** and **D16 non-claim
  (C11b)**: both are explicit scope boundaries, not oversights; T038
  confirms or challenges each.
- **Embed facade no-op contract (C11b)**: `remove_file`'s frozen "a no-op
  removal is applied" bool is preserved while only actual removals drive
  the observation lane — the contract collision and its resolution are in
  the map's C11b record.

## 6. What this green does not prove

Everything in the Slice 3 evidence's "what green does not prove" that is
not explicitly discharged above still applies. In particular: the campaign
oracle proves partition closure, not per-member set correctness (§2); the
darkness seal proves reviewed-baseline preservation, not compiler-semantic
absence; the D16 adjudication claims a per-response boundary, not
distributed atomicity; and no live acceptance beyond the quickstart
spot-checks recorded in the map has been performed by this document.

## 7. Open obligations before merge

- **T038**: multi-round adversarial review (cfg-lens sweep included),
  findings fixed RED-first or explicitly adjudicated; its rounds append to
  this document.
- **T039**: `docs/migrations/v11-index-lifecycle.md` + the V10→V11 embedder
  guide.
- **T040**: SC-008 check and the explicit operator approval — the merge
  gate. Merging to `main` triggers release-please to mint **11.0.0**.
- **T041**: agentmemory `[symforge]` saves and `cargo clean` in the
  campaign worktree.
