# Activation Cut Execution Map (T029–T037 working notes)

Working document for the Wave 2 branch `feature-020-slice-4-activation`.
Fuses the pre-implementation surveys (dark-seam map + live-V10 map, both
verified against this worktree on 2026-08-18) with the commit sequence.
Not a contract: the frozen 020 tree and the 028 spec/tasks stay authoritative.
No volatile git facts here by design (see CLAUDE.md docs hygiene).

## 1. The two worlds the cut connects

**V10 (live today)**: `SharedIndexHandle` (src/live_index/store.rs) is both
data plane and authority plane — ArcSwap<LiveIndex> + published_state +
generation fences + write mutexes + git_temporal jobs. Every writer, watcher
tick, background verifier, and handler touches it directly:

- Bootstrap: `daemon.rs::bootstrap_project_index` (snapshot-first +
  background_verify spawn); `ProjectInstance::activate` starts watcher +
  git_temporal separately; `main.rs::run_local_mcp_server_async` builds inline
  and spawns watcher/git_temporal/periodic-checkpoint/sidecar;
  `server/serve.rs` loads + background_verify, no watcher.
- Bare-`SharedIndex` holders (publication_roots category): daemon
  `ProjectInstance::index`, `SessionRuntime::index` + `::project_indexes`,
  `DaemonState::bases`, `ServerRuntime::index`, `SidecarState::index`,
  protocol `SymForgeServer::index`.
- Writers: `protocol/edit.rs::reindex_after_write` → `index.update_file`
  (called by execute_batch_edit/insert/rename on success AND rollback);
  `live_index/single_file.rs::{update_file_from_disk, remove_file}` → fenced
  publish (ALSO re-exported raw by embed.rs today); gitignore_hygiene +
  cli/init + persist::ensure_gitattributes_merge_hint write disk only (no
  index edge); knowledge_curation `write_policy` publishes INDIRECTLY (its
  durable write is picked up by the watcher lane).
- Watcher: `process_events` → `read_and_index` / fenced remove; re-spawns
  git_temporal on content-generation advance. `background_verify` re-parses
  via the same admission seam under `feature = "server"`.
- Hooks: HTTP-only ingress (sidecar first, daemon proxy fallback, fail-open);
  PreTool is static text, no data source.
- Embed: flat raw re-exports in `src/embed.rs` (the 79 raw_embed retirement
  members); `lib.rs:31-32` `pub(crate) mod server_api` (flip target);
  `src/live_index/mod.rs:25-26` `#[path]` mount (flip target — delete, add
  private `mod index_lifecycle;` to lib.rs, re-export public types from
  embed.rs; no file moves). `server_api::run` currently returns
  `activation_pending` and holds no call edge into index_lifecycle.

**V11 (dark, complete)**: authority machinery finished, evidence fixtured.

- `authority.rs::SourceRuntime` — 5-phase machine;
  `request_mutation_grant(MutationGrantInput::LiveCurrent) →
  CurrentMutationGrantAuthority` (freezes → non-Current, epoch++).
- `mutation.rs::SourceMutationPermit::grant(grant, lease, drain)` —
  whole-authority validation; `replace_beneath`/`commit → RefreshTicket`.
- `transition.rs::apply(runtime, kind, outgoing, incoming, observer_cut,
  outstanding)` — Freeze→Drain→Install; Install revokes the outgoing root
  lease first.
- `physical_root.rs::PhysicalRootLease` — cap-std beneath-confined I/O,
  staged replacement re-checks revocation at commit.
- `capacity.rs::ProcessCapacityPool` + `process_runtime.rs::
  ProcessIndexRuntime::incarnate/attach(SurfaceKind)` — one process, one
  capacity domain (Daemon|Stdio|Serve|Embed).
- `registry.rs::ProjectRegistry::admit/install/stop` — single-flight
  admission, non-revivable tombstones. `adapters.rs::plan_admission/
  execute_plan` — the admission decision layer (SC-019 enforced).
