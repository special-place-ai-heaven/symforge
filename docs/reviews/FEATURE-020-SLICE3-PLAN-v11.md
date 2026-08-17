# Slice 3 plan — behavior-neutral seams, provenance, and a dark runtime

Slice 3 is T041–T052. Production stays on V10 authority for the whole slice. The
deliverable is typed seams that Slice 4 can activate in one cut, plus a runtime that
compiles, is tested, and is provably unreachable from every production entry point.
The post-Round-3 PR 4 candidate remains an in-progress corrective change, not slice
closure. Its bounded production scope covers defects exposed by the exact T050/T051
audit in already-shipped paths: the schema-hidden A-019 relay; compact-facade and
daemon project routing; `index_folder` root/ACTIVE bookkeeping; whole-call `ask`
routing; and the local impact result's publication/evidence binding. The relay was
production-reachable and source-write-capable behind a read-only facade, while the
routing paths could mix HOME, ACTIVE, or an explicitly selected sibling and the
impact path could describe a different publication from its rendered result. The
repair narrows or binds those existing paths, hardens their existing harness, and
does not activate V11 authority.

## PROGRESS LEDGER — durable, does not depend on session state

| Task | State |
|---|---|
| T041 RED provenance/contract negatives | landed (PR 1) |
| T042 RED cross-authority | landed (PR 1), except approved D14 live-observer proof deferral to T056/T063 |
| T043 sealed provenance types | landed (PR 2) |
| T044 read_gate authority split | landed (PR 2) |
| T045 protocol lane migration | landed (PR 2) |
| T046 published-source-set consolidation | landed (PR 2) |
| T047 dark runtime | landed (PR 3, squashed as 6d1c58df) |
| T048 dark public API + export delta | landed (PR 3, squashed as 6d1c58df) |
| T049 AAP migration receipt | landed (PR 3, squashed as 6d1c58df) |
| T050 activation-cut matrix | historically landed; reopened by Round 3, current exact 116-slot overlay plus 16 source-free mode residuals have their focused controls, adversarial mutations, and final gates observed and await the non-closure commit and fresh review |
| T051 dark unreachability | historically landed (PR 3); reopened by Round 3, layered lexical + narrow-call-surface + whole-source guard has its frozen source pin, observed macro/alias mutation, and final gates observed and awaits the non-closure commit and fresh review |
| T052 gates + adversarial review | in progress (PR 4, unpushed) — A-019 relay containment, compact-facade selector containment, T050/T051 repairs, harness hardening, required mutations, and candidate gates are implemented and observed; the non-closure commit and fresh review remain before closure |

Entry state, verified at `10e036b5` (not inherited):

- `python execution/refreeze_v11.py verify-internal --target-ref HEAD` — passed.
- `node scripts/validate-lifecycle-oracle-traceability.cjs` — OK, 78 requirements,
  24 acceptance oracles, 13 retirement categories.
- Slices 0–2 are on `main`; Slice 2 landed as `6c3794f3` with all eight PR checks green.
- External approval sequences 1–3 are signed and archived; sequence 4 is an UNSIGNED
  DRAFT bound to `95f24cb1`, a commit the squash-merge replaced. `verify-approval` is
  invoked only by `.github/workflows/release.yml` (T089), so this is a release-closure
  item, not a Slice 2→3 gap. The draft gets re-targeted at the release commit;
  `scripts/prepare-refreeze-approval.py` replaces an unsigned draft in place and keeps
  its chain position, so no orphan signature exists.

## Reality-fit findings from the investigation

These change how the slice is built. Each is a measurement, not a reading of intent.

**1. The 64 V11 atoms must NOT be exported in this slice, and the contract agrees.**
`contracts/public-api-v11.json` lists 64 `introduced_v11_atoms`, every one under
`symforge::embed::`. Its `observed_graph.status` is `pre_activation_required` and
`crate.identity_status` is likewise `pre_activation_required`. The
`activation_rule` ("missing and extra atoms both refuse activation") therefore binds
at ACTIVATION, not now. T048's phrase "generate the exact future export delta" means
compute and record the delta — not apply it. The live `src/lib.rs` census stays byte-
frozen for the whole preactivation period. This is the same constraint Slice 2 hit,
where a top-level `pub mod index_lifecycle;` would have widened the census and the
`#[path]` re-anchor avoided it; the same discipline applies here.

