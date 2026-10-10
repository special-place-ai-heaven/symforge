# Embed parity gap matrix (2026-10-10)

Read-only audit of branch `feat/embed-parity-v1` at commit 27724a27, produced by an
Opus investigator from source inspection (symforge index tools), no builds. It
reconciles four inputs: the MCP surface (`SYMFORGE_TOOL_NAMES`, the `#[tool]`
handlers, resources, prompts), the embed surface (`symforge::embed::parity`), the
handwritten claims in `docs/contracts/embed-parity-v1-coverage.md`, and the
`tests/embed_*.rs` fixtures. Append-only; do not rewrite history here.

**Result: not full parity.** Of 59 MCP surfaces: FULL 32, PARTIAL 21, MISSING 6
(tools 18/18/5, resources 6/3/1, prompts 8/0/0). **No capability is proven
class C** (inherently server-bound). Every gap is plumbing (A) or extraction
from a server-only module (B).

**Inventory correction.** `SYMFORGE_TOOL_NAMES` (`src/cli/init.rs:719`) carries 41
names on this branch; `secret_remediate` is the 41st. CLAUDE.md's "40 registered"
is stale once this branch lands.

## Legend

- `Q::X` = `QueryRequest::X` (`src/embed/parity.rs:106`); `H::X` = `HostRequest::X` (`src/embed/parity/host.rs:251`).
- `t:` = `src/protocol/tools.rs`; `e:` = `src/protocol/edit_tools.rs`.
- Doc = coverage-doc claim for the row: acc (accurate), over (overstated), under (understated).
- Class A = plumbing only, shared engine exists. B = logic lives in a server-only module and must be extracted first (module named). C = inherently needs a server process or transport.

## Tools (41)

