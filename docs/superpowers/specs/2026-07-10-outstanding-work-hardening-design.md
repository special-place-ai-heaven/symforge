# Outstanding Work Hardening — Design

**Status:** APPROVED · **Date:** 2026-07-10 · **Scope:** close `docs/OUTSTANDING-WORK.md`

**Evidence base:** fresh full-surface SymForge index (`715` files, `21,475`
symbols after the lesson file was added), Feature 012/018 specifications and
implementation, agentmemory and vault recall, three independent read-only
audits, live branch-binary dogfood, and fresh verification gates.

`docs/OUTSTANDING-WORK.md` is an input ledger, not a binding specification.
When it disagrees with current source, tests, runtime behavior, or a newer
shipped contract, the executable evidence wins and the stale ledger item is
closed with that evidence. Small adjacent improvements are in scope when they
remove a demonstrated weakness without introducing speculative machinery.

## 1. Outcome

Finish the outstanding-work ledger without building a second router or
speculative data/CCR machinery.

The product objective remains replacement of the common repository tools an
LLM otherwise chains together: broad file reads, grep/ripgrep, glob/find,
manual symbol/caller tracing, raw patching, and repeated git-diff inspection.
SymForge should answer those jobs with less context, stronger structure, and
equal or better evidence. Routing, persistence, and recovery work is justified
only where a demonstrated defect prevents that substitution; it is not an end
state of its own.

The daemon already owns a multi-project registry, shared immutable bases, and a
per-session `WorkingSet`. The remaining isolation failure is caller identity:
one MCP adapter connection is one daemon session, while a harness may share
that connection across its main agent and subagents. Destructive retargeting of
that shared session therefore moves every caller together.

The approved correction is:

1. keep the existing daemon and project registry;
2. make the connection's home project immutable after initialization;
3. open projects explicitly and return their stable project identity;
4. route every project-scoped operation by an explicit project target;
5. keep registry locks short and move indexing/checkpoint work to per-project
   synchronization;
6. fix reconnect, daemon-singleton, descriptor, and stale-session recovery
   seams;
7. close stale backlog items instead of implementing superseded proposals.

## 2. Constraints and non-goals

- Local-first `LiveIndex` remains the authoritative query engine.
- No external query database, tenancy platform, dashboard, or second index.
- No staged-edit overlay work. Current edits write through to disk and the
  shared index, so an overlay is not a source of private fresh state.
- No new dependency. Reuse current maps, `Arc`, locks, `SharedIndex`, snapshot
  persistence, and result-status machinery.
- No JSON-ratio/pure-data classifier without a new reproducible leak. Existing
  generated-output demotion and 018 source-focused defaults are the policy.
- No CCR expansion to adaptive/windowed single-target reads. Their current
  non-CCR profiles are intentional Feature 011 behavior.
- No per-harness daemon. It duplicates indexes/watchers and recreates daemon
  sprawl.
- A synchronization primitive may remain per project. "No lock" means no
  cross-project/global lockout, not unsynchronized mutation.

### 2.1 Product acceptance bar

The result must be good enough that an LLM can choose SymForge as its default
repository interface, not merely tolerate it as another search tool:

- **Correct identity:** every project-scoped response carries enough project
  ID/root, index generation, and freshness evidence to rule out a wrong-root or
  stale-index answer.
- **Direct availability:** the documented default full surface works through a
  normal MCP client connection. Dogfood must not require a private helper to
  reach the tools the product claims to ship; compact mode remains an explicit
  opt-in.
- **Actionable trust:** parse quarantine, partial coverage, truncation, stale
  snapshots, ambiguous routing, and unsupported modes are visible and include
  a deterministic next action. Silence or implicit fallback is failure.
- **Measured token economy:** representative orientation, symbol-context,
  reference, and change-analysis tasks record SymForge response tokens against
  the exact-file/raw-search context otherwise required. Savings are reported
  with retained-answer checks; a compression footer alone is not evidence.
- **Common-tool substitution:** the scorecard covers file discovery, targeted
  reading, text/symbol search, caller/dependent tracing, change inspection, and
  structural editing. A workflow counts as replaced only when an LLM can reach
  the correct actionable result without an auxiliary raw repository tool.
- **Useful ranking:** bounded results must preserve diversity and task-relevant
  symbols instead of spending the budget on repeated generic names.
