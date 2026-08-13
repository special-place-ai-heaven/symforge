# Slice 3 reconnaissance findings

Produced by four independent read-only censuses plus one adversarial critique, run
against `10e036b5`. Every item below was RE-VERIFIED by hand before being written down;
the agent report was treated as input, not as evidence.

Critique verdict: **plan-needs-amendment** (6 blockers, 8 corrections, 8 defect
candidates).

## D1 — SECURITY: ungated git-object content disclosure (confirmed)

`src/protocol/format.rs:6537-6564`, inside one loop body of the `diff_symbols` renderer:

- `base_content` = `repo.file_at_ref(base, file_path)` — **no admission gate**, both modes.
- `target_content`, uncommitted mode — routed through `read_gate::admit_worktree_text`,
  and on refusal it emits `content_withheld_by_admission` and `continue`s.
- `target_content`, committed mode — `repo.file_at_ref(target, file_path)` — **no gate**.

Both ungated buffers flow into `extract_symbols_for_diff` / `extract_symbol_signatures`
and are rendered as symbol names and signatures.

`read_gate.rs:1-7` states it is "the single admission/disclosure gate for raw-disk
content reads", and its own doc records that it exists BECAUSE three lanes — including
`diff_symbols` in uncommitted mode — each disclosed a security-demoted file. The
worktree read in that lane was fixed. The git-object reads beside it were not, so a file
the gate withholds on the worktree side is still fully disclosed through git objects.

The gated branch carries a comment explaining that falling through with empty content
"would render every symbol in the file as REMOVED, which is a false claim about the file
rather than a refusal to describe it." The two ungated branches do exactly that
`.unwrap_or_default().unwrap_or_default()` fallthrough.

Same shape reported in `detect_impact` (`src/protocol/tools.rs:8422-8425`), where
`admit_worktree_text(...).unwrap_or_default().unwrap_or_default()` erases `Err` and
`None` alike — see D8.

## D2 — the tripwire for D1 has two blind spots (confirmed)

`src/protocol/tools.rs:31232-31258`,
`no_protocol_lane_reads_the_working_tree_ungated`, whose own doc comment says it "fails
the moment a FOURTH one is written, which is how the original three came to share an
ungated read in the first place."

1. `let needle = [".file_from_", "workdir("].concat();` matches ONLY
   `.file_from_workdir(`. D1 reaches content through `file_at_ref`, so it passes clean.
2. `fs::read_dir(&protocol)` is non-recursive, and the
   `path.extension() != Some("rs")` guard skips directory entries — so
   `src/protocol/format/` and `src/protocol/tools/` are never scanned at all.

The guard against the next ungated lane would not have caught the one that already
exists.

## D3 — frozen-corpus discrepancy: `DerivedLimitKind` is 8 live vs 6 frozen (confirmed)

`src/live_index/knowledge_bridge.rs:179-189` declares eight variants: `Cards`,
`BridgeLinks`, `OwnershipSelectors`, `AmbiguousSamples`, `AuthorityRecords`, `Findings`,
`MetadataBytes`, `Output`. `specs/020-repository-knowledge-index/data-model.md:1007-1014`
declares six — omitting `OwnershipSelectors` and `AmbiguousSamples`, both of which
production actively records.

`LimitBreach` already exists at `knowledge_bridge.rs:191-195`, byte-identical to the
frozen shape.

This is NOT an implementation choice. T043 is instructed to transcribe the frozen types
verbatim, which would create a second, conflicting, lossy `DerivedLimitKind` in one
crate. **Do not edit the frozen corpus to resolve this.** It needs an explicit decision
recorded before T043 is written: reuse the live 8-variant type, or amend the corpus
through the manifest/approval chain.

## D4 — `introduced_v11_atoms` is 60 embed + 4 `server_api` (confirmed)

`contracts/public-api-v11.json` `migration_v10.introduced_v11_atoms` is not uniformly
under `symforge::embed`. Four atoms are `symforge::server_api`,
`::ServerBootstrapError`, `::ServerExit`, `::run`, and `grep -rn server_api src/`
returns nothing — the module does not exist.

Consequence for T048: the export delta includes a genuinely NEW top-level
`pub mod server_api;` in `src/lib.rs`. That is the single case where the Slice 2
`#[path]` re-anchor trick cannot apply, because a top-level public module is exactly
what the atom demands at activation.

## D5 — `published_repo_outline` is write-only state (confirmed by the reporter)

`src/live_index/store.rs:1329` declares it; it is stored at `:3211`, `:3299`, `:3311`
and never loaded. The public accessor at `:2317` reads
`published_generation().outline` from the captured set instead. Dead state carried
through the hot publish path.