- `runtime.rs::DarkRuntimeFactory` → `ProjectIndexRuntime` →
  `ProjectPublicationRoot` (ArcSwap<ProjectRuntimePublication>);
  `acquire_strict` implements F020-V11-A20. All mutating doors are
  `*_for_test` fixtures that mint evidence unconditionally.
- `public_api.rs::ProcessRuntimeApi::{acquire, open_embedded_source,
  begin_shutdown}` + wrap_table() work list; `embedded.rs::
  EmbeddedSourceFactory/EmbeddedSourceHandle` — sole-handle semantics ready;
  search/refresh return honest dark refusals awaiting bound generations.
- Wave 1 modules: supervisor / candidate / verification / query / observer /
  snapshot — oracle-green dark.

`src/index_lifecycle/activation.rs` does NOT exist yet (T030 creates it).

## 2. Load-bearing preconditions and rulings

1. **runtime.rs:63-66 ACTIVATION PRECONDITION**: before the keyword flip,
   every `#[cfg(any(test, feature = "server"))] *_for_test` door must become
   `#[cfg(all(test, feature = "server"))]` (their oracles move in-crate) or
   sit behind a dedicated non-server test feature — otherwise fixture
   evidence ships in the release binary.
2. The 30 doc-recorded cut obligations indexed in §5 below are the work
   checklist; each is either discharged by the cut or explicitly carried into
   the T038 evidence doc with rationale.
3. Frozen census partition (oracle `all_ingress_uses_exact_typed_authority_
   branch`): 244 slots / 13 categories; surface split 102 branch-bearing +
   3 non-ingress + 11 authority-free; 8-branch MODEL_SURFACE (test-local by
   design — the src-side typed enum is ours to build); replay residual
   (8 tools / 7 duplicate writers); estimate dispositions [7,5,3,1]; tools
   full-39/compact-3/union-40.
4. Wave-1 pair-review recorded obligations (from the merged PR reviews):
   latch clear-on-emission-vs-proof seam + HandoffBarrier threading +
   ObserverId/ObserverToken unification (observer.rs, T064 work); real
   RequiredArtifactSet (candidate.rs); persist.rs wiring + CURRENT_VERSION
   bump (snapshot lane, T065→cut); health_view/protocol wiring (query lane);
   missing/forged bijection oracles; transport codes vs frozen
   embed::SourceRefusal alignment; MetadataOnlyReason in-crate
   exhaustiveness pin.

## 3. Core design decision (D1): the authority facade owns the data plane

The V11 layer is authority-only; the LiveIndex machinery remains the data
plane. The cut inserts a typed facade that OWNS the `SharedIndexHandle` and
is the only thing allowed to touch it:

- `ProjectRuntimePublication` carries the real payload (the published
  LiveIndex view) instead of fixture data.
- A per-project authority handle (activation.rs) exposes ONLY typed-branch
  acquisitions mirroring MODEL_SURFACE: strict generation leases
  (GenerationLeased), mutation grants (MutationPermitted), the observed
  branches, and Refused. Field-type replacement drives the compiler over
  every touch site: `ProjectInstance::index`, `SessionRuntime::index/
  project_indexes`, `ServerRuntime::index`, `SidecarState::index`,
  `SymForgeServer::index` change type, and rustc enumerates the rerouting
  work — unrepresentable over checked.
- Chokepoints cover families: daemon dispatch goes through
  `runtime_for_target`; sidecar handlers all read `SidecarState`; protocol
  tools go through `SymForgeServer`. Branch selection happens at those necks,
  not by rewriting each handler body.

## 4. Commit sequence on this branch (mid-branch mixing allowed; PR is the unit)

- C1 (T030 core): `src/index_lifecycle/activation.rs` — ActivationCut
  machine (LegacyOpen → LegacyClosing → PreventiveV1Open, monotonic,
  process-wide, non-configurable), lane registration, the authority facade
  type; plus the cfg-tightening precondition (move needed oracles in-crate,
  `any(test, feature="server")` → `all(test, feature="server")` on fixture
  doors). Compiles dark; census/seal updated.