- **Dogfood independence:** two independent connections and multiple logical
  callers sharing one connection can work across at least two real projects
  without changing one another's defaults.

The final review publishes this scorecard, including failures and cases where
SymForge costs more than the baseline. No favorable-only sampling.

## 3. Alternatives considered

### 3.1 Caller contexts beneath a daemon session

Key project state by `(daemon_session_id, caller_context_id)` and require the
harness to propagate a stable caller identity.

**Gain:** smallest daemon change and true caller-local defaults.

**Cost:** current MCP requests and target harnesses do not expose a portable,
trusted agent identity. Correctness would depend on every harness changing.

**Verdict:** reject until an interoperable caller identity exists.

### 3.2 Immutable home plus explicit request routing — selected

One adapter connection has one immutable home project. Additional projects are
opened into its working set. Every project-scoped request may select one by ID
or unambiguous alias; omission always means the immutable home.

**Gain:** correct even when many agents share one MCP connection; uses the
registry already present; removes mutable ambient root as the failure source.

**Cost:** wider schema/handler migration and a deliberate daemon-mode semantic
change for `index_folder`.

### 3.3 One daemon per harness

**Gain:** process-level isolation with little router work.

**Cost:** duplicate indexes, watchers, memory, runtime descriptors, and the
same multi-process sprawl already observed.

**Verdict:** reject.

## 4. Project identity and lifecycle

### 4.1 Terminology

- **Home project:** the project bound during adapter startup/client-root
  initialization. It is immutable after the first ordinary tool call.
- **Open project:** any project present in the session `WorkingSet`, including
  home.
- **Project ID:** the existing deterministic canonical-root hash.
- **Project alias:** a human-readable basename accepted only when it resolves
  to exactly one open project. Ambiguity returns candidate IDs and roots.

`path` remains a within-project path filter. It never means project selection.

### 4.2 `index_folder`

Local/embedded mode retains the current single-index reset behavior.

Daemon-backed mode changes deliberately:

- `index_folder(path)` opens or refreshes that project in the session working
  set and returns `project_id`, alias, canonical root, file count, symbol count,
  and checkpoint outcome.
- It never changes the immutable home project.
- Existing `add=true` remains accepted as a compatibility spelling for the same
  safe open behavior and is documented as redundant.
- A successful full index writes the atomic local snapshot immediately. The
  index must not revert to an older snapshot on the next connection. Explicit
  `checkpoint_now(verify_after_write=true)` remains the operator-grade verified
  checkpoint.
- Replays preserve the same project identity and checkpoint receipt.

No public remove operation is added. Session close and stale-session reaping
already provide the required ownership cleanup; add removal only when a real
long-lived-session pressure case exists.

### 4.3 Project inventory

`status(detail="projects")` exposes the current session inventory without a
new management tool:

- project ID and alias;
- canonical root;
- `home` marker;
- index/watcher state and generation;
- opened/last-used timestamps;
- snapshot/checkpoint state.

Default `status` output remains byte-compatible except for existing truthful
health corrections.

## 5. Request routing

### 5.1 Shared resolver

Add one shared resolver used by every project-scoped handler:

```text
explicit project ID/alias -> selected open project
omitted project          -> immutable home project
unknown/ambiguous target -> deterministic error with candidates
```

The resolver performs lookup only. It never indexes, changes home, or bumps
frecency.

### 5.2 Surface coverage

Add optional `project` targeting to single-project reads and guidance:

- `get_symbol`, `get_symbol_context`, `get_file_context`, `get_file_content`;
- `get_repo_map`, `search_files`, `find_dependents`, `diff_symbols`;
- `what_changed`, `analyze_file_impact`, `validate_file_syntax`;
- `explore`, `ask`, `conventions`, `edit_plan`, inventory/investigation tools;
- compact `symforge` routing.

Keep `project`/`projects` on set-valued discovery operations and extend parity
where supported:

- `search_symbols`, `search_text`, `search_files`, `find_references`.

Structural edits accept one optional `project`. Existing
`working_directory` routing remains authoritative for worktrees and must lie
inside the selected project; disagreement fails before writing. Batch edits may
target multiple explicit projects only when every edit names its project.

Unqualified calls remain on home. No operation silently changes another
caller's default.

## 6. Concurrency and lock discipline

