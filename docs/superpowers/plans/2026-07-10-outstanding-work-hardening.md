# SymForge Outstanding-Work Hardening Implementation Plan

> **For Codex:** REQUIRED SUB-SKILLS: use `superpowers:subagent-driven-development`
> for independent slices, `superpowers:test-driven-development` for every
> behavior change, and `superpowers:verification-before-completion` before any
> completion claim.

**Goal:** Make SymForge a trustworthy, token-saving replacement for the common
repository tools an LLM otherwise chains together, while closing every current
item in `docs/OUTSTANDING-WORK.md` against executable evidence.

**Architecture:** Keep the existing daemon, deterministic project IDs,
`WorkingSet`, `LiveIndex`, and local snapshots. Replace mutable session
retargeting with an immutable home plus explicit project routing. Store project
instances behind per-project locks so the registry lock never spans indexing
or checkpoint IO. Carry project/freshness evidence through daemon responses,
make reconnect/descriptors/session cleanup deterministic, and fix only the
admission and client-init residuals that current code proves are real.

**Tech stack:** Rust, Tokio, Axum, rmcp, tree-sitter, postcard snapshots,
parking_lot locks, existing test fixtures, PowerShell only for reproducible
dogfood/reporting. No new runtime dependency.

**Truth rule:** `docs/OUTSTANDING-WORK.md` is a ledger, not a specification.
Current source, tests, runtime behavior, and newer shipped contracts win. Close
stale items with evidence instead of implementing them.

**Command rule:** The user's explicit Terminal Commander instruction is the
narrow exception to the repository's Four-Pillars default: run every Cargo
command through Terminal Commander with the active Cargo rule pack. Direct
shell is reserved for a stated pillar/Terminal-Commander limitation. Never
stream full Cargo logs into model context.

**Focused-test rule:** Every red/green command names the fully-qualified test
and uses libtest `--exact`. The Terminal Commander receipt must report at least
one executed test; `0 tests` is a failed gate, never a pass.

**Execution order:** Task 1, Task 2 (including session-state partition), Task 6,
Task 3, Task 4, Task 5, Task 7, Task 9, Task 8, Tasks 10-13. The section numbers
group related work; this dependency order is authoritative.

---

### Task 1: Close Feature 018's real residuals

**Files:**

- Modify: `src/protocol/tools.rs`
- Modify: `src/live_index/search.rs`
- Test: inline tests in `src/live_index/search.rs` and `src/protocol/tools.rs`

**Step 1: Add red browse-orientation tests**

Add a fixture with many `new` functions across files plus several distinct
important symbols. Assert that browse mode is deterministic and that one exact
generic `(name, kind)` cannot occupy the whole page. Keep the engine-level
structural neutrality assertion, then add a public-handler regression around
the actual frecency bump hook/store so a browse request proves it records no
commitment signal. Do not pretend the pure `&LiveIndex` search function can
mutate a store it does not own.

**Step 2: Prove the regression**

Run via Terminal Commander:

```text
cargo test --lib live_index::search::tests::test_browse_deduplicates_repeated_generic_names -- --exact --test-threads=1
cargo test --lib protocol::tools::tests::test_browse_handler_is_frecency_neutral -- --exact --test-threads=1
```

Expected: the new diversity assertion fails on the current implementation;
the frecency assertion must fail if browse records a commitment signal.

**Step 3: Implement the minimum browse-only fix**

After the existing importance comparator, collapse browse candidates by exact
`(name, kind)` and retain the highest-ranked deterministic representative.
Do not deduplicate query mode, where same-name definitions are intentional.
Correct the stale `search_symbols` description in `src/protocol/tools.rs` that
still promises path/line ordering.

**Step 4: Verify**

Run the focused engine and public-handler browse/search tests. Feature 018
bookkeeping and the historical Grok banner are handled atomically with the
canonical prompt in Task 12, so this commit cannot contain a dangling link.

**Step 5: Commit**

```text
fix: finish source-focused browse hardening
```

---

### Task 2: Move daemon projects behind per-project slots

**Files:**

