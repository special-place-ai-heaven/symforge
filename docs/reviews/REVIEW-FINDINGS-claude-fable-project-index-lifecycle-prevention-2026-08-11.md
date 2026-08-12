# Review Findings — Project Index Lifecycle Prevention

**Target:** `5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5`  
**Identity verified:** yes  
**Verdict:** BLOCK

Identity evidence: commit exists; tree `2e7eec650a9784b1c8c496747a3cd32ebccae806`;
design blob `ba33a94acdf61e119e618eb1d6fd441805676652`; context blob
`667b66c929b500daf3b4993f078e2717965be339`; design SHA-256
`a8f2e679965fddf8dadcc6d2eb12311de361a57f8c1a25a48e87681b8fdabad2`; diff blob
`1cb38d3f65600d0002da9d5db263682496fd42e8`. All match the packet. Working tree
clean; the packet commit being newer than the target is expected per the packet.

The verdict is BLOCK on one P1 finding (F1). The architecture itself is sound and
the amendment list is substantively complete; F1 is an enumerable lane the design
never names, through which the headline guarantee is falsified as written. The
smallest sufficient correction is one design paragraph plus one Slice 0 oracle.
Findings F2–F5 are P2: required corrections, individually non-blocking.

## 0. Causal claims independently verified in source

Every §2 claim I spot-checked is accurate at the target commit:

- **§2.1 PROVEN** — `bootstrap_project_index` converts `ScoutCapacityError` into
  `Ok(LiveIndex::empty())` with a Degraded freshness status
  (`src/daemon.rs:3521-3536`). The seam type cannot distinguish refusal from a
  verified index.
- **§2.4 PROVEN** — `ensure_project_slot_for_session_with` runs the full load
  outside the map lock and discards a duplicate via `entry.or_insert`
  (`src/daemon.rs:1103-1108`).
- **§2.8 PROVEN** — `effective_fence_generation`'s doc comment asserts reload
  publishes the new root before bumping the generation
  (`src/watcher/mod.rs:248-254`); the store does the opposite:
  `project_generation.fetch_add` at `src/live_index/store.rs:2409` precedes
  `swap_and_publish` at `store.rs:2414`. The described stale-root adoption window
  is real, and the store's under-lock re-check compares against the same adopted
  generation, so it passes rather than rejects.
- **§2.10 PROVEN** — `reload_with` calls `abort_watcher_task` (`daemon.rs:3338`)
  before `reload_index(...)?` (`daemon.rs:3340`); on failure the `?` returns
  before `start_project_watcher` (`daemon.rs:3344`). The recovery observer is
  gone.

## Findings (ordered by severity)

### F1 — Raw-disk content lane serves bytes outside the verified generation under a strict lease