| # | Tool | MCP | Embed | Status and proof | Doc | Class |
|---|---|---|---|---|---|---|
| 1 | health | t:5789 | H::Health | PARTIAL: no quarantine_limit/offset paging (t:604); no frecency, worktree-misuse, watcher sections (t:5416); HostHealth host.rs:442 carries counts and proof only | acc | A (daemon lines N/A) |
| 2 | health_compact | t:5811 | same HostHealth | PARTIAL: no compact projection | acc | A |
| 3 | status | t:9187 | H::Status | PARTIAL: no STEL status body or calibration lines | acc | B `crate::stel` |
| 4 | index_folder | t:6173 | open_embedded_source(_with_options) + H::Refresh | PARTIAL: no allow_protected_root, idempotency_key, in-place reset (t:514); `add` covered by federation | acc | A |
| 5 | checkpoint_now | t:5837 | H::Checkpoint | FULL | acc | - |
| 6 | validate_file_syntax | t:7459 | Q::Syntax (embed_query.rs:1558) | PARTIAL: indexed parse state only; MCP re-parses disk bytes when unindexed/not ready/rejected via `read_gate::observe_disk_beneath` | over | B `protocol::read_gate` |
| 7 | get_file_content | t:7181 | Q::FileContent / SourcePage | PARTIAL: all options present; no synchronous disk freshen (`freshen_exact_path_for_targeted_retrieval` t:1073) | acc | B `protocol::tools` |
| 8 | get_symbol | t:2766 | Q::SymbolRead | FULL | acc | - |
| 9 | get_repo_map | t:3088 | Q::RepoMap | FULL | acc | - |
| 10 | get_file_context | t:3264 | Q::FileContext | FULL (embed adds include_tests) | acc | - |
| 11 | get_symbol_context | t:3621 | Q::SymbolContext | FULL | acc | - |
| 12 | search_symbols | t:4230 | Q::SymbolSearch + federation::query_sources | FULL | acc | - |
| 13 | search_text | t:4360 | Q::TextSearch | PARTIAL: every option exists; no zero-hit untracked sweep (t:2325 calls `matching_untracked_paths_for_search_text` t:1836) | over | B `protocol::tools` |
| 14 | search_files | t:4729 | Q::FileSearch | FULL (frecency/co-change need QueryPolicy derived-state permission) | acc | - |
| 15 | search_knowledge | t:4384 | Q::SearchKnowledge (shared input) | FULL | acc | - |
| 16 | review_knowledge | t:4440 | H::Knowledge | FULL | acc | - |
| 17 | curate_knowledge | t:4502 | H::Knowledge | FULL | acc | - |
| 18 | find_references | t:7586 | Q::ReferenceSearch (shared input) | FULL | acc | - |
| 19 | find_dependents | t:7907 | Q::DependentSearch | FULL | acc | - |
| 20 | inspect_match | t:4682 | Q::InspectMatch | FULL | acc | - |
| 21 | analyze_file_impact | t:4133 | Q::Impact | PARTIAL, different operation: git diff + dependents only; no disk re-admit, new_file, co_changes, estimate (t:727) | over | B `sidecar::handlers` via t:4161 |
| 22 | what_changed | t:6473 | none (`parity/changes.rs` on disk, undeclared) | MISSING | acc, red test undisclosed | A |
| 23 | diff_symbols | t:10000 | Q::Diff single path | PARTIAL: no repo-wide base/target, path_prefix, language, code_only, compact, summary_only, estimate, max_tokens (t:826) | acc | A |
| 24 | detect_impact | t:6778 | none (`parity/detect_impact.rs` on disk, undeclared) | MISSING | acc, red test undisclosed | B `protocol::read_gate` admit_git_text (read_gate.rs:475) |
| 25 | explore | t:7986 | Q::Explore (shared engine) | PARTIAL: estimate (and max_tokens) only | under | A |
| 26 | conventions | t:8113 | Q::Conventions | FULL | under | - |
| 27 | context_inventory | t:9701 | Q::ContextInventory | FULL | acc | - |
| 28 | investigation_suggest | t:8193 | Q::InvestigationSuggest | FULL | acc | - |
| 29 | symforge_retrieve | t:9640 | Q::Retrieve | FULL (equivalent) | acc | - |
| 30 | ask | t:9717 | none (`parity/ask.rs` and `index_lifecycle/embed_ask.rs` on disk, undeclared) | MISSING | acc | A |
| 31 | edit_plan | t:8169 | edit_plan(&EditTarget) embed_mutation.rs:293 | PARTIAL: guard/ranges/ref count only; no file target, references, co-change, plan | acc | B `protocol::edit_plan` |
| 32-38 | replace_symbol_body e:978, edit_within_symbol e:1715, insert_symbol e:1266, delete_symbol e:1494, batch_edit e:2014, batch_insert e:2270, batch_rename e:2157 | preview/apply pairs in embed_mutation.rs / embed_batch.rs | PARTIAL each: working_directory only | acc (edit_within under) | B `protocol::edit_hooks` + `crate::worktree` |
| 39 | symforge (compact facade) | t:8237 | none | MISSING | acc | B `crate::stel::planner` |
| 40 | symforge_edit | t:8905 | none | MISSING | acc | B `crate::stel` |
| 41 | secret_remediate | secret_remediate.rs:63 | H::InspectSecrets / Preview / Apply | FULL | acc | - |

## Resources (`src/protocol/resources.rs` vs `HostResourceRequest` host.rs:292)

| Resource | MCP | Embed | Status | Doc | Class |
|---|---|---|---|---|---|
| repo/health | :154 | RepoHealth | PARTIAL (health gaps above) | under | A |
| repo/outline | :158 | RepoOutline | FULL | under | - |
| repo/map | :170 | RepoMap | FULL | under | - |
| repo/changes/uncommitted | :182 | NONE, absent from enum | MISSING | doc implies assigned | A, after what_changed |
| tools/catalog | :197 | ToolsCatalog | PARTIAL: returns HostCatalog, not shared `smart_query::render_tool_catalog` | under | A |
| glossary | :198 | Glossary | PARTIAL: different hard-coded text (host.rs:1570) vs `render_glossary` (:345) | under | A |
| file/context, file/content, symbol/detail, symbol/context | :199 :210 :244 :257 | matching variants (+Options) | FULL x4 | under | - |

## Prompts

All 8 FULL via `HostPromptRequest` (host.rs:321) over shared `crate::prompt_engine`
(host.rs:2387-2479). Admin needs a host-supplied `HostPromptContext`. Coverage doc
says "pending"; that is understated.

## Class C candidates examined, none proven

- Daemon lines in status/health describe a process that does not exist in embed: not applicable, not impossible.
- project/projects selectors and `index_folder add`: covered by `federation::query_sources`.
- Live freshness: no watcher/notify in embed by feature choice, so class B, not C.
- Admin dashboard URL: covered by injected host context.

## Red tests and dead tracked files