- Modify: `src/daemon.rs`
- Test: inline daemon tests in `src/daemon.rs`

**Step 1: Add red lock-isolation tests**

Construct two loaded projects. Stall project A's mutation lane and prove project
B's `session_runtime`, read dispatch, and lifecycle lookup complete without
waiting. Also prove concurrent same-project reloads serialize while ordinary
same-project reads continue from the previously published `LiveIndex` until the
new generation is atomically published.

**Step 2: Prove current global blocking**

```text
cargo test --lib daemon::tests::test_project_b_reads_while_project_a_reloads -- --exact --test-threads=1
cargo test --lib daemon::tests::test_same_project_reads_prior_generation_during_reload -- --exact --test-threads=1
cargo test --lib daemon::tests::test_sessions_on_same_project_do_not_share_context_cache -- --exact --test-threads=1
```

Expected: current inline `ProjectInstance` storage cannot isolate lifecycle
mutation from registry lookup or preserve the same-project read witness.

**Step 3: Introduce the smallest slot type**

Use `Arc<ProjectSlot>` with a short-lived metadata lock and a separate
per-project mutation mutex; do not put the whole `ProjectInstance::reload`
behind a slot write lock. Clone the `SharedIndex`/needed handles under the
metadata lock, release registry and metadata locks, build/reload through the
existing atomic `SharedIndex` publication path, then briefly update watcher and
metadata state. Same-project reads therefore keep serving the old published
generation while parsing proceeds.

**Step 4: Migrate lifecycle paths**

Update `open_project_session`, `register_session_for_existing_project`,
`intern_base_for_project`, `refresh_working_set_bases`, `index_folder_for_session`,
`index_folder_additive`, `add_project_to_session`, `remove_project_from_session`,
`close_session`, `list_projects`, `list_sessions`, `project_health`, `health`,
`session_runtime`, `ProjectInstance::reload`, handlers, and watcher cleanup.
Load a new instance outside the map lock, race-check insert, and activate through
the winning slot.

**Step 5: Partition session-scoped server state**

Add a session-local server/context cache keyed by project ID to `SessionRecord`
and stop serving every session through `ProjectInstance.server`. Two sessions
on the same shared index must not share `SessionContext`, CCR retrieval cache,
or other request/session commitment state. Keep the `SharedIndex` and truly
project-wide metrics shared. Add a regression reproducing the observed
cross-session cache hit before the fix.

**Step 6: Verify and commit**

Run daemon lifecycle/race tests, then:

```text
refactor: isolate daemon project synchronization
```

---

### Task 3: Make home identity immutable and `index_folder` additive

**Files:**

- Modify: `src/daemon.rs`
- Modify: `src/protocol/tools.rs`
- Modify: `src/protocol/format.rs`
- Test: inline daemon and protocol tests

**Step 1: Replace retarget expectations with red immutable-home tests**

Cover two logical callers sharing one connection: opening/indexing B must leave
unqualified reads and status bound to home A. Assert `index_folder(path=B)` and
the compatibility spelling `add=true` produce the same open/refresh semantics.
Assert the response contains deterministic project ID, display name/root, counts, and
checkpoint outcome.

Also pin the breaking transport contract: daemon mode opens/refreshes without
retargeting, local/embedded mode retains its one-index reset behavior, and a
daemon-proxy failure must refuse rather than silently execute the same call as
a destructive local fallback.

**Step 2: Prove the old destructive behavior**

```text
cargo test --lib daemon::tests::test_index_folder_open_keeps_immutable_home -- --exact --test-threads=1
```

Expected: current `test_index_folder_rebinds_session_to_new_project_root`
behavior contradicts the new witness.

**Step 3: Make the session binding immutable**

Rename `active_project_id` to `home_project_id` and remove ordinary mutation.
Keep the existing `WorkingSet`; opening a project joins its `session_ids`,
refreshes its base, and never evicts or replaces home. Retire test-only
`set_active_project`/`remove_project_from_session` paths unless a live caller is
found.

