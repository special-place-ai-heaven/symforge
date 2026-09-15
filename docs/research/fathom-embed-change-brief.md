# SymForge embed change brief for Fathom

> **Status (2026-09-15).** Written by Fathom's driving session from a read of Fathom main `7f88ade` and SymForge tag `v11.2.0` (`b549aa7`); no build or test was run. Checked in four rounds by independent citation and feasibility reviewers (issues per round: 14, 14, 6, 11, each round finer-grained than the last); the final revision fixes the last round's 11 findings and was not re-checked after that. Treat every `file:line` as a pointer to re-read against current SymForge main, not as settled fact. CR-0 is a defect reachable today in any Fathom build with the `symforge` feature.

## 1. Context

- **What Fathom is.** Fathom is a Rust tool that reads a repository and writes documentation. It targets Windows first and ships as a single exe with a local web UI.
- **How it uses SymForge.**
  - Fathom depends on SymForge as a path dependency with `default-features = false, features = ["embed"]` (`fathom: Cargo.toml:43`).
  - All SymForge use sits in one private module, `fathom: crates/fathom-repo/src/lib.rs:1329-1461`.
  - That module calls `ProcessIndexRuntime::acquire()` (the facade name for `ProcessRuntimeApi`, `src/embed.rs:84`), then `open_embedded_source(EmbeddedSourceSpec::current_worktree(root))` (`fathom: crates/fathom-repo/src/lib.rs:1447-1454`).
  - It polls `runtime_view()` every 250 ms until the phase is `Current`, with a 120 s limit (`fathom: crates/fathom-repo/src/lib.rs:692-698, 837-863`).
  - It then sends one kind of request, `search_symbols{query: None, path_prefix: None, limit: u32::MAX}`, and may send it more than once (`fathom: crates/fathom-repo/src/lib.rs:870-894`):
    - It repeats the request while the refusal advice is `Automatic` or `OnEvent`, calling `wait_until_ready` before each attempt, until the 120 s deadline.
    - `Never` and `Operator` fail at once (`fathom: crates/fathom-repo/src/lib.rs:880-893`).
  - It closes the source by dropping the handle at the end of `capture` (`fathom: crates/fathom-repo/src/lib.rs:1437-1457`).
  - Fathom sends no other request. Beyond the calls above, it uses only the accessors `SourceRefusal::kind`, `SourceRefusal::retry`, `Display` and `Claim::value` (`fathom: crates/fathom-repo/src/lib.rs:1340-1346, 1417`). A contract change to any of these also breaks Fathom.
- **Threading.** The whole capture is one blocking call.
  - The server runs it under `tokio::task::spawn_blocking` (`fathom: crates/fathom-server/src/lib.rs:799`).
  - The CLI calls it directly inside the async `run()` (`fathom: crates/fathom/src/main.rs:121-125`), which `#[tokio::main]` drives (`fathom: crates/fathom/src/main.rs:139-141`).
- **Revision.**
  - Fathom's CI and release check out SymForge **tag v11.2.0**: `ci.yml` hard-codes it (`fathom: .github/workflows/ci.yml:57, 93, 128`) and `release.yml` uses `SYMFORGE_REV` (`fathom: .github/workflows/release.yml:50, 95, 144`).
  - `Cargo.lock` records no tag or commit for SymForge. Its `symforge` entry has `version = "11.2.0"` and SymForge's dependency list, but no source or checksum (`fathom: Cargo.lock:3643-3645` onward).
  - This research read commit b549aa7. Every SymForge citation below is against that revision. **Re-check each one against current SymForge main before changing code.**
  - Paths without a prefix are SymForge paths. `fathom:` paths are in the Fathom repository.
- **Evidence level.** This brief comes from reading code and ledger text. No SymForge or Fathom build or test was run for it.

---

## 2. Change requests (highest value to Fathom first)

### CR-0. A second embed open of the same root must refuse, whichever runtime it comes through (live defect, does not depend on CR-1)

**The problem in Fathom**
- **Fathom can reach this at 7f88ade when built with the `symforge` feature.**
  - The one-run-per-repository rule compares canonicalised request paths (`fathom: crates/fathom-server/src/lib.rs:722-735`). Nothing checks that `RunRequest.repo_path` is a working-tree top-level (`fathom: crates/fathom-server/src/lib.rs:697-711`).
  - Capture then opens SymForge on `working_tree_root(repo_root)`, which runs `git rev-parse --show-toplevel` (`fathom: crates/fathom-repo/src/lib.rs:237-250, 1447-1454`).
  - Say two runs are in flight at once, one on `C:\repo` and one on `C:\repo\crates`. They get different run keys, so both pass the check. Each capture then opens the same SymForge root through its own `acquire()`.
  - When the first capture drops its handle, the second run's `search_symbols` refuses with `Never` (mechanism below). Fathom reports that as an immediate failure (`fathom: crates/fathom-repo/src/lib.rs:885-889`).
- **Fathom stopgap.** Key the one-run rule on the canonicalised working-tree top-level instead of `canonicalize(repo_path)`, that is `fs::canonicalize(working_tree_root(repo_path))`.
  - `working_tree_root` is private to fathom-repo today (`fathom: crates/fathom-repo/src/lib.rs:237`). It returns git's `--show-toplevel` text unchanged (`fathom: crates/fathom-repo/src/lib.rs:237-239`).
  - SymForge's key comes from the canonicalised root (`src/discovery/mod.rs:1538-1541`). Without the canonicalise step, two request paths that name the same top-level differently get different Fathom keys but the same SymForge key. Examples are a junction, a subst drive or a difference in case. That would reopen this defect by another route.
  - **Where the git call runs.** `start_run` is an async axum handler that returns `Response`, not `Result` (`fathom: crates/fathom-server/src/lib.rs:707-735`). `working_tree_root` starts a synchronous `git rev-parse` subprocess (`fathom: crates/fathom-repo/src/lib.rs:237-239, 258`). Run it on a blocking thread (`spawn_blocking`) before taking the run-table lock (`fathom: crates/fathom-server/src/lib.rs:727`), so it never blocks a tokio worker in the request path.
  - **A path that is not a repository.** Today such a POST returns 202, and the run fails later in capture with `NotARepository` (`fathom: crates/fathom-repo/src/lib.rs:244-246`). Pick one of two behaviours:
    - fall back to today's key (`fathom: crates/fathom-server/src/lib.rs:722-723`), which keeps the 202-then-failed behaviour; or
    - return a 4xx, and record that as an API change.
  - **Alternative, not recommended before CR-1.** Fathom can hold one process-wide `ProcessIndexRuntime` instead of acquiring one per capture. SymForge's existing per-factory refusal (`embedded.rs:492-496`) then keys on SymForge's own canonical root identity, and Fathom needs no path handling of its own. The cost:
    - The factory `open` mutex is held for the whole of `open_bound`, including admission and worker spawn (`embedded.rs:493-541`).
    - `close_one` and `shutdown_all` also hold it across `shutdown()`, whose `worker.join()` has no timeout (`embedded.rs:397-406, 555-567, 576-589`).
    - With one shared runtime, every open and close in the Fathom process queues behind the slowest close's join. That includes captures of unrelated repositories.
    - At v11.2.0 that join lasts the rest of a full reload. A capture that times out at 120 s while still `Loading` drops its handle mid-reload. Closing a `Current` source can also wait for a full tick scout (`embedded.rs:231`) plus up to 200 ms.
    - Recommendation: use the per-capture runtime plus the working-tree-key stopgap until CR-1 ships. Consider the shared runtime only after that, if at all.