- `tests/embed_changes.rs` and `tests/embed_detect_impact.rs` cannot compile: undeclared `parity::changes` / `parity::detect_impact`, missing `QueryRequest`/`QueryOutput` variants WhatChanged, DiffSymbols, DetectImpact.
- `tests/embed_ask.rs` cannot compile: undeclared `parity::ask`, missing Ask variants.
- `tests/embed_parity.rs` cannot compile: `ExploreRequest` lacks estimate and max_tokens.
- Dead tracked files: `src/embed/parity/{ask,changes,detect_impact}.rs`, `src/index_lifecycle/embed_ask.rs`.
- `examples/embed_room_consumer` uses public API only by inspection (default-features=false, features=["embed"], own [workspace]). Not built in this audit.

## Status update: class A plumbing batch (appended 2026-10-10)

This file is append-only, so the rows above keep their audit-time status. The
following rows moved from PARTIAL to FULL on branch `feat/embed-parity-v2`.

| Row | Status | Fixture |
|---|---|---|
| 1 health | FULL (daemon, sidecar, hook, binary and worktree-misuse sections reported not applicable) | `tests/embed_health.rs`; MCP golden `health_quarantine_paging_matches_embed_parity_golden` |
| 2 health_compact | FULL | `tests/embed_health.rs` |
| 4 index_folder | FULL (`add` covered by federation) | `tests/embed_index_folder.rs` |
| repo/health resource | FULL | `tests/embed_health.rs` |
| tools/catalog resource | FULL | `tests/embed_health.rs` |
| glossary resource | FULL | `tests/embed_health.rs` |

## Status update: class B read and search batch (appended 2026-10-10)

| Row | Status | Fixture |
|---|---|---|
| 6 validate_file_syntax | FULL: indexed report, disk re-parse when unindexed or the freshen did not publish, shared refusal metadata. A not-Current source refuses rather than parsing disk, because every embedded claim binds a Current publication (`capture_query_snapshot` in `src/index_lifecycle/embedded.rs`) | `tests/embed_disk_parity.rs`; MCP golden `validate_file_syntax_matches_embed_parity_golden` |
| 7 get_file_content | FULL: shared synchronous exact-path freshen before capture. The same freshen also runs for file context and symbol context, which MCP freshens too | `tests/embed_disk_parity.rs`; MCP golden `targeted_read_freshen_matches_embed_parity_golden` |
| 13 search_text | FULL: shared zero-hit untracked sweep with the MCP diagnostic | `tests/embed_disk_parity.rs`; MCP golden `search_text_untracked_sweep_matches_embed_parity_golden` |

Red embed-cell fixtures, root causes (2026-10-10):

- `tests/embed_host.rs` `wire_dispatch_is_source_bound_and_refuses_untrusted_lifecycle_requests` asserted `file-content` in the catalog's static resources. MCP lists it as a resource template, and the catalog already mirrors that, so the assertion was corrected to the template list. Green.
- `tests/embed_git_isolated_config.rs` (two tests) is blocked, not fixed. `PreparedGitView::repository` (`src/index_lifecycle/embed_git.rs:28-36`) and the fixture open the repository with libgit2 open flag `1 << 5`, described as supplied by "the pinned local libgit2 patch". No such patch exists: `Cargo.toml` `[patch.crates-io]` carries no `libgit2-sys` entry and `vendor/` has no libgit2. Upstream libgit2 ignores the unknown flag, so global config, attributes and excludes are still read; `prepare_git_view` refuses `InvalidRepository` on a malformed global include, and the raw open still sees global attributes. Making it pass needs a vendored, patched `libgit2-sys`, which is a vendor and dependency change.

## Status update: edit family and recorded deviations (appended 2026-10-10)

| Row | Status | Fixture |
|---|---|---|
| 31 edit_plan | FULL: `QueryRequest::EditPlan { target }` renders the shared `guidance::edit_plan` plan (moved verbatim from `protocol::edit_plan` and `protocol::format::edit_impact_summary`) for symbol, `path::name`, `Type::method` and file targets | `tests/embed_edit_plan.rs`; MCP golden `edit_plan_matches_embed_parity_golden` |
| 32-38 edit tools | `working_directory` wired: `route_edit` resolves only against a host-admitted `AdmittedEditTarget` list, checks the git common dir from each admitted root's own `.git`, and reports MCP's `working_directory`/`rerouted`/`wrote_to`/`indexed_path` through the shared suffix renderer | `tests/embed_edit_route.rs` |
| 6 validate_file_syntax | Not-Current deviation closed: `validate_syntax_from_disk` returns a `DiskSyntaxObservation` with no publication identity | `tests/embed_disk_parity.rs` |
| 22 what_changed | `max_tokens` deviation closed: uncommitted mode only, 0 is uncapped | `tests/embed_changes.rs` |