- C2 (T029 writers lane): SymForge-owned writes acquire a fresh
  `SourceMutationPermit` (publish non-Current first): protocol/edit.rs batch
  paths, knowledge curation apply; gitignore/init/.gitattributes classified
  per contract (source-authorized hygiene vs permit-free ProjectStateDir and
  post-image team-artifact state writes → StateWriteAuthorized).
- C2b (T029 writers census closed — executed 2026-08-19): gitignore hygiene
  (`reconcile_root_gitignore` append) and the `.gitattributes` merge hint
  (`persist.rs`) acquire the permit via the shared
  `acquire_write_serialized`; the retired `atomic_replace` is deleted
  (inventory-mismatch onset, §4b). The registry key canonicalizes so every
  spelling of one physical root converges on one authority. Classification
  adjudications recorded in-file: `cli/init.rs::run_init_with_paths` writes
  only user-scope client configs (permit-free; repo-source writing delegated
  to the hygiene lane); `edit_tools.rs` census members delegate all disk I/O
  to the `edit.rs::atomic_write_file` chokepoint; curation ledger writes
  (state dir) are permit-free `StateWriteAuthorized`, while the POLICY file
  `.symforge-knowledge.toml` at the repository root is repository-source —
  its two writers (`write_policy` apply path, `recover_on_project_load`
  recovery) are source-authorized and take the permit lane at C3 with the
  callback registration (recorded residual, alongside C2's worktree-reroute
  residual in `edit.rs`).
- C3a (T029 observation lane core — executed 2026-08-19): the
  ProjectSourceAuthority gains the observation lane — per-source
  supervisors, the isolated delta-candidate pipeline to its single commit
  point, the bounded coalescing accumulator, and the ObserverSlot with the
  ObserverId/ObserverToken unification (publications carry the ACTIVE
  incarnation's token; the two accumulator latches the observer module
  recorded as T064 obligations are threaded: handoff barrier at
  registration, scope-dirty for recovery). Every permit return consumes the
  accumulated cut. WriteAuthority drop now RECOVERS through the re-scout
  lane (scope-dirty full baseline) — the C2 stranding oracle inverted
  RED-first. Wired: watcher run-loop registers one incarnation per
  instance; process_events observes admissions/removals under it (stale
  incarnations refused — late callbacks unreachable); overflow latches the
  gap; the embed facade update/remove observe as the current incarnation.
  Recorded residuals: periodic/fresh reconciliation sweeps observe only
  through the barrier/gap latches (their per-file re-admissions join at
  C4); data-plane admissions keep the V10 generation fence until C4.
- C3b (T029/T064/T065 policy lane + callbacks census closed — executed
  2026-08-19): the curation POLICY file's two writers take the permit lane
  as a DELEGATED side effect — `begin_delegated` puts the permit in flight,
  the contract-pinned staged durability protocol runs untouched
  (failpoints, digest verification, fsyncs), and `attest_delegated` has the
  pinned lease re-read the target and mint a receipt only for the exact
  authorized post-image (`Ok(None)` = mismatch = drop-recovery is the only
  honest terminal). Wired at the apply path and the `PendingWrite`
  recovery arm shared by replay and `recover_on_project_load` (which
  discharges that census row). `background_verify` carries the observer
  incarnation current at its spawn (threaded from all three spawn sites:
  daemon bootstrap, stdio main, serve); its re-admissions/removals observe
  under that id and a successor registration refuses them — pinned RED-first
  together with the data-plane-continues residual. Enrichment callbacks
  adjudicated in-file: local-ref reconcile and git_temporal are
  publication-fence-gated data-plane enrichment (no source admission;
  typed-root gating at C4); periodic checkpoint is permit-free
  ProjectStateDir state; edit_hooks resolve/after_commit are routing/fan-out
  with their writes landing through already-wired lanes; the three watcher
  census rows are discharged by C3a's registration inside
  `run_watcher_with_stop`. DESIGN FIX exposed by the C3b foreign-root
  oracles: since C2 the authority idled with its cap-std directory handle
  OPEN, which on Windows blocked renaming (and hostage-held) every
  repository root ever written — `PhysicalRootLease` is now DORMANT between
  permit cycles (`parked`/`reopened`, identity- and revocation-preserving);
  the confinement handle exists only while a permit is in flight, which is
  the window the confinement claim protects.