**2. The compile-fail harness already exists and is Python-driven, not `trybuild`.**
`tests/fixtures/public-api-v11-consumer/` already has `all-cfg/`, `compile-fail/`
(with `cases.json` and three `.rs.in` templates: `impl_family_absent`, `path_absent`,
`trait_absent`), and `dependent-positive/`, all driven from `execution/refreeze_v11.py`.
There is no `trybuild` dev-dependency and adding one would be redundant. T041's
"compile-fail/private-constructor cases" extend `cases.json` and the templates; they do
not introduce a second harness. `public-api-v11.json` carries 12 `negative_assertions`
to satisfy.

**3. T050 is a 244-member matrix, and the inventory is frozen.**
`contracts/v10-authority-retirement-v11.md` closes 13 categories totalling 244 members
— writers 25, callbacks 14, publication_roots 9, cache 9, ccr 4, snapshot 13, tools 40,
resources 10, prompts 8, sidecar 24, hooks 7, compatibility_aliases 2, raw_embed 79 —
all `executed: false`. Every member needs an exact Slice 4 owner across eight branches
(`GenerationLeased`, `DiskObserved`, `WorktreeScopeObserved`, `GitObserved`,
`RuntimeHealthObserved`, `MutationPermitted`, `StateWriteAuthorized`, `Refused`). The
inventory itself must not be edited to make the matrix pass.

**4. T044 is small, and T046 is much smaller than the task text implies.**
`src/protocol/read_gate.rs` is 178 lines with three functions
(`admit_worktree_text`, `disk_read_would_refuse`, `admit_disk_read`). Splitting
generation-byte resolution from beneath-confined disk observation is a contained
change.

T046 reads as though the captured published source set must be built. It already
exists: `src/live_index/store.rs:1309` holds `published_source_set:
ArcSwap<PublishedSourceSet>`, and `store.rs:6668`
(`published_source_set_is_the_single_atomic_root_for_current_source`) already pins it
as the single atomic root for current source. So T046 is a MIGRATION of the remaining
legacy readers onto an existing atomic root, not a new consolidation.

Note the task names `src/live_index/view.rs`, but the ~10 independent `ArcSwap` fields
that produce sequential reads live in `store.rs` (`live`, `published_source_set`,
`project_state_dir`, `source_exclusions`, `scout_plan`, `freshness_status`,
`published_state`, `published_repo_outline`, `git_temporal`). `view.rs` is a consumer.
The work therefore spans both files; the task's named file is where the consumer-side
change lands, and that is worth stating in the evidence rather than silently widening
scope.

**5. "Dark" has a precedent and a mechanical definition: no production call edge.**
`src/index_lifecycle/mod.rs` states it as "`grep -rn index_lifecycle src/` returns no
hit outside it other than its own declaration in `live_index/mod.rs` — a `#[path]`
attribute and the `pub mod` line it decorates. Neither is a call edge." T051 formalizes
exactly that into an executing test rather than inventing a second notion of darkness.
That same doc comment also records a prior correction where an integration was claimed
that did not exist — the reporting defect this feature exists to prevent. T047/T048
must not repeat it: the module doc states what has no caller, and the test proves it.

**6b. BINDING — one identity counter, shared through a new non-lifecycle module.**

`GenerationIdentity` and its siblings are MACRO-GENERATED by `identity_newtype!`
(`src/index_lifecycle/authority.rs:21-34`), which mints from a process-wide
`NEXT_IDENTITY: AtomicU64`. A plain declaration search finds no `struct
GenerationIdentity` at all, and an earlier census of mine reported it MISSING — which
would have had T043 redeclare it.