Change the registry value from an inline mutable `ProjectInstance` to an
`Arc<ProjectSlot>` (or the smallest equivalent existing-type refactor).

The daemon-wide project-map lock may only:

- look up a slot;
- insert a fully prepared slot after a race re-check;
- remove an unowned slot.

It must never cover filesystem discovery, parsing, snapshot IO, watcher
restart, or `LiveIndex` reload.

Each project slot owns the synchronization needed to serialize reload and
checkpoint for that project. Reads against other project slots continue while
one project indexes. Preserve a documented lock order for registry/session
metadata, and never hold a registry/session lock while waiting on a project
operation.

Replace the process-global snapshot write bottleneck with same-project/path
serialization. Distinct `.symforge/index.bin` paths must checkpoint
independently; same-path writes remain serialized and atomic.

## 7. Reconnect, descriptors, and daemon uniqueness

### 7.1 Reconnect

The adapter stores immutable home plus the roots/IDs it opened. After daemon
loss it:

1. reopens home;
2. reopens the remaining working-set projects;
3. verifies returned deterministic IDs;
4. resumes explicit routing.

Reconnect never falls back to a stale mutable `project_root` field.

### 7.2 Runtime descriptors

Replace last-writer-wins fixed session ownership with per-session descriptors.
Each adapter removes only its own descriptor. Hook lookup selects a live
descriptor matching the caller root and reports ambiguity rather than silently
choosing another session. Retain a compatibility pointer only if existing
clients require it.

### 7.3 Daemon singleton and stale sessions

- Direct `symforge daemon` startup uses the same start lock and running-daemon
  check as auto-spawn. If a live daemon exists, report and reuse/refuse instead
  of overwriting the runtime record.
- Different `SYMFORGE_HOME` values remain intentionally isolated.
- A bounded reaper closes sessions whose heartbeat expired, exercising the
  existing close/GC path. TTL and last-seen evidence are visible in status.

## 8. Feature 018 closure

Fresh gates are green on the branch: fmt, check, clippy `-D warnings`, all
targets, release build, and embed check. Live branch-binary dogfood also proved
source-focused `what_changed` and the CCR footer.

Before merge:

1. correct the stale `search_symbols` description that still claims browse is
   sorted by path/line;
2. add a failing browse regression showing one generic repeated name (`new`)
   cannot occupy the whole orientation page, then implement the minimum
   deterministic dedup/ranking adjustment;
3. strengthen the frecency-neutrality assertion to observe the frecency store,
   not only `reverse_index`;
4. mark T001-T029 truthfully with fresh gate/live evidence;
5. prepend a historical/remediation notice to `docs/grok_report.md`.

Push, PR creation, merge, publication, and `cargo clean` remain explicit
operator gates. The earlier 8.13.9 release is already verified published and
requires no action.

## 9. Deferred-work disposition

### 9.1 Admission demotion

Close the speculative JSON-dominated-directory item. Generated-output
demotion already ships and the current repository has no dominating generated
corpus.

Implement only the real parity seam: watcher admission for a newly created file
under an already-demoted untracked generated-output directory must retain
`GeneratedOutput` Tier-2 classification. Reuse the existing policy; do not add
a second classifier.

### 9.2 CCR residual

Close without code. Feature 011 intentionally excludes adaptive/windowed
single-target reads; full repo map and ranked discovery payloads already use
CCR. Compact/tree maps remain bounded summaries.

### 9.3 Grok integration and dogfood artifacts

- Add `InitClient::Grok` and include it in `All`.
- Verify the installed Grok TOML schema before implementation.
- Preserve unrelated config and user-set values; emit native SymForge command,
  `RUST_LOG=off`, and `SYMFORGE_WORKSPACE_ROOT` only where the schema supports
  them.
- Skip global quiet-stderr behavior unless live Grok needs it.
- Create `docs/grok-dogfood-prompt.md` as the canonical v1 prompt because no
  prior source artifact exists. Base it on the historical report and require
  start/end version, root, index ID/counts/tiers, clean-checkout delta, every
  created/indexed artifact, pre-existence, cleanup, and ending deltas.

### 9.4 Environment and housekeeping

- Terminal Commander is healthy and becomes the mandatory runner for noisy or
  long Cargo commands.