- C3 (T029 observation lane): watcher process_events + single_file admission
  route through the isolated candidate pipeline permit-free; background
  verify / git_temporal / local-ref / periodic checkpoint callbacks register
  with supervisor+observer incarnations (no callback holds publication
  authority; late V10 callbacks unreachable).
- C4a (T030 roots, structural — executed 2026-08-19): the D1 ownership
  move. `activation.rs::ProjectRuntimeHandle` (inner field deliberately
  named `data_plane`, NOT `index` — the census derivation counts every
  `index: SharedIndex(Handle)` field as a V10 root, and the sole authorized
  holder is the replacement, not a root) now owns the data plane at all six
  retired publication_roots fields: `ProjectInstance::index`,
  `SessionRuntime::index`, `SessionRuntime::project_indexes`,
  `SymForgeServer::index`, `ServerRuntime::index`, `SidecarState::index`.
  Field-type replacement drove the compiler over every touch site
  (~200 sites across daemon/protocol/server/sidecar + tests); every read
  routes through the enumerable `data_plane()`/`shared()` door, pinned
  RED-first by the structural oracle
  `root_holders_store_no_bare_shared_index` (struct-scoped, so params and
  locals cannot satisfy the claim). Behavior-preserving by design: typed
  acquisition branches arrive at the dispatch necks with C4b/C4c, not by
  rewriting handler bodies. `IndexBase::index` (Arc<LiveIndex>, cache
  census) deliberately untouched — cache disposition is C4c/C5 work.
- C4b (T030 bootstrap — executed 2026-08-19): bootstrap flows through the
  activation machine and the process registry. `activate_surface(surface)`
  runs the startup CEREMONY on the process machine — register all nine
  frozen lanes, `begin_closing`, confirm every drain, `open_preventive` —
  BEFORE the surface serves, which is what makes each drain confirmation
  truthful: at that moment the bootstrapper IS every lane's owner and can
  observe that nothing has entered the legacy gate. It also attaches the
  surface to the one `ProcessIndexRuntime` (dark budgets: 1 GiB per
  surface, 4 GiB process; C7/C8 measure real ones). Wired at daemon state
  construction, stdio `run_mcp_server_async`, and `serve::run`; embed
  attaches at C5. `admit_project` is the single admission door: plan
  (`plan_admission`) → single-flight admit (`execute_plan`, live
  occupancies join on matching root+placement) → install charged to the
  surface's capacity owner; wired at `ProjectInstance::load_bound`, the
  stdio local bind, and the serve load — refusals fail the open honestly.
  `ProjectSlot::stop` retires the admission slot, so daemon eviction and
  retarget re-admit fresh. The authority presents a STABLE admission-root
  identity (its first lease's), so every open of one canonicalized root
  joins one occupancy; recorded residuals: a root physically replaced at
  the same path keeps its admission identity until C5's transitions own
  rebinding; the serve path surfaces no RootBinding and presents
  NormalProject (its loader resolves protected roots upstream); the
  registry-ownership move is the admission door — the
  `project_source_authority` static remains the per-root convergence
  lookup until C5 narrows it.
- C4c part 1 (T030 roots — executed 2026-08-19): the sweeps join the lane
  and the daemon neck gets its typed acquisition branch. Reconciliation
  (`reconcile_stale_files_with_stop_and_hook` and every caller) and
  freshen-on-read (`freshen_file_if_stale` at the watcher fallback, the
  sidecar handler, and the three edit/retrieval request paths) carry an
  `ObserverId`: long-running sweeps carry the watcher's spawn-registered
  incarnation, synchronous request paths the incarnation current at call
  time (the C3b synchronous-facade ruling). Re-admissions observe on the
  mutation EVIDENCE (`Reindexed`), removals on the completed removal —
  before any generation re-check, since a spurious observation only
  dirties the next cut while a missed one loses the change. Data-plane
  admissions are thereby GATED: every sweep re-admission now flows
  through the candidate pipeline's capacity/supersession gate (a refused
  candidate latches a gap, never drops the change). The daemon's
  `ProjectRuntimeHandle` carries its admission slot
  (`bind_admitted`/`acquire`); `runtime_for_target` refuses a retired
  slot via the registry's own shared revocation flag. Deliberately NOT
  carried on stdio/serve handles: those admissions are process-lifetime
  with no stop path, so a refusal branch there could never fire
  (reporting-invariant ruling); C5's typed bootstrap revisits. RED
  observed on all three oracles before wiring
  (`reconciliation_sweep_feeds_the_observation_lane_under_the_carried_incarnation`,
  `freshen_on_read_feeds_the_observation_lane`,
  `retired_admission_slot_refuses_dispatch_at_the_runtime_neck`);
  `src/protocol/tools.rs` and `src/protocol/edit_tools.rs` joined
  WIRED_PRODUCTION_FILES.