T043 must NOT redeclare it. A second `fresh()` on a second counter is TWO IDENTITY
SPACES, which is strictly worse than the Slice 2 name mismatch: identities from the two
spaces would compare unequal while both claiming to be fresh.

It also must not import from `src/index_lifecycle/`. `claim_provenance` lives under
`protocol`, and a `protocol -> index_lifecycle` reference is exactly the call edge
T051's darkness proof forbids — `index_lifecycle/mod.rs` states its darkness as
"`grep -rn index_lifecycle src/` returns no hit outside it".

Resolution: move `identity_newtype!` and `NEXT_IDENTITY` into a small module that is
under NEITHER tree — `src/lifecycle_identity.rs`. `authority.rs` and
`claim_provenance.rs` both use it. One counter, no protocol-to-lifecycle edge.

Three constraints hold at once, each verified rather than assumed:

1. **Census (retirement closure).** `src/lib.rs`, `src/lifecycle_identity.rs`,
   `src/index_lifecycle/authority.rs`, `src/protocol/format.rs` and
   `src/protocol/claim_provenance.rs` are in NONE of the five closure path lists.
2. **Public-API census.** `derivePublicApiAtoms`
   (`validate-lifecycle-oracle-traceability.cjs:1998`) reads ONLY `src/lib.rs`
   `^\s*pub\s+mod\s+NAME\s*;` lines plus `src/embed.rs` items and ITS `pub use crate::`
   re-exports. So the declaration in `lib.rs` is `pub(crate) mod lifecycle_identity;` —
   `pub(crate)` does not match `pub\s+mod`, so no atom is added. A plain `pub mod` WOULD
   add `symforge::lifecycle_identity` and widen the frozen surface. Nothing under
   `src/protocol/` is read by that function at all.
3. **Visibility.** The crate runs `-D warnings`, so a `pub` field typed by a merely
   crate-visible type trips `private_interfaces`. `claim_provenance` therefore
   re-exports the identities it exposes with `pub use crate::lifecycle_identity::...`.
   A `pub use` outside `embed.rs` is not counted by the census.

**6a. BINDING — `DerivedLimitKind` and `LimitBreach` are the LIVE types, not the frozen ones.**

The frozen `data-model.md:1007-1014` declares `DerivedLimitKind` with SIX variants.
`src/live_index/knowledge_bridge.rs:179-189` ships EIGHT: the frozen six plus
`OwnershipSelectors` and `AmbiguousSamples`, both of which production actively records
and tests. `LimitBreach` at `:191-195` is already byte-identical to the frozen shape.

T043 uses the LIVE eight. It does not transcribe the frozen six, because a six-variant
second enum would silently drop two limit kinds that production reports — the exact
reporting defect this feature exists to prevent, rebuilt on purpose.

T043 also does NOT amend the corpus. An amendment to `data-model.md` is documentation
catching up with shipping code; it is not required to write the types, and a drive-by
frozen-corpus edit is the one path this campaign forbids. The staleness is recorded in
the Slice 3 evidence document so a later amendment can add the two names deliberately,
with its own manifest hash and approval chain.

**6. The provenance model is fully frozen and is ~15 types.**
`data-model.md:1683–1899` fixes `DiskObservationReceipt`, `WorktreeScopeObservationReceipt`,
`GitObservationReceipt`, `AtomicAuthority`, `ClaimInput`, `ClaimProvenance`, `Claim<T>`,
`SourceRefusal`, `ClaimContext`/`ClaimContextInput`, `OutputCoverage`, `LimitBreach`,
`DerivedLimitKind`, `WorktreeObservationCut`, `WorktreeScopeCoverage`, `GitResolvedFrom`.
T043 transcribes these names verbatim — Slice 2 lost time to four seam-name mismatches
invented rather than copied, and that must not repeat.

