# Feature 020 Slice 3 evidence (T041–T052)

## T045 — the lanes and the measured envelope (PR 2)

**Batch one** routed the three disk lanes through `observe_disk_beneath` and
closed D8 by routing `detect_impact`'s base seed through `admit_git_text`,
deleting the tripwire's sentinel allowlist outright. The `writers` drift was
OBSERVED, not assumed: the checker was run after the first `tools.rs` edit and
reported `RETIREMENT_CLOSURE_MISMATCH` for `writers` and for `writers` alone.

**Batch two — the forgeable envelope axis.** `format_search_envelope` collapsed
to the compact `Trust:` banner on `source_authority == "current index"` — a
string equality any caller could satisfy by assertion. Two lanes did exactly
that: the context bundle passed the literal whenever it had not disk-refreshed,
and `what_changed`'s Timestamp arm passed it unconditionally — both collapsing
the envelope while the index could be Verifying or Degraded. The second lane
was found by the COMPILER during the migration, not by the census.

The collapse now rides on `SourceAuthority`, a type honest by construction:
`from_freshness` is the only constructor that can produce a collapsible value
and it takes the index's measured `FreshnessStatus`; `never_collapse` covers
disk-refreshed, composite, and git authorities whose labels are display only.
A lying literal is UNREPRESENTABLE — no constructor accepts a caller-chosen
string and marks it collapsible. Behavior is byte-identical for measured
Current and for every already-loud lane; the sanctioned change is that the two
asserting lanes now go loud with the honest label when freshness is not
Current. Composite labels keep their existing text, including the recorded
wart that they say "current" unconditionally — a text change was not in scope.

Mutation M13 flipped the Degraded arm to collapsible and was caught by
`a_measured_degraded_authority_never_collapses_however_clean_the_rest_is`
alone, then restored. Twelve mutations across the slice: eleven caught by
name, one survivor that forced its oracle, one guard proven structural.

**D16 — `ProjectEvidence` and the structured `_meta` surface stay untyped in
this PR, deliberately.** The MCP `_meta` object already carries an untyped
provenance record with `generation` / `load_source` / `index_state`. Replacing
it with `Claim`/`ClaimProvenance` is a client-visible schema change, not a
read-gate migration, and no frozen atom requires it preactivation. Recorded
here next to D12/D13 as T048/structured-activation work: the competitor is
untyped strings versus the provenance types, and the swap belongs to the
activation surface, not to T045's task-text word "structured".

Gates on the batch-two tree: lib suite 3166 passed 0 failed including the new
envelope oracle; clippy all targets denied warnings clean; embed 1332 passed
0 failed; fmt clean. At the time batch two landed, the checker reported the
expected `writers`-only mismatch; the regeneration has since HAPPENED — the
T046 section's before/after table is the truth, and the pins are clean.

## T046 — per-caller single capture, and the one regeneration (PR 2)

Every approved site now takes ONE `published_generation()` capture at entry and
reads every axis — live rows, freshness, health counts, temporal, outline —
off that capture, which is possible because every accessor already resolves
through the bundle; the defect was per-call re-loading, not field scatter.