Remaining differences, with evidence:

- `edit_plan` co-change lines need Ready git temporal data. Embed never computes it: `spawn_git_temporal_computation` has callers only in server modules, and its `load_commits` (`src/live_index/git_temporal.rs:713`) shells out to `git log`. A restored publication that carries temporal data renders the line.
- A batch whose per-action overrides route different files into different sources has no embed equivalent. MCP stages every file into one rollback transaction; an embedded batch commits under one source root and one replay store (`src/index_lifecycle/embed_batch.rs:665` and `:678`). Batches routed wholly into one admitted worktree work by rebasing each guard.
- The `SYMFORGE_WORKTREE_AWARE` policy is not read, because it is process-global environment. The host's admitted list is the routing policy.

## Status update: git temporal, file impact and routed batches (appended 2026-10-10)

| Row | Status | Fixture |
|---|---|---|
| 14 search_files | `changed_with` co-change now has data: the embed temporal lane walks the source's history with MCP's producer and aggregator when the host permits derived-state preparation, and reports Unavailable with the reason otherwise | `tests/embed_temporal.rs` |
| 21 analyze_file_impact | FULL: `QueryRequest::FileImpact` with every MCP option, re-admitting from disk through the shared `guidance::file_impact` engine the MCP handler and sidecar hook now call | `tests/embed_file_impact.rs`; MCP golden `analyze_file_impact_matches_embed_parity_golden` |
| 31 edit_plan | Co-change lines render from the embed temporal lane | `tests/embed_temporal.rs` |
| 32-38 batch_edit, batch_insert | A batch whose per-action overrides route files into two admitted worktrees commits as one staged transaction, as MCP's `execute_batch_edit` does | `tests/embed_edit_route.rs` |

Corrections to earlier entries, with evidence:

- The earlier note that `load_commits` shells out to `git log` was wrong. It calls
  `GitRepo::log_with_stats` (`src/git.rs`), which is in-process libgit2: the newest
  500 commits within 90 days from HEAD, TIME-sorted, each diffed against its first
  parent with no rename detection. Embed lacked temporal data only because the
  sole caller of `GitTemporalIndex::compute` was the server scheduler
  `spawn_git_temporal_computation`. The producer (`load_commits_from`) and the
  aggregator (`GitTemporalIndex::aggregate`) are now shared, and the producer
  parity unit test runs both repository opens over a rename and merge fixture.
- MCP's guarantee for a batch spanning two worktrees: one `commit_staged` run over
  absolute paths in both roots (canonical path locks, every pre-image verified,
  rollback across both). Its replay receipt is NOT verifiable across roots:
  `bind_post_image_to_source` checks targets only beneath the indexed root's
  anchor (`verify_post_image_bound` in `src/idempotency.rs`), so a batch with a
  target in a worktree outside that root stores no receipt and a retry reports
  that reconciliation is required. Embed matches the transaction and adds
  verified replay: its one record covers every part and a retry verifies each
  part through that part's own source authority.

Remaining differences, with evidence:

- Embed refreshes a changed tree by full reload (`reload_and_publish` in
  `src/index_lifecycle/embedded.rs`), which advances the project generation and
  clears pre-update snapshots (`src/live_index/store.rs`). MCP's watcher
  re-indexes one file and keeps its pre-edit snapshot. So when the embedded
  worker has already reloaded an edited file, `analyze_file_impact` diffs against
  the reloaded copy and reports it unchanged, where MCP still reports the edit.
- Temporal data is walked from the source root's own repository
  (`GitRepo::open_worktree_root`), like embed's other git lanes, not from the
  host-prepared isolated view, which no query lane reads.

## Status update: STEL facades, status and residuals (appended 2026-10-10)

`crate::stel` now compiles under `embed`: the planner, economics, executor,
ledger, status and edit planner are feature-neutral, with only the rmcp tool
schemas (`stel::surface_list`) left server-only. The MCP handlers and the
embedded host call one shared runtime (`stel::runtime`) for the durable
calibration store, the `status` body and the facade ledger, and one shared
search composition (`guidance::search`, `guidance::file_search`) for the
`search_text`, `search_symbols` and `search_files` answers.