**7. PR 4's post-Round-3 repair has six bounded workstreams, all still candidate
work.** First, the A-019 relay is restricted to source-mutation-safe measurements,
keeps its raw legacy result free of fabricated semantic status, and is pinned at the
real MCP boundary. Second, project routing must bind HOME/ACTIVE/explicit targets
before dispatch: the compact facade cannot substitute adapter-local path, bytes,
cache/fusion/co-change state, or evidence; selector-less ACTIVE tools carry an
adapter-authored private pin; relative `index_folder` paths resolve once; overlapping
activations and reconnect mirror updates share one lane; and `ask` routes as one
selected-project operation after its local no-echo guard. Third, local
`analyze_file_impact` renders, enriches, and records evidence from the exact winning
reindex publication. Fourth, T050 records the exact 102-overlay + 3-non-ingress +
11-authority-free partition and separately pins the 16 successful source-free modes
on otherwise branch-bearing members. Fifth, T051 combines lexical outside-caller
sweeps, the narrow approved call surface, and a fail-closed whole-`src/`
reviewed-baseline seal. Sixth, the verify-tools harness gains non-vacuous content
requirements in both fixture sets. None of these statements is closure evidence
until the final mutations, gates, immutable non-closure commit, and fresh full-range
review complete.

## T045 and T046 MUST regenerate the retirement census. This is designed, not a blocker.

`contracts/v10-authority-retirement-v11.md` carries a `preactivation_closure` with five
live SHA-256 digests over the NORMALIZED RELEASE FORM of named file sets. Slice 3's
edit targets sit inside four of the five:

| category | pinned digest covers | touched by |
|---|---|---|
| `ccr` | `src/protocol/ccr.rs` — that file alone | T045 |
| `cache` | `session.rs`, `knowledge_curation.rs`, `daemon.rs`, `sidecar/mod.rs`, `worktree.rs` | T045 |
| `writers` | `tools.rs`, `edit.rs`, `edit_tools.rs`, `knowledge_curation.rs`, … | T045 |
| `publication_roots` | `store.rs`, `protocol/mod.rs`, `daemon.rs`, `server/mod.rs`, `sidecar/mod.rs` | T046 |
| `callbacks` | `persist.rs`, `edit_hooks.rs`, `git_temporal.rs`, `watcher/mod.rs`, … | T045, if it reaches `knowledge_curation.rs` |

**It is probably five, not four, and that is decided in PR 2's FIRST commit — not at
gate time.** `src/protocol/knowledge_curation.rs` sits in THREE categories at once
(`cache`, `callbacks`, `writers`). If T045's lane walk touches it, `callbacks` moves too
and all five digests regenerate. The lane census answers whether it does; the answer is
recorded before the first line of T045 is written, because discovering a fifth
regeneration while trying to make a red gate green is precisely how a deliberate act
turns into a silent fixup.

The census rule is that ANY change to code a release build compiles moves the digest.
So T045 and T046 necessarily break these. That is expected. The gate
(`scripts/validate-lifecycle-oracle-traceability.cjs:2458` `validateRetirementClosure`)
fails them as `RETIREMENT_CLOSURE_MISMATCH`, and its own comment at :2493 states the
intent: "every slice that legitimately edits a censused file has to regenerate it, and
the gate deliberately refuses to print it so a mismatch cannot be papered over by
copying the number out of the failure."

**The procedure, verified on this tree, not read off the comment:**

```
SYMFORGE_LIFECYCLE_EMIT_CLOSURE=1 node scripts/validate-lifecycle-oracle-traceability.cjs
```

emits `CLOSURE <category> <digest>` for all five WITHOUT relaxing the comparison. Run
at `10e036b5` it emitted values identical to the five pinned digests and still reported
OK, so both the emitter and the current tree are consistent.

Consequences the plan binds:

- Regenerating a closure digest is a DELIBERATE, reviewed act, recorded in the evidence
  document with the before/after digest and the reason the censused file changed. It is
  never a silent fixup to make a red gate green.
- `record.paths` must equal every member-owned `src/` path derived from that category's
  `entries[].members` (:2468). Adding or removing an inventory member changes `paths`
  too, and the inventory itself stays frozen — so T045/T046 must not add members.