Migrated: `health_for_runtime` and `health_compact_for_runtime` (four loads
each → one), daemon `project_health` (freshness now describes the same
publication as the counts beside it), the daemon call-evidence block and
`local_project_evidence` (generation number, load_source, counts, and state
all off `current_generation()`; the atomic counter is no longer a side
channel — including `runtime_status_for`, whose reported project-generation
is now a caller-supplied parameter: the health pair passes its captured
bundle's value, and the two capture-less callers pass the atomic EXPLICITLY,
named at the site), `search_symbols`, `search_text` (handler + renderer share the
caller's capture through a new parameter), `search_files` (13 loads → 1),
`find_references` (11 → 1), `append_impact_footer`, `edit_plan`, and
`analyze_file_impact`, whose capture is taken BEFORE the sidecar await so the
co-change footer describes a publication the impact result actually saw.
`terminal_dispositions` was re-rooted from the raw `live` field onto the
bundle, closing the store-order window where new content could pair with the
old publication. The write-only `published_repo_outline` ArcSwap field was
deleted after re-verifying zero loads on the current HEAD; the accessor and
both its tests read the bundle and keep working.

Left alone, by prior agreement: the read-MUTATE-read publish paths, watcher
reconcile, Tier-3 mutex-held store functions, `what_changed` — same class as
the search tools, recorded as OUT of this PR rather than silently expanded —
and the `scout_plan` / `source_exclusions` / `project_state_dir` ArcSwaps,
which the bundle has no fields for.

Behavior neutrality: the full library suite passed 3166 to 0 with ZERO test
adjustments — the RISK-B worry that tests pinned torn interleavings did not
materialize, and the Slice-0 root-split oracle got strictly stronger and
stayed green.

**The one regeneration — prediction versus measurement.** The PR 2
first-commit decision predicted FIVE categories dirty. Measured at the end:
FOUR moved, `ccr` byte-identical, because CCR was trimmed out of T045 batch
two by review. The regen updates exactly the four that moved:

| category | before | after |
|---|---|---|
| writers | `5137cd7b…3af7dd` | `bafa517a…daeee1` |
| callbacks | `48938137…97e8b22` | `026c548b…fe577b` |
| publication_roots | `e37555ad…61e82d` | `b90b8d88…190b54` |
| cache | `4eb220e8…5c18a38` | `6fb4cace…14fa095` |
| ccr | `8ad77748…84ad246` | UNCHANGED |

The checker's own second-order pin (`FROZEN_DIGESTS.retirement_records`) was
regenerated through its emit opt-in the same way: old `4c118fab…76a6fb`, new
`313dceda…9c21bf`. Checker reports OK after both.

## T044 — the authority choice is explicit (PR 2)

Observed RED first: both oracles failed `E0432` naming exactly the three new
seam items and nothing else. Then the seam, in `src/protocol/read_gate.rs`,
on the policy/bytes/git/disk split #571 carved:

- `resolve_generation_bytes` — serves `IndexedFile.content`, the bytes the
  generation PUBLISHED. **The defect it exists to prevent is structurally
  unrepresentable in it**: the function takes no workspace root, so an
  in-function disk backfill cannot even locate a file, and its return borrows
  from the index, so owned disk bytes cannot be returned without a deliberate
  leak. This is recorded INSTEAD of a mutation for the never-reads-disk
  guard, because the only writable mutant is one whose `fs::read` cannot find
  the fixture and therefore survives for reasons unrelated to the property —
  a theatrical mutant would be evidence-shaped noise. The oracle still pins
  the behavior: published bytes survive a disk rewrite, and an unindexed
  file resolves `NotInGeneration`, never disk content.
- `observe_disk_beneath` — the deliberate lane, lexically confined beneath
  the workspace root, refusing absolute paths, prefixes, and `..` components
  BEFORE any read; the refusal never carries escaped content. Symlink policy
  deliberately remains the crate's existing never-follow walk; the ceiling
  and upgrade path are marked in the code.
- Both re-exported through `claim_provenance` the same way as the identities,
  because `read_gate` is crate-private and the oracles are a separate crate.
  No `protocol/mod.rs` edit; no census atom.

Mutation M12 — confinement disabled — caught by
`a_disk_observation_is_confined_beneath_its_root` alone, restored. Eleven
mutations across the slice so far: ten caught by name, one survivor that
forced a new oracle, plus one guard proven structural rather than mutated.

Gates on the T044 tree: oracle files 36 passed 0 failed; clippy all targets
denied warnings clean; embed 1332 passed 0 failed; fmt clean; traceability
OK; all five closure digests byte-identical — T044 touched only uncensused
files, per the PR 2 first-commit decision.

Living document for the slice; T052 completes it. Every claim here was observed,
not inferred. Where a command is cited, it was run on the named tree.

## T041 + T042 — observed RED (durable record)

The RED observation lives in branch commit `cdb3ff20`, which a squash-merge will
collapse, so the evidence is recorded here as well.

Command, on `cdb3ff20`'s tree (before `claim_provenance.rs` existed):

```
cargo test --test read_gate_authority_v11 --test claim_provenance_v11 --no-run
```

Observed output:

```
error[E0432]: unresolved import `symforge::protocol::format::claim_provenance`
   |                                 ^^^^^^^^^^^^^^^^ could not find `claim_provenance` in `format`
error[E0433]: cannot find `claim_provenance` in `format`   (x4)
error: could not compile `symforge` (test "claim_provenance_v11") due to 5 previous errors
```

Every error names the missing module and nothing else, so the RED was about the
absent types, not a malformed test.

## T043 — GREEN transition and the mutation ledger

After `src/lifecycle_identity.rs`, `src/protocol/claim_provenance.rs`, and the
`#[path]` anchor in `format.rs` landed, the same two files compiled and passed:
initially 22, then 23 after M2 forced a new oracle (below).

**Mutation ledger.** Each guard was flipped in production, the suite run, the
named oracle observed failing ALONE, and the guard restored. A guard whose
mutation survives is not enforced; one did, and the response was a new test, not
a shrug.

| # | Mutation (production change) | Expected catcher | Observed |
|---|---|---|---|
| M0 | `AtomicAuthority::proves_repository_absence` → `true` | `no_local_negative_receipt_can_be_widened_to_repository_absence` | CAUGHT — that test alone failed, message named `DiskObservation` |
| M1 | empty-derivation refusal disabled (`if inputs.is_empty()` → `if false`) | `a_derivation_refuses_an_empty_input_set` | CAUGHT — alone, 11 held |
| M2 | bijection LENGTH check disabled (`false && captured.len() != …`) | — | **SURVIVED. 12 passed.** See below. |
| M2' | same mutant, after the new oracle | `a_selected_aggregate_refuses_an_extra_unselected_generation` | CAUGHT — alone, 12 held |
| M3 | `roots_are_compatible` → always `true` | `a_derivation_across_two_roots_is_refused_rather_than_composed` | CAUGHT — alone, 9 held in its file; other file 13 green |
| M4 | `render_bounded` mints a fresh `ProvenanceIdentity` | `truncated_coverage_never_enters_a_claim_identity` | CAUGHT — alone, 12 held |

Final state after all restores: **23 passed, 0 failed** across both files.

**The M2 survival was a real test gap, not a weak mutant.** The bijection
condition is `len_mismatch || !all_contained`; the mutant disabled only the
length half, and the containment half caught the only fixture the suite had
(missing generation). The length guard alone is what catches an EXTRA captured
generation nobody selected — "Missing, extra, forged, or uncaptured inputs
refuse" (`data-model.md:1893`) — and no test exercised that arm. The new oracle
`a_selected_aggregate_refuses_an_extra_unselected_generation` was written while
the mutant was live, observed catching it, and kept.

## T043 stand-ins that must not be "completed" casually

- **`ObservationLease::completed_render_authority` always returns `Ok`.**
  `OutputCoverage::Truncated` is gated on holding a `CompletedRenderAuthority`;
  in Slice 3 that token is obtainable from any `ObservationLease`, because the
  real strict-lease machinery is Slice 4 (T047/T060). The gate is the TYPE, not
  a runtime check. Do not "complete" this method by adding a fake check that
  pretends to verify lease completion it cannot observe — that is the reporting
  defect this feature exists to prevent. Slice 4 replaces the constructor's
  evidence, not its shape.
- The other lease constructors (`observe_missing_path`, `complete_scope_scan`,
  `admit_generation`) are the same shape: sealed constructors whose *evidence*
  arrives with the real runtime. Their `Result` returns exist so the signatures
  do not change when the evidence does.

## Deliberate decisions in force (recorded before code was written)

- **D3** — `DerivedLimitKind`/`LimitBreach` are the LIVE eight-variant types from
  `live_index::knowledge_bridge`, imported, never transcribed. The frozen six is
  stale; a later corpus amendment may add the two names. Confirmed by the
  compiler: the integration crate imports the live type directly.
- **D9** — where `data-model.md` and `contracts/public-api-v11.json` disagree,
  the ATOMS win (opaque `SourceRefusal` + `SourceRefusalKind` + `RetryAdvice`,
  `Claim::producing_runtime_identity`), because the activation rule is
  machine-enforced and the prose is not. Neither document was amended.
- **One identity counter** — `identity_newtype!` and `NEXT_IDENTITY` moved to
  `src/lifecycle_identity.rs` (`pub(crate)` in `lib.rs`, so the public-API
  census gains no atom); `index_lifecycle/authority.rs` re-exports its six
  identities from there. No `protocol → index_lifecycle` call edge exists, so
  T051's darkness proof is intact.
- **The `#[path]` anchor lives in `format.rs`**, not `protocol/mod.rs`
  (censused; also `read_gate` is `pub(crate)` so the oracles — a separate crate —
  could not see the module through it).

## The adversarial audit of the T043 draft, and what it changed

A 5-agent audit ran against the committed draft `225b18bf` — four independent
auditors over seam fidelity, atom coverage, task-text completeness, and embed
cfg, then a synthesizing verdict, each verifying against the frozen corpus
before promoting anything. Every finding below was RE-VERIFIED here before
being acted on.

**Fixed in the follow-up commit, each with its reason:**

- **`OutputCoverage::Truncated` was FORGEABLE while claimed sealed** — a pub
  struct variant, so `Truncated { breaches: vec![] }` compiled anywhere with no
  authority, while doc and commit message claimed the seal. The audit named it
  for what it was: reporting an enforcement the type system did not provide.
  Now `Truncated(TruncationBreaches)` with a private field and no public
  constructor; the ONLY producer is `CompletedRenderAuthority::truncate`.
- **`RetryAdvice` and `OperationKind` violated the module's own atoms-win
  rule.** The contract fixes `RetryAdvice = Automatic | Never | OnEvent |
  Operator` and `OperationKind` as the SEVEN-variant runtime vocabulary; the
  draft invented three retry variants and squatted the OperationKind name with
  four provenance shapes. Both now verbatim from the contract; provenance
  shapes are named by `ClaimProvenance::kind_name` alone.
- **`ObservationLease::refuse` fabricated evidence** — it filled
  `evidence_identity` with a fresh identity corresponding to nothing examined,
  and the oracle blessed it by asserting only `is_some`. The parameter now
  forces the caller to name what it examined, and the Cartesian asserts the
  EXACT identity round-trips.
- **`render_bounded` discarded its coverage argument**, making the retention
  oracle unfalsifiable. Coverage is now retained on the claim, readable via
  `rendered_coverage`, still off provenance identity.
- **`KnowledgeVoice` validated an invented model** — a `Consistency` variant
  that exists in no frozen document, while dropping `Current`, which the
  frozen default selection MUST include. Now the frozen six; "never selects
  consistency" is structural, since no such voice is expressible.
- **`SelectedAggregate` could not name its own evidence** — `authorities()`
  yielded nothing for it while `authority_count()` counted its generations, it
  dropped the frozen `additional_authorities` field, did no root check, and
  `BTreeMap::from_iter` silently collapsed forged duplicate keys.
  `authority_count()` is now literally `authorities().count()`.
- **`into_failed_read` minted a `for_test` receipt on a non-test path**; the
  caller now supplies the operation being served.
- **Identity newtypes had gained `Ord`**, making mint order observable — an
  inference channel added only so a test could sort. Reverted to the original
  derive set; the test uses a `HashSet`.
- **Both oracle files lacked the sibling-convention `#![cfg(feature =
  "server")]`** — invisible to the `--lib` embed gate but a break of the
  documented all-targets embed invocation. Added.
- **The darkness prose in `index_lifecycle/mod.rs` had become false** — it
  claimed grep-level absence, which `lifecycle_identity.rs`'s doc comments now
  violate in prose. Restated as the call-edge property T051 will formalize.

**Mutation ledger, continued.** The three new guards were each flipped,
observed caught BY NAME, and restored — final suite 29 green:

| # | Mutation | Caught by |
|---|---|---|
| M5 | comparison root gate disabled | `a_comparison_across_two_roots_is_refused_rather_than_composed`, alone |
| M6 | duplicate-key forgery guard disabled | `a_selected_aggregate_refuses_a_forged_duplicate_capture` — via the KIND assertion, proving forgery is distinguishable from a selection mismatch |
| M7 | aggregate root check disabled | `a_selected_aggregate_refuses_a_foreign_root_authority`, alone |

**Deferred with records — the D-ledger:**

- **D10 — receipt-field simplifications vs the frozen data model.** The Slice 3
  receipts drop `parent_identity`, `stable_read`/`ByteDigest`, `FileStamp`,
  `policy_versions`, `started_at`/`finished_at`, `manifest_digest` and
  `stable_entry_count` on scope coverage, `repository_id`/`resolved_from`/
  `object` on Git receipts, and use `String` where the model has
  `CatalogPath`/`PhysicalRootIdentity` typed paths. All prose-only — no atom,
  oracle, or seam pins them — and the machinery that makes them load-bearing
  is Slice 4. NOTE: the `String` paths cannot carry non-UTF8 opaque paths,
  which collides with T053's lossless opaque-path oracle; Slice 4 must widen.
- **D9 append** — every `ClaimProvenance` variant carries `identity` per the
  atom `ClaimProvenance::identity`, which the data-model prose lacks.
- **D11 — duplicate `PhysicalRootLease` name.** The provenance fixture
  coexists with the real `index_lifecycle/physical_root.rs` type the data
  model references. The recon census wrongly listed it as nonexistent, which
  caused the duplication. Reconciliation belongs to the Slice 4 wiring that
  connects provenance to the real lease; no enforced check breaks today.
- **D12 — activation-time surface unwind.** The module is mounted at
  `symforge::protocol::format::claim_provenance`, and `OutputCoverage`
  publicly exposes `live_index::LimitBreach` — both forbidden by negative
  assertions AT ACTIVATION, both legal today because `observed_graph.status`
  is `pre_activation_required`. T048's embed boundary must wrap or unwind.
- **D13 — atom accessor shapes are the EMBED boundary's problem.** The
  contract fixes `&str` identity returns, reference returns, `Display` +
  `Error` on `SourceRefusal`, and opaque structs where this module has enums.
  The atoms describe `symforge::embed::*`; T048's re-export layer wraps the
  internal types into contract shapes, and T049's dependent-positive fixture
  is the enforcement. Recorded so T048 does not assume a 1:1 re-export.
- **D14 — one T042 clause is currently unfalsifiable.** The
  preserving-Current half compares an immutable local identity to itself; it
  becomes falsifiable when T047's runtime exists. T052's review must not count
  it as coverage until then.
- **D15 — compile-fail harness sequencing.** `cases.json`'s T043-era subjects
  resolve only after T048's re-exports; T049 must not run before T048. The
  harness has zero `OutputCoverage` cases; the seal fix above is what makes
  them writable.
- **ClaimContext / `acquire_claim_context` are still absent** — named by
  T043's task text, needed by T042's rebind clauses. They are the NEXT chunk
  of T043, not a deferral.

**Dogfood catch — a symforge defect observed by an auditor.** `get_symbol` for
`LimitBreach` returned `Decision: cache_hit` with "Reuse the content already
loaded in this session" and `session_age_secs=5402` — in a subagent session
that had never loaded that content. A cache voucher pointing at content the
requesting context never observed is symforge's own reporting-invariant
failure class; `force_refresh=true` was the workaround. Reported separately;
not a campaign item.

**Audit-environment lesson.** Two auditors read this worktree WHILE the
mutation loop held a live mutant and promoted the mutant to a blocker. Any
audit fanned out into a mutation-owned worktree must read from a pinned
`git show SHA:` baseline, not the working tree.

## The ClaimContext chunk — the last piece of T043's named surface

`ClaimContext`, `ClaimContextInput`, `CurrentQueryLease`,
`OperationRelationshipContract`, and the free function `acquire_claim_context`
now exist, on the frozen shape from `data-model.md:1844-1872` under the
recorded Slice 3 adaptations: `String` keys per D10, the local lease per D11,
and a `Vec` whose emptiness is refused in the constructor per D10's
NonEmptyVec record.

The closed relationship table is derived from `OperationKind` and nothing
else: search operations permit the cross-source relation and require a
`Current` lease per input; runtime lifecycle operations act on one source and
require none. Both directions of every rule carry an accepting pair:

- empty acquisition → `InvalidSelection`; one-input acquisition admitted
- root drift between acquisitions under `CloseSource` → `SourceUnavailable`;
  the SAME two roots under `SearchText` admitted, because that is the closed
  contract's explicit cross-source relation, not a loophole
- `SearchText` input without a `Current` lease → `AdmissionUnavailable`; with
  the lease admitted; `RefreshSource` legitimately omits it
- a returned context retains exactly the roots, sources, and repository ids
  captured at acquisition — the falsifiable half of "a rebind after return
  does not trigger a trailing live-state check"

`current_query_lease` joins the fixture-evidence family: shape sealed, its
`Ok` unconditional until Slice 4's strict-lease machinery provides the
refusing evidence. Same rule as `completed_render_authority`: do not complete
it with a fake check.

**Mutation ledger, continued** — final suite 34 green:

| # | Mutation | Caught by |
|---|---|---|
| M8 | empty-acquisition guard disabled | `a_context_refuses_an_empty_acquisition`, alone |
| M9 | root-drift guard disabled | `a_rebind_between_input_acquisitions_is_refused`, alone |
| M10 | requires-current guard disabled | `a_generation_structured_operation_requires_a_current_lease_per_input`, alone |

Ten mutations total across T043: nine caught by a named oracle alone, one
survivor that forced a new oracle and was then caught by it.

## The traceability catalog caught an invented name (T041)

First run of `node scripts/validate-lifecycle-oracle-traceability.cjs` on the
T043 tree FAILED:

```
ERROR PLANNED_TEST_CASE_MISSING: trace.catalogs.tests.TEST-PROVENANCE:
tests/claim_provenance_v11.rs::operation_contract_cartesian_matrix
```

The frozen catalog pins `TEST-PROVENANCE` to that exact function name
(CMD-PROVENANCE, owner T041, `introduced_slice: 3`), and the pin activates the
moment the FILE exists. The Cartesian test had been written under an invented
name — the Slice 2 failure mode, caught by the machine this time. Renamed to the
pinned name and WIDENED to match it: the pinned name says OPERATION contract, so
the operation kind became an axis — 4 operations x 4 refusal kinds x 3 retry
advices, `seen == 48`. The pinned command was then run verbatim and observed:
`cargo test --test claim_provenance_v11 operation_contract_cartesian_matrix --
--exact` -> `1 passed; 0 failed; 12 filtered out`.

## Embed-gate result, and why it passes by design

Prediction before running: FAIL, because the nine new `lifecycle_identity` items
are consumed only by `claim_provenance`, which sits under the server-gated
`protocol` module. Observed: **PASS, 1332 passed, 0 failed** (up 3 — the new
module's own unit tests run under embed).

The prediction missed `src/lib.rs:4`:
`#![cfg_attr(not(feature = "server"), allow(dead_code))]`, whose comment states
the policy: under embed an embedder uses a subset of the engine API, so
unused-but-public helpers are expected, not dead. `protocol` IS absent under
embed (`lib.rs:67`), and the identities are idle there BY DESIGN. No cfg-gating
of the new items is needed, and none was added.

## Gate results for the T043 chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean, 29s warm |
| embed lib gate (`--no-default-features --features embed --lib`) | 1332 passed, 0 failed, 4 ignored |
| traceability checker | OK (78 requirements, 24 oracles, 13 categories) — after the pinned-name fix above |
| pinned CMD-PROVENANCE, verbatim | 1 passed, 0 failed, `--exact` |
| both oracle files | 23 passed, 0 failed |
| five closure digests re-emitted | byte-identical to the pinned values |
| full `cargo test --all-targets` | — (runs before the PR, not per chunk) |