Route both omission and `add=true` through the existing durable idempotency
ledger. Same key/same canonical request replays the stored identity/receipt;
same key/different request fails deterministically. Never retain the current
additive path's no-ledger exception for the new default.

**Step 4: Persist successful full indexes**

After a successful daemon full reload, invoke the existing atomic snapshot path
and return its receipt. Publish the new index only after reload succeeds; keep
the prior valid instance/snapshot on reload error. A snapshot failure does not
roll back the successfully published in-memory generation; it returns an
explicit degraded checkpoint outcome and leaves the prior valid snapshot in
place. Task 7 later carries the same facts as structured metadata. Preserve
current local/embedded reset behavior and idempotency-key conflict semantics.

**Step 5: Verify recovery and commit**

Start a fresh daemon/client from the written snapshot and assert B restores
with the same ID/counts while A remains home.

```text
feat: make daemon project opens non-destructive
```

---

### Task 4: Centralize explicit project routing across read/guidance tools

**Files:**

- Modify: `src/daemon.rs`
- Modify: `src/protocol/read_tools.rs`
- Modify: `src/protocol/search_tools.rs`
- Modify: `src/protocol/tools.rs`
- Modify: `src/protocol/smart_query.rs`
- Modify: `src/protocol/investigation.rs`
- Modify: `src/stel/planner.rs`
- Modify: `src/stel/executor.rs`
- Modify: `src/stel_core/types.rs`
- Modify: `tests/strict_client_schema_compat.rs`
- Modify: `tests/stel_param_disposition.rs`
- Test: inline protocol/daemon tests plus strict schema tests

**Step 1: Add red routing/schema table tests**

Create one table covering `project` selector parity for `get_symbol`, `get_symbol_context`,
`get_file_context`, `get_file_content`, `get_repo_map`, `search_files`,
`find_dependents`, `diff_symbols`, `what_changed`, `analyze_file_impact`,
`validate_file_syntax`, `explore`, `ask`, `conventions`, `edit_plan`,
`investigation_suggest`, and compact `symforge`.
For each tool, pin optional `project` schema and verify explicit B returns B
while omission returns home A. Keep `project/projects` mutually exclusive and
strict-client-compatible.

`context_inventory` remains session-scoped, receives no project selector, and
must read the session-local context introduced in Task 2. Only set-valued
discovery (`search_symbols`, `search_text`, `search_files`, `find_references`)
accepts `projects`; exact reads, guidance, git, impact, and edits accept only
one `project`.

**Step 2: Prove missing parity**

```text
cargo test --lib daemon::tests::test_project_routing_parity_table -- --exact --test-threads=1
cargo test --test strict_client_schema_compat -- --test-threads=1
```

Expected: only the three current cross-project discovery verbs pass.

**Step 3: Add one shared resolver**

Add `DaemonState::runtime_for_target(session_id, project)` that resolves an ID
or unique current `project_name` only within the session's open working set.
No persistent alias model is introduced. Omission selects
immutable home. Unknown/not-open/ambiguous values return deterministic candidate
data and never trigger indexing or frecency.

**Step 4: Route before local decode**

In `call_tool_handler`, peek and validate the target, select the project slot,
strip routing-only fields, then call the existing per-project implementation.
Do not duplicate tool logic. For set-valued `search_files`, first expose the
existing per-project ranked file hits as a structured internal result, then
merge attributed hits across targets with one deterministic global cap and
token budget before formatting. Do not parse/merge the current formatted
strings or apply a separate ranking policy.

**Step 5: Enable compact-facade routing**

Remove the current surface-only limitation from compact `symforge`; pass the
selected project through each planned primitive. Update the real compact path
in `src/stel/planner.rs`, `src/stel/executor.rs`, `src/stel_core/types.rs`, and
`tests/stel_param_disposition.rs` in addition to the protocol handler. Preserve
compact mode as an explicit opt-in and add a default-surface probe asserting
the normal MCP connection lists the documented full surface without a helper.

**Step 6: Verify and commit**

```text
feat: route repository tools by explicit project
```

---

### Task 5: Route structural edits without cross-root ambiguity

**Files:**