- **Severity:** P1 · **Classification:** PROVEN (mechanism exists and is
  unaddressed; the design's migration list does not route it)
- **Design location:** §1 target guarantee; §5 Invariant 12; §9 default policy;
  Slice 3 consumer list ("MCP, HTTP, resource, prompt, hook, cache, checkpoint,
  and health") — the raw-disk read gate is absent from all of them.
- **Source location:** `src/protocol/read_gate.rs:92-146`. `admit_disk_read` is
  the documented single gate for every lane that reopens a repository file from
  disk (`get_file_content` raw path, `search_text` untracked sweep,
  `diff_symbols` uncommitted mode, `detect_impact` WORKTREE seeding via
  `admit_worktree_text`). It performs `std::fs::read(canon_path)` and classifies
  the bytes for *security* admission only. There is no byte-identity check
  against the publication that produced the caller's verdict; its own comment
  concedes "a clean manifest cannot authorize bytes that changed after it was
  published" — and then discloses those changed bytes anyway if they pass the
  security classifier.
- **Violated invariant:** the target guarantee — "a query receives one verified
  generation, … or a refusal. Candidate and partial generations are never
  queryable" — and Invariant 12: "Content, generation, availability, and work
  health come from the runtime snapshot." The live filesystem is a third content
  authority the state model does not name. The design's staleness argument
  ("'Current' is not filesystem-linearizable") covers serving *old generation
  bytes* while disk is newer. This lane does the inverse: it serves *newer disk
  bytes* attributed to an old generation's lease, mixed in one response with
  that generation's manifest dispositions and structure.
- **Concrete trace:**
  1. Source is `Idle`, active generation G. External editor (no SymForge intent)
     writes file F. The watcher notification is delayed — a window the design
     explicitly accepts (§7: "A filesystem event may be delivered after a query
     acquires its lease").
  2. A strict query acquires its lease: runtime snapshot shows Idle + G. Nothing
     refuses; `Dirty` has not published because no event was delivered.
  3. The tool lane reaches `admit_disk_read` for F and returns the post-write
     bytes, rendered under a response envelope that "names the captured source
     version and generation" — G, which never contained those bytes.
  4. Structure (symbols, dispositions, ranges) in the same response comes from
     G. The response is a hybrid of generation G and un-observed disk state,
     labeled as verified G. This is precisely the class the design exists to
     prevent, reachable with zero component failures.
- **Smallest sufficient design correction:** add to §5.1/Invariant 12 and the
  Slice 3 routing list: under a strict lease, a disk-backed content read for a
  path present in the lease generation's manifest must either (a) serve the
  generation-owned bytes, or (b) validate the read bytes against the
  generation's admitted byte identity (§5.1 item 2 already requires those
  identities exist) and refuse with an explicit "changed since generation
  capture" verdict on mismatch. Paths absent from the manifest keep today's
  behavior but must be labeled as outside the generation. Name
  `src/protocol/read_gate.rs` explicitly in Slice 3.
- **Regression check that fails before the correction:** deterministic failpoint
  pausing a strict content read between lease acquisition and
  `admit_disk_read`, while a test writer replaces the file's bytes. Assert the
  response is a refusal or byte-identical to generation content; today it
  returns the fresh disk bytes under the old generation label.

### F2 — Stamp-parity promotion barrier cannot detect a same-stamp content change with a dropped notification; the staleness is then permanent

- **Severity:** P2 · **Classification:** PROVEN (by the design's own model
  assumptions; requires an OS-dropped event plus a stamp collision)
- **Design location:** §7 — "repeat authoritative metadata scout and require
  candidate manifest parity"; "stable per-file reads alone cannot prove one
  repository snapshot"; "periodic authoritative reconciliation remains
  mandatory". Invariants 7–8, Invariant 11 (snapshot-seed verification).
- **Violated invariant:** Invariant 7 (promotion requires a complete verified
  scope) and the meaning of "verified": the generation certifies completeness
  for bytes that were never observed.
- **Concrete trace:**
  1. Scout observes F at (mtime t, size s); candidate reads F's bytes.
  2. An external writer rewrites F with same size within the platform's stamp
     granularity of t.
  3. The OS drops the notification without an overflow signal — the exact
     possibility the design invokes to make periodic reconciliation mandatory.
     No journal record exists, so no gap is detected and no replay covers F.
  4. The final authoritative *metadata* scout finds (t, s) — manifest parity
     holds. Promotion certifies a complete strict scope containing pre-write
     bytes for F.
  5. Mandated periodic reconciliation also compares stamps; they match forever.
     The stale "verified" generation is undetectable by every mechanism the
     design specifies. Snapshot-seed verification (Invariant 11) has the same
     hole for offline edits if it is stamp-based.
- **Smallest sufficient design correction:** adopt the git racy-clean
  discipline: the promotion-barrier scout re-hashes any file whose stamp falls
  within the stamp-granularity window of the candidate's read of that file, and
  periodic reconciliation verifies admitted byte identities (hashes), not stamps
  alone, on a bounded rolling schedule. State the same requirement for
  snapshot-seed verification.
- **Regression check:** fixture writes same-size content with a pinned mtime
  while the watcher event is suppressed; assert the candidate cannot promote a
  completeness certificate over the unobserved bytes.

### F3 — Lock order is defined for one source pair only; project-level operations over multiple journals are unspecified and can deadlock

- **Severity:** P2 · **Classification:** LIKELY
- **Design location:** §7 lock-order paragraph ("`SourceJournal` first, then the
  `ProjectSlot` publication writer. No code may acquire them in reverse
  order."); Invariant 13 lists stop/rebind and mode switch in the linearization
  domain but gives no acquisition rule for them.
- **Violated invariant:** Invariant 13's single fixed lock order, which as
  written cannot be obeyed by an operation that must touch the slot and more
  than zero journals it does not already hold.
- **Concrete trace:** close/rebind must revoke publication authority (ProjectSlot
  writer) and quiesce/invalidate every source journal. If close acquires the
  slot writer first and then any journal, it reverses the normative order
  against a concurrent append (journal → slot): append on source B holds
  journal(B) and waits for the slot writer; close holds the slot writer and
  waits for journal(B). Classic cycle. Alternatively, an implementer who
  acquires all journals first must pick an ordering among journals — which the
  design does not define — and hold N locks across the revocation.
- **Smallest sufficient design correction:** two sentences in §7: (1) no thread
  may acquire any `SourceJournal` lock while holding the `ProjectSlot`
  publication writer; (2) project-level operations publish revocation intent via
  one slot-writer publication (making stale tokens invalid by epoch), then visit
  journals independently, in canonical `SourceId` order when more than one must
  be held.
- **Regression check:** Loom test — close/rebind versus concurrent appends on
  two sources; the model as literally written admits the cycle.

### F4 — Unbounded query-lease pinning plus one process-wide FIFO admits cross-project starvation that no state names

- **Severity:** P2 · **Classification:** LIKELY
- **Design location:** §8 (single monotonic FIFO; "A verified generation retains
  its persistent charge … until the last query, cache, or base reference
  drops"; "Cancellation never releases live blocking work"); §10 failure table
  has no row for it; §13 liveness is conditional on query-drop but nothing
  bounds a lease.
- **Violated invariant:** the honesty goal of §10/Q14: a stall scenario that is
  neither self-healing nor surfaced as `Blocked { operator_action }`.
- **Concrete trace:** a hung or leaked consumer holds a query lease pinning a
  retired generation of large project A. A's replacement candidate sits at the
  FIFO head waiting for capacity that can only come from the pinned residency.
  Every cold open of every other project queues behind the head — the design
  forbids bypass ("later smaller requests do not bypass the head"). All sources
  report honest-looking `WaitingForCapacity`/`Building` forever; no state
  transitions to `Blocked`, no operator action is named, and the stall is
  process-wide, not per-project.
- **Smallest sufficient design correction:** either bound lease lifetime, or
  (less intrusive) make admission classify a head whose wait is fully explained
  by pinned-retired residency and surface `Blocked { operator_action:
  release/lease-age evidence }` after the retry budget, while allowing
  independent-capacity-class requests to proceed. Add the row to §10 and lease
  age to the health projection.
- **Regression check:** proptest command sequence with one never-dropped
  query pin; assert every other project's open eventually schedules or a
  `Blocked` state naming the pin is published. The current model fails the
  disjunction.

### F5 — Slice 4 ships availability collapse for the primary edit→query workflow; the measuring gate arrives one slice too late

- **Severity:** P2 (feasibility; partially disclosed) · **Classification:**
  LIKELY
- **Design location:** §9 ("Any delivered source-affecting event atomically
  makes the source runtime `Dirty`; every existing public index-dependent read
  refuses until a complete replacement promotes"); Slice 4 ("events atomically
  mark `Dirty` and coalesce one full rebuild"); the sustained watcher-burst
  latency gate is listed under release gates and Slice 5.
- **Issue:** between Slice 4 and Slice 5, every single file edit costs a full
  isolated rebuild during which all public reads refuse. SymForge's dominant
  workload is an agent alternating edits and symbol queries. On repos where a
  full rebuild takes tens of seconds, strict-current SymForge is refused for a
  large fraction of wall time — a product regression the design's policy
  discloses in principle but never quantifies or gates. The refusal policy and
  the delta optimization are separated by a slice boundary with no measured
  criterion between them.
- **Smallest sufficient design correction:** attach the burst latency/refusal-
  window gate to *enabling* strict watcher-event refusal (end of Slice 4), not
  to Slice 5; or land Slice 4's watcher lane and Slice 5's delta candidates as
  one enabling unit. State a maximum acceptable p95 edit→queryable window for
  the calibration repos.
- **Regression check:** the existing calibration harness measuring edit→
  queryable p95 before strict refusal becomes default-on.

## P3 notes (optional, non-blocking)

- **Spec 020 vacuous-scenario cleanup.** Under the strict lifecycle, "degraded
  coverage" arms of surviving scenarios (e.g. the search no-match tri-state at
  `spec.md:187`, walker degraded-coverage language at `spec.md:151`) become
  unreachable rather than wrong. Amend wording for coherence so future readers
  don't reintroduce the state to satisfy a dead test arm.
- **`Degraded` spelling inversion.** Keeping the public exhaustive variant while
  changing its meaning from "queryable-but-stale" to "refusing" is
  source-compatible but behavior-breaking for consumers that matched on it.
  Needs an explicit versioned release note; consider a deprecation doc-comment
  in the same change.
- **Slice 3 must replace, not parallel, the sequential multi-ArcSwap
  publication.** The single runtime-snapshot swap has to become the only
  publication before Slice 4; the Slice 0 hybrid-read positive control should be
  the explicit gate that Slice 3 turns green, otherwise a partially migrated
  handler can still mix authorities within one response during the migration
  window.

## Answers to the required adversarial questions

1. **Prevention or relabeling?** Genuine prevention. The active pointer is
   immutable under failure, no `Degraded` generation exists, `Blocked`/`Dirty`
   change availability rather than content, and `Unavailable` evidence is
   constrained to versioned contract members treated as no-evidence. The one
   lane where untrusted bytes still flow as Current is F1.
2. **Bypass paths?** F1 (`read_gate.rs` disk lane) is a real bypass in the
   migration enumeration. The embed facade is honestly carved out, not a silent
   bypass. Content-addressed memoization as constrained (exact bytes + policy
   versions as keys) leaks at most timing, not content or candidate existence.
   Snapshot restore, checkpoint, hooks, sidecar, resources, prompts are all
   named and routed.
3. **Write vs strict query?** SymForge-owned writes are closed by the pre-write
   `Dirty` publication under the journal lock; rollback-to-`Idle` requires the
   full revalidation proof — sound. External writes in the watcher-delay window
   are honestly modeled for generation-served answers (response names captured
   version); the exception is again F1's disk lane.
4. **Journal loss?** Registration precedes S0, S0 precedes the scout, so no
   pre-scout blind spot exists by construction. Promotion's tail==W under both
   locks means every record is replayed-before or retained-after. Loss requires
   OS non-delivery, which the design routes to mandatory reconciliation —
   defeated only by the F2 stamp collision.
5. **Lock order sufficient/deadlock-free?** Sufficient and cycle-free for a
   single source's append/Dirty/promotion/query. Unspecified for project-level
   operations over multiple journals (F3).
6. **Binding token?** Yes. The never-reused slot instance ID kills close/reopen
   ABA; binding/observer epochs kill the §2.8 split-brain (token validated as
   one value under the publication writer); open-handle physical-root identity
   with conservative failure covers same-path replacement, including the NTFS
   file-ID caveat, because the held handle pins the directory object.
7. **Contract closed? Amendments complete?** Closed: every member is Complete or
   no promotion; terminal policy exclusions are accounted dispositions, which is
   truth-preserving; `Unavailable` cannot become evidence of absence. FR-022,
   FR-031, FR-039, the plan phases, and SC-019 are all explicitly amended
   (§3 items 9, 10, 11, 12); NFR-003's rewrite is strictly stronger. Remaining
   spec language is vacuous, not contradicted (P3 note).
8. **Frecency instant mixing?** Prevented as specified: one SQLite
   snapshot/transaction, one session-version copy, one captured evaluation
   time, capture ordered after the runtime-snapshot load, no lock overlap with
   journal/publication, ordering-only authority. The "no reopening after
   capture" rule is the load-bearing sentence and is present.
9. **Capacity closed?** The enumeration covers every residency class I could
   name, including parser high-water, retired-pinned, operational snapshots,
   and snapshot triple-form coexistence. No hold-and-wait plus full-charge
   atomic reservation excludes admission deadlock. The gap is liveness, not
   accounting: F4 starvation.
10. **Failed reload / tombstones?** Yes: the observer survives until successor
    handoff (fixes the proven §2.10 defect); the narrowed tombstone lifetime is
    safe because retired generations are immutable and publication-incapable by
    construction, holding only independently-owned capacity.
11. **Migration double-authority?** The sequence is coherent: Slice 2 lands
    reservations before any active-plus-candidate path exists; Slice 3 forbids
    a second active pointer and migrates all consumers before Slice 4 makes
    candidates reachable; Slice 4 removes the legacy in-place lane in the same
    step. The single risk is the Slice 3 migration window (P3 note).
12. **Deep modules or God coordinator?** Three genuinely deep modules with
    local ownership; each passes the deletion test. I could not name a smaller
    interface that preserves the invariants — journal, promotion, and lease
    must be co-owned or the linearization domain (Invariant 13) fragments, and
    that is the whole defect being fixed.
13. **Compatibility projection honest?** Substantively yes: enums stay
    exhaustive, HTTP 503 matches the released sidecar decision, protected-root
    membership precedes join (Invariant 15), quarantine and deletion
    convergence are preserved by §3's closing list. The `Degraded` behavior
    inversion needs disclosure (P3).
14. **Liveness honesty?** Two stalls found: F4 (undisclosed, process-wide,
    no operator action named) and F5 (disclosed policy, unquantified cost,
    gate attached one slice too late). Retry-exhaustion→`Blocked` and
    never-stabilizing sources are honestly stated.
15. **Concrete failing interleaving?** Two given with traces: F1 (external
    write in the watcher-delay window + raw-disk content read under a valid
    strict lease → hybrid response labeled as verified) and F2 (same-stamp
    rewrite + dropped notification → permanently certified-stale generation).
    Every other interleaving I tried — stale root-A mutation, dual first
    opens, promotion vs watermark event, close/reopen vs surviving loader,
    rollback vs external event, operational-snapshot capture vs frecency bump
    — is excluded by the token/lease/linearization model as specified.

## Verdict rationale

F1 falsifies the headline guarantee through a lane the design never names, with
zero component failures required — that meets "weakens a load-bearing safety
property without disclosure." It is also cheap to fix: one design paragraph, one
named file in Slice 3, one Slice 0 failpoint oracle. With F1 amended (and F2–F5
recorded as required corrections in their slices), this design should pass
re-review: the causal analysis is accurate against source, the module boundaries
are genuinely deep, the amendment list is complete, and the promotion/lease model
excluded every other adversarial interleaving I constructed.
