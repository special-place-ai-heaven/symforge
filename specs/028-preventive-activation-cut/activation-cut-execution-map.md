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
- C3 (T029 observation lane): watcher process_events + single_file admission
  route through the isolated candidate pipeline permit-free; background
  verify / git_temporal / local-ref / periodic checkpoint callbacks register
  with supervisor+observer incarnations (no callback holds publication
  authority; late V10 callbacks unreachable).
- C4 (T030/T031 roots): bare SharedIndex holders replaced by the authority
  facade; bootstrap flows through activation machine + registry/adapters
  admission; serve/stdio/daemon/sidecar surfaces attach via
  ProcessIndexRuntime.
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