- Modify: `src/daemon.rs`
- Modify: `src/protocol/edit.rs`
- Modify: `src/protocol/edit_tools.rs`
- Modify: `src/stel_core/types.rs`
- Modify: `src/protocol/tools.rs` (`symforge_edit_facade_tool`)
- Modify: `src/protocol/surface_probe.rs`
- Modify: `src/stel/surface_list.rs`
- Modify: `tests/strict_client_schema_compat.rs`
- Test: existing edit safety/worktree tests plus new daemon routing fixtures

**Step 1: Add red edit-routing tests**

Assert an explicit B edit mutates only B, an omitted edit mutates home A, and
an unknown/ambiguous target writes nothing. Assert `working_directory` must be
the selected project itself or a worktree that `worktree::resolve_target_path`
proves belongs to that repository; an unrelated root rejects before preview or
apply. Pin replay behavior to the selected canonical repository identity.

Run the red witness:

```text
cargo test --lib daemon::tests::test_explicit_project_edit_routes_and_preserves_worktree -- --exact --test-threads=1
```

**Step 2: Add one optional batch-level project**

Add `project` to single and batch structural-edit schemas. Keep each batch
single-project so validation, rollback, and idempotency remain one transaction;
reject per-edit cross-project mixtures rather than inventing distributed atomic
rollback. Add the selector at batch level only; do not put it on `SingleEdit` or
`InsertTarget` and thereby create conflicting nested routing.

**Step 3: Reuse the shared daemon resolver**

Resolve the project before invoking existing edit safety. Preserve worktree
routing within that project and the current generation/hash/idempotency guards.
Re-run compact schema-byte guards in `surface_probe` and `surface_list` after
adding the field.

**Step 4: Verify and commit**

```text
feat: make structural edits project-explicit
```

---

### Task 6: Remove the global snapshot bottleneck

**Files:**

- Create: `src/path_locks.rs` (only if the existing edit lock can be reused
  without dependency inversion)
- Modify: `src/lib.rs` (only when adding the shared helper)
- Modify: `src/live_index/persist.rs`
- Modify: `src/protocol/edit.rs` (only when extracting the shared helper)
- Test: inline persistence tests

**Step 1: Add red lock-identity/concurrency tests**

Assert two writes to the same canonical `.symforge/index.bin` share a lock,
while different snapshot paths receive different locks and can reach their
atomic-write critical sections independently. Assert `reset_snapshot_state`
uses the same lock and cannot delete a temp/final artifact during a write.

```text
cargo test --lib live_index::persist::tests::test_snapshot_path_locks_isolate_distinct_projects -- --exact --test-threads=1
```

**Step 2: Replace `SNAPSHOT_WRITE_LOCK`**

Use a standard-library registry of weak per-path mutexes. First ensure and
canonicalize the `.symforge` directory (the final `index.bin` may not exist),
then key `dir.join(INDEX_FILENAME)`, clean dead weak entries opportunistically,
and retain same-path serialization plus temp-write/rename atomicity. Reuse the
existing weak path-lock pattern from `src/protocol/edit.rs` by extracting a tiny
crate-level helper if that keeps module direction clean; otherwise document why
the persistence-local copy is intentionally separate.

Use a unique same-directory temp name containing PID plus an atomic counter so
local and daemon processes cannot clobber one fixed `index.bin.tmp`. Atomic
rename still publishes the final path; in the unsupported concurrent
cross-process same-root case, the last complete writer may win but corruption
or temp collision is forbidden. Extend stale-temp cleanup to the unique prefix.

**Step 3: Verify and commit**

```text
fix: serialize snapshots per project path
```

---

### Task 7: Carry project and freshness evidence in responses/status

**Files:**

- Modify: `src/daemon.rs`
- Modify: `src/protocol/mod.rs`
- Modify: `src/protocol/result_status.rs`
- Modify: `src/protocol/tools.rs`
- Modify: `src/stel/status.rs`
- Modify: `src/stel_core/types.rs`
- Modify: `src/protocol/format.rs`
- Test: daemon/protocol result-status and status fixtures