- C4c part 2 (T030/T031 roots, remaining): cache-census dispositions —
  adjudicate the nine frozen cache members against the category's three
  assertions; survey found the members generation-fenced or
  non-authoritative by construction EXCEPT `WorktreeCache`, whose
  `lookup` hit ignores which indexed root populated the entries (a
  cross-project reroute a fresh `git worktree list` would refuse) — fix
  RED-first, classify the rest in-file.
- C5 (T031 exposure): embed.rs raw re-exports removed per the 79-member
  raw_embed dispositions → V11 replacement API + EmbeddedSourceHandle
  re-exports; `server_api` `pub(crate)`→`pub`; mount flip; exact-graph
  equality across the 26 configuration cells re-verified.
- C6 (T032): the four stand-ins get observing bodies, `#[ignore]` removed;
  focused cli/init + persistence tests (cold-recovery-cannot-mint-permit,
  frozen FR-051 matrix).
- C7 (T033): criterion dev-dep + `[[bench]] observed_refresh_gate_v1`
  (frozen registration `criterion_group:observed_refresh_gate_v1_group->
  observed_refresh_gate_v1`) + fixtures.
- C8 (T034): capacity conservation oracle
  `whole_runtime_capacity_is_conserved_under_activation` + bench additions.
- C9 (T035): gate run vs baseline `1521abb0` → docs/reviews/
  OBSERVED-REFRESH-GATE-v1.md.
- C10 (T036): tests/delta_full_rebuild_equivalence_v11.rs (frozen names).
- C11 (T037): campaign run + full gate battery via Terminal Commander.

Gate battery after every commit-group, serial via TC; fmt BEFORE pin
refresh; embed feature gate before every push.

## 4b. Traceability validator lifecycle (verified against the .cjs, C2 planning)

The validator classifies the live tree by comparing lib.rs's public-mod
census against the frozen atom sets (`resolvePublicApiLifecycle`):