- Normalization drops comments and test-only `cfg` items, so T041/T042/T051's test files
  do NOT move any digest by themselves. But that alone does NOT make PR 1 census-clean:
  PR 1 must also carry T043's production module, because the tests cannot compile
  without it. PR 1 is census-clean only because that module is declared from an
  UNCENSUSED parent — see the PR 1 section. Add the `mod` line to
  `src/protocol/mod.rs` and PR 1 moves `publication_roots`.

## PR 2 FIRST-COMMIT DECISION — five digests regenerate, not four

Made here, before any PR 2 code, per the rule that a digest regeneration is a
deliberate act and never a gate-time discovery. Grounds are the two recon
censuses, both verified against code at the time they ran:

**The answer is FIVE.** T046's read-site migration necessarily spills beyond
`store.rs` into `src/daemon.rs`, `src/watcher/mod.rs`, `src/live_index/persist.rs`,
and `src/live_index/git_temporal.rs` — all four inside the `callbacks` closure
path list — because the torn readers live at the call sites, not in the store.
The Tier-1 torn readers alone (`health_for_runtime`, `health_compact_for_runtime`,
`DaemonState::project_health`, the search-tool trust banners) span `tools.rs`
and `daemon.rs`. So `callbacks` moves along with `writers`, `cache`, `ccr`, and
`publication_roots`. All five regenerate ONCE, at the end of PR 2, each with
before/after digests recorded in the evidence document.

Scope resolutions bound with the decision:

1. **T045 is bounded to `src/protocol/` by its own task text**, which resolves
   the one unresolvable lane name: "persistence" means the protocol-side
   persistence surfaces — the embedded cache payload rendering and the session
   read-cache — NOT `src/live_index/persist.rs`, which is index persistence and
   Slice 4 writer-migration territory.
2. **T046's named file is wrong and the correction is recorded**: `tasks.md:923`
   says `src/live_index/view.rs`, which contains zero of the nine `ArcSwap`
   fields and zero reads of them — it is the Feature-012 base+overlay spike.
   The work lands in `store.rs` and the reader call sites. The frozen tasks
   file is NOT edited; this paragraph is the record.
3. **Five of the nine fields are already consolidated** behind
   `published_source_set` on the public read surface. The residual defect is
   HOW MANY TIMES one caller loads the set, so T046 is per-caller
   single-capture, not a field migration.
4. **`published_repo_outline` is write-only dead state** — stored on every
   publish, loaded nowhere. T046 deletes it outright.
5. **`terminal_dispositions()` ordering hazard**: the only lock-free reader of
   the raw `live` field, which `swap_and_publish` stores four lines BEFORE
   `published_source_set` — a caller pairing the two can see new content
   against the old publication. T046 re-roots it on the captured set.
6. **The read-MUTATE-read sites are exclusions, not candidates** — the
   before/after samples enumerated in the recon findings doc stay untouched,
   listed in the evidence with the reason.

## Two risks, both now measured rather than assumed

**RISK-A — cache identity under T045. SETTLED, and it is benign.**
The concern was that a claim's `OperationReceipt` would leak into a cache key and move
cache identity inside a slice that promises no behavior change. Measured: the CCR key
is built at `src/protocol/ccr.rs:225–228` and hashes exactly two things — `tool_name`
and `formatted`, the rendered output. Provenance is not an input. Therefore behavior-
neutrality of the RENDERED OUTPUT is identical to cache-key neutrality: if T045 changes
no byte of any lane's output, no CCR key moves, by construction rather than by hope.

This converts RISK-A into one falsifiable assertion, which T045 must carry as a test:
the CCR key input set remains `(tool_name, formatted)`. If a later task needs
provenance in the key, that is a deliberate amendment with its own evidence, not a
silent absorption.

**RISK-B — T046 changes read tearing, which is an observable behavior change.**
Migrating the remaining legacy readers onto the existing atomic `PublishedSourceSet`
converts a torn multi-read into a consistent one. That is the task's purpose, but it is
observable, and a test that pins today's torn interleaving would break. Breaking it
would be correct — but only if it is FOUND first and named, not discovered at gate
time and quietly adjusted. The affected tests are identified before `store.rs` or
`view.rs` is touched, and each one that changes is listed in the evidence document with
the reason it changed.

