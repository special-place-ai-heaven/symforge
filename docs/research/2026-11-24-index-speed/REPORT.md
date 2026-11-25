# SymForge first-use latency

**Windows, 24 November 2026 · research baseline · SymForge 12.1.0 · source `3a4fb63d`**

## Decision

Approve the compact-map fix first. On the SymForge checkout a warm compact map took about 36 seconds to return a truncated directory list. On a fresh AAP checkout the map took 111 seconds. Both outputs omitted the entire key-type section that the formatter had ranked. A controlled corpus confirmed that spreading the same types among more files increases the cost even when every returned byte is identical. Preserve the exact text, counts and ranking whenever the key types are visible.

Cold code search has a separate bottleneck. Scouting alone took 4.73 s on moldwright and 3.62 s on Agent Army Professionals (AAP), already beyond the three-second target. Markdown processing matters most on AAP, where the deferred bridge and authority took a further 4.30 s. The [ranked PRs](#ranked-pull-requests) address these paths separately.

Keep all four targets. Measure request to first correct hit, first complete answer and maximum warm map latency. A loading response is feedback, not a successful search. Record the root, freshness and coverage with every result.

## Independent review reconciliation

**25 November follow-up.** A two-crate control reproduced the watcher attribution by changing one private import into a public re-export. The first resolver correction removes the disconnected fixture from the top 15 of the pinned SymForge map. Its scoped arrow falls from 487 to 56; the independent source audit still gives zero. The clean three-call depth-one map medians were 35.663 s before and 36.055 s after. Keep #12 on hold until the direct imports and named re-export chains are correct. [Visible importer audit](LEDGER.md#visible-importer-audit-for-2)

Eight paired code-mode controls also exposed a missed genuine importer. Both revisions resolve the crate root’s import when the target is `service.rs` and miss it when the same module lives in `service/mod.rs`. The candidate removes the unrelated fixture from every case. Recover the directory-module edge before promoting #2. On 5,127-file Moldwright the visible map still took 9.110 s through code mode and 7.812 s on the first embedded call. The two surfaces returned identical map bodies. On cetis.ceregister, which contains no Rust, the candidate's three-call median was 2.8–4.5% slower in both run orders. [Correctness controls, timings and parity](LEDGER.md#visible-importer-audit-for-2)

On a separate 5,127-file Moldwright restore, verifying each snapshot file against its source took 32.458 s and post-restore verification took another 15.991 s. The successful stdio response arrived 50.233 s after launch and included a function confirmed in the pinned TypeScript source. All work before the first proof, including reading and decoding the snapshot, took at most 0.94 s. Five standalone replays on the same snapshot later measured a median 14.1 ms to read it and 127.1 ms to verify its checksum and decode it, with a likely warm file cache. Prioritize the two source checks and retain detection of edits made between them. [Snapshot restore measurements](LEDGER.md#snapshot-restore-to-first-hit-for-7)

The first implementation is on `perf/compact-map-hidden-ranking` at `d53e429f`. On AAP the packaged CLI exceeded its 60 s deadline. The candidate returned the expected 4,345-file map in 69 ms wall time, with working code searches immediately before and after. Its rendered SHA-256 matches the completed baseline run on the pinned tree. Persistent code mode returned the first map in 11 ms. On moldwright the ranking is visible and the first candidate map still took 8.556 s. [Per-surface results and limits](LEDGER.md#measured-results-for-fix-1)

Gemini and Fable independently reviewed revision `806d762a`. Fable supplied its findings in chat. The timing in Fable’s review is its measurement; the replay below is ours. Both reviewers kept **#1 first** and identified the risk in predicting a visible row’s size before ranking. We adopt the narrower directory-prefix cutoff. A fixed directory count (roughly 60 in some examples) cannot replace the actual UTF-8 byte budget. Gemini supplied the exact separator, tuple-count and ellipsis traps; Fable supplied a real slow map where the key types were visible.

| Finding | Adjudication and effect on the plan |
|---|---|
| Visible ranking may reward spurious dependencies | Fable observed `OrderSnapshot` first with 487 importers in a 33.211 s SymForge depth-1 compact map. We reproduced that exact row and arrow on the pinned `3a4fb63d` clone, installed release, `SYMFORGE_NO_DAEMON=1`, `q repo_map --root <clone> -a detail=compact -a depth=1`: 1,765 files, 41.182 s wall / 41.125 s CLI, exit 0 (TC `01a20d60e4de776ba7c87e7a44c8d3e8`). Separately, the live `find_dependents(path="tests/fixtures/compression_ratio/rust/service.rs",limit=1000)` result attributed `benches/observed_refresh_gate_v1.rs:50` to that fixture, citing its import of `symforge::watcher::WatcherInfo`. The map assigns a **file-level** count to each type in that file. Searching for the struct name alone cannot establish the correct count. #2 now audits real direct and transitive file edges before tuning. At review time the cause was a hypothesis. The 25 November controls reproduced one false re-export and established that the fixture has zero genuine importers. #12 must wait for correct counts and observed repeated requests within a publication. [Dependent resolver](../../../src/live_index/query.rs#L3879-L3919) · [Re-export traversal](../../../src/live_index/query.rs#L4025-L4075) |
| Restore has a second read gate | Pending or running snapshot verification makes the index report loading. The second pass stats the corpus, spot-checks approximately 10% of stat-cleared resident files and performs further work on flagged files. That percentage does not describe the whole check or bound its cost. The moldwright trace combines decode and first proof in the ~12.5 s interval, then records 10.468 s of background verification. #7 measures both checks and decode against first stdio hit before selecting a fix. A configurable 15-minute post-session grace explains when an attach may become a restore. Session frequency and the claim that most sessions restore remain unmeasured. [Read gate](../../../src/live_index/health_view.rs#L374-L388) · [Verification sampling](../../../src/live_index/persist.rs#L3996-L4048) · [Daemon grace](../../../src/daemon.rs#L113-L121) |
| Watcher scheduling cannot explain the two measured scout times | On the five-second background handoff route, the scout finished in 3.621 and 4.733 s. Watcher activation followed session open. File admission skips the empty cold placeholder; the sweep still discovers entries and can overlap later phases. #3 has a smaller potential gain and no ordering edge to #8. Gemini added edit-to-visibility latency; Fable added checking whether each intervening read is current or explicitly pending reconciliation. Whether those reads currently say Ready remains an open runtime question. Directory proportions alone cannot prove time savings. Retire #8 as scoped and retain scout timing as an independent investigation. [Activation](../../../src/daemon.rs#L4629-L4643) · [Cold handoff](../../../src/daemon.rs#L5129-L5141) · [Watcher admission](../../../src/watcher/mod.rs#L39-L49) |
| Code and doc deadlines have capability prerequisites | The measured full scout already exceeds 3 s. #9 needs a permitted partial generation and typed code coverage. On the current daemon route the deferred bridge follows code publication; a base arriving at 8 s cannot feed a 5 s doc answer. #11 needs early admitted source and an authority path able to release each item safely, plus #6. #10 can shorten the later bridge on AAP; the measured moldwright bridge was only 92 ms. Its sorted admission order matters when budgets bind. Preserve that order, prevent code-hit regressions, and check all early evidence against the authorized serial set. [Publish sequence](../../../src/live_index/store.rs#L5817-L5832) · [Authority gate](../../../src/live_index/knowledge_retrieve.rs#L497-L509) · [Bridge admission order](../../../src/live_index/knowledge_bridge.rs#L446-L505) |
| The CLI and the agent have different waits | A CLI session can get the five-second loading refusal. The deferred stdio front end can hold ordinary calls up to 25 s while diagnostic calls can answer during startup. #4 and #6 now follow #1 in priority. Accept on actual stdio with concurrent status and search, preserving separate CLI receipts. #5 must account for the four advertised code-mode names and 4,800-byte listing limit. [Stdio wait](../../../src/protocol/deferred_stdio.rs#L42-L50) · [Dispatch](../../../src/protocol/deferred_stdio.rs#L219-L249) · [Surface test](../../../tests/codemode_mcp_surface.rs#L69-L89) |

## Ranked pull requests

Numbers are stable proposal IDs. Start with #1. Bring #6 and #4 forward for client-visible readiness, audit #2 before optimizing visible ranks, and split #7’s clocks before prioritizing it against #3. The [implementation ledger](LEDGER.md) tracks prerequisites and proof. Estimated gains are hypotheses or bounds on removed work. Only baseline timings in this report have been measured. Stop after the acceptance targets hold on the frozen workloads.

| Rank | One change | Expected gain and cost | Main risk and proof required |
|---|---|---|---|
| 1 | Skip dependency ranking when the existing directory rows already push **every key-type line** beyond the compact-map byte budget. Count eligible types only as far as needed to reproduce the omitted-line count. Use the existing budget and formatting routines. | Removes all candidate-by-reference scans for maps shaped like the observed SymForge root. Small formatter change. Visible-key and boundary cases keep the current path. | An off-by-one could alter the footer or hide a type. Compare the entire old/new rendered bytes, counts and ordering just below, at, and above the boundary, including zero/fewer than 15/exactly 15 types. Preserve the last directory row’s newline charge when types follow, first-overlong-row behavior and both truncation notices. Run first and repeated queries on SymForge and AAP. |
| 2 | Establish the correct distinct importing files for the top 15 visible rows. Correct the demonstrated false edge once a failing fixture isolates the resolver path. | Corrects the context an agent receives. Preparing match strings once remains a separate performance candidate after correctness. | The arrow counts importers of the containing file. Verify actual file-level edges, aliases, same-stem modules, qualified calls and valid re-exports using an independent oracle. Text search assists the audit but can miss indirect imports. Check each intentional count/order change, then remeasure cetis.ceregister and SymForge depth 1. |
| 3 | Queue the watcher’s mandatory first reconciliation behind the active initial reload on an empty project. Register the watcher and retain its change/overflow events immediately. Keep the existing post-registration reconciliation and run it as soon as the base load settles. | May reduce competition during later portions of the measured 8–13 s cold path. In both traced five-second handoffs the scout finished before watcher activation. Medium. Both traces showed a full sweep finding zero current paths. The saved time is unknown until A/B; this alone cannot promise 3 s. | Preserve the mandatory sweep, startup failure recovery, and Starting state until event handling is actually available. During a real cold load, create, rename, edit and delete files before and after registration, force overflow, then compare first and settled answers to disk. Time each mid-load edit until the changed symbol becomes queryable or the obsolete path disappears. Reject if visibility or recovery materially slows, edits are lost, or end-to-hit latency fails to improve. |
| 4 | Expose cold-load progress in `status` and the loading refusal. Reuse `ReloadProgressSink`'s discovered/parsed counters, register it for the daemon cold job, and report phase and observed counts from the same project and attempt. Publish bridge/authority state separately. Report an unknown total until scouting finishes. | A bot can tell whether to wait, query code, or query docs. Small/medium. No claim of shorter indexing time. | Discovered, skipped and parsed have different denominators. Never promise 100% before the corresponding view is queryable. Sample status through two live cold sessions, an empty repo, cancellation and a failed load. Give retry guidance while an ETA is uncalibrated. |
| 5 | Add a narrow **bind** control for unbound local code-mode stdio. Take an explicit approved repository path outside the Python sandbox. Route through validated root admission and the existing session open/attach path. A replay of the same root returns the same project and current progress. | Removes the restart/configuration round trip for a roots-less client. Medium. Existing configured clients retain their startup flow. | Preserve operator allowlists, raw/canonical sensitive-root checks, physical-root identity, state placement, idempotency and session isolation. No protected override or arbitrary-path lookup in a read call. Prove the actual home-launched session can bind, find a repo-specific symbol and doc, and returns the same project ID on replay. A disallowed root must leave it unbound. Keep HTTP single-/multi-project restrictions. |
| 6 | Expose typed knowledge readiness/coverage with existing code-mode search results and pin it to the source generation that produced the hits. Keep existing fields and rendered evidence. | Makes first answers useful to LLMs without parsing prose. Small. Does not itself produce a doc hit sooner. | Exercise useful partial hit, partial miss, complete miss, stale and withheld evidence. Complete results must match the current hit IDs, byte ranges, provenance, ranking and visibility on the same revision. |
| 7 | Shorten same-root snapshot restore to first correct hit. Measure snapshot read/decode, anchored per-file proof and the post-restore verification gate independently before selecting a change. | The measured client request waited about 24 s. Establish attach/restore frequency before ranking this against fresh indexing. | Preserve offline-change detection, quarantine and deterministic results for incompatible, changed, deleted, symlinked or root-replaced files. Compare first useful stdio hit and peak bytes on the same snapshot. |

**8. Retire the non-file scout shortcut as scoped.** Both measured scouts exceed the full 3 s code budget. Rejecting known non-files before canonicalization removes one operation per non-file. Directory count does not bound the time spent resolving directories. No isolated timing justifies this change as the solution. Measure walk and entry costs independently of #3 if scout work is revisited.

**9. Decide partial code publication now.** The observed scout alone missed 3 s on both roots. The normal code path publishes a completed generation by swap. An early file-local symbol response needs a safe partial publication with individually admitted, hash-verified definitions, generation identity and typed coverage. Establish whether these can be available before full scouting. Validate a distinctive positive hit and an inconclusive miss on a fresh stdio client while preserving complete and cross-file results.

**10. Parallelize document extraction on a bridge-limited repository.** A medium-sized change can parallelize only per-file candidate extraction in bounded batches of sorted eligible paths on the existing worker pool. Merge in path order, retain global card/candidate limits and the second-stage resolution against the same content generation. This can reduce elapsed extraction time. It cannot remove authority time or provide verified links before the symbol index exists. Measure peak committed bytes as well as wall time, bound pending work by bytes, and release decoded buffers after each batch. Reject the change if first verified code-hit latency worsens beyond measured baseline variation on either AAP or moldwright.

**11. Offer early doc search once the required source and authority can publish in time.** Scope this medium-sized change to one doc search operation. It could admit already screened sections before all cross-file links settle, improving time to first useful evidence on AAP. Publish exact path/hash/span and provisional coverage together. Keep authority-dependent or withheld evidence off that path until it is safe to publish. Capture all early responses. Each disclosed evidence item must occur byte-for-byte in the fully authorized set for the same query and source generation. Compare at the evidence level before top-k truncation, so later ranking changes can displace an authorized hit. A later-withheld item appearing even once fails the check. Compare final hit order, evidence, link identities and source bytes to a complete serial build.

**12. Hold count reuse until counts are correct and repeat use is measured.** The formatter already computes each candidate file’s distinct importer count once per request. Measure calls per immutable publication, scope overlap, and actual repeated lookups. If overlap is substantial, try a bounded, lazy cache for that count owned by the immutable dependency view. Keep scope selection, churn-based ordering and rendering live. A hit removes one full dependent lookup. The first lookup still pays in full. Cost is one small read-path PR if the view can own the cache. Prove byte-identical output on overlapping scopes, then change an importer outside both scopes, remove a re-export, switch roots and issue simultaneous reads during publication. Compare counts against the uncached resolver, first-map latency, repeated-map latency and peak memory. Drop this option if safe reuse requires a new storage layer or the measured overlap is low. [Existing per-request counts](../../../src/index_lifecycle/guidance/read_context.rs#L587-L603)

**Boundary for PR 1.** Use the existing budget algorithm on the prefix through the last directory row. With at least one eligible type, every prefix item pays its UTF-8 length plus one newline. Skip only when the cut is inside that prefix or the following empty separator cannot fit. At 3,999 prefix bytes the separator still fits; at 4,000 it does not. Preserve forced inclusion of an overlong first item. A predicted first type-line length is unsafe because sorting can change the row and add a dependency-count annotation. For `n > 0` eligible top-level type tuples, the full section has `2 + min(n, 15) + int(n >= 15)` lines. Exactly 15 emits an ellipsis. Multiple tuples may share a path; zero tuples produce no section. [Budget loop and formatter](../../../src/index_lifecycle/guidance/read_context.rs#L339-L370)

The native samples establish where the worker spent time during capture. They leave the removable fraction of whole-request latency unmeasured. In PR 1, time dependent counting and sorting separately from total formatter and end-to-end wall time on both pinned roots. If a map that skips ranking still misses 2 s, profile the remaining path before considering any ranking cache. [Warm profiles](#windows-sampling-and-replay)

## Point-in-time benchmark

Windows 11 Pro build 26300, Core Ultra 7 265, 20 logical processors, 32 GB host. The research checkout was fast-forwarded to `3a4fb63d0094c9bf50dfe1261be9b0a86641f7ad`. The installed Windows binary reported 12.1.0 and SHA-256 `E79435EDA295596C8719F388453FAFCBEBFD917D6CAC2338A0E507B27F372D5E`. The checkout pins Rust 1.100.0. `git ls-files` counted 1,892 tracked paths in this revision. Counts served by the index describe its own publication and eligibility rules.

### SymForge querying its own repository

Private replay material on this benchmark host is in `E:/project/symforge-ops/2026-11-24-index-speed/`. From that directory run `uv run --no-project python sf-index-speed-mcp-baseline.py bound` or `unbound`. Run `sf-index-speed-mechanism.py` the same way for the controlled map experiment. It creates a fresh fixture. The report includes the protocol and fixture setup for use elsewhere.

One persistent stdio MCP connection, warm resident project. Launch the binary from the user's home directory with `SYMFORGE_SURFACE=code` and `SYMFORGE_WORKSPACE_ROOT=E:/project/symforge`. Send MCP `initialize` (version `2025-06-18`, empty capabilities), `notifications/initialized`, then `tools/list`. Submit each expression below to `tools/call` with `name=code_execute` and the expression as `arguments.code`. Clocks are wall time around the individual RPC and `duration_ms` inside its JSON result. Rows are single calls unless repeated explicitly.

| Expression on SymForge | Wall / internal, ms | Observed result |
|---|---:|---|
| `[(x["name"],x["path"]) for x in search_symbols(query="repo_map_text_scoped",limit=1)]`, first read on the new connection | 125 / 45 | Exact function in `src/index_lifecycle/guidance/read_context.rs` |
| Same expression three more times | 12 / 11, 10 / 10, 13 / 12 | Same name and path each time |
| `repo_map(detail="tree",path="src/cli",depth=1)["total_files"]` | 2 / 1 | 18 indexed files in scope |
| `search_knowledge(query="checkpoint_now",limit=5)["hit_count"]` | 491 / 490 | 5 hits. This checks hit count only. |

Between the first and repeated reads, `status` reported root `E:/project/symforge`, generation 197, 1,766 files, 63,925 symbols, `snapshot_restore`, and **Ready (stale)**. Retain that trust label in comparisons. Earlier, the same MCP index missed the newly arrived `src/cli/code.rs`; source citations below use the Git revision. A request for status made immediately after initialization in a separate probe did not yet carry project evidence. The successful probe waited for a real read before checking it. Terminal Commander job `01a20cddc951778bb9987364a273fda1`.

Two separate CLI processes targeting SymForge while its MCP index was resident returned the same function. Run the following twice in PowerShell. The CLI's `duration_ms` was 67 and 50 ms respectively, with 195 ms wall time for each launch through JSON decoding. The success envelope does not identify the backend. A failed attach can silently select the local reader, so these CLI timings alone do not prove reuse. The brief observed 2–3 s of additional CLI time on other runs. Use measured wall time each run. In `src/cli/code.rs::run_query_cli`, the internal clock includes backend attachment/open, dispatch and best-effort session close. It starts after root resolution and ends before output serialization. Job `01a20cd9cda47388987b95cc1026b9f6`.

```powershell
$exe = Join-Path $env:USERPROFILE '.npm-global/node_modules/symforge/node_modules/symforge-windows-x64/bin/symforge.exe'
$root = 'E:/project/symforge'
$watch = [Diagnostics.Stopwatch]::StartNew()
$result = & $exe q search_symbols --root $root -a query=repo_map_text_scoped -a limit=1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $result.value[0].path -ne 'src/index_lifecycle/guidance/read_context.rs') { throw 'wrong result' }
$result.duration_ms
$watch.ElapsedMilliseconds
```

### Earlier measurements supplied with the brief

Windows installed `symforge.exe q <op> --root <repo>`. These are the owner's observations from the same day. The original search strings, SHAs, per-attempt envelopes and some counts were not recorded in the brief. The command forms below reproduce the workloads, but exact replay of those missing inputs is impossible. Use the fully specified self-benchmark above and the fresh profiles below for comparisons.

| Warm, already indexed | Git files | `q search_symbols` | `q search_knowledge` |
|---|---:|---:|---:|
| Agent_Army_Professionals (AAP) | 5,082 | 78–118 ms | 797 ms |
| cetis.ceregister | 794 | 31–38 ms | 32 ms |
| moldwright | 5,536 | 160–205 ms | 156 ms |

| First use, previously unindexed | Markdown files | Code query | Doc query | Warm symbol query |
|---|---:|---:|---:|---:|
| Citadel | 50 | 3.7 s | ~5.6 s | 27 ms |
| overwatch | 1 | 4.1 s | ~11 s | 14 ms |
| assay | unknown | 547 ms first query | unknown | 55 ms |
| testpilot | unknown | 7.3 s to a loading response | unknown | 21 ms |

| Compact map, `& $exe q repo_map --root $root -a detail=compact` | Git files | First readiness probe including retries | Second call |
|---|---:|---:|---:|
| SymForge | 1,847 | 59.5 s | 35.5 s |
| AAP | 5,082 | 73 s to daemon error | 60.0 s, success unverified |
| moldwright | 5,536 | still loading at 120 s, 33 attempts | 26 ms |
| cetis.ceregister | 794 | 14.3 s | 9.1 s |
| ce-registry-pro | 2,051 | 12.6 s | 4.5 s |
| terminal-commander | 1,085 | 8.7 s | 3.7 s |
| fathom | 629 | 6.5 s | 1.0 s |
| rtk | 287 | 2.4 s | 225 ms |

The first column of times includes attempts spaced by 0.5 s and has a 120 s stop. Those times cannot isolate parsing, daemon startup, or rendering. The large difference between AAP and moldwright at similar tracked-file counts argues for recording admitted files, reference density, candidate type paths and output coverage. Valid map details are `compact`, `full`, and `tree`. `low` is refused.

## Why compact maps cost seconds

### Which slow roots actually show key types?

The Gemini review identified three candidates for measuring PR 2. A replay on 24 November showed that only **cetis.ceregister** displayed a ranked type row at the root. The other two are additional PR 1 cases. Each pair below came from one warmed, local code-mode stdio session on a clean clone of the stated commit. Both calls returned the same rendered SHA-256 within each pair.

| Root and revision | Indexed files | Two compact-map RPC wall times | Returned key-type section |
|---|---:|---:|---|
| cetis.ceregister `d2cd33ea` | 766 | 8.647 / 8.428 s | Visible type rows |
| ce-registry-pro `a50fe3fc` | 1,732 | 4.432 / 4.265 s | All 18 lines omitted |
| terminal-commander `a95e6843` | 1,039 | 3.219 / 3.170 s | All 18 lines omitted |

Replay with `uv run --no-project python visible-roots-review.py` in the private evidence directory, TC job `01a20d584d5870388712d528373d62ff`, exit 0. The driver used the installed release with `SYMFORGE_NO_DAEMON=1`, verified a nonempty tree and the bound root, then issued `repo_map(detail="compact")` twice. It checked the returned type rows and omission footer. These two calls establish workload shape and baseline latency; use the full alternating series for a speedup claim.

### Ranking cost on the two large-map cases

On the installed executable, `& $exe q repo_map --root E:/project/symforge -a detail=compact` returned `status=ok` and 1,749 indexed files. The JSON reported **35,436 ms**, and Terminal Commander measured **35,745 ms** for the process (exit 0, job `01a20cd96338734cab2bec6aa9f1b9c3`). The payload included the budget notice. This is a successful warm-project call; `q` does not identify the lane in its success envelope.

Both the code-mode MCP and `q repo_map` reach `execute_op_on_generation` in [the code backend](../../../src/codemode/backend.rs). A daemon request dispatches that work on the blocking pool. Full and tree read the published outline. Compact calls [`repo_map_text_scoped`](../../../src/index_lifecycle/guidance/read_context.rs), which groups directory/language counts, collects top-level key types, ranks them, then applies a 4,000-byte rendering budget. The legacy MCP handler can also append knowledge digests. The code-mode path under test does not append them.

For every distinct path contributing a key type, the formatter invokes [`find_dependents_for_file`](../../../src/live_index/query.rs). Each invocation builds symbol-name sets, visits other files and their references, matches imports and eligible type/call references, then the caller deduplicates by importing file. It also checks qualified calls without imports and follows Rust re-exports up to two hops. The path memo lives for one map call. Thus even a warm index repeats work proportional to candidate paths times the corpus scanned. The final ranking also uses churn, path, symbol name and kind. The byte cap and `max_files` do not bound this ranking work. In the reproduced SymForge map, the output itself said 47 directory rows and 18 lines of the key-types section were omitted. All 18 possible lines (separator, heading, 15 types, ellipsis) came after the cutoff.

**Controlled check of the ranking mechanism.** On one local, no-daemon MCP connection, query two scopes of the same 257-file synthetic Rust repository. Both scopes contain 128 files, 128 functions, 128 types, and the same imports. Only the distribution of types changes. With all types in one candidate path, two warm scoped maps took **1, 1 ms** wall. With one type in each candidate path, three warm maps took **79, 74, 74 ms**. The first map on the new process took 1,123 ms wall including startup, 1 ms internally. Every response returned 128 scoped files and a visible key-types section, and the final status root matched the fixture. This experiment isolates the multiplier. It does not extrapolate synthetic timings to AAP. TC job `01a20ce1d16b73b5bfdfad5f078d76e9`.

Recreate the fixture in an empty directory with the following Python, then run the same stdio session as the self-benchmark, setting `SYMFORGE_NO_DAEMON=1` and `SYMFORGE_WORKSPACE_ROOT` to the fixture. For each scope in order `together, apart, apart, together, together, apart`, submit `m=repo_map(detail="compact",path="<scope>")` followed on a new line by `[m["total_files"], "Key types:" in m["rendered"]]`. Require `[128,true]` and the fixture root in status.

```python
from pathlib import Path
import subprocess

Path("shared.rs").write_text("pub struct Shared;\n", encoding="utf-8")
for lane in ("together", "apart"):
    Path(lane).mkdir()
    for i in range(128):
        ids = [i] if lane == "apart" else (range(128) if i == 0 else [])
        source = "use crate::shared::Shared;\n" + f"pub fn f{i}(_: Shared) {{}}\n"
        source += "".join(f"pub struct T{j};\n" for j in ids)
        (Path(lane) / f"m{i:03}.rs").write_text(source, encoding="utf-8")
subprocess.run(["git", "init", "-q"], check=True)
subprocess.run(["git", "add", "."], check=True)
```

The production AAP error is consistent with the query HTTP deadline. `DaemonSessionClient::new_with_auth_token` constructs the per-session client with a 60 s timeout, and `call_code_query` adds the message `calling daemon code-query` to send failures. The session-open request has a separate budget. The server schedules non-abortable work, so a client retry after expiration can overlap the original request. The error string alone cannot distinguish a deadline from another transport failure or prove when the server finished. Extending the deadline would leave the two-second target unmet.

**Reproduced on a ready AAP daemon.** On a fresh pinned clone, wait for a nonempty bridge publication, then call the installed release against the same private daemon and explicit root. A known code search found the expected path in 334 ms wall (58 ms inside the CLI). The next compact map failed after **60.090 s wall / 60.048 s internal**, exit 1, with `daemon_transport` and `calling daemon code-query`. The same code hit returned immediately afterward in 107 ms wall (61 ms internal). This reproduces the transport boundary on a working index. The client omitted the underlying timeout reason. Replay `uv run --no-project python daemon-deadline.py`, which runs `q search_symbols --root $copy -a query=main -a limit=50` before and after `q repo_map --root $copy -a detail=compact`. TC job `01a20d12146b773abc8bdd2d73f2ff68`.

**Same output, different cost.** A second pair of 257-file fixtures put the 128 candidate files in a single `core/` directory and added 128 identical directory rows to both maps. This pushed the key types out of view. Two warm unscoped calls per variant took **2 and 1 ms** with types together, **34 and 36 ms** with types apart. All four complete rendered strings, including the 18-line omission notice, had SHA-256 `04f8edece62279c7976a61a10c4aa50f76b23f55b223ef848b9c7a5b80c7d903`. This is an example where expensive inputs cannot affect any byte the agent receives. It is a baseline for PR 1; no proposed optimization ran. Replay with `uv run --no-project python sf-index-speed-omitted.py together` and `apart` in the private evidence directory. TC jobs `01a20cf4b459741f9c6734107fd11487` and `01a20cf4be6f73dbbcf363fab44eb275`.

**First compact map on a fresh checkout.** Using the installed binary with `SYMFORGE_NO_DAEMON=1` bypassed the per-session HTTP deadline and returned a full response on each pinned root. Wall time starts immediately before spawning `q` and ends on process exit. Both cases include local index construction and rendering. OS file caches were uncontrolled and a Cargo build was active. In a fresh private control directory set `SYMFORGE_HOME` to that directory and `SYMFORGE_WORKSPACE_ROOT` to the new checkout. Run `& $exe q repo_map --root $copy -a detail=compact` once. On this host the complete replay is `uv run --no-project python clean-map.py`, job `01a20d064bae7010821938298db2a437`, exit 0. It checks out the requested revision and verifies absence of `.symforge`, success, indexed count and rendered text.

| Clean checkout | Wall / CLI clock | Indexed files | Omitted directory / key-type lines | SHA-256 of returned UTF-8 map |
|---|---:|---:|---:|---|
| SymForge `3a4fb63d` | 39.757 / 39.670 s | 1,765 | 50 / 18 | `fa1b89ca21b29a431c502cdaa67faee93bb2bfbbd0a07d2abfcbca720c0530be` |
| AAP `ab63525a` | 111.054 / 110.981 s | 4,345 | 15 / 18 | `edbe8b018b05dbb89503a7dc568f306f2fff21fd8d966115db0080804abfef56` |

### Windows sampling and replay

Very Sleepy 0.91 recorded the installed release on both repositories. Its official installer had a valid Authenticode signature for Vladimir Panteleev and SHA-256 `88d3644afb449f6965ebd9b863a4bd37b3175748fe726ca20a9f638fb0906eda`. The SymForge capture contained 15,425 samples in a 75.031 s recording window, AAP 18,372 in 90.047 s. The recording windows include startup and waiting and do not measure query latency. The shipped binary lacks matching debug symbols. Absolute addresses and nearest exported names in these two recordings cannot identify the Rust bottleneck. Reproducible distribution captures in the private evidence directory are `symforge-map-stripped-under-build.sleepy` (SHA-256 prefix `a7070c3b637edb8e`) and `aap-map-stripped-under-build.sleepy` (`b827ff57e27b88e6`). [Very Sleepy release](https://github.com/VerySleepy/verysleepy/releases/tag/v0.91)

To resolve Rust stacks, build the same revision on the pinned MSVC toolchain. A build with the shipping LTO/codegen settings and debug symbols exhausted memory on this 32 GB host. The following diagnostic build succeeded, job `01a20cf3fa2372f59d46f0aed1adcf88`. Its executable SHA-256 was `9ae3f8f2e0115e7fb837465943aafad5ec97b5b919bddca97a1023b2dee15b6e` and its PDB SHA-256 was `e4c9d7f27faad733e5bc91ba6850bddd705238e587b7b1b6ac63921c40c26d42`. The code-generation settings affect inlining and absolute speed. Use this pair for stack attribution and the shipping release for service-level timings.

```powershell
$env:CARGO_TARGET_DIR = 'C:/Target/symforge/symforge-profiler/target'
$env:CARGO_PROFILE_RELEASE_DEBUG = '1'
$env:CARGO_PROFILE_RELEASE_STRIP = 'false'
$env:CARGO_PROFILE_RELEASE_LTO = 'false'
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '8'
cargo build --release --locked --bin symforge
```

**Warm SymForge CPU sample.** On a local code-mode stdio server, verify the source symbol before dispatching `repo_map(detail="compact")`. Attach Very Sleepy to that already-running process while the map is executing. Of 7.183 weighted seconds with an instruction pointer in the executable, 6.002 included both `repo_map_text_scoped` and `find_dependents_for_file`, and 5.074 included `matches_target_import`. Sampled leaves included the matcher, string formatting, string writes and buffer growth. This locates repeated target matching beneath dependency ranking. The capture contains 3,468 samples across all threads, including parked threads. Weighted executable-leaf time is a selected denominator, not the percentage of total process CPU. This diagnostic code-mode call exhausted its 30 s interpreter budget and returned a timeout after 34.25 s. It produced a valid CPU sample but no map payload. File `symforge-warm-nodaemon.sleepy`, profiler job `01a20d14c25872a9882621d03bfc390b`.

**Warm AAP CPU sample.** The same no-daemon code-mode protocol on the pinned AAP clone produced 4,238 samples. All 2.214 weighted executable-leaf seconds included both the compact formatter and its dependent scan, and 1.713 included the import matcher. String formatting, pattern matching and hashing appeared below the ranking path. The AAP CPU capture succeeded (profiler job `01a20d18f8d8707ea338fa406dd8e443`); the sampled MCP request had produced no valid map payload after 73 s. Its driver was cancelled and cleaned up at 97.899 s from process start (job `01a20d189a11729e85ab1552ee86e681`). These samples locate work PR 1 can skip and motivate matcher tuning after the visible-count audit. Neither sampled proportion is a predicted speedup. The production daemon failure and the complete local release result above measure what a client received.

Replay either private `symforge-warm-profile.py` or `aap-warm-profile.py` with `uv run --no-project python <driver> <pinned-clone>`. Both launch the diagnostic executable with its matching PDB, set `SYMFORGE_NO_DAEMON=1` and `SYMFORGE_SURFACE=code`, and run a marker search on the same session. On `MAP_DISPATCHED`, attach to the printed **server PID** with `sleepy.exe /q /a:<pid> /t:8 /o:<new-file.sleepy>`. Inspect the ZIP's symbol dictionary and weighted call stacks with `uv run --no-project python profile-summary.py <new-file.sleepy>`. The archived no-daemon captures have SHA-256 prefixes `3465032dd2c7941d` (SymForge) and `25c655dcf3c80268` (AAP).

A separate named private-daemon SymForge capture also returned the full map successfully with 1,765 indexed files in 38.507 s wall and 38.458 s inside the CLI. Its complete map hash matched the release cold map at the same revision. Of 3.195 weighted executable-leaf seconds, 2.841 included the compact formatter and dependent scan. This is a build with different code generation and an active profiler. The paired harness later failed its AAP pre-query marker assertion, so only its verified SymForge result is used. TC job `01a20d17364f720a9d582adc1cca33e7`, first result.

A later instrumented run measured complete ranking intervals on a single published generation per root. On AAP the original algorithm made 899 dependent lookups in 107.729 s. Fix #1 made zero and returned the same bytes in 5.441 ms of formatter time. On SymForge the original made 335 lookups in 37.588 s, and #1 returned the same bytes in 3.306 ms. The visible controls still ran the ranking. [Same-generation evidence and independent client timings](LEDGER.md#measured-results-for-fix-1)

## Cold phase profiles on the two large repositories

**When did a client get useful answers?** In a second pair of fresh clones, one driver started each private daemon and set a monotonic clock immediately before the first code query. Separate waiting clients dispatched real queries on the base-load and nonzero-card bridge events. No manual delay or retry interval separates those events and requests. The first query on each root returned `index_loading`. Every reported success had exit 0 and matched an independently checked source or tracked document. These are first *observed* useful answers at the specified event boundaries, with OS file caches uncontrolled and a concurrent build.

| Fresh clone | First loading reply | Base / bridge event after first dispatch | Verified code hit | Verified doc hit |
|---|---:|---:|---:|---:|
| moldwright | 5.114 s | 7.497 / 7.497 s | **7.814 s** · `apps/engine/src/cli/bin.ts#main` | **7.814 s** · `.claude/skills/i18n-strings/SKILL.md` |
| AAP | 5.289 s | 7.876 / 11.496 s | **8.035 s** · `00_brief/design/topics/tools/build-tokens.py#main` | **11.946 s** · `.agent/execplans/HANDOFF-2026-08-12-MC20-D041.md` |

The AAP code answer arrived 3.46 s before its bridge event. This directly demonstrates the early-code behavior. Both code and doc deadlines were missed on both roots. Replay from the private evidence directory with `uv run --no-project python first-hit.py moldwright Agent_Army_Professionals`. It issues `q search_symbols -a query=main -a limit=50` and `q search_knowledge -a query=<Moldwright or architecture> -a max_tokens=3000`, each with the explicit clone root. Full paired run, TC job `01a20cfdf2d472cd97d3167c0fe2989f`.

These are elapsed spans emitted by the running Windows daemon. Both checkouts were clean local clones with an empty project state directory at first use. Each used its own control directory and foreground daemon. Their logs identify the exact clone in both the build-start and completion events. The timed client calls ran through Terminal Commander's `run_and_watch`. A release build for symbolization overlapped part of this investigation, so use the samples to locate work and repeat under controlled load before evaluating an optimization.

| From `LiveIndex::build_reload_data starting`, unless marked duration | moldwright, `3d172845` | AAP, `ab63525a` |
|---|---:|---:|
| Tracked files / tracked Markdown, `git ls-files` | 5,535 / 137 | 5,081 / 1,736 |
| Scout complete, elapsed | 4.733 s | 3.621 s |
| Manifest entries / executable scout work set | 5,535 / 5,324 | 5,071 / 4,542 |
| Staged / terminal in that work set / symbols | 5,127 / 197 / 106,045 | 4,345 / 197 / 210,799 |
| Worker pool initialized to admission+parse report, interval upper bound | 6.544 s | 3.702 s |
| Derived trigram build, duration | 358 ms | 692 ms |
| Reverse and path indices, duration | 83 ms | 86 ms |
| Reload data built, elapsed | 12.094 s | 8.439 s |
| Background cold-load completion, elapsed | 12.901 s | 9.042 s |
| Knowledge bridge, cards / duration | 1,958 / 92 ms | 21,370 / 3.783 s |
| Authority, duration | 40 ms | 513 ms |
| Bridge publication observed, elapsed | 12.901 s | 13.097 s |
| Daemon acknowledged first snapshot written, elapsed / bytes | 13.617 s / 45,891,867 | 10.066 s / 98,578,812 |

Rows overlap. In particular, the watcher was already running its first reconciliation during the initial build. Its first sweep reported 5,535 and 5,071 admitted respectively, with zero already current. That establishes concurrent work but does not measure its CPU share or prove it blocked publication. The AAP snapshot finished before bridge publication. Git temporal work ran in the background. Neither trace isolates parser CPU from read/admission time. The five-second session-open allowance returned a loading placeholder on both roots; preparing reload data continued independently. Time between starting the daemon and issuing the first query includes an operator gap and is excluded.

**Reproduction.** Use a fresh local clone per cold run outside any source tree. With `$exe` defined above, set `$source` to the named original checkout, `$copy` to a new clone path, `$control` to a new private directory. For the recorded samples the clones were `C:/Target/symforge/index-speed/moldwright-daemon` and `aap-daemon` under the same parent. Both trace and client processes inherited these environment settings. Run the daemon in a separate foreground Terminal Commander job and retain the phase lines from stderr.

```powershell
git clone --local --no-hardlinks --quiet $source $copy
$revision = if ((Split-Path -Leaf $source) -eq 'moldwright') { '3d172845ab17f6077db19061d22973462522c91a' } else { 'ab63525a45656f4232f7cc49886e7a63347c9694' }
git -C $copy checkout --quiet --detach $revision
if ((git -C $copy rev-parse HEAD) -ne $revision) { throw 'wrong revision' }
if (Test-Path -LiteralPath (Join-Path $copy '.symforge')) { throw 'expected a fresh clone' }
$env:SYMFORGE_HOME = $control
$env:SYMFORGE_WORKSPACE_ROOT = $copy
$env:SYMFORGE_DAEMON_IDLE_SHUTDOWN_SECS = '60'
$env:RUST_LOG = 'info'
Set-Location -LiteralPath $copy
& $exe daemon
```

In a second process with the same environment, run `& $exe q search_symbols --root $copy -a query=main -a limit=5` immediately, then again after the base load completes. Run `& $exe q search_knowledge --root $copy -a query=architecture` for AAP or `-a query=Moldwright` for moldwright after bridge publication. The first calls returned `index_loading` at 5,209 and 5,070 ms respectively in the CLI envelope. The repeated code queries returned five real AAP hits including `00_brief/design/topics/tools/build-tokens.py#main` (256 ms), and moldwright results under `apps/desktop-app/` (46 ms). AAP's 554 ms doc query returned ten hits including the tracked `.agent/execplans/HANDOFF-2026-08-12-MC20-D041.md`. Moldwright returned the tracked `.claude/skills/i18n-strings/SKILL.md`. The repeated requests in this series were issued manually; their service durations are not time-to-first-hit from cold open. A preliminary shared-daemon run lacked sufficient root evidence and is excluded.

**Native cold CPU samples.** On two additional fresh, pinned clones, attach Very Sleepy to the diagnostic daemon 70 ms after its indexing pool initialization event. Record all existing threads for six seconds with `sleepy.exe /q /a:<daemon-pid> /t:6 /o:<path>`. After publication, require the same independently known code hit as the first-hit driver. Both passed. Replay `uv run --no-project python cold-stacks.py` and inspect each printed per-run archive path with `uv run --no-project python profile-summary.py <capture>`. Terminal Commander job `01a20d10f817743ab296eb89fdf0b6dd`, exit 0.

| Fresh daemon | Recording | What the symbolized stacks establish |
|---|---:|---|
| moldwright | 1,914 samples / 6.140 s | Tree-sitter stack-node allocation ran on a Rayon worker while the reload also spent samples building trigrams and sorting. |
| AAP | 1,980 samples / 6.063 s | Admission and parsing ran on Rayon workers. Sampled leaves included Tree-sitter cursor traversal, query advancement and lexing, source secret scanning, and trigram construction. |

These windows start after scouting and deliberately span multiple phases. Sampling every thread includes parked workers. The phase table supplies elapsed costs. The stacks confirm real parser and derived-index work and show that moving Markdown to a dedicated pool would still leave both on the code path.

Terminal Commander receipts, all IDs prefixed by `job_`:

| Repo | Foreground phase trace | First code request | Confirmed code result |
|---|---|---|---|
| moldwright | `01a20cde6f7f7538a2debfbb171afc68` | `01a20cde93d473f0832362630f63b4ef` | `01a20cdf3a3277e28419aba524616011` |
| AAP | `01a20ce14137729d858ad8c47d67b0a7` | `01a20ce16d3777d0a6707785c8eea1cc` | `01a20ce1bd7074c5859568d617d25993` |

The foreground daemon jobs were stopped after the observations. Read the timed events and query returns as evidence. AAP doc response `01a20ce1ff087010962201f29d4804c5`; tracked-path check `01a20ce44fa170f7b873c790e20fbdab`.

## Publication, reuse and workspace selection

**Can code serve before docs?** Yes, on the daemon's cold session-open path. The scout admits files and Rayon reads and parses admitted entries, including eligible Markdown. Building the trigram, reverse and path indexes and publishing the first code generation still stand before normal code reads. The first content publication can carry a loading knowledge bridge. A worker builds cards and links, then builds authority and attempts a fenced publication. It can retry if the publication changed and can leave partial coverage on failure. Regular reloads and every startup route need their own measurements. See `src/daemon.rs::ColdLoadJob::open_within`, `src/live_index/store.rs::{admit_and_parse_entries,fill_deferred_knowledge_bridge}` and `src/live_index/knowledge_bridge.rs::build_knowledge_bridge`.

Knowledge work includes text decoding, heading cards, candidate links, code-symbol resolution, deterministic sorting, budgets and authority. Markdown count alone predicts little. The overwatch observation does not establish where the intervening seconds went. On a loading *code* generation, `execute_op_on_generation` refuses reads. After code publication, knowledge search can return a successful outer JSON object with zero hits and a *rendered* readiness or degraded-coverage explanation. The typed result supplies hit counts but lacks a dedicated coverage field. Preserve the distinction between a miss, incomplete coverage, and withheld evidence. See `src/codemode/backend.rs::{execute_op_on_generation,CodeReadOp::SearchKnowledge}` and `src/knowledge/search.rs::{lane_readiness_text,render_response}`.

**Will another session load the saved index?** Same daemon and canonical project means the session can join the existing project slot, including during its idle grace period. When the project is absent from memory, the daemon tries the admitted snapshot first. It rehydrates file records, loads a valid persisted trigram table or rebuilds it, reconstructs reverse/path indexes, publishes a usable view, and schedules verification against disk. Bridge and authority coverage can still be loading. Persisting after a successful cold build is separate from the session acknowledgement. An absent, incompatible or invalid snapshot follows the rebuild/quarantine policy. On the separate no-daemon CLI fallback, `open_read_only` performs verification in the caller, so repeated one-shot commands pay their own open costs. Keep disk restore and resident attach as separate benchmark cases. See `src/daemon.rs::{register_session_for_existing_project,bootstrap_project_index_within,restore_project_snapshot}`, `src/live_index/persist.rs::snapshot_to_live_index_with_code_signals_bound` and `src/codemode/backend.rs::LocalCodeBackend::open_read_only`.

**How can a session bind a repo?** The same installed executable launched from home with no workspace override, no `CLAUDE_PROJECT_DIR`, and no client roots returned `Empty`, zero files and `empty_bootstrap`. Its four advertised tools were `code_search`, `code_execute`, `status` and `symforge_edit`. Setting the validated workspace environment before launch gave the successful self-benchmark above. A CLI caller can supply `q ... --root <repo>` for that command. A capable MCP client can declare roots at initialization; the server can use a usable root under its existing resolution policy. An already unbound, roots-less code session lacks an advertised in-band bind. Its `project` parameter selects an opened project by ID or alias. HTTP serve separately supports preconfigured aliases and restricts `--allow-index-folder` to a single project. See `src/protocol/mod.rs::{fetch_client_root_uris,bind_declared_client_root}`, `src/protocol/surface_probe.rs`, `src/server/serve.rs` and `src/protocol/serve_projects.rs`. Home launch receipt `01a20cdd8896745fa7024d7259d122b1`.

**A separate process restored the fixture.** Close the fixture's first stdio process. With its 139,920-byte `.symforge/index.bin` in place, launch the same no-daemon stdio configuration a second time. Run `[(x["name"],x["path"]) for x in search_symbols(query="Shared",limit=1)]`, then `status`. The result was exactly `Shared` in `shared.rs` in 885 ms request wall time, 2 ms internal. Status confirmed the same physical root, `snapshot_restore`, 257 files, generation 6 and `Ready`. This proves restore independently of daemon residency on the small fixture. The 885 ms includes time the first request spent waiting for startup and excludes the earlier handshake. TC job `01a20ce39bee7428aa39cc9ac59b96ac`.

**A real 5,127-file restore was much slower.** Close the private moldwright daemon and launch a separate local stdio server with `SYMFORGE_NO_DAEMON=1` and `SYMFORGE_WORKSPACE_ROOT` set to the original clone containing its snapshot. From home, use the handshake above, call `[(x["name"],x["path"]) for x in search_symbols(query="main",limit=5)]`, and check status. Two successful first-request wall/internal pairs were **24,027/25 ms** and **23,478/21 ms**. Both found `main` in `apps/desktop-app/src/main/main-window.ts` and reported `Ready`, 5,127 files and `snapshot_restore`. An earlier request returned a non-JSON tool body and failed the harness assertion; its cause was not captured. Jobs `01a20ced0acf71d5930d66bf1f2f4516`, `01a20cee1952720f86fbb32a1d985bfa` and `01a20cec0a747257af581783d8708466` respectively. These runs overlapped the symbol build.

With `RUST_LOG=info`, the second process logged file-map rebuild at +12.462 s from launch, trigrams at +12.515 s, reverse/path indexes by +12.582 s, outline by +12.738 s and deferred knowledge by +13.139 s. Background verification finished at +23.207 s and reported 10.468 s duration. The first interval includes snapshot read/decode and **per-file source proof** before hydration. In `run_snapshot_store_restore_proof_bound`, the loader obtains an anchored read and checks source bytes against each recorded hash. The phase trace does not split that interval further or establish that every concurrent verifier blocks every transport. The no-daemon stdio path spawns verification; `q`'s local fallback calls its verifier synchronously. Both are different from attaching to an already resident daemon.

## What comparable tools publish

### Projects that build repository maps for coding agents

| Project | What its source does | What transfers to SymForge |
|---|---|---|
| **Aider** | Groups defining and referring files by identifier before building a weighted graph. It reuses per-file tags by modification time and can reuse a rendered map after an expensive run. On a cache miss it still ranks the tags before fitting them to the token budget. [Tag cache](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/repomap.py#L233-L262) · [Graph](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/repomap.py#L447-L525) · [Reuse and budget fitting](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/repomap.py#L585-L703) | Reuse work across requests. Key exact counts to the verified dependency view. A timestamp or unchanged set of filenames cannot establish that an importer still contains the same reference. |
| **mackenney/repo-mapper** (Rust) | Builds identifier-to-file tables in one pass over tags, then joins the referring and defining files for each shared identifier. Common identifiers can still produce many pairs. [Tag tables and edge construction](https://github.com/mackenney/repo-mapper/blob/5d0bea7e87d7fa51afc54ee158d45ddad0723acb/src/graph.rs#L17-L51) · [Join loop](https://github.com/mackenney/repo-mapper/blob/5d0bea7e87d7fa51afc54ee158d45ddad0723acb/src/graph.rs#L104-L142) | For a *first* visible ranking, this suggests using SymForge’s existing name-to-reference locations to shortlist qualified calls, then applying the current language and export predicates. A name-only join cannot establish imports or transitive re-exports. Profile this pass separately before changing it. [Existing postings](../../../src/live_index/store.rs#L6424-L6439) · [Qualified-call checks](../../../src/live_index/query.rs#L3970-L4013) |
| **Adulari/forge** (Rust) | Bulk indexing recomputes PageRank when the graph changes. Map rendering reads stored ranks and selects rows to budget. The read asks for all ranked nodes, and rank maintenance is best-effort. [Update path](https://github.com/Adulari/forge/blob/22d4346c6b8bc0d8432419a286191f4d26911e3d/crates/forge-index/src/lib.rs#L269-L275) · [Map reader](https://github.com/Adulari/forge/blob/22d4346c6b8bc0d8432419a286191f4d26911e3d/crates/forge-index/src/map.rs#L43-L96) | Separate calculation from later reads. PR 12 applies this to SymForge’s existing distinct-importer count and computes it on demand, keeping extra graph work off the cold code-search path. |

These source checks establish how the projects work. None supplies a comparable measurement on the pinned SymForge and AAP checkouts. **PR 1 remains first** because both actual root maps omit the entire ranked section. The peer methods apply where the user can see the ranking or reuse it across queries. Switching to PageRank would change which types an LLM sees.

### Indexing, progress and publication

| Tool | Evidence and application |
|---|---|
| clangd | Its file index supplies open-file symbols while background work builds the project-wide index. Cached shards reduce repeat work. It publishes an early build, batches subsequent builds, drains remaining work at idle, and rejects an older build finishing late. SymForge can use the same ordering rule when publishing doc coverage. [Index design](https://clangd.llvm.org/design/indexing) · [Background publisher](https://github.com/llvm/llvm-project/blob/llvmorg-20.1.8/clang-tools-extra/clangd/index/BackgroundRebuild.cpp#L28-L109) |
| rust-analyzer | The loader records scan roots done/total, configuration version, and begin/report/end. Intermediate events are coalesced to keep serialization off the critical path. SymForge should label the unit it actually counts and keep progress per project. [Rust 1.100.0 source](https://github.com/rust-lang/rust/blob/1.100.0/src/tools/rust-analyzer/crates/rust-analyzer/src/main_loop.rs#L1013-L1079) |
| Tantivy | The reader finishes preparing a new searcher before publication. A query retains the same searcher for its lifetime. SymForge already publishes immutable generations; derive hits, trust and coverage from that same view. [Reader implementation and contract](https://docs.rs/tantivy/0.27.0/src/tantivy/reader/mod.rs.html#251-318) |
| MCP | Request progress belongs to an active request token. Background indexing after a tool returns still needs pollable status. The 2026-07-28 specification deprecates client roots and recommends explicit parameters or configuration. Treat the chosen root as a request subject to server policy. [Progress](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress) · [Roots](https://modelcontextprotocol.io/specification/2026-07-28/client/roots) |

## Acceptance on the real client

### When does targeted retrieval save context?

For a question about one named symbol, let `D` be the tokens to request and read the file directly, `T` the tokens spent trying the targeted route, and `p` the probability that its result answers the question without a fallback. If a failed attempt requires a complete direct read, expected savings are `p × D − T`. Include discovery in `T` when the name or calling convention is unknown. Charge a session-wide schema once when it is newly needed. For a question about the whole file, a shorter response that omits necessary evidence cannot be counted as a saving.

The corresponding decision for a *default map hint* is stricter. Sum the actual cost of the routes a model would take without hints, subtract the cost with hints, then subtract the extra map tokens on every request, including map-only requests. Compare against a model that already chooses direct reads for small files. Report both model-visible context and transport size separately. Code mode can return a projection containing just the evidence the model needs.

For rollout, let `H` be the average extra tokens per served map and `S` the average *incremental* tokens saved when a hint changes a decision. If the unchanged decisions cost the same, the fraction of maps with changed decisions must exceed `H / S` to break even. Include costly wrong turns when computing `S`. This gives a directly testable threshold even when the deployment's mix of map-only and symbol-local work is uncertain.

The initial pilot used `o200k_base`, observed code-mode request and reply bytes, and a modeled direct-read envelope containing the Git file. Four returned source spans matched their respective Git line ranges after trimming outer whitespace. The question in each row required only that symbol's local implementation. The symbol call excludes discovery when its name is already known.

| File on pinned tree | File bytes | Direct read, tokens | Known symbol, tokens | Additional discovery, tokens |
|---|---:|---:|---:|---:|
| SymForge `src/embed/parity/read_context.rs` | 2,141 | 663 | 409 | 527 |
| SymForge `src/parsing/languages/elixir.rs` | 6,301 | 1,676 | 333 | 532 |
| SymForge `src/index_lifecycle/embed_remediation.rs` | 37,780 | 9,132 | 344 | 2,074 |
| AAP `07_app/crates/aap-core/src/identity.rs` | 16,423 | 5,148 | 339 | 1,712 |

For the 2,141-byte file, a known name saved 254 tokens at the wire. Discovering the name first cost 273 tokens more than a direct read. The 37,780-byte case saved 6,714 tokens even with discovery. The four-tool schema was another 894 visible tokens when charged to a new session. On the 422,500-byte `src/live_index/persist.rs`, the broad discovery call returned a truncated preview costing 7,445 tokens. Searching for `snapshot_path_and_lock` within its directory cost 302 tokens, and retrieving the declaration at lines 71–109 cost 711. Together those calls cost 1,013 tokens against 90,478 for the raw file contents alone. Job `01a20dcfaac177cbb96043736110bb64`. Raw pilot receipts are in `direct-vs-symbol-20261124-144138` in the private replay directory, with `direct-vs-symbol-cost.py` as the driver. These observations establish route costs for named tasks. Default-map utility also depends on whether the hint changes the next action.

A cheap directory estimate can group files by vote using their admitted, indexed byte lengths and usable symbol spans. For a known target, the avoided bytes are the file size minus the requested span. Overlapping spans cannot be counted twice. Each file gets one vote and a directory containing many small files and one large file retains that distinction. A mixed or uncertain result lets the model decide. A byte estimate is appropriate in the serving path only after calibration against real tokenized responses. Token counts vary with the language and encoding. Fixed-bucket counts can be added across directories without retaining the file list. [tiktoken](https://github.com/openai/tiktoken) · [Aggregating histogram buckets](https://prometheus.io/docs/practices/histograms/)

The small-file case occurs in a benchmark repo. On moldwright `3d172845`, Git lists 4,278 regular tracked TS, TSX, JS, JSX, Rust, Python and Go files. Of those, 190 are at most 256 bytes, 399 at most 512 bytes, and 902 at most 1 KiB. Under `apps/ui`, 128 of 2,300 such files are at most 256 bytes and the largest is 61,446 bytes. These are Git blob sizes. Runtime routing counts only the files the indexed publication can serve.

SymForge already uses indexed character counts and estimated invocation cost in its compact STEL facade. That facade's manual-read baseline is capped at 50 lines on larger files. The code-mode experiment compares a complete direct read with a sufficient targeted read, so its break-even point requires separate measurement. Admission must precede collecting any cost signal, and a rendered hint must stay within the authority of its source generation. [Existing cost calculation](../../../src/stel/controller.rs) · [Manual-read baseline](../../../src/index_lifecycle/guidance/file_read.rs)

Aider samples regularly spaced lines from longer map text, tokenizes the sample with the selected model, and scales by the ratio of total to sampled characters. That offers an offline cross-check for the byte estimate. Sampling source text at request time would read bytes from every participating file. Indexed span lengths let SymForge estimate how much source a retrieval omits without visiting the text. [Aider's token estimator](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/repomap.py#L89-L101)

#### Routing trial on held-out checkouts

The explicit `repo_map(detail="routing")` view gives each admitted file one vote when considering a **single known symbol**. The trial uses a direct vote for files up to 256 bytes with usable spans. A targeted vote requires at least three quarters of up to 32 evenly spaced symbol records to omit 8,192 or more source bytes each. It abstains near the crossover or when evidence is stale, incomplete or inconsistent. These thresholds are conservative experimental choices. A query visits the manifest once, inspects at most 32 existing span records per admitted file in scope, and sorts the represented directories. For P manifest entries, F admitted files in scope and D directory groups, this is O(P + 32F + D log D) operations aside from path and output string lengths. Transient counts scale with directories. The query performs no file I/O, tokenization, dependent lookup or additional indexing.

| Pinned scope | Represented / admitted files | Direct / targeted / uncertain among represented files | Compact / routing rendered tokens |
|---|---:|---:|---:|
| terminal-commander root | 946 / 1,039 | 4 / 317 / 625 | 959 / 1,190 |
| terminal-commander `crates/probes` | 17 / 17 | 0 / 10 / 7 | 363 / 130 |
| moldwright root | 5,127 / 5,127 | 117 / 480 / 4,530 | 890 / 895 |
| moldwright `apps/ui` | 1,957 / 2,441 | 47 / 174 / 1,736 | 992 / 1,190 |

On five interleaved calls per view, `crates/probes` took 250.48 ms median for compact and 1.87 ms for routing. Its compact view ranks visible types, and status moved from generation 11 to 17 during the observation, so this comparison is diagnostic. On `apps/ui`, where compact already skips the ranking, those medians were 5.37 ms and 7.39 ms on stable generation 29. Routing answers a different question and omits the key types, so its shorter response on `crates/probes` cannot establish savings for a model that needs those types. The earlier #1 and later routing binaries differ in their SymForge crate debug information and codegen-unit settings. Both sides of **these** comparisons used the same routing candidate, SHA-256 `0ae6c7ce193aa7f83ae7b7708fb39f41b814ba36cf8ce478e19e21234aca7bd9`. Job `01a20e1fef0f70dfaa80674c2529de8e`, private replay folder `heldout-routing-20261124-151504`.

**Keep the compact map as the default.** On the preregistered local code-mode workload, seven compact queries cost 4,932 model-visible request and response tokens. A simulated additive hint that preserved every original byte cost 5,008. Three scopes could not accommodate the hints within the byte limit. Two scopes could change the next call, but both proposed targeted reads failed to supply the requested evidence. Charging the two failed attempts and their direct-read fallbacks adds a further 539 tokens. On this workload the additive policy would spend **615 extra tokens** for the same evidence. The explicit routing view took 4,743 tokens across the seven map queries and omitted some directory and key-type rows. These measurements do not support replacing compact or adding hints to it by default.

The two failed probes matter for different reasons. Searching the 159,864-byte PTY source for the requested file-local function used 341 tokens before the direct fallback. The requested `get_repo_map` declaration in the 466,109-byte MCP source was absent and its lookup used 198 tokens. In contrast, the moldwright declaration query returned the correct type as a unique 70-byte slice of line 3, but took 215 tokens against 60 for a direct read. For the 439-byte terminal-commander module, the full-file task cost 115 tokens directly. These direct costs include a modeled read request and the complete working-tree source. The measurement used `o200k_base`, separately recorded the Git blob and checkout hashes, charged the code-mode request and visible reply once, and retained raw transport receipts. The additive policy was a simulation over actual result bytes, and its choice of next calls was specified in advance. No live model preference or deployment frequency was inferred. Reproduce with `heldout-routing-cost.py` and the companion `direct-vs-symbol-heldout-plan.md` in the private replay directory.

One supplemental check isolates **file size from target availability**. On the very same 466,109-byte file, the known function `tool_requires_daemon` at lines 456–462 was present. The file-scoped routing view returned one targeted vote, and the projected symbol call returned 410 bytes that matched a unique span in the checked-out source. It cost 311 tokens against 104,193 for a direct read. Discovering the map stub cost 166 tokens and the explicit file-scoped routing map cost 206. Charging both would bring the three calls to 683 tokens, before any new session-wide schema charge. All calls used Ready generation 4. This case was selected after the held-out run and is excluded from the rollout calculation. Job `01a20e383e0b77aa855099efd34d2700`.

The implemented opt-in view is on `feat/map-read-cost` at `799b8007`. The final server reproduced all seven held-out routing maps with the same file counts and rendered bytes, including the two scopes compared against an independently built Rust integration. [Final stdio and embedding receipts](LEDGER.md#read-routing-and-embedded-counts)

### Benchmark protocol

Freeze source revisions and input trees, binary and flags, accepted query scopes and independent expected hits. Compare a baseline and **one** candidate in alternation. Keep the same host, power policy, worker limits and source placement. For cold passes, clone tracked bytes into distinct normal roots with no snapshot; record OS cache state as uncontrolled unless explicitly measured. For restore, restart the process on the *same* root with the snapshot present. For resident tests, check canonical root, generation, load source, trust and corpus counts before and after. Record peak bytes, timeout/error count and result evidence alongside elapsed time. Never subtract a nominal CLI startup constant.

- **Map.** Run a first map per publication and repeated warm maps on AAP and moldwright, plus SymForge. For #2 audit real visible rows against proven file-level dependencies. Include a visibly ranked scope on each large workload and a controlled correctness fixture. Use cetis.ceregister for #2 and retain ce-registry-pro and terminal-commander as hidden-section controls for #1. [Pinned visibility replay](#which-slow-roots-actually-show-key-types) Use persistent MCP and CLI. Require the first and repeated maps on a ready publication under 2 s on the 5,000-file workloads, with zero timeouts. #1 requires identical bytes; #2 must justify each changed arrow and rank against the independent oracle. Include a real edit/rename and a temporal ranking change to catch stale reuse.
- **First code and docs.** Start before opening the project over code-mode stdio and capture status while reads wait. Report CLI timings separately. Independently confirm a distinctive source symbol and a distinctive doc path/heading from Git. Stop each clock only on that intended hit with appropriate trust and coverage. Report first partial, first complete, refusal windows and any gaps between probes. Require code under 3 s and doc success under 5 s, or demonstrate on a recorded client trace exactly what useful partial evidence the agent received by each deadline.
- **Proof per PR.** Save the raw timing/count receipts or their sanitized projections, canonical roots, query input and result fingerprints for each run. Use at least five fresh starts for cold timing and twenty warm calls per shape for p50, p95 and maximum, including failures in the denominator. Repeat on a held-out repository before rollout. A benchmark shortcut that changes ranking, omits eligible docs, weakens source checks or regresses another supported surface fails regardless of speed.
- **Deferred work.** A compiler flag, new allocator, snapshot format or service replacement has no measured justification here. Release already enables LTO. Keep the existing latest-stable toolchain and run proposed variants against the same release profile.