| Row | Status | Fixture |
|---|---|---|
| 3 status | FULL: `HostRequest::StatusReport` and `EmbeddedSourceHandle::stel_status` render the shared STEL readout with the session ledger and a durable calibration store opened in the source's state directory under derived-state permission. `reset_calibration` clears that store and needs the checkpoint right at the host. The daemon instance, daemon env surface, degraded fallback, daemon version and proxy overlay lines are reported not applicable with reasons | `tests/embed_stel_status.rs`; MCP golden `status_matches_embed_parity_golden` |
| 39 symforge | FULL: `QueryRequest::Symforge` runs the shared planner, grounding, tuned economics, bypass, degrade, compact caps, find fusion and chain-failure rules, executes each planned primitive on its native lane and renders the shared serve bodies, envelope and ledger. All nine golden cases render MCP's bytes and outcome class | `tests/embed_symforge.rs`; MCP golden `symforge_facade_matches_embed_parity_golden`; shared fixture `tests/fixtures/stel_facade/parity.json` |
| 40 symforge_edit | Routing, economics, envelope, ledger, outcome class, error flag and resulting bytes match MCP for previews of all three ops, a missing symbol, a keyed apply, its idempotent replay, a conflicting key and a refused replay after the file moved (the 2d733398 rule holds). See remaining differences below | `tests/embed_symforge.rs`; MCP golden `symforge_edit_matches_embed_parity_golden`; shared fixture `tests/fixtures/stel_facade/edit_parity.json` |
| 14 search_files | The ranked lane now runs MCP's zero-hit untracked sweep (`FileSearchResult::untracked_paths`); the audit's FULL verdict had missed it | `tests/embed_disk_parity.rs` |
| 21 analyze_file_impact | Reload deviation closed: the embedded refresh keeps the watcher's per-file pre-update baseline | `tests/embed_file_impact.rs` |
| 30 ask | Search routes now carry MCP's result envelope and CCR budget through the shared search composition | `tests/embed_ask.rs` |
| 32-38 MCP | MCP defect fixed: a batch routed across two worktrees binds each target to its own admitted authority, so a same-key retry replays. The per-target read (`post_image_digest_beneath`) is the one the embedded routed batch uses | `batch_edit_routed_across_two_worktrees_replays_by_key` in `tests/worktree_awareness.rs` |

Row census after this batch (59 surfaces):

- FULL with a recorded fixture: tools 1-21, 26-29, 31, 39 and 41; tools 32-38 at the typed-lane standard their batch recorded; every resource except `repo/changes/uncommitted`; all 8 prompts.
- Implemented with every MCP option and native fixtures, but never compared with an MCP golden and carrying no verdict in this file: tools 22 `what_changed`, 23 `diff_symbols`, 24 `detect_impact`, 25 `explore`, 30 `ask` (routing only) and the `repo/changes/uncommitted` resource.
- Tool 40 `symforge_edit` is not FULL, and no limit is proven: see below.

Remaining differences, with evidence:

- `symforge_edit`'s primitive body is the embedded edit lane's own rendering of its typed result (summary line, bounded diff, post-image hash, reroute suffix), not MCP's legacy tool text. MCP's text carries pieces the embedded edit lanes do not produce: a tee snapshot written before the write (`edit::format_tee_snapshot_suffix`), stale-reference warnings (`edit::detect_stale_references`), the project-config trust suffix and the impact footer. The embedded lanes write no tee snapshot at all, which also applies to rows 32-38.
- An idempotent `symforge_edit` replay writes nothing and reports `replayed: true`, but its text is a replay notice, not the original answer: the embedded replay store keeps only response and post-image digests (`ReplayOutcome` in `src/embed/parity/replay.rs`), where MCP's `FileReplayStore` keeps the response text. A same-key conflict refuses with the same class; its text names each store's own request hashes.
- A facade step whose native lane refuses (for example a missing file) reports the typed refusal as its body (`Error: <tool> refused: <kind>`) with MCP's outcome class, where the MCP primitive renders its own message.
- The CCR handle a facade search step offloads to lives in the embedded session's CCR store, so its footer hash differs from MCP's for the same bytes.