- Main CI and the release build compile without `symforge` (see section 3), so builds from main do not reach this yet. Any build with the feature does.
- A search of `docs/STATE.md` and `docs/PRODUCT-GAPS.md` found no ledger row for this.

**What SymForge does today**
- **The design says a duplicate open must refuse.**
  - V1 deliberately chooses a single exposed owner, not shared-close refcounting. Distinct SourceSlots for the same key are forbidden, so that one independently returned handle cannot revoke another (`docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md:2225-2234`).
  - A duplicate embedded open "never joins ownership of the already-exposed source" (same file, `:1171-1174`). Its test plan requires the second open to be refused (`:2459-2464`).
  - `CONTEXT.md:435-440`, the doc comment at `src/index_lifecycle/public_api.rs:522-524` and the one at `src/index_lifecycle/embedded.rs:482-486` say the same.
- **The code does not enforce that across runtimes.** A second open of the same root through a separate `acquire()` joins the existing live admission instead of refusing (`src/index_lifecycle/registry.rs:382-398`; `src/index_lifecycle/activation.rs:1276-1282`).
  - The one-handle rule does not stop this: it only checks the factory's own map (`embedded.rs:492-496`), and each `acquire()` creates a new owner and factory (`public_api.rs:513-520`, factory at `:517`).
  - Each open also builds its own `LiveIndex::empty()` data plane and worker (`embedded.rs:506-516`).
- Closing either handle then calls `stop(&key)` (`embedded.rs:408`). That removes and revokes whatever live occupancy holds the key, with no slot or holder check (`registry.rs:493-511`).
- The other handle's `search_symbols` then refuses with `Never` through `runtime.acquire()` (`activation.rs:1053-1062`; `embedded.rs:762-769`). Its phase can still read `Current`, because the worker reloads through `data_plane()`, which skips the revocation check (`embedded.rs:268`; `activation.rs:1064-1067`).
- The worker-spawn failure path in `open_bound` calls the same unconditional `stop(&key)` (`embedded.rs:517`).

**Fix**
- Make the embed sole-owner refusal process-wide instead of per-factory.
  - Keep a process-wide set of open embed keys.
  - In `open_bound`, check and insert that set before `admit_project_with_outcome` (`embedded.rs:498`).
  - A key already present gets the existing `SourceAlreadyOpen` refusal. Today that maps to `SelectionUnavailable`/`OnEvent` (`public_api.rs:550-555`).
- **Remove the key through a drop guard, not by hand on each exit.**
  - Create the guard in `open_bound` when the key is inserted, and carry it into the binding. Release it at the end of `EmbeddedBinding::shutdown`, after the admission stop (`embedded.rs:408`). Any earlier return or unwinding panic drops it and removes the key.
  - Why a guard: removal written only on the `Err` returns (`embedded.rs:505, 516-518`) and at the end of `shutdown()` is skipped by a panic in between. That path has several `.expect()` calls on mutexes (`embedded.rs:310, 385-388, 398-400, 412-415`). It also runs the bootstrap-lock and ceremony `expect`s in `activate_surface` (`activation.rs:1127-1154`), which `admit_project_with_outcome` calls first (`activation.rs:1213`).
  - Why it matters more once the set is process-wide: Fathom drops a whole runtime per capture, so a leaked factory entry dies with that runtime today. A leaked entry in a process-wide set would refuse that root with `OnEvent` for the life of the process. Fathom's server turns a capture panic into a failed run and keeps serving (`fathom: crates/fathom-server/src/lib.rs:802`), so the lockout would persist.
- **Scope it to embed opens only.** Do not change the join-live arm of `ProjectRegistry::admit_with_outcome` (`registry.rs:332`; the arm is at `:382-398`).
  - The daemon admits through it in production (`daemon.rs:3421`).
  - `cleanup_failed_daemon_admission` is written for a JoinedLive opener (`daemon.rs:3530-3531`).
  - The test `joined_live_failed_project_load_retires_after_last_failed_holder` pins that behaviour (`daemon.rs:7038-7075`).
- **Make the spawn-failure path at `embedded.rs:517` holder-safe.** Drop the binding first, then call `stop_if_unheld` (`registry.rs:514-541`), the same way `cleanup_failed_daemon_admission` does (`daemon.rs:3524-3542`).
- **Not proposed: shared-close refcounting**, where close skips the revoke while another open still holds the admission.
  - It contradicts the design above.
  - It would make the forbidden state permanent: two separate `LiveIndex::empty()` data planes (`embedded.rs:506-508`), two workers and two full scouts every 200 ms (`embedded.rs:231`) on one root, all under one admission slot.
  - Today's types also do not allow it. `stop_if_unheld` refuses while `Arc::strong_count > 1` (`registry.rs:526-528`), and the binding holds the admission through `runtime` (`activation.rs:1041-1044`). `shutdown(&self)` always runs while at least two `Arc`s to the binding are alive: the handle (`embedded.rs:634`) and the open map entry (`embedded.rs:524`). On the `close_one` path there is a third, the clone at `embedded.rs:563`. `shutdown_all` (`embedded.rs:580-585`) takes no clone.

**Tests SymForge should add**
1. **`second_open_through_another_runtime_refuses_until_first_closes`.**
   - Open root R through runtime A.
   - Open R through a second `acquire()` B. Assert that B gets no handle and receives the `SourceAlreadyOpen` refusal (today `SelectionUnavailable`/`OnEvent`). At v11.2.0 B's open succeeds (`registry.rs:382-398`; `embedded.rs:492-496`).
   - Close A, then open R through B again. Assert that it succeeds and that the new incarnation cannot be confused with the old one (design `:2463-2464`).

Regression guards:
- An open of R whose admission fails or whose worker fails to spawn leaves R free, so a later open of R succeeds.
- A panic injected between the key insert and worker start leaves R openable.

**What Fathom will change**
- Under this fix, a colliding run fails when it opens, with an `OnEvent` refusal. It no longer fails mid-capture with `Never`.
- `fathom: crates/fathom-repo/src/lib.rs:1450-1454` maps that refusal straight to an error. The one-run stopgap is therefore permanent, not temporary. Fathom can ship it without waiting for SymForge.

---

### CR-1. Closing a source while it indexes must return promptly (need N1)

**The problem in Fathom**
- A user can press Stop while a run is still indexing. Nothing reaches the capture: the stop route only sends on a watch channel (`fathom: crates/fathom-server/src/lib.rs:1012-1044`).
- The flag is read only after capture returns, in `begin_generating` (`fathom: crates/fathom-server/src/lib.rs:799-805, 873-889`). The code says so: "Capture cannot be interrupted; a stop that lands during it is honoured when capture returns" (`fathom: crates/fathom-server/src/lib.rs:777-782`).
- The ledger banks this.
  - The row `stop-mid-index-abort` is BANK with no lane. It reopens "when the SymForge embed API offers a cancellable reload or a non-blocking close" (`fathom: docs/STATE.md:342-343, 366`).
  - The owner decision keeps mid-index abort BANK (`fathom: docs/diagnostics/DIAG_FATHOM_STOP_ROUTE_NOTES.md:19`).
  - The diagnostics notes add that a stopped run holds one blocking-pool thread until the reload finishes (`fathom: docs/diagnostics/DIAG_FATHOM_STOP_ROUTE_NOTES.md:26`).