## Order of work, and why

The task order in `tasks.md` is already dependency-correct; the only addition is
grouping into four reviewable landings, because Slice 2's unrefuted external criticism
was change size, not correctness.

**PR 1 — provenance core (T041, T042, T043), and it must not move a digest.**
RED first: both test files written and OBSERVED failing before `claim_provenance.rs`
exists. Then the sealed types, verbatim from the data model. Ends green with zero
production call sites migrated.

T041/T042 cannot ship WITHOUT T043. A Rust test that names a type which does not exist
does not compile, so a "RED tests only" PR fails `cargo test --all-targets` and cannot
merge. RED is a LOCAL observation recorded in the evidence document, not an
independently mergeable commit. The three tasks therefore land together.

That creates the trap this PR has to dodge. `src/protocol/claim_provenance.rs` is itself
uncensused, but the obvious declaration — `mod claim_provenance;` in
`src/protocol/mod.rs` — is a release-compiled edit to a file inside the
`publication_roots` path set, so it WOULD move that digest and break this PR's whole
claim to be census-clean.

**Binding decision: the `mod` line does not go in `src/protocol/mod.rs`.** It is declared
from a parent that is in NONE of the five path sets. Verified against the closure on this
tree: `src/protocol/read_gate.rs` — NOT CENSUSED; `src/protocol/format.rs` — NOT
CENSUSED; `src/live_index/mod.rs` — NOT CENSUSED (which is exactly why Slice 2's
`#[path]` re-anchor was free). `read_gate.rs` is the right parent: T044 edits it anyway
and it already owns the read-authority seam.

The file stays at the contract-mandated path `src/protocol/claim_provenance.rs`; only
the declaring parent moves. This is the Slice 2 `#[path]` move again — contract file
location, uncensused parent — and it must be written down IN THE CODE, because the next
agent's instinct will be to "tidy" that `mod` line back into `protocol/mod.rs` and
silently move a frozen digest.

**The exact spelling, resolved by compiling rather than asserted.** An earlier draft of
this plan guessed that `#[path]` inside a non-`mod.rs` file resolves relative to that
file's own module directory (`src/protocol/read_gate/`). That guess was WRONG, and a
throwaway crate mirroring this layout proved it: `#[path = "../claim_provenance.rs"]`
made rustc report

```
error: couldn't read `src\protocol\..\claim_provenance.rs`
```

naming its base directory as `src/protocol/` — the directory CONTAINING `read_gate.rs`,
not `read_gate/`. The spelling that compiles and links is therefore the bare one:

```rust
// src/protocol/read_gate.rs
#[path = "claim_provenance.rs"]
pub(crate) mod claim_provenance;
```

which resolves to `src/protocol/claim_provenance.rs` and exposes the module as
`crate::protocol::read_gate::claim_provenance`. Confirmed compiling and passing a
linkage test in an isolated crate before any symforge file was touched.

**PR 2 — authority split and lane migration (T044, T045, T046).** RISK-A settled
before the first lane moves; RISK-B's affected tests identified before `view.rs` is
touched. Every step keeps V10 output identical, proven by the existing suite, not by
assertion.

**PR 3 — dark runtime and dark API (T047, T048, T049, T051).** New modules
`src/index_lifecycle/runtime.rs` and `src/index_lifecycle/public_api.rs`, reachable
only from a dark factory; T051's unreachability proof lands in the SAME PR as the code
it constrains, so the constraint can never be merged later than the thing it guards.

**PR 4 — activation matrix and corrective closure candidate (T050, T051, T052).**
The exact 244-member authority join and 116-slot surface partition, the separate 16
source-free mode residuals, layered T051 darkness guards, narrow A-019 relay
containment, selected-project/ACTIVE routing containment, exact local-impact
publication binding, and both verify-tools fixture sets move together. After final
mutation evidence and the full local gate set, they land in one explicitly
non-closure commit. A fresh independent review then reads the complete PR 4 range at
that immutable SHA. Findings restart repair, gates, commit, and review; only a
trustworthy clean review permits a later evidence/ledger closure commit.