- PATH verification currently resolves 14/15 required tools through Terminal
  Commander; `deno` is missing. Treat this as environment configuration, not
  SymForge code.
- Terminal Commander, disk-floor, and plugin reload notes leave the SymForge
  product ledger after their operational verification.
- `tasks/lessons.md` remains the correction ledger.

## 10. Error handling and trust

- Unknown project: `not_found`, names target, lists open candidates.
- Ambiguous alias: `ambiguous`, lists candidate IDs and roots.
- Project not open: explicit corrective `index_folder(path)` guidance.
- Cross-project mode unsupported by a tool: `invalid_request`, never silent
  fallback to home.
- Index/checkpoint failure: prior valid index/snapshot remains served; partial
  state is never published.
- Reconnect replay mismatch: fail closed and expose status; do not answer from a
  different root.
- Slow/busy project: bounded project-local status, never global daemon stall.

Every ordinary response must retain bound/selected project evidence in the
trust envelope where practical.

## 11. Testing strategy

All behavior changes follow red-green-refactor. Minimum regression witnesses:

### 11.1 Isolation and routing

- Two daemon sessions: opening/targeting B in one never changes the other's A.
- Two logical callers sharing one connection: opening/targeting B leaves
  unqualified reads on immutable home A.
- Target parity for every added read/context/map/search/edit schema.
- Unknown and ambiguous project targets return machine-readable outcomes.
- `index_folder` returns identity and a snapshot that restores on a new process.

### 11.2 Concurrency and recovery

- Artificially slow project-A index while project-B reads complete without
  waiting on the registry lock.
- Same-project concurrent reloads serialize; different projects proceed.
- Kill daemon after opening B; reconnect restores home and open-project routing.
- Two adapters on one root keep independent descriptors; one shutdown does not
  delete the other.
- Expired heartbeat reaps the session and orphan project/watcher once.
- Direct concurrent daemon starts yield one live daemon/runtime record.

### 11.3 Residuals

- Browse output cannot be monopolized by repeated generic names and remains
  deterministic/frecency-neutral.
- Watcher-created file under demoted generated output stays Tier 2.
- Grok registration creates, preserves, and is idempotent; `All` includes it.
- Dogfood prompt contains every baseline/artifact-disclosure field.

### 11.4 Verification execution

Terminal Commander owns all long/noisy commands using the active Cargo rule
pack. Collect structured failure signals plus final exit receipts only:

```text
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed
```

Final dogfood uses at least two projects under `E:\project`, two independent MCP
connections, and one shared-connection caller scenario. Verify code truth with
SymForge, publish the product-acceptance scorecard from section 2.1, then run
an adversarial review before integration.

## 12. Rollout and integration gates

1. Close 018 residuals on its current branch and re-run its focused/live gates.
2. Merge 018 only after explicit user approval.
3. Implement 019 in dependency order: identity/surface, non-blocking registry,
   reconnect/descriptors/singleton/reaper, then parity rollout.
4. Land watcher parity, Grok init, and documentation closures as independent
   commits.
5. Run the full gate and multi-project dogfood.
6. Present PR/release evidence. Do not push, merge, publish, or clean build
   artifacts without the corresponding operator approval.

## 13. Terminal Commander feedback for its next build

- `registry.import_pack(activate=true)` requires `scope`, but `scope` is exposed
  as `unknown`; document `{"kind":"global"}`.
- `run_and_watch` advertises `timeout_ms` but accepts `wait_ms`, capped at 60s.
- Timed-out `run_and_watch` returns `recover_hint:null`; name the exact next
  calls.
- `wait` requires `bucket_id + cursor + timeout_ms`, not the returned `job_id`.
- Shared schemas expose fields rejected by individual actions (for example
  `summary(compact=...)`). Publish the action-to-field matrix.
- The argv denial recommends `shell_exec`, while the exposed action is `exec`;
  use one public name in errors and schema.
- Direct argv execution rejects a PowerShell script file as
  `ShellInterpreterDenied`; with the shell lane disabled, there is no permitted
  Terminal Commander route for a legitimate signed/local script. Either allow
  `powershell -File <script>` as a distinct non-command-string lane or document
  and expose a policy-safe script action.
- Shell execution is disabled in the active DeveloperLocal profile. This is a
  valid policy, but it means aggregate PATH probes require one direct argv call
  per program; document that expected workflow.