**Step 1: Add red trust-envelope tests**

For home and explicit B calls, assert machine-readable metadata identifies
project ID, canonical root, current generation, snapshot/load source, and any
known index-health warning. Existing tool-specific partial/quarantine/truncation
text must remain truthful and actionable; a universal typed model for those
conditions is not part of this migration. Assert default human text remains
unchanged where compatibility requires it and cover both local and daemon-backed
responses.

```text
cargo test --lib daemon::tests::test_tool_receipt_carries_project_evidence -- --exact --test-threads=1
cargo test --lib protocol::tools::tests::test_local_tool_meta_carries_project_evidence -- --exact --test-threads=1
```

**Step 2: Preserve a structured daemon receipt**

Introduce a typed daemon tool receipt carrying text plus selected-project
evidence through `call_tool_handler` -> `DaemonSessionClient::call_tool_value`
-> `SymForgeServer::proxy_tool_call` -> the public statused wrapper. Keep a
compatibility text accessor for direct bridge/hook callers, but never downcast
to `String` and then try to reconstruct evidence. Local handlers build the same
metadata directly from their bound index/root.

**Step 3: Add `status(detail="projects")`**

Render the session's open-project inventory: ID, display name/root, home marker,
generation, watcher/index state, opened timestamp, session last-seen evidence,
and snapshot/checkpoint state. Treat the existing `project_name` as display
text, not as a new persistent alias entity. Add the `Projects` detail variant to
`StelStatusDetail` and its strict schema tests. Expose the same inventory from
full-surface `health`/`health_compact` so the documented 36-tool surface can
list and select projects without compact `status`. Default outputs must remain
compatible unless project detail is requested.

**Step 4: Verify and commit**

```text
feat: expose project-scoped trust evidence
```

---

### Task 8: Make reconnect and runtime descriptors multi-session safe

**Files:**

- Modify: `src/daemon.rs`
- Modify: `src/main.rs`
- Modify: `src/sidecar/port_file.rs`
- Modify: `src/sidecar/server.rs`
- Modify: `src/cli/hook.rs`
- Modify: `src/cli/update.rs`
- Modify: `src/protocol/tools.rs`
- Test: inline daemon/sidecar/hook tests

**Step 1: Add red reconnect/descriptor tests**

Open A+B, kill the daemon, reconnect, and assert A is still home and B remains
explicitly routable with the same deterministic ID. Start two adapters on one
root; closing one must not delete or invalidate the other's descriptor. Hook
lookup may choose the deterministically freshest healthy session when multiple
sessions have the same canonical home/project ID; it must reject conflicting
root/identity matches instead of choosing last writer.

```text
cargo test --lib daemon::tests::test_reconnect_reopens_home_and_working_set -- --exact --test-threads=1
cargo test --lib sidecar::port_file::tests::test_per_session_descriptors_do_not_delete_siblings -- --exact --test-threads=1
```

**Step 2: Store immutable reconnect state**

Replace `DaemonSessionClient.project_root` with immutable home root plus a
shared ordered map of successfully opened roots/IDs. On reconnect, open home,
reopen siblings, verify IDs, and fail closed on mismatch.

**Step 3: Replace the fixed ownership file**

Write one atomic JSON descriptor per adapter/session containing session ID,
project/root, PID, and heartbeat/update time. Cleanup removes only the caller's
descriptor. Hook lookup scans the caller root's descriptors, discards stale
entries, validates root/project identity, then selects freshest health with a
stable session-ID tie break. Migrate sidecar server/status and update cleanup
consumers together so fixed and per-session records cannot disagree. Keep the
legacy pointer only as a read-compatible migration aid.

Retest `.symforge/codex-mcp-call.ps1` as non-shipped dogfood infrastructure;
preserve it only if it still needs the legacy pointer, and do not make an
ignored local helper part of the public compatibility contract.

**Step 4: Verify and commit**

```text
fix: recover daemon sessions without descriptor races
```

---

### Task 9: Enforce daemon uniqueness and reap stale sessions

**Files:**