- Fathom can add its own cancel check to its polling loop. The handle's `Drop` would still block.

**What SymForge does today**
- **The worker runs a full reload before its first stop check.**
  - It calls `reload_and_publish()` (`src/index_lifecycle/embedded.rs:217`) before reading `control.stop`.
  - The flag is first read after `wait_timeout` returns (`embedded.rs:219-227`), and in `wait_refresh_visibility_or_stop` (`embedded.rs:252-259`).
  - `shutdown` wakes waiters with `notify_all` (`embedded.rs:392-396`). A notify sent while the worker is inside a reload or inside the tick scout (`embedded.rs:231`) reaches no waiter. The worker then sleeps up to `EMBED_OBSERVER_POLL` (200 ms, `embedded.rs:23`) before it sees `stop`.
- **The reload takes no stop input.**
  - `reload_for_binding_with_exclusions` has no cancel parameter (`src/live_index/store.rs:2565-2582`), and neither does `build_reload_data_for_binding_with_exclusions` (`store.rs:5259-5363`).
  - Parsing is a rayon `par_iter` (`store.rs:4598-4763`) on one process-wide pool (`store.rs:58, 110-113`). `install` is synchronous, so no pool work outlives the call.
- **Closing joins the worker.** `EmbeddedBinding::shutdown` sets `shutdown_started`, phase `Stopping` and `control.stop`, then calls `worker.join()` with no timeout (`embedded.rs:382-406`).
- **The join happens under the factory's open-map lock.** `close_one` (`embedded.rs:555-567`) and `shutdown_all` (`embedded.rs:576-589`) hold that mutex across the join.
- **Every close path ends in that join.**
  - `Drop` for the handle (`embedded.rs:1082-1100`).
  - `close()` (`embedded.rs:669-682`).
  - `begin_close()` (`embedded.rs:690-713`).
  - Runtime owner drop, through `shutdown_all` (`embedded.rs:576-595, 619-623`).
  - `ProcessIndexRuntime::begin_shutdown` (`src/index_lifecycle/public_api.rs:572-581`, through `embedded.rs:614-616`). This one is a contract atom (`specs/020-repository-knowledge-index/contracts/public-api-v11.json:3362`).
- **The flag a cancel would need already exists.** `EmbeddedBinding.shutdown_started` (`embedded.rs:161`) is set before the join (`embedded.rs:383`). Nothing on the indexing path reads it.
- **Worker phase writes can overwrite `Stopping`.**
  - `set_phase(Refreshing)` (`embedded.rs:240`), `set_blocked()` (`embedded.rs:274-277, 313-317`) and `phase = Current` (`embedded.rs:295`) all write the phase without checking for shutdown.
  - A reload that fails while a close is in progress makes readers see `Stopping`, then `Blocked`, then `Stopped` (`embedded.rs:391, 407`).
  - During the `Blocked` window, `runtime_view()` reports `Blocked` on every close path (`embedded.rs:716-731`).
  - `is_open()` also returns true in that window, but only when the close came through `ProcessIndexRuntime::begin_shutdown` or the runtime owner's drop (`embedded.rs:576-595, 614-623`). Those paths leave `EmbeddedSourceHandle.closed` unset (`embedded.rs:652-660`). After `close()` or `begin_close()` it returns false, because both set `closed` before they call `close_one` (`embedded.rs:673, 691`).
- **Closing stops the whole admission, not just this open.** `shutdown` calls `process_project_registry().stop(&self.key)` (`embedded.rs:408`). Under the sole-owner design that is intended. CR-0 makes sure no second embed open can share the admission.

**Proposed change (no public API change)**

Make the existing close paths cooperative. Have the reload read the flag that `shutdown()` already sets.

```rust
// proposed, crate-private. The existing function has other callers
// (daemon.rs:3651, 3897; store.rs:2546, 2558; a test at watcher/mod.rs:3317),
// so they keep a never-cancelled path.
pub(crate) fn reload_for_binding_with_exclusions_cancellable(
    &self,
    root: &Path,
    project_state_dir: Option<ProjectStateDir>,
    source_exclusions: discovery::SourceExclusions,
    cancel: &AtomicBool,            // EmbeddedBinding.shutdown_started
) -> anyhow::Result<()>;            // Err(cancelled) publishes nothing
```

How the parse step exits:
- `admit_and_parse_entries` returns `AdmitParseResult`, not a `Result` (`store.rs:4580-4586`).
- Its reload caller asserts one disposition per catalog entry and builds a manifest right after (`store.rs:5323-5327`).
- The clean exit is to make it return a `Result`. That also touches `LiveIndex::load`, which shares it (`store.rs:4553-4556`; call sites `store.rs:4937, 5313`). Faking cancelled items as terminal outcomes would feed that manifest code.

Where to check the flag:
- **In the parse closure.**
  - Check at the top, before the byte-budget acquire (`store.rs:4601`, before `4657`).
  - Check again right after the acquire. The budget wait loops on a condvar with no stop input (`store.rs:197-205`), so a thread waiting there only sees the flag once budget frees.
- **Between reload steps.**
  - Between the scout and the parse (`store.rs:5277-5313`).
  - Around the git reads, which have no stop input:
    - `untracked_generated_output_demotions` at `store.rs:5296-5297` reads the git index by default, through `tracked_path_set_for_build_dir_rescue` (`discovery/mod.rs:2399, 2333-2340`). It skips that read only when `SYMFORGE_INDEX_GENERATED_OUTPUT` is set (`discovery/mod.rs:2396-2398`).
    - `tracked_path_set_for_exclusion` at `store.rs:5292` reads the git index only when `SYMFORGE_EXCLUDE_UNTRACKED` is enabled (`discovery/mod.rs:2313-2315`).
    - The scout walk has one more. The first time the walk hits a root-level build directory, it calls `tracked_path_set_for_build_dir_rescue` (`discovery/mod.rs:604-605`), which opens the repository and reads the whole git index (`discovery/mod.rs:2333-2340`).
  - Around the coupling step. `init_coupling_store` at `store.rs:5336-5337` runs `git2::Repository::discover` whenever a state directory is set and the coupling policy is not `Disabled`. The policy is resolved at `coupling/lifecycle.rs:24-43` and is lazy by default (`:41`); the discover call short-circuits for `Disabled` at `:148` (discover itself is at `:46-48`). Embed opens set a state directory by default (`discovery/mod.rs:1617-1623`). Under warm-on-start it also opens the coupling database (`lifecycle.rs:162-163`).