## Binding constraints for this slice

- **Run the embed gate on the FIRST commit that adds a file.** `cargo test
  --no-default-features --features embed --lib -- --test-threads=1`. Default-feature
  gates cannot catch a feature-gated `cfg` mistake, and this campaign has already paid
  for that lesson once.
- **Long cargo runs go through Terminal Commander**, never the Bash tool — the 600 s
  ceiling kills a cold `--all-targets` mid-write and corrupts `target/`.
- **Do not edit the frozen corpus** to make a check pass. A frozen-clause change is an
  amendment with a manifest hash, an `EXPECTED_AMENDMENT_MAPPINGS` entry, and a
  regenerated digest chain.
- **Every negative test pairs with its accepting case**, and each guard is
  mutation-tested — remove the guard, prove the specific test fails, restore it.
- **No known unapproved gap crosses the Slice 3 → Slice 4 boundary.** The approved
  residuals and their existing owners are explicit; recording them is not claiming
  they are closed:

  - Eleven whole-member authority-free ingresses do not satisfy `INV-SURFACE` as
    written; T066 must exclude them from the invariant or add a branch. The static
    glossary/catalog boundary-metadata part is owned by T066/T067.
  - Sixteen successful source-free modes remain on otherwise branch-bearing rows.
    The hook pass-through is owned by T064/T066/T067/T072; the fifteen tool modes by
    T066/T067/T072.
  - Eight identical-success edit replay modes lack a source-bound typed receipt.
    T058 owns the causal RED, T064 the bound replay receipt, T066 branch
    registration, and T072 activation.
  - D14's stand-in does not prove live-observer invalidation. T056/T063 own the real
    observer/query proof; it is not counted as T042 execution evidence.
  - `detect_impact` currently consumes generation symbols/caller graph while its
    frozen target is pure Git/worktree observation. T064 owns that refactor.
  - Repeat-cache hits and CCR retrieval lack the frozen publication/source identity
    fences. T064/T066/T067 own those activation prerequisites.
  - Static resource project evidence is presently unfenced, and standalone/session
    health/stat routes do not yet share the unconditional target guard. T066/T067 own
    those boundary repairs.
  - Cross-process `ProjectEvidence` is ancillary untyped metadata and is not an
    atomic body/publication transaction under arbitrary concurrent publication.
    That existing D16 boundary remains structured-activation work; PR 4 fixes the
    deterministic local impact mismatch but does not claim product-wide atomicity.
  - Cancellation or timeout after the daemon begins a non-abortable
    `index_folder` leaves an explicitly unknown distributed outcome. Completed calls
    are serialized through the ACTIVE mirror and every project-bound follow-up is
    canonically pinned, so the unknown outcome cannot silently retarget a read or
    write. An activation epoch/authoritative resync is recovery work, not a reason to
    weaken the current pin.

  PR 4 adds neither a ninth authority branch nor a frozen-corpus amendment to hide
  any of these. The T051 guard is deliberately bounded to in-tree production source:
  its sealed excluded implementation set composes with the outside caller/splice
  sweep; generated `OUT_DIR`, proc-macro, and dependency behavior are outside that
  claim. The pre-existing normal-STEL rendered-text status classifier is unchanged,
  is not used by any Feature 020 proof, and remains an approved typed-result follow-up
  outside this slice. Any other known gap must be repaired or separately approved
  before Slice 3 closes.
- **T050 exactness is review-backed, not inferred from green alone.** The executable
  matrix proves vocabulary, closure, membership, and non-empty bases. M63c proves it
  cannot judge the semantic correctness of one row; the exhaustive handler-body
  audit and cited per-row bases supply that proof, and the fresh adversarial review
  must independently accept it.
- **Do not start Slice 4 early.** Slice 4 is the single indivisible activation cut.
- Keep nested parentheses out of commit bodies; release-please silently drops the commit.