- Modify: `src/daemon.rs`
- Modify: `src/main.rs`
- Create: `tests/daemon_singleton.rs`
- Test: daemon startup/session lifecycle tests

**Step 1: Add red process/lifecycle witnesses**

Race the built foreground `symforge daemon` and auto-spawn against the same
`SYMFORGE_HOME` in an integration test; assert one runtime record and one live
daemon. Create an expired heartbeat and assert the session closes through the
normal GC path, removing orphan watchers/projects once.

```text
cargo test --test daemon_singleton -- --test-threads=1
cargo test --lib daemon::tests::test_reaper_rechecks_heartbeat_before_close -- --exact --test-threads=1
```

**Step 2: Share the startup guard**

Factor a non-spawning guarded-start seam from the existing start lock,
compatible-daemon check, and incompatible-record cleanup. Auto-spawn may call
the spawning path; foreground/service `symforge daemon` acquires the guard and
binds in the current process when no daemon exists. Never overwrite a live
runtime record. Preserve distinct `SYMFORGE_HOME` isolation.

**Step 3: Add a bounded reaper**

Spawn one daemon-owned interval task with an explicit TTL constant/environment
override. Collect `(session_id, observed_last_seen)` candidates, then call
`close_session_if_expired` which rechecks the same observation/cutoff under the
sessions write lock and atomically removes/claims the record before shared
project cleanup. A heartbeat that wins the race preserves the session; once the
reaper claims it, heartbeat fails rather than resurrecting it. Store the reaper
task in `DaemonHandle` and abort/join it during shutdown so restarts/tests cannot
leak background tasks. Expose last-seen/TTL evidence in detailed status.

**Step 4: Verify and commit**

```text
fix: enforce one daemon and reap stale sessions
```

---

### Task 10: Preserve generated-output admission in the watcher

**Files:**

- Modify: `src/live_index/store.rs`
- Modify: `src/discovery/mod.rs`
- Modify: `src/watcher/mod.rs`
- Test: inline watcher/store tests

**Step 1: Add red parity fixtures**

After bulk load demotes an untracked `graphify-out`/cache-like directory,
create a supported source file beneath it and send the watcher path. Assert it
remains Tier 2 with `SkipReason::GeneratedOutput`. Also pin tracked-file rescue,
opt-in inclusion, and non-git fail-open. Add the stronger case where the entire
generated directory appears after initial load, plus transitions back to Tier 1
after tracked rescue or explicit inclusion. Prefix-wide tracked rescue must not
scan every tracked repository path on each watcher event.

```text
cargo test --lib watcher::tests::test_new_generated_output_directory_stays_metadata_only -- --exact --test-threads=1
```

**Step 2: Extract and reuse one policy helper**

Expose the smallest reusable helper from the existing
`untracked_generated_output_demotions`/bulk admission path. Pass project-root
context into the watcher single-file path and compose generated-output policy
with the existing `classify_admission` size/content decision. Do not add a JSON
classifier or a second directory heuristic.

**Step 3: Verify and commit**

```text
fix: keep watcher admission aligned with bulk indexing
```

---

### Task 11: Add native Grok initialization

**Files:**

- Modify: `src/cli/mod.rs`
- Modify: `src/cli/init.rs`
- Modify: `tests/init_integration.rs`
- Test: CLI parse and init integration fixtures

**Step 1: Inspect the installed Grok schema**

Before editing, locate the active Grok configuration and verify its TOML server
shape, command/args/env representation, and workspace-root support. Record the
exact observed schema in the test fixture; do not infer it from another client.

**Step 2: Add red client tests**

Add `InitClient::Grok`, CLI parsing, Grok-only registration, `All` inclusion,
preservation of unrelated TOML/user values, and idempotency tests. Assert native
SymForge command, `RUST_LOG=off`, and workspace-root environment only where the
verified schema supports them.

```text
cargo test --test init_integration test_run_init_grok_preserves_toml_and_is_idempotent -- --exact --test-threads=1
```

**Step 3: Implement a preserving TOML merge**