## D6 — `health_for_runtime` renders one report from four independent lock-free loads

`src/protocol/tools.rs:6978`, `:7048`, `:7054`, `:7075` — `published_state()`,
`git_temporal()`, `read()`, `published_source_set()` all load separately inside one
function body, so a publication landing mid-render mixes generations within a single
user-visible health report. `health_compact_for_runtime` has the identical shape.

This is the CLAUDE.md reporting invariant in its literal form: the thing that reports is
not the thing that knows.

## D7 — `get_symbol`'s session cache key carries no generation identity

`src/protocol/session.rs:452-470` hashes only `kind`, `symbol_line`, `max_tokens`
(used at `tools.rs:4302`). Its two siblings do include generation identity —
`get_file_content` at `tools.rs:8743-8749` hashes `project_generation`, `source_id`,
`publication_generation`. So a symbol body served before a publication can be replayed
from cache after it, because nothing in the key changes when the index does.

**This also re-opens RISK-A**, which the plan had declared settled on `ccr.rs` evidence
alone. The `cache` closure category is a different lane from `ccr`, and it hashes
provenance-adjacent identity directly.

## D8 — `detect_impact` collapses a gate refusal into an empty seed (likely)

`src/protocol/tools.rs:8422-8425`:
`admit_worktree_text(...).unwrap_or_default().unwrap_or_default()` erases both `Err`
and `None`. For a supported language, empty content yields an empty current-symbol map
rather than the documented "seed every indexed symbol" fallback, because the fallback at
`:8433` fires only when the extractor returns `None`, which empty content does not
produce.

## D9 — the two frozen documents disagree about T043's own API (confirmed)

`data-model.md` gives the DATA shape; `contracts/public-api-v11.json`
`introduced_v11_atoms` gives the ACTIVATION surface. They conflict, and T043 cannot
satisfy both.

1. **`Claim`'s last member has two frozen names.** `data-model.md:1809` declares
   `pub producing_publication: ProducingPublicationIdentity`. The atom is
   `symforge::embed::Claim::producing_runtime_identity`. Same member, two spellings.
2. **`SourceRefusal` has two frozen SHAPES.** `data-model.md:1812-1831` is an enum of
   four variants with `pub` fields. The atoms give it ACCESSORS instead — `::kind`,
   `::operation`, `::retry`, `::evidence_identity` — plus `SourceRefusalKind` and
   `RetryAdvice`, two types `data-model.md` never mentions. An accessor surface implies
   opaque internals, which is the opposite of pub-field variants.
3. **`OperationReceipt` likewise** is not spelled in `data-model.md` at all, but the
   atoms fix four members: `::identity`, `::operation_kind`, `::schema_version`,
   `::canonical_argument_hash`.

**Proposed resolution, needs a decision before T043 is written.** The API contract wins
for API NAMES and VISIBILITY, because `expected_graph.activation_rule` is enforced
exactly at activation — "missing and extra atoms both refuse activation" — whereas
`data-model.md` is descriptive prose that no checker compares against code. So: opaque
types with the atom-named accessors, `producing_runtime_identity`, and
`SourceRefusalKind` / `RetryAdvice` as real types.

Same handling as D3: do NOT amend either document inside T043. Record the divergence and
let a later amendment reconcile them deliberately.

Note the atoms also cover T047's runtime surface, not just T043's:
`ProcessIndexRuntime`, `EmbeddedSourceHandle`, `RefreshTicket`, `ShutdownReceipt`,
`SourceCloseReceipt`, `SourceRuntimeView`, and the search request/result types. T043
owns only the provenance subset.

## T043 support-type census — what exists, what T043 must create

Verified by declaration search on `10e036b5`. Slice 2 lost real time to inventing seam
names that already existed, so this is the list T043 works from.

**Already exist — import, do not redeclare:**

| Type | Where |
|---|---|
| `BindingAuthority` | `src/index_lifecycle/authority.rs:94` |
| `PhysicalRootIdentity` | `src/index_lifecycle/physical_root.rs:46` |
| `GenerationIdentity` | `authority.rs:50`, MACRO-GENERATED |
| `PublicationIdentity`, `BindingIdentity`, `ObserverToken`, `CandidateIdentity`, `SnapshotIdentity` | `authority.rs:36-59`, macro-generated |
| `CatalogPath` | `src/domain/index.rs:804` |
| `PlatformFileId` | `src/domain/index.rs:860` |
| `FileStamp` | `src/domain/index.rs:864` |
| `RepositoryId` | `src/domain/index.rs:479` |
| `LimitBreach`, `DerivedLimitKind` | `src/live_index/knowledge_bridge.rs:179-195` — the LIVE EIGHT, see 6a |