## Current PR 4 checkpoint and binding closure sequence

The previous candidate cycle completed its joint non-closure commit and received
three independent external reviews. Their consolidated adjudication returned
**FINDINGS — do not land unchanged**: one confirmed MAJOR (the `ask` handler's
nested tool dispatches dropped the resolved project after an allowed HOME
fallback), two confirmed MINOR defects (the hook-adoption formatter misdiagnosed
all-sidecar-error runs as "no sidecar found"; one stale predicate-name comment),
and one false positive. The repairs were made test-first with RED witnesses and
mutation-sensitivity checks. A Round-2 fresh four-lens review of the committed
repair candidate returned CLEAN on both code lenses and three MINOR
documentation/test-coverage findings, all repaired in the current tree: a third
formatter control pinning the mixed sidecar quadrant, and two evidence-document
corrections. The five new library tests raise the library target from 3,215 to
3,220. The current tree is the repaired candidate, deliberately
**in progress / non-closure**. Every candidate gate below has been re-observed on
the repaired frozen source. Only the new joint non-closure commit and fresh
review remain pending. Historical green rows in the evidence document cannot
satisfy these obligations for changed source or harness bytes.

| Candidate obligation | Required result before the non-closure commit |
|---|---|
| Focused A-019 relay, selector-containment, activation-cut, T051 darkness, and real-MCP controls | OBSERVED — re-observed on the repaired frozen source: selector family 12/0, ask-fallback family 3/0, hook-adoption formatter family 6/0 including the Round-2 mixed-quadrant control, preventive 8/0 with the regenerated pin, runtime-dark 11/0, public API 2/0, overlay exactness 1/0; the nested-pin, formatter-honesty, and Round-2 crafted-conjunct mutations each turned their intended witnesses RED and were byte-exactly restored with hash receipts |
| Formatting and traceability | OBSERVED — final-source `cargo fmt --check`, `git diff --check`, and the 78/24/13 lifecycle traceability census are clean; the census regeneration for the repair moved only the `writers` closure, exactly the category owning the repaired production path |
| Clippy | OBSERVED — final-source `cargo clippy --all-targets -- -D warnings` clean with a directly observed exit-0 receipt |
| Embed configuration | OBSERVED — `cargo test --no-default-features --features embed --lib -- --test-threads=1`: 1,333 passed, 0 failed, 4 ignored, directly observed exit 0 |
| Debug binary + verify-tools fixtures | OBSERVED — current-source debug build clean; `target/debug/symforge.exe` produced 7 PASS / 1 REVIEW / 0 FAIL on the synthetic fixture and 10/1/0 on `verify-tools-real`, both expected unit-mismatch REVIEW cases unchanged |
| Release binary + verify-tools fixtures | OBSERVED — current-source local release build clean; `target/release/symforge.exe` produced the same 7/1/0 synthetic and 10/1/0 real results; this was not deferred to PR CI |
| Full target suite | OBSERVED — `cargo test --all-targets -- --test-threads=1` exited 0 with directly observed trust in 587,665 ms; the main library target reported 3,220 passed / 0 failed / 5 ignored and every integration target completed cleanly |

Once those rows are observed on one stable LF-normalized tree, commit every PR 4
candidate file together with a non-closure subject. Run the fresh adversarial review
against that committed SHA and the complete PR 4 range with no concurrent edits. If
the review finds anything, repair and repeat the mutation/gate/commit/review loop. If
it is trustworthy and clean, update evidence and the progress ledger in a second
commit. Do not push until separately authorized.

## Claims inherited, not verified

- That Slice 4 can actually implement and activate all 244 inventory owners. Slice 3's
  exhaustive source audit determines the intended per-member owner; Slice 4 must prove
  its implementation realizes those owners without mixed authority.
- Grok's five smoke findings against 10.3.0 (tools catalog, path-as-project correction,
  `index_folder` parent refusal, `broken_anchor` vs `exact:symbol`, outline dropped
  under budget) are a separate product pass, not observed by me, and not Slice 3 work.