Reuse an existing TOML facility if already present; otherwise implement the
smallest lossless targeted update without adding a runtime dependency. Do not
globally silence stderr unless live Grok proves it necessary.

**Step 4: Verify and commit**

```text
feat: register symforge with grok init
```

---

### Task 12: Close the ledger and create canonical dogfood artifacts

**Files:**

- Create: `docs/grok-dogfood-prompt.md`
- Create: `docs/reviews/2026-07-10-tool-substitution-scorecard.md`
- Modify: `docs/OUTSTANDING-WORK.md`
- Modify: `tasks/todo.md`
- Modify: `specs/018-dogfood-surface-hardening/tasks.md`
- Modify: `docs/grok_report.md`
- Modify: Feature 018 task/quickstart docs only where evidence changed

**Step 1: Write the canonical prompt**

Base v1 on the historical report and require start/end version, canonical root,
project ID, counts/tiers, clean-checkout delta, every created/indexed artifact,
pre-existence, cleanup, and ending deltas.

**Step 2: Create the replacement scorecard template**

Define fixed rows for file discovery, targeted read, text/symbol search,
caller/dependent tracing, change inspection, and structural edit preview. Each
row records:

- raw-tool context tokens needed to reach the answer;
- SymForge response tokens;
- facts required and facts retained;
- project/freshness evidence;
- recovery/actionability on a forced failure;
- whether the workflow needed any auxiliary raw repository tool.

Task 13 fills the rows from pinned commits and controlled runs. Include
unfavorable cases. A CCR footer without retained-answer proof does not count as
savings.

**Step 3: Resolve every ledger item**

Mark each item implemented, superseded, environment-only, or operator-gated,
with a commit/test/live-evidence pointer. Explicitly close the stale JSON-ratio
and CCR-expansion proposals. Keep Terminal Commander product feedback in the
design/final handoff rather than presenting it as SymForge implementation work.
Update `tasks/lessons.md` only if another user correction occurs.

**Step 4: Validate docs and commit**

Run SymForge syntax/impact/placeholder checks and `git diff --check` through
Terminal Commander.

```text
docs: close outstanding-work ledger with dogfood evidence
```

---

### Task 13: Run the final verification and adversarial product review

**Files:**

- Modify only failing code/tests/docs proven necessary by the gates
- Append final evidence and review to `tasks/todo.md`

**Step 1: Run focused suites after each slice**

Use Terminal Commander and retain structured failure signals plus exit receipts.

**Step 2: Run the full gate**

```text
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed
npm test
```

**Step 3: Dogfood the release binary**

Pin the final SymForge commit and
`E:\project\terminal-commander@782bca0afcae5d4136df45a15b4f91f7fd3a965a`
(or record the new commit if the user changes it before execution). Run only
`target/release/symforge.exe` with an isolated temporary `SYMFORGE_HOME`, fresh
runtime descriptors, and no installed/global daemon reuse. Index both projects,
exercise two independent MCP connections and a shared-connection logical-caller
scenario, then close sessions and remove only the isolated state/fixtures.

For every scorecard row, run a cold and warm trial with identical questions and
explicit token budgets. A controlled benchmark harness may execute the raw
read/rg/glob baseline solely to count UTF-8 output bytes and verify expected
facts; it must not inject the raw source into the working model context. Use one
published metric for both lanes (reported model tokens where available,
otherwise `ceil(utf8_bytes / 4)`), record the metric source, and assert retained
facts before comparing. Verify the default full surface directly, home
immutability, explicit reads/edits, reconnect, checkpoint restore, quarantine
honesty, and the completed scorecard.

**Step 4: Request adversarial review**

Use the adversarial-review workflow against the implementation diff. Resolve
every confirmed correctness/security/token-economy finding or record a concrete
operator decision.

**Step 5: Finalize the task ledger**

Add a `Review` section to `tasks/todo.md` with commits, exact exit receipts,
dogfood evidence, known limitations, and Terminal Commander issue results.

**Step 6: Stop at operator gates**

Do not push, open/merge a PR, publish a release, or run `cargo clean` without
the user's explicit approval for that external/destructive action.