**A plain declaration search MISSES the macro-generated identities.** `GenerationIdentity`
has no `struct GenerationIdentity` anywhere; `authority.rs:21-34` defines
`identity_newtype!`, which emits `pub struct $name(NonZeroU64)` with a `fresh()`
constructor off a process-wide `AtomicU64`. Any new Slice 3 identity uses that macro
rather than a hand-rolled newtype — but note the macro is currently PRIVATE to
`authority.rs`, so T043 either shares it deliberately or repeats the shape knowingly.

**Do not exist — T043 creates them:** `ObservationTime`, `StableReadReceipt`,
`ByteDigest`, `GitObjectId`, `ObserverEpoch`, `InvalidationSequence`, `ManifestDigest`,
`NonEmptyVec`, `NonEmptyMap`, `ProjectSourceKey`, `GenerationAuthority`,
`WorktreeObservationScope`, `ScopeAndPolicyVersions`, `WorktreeScanId`,
`SourceSelectionReceipt`, `ComparisonRelation`, `OperationReceipt`, `OperationKind`,
`EvaluationProvenance`, `ProducingPublicationIdentity`, `CanonicalArgumentHash`,
`AdmissionSubject`, `UnavailableCause`, `ResolvedSourceReceipt`, `ResolvedSelectionSet`,
`CurrentQueryLease`, `PhysicalRootLease`, `OperationRelationshipContract`,
`GitIndexChecksum`.

That is ~29 new types against ~10 reused ones, which is the real size of T043 and is
larger than the plan's "~15 types" implied. The frozen definitions name them; none of
the names are open to invention.

## Corrections to the Slice 3 plan itself

Each verified by hand; all six critique blockers stand.

1. **`view.rs` consumes nothing.** The plan claimed the nine `ArcSwap` fields live in
   `store.rs` with "`view.rs` a consumer". `view.rs` contains zero reads of any of the
   nine and zero `ArcSwap` code — five hits, all prose comments (`:3`, `:92`, `:94`,
   `:1195`, `:1217`). `tasks.md:923` names the wrong file for T046; that is a task-text
   correction to record with evidence, not a scope widening to invent.
2. **Nine fields, not ~10**, and five of the nine already resolve through
   `published_source_set` on the public read surface. The residual problem is how many
   times ONE caller loads the set, not which field it reads.
3. **T046 needs a read-MUTATE-read exclusion rule.** At least ten production sites load,
   mutate, then load again — `store.rs` 2494/2495, 2551/2552, 2660/2661, 2798/2799,
   2813/2814, 3025/3027, 3104-3111, 3142/3143 and `watcher/mod.rs` 703 vs 853, 433 vs
   491. `store.rs:2660` `swap_and_publish(live)` is followed at `:2661` by
   `published_generation()`, whose value becomes `IndexedFilePublicationReceipt.published`
   (`:2665-2668`). Hoisting that to an entry-time capture makes every publication receipt
   one generation stale. These are deliberate before/after samples and are OUT of T046's
   scope by construction; they must be enumerated as exclusions before any edit.
4. **All five closure digests regenerate regardless**, because T046 alone reaches four
   of the five path sets. The `knowledge_curation.rs` contingency the plan staked the
   decision on is not the deciding factor.
5. **PR 1's anchor must be `format.rs`, not `read_gate.rs`.** `protocol/mod.rs:16`
   declares `pub(crate) mod read_gate;`, so anchoring there makes the whole provenance
   module crate-private — and `tests/claim_provenance_v11.rs` is a SEPARATE CRATE that
   cannot see it. `protocol/mod.rs:9` declares `pub mod format;`, and `format.rs` is
   uncensused and is not a T044 target. Verified: `read_gate.rs`, `format.rs` and
   `live_index/mod.rs` are in none of the five closure path sets.
6. **"Provenance is not a CCR input" is misleading.** Provenance is already rendered
   into `formatted` today, so it is a transitive input. The conditional conclusion
   survives; the sentence does not.
7. **T044 is not "small" in the sense the plan meant.** The 178-line count is right but
   is the wrong measure: `read_gate.rs` performs no confinement at all today, so
   "beneath-confined disk observation" means ADDING containment the gate does not have.
8. **The frozen corpus specifies two layers with the same type names and incompatible
   shapes, and gives no precedence rule.** T043 alone cannot satisfy both — see D3.