- **Inside the derived-index build.** Check between the trigram, reverse-index and path-index builds (`store.rs:4040-4048`), not only before it. The comment at `store.rs:5329-5331` reports that this derived-index tail measured 60-65% of real startup on the fresh-load path. It was not measured for reload.
- **Before the write lock.** A bail before `store.rs:2621` publishes nothing. The last check before that lock must come after the coupling step at `store.rs:5336-5337`, or that step needs its own check.
- **In the scout (required).**
  - Every reload starts with a full scout (`store.rs:5277`), and the worker runs another full scout every tick (`embedded.rs:231`) and after each reload (`embedded.rs:279`).
  - The scout walks the tree (`src/discovery/mod.rs:532-559`). It reads metadata for each entry (`discovery/mod.rs:789`) and reads a probe from each ingest candidate (`discovery/mod.rs:850-853`).
  - `scout_repository_with_exclusions` is public and is defined at `discovery/mod.rs:669`.
    - Production callers: `watcher/mod.rs:376, 655`; `store.rs:4898, 5277`; `embedded.rs:304`.
    - Test callers: `watcher/mod.rs:2494` and `3302`, both inside `mod tests`, which starts at `watcher/mod.rs:1430`.
  - Add a crate-private cancellable variant rather than changing its signature. It checks in three places:
    - in the walk loop (`discovery/mod.rs:559`);
    - right after the lazy tracked-set read (`discovery/mod.rs:604-605`);
    - in the entry loop (`discovery/mod.rs:786`).

    A check only at the top of each walk iteration would wait for that git read to finish.
- **In the worker.**
  - Replace the wait at `embedded.rs:219-223` with `wait_timeout_while(control, EMBED_OBSERVER_POLL, |c| !c.stop && !c.refresh_requested)` (std `Condvar`; fields at `embedded.rs:146-149`).
  - Use the same shape with `|c| !c.stop` at `embedded.rs:253-257`.
  - This one change closes both the lost stop and the lost refresh wakeups. Without it, a cancelled first reload (`embedded.rs:217`) still costs up to 200 ms before the worker exits.
  - Check `shutdown_started` after the tick scout (`embedded.rs:231`). The post-reload scout at `embedded.rs:279` only runs when the reload returned `Ok` (an `Err` returns at `embedded.rs:274-277`), so skip it when cancelled on that path.
- **Phase writes.** Once `shutdown_started` is set, the worker must not overwrite `Stopping` (`embedded.rs:240, 295, 313-317`).
  - Checking the atomic before calling `set_phase` or `set_blocked` still leaves a gap. `shutdown()` sets the atomic at `embedded.rs:383` and takes the state mutex to write `Stopping` only later (`embedded.rs:391`). A worker can read the flag as false and then call `set_blocked()` or `set_phase(Refreshing)`, which lock inside (`embedded.rs:309-317`). It can then still write `Blocked` after `Stopping`. The `Refreshing` write at `embedded.rs:240` also acts on a phase read taken under a separate lock (`embedded.rs:237-238`).
  - Rule: on the worker path, skip the `Blocked`, `Refreshing` and `Current` writes when the reload returned the typed cancelled error, or when `shutdown_started` reads true inside the same state-mutex critical section as the write. Replace `set_phase` and `set_blocked` on the worker path with conditional variants that do this. The `phase = Current` write at `embedded.rs:288-295` already holds the lock and needs the same check.
  - Do not check `phase == Stopping` instead. Between `embedded.rs:383` and `:391` the atomic is already true but the phase is still `Loading` or `Refreshing`. A reload cancelled in that gap would pass a phase check and write `Blocked`, which also clears `current_publication_identity` (`embedded.rs:313-317`). The `Stopping` write always comes after the atomic is set, so only a read of the atomic under the lock, or a branch on the typed cancelled error, is safe.

**Behaviour and edge cases (the contract Fathom will rely on)**
- **Cancel mid-parse.**
  - Files already inside a parse call finish, and the remaining items return at once.
  - Nothing is published: phase ends `Stopped`, `source_version` does not advance, and `current_publication_identity` stays `None`.
  - Readers never see `Blocked` after `Stopping`.
  - **Question: what does "promptly" mean?** Parse work does not set the bound. Parse calls are bounded by pool size and a per-file read ceiling of 1 MiB (`store.rs:158`), so they are the small part. Under the check points above, the longest steps that cannot be interrupted are single calls, and a close that lands inside one waits for it to finish:
    - `TrigramIndex::build_from_files` (`store.rs:4043`), part of the derived-index tail, which runs to completion once started;
    - the whole git-index read in `tracked_path_set_for_build_dir_rescue` (`discovery/mod.rs:2333-2340`), reached from both the scout (`:604-605`) and `untracked_generated_output_demotions` (`store.rs:5296-5297`);
    - `git2::Repository::discover` (`coupling/lifecycle.rs:47`);
    - the Debug format and hash of the whole scout plan (`embedded.rs:306`).

    Can SymForge state or measure the worst of these on a large repository, and define "prompt" as that bound? Otherwise, add checks inside the trigram build as well.
- **Drop while indexing.** Same as cancel. All five close paths go through `shutdown()`.
- **Refreshing.** A close during a refresh reload also returns promptly and publishes nothing, but the end state differs from a cancelled first reload.
  - The `Refreshing` write (`embedded.rs:240`, through `set_phase` at `309-311`) leaves `source_version` and `current_publication_identity` in place.
  - After a cancelled refresh that started from `Current`: phase ends `Stopped`, `source_version` stays at the last `Current` value, and `current_publication_identity` keeps the last `Current` identity (`embedded.rs:288-295`), not `None`. The cancel path must therefore not clear the identity the way `set_blocked` does (`embedded.rs:316`).
  - A refresh entered from `Blocked` (`embedded.rs:237-239`) already has identity `None`, set at `embedded.rs:316`. After a cancel it ends `Stopped` with identity still `None`.
- **Repeated and concurrent close.**
  - A repeat on the same handle returns at once through `EmbeddedSourceHandle.closed.swap` (`embedded.rs:673-675, 691-692, 1096-1098`).
  - A concurrent close through another path waits on the factory `open` mutex until the first join returns (`embedded.rs:555-567, 576-589`).
    - One example: `shutdown_all`, reached from `begin_shutdown` on a clone of the runtime, racing a handle drop on another thread.
    - Another: the runtime owner's drop racing a handle drop the same way.
    - By the time the second closer gets the lock, the key is already gone from the map.
  - `shutdown()` has only two callers, `close_one` (`embedded.rs:566`) and `shutdown_all` (`embedded.rs:584`). Both hold that mutex across the call, and a binding sits in exactly one factory's map. So the `shutdown_started.swap` branch (`embedded.rs:383-390`) never sees a concurrent second caller.
  - The contract should state this: concurrent closers of one runtime queue on the factory lock, and the wait lasts as long as the first close's join, which CR-1 makes prompt.
- **A cancel that is not a shutdown.** Out of scope. A standalone cancel that left the source `Blocked` would reload on every tick (`embedded.rs:237-244, 284-286`), so this request only covers cancel-on-close through `shutdown_started`.
- **Windows file handles after close (unmeasured).** Fathom needs to know whether the repository directory can be renamed or deleted once drop returns. Known facts:
  - No rayon work outlives the reload, because `install` is synchronous (`store.rs:4598`).
  - The reload opens a coupling store when a state directory is set (`store.rs:5336-5337`). Embed opens choose `<root>/.symforge` when they can (`discovery/mod.rs:1617-1623`).
  - Under the default lazy policy (`src/live_index/coupling/lifecycle.rs:41`), the store only opens a database that already exists (`lifecycle.rs:54-65, 152-159`). That handle lives inside the index.
  - Under the warm-on-start policy it spawns a detached `coupling-init` thread that nothing joins (`lifecycle.rs:183-192`).
  - How long the index outlives the handle's drop was not traced.