- **Preactivation** (until C5's keyword flip): enforces (1) the frozen
  byte-census digests over the five closure categories
  (`RETIREMENT_CLOSURE_MISMATCH`), (2) exact equality of the
  pattern-DERIVED semantic inventory with the frozen member lists
  (`RETIREMENT_SOURCE_INVENTORY_MISMATCH`) — fields typed
  `SharedIndex(Handle)` named index/project_indexes, writer/callback items
  by file+name, `pub struct SharedIndexHandle`, pub reload methods — and
  (3) source-anchor resolution of every member.
- **Postactivation** (lib.rs matches kept+introduced atoms): switches to
  (1) no retired atom reachable in the public graph
  (`POSTACTIVATION_RETIRED_API_REACHABLE`) and (2) every frozen seam
  anchor resolves in live source (`POSTACTIVATION_V11_SEAM_UNRESOLVED`) —
  which REQUIRES a resolvable `V11PublicApi` construct in
  `public_api.rs` (does not exist yet; C5 creates it) alongside
  `activation.rs::ActivationCut` (exists). The semantic-inventory
  equality is NOT enforced postactivation, so retained function names in
  rerouted writers are fine after the flip.

**Planned mid-cut state**: from C2 until C5 the validator is expected RED
with exactly these codes, verified against the live run at each commit:

- `RETIREMENT_CLOSURE_MISMATCH` for every category whose censused FILES a
  landed commit edited — `writers` from C2; `callbacks` joins at C2b
  because `persist.rs` and `knowledge_curation.rs` sit in both categories'
  path closures; further categories join as C3/C4 touch their files.
- `RETIREMENT_SOURCE_INVENTORY_MISMATCH` for each retired name-anchored
  member (`extra_in_contract`), plus its mechanical shadow
  `PREACTIVATION_SOURCE_ANCHOR_UNRESOLVED` for the same member: one
  deletion produces both, since the frozen contract still names an anchor
  the live tree no longer holds. First member:
  `src/gitignore_hygiene.rs::atomic_replace` at C2b (earlier than first
  planned — the original note expected the inventory mismatch only at D1's
  `index: SharedIndex` field removals, which will add their own rows).

Any failure code outside this enumeration in that window is a real defect.
Observed at C2b (2026-08-19): exactly the four lines the enumeration
predicts — closure mismatch (writers, callbacks), inventory mismatch +
anchor shadow (atomic_replace). Observed at C3b (2026-08-19): exactly SIX
lines — `cache` and `publication_roots` closures joined because C3b edits
`daemon.rs` (five cache rows + three publication_roots rows) and
`knowledge_curation.rs` (`probe_cache` sits in the cache closure); no new
name-anchored member was deleted, so the inventory/anchor pair is still
only `atomic_replace`. The
Observed at C4a (2026-08-19): exactly SEVEN lines — the C3b six plus the
publication_roots inventory mismatch whose `extra_in_contract` names all
six retired `index`/`project_indexes` field members (the D1 rows the C2b
note predicted); anchors still resolve (the field names survive with the
handle type), so no new anchor shadows. The
validator returns green via the postactivation path at C5; that is a hard
PR exit criterion. Tool profiles (full=39 / compact=3) must hold in BOTH
lifecycles.

**Seal transformation at C2**: the call-edge sweep inverts from "no live
file names index_lifecycle" to a pinned reachability roster — the set of
live files holding call edges is exactly the planned wiring set, extended
per commit. This preserves the no-unplanned-edges property through the
mid-cut window and becomes the executed-reachability record the frozen
retirement contract says replaces the preactivation census.

## 5. Doc-recorded cut obligations index (verified file:line)

mod.rs:26 (writer lanes consume permits = Slice 4); mod.rs:30-55 (per-module
"activation is the only planned production caller" × candidate/observer/
query/snapshot/verification); mod.rs:57-58 (dark factory single door);
live_index/mod.rs:21-24 (mount flip recipe); authority.rs:746-749
(retained_generation A20 wording trap); mutation.rs:100-103
(NoSideEffectProof becomes real behind the write lane);
public_api.rs:6-8 (wrap_table is the work list), :219-220 (EmbedRefreshTicket
wiring changes evidence not shape), :374-375 (shutdown wiring), :455-456
(acquire gains refusing evidence); adapters.rs:7-9, :108, :155-156
(execute_plan gains its production caller); embedded.rs:3-8 (module goes
reachable), :349-352 (DeadlineElapsed becomes producible);
registry.rs:15-18, :251-253 (lifecycle registry replaces daemon slot map as
admission authority); runtime.rs:7-8, :10-16 (D-ledger payload obligations),
:18-21 (fixture evidence replaced), :54 (real root lease), :63-66 (cfg
precondition), :144-147 (VerifiedGeneration fields), :253-256 (permit return
via fresh candidate publication), :474-476 (retrying capture);
protocol/claim_provenance.rs:72-73 (fixture PhysicalRootLease replaced by
the real lease acquisition).