- **Lock held during the join.** With one runtime per capture (Fathom's shape today), and once the join is prompt, `close_one` holding the `open` mutex (`embedded.rs:555-567`) stops mattering to Fathom. A shared runtime would queue every open behind that join (see the CR-0 alternative). No change is requested.

**Tests SymForge should add**

*Test gate.* The existing `reload_outside_lock` hook cannot be reused:
- It is `#[cfg(test)]` and thread-local, and it is consumed when it fires (`store.rs:3875-3912`).
- The reload runs on the `symforge-embed-N` thread (`embedded.rs:204-206`) and parsing runs on the pool threads (`store.rs:58, 4598`). A hook installed on the test thread never fires.
- `cfg(test)` items are also invisible to integration tests built through `__test-internals` (`src/lib.rs:27-39`; `Cargo.toml:170`).

The gate needs to be process-wide and scoped to one root, so parallel lib tests that also reload do not trip it. It must live in-crate and compile under the embed-only lib test build (see the convention at `store.rs:3858-3862`). Until the gate exists, a test that uses it only fails to compile at v11.2.0. The notes below give the behavioural failure once the gate exists.

1. **`drop_while_loading_returns_before_reload_completes`.** Hold the parse after N files, drop the handle from another thread, then release. Assert that drop returns, that fewer than all files were parsed, and that the published generation did not change. At v11.2.0 drop waits for the full reload.
2. **`cancelled_reload_publishes_nothing`.** Same setup, but close with `close()` and read `runtime_view()` on the still-alive handle (`embedded.rs:716-731`). Assert phase `Stopped`, `source_version == 0` and `current_publication_identity == None`. A concurrent reader must never observe `Blocked` after `Stopping`. At v11.2.0 that sequence can occur (`embedded.rs:274-277, 391, 407`).
3. **`close_during_refreshing_returns_promptly`.**
   - Reach `Current` and record `source_version` and `current_publication_identity`.
   - Change a file, hold the refresh reload, call `close()`, and assert it returns before release.
   - Read `runtime_view()` on the still-alive handle. Assert phase `Stopped`, `source_version` equal to the recorded value, and `current_publication_identity` equal to the recorded identity.
4. **`close_after_cancelled_reload_does_not_sleep_a_poll`.** Hold the first reload, call `close()` from another thread, release, and assert close returns well under 200 ms after release. At v11.2.0 the notify is sent while the worker is inside the reload, so the worker then sleeps the full wait before it sees `stop` (`embedded.rs:219-225, 392-396`).
5. **`close_during_tick_scout_does_not_sleep_a_poll`.** Hold the tick scout (`embedded.rs:231`) on a `Current` source, close, and release. Same assertion. A plain idle close is not a reliable failing test: a worker parked in `wait_timeout` wakes at once on `notify_all`.

Regression guards:
- `close()` followed by `Drop` still coalesces (`embedded.rs:673-675, 1096-1098`).
- Hold the first reload. Race `begin_shutdown` on a runtime clone against `close()` on the handle from another thread, then release.
  - Both return once the first join returns.
  - Exactly one of these holds: `CloseReceipt::performed_shutdown()` is true (`embedded.rs:84`, set at `:676-681`), or `ShutdownReport.closed_sources == 1`.
  - `joined_workers <= 1`.
  - A race against a handle drop cannot be checked through receipts, because `Drop` discards the result of `close_one` (`embedded.rs:1096-1098`). If `shutdown_all` then finds an empty map, it reports `joined_workers == 0` (`embedded.rs:576-595`). Observing the drop path needs an in-crate join counter.
- **New and unmeasured today:** on Windows, the repository directory can be renamed right after drop returns, under both coupling policies.

**What Fathom will change once this lands**
- Thread a cancel input through `CaptureFn` (`fathom: crates/fathom-server/src/lib.rs:83`), then `fathom::capture` (`fathom: crates/fathom/src/lib.rs:126-138`), then `capture_from_source` and `wait_until_ready` (`fathom: crates/fathom-repo/src/lib.rs:788-863`).
- On stop, return a cancelled error and let the handle drop inline, before the run leaves Indexing.
- Extend `stop_during_indexing_ends_cancelled_before_generation` (`fathom: crates/fathom-server/src/lib.rs:3369-3416`) so a stop during indexing ends the run without waiting for indexing.
- Fathom needs no new SymForge type for this.

---

### CR-2. A read-only index census on the handle (N4 and N3; narrower value than first assumed)

> **Owner note (2026-09-15): optional.** "symforge can index any folder on a whim without cost." Fathom therefore builds its Re-index and pre-run counts (lane L25) and its module inventory with per-module symbol counts (lane L18) itself: open an embedded source on the folder, wait for `Current`, group one `search_symbols` result by path. Neither lane waits on this request. CR-2 would only add parsed files that carry zero symbols, and save Fathom's own grouping; it does not save indexing.

**The problem in Fathom**
- **N4 (row rg-29, lane L18, `fathom: docs/STATE.md:435`).**
  - Fathom's file list comes only from grouping `search_symbols` matches by path (`fathom: crates/fathom-repo/src/lib.rs:896-928`). `FactSet.files` therefore holds only files with at least one symbol (`fathom: crates/fathom-repo/src/lib.rs:716-721`).
  - Per-directory totals exist only as text in the public `factset_text` (`fathom: crates/fathom-repo/src/lib.rs:1081-1191`, tree at `1155-1167`), built by the private helpers `common_root`, `TreeDir`, `build_tree` and `census` (`fathom: crates/fathom-repo/src/lib.rs:1197-1323`).
- **What L18 needs is mostly buildable at v11.2.0.**
  - L18 defines a module as a first-level directory under the common source root that fathom-repo's census already computes. Its scope is a public module list with per-module symbol totals, and its test fixture has three modules, two of them named by the graph (`fathom: docs/PRODUCT-GAPS.md:1503-1531`).
  - Making the private census public covers that.
  - CR-2 adds only parsed files with zero symbols. The only directories it adds are those whose parsed files all have zero symbols.
  - Directories holding only files that were never parsed are still missing (see "What set is counted").
- **N3 (row rg-39, no lane, "Audit suggests BANK", `fathom: docs/STATE.md:443`).**
  - A Re-index control needs counts without a full capture.
  - File and symbol counts are already available at v11.2.0 from open, wait and `search_symbols_all` (`fathom: crates/fathom-repo/src/lib.rs:839-894`).
  - A census call would save only git identification, the manifest read and fact building (`fathom: crates/fathom-repo/src/lib.rs:793-798, 819`). It would not save indexing: an embed open starts from an empty index and runs a full reload (`embedded.rs:506, 217`).

**What SymForge does today**
- **The census exists internally.**
  - Every publish captures a `RepoOutlineView` (`store.rs:3487-3497`).
  - `capture_repo_outline_view` builds it from `all_files()` with no symbol filter. It drops paths that fall outside the indexed root and sorts by path (`src/live_index/query.rs:2606-2635`; `query.rs:839-847`).
  - The view carries `total_files` and `total_symbols`, and for each file `relative_path`, `language` and `symbol_count`.
- **It is reachable inside the crate** through `published_repo_outline()` (`store.rs:2502-2505`). `PublishedIndexState` holds `file_count`, `parsed_count`, `symbol_count` and `tier_counts` (`store.rs:837-897`, via `published_state()` at `store.rs:2497-2500`).
- **The embed facade exports none of it** (`src/embed.rs:77-88`).
- **There is no other way to list files.**
  - `search_text` refuses an empty query (`embedded.rs:837-844`).
  - `files_by_dir_component` is keyed by every parent directory name, lower-cased and flattened (`store.rs:5604-5609, 5663-5682`), so it cannot give first-level totals.

**Proposed API (embed feature; not purely additive, see Contract cost)**

```rust
// proposed
impl EmbeddedSourceHandle {
    /// Census of the published generation. Same readiness rule as
    /// search_symbols: refuses unless phase is Current.
    pub fn index_census(&self) -> Result<Claim<IndexCensus>, SourceRefusal>;
}

// proposed
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexCensus {
    pub total_files: u64,
    pub total_symbols: u64,
    pub files: Vec<CensusFile>,     // sorted by path, '/'-separated, relative
}

// proposed
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CensusFile {
    pub path: String,
    pub language: String,           // LanguageId::name(), as engine_info does (embed.rs:47-56)
    pub symbol_count: u32,
}
```

- **Template.** Follow `search_symbols` (`embedded.rs:734-823`): the closed check, then `current_claim` (`embedded.rs:330-363`), then read and map.
- **One generation.** Load one `published_generation()` (`store.rs:2297-2299`) and take both `.live` and `.outline` from that one value (`store.rs:912-926`). `read()` and `published_repo_outline()` each load separately (`store.rs:2287-2289, 2503-2505`), so two calls can land on either side of a publish.
- **Honour revocation.** Go through `runtime.acquire()` (`activation.rs:1053-1062`), not `data_plane()`.
- **Keep internal types out.** Do not expose `LanguageId` or `NoiseClass` (`query.rs:843-846`).
- **Keep it flat.** Fathom folds paths into first-level directories itself.
- **Contract cost (a contract amendment, not just an addition).**
  - The facade exposes exactly the frozen contract's atoms (`embed.rs:10-13`) and is semver-public (`embed.rs:21-28`).
  - The delta test reads the contract's `introduced_v11_atoms` and asserts `atoms.len() == 64` (`tests/public_api_delta_v11.rs:37-43`).
  - It also checks the wrap table against the top-level atoms (`:86-96`) and compares against the checked-in `docs/reviews/FEATURE-020-EXPORT-DELTA-v11.json` (`:148-162`).
  - Adding `index_census`, `IndexCensus` and `CensusFile` means amending the contract JSON, the atom count, the delta file and the wrap table.
  - That test is `#![cfg(feature = "server")]` (`:14`), so an embed-only run does not check it.
  - The method also needs an `OperationKind` for its claim and refusals (`embedded.rs:330-334`; `public_api.rs:413-425`). `OperationKind` is an exhaustive enum pinned by `ALL: [Self; 7]` (`src/lifecycle_identity.rs:135-157`) and re-exported by the facade (`embed.rs:84`).
  - A new variant breaks downstream exhaustive matches. Reusing `SearchSymbols` would mislabel the receipt.

**Behaviour and edge cases**
- **Refusal.** Same as `search_symbols` (`embedded.rs:336-346, 745-752`):
  - `Never` once closed.
  - `OnEvent` while `Loading` or `Refreshing`.
  - `Operator` for `Blocked`.
  - `Never` for `Stopping` or `Stopped`.
- **Generation consistency.** No lock ties the data read to the claim today (see Q8 below). To give the census that guarantee:
  - Store the numeric `publication_generation` in `EmbeddedRuntimeState` (`embedded.rs:138-143`) when it is set at `embedded.rs:288-295`.
  - Refuse with `OnEvent` when the loaded generation does not match it.
- **What set is counted.**
  - `all_files()` walks only the parsed-files map (`query.rs:1244-1247`). That map is filled only from `Parsed` outcomes (`store.rs:4760, 4771-4781, 5369`).
  - Files admitted as metadata only, blocked by content policy or hard-skipped become terminal dispositions and are absent (`store.rs:4713-4727, 4782-4790`).
  - Metadata-only paths are listed by a separate iterator (`query.rs:1249-1256`). The full catalog is `RepositoryManifest` (`src/domain/index.rs:1103-1115`), held on `PublishedGeneration.manifest` (`store.rs:919`).
  - Document which set the census counts.
- **Zero-symbol files.** Parsed files with no symbols must be present with `symbol_count: 0`.

**Tests SymForge should add (each fails at v11.2.0, because the method does not exist)**
1. **`index_census_lists_zero_symbol_files`.** Use a directory whose files parse to zero symbols. Assert those paths are present with `symbol_count == 0`.
2. **`index_census_symbol_counts_match_search_symbols_per_path`.**
   - Use an idle fixture with no paths outside the root.
   - Assert that each file's `symbol_count` equals that path's match count from `search_symbols{None, None, u32::MAX}`, and that both claims carry the same `source_version`.
   - The equality is not guaranteed in general: the outline drops paths outside the root (`query.rs:2616-2618`) and `search_symbols` does not (`embedded.rs:774`).
3. **`index_census_refuses_until_current`.** A call during `Loading` refuses with `SourceUnavailable` and `OnEvent`.
4. **Contract.** Amend the contract atoms and count, regenerate the delta file, and add the names to the `contract` module (`embed.rs:90-106`) and to the wrap table.

**What Fathom will change once this lands**
- **Capture.** After `search_symbols`, capture calls `index_census` under the same before/after runtime-view check (`fathom: crates/fathom-repo/src/lib.rs:809-817`). It writes per-module totals to `modules.json` (L18).
- **Module keys (a side effect to plan for).** Zero-symbol files at the repository root would make `common_root` return `""` (`fathom: crates/fathom-repo/src/lib.rs:1197-1217`). That changes every module key under OD-7.
- **Re-index (rg-39, no lane).** Open, wait for `Current`, call `index_census`, close. This still costs a full index.
- **Run JSON census.** The `files` count in `GET /runs/{id}` comes from files with symbols (`fathom: crates/fathom-server/src/lib.rs:891-903`). Switching it to census totals changes what the count means, and Fathom will record that. No UI consumes this JSON yet (`fathom: docs/PRODUCT-GAPS.md:239`).

---

### CR-3. Live indexing progress counters (need N2; speculative)

**The problem in Fathom**
- While a run indexes, the `GET /runs/{id}` indexing stage reports elapsed time and `null` counts (`fathom: crates/fathom-server/src/lib.rs:247-252, 310-327`). Counts appear only after capture returns (`fathom: crates/fathom-server/src/lib.rs:799-804, 891-903`).
- No template calls `/runs`, so there is no Home generating screen to show counts on yet (`fathom: docs/PRODUCT-GAPS.md:239`; `fathom: docs/STATE.md:425, 429`).
- **The owner decision is not recorded.**
  - The task description says the owner chose elapsed-only.
  - The ledger at 7f88ade still says "no lane — owner call pending" (`fathom: docs/PRODUCT-GAPS.md:630`; `fathom: GOAL_FATHOM_EMPTY_THE_LEDGER_REMAINING.md:188`).
  - Row rg-20 is OPEN and lists both elapsed-only and a progress callback (`fathom: docs/STATE.md:427`).
- `fathom: docs/PRODUCT-GAPS.md:251-253` says indexing shows files and symbols "because SymForge reports them". That is false at v11.2.0, and Fathom will correct it.

**What SymForge does today**
- `SourceRuntimeView` holds only `binding_identity`, `current_publication_identity`, `observer_epoch`, `phase` and `source_version` (`public_api.rs:392-400`).
- The phase stays `Loading` until the whole reload publishes (`embedded.rs:189-194, 288-296`).
- Counts appear only in `info!` log lines (`store.rs:5279-5283, 4793-4798, 5340-5348`). Those are not a contract, and Fathom will not parse them.
- `SourceRuntimeView` has public fields and is not `#[non_exhaustive]` (`public_api.rs:392-394`), so adding fields to it would be a breaking change.

**Recommendation: defer.**
- The reload path is crate-private, so counters can be added later at no semver cost.
- Do not build a shared "reload observer" struct in CR-1 just to hold one cancel flag.
- A new handle method has the same contract cost as CR-2 (atom count, delta file).

**Proposed API (only if the owner schedules rg-20)**

```rust
// proposed
impl EmbeddedSourceHandle {
    /// Counters for the reload in progress, or the last completed one.
    /// Infallible, lock-free, valid in every phase.
    pub fn index_progress(&self) -> IndexProgress;
}

// proposed
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexProgress {
    pub files_discovered: u64,
    pub files_parsed: u64,
    pub symbols_found: u64,
}
```

- **Define the counts precisely.**
  - **`files_discovered`.** The scout reports catalog entries and, separately, entries the index can execute (`store.rs:5279-5283`). Pick one and name it.
  - **`files_parsed`.** Count after the fold, or name the field for attempts:
    - A `Parsed` outcome is built at `store.rs:4760`, but the staged-bytes handoff can still turn a parsed file into a hard skip (`store.rs:4749-4758`).
    - The later circuit-breaker fold returns its own files and dispositions (`store.rs:4815-4826`; described at `store.rs:4569-4570`).
- **Where the counters live.** An `Arc` of atomics on `EmbeddedBinding`, next to `state` (`embedded.rs:157`).

**Behaviour and edge cases**
- Counters reset at the start of each reload. Fathom only reads them during `Loading`.
- The tick scout (`embedded.rs:299-307`) does not touch them.
- Counts only rise within one reload and may lag by a few items.
- After a cancelled close (CR-1), counters keep their last values.
- The counts at `Current` need not equal CR-2's census totals:
  - the fold and the handoff above can change them;
  - the reload merges admissions that landed after its watermark (`store.rs:2623`);
  - the outline drops paths outside the root (`query.rs:2616-2618`).

**Tests SymForge should add**
1. **`index_progress_rises_during_loading`.** With the CR-1 gate held after N files, `files_parsed == N`, and `files_discovered` equals the scout's count as defined above.
2. **`fingerprint_poll_does_not_move_progress`.** Counters stay the same across several poll intervals on an idle `Current` source.

**What Fathom will change once this lands**
- `CaptureFn` gains a progress sink.
- While Indexing, the stage JSON fills `files` and `symbols` from `index_progress()` instead of `null`.

---

## 3. What Fathom does NOT need from SymForge

- **No async API.** The embed surface is synchronous std code, and Fathom runs it on blocking threads.
- **No Windows build fix, pending proof.**
  - At v11.2.0 the physical-root probe calls `GetFileInformationByHandle` through the `windows` crate (`src/index_lifecycle/physical_root.rs:92-134`; `Cargo.toml:153, 159`).
  - From reading the code, Fathom's comments about symforge 11.1.5 on Windows look stale. They come in two wordings:
    - "does not build on Windows": `fathom: crates/fathom-repo/Cargo.toml:10-12`, `fathom: crates/fathom-repo/src/lib.rs:86-89, 1325-1328`
    - "does not compile for x86_64-pc-windows-msvc": `fathom: crates/fathom/Cargo.toml:31-36`, `fathom: crates/fathom/src/lib.rs:17-20`, `fathom: crates/fathom/src/main.rs:11-14`
  - The ledger records a Windows MSVC compile and test of v11.2.0 only on the unmerged PR #58 (`fathom: docs/STATE.md:409`).
  - No job on main builds `--features symforge` (`fathom: .github/workflows/ci.yml:68, 71, 101, 104, 136, 139, 142, 145`; `fathom: .github/workflows/release.yml:169`).
  - Fathom removes the comments once #58 lands.
- **No new fields on `SourceRuntimeView`.** That would be a breaking change (see CR-3).
- **No non-blocking close, and no honoured receipt deadline, once CR-1 lands.**
  - Fathom never calls `begin_close`, `SourceCloseReceipt::wait`, `begin_shutdown` or `ShutdownReceipt::wait`.
  - For the record: `begin_close` takes no deadline and closes synchronously (`embedded.rs:690-713`).
  - `SourceCloseReceipt::wait` ignores its deadline (`embedded.rs:1035-1047`, `let _ = deadline` at `:1039`). So does `EmbedShutdownReceipt::wait` (`public_api.rs:367-375`).
  - `ReceiptWaitError::DeadlineElapsed` is never produced (`embedded.rs:993-998`).
  - Fixing that is SymForge's call.
- **No snapshot restore on embed open**, no directory-tree API, and no file listing through `search_text`.
- **No change to truncation.** Fathom passes `u32::MAX`, and `truncated` is computed after collecting everything (`embedded.rs:800-814`).
- **No fix to `path_prefix`.** Fathom does not pass one. For information: the trailing `/` is trimmed and a plain `starts_with` is used, so `src/` also matches `srcx/…` (`embedded.rs:426-434, 774-780`).
- **No change to the tree-sitter-scss patch.** SymForge carries `[patch.crates-io] tree-sitter-scss` (`Cargo.toml:178-181`), and Fathom mirrors it at its workspace root (`fathom: Cargo.toml:13-20`).
- **Already available at v11.2.0, and enough for Fathom:**
  - `engine_info()` (`embed.rs:47-63`)
  - `request_refresh()` (`embedded.rs:365-380, 928-957`)
  - `search_symbols` with its readiness refusals (`embedded.rs:330-363, 734-823`)
- **Not a protection Fathom can rely on at v11.2.0:** the one-handle-per-source refusal.
  - It only holds within one runtime (`embedded.rs:492-496`; `public_api.rs:513-520`), and Fathom acquires a fresh runtime on every capture (`fathom: crates/fathom-repo/src/lib.rs:1447`).
  - CR-0 makes it process-wide.

## 4. Questions for the SymForge session

**Still open**
- **Q3.** When a reload's scout coverage is degraded, freshness is not `Current` (`store.rs:2615, 2641-2657`). The source goes `Blocked` and reloads on every 200 ms tick (`embedded.rs:237-244, 284-286`). Is that intended for an embedder? Today such a source repeats full reloads until Fathom's 120 s timeout.
- **Q4.** Is there an intended way for a short-lived embedder to avoid creating the state directory in the repository, or to avoid a refresh during a capture? What the code does today:
  - An embed open creates `<root>/.symforge` when it can. The placement is chosen at `discovery/mod.rs:1617-1623`, and the directory is created by `create_dir_all` at `discovery/mod.rs:1690`, reached from `public_api.rs:545`.
  - That directory is left out of the scout (`SourceExclusions::for_state_placement`, `discovery/mod.rs:157-178`; used at `embedded.rs:262, 300`), so creating it does not by itself cause a refresh.
  - The worker re-scouts the rest of the tree every 200 ms while a handle is open (`embedded.rs:23, 231, 299-307`).
  - A change elsewhere in the tree can start a refresh. That refresh moves `source_version`, and Fathom then fails with `IndexDrift` (`fathom: crates/fathom-repo/src/lib.rs:809-817`).
- **Q6.** Is CR-2 or CR-3 a minor release under `embed.rs:21-28`?
  - Either one amends the frozen atom list (`tests/public_api_delta_v11.rs:43`) and may need an `OperationKind` variant (`lifecycle_identity.rs:135-157`).
  - For context: `EmbeddedSourceHandle` already has public `close`, `identity`, `key`, `is_open` and `finalize` (`embedded.rs:640-986`), which are not among the contract's atoms (`public-api-v11.json:3344-3349`).
- **Q9.** What handles or threads remain after drop returns, on Windows, under each coupling policy? See CR-1, "Windows file handles after close".
- **Q10.** What bound can SymForge state or measure for "close returns promptly"? See CR-1, "Cancel mid-parse".

**Answered by the code at b549aa7**
- **Q1 (other callers of the reload).** Yes, there are other callers.
  - `reload_for_binding_with_exclusions` is also called by `reload_for_binding` (`store.rs:2546`), `reload_for_state_placement` (`store.rs:2558`), the daemon (`daemon.rs:3651, 3897`) and a watcher test (`watcher/mod.rs:3317`).
  - `admit_and_parse_entries` is shared with `LiveIndex::load` (`store.rs:4553-4556, 4937`).
  - Those callers need a path that is never cancelled.
- **Q2 (reopen during teardown).** This is a real defect, and Fathom can reach it today (see CR-0).
  - A second embed open of the same root through another runtime joins the live admission instead of refusing. Closing either open then revokes the other (`registry.rs:382-398, 493-511`; `activation.rs:1276-1282`; `embedded.rs:408`).
  - SymForge's design forbids that shared state (design `:2225-2234`).
  - The fix makes the sole-owner refusal process-wide and does not depend on CR-1.
- **Q3 (latch, own ticket).**
  - `physical_replacement_latched` is set at `store.rs:2608-2610`. The only other write is its initialisation to false (`store.rs:1627`).
  - Once set, freshness stays degraded for the life of the handle (`store.rs:2612-2613, 2641-2657`).
  - The source then stays `Blocked` and runs a full reload on every tick until it closes (`embedded.rs:237-244, 284-286`).
  - This deserves its own ticket.
- **Q5 (what `all_files()` includes).** Parsed files only, as described in CR-2's "What set is counted".
- **Q7 (up to 200 ms stop delay).** This is the lost-wakeup shape described in CR-1. The `wait_timeout_while` change fixes it.
- **Q8 (claim and data in step).** No, nothing ties the data read to the claim.
  - `current_claim` copies the state under the state mutex and releases it on return (`embedded.rs:335-363`). `search_symbols` reads the index later, without that lock (`embedded.rs:762-770`).
  - The worker publishes (`store.rs:2669`) before it updates the claimed state (`embedded.rs:288-295`), with a full scout in between (`embedded.rs:279`).
  - A claim taken just before a refresh can therefore be paired with newer content.
  - Fathom's before/after check compares only `source_version` and the publication identity (`fathom: crates/fathom-repo/src/lib.rs:809-812`), so a read that lands in that window passes it. This comes from reading the code, not from a measurement.

## 5. Release and handoff

1. **Order of work.**
   - SymForge lands CR-0 first. It is a live defect, needs no public API change, and can ship without CR-1.
   - SymForge then lands CR-1 and tags a release.
   - CR-2 comes only if accepted with its contract amendment. CR-3 comes only if the owner schedules rg-20.
   - The release notes should say which of the open questions were answered.
2. **Fathom bumps its pin.**
   - Update the SymForge checkout refs: `fathom: .github/workflows/ci.yml:57, 93, 128` hard-code `v11.2.0`, and `fathom: .github/workflows/release.yml:50` sets `SYMFORGE_REV`.
   - Regenerate `Cargo.lock`:
     - SymForge is a path dependency (`fathom: Cargo.toml:43`), so its `symforge` entry has no source or checksum, only `version` (`fathom: Cargo.lock:3643-3644`).
     - The entry does list SymForge's dependencies (`fathom: Cargo.lock:3645` onward), and the lock holds every crate those pull in.
     - After moving the checkout ref, regenerate the lock against the new sibling checkout, for example with `cargo update -p symforge`.
     - Commit the lock whenever the version or the dependency set changes. CI runs with `--locked` (`fathom: .github/workflows/ci.yml:68, 71, 101, 104, 136, 139, 142, 145`), and so does the release workflow (`fathom: .github/workflows/release.yml:103, 156, 169`).
   - Remove the stale 11.1.5 comments.
   - Update the local mirrors of `SourceRuntimePhase`, `SourceRefusalKind` and `RetryAdvice`. Those `From` impls are exhaustive, so a new variant in any of them is a compile error in Fathom (`fathom: crates/fathom-repo/src/lib.rs:1348-1385`). Fathom does not match on `OperationKind`.
3. **Separate from this brief.**
   - Fathom's release build compiles without the `symforge` feature (`fathom: .github/workflows/release.yml:169`), and no main CI job builds that feature (`fathom: .github/workflows/ci.yml:68, 71, 101, 104, 136, 139, 142, 145`).
   - Fixing that is Fathom's own work in PR #58, which is open (`fathom: docs/STATE.md:409`).
   - The shipped exe will not exercise these changes until #58 merges.
4. **Ledger rows each change unblocks:**

| SymForge change | Fathom row / lane |
|---|---|
| CR-0 process-wide sole-owner refusal | No ledger row found in `docs/STATE.md` or `docs/PRODUCT-GAPS.md`. Fathom keys the one-run rule on the canonicalised working-tree top-level (`fathom: crates/fathom-server/src/lib.rs:722-735`). That change is permanent: under the fix, a colliding run fails when it opens |
| CR-1 prompt close while indexing | `stop-mid-index-abort`: BANK, no lane; reopens when the embed API offers a cancellable reload or a non-blocking close (`fathom: docs/STATE.md:366`) |
| CR-2 `index_census` (adds zero-symbol files; L18 as scoped can start at v11.2.0) | `rg-29-module-inventory` / L18 (`fathom: docs/STATE.md:435`; `fathom: docs/PRODUCT-GAPS.md:1503-1531`) |
| CR-2 `index_census`, plus CR-1 so a Re-index can be stopped | `rg-39-reindex-counts`: no lane, audit suggests BANK (`fathom: docs/STATE.md:443`) |
| CR-3 `index_progress` | `rg-20-live-index-counts`: no lane, owner call pending (`fathom: docs/STATE.md:427`; `fathom: docs/PRODUCT-GAPS.md:630`) |