# SymForge: MCP exposure, embedded parity, and AAP handoff

As of 2026-10-09. Uncommitted report requested for the SymForge repair/integration agent.
AAP worktree: C:\AI_STUFF\PROGRAMMING\aap-kernel-24.
Branch: feat/tool-kernel-remainder. Verified hardening HEAD/remote: 6a138ee3cb89de85ffff0c42c101af8b555a841e.
Related evidence: CODEX-REPORT-24.md. Terminal Commander has its own separate report.

## Conclusion and ownership

AAP already has a substantial native SymForge bridge. It embeds locked SymForge 10.0.0; the installed executable reports 11.5.4, while the official latest release found during this investigation is 11.5.6. Embedded parity is not achieved.

V11 removes public raw-index/replay modules AAP currently consumes. Its supported embed facade covers typed indexing lifecycle, symbol/text/knowledge search, census/progress, and refresh; it lacks public equivalents for several file/context/reference/impact/edit operations advertised through MCP. A direct version bump would remove required imports, so the migration needs coordinated upstream public contracts and AAP adapter changes. This conclusion follows API/source inspection; no V11 dependency bump/compile experiment was performed on this branch.

SymForge agent ownership: publish and test supported embedded equivalents for the required MCP semantics, explicit capability identity, and safe durable replay ownership/reconciliation (or document replay as a separately owned component). AAP ownership: migrate aap-code-intel and aap-graph away from private implementation access, adapt aap-mcp replay, preserve current behavior and tests, align CI, and demonstrate live parity.

The session exposes no SymForge MCP tools. That is a diagnosed tool-surface/configuration gap, not a measured SymForge server failure. No optional MCP server was enabled, no daemon upgraded/restarted, and no other repository/ref/configuration changed. Backend POST /mcp/tools/invoke and aap_supervised_shell remain deferred for the explicitly requested security review.

## Exact identities and MCP exposure

| Component | Observed identity | Evidence / practical limit |
|---|---|---|
| AAP locked embed | symforge 10.0.0; git 5348c3e4f283227603c4095b3ffef840993facca | Cargo.lock; Cargo.toml:145 uses git main, default-features=false, features=[embed] |
| Installed SymForge executable | 11.5.4; reports 11.5.6 available | Read-only symforge --version; this is executable identity, not a server health result |
| Official current release inspected | v11.5.6 | [Official releases](https://github.com/special-place-ai-heaven/symforge/releases), [tagged embed source](https://github.com/special-place-ai-heaven/symforge/blob/v11.5.6/src/embed.rs) |
| Codex session tool inventory | Zero SymForge tools | Actual tools available to this root session |
| Codex MCP configuration | Only terminal_commander entry observed | Safe selected-field inspection of ~/.codex/config.toml; no raw config or credential values reproduced |
| Claude global configuration | symforge.exe entry, disabled=false | Separate harness configuration; does not establish availability in this Codex session |

The V11 migration is documented in the [official lifecycle migration guide](https://github.com/special-place-ai-heaven/symforge/blob/main/docs/migrations/v11-index-lifecycle.md). Main documentation can move; the v11.5.6 source links below identify the inspected release API.

Needed MCP diagnostic follow-up when the owner exposes SymForge in the target harness: record executable/build/version and project root, then call health/status, identify indexed generation/admission/degradation, run bounded symbol/text/file-context operations on a fixture, and capture exact typed errors plus correlation IDs. Do not substitute an enabled Claude entry or --version output for those live tests. Do not print environment values or secrets. This session cannot claim MCP query/edit/reconnect health without that live tool surface.

The current release includes a Windows named-pipe read-gate fix after installed 11.5.4. That is relevant upstream release information, not proof that this session's absent tools were caused by that defect. Diagnose exposure separately from named-pipe connectivity and embed behavior.

## Existing AAP integration that must survive migration

AAP is not missing the entire bridge:

- crates/aap-code-intel/src/port.rs:19 defines 17 CodeIntelPort operations: coding/review context; file outline; symbol body/trace; references/dependents; symbol/text search; impact; syntax validation; symbol diff; project indexing; file refresh; indexed status; stop; engine identity.
- crates/aap-code-intel/src/adapter.rs:28-39 imports the V10 embed facade and raw types. The implementation around 1123-2058 consumes lifecycle and index data for those operations. Around 2059 onward it also implements aap_core CodeIntelBridge for file search/read, symbol resolution, and status.
- crates/aap-code-intel/src/types.rs owns AAP DTOs, text search options, bounded contexts, index provenance/degradation, and engine identity.
- crates/aap-code-intel/src/read_cache.rs caches reads; preserve invalidation/freshness semantics.
- crates/aap-code-intel/src/adapter/tests.rs exercises the current native behavior, including file reads/search/symbol/status, impact/syntax/diff/context.
- crates/aap-graph/src/lib.rs:28,485,640,730 directly consumes the V10 index for graph projection; its tests include projection/restart snapshot cases around 1522/1652.
- crates/aap-backend/src/main.rs:1112 creates the native bridge; existing read-only project code-intel routes live in routes/projects/code_intel.rs:201 onward. Guest-agent and agent paths also consume the bridge.
- crates/aap-mcp/src/kernel.rs:11-13 imports the V10 replay/domain API added by this port.

V11 makes LiveIndex, SharedIndex, domain, and idempotency implementation modules private. Port the affected aap-code-intel, aap-graph, and aap-mcp callsites together. Do not solve this by copying V10 internals into AAP or importing V11 private modules; that would leave future embed parity fragile.

## MCP-to-embed capability inventory

The [official tool surface](https://github.com/special-place-ai-heaven/symforge#the-tool-surface) table lists 39 names; tagged source also registers secret_remediate, giving the advertised 40. The compact symforge facade is separately registered/profile-filtered. Counting README rows alone misses that capability.

| Capability group / MCP names | Existing AAP behavior | Inspected V11 public embed coverage / required work |
|---|---|---|
| Orient: health, health_compact, status, get_repo_map, explore, ask, conventions, context_inventory, investigation_suggest | Engine identity/status plus AAP coding/context builders and graph | Typed runtime/census/progress exists; richer orientation/routing/context queries need supported reusable contracts or explicit AAP composition |
| Read: get_file_context, get_file_content, get_symbol, get_symbol_context, inspect_match | Outline/body/read/symbol/trace already provided through V10 bridge | Public body/outline/context/reference query contracts missing from inspected facade |
| Search: search_symbols, search_text, search_files, symforge_retrieve | Symbol/text/file search, enclosing symbol context and options | Typed symbol/text requests exist, but filters/semantics are incomplete relative to AAP; file/retrieve equivalents need contracts |
| Knowledge: search_knowledge, review_knowledge, curate_knowledge | Current CodeIntelPort has no direct knowledge operations | Typed knowledge search exists; review/curation semantics and write authority require explicit embed API and AAP feature decision |
| Trace/impact: find_references, find_dependents, what_changed, diff_symbols, analyze_file_impact, detect_impact, validate_file_syntax | References/dependents, impact/syntax/diff/review context already implemented | Public equivalents missing from inspected facade; cannot replace them with empty results |
| Edit: edit_plan, replace_symbol_body, edit_within_symbol, insert_symbol, delete_symbol, batch_edit, batch_insert, batch_rename, symforge_edit | CodeIntelPort is read-oriented; agents have separate filesystem/edit authority | Supported structural edits with dry run, admission, expected-generation/content conflict checks and refresh receipts needed before AAP can claim edit parity |
| Index: index_folder, checkpoint_now | Index_project/reindex_file/stop_project and watcher/snapshot lifecycle | ProcessIndexRuntime/EmbeddedSourceHandle and refresh tickets exist; define checkpoint/restore/provenance equivalents |
| Sensitive remediation: secret_remediate | No new AAP route or capability added here | Preview defaults true in tagged protocol source; apply is a gated mutation with operation identity. Require reusable contract and explicit authority before exposing it |

Primary API evidence: [embed.rs](https://github.com/special-place-ai-heaven/symforge/blob/v11.5.6/src/embed.rs), [embedded source implementation](https://github.com/special-place-ai-heaven/symforge/blob/v11.5.6/src/index_lifecycle/embedded.rs) around 1147-1555, and [secret remediation protocol](https://github.com/special-place-ai-heaven/symforge/blob/v11.5.6/src/protocol/secret_remediate.rs).

### SF-EMBED-01: public query/context parity

Required upstream contract: one supported embed interface with bounded typed methods for outline/imports, admitted file/symbol source, callers/callees/references/dependents, map/context/provenance, impact, syntax, and symbol diff. It should delegate to the same internal implementation as the MCP operation where practical. Give each operation explicit admission, degradation, snapshot generation, truncation, and typed refusal semantics.

Do not return a successful empty Vec when a feature is unsupported, indexing is degraded, or a deadline expired. AAP's agents use this evidence to decide where code may be changed and which callers need review.

V11 SymbolSearchRequest currently offers query/path_prefix/limit but no kind filter. TextSearchRequest offers query/path_prefix/limit/case_sensitive, but lacks AAP regex/include_tests filters and enclosing-symbol result/context. Enclosing-symbol context is an AAP TextMatch result field, not a TextSearchOpts input option. Preserve existing TextSearchOpts and search kind semantics with tested upstream equivalents or report explicit unsupported options. Do not silently reinterpret regex as literal text.

Acceptance fixture: small Rust and TypeScript project with duplicate symbol names/kinds, imports/re-exports, dynamic string references, tests, parse errors, binary/nonadmitted/secret-bearing paths, a document, and a deleted/renamed file. Compare equivalent MCP and embedded results after normalizing transport wrappers; verify provenance, bounds, filters, enclosing contexts, degradation, and refusal. No tests should require exposing a secret value.

### SF-EMBED-02: supported graph projection

AAP aap-graph currently reads V10 LiveIndex internals. Add a bounded typed public graph/index projection contract, or expose the references/imports/symbol relations AAP needs to construct it without lock or internal-layout knowledge. Preserve symbol identity, edge direction, multi-language classification, file provenance, generation and restart behavior.

Acceptance: current graph projection and restart snapshot tests remain green; migration tests cover duplicate names, changed/deleted files, partial parse state, concurrent refresh, and deterministic snapshot generation. Do not assume file census alone replaces references/import edges.

### SF-EMBED-03: lifecycle, snapshot trust, refresh, and stop

Use ProcessIndexRuntime/EmbeddedSourceHandle rather than raw SharedIndex locks. Give AAP a stable host policy for root admission, worktree/project isolation, concurrency/resource bounds, watchers, cancellation, refresh completion, checkpoint and stop. Preserve source/status degradation instead of inferring success from an index handle.

The migration guide describes prior-format V10 snapshots being refused as current authority, with a copy quarantined under .symforge/v11 and original bytes preserved. V11 cold reindexes/reobserves instead of restoring those V10 snapshots. It also notes richer per-entry SnapshotStore proof is still unwired (guide section 4). That documentation does not establish full trust/provenance parity for restored records.

Acceptance: open a project with a V10 snapshot; verify it is refused as current, copied/quarantined, original bytes preserved, and current publication comes from re-observation. Incompatible/corrupt/secret-policy-changed snapshots refuse or reindex explicitly; refresh tickets have bounded wait and actual generation proof; cancelled/failed refresh must not be reported complete; stop/restart preserves documented semantics; two projects/worktrees cannot consume one another's state. A V11 refresh ticket is an in-process queued-work identity, not a durable tool replay operation key.

### SF-EMBED-04: structural edit and knowledge parity

These are MCP capabilities beyond the current read-oriented AAP port. Define which are needed by AAP agents before extending authority. Publish stable edit-plan/dry-run/batch APIs that share MCP admission/conflict/write/refresh behavior, then expose them through an AAP-owned port with expected content/generation and explicit cancellation/partial-result semantics. Multi-site rename must also account for strings/re-exports/tests; index matches alone are not a global AST proof.

Acceptance: dry-run leaves bytes unchanged; apply refuses stale content/generation or policy violation; concurrent edit race yields conflict rather than overwritten work; partial batch failure is explicit and recoverable; refresh reflects final bytes; cancellation after write remains an uncertain mutation where appropriate. Knowledge review/curation and secret remediation need separate preview/apply authority and redacted results. Do not expose these merely because a tool name exists.

## Replay defects/limits and exact repair handoff

Source: [pinned V10 idempotency.rs](https://github.com/special-place-ai-heaven/symforge/blob/5348c3e4f283227603c4095b3ffef840993facca/src/idempotency.rs).

### SF-REPLAY-01: reservation publication has an incomplete-record window

check_or_reserve around line 227 atomically create_dir(key_dir), then separately publishes its record through write_record_atomic around 352. A second independently opened store in between gets AlreadyExists and load_existing around 313, which yields IncompleteReservation. A crash/write failure in the gap can leave a persistent key directory without a record.

This is fail-closed rather than a duplicate-write permission, but can permanently block an operation which never reached tool execution.

Deterministic minimal reproduction, proposed for upstream:
open two FileReplayStore instances on the same ProjectStateDir; construct a key; create the parent directory of store.record_path(&key) without a record; call check_or_reserve(key, request_hash); observe IncompleteReservation. For the live interleave test, install a barrier between winner mkdir and record publication, have the second process reserve, then release the first. Include process crash and fs write failure at the same barrier. A manually prepared directory demonstrates the missing-record state; the interleave/crash tests prove the transition behavior.

### SF-REPLAY-02: caller cancellation can detach reservation I/O

AAP reserve and persist run via spawn_blocking (kernel.rs around 126-205). Deadline/cancellation can stop waiting while the disk task continues. A reservation can therefore become durable after a timed-out/cancelled pre-route call. That detached task never calls the server, but the explicit operation key can then remain blocked.

After routing begins, timeout/cancellation/abort must keep the reservation uncertain because a remote mutation may still finish. Automatically deleting or expiring such reservations would permit duplicate side effects.

Required upstream contract: atomic visible reservation ownership/fencing, a typed durable state transition for execution-started/completed/uncertain, and explicit inspection/reconciliation semantics. Release a never-started operation only with proof and owner-token/CAS fencing; never blindly retire a Reserved write based on age. Specify crash/disk-full/partial-write/fsync behavior and cross-process concurrency. A small AAP-owned transactional replay component is an alternative if SymForge intentionally removes replay from its public product API; that ownership choice must be explicit, not hidden by a dependency bump.

V11 makes symforge::idempotency/domain private and the inspected embed facade does not replace this public replay API. Resolve it before upgrading aap-mcp.

### SF-REPLAY-03: operation identity privacy and retention

ReplayRecord around line 79 stores key_hash/request_hash/status/timestamps/optional response rather than raw key. AAP scopes the key using project/server/tool/explicit caller key (kernel.rs around 307-335) and binds arguments by request hash. This protects scope and conflicts; deterministic unsalted hashes do not provide cryptographic secrecy for predictable keys.

Raw caller keys still exist in memory, and IdempotencyKey(String) derives Debug. AAP response_text stores the serialized full ToolOutcome in plaintext (kernel.rs around 350-365). Do not claim a hash-based file path makes replay output confidential.

Needed integration policy: high-entropy caller operation IDs, hash-only/redacted diagnostics, admitted project state directory and filesystem permissions, bounded outcome size/retention, secret-aware persistence policy, and reconciliation audit. Treat state as a source of possibly sensitive tool results. No new global/raw-secret logging, no broad dump of response_text.

### AAP replay behavior already hardened and verified

An explicit ToolExecuteContext.idempotency_key represents one logical operation. Correlation IDs are tracing only. No explicit key means independent executions, even with a configured replay store. Keys require a store and are scoped to project/server/tool; changing arguments conflicts. Known mutators never acquire retry permission from client JSON or a read-looking name. Trusted in-process descriptors are required to grant retry. Cancellation and one total monotonic invocation budget cover classification/reserve/retry/backoff/persistence.

Successful and definite error outcomes are cached. Corrupt/missing/mismatched outcomes fail closed. Abort/cancel/timeout after reservation remain uncertain. A failed outcome write reports that execution may already have occurred; it does not fabricate success. A new key is a new operation and can duplicate an already-applied uncertain effect.

Relevant existing regressions in crates/aap-mcp/tests/fault_injection_kernel.rs:
independent reads/writes around 200/213; same explicit key 226; argument conflict 245; persistence-failure release-file handshake 282; corrupt record 329; aborted write across reopened stores 363; total retry budget 404; scope 422. kernel.rs around 604 covers completed replay records without outcome. These locations identify source/test areas; line numbers can move.

Add missing acceptance cases: truly separate processes racing reserve with publication barrier, pre-route reserve timeout/cancellation with delayed blocking I/O, crashed publisher without record, owner-token fencing/reconciliation, disk-full at each transition, external mutation completing after client cancellation, plaintext-outcome redaction/retention, and v10 replay-state compatibility during the chosen migration. Do not call mock or source inspection a live cross-process result.

## Existing MCP room failures are AAP work

The final aap-mcp suite has 102 passed / 7 failed, exit 101. The seven failures are existing T098 room-client stubs reproduced on untouched main and matched against origin/docs/aap2-design:docs/aap2/LINUX-VERIFY-cloud-c70d1abe.md. They do not diagnose the external SymForge MCP server. The 19 new live stdio fault/replay tests pass, including ten parallel stress repetitions.

Exact failing AAP test names (all under client::tests):

```text
crash_restarts_a_new_instance_then_reprobes_with_real_tool_discovery
every_room_operation_rejects_each_independently_mutated_response_fence
discovery_probe_and_reprobe_reject_unexpected_toolset_digest
invalid_request_fences_are_rejected_before_any_room_port_io
manifest_mismatch_and_disabled_component_are_denied_for_every_operation
unavailable_room_port_has_no_host_fallback_for_every_operation
room_discover_invoke_and_probe_use_exact_fences_and_list_tools_evidence
```

Repair these in AAP's room MCP client/authority integration (T098). Do not attach a host MCP fallback to bypass room fences. Evidence is in `/home/robert/aap-kernel-logs/hardening-final-mcp-verified.log`, with exit 101 in the adjacent `.exit` file.

The broad MCP crate name encompasses AAP's internal transport/kernel. Fault injection uses a local Python JSON-RPC fixture with deterministic release handshakes. It is not an external SymForge integration test. Terminal Commander's report separately records its live MCP checks and missing receipt.

## Version, CI, license and integration decisions

- Cargo.toml follows SymForge git main, but Cargo.lock currently pins 5348c3e. A routine cargo update -p symforge can introduce the V11 API break. Use an immutable tested revision for the migration/release decision; keep lockfile and reported engine identity aligned.
- AAP .github/workflows/ci.yml still checks out SymForge v7.21.1 five times and carries stale path-dependency/lock comments. Align the actual CI source/input with the selected supported embed release and test the real git dependency rather than a stale checkout. Inspect whether each checkout is used before deleting or replacing it.
- [Tagged rust-toolchain.toml](https://github.com/special-place-ai-heaven/symforge/blob/v11.5.6/rust-toolchain.toml) pins Rust 1.96.0; AAP declares rust-version=1.93. A toolchain pin is not proof of actual minimum compiler support. Compile/check the chosen release with the intended WSL/CI compiler, document transitive MSRV, and update CI deliberately if needed.
- SymForge uses [PolyForm Noncommercial 1.0.0](https://github.com/special-place-ai-heaven/symforge/blob/main/LICENSE). AAP's MIT manifest does not relicense it or grant commercial rights. deny.toml already contains a scoped SymForge exception. Check chosen release metadata and complete dependency licenses; keep TC exceptions separate.
- Preserve admission/secret policy, bounded source output, project-state isolation, no raw-secret Debug/logging, and plaintext replay result access/retention controls. Metadata/capability negotiation must not silently grant write authority.
- Integrating edit/remediation/replay capabilities must preserve AAP's existing room authority and the pre-mount security gate. Do not mount backend invoke or supervised shell to make a parity demo pass.

## Validation evidence and commands

Final hardened port evidence in the main report:
cargo test -p aap-mcp --no-fail-fast: exit 101, 102 passed / 7 known T098 failures.
cargo test -p aap-agents --lib: exit 0, 1,788 passed.
cargo test -p aap-agents --lib retrying_ai_call: exit 0, 23 passed.
cargo test -p aap-agents --test retry_budget: exit 0, 26 passed.
cargo test -p aap-core: exit 0, 989 passed.
cargo clippy -p aap-agents -p aap-mcp -p aap-terminal -p aap-core --lib: exit 0, existing warnings.

All ran under Ubuntu-24.04 with CARGO_TARGET_DIR=/home/robert/aap-kernel-target and RUSTC_WRAPPER=; actual logs/exit files are under /home/robert/aap-kernel-logs. Example direct reproduction:

```text
wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/AI_STUFF/PROGRAMMING/aap-kernel-24 && CARGO_TARGET_DIR=~/aap-kernel-target RUSTC_WRAPPER= CARGO_BUILD_JOBS=2 cargo test -p aap-mcp --no-fail-fast"
```

Additional current V10 bridge checks completed in this investigation:

| Exact Cargo argv | Log label / TC job | Result |
|---|---|---|
| cargo test -p aap-code-intel --no-fail-fast | parity-code-intel-v10 / job_01a12209789073dca28c97798de13c7e | exit 0; 35 unit tests passed, 0 failed; 0 doctests |
| cargo test -p aap-graph --no-fail-fast | parity-graph-v10 / job_01a1220c2f34720f8dbf3709361c6073 | exit 0; 9 unit tests passed, 0 failed; 0 doctests |

Actual command form was `wsl -d Ubuntu-24.04 -- python3 /mnt/c/Users/poslj/AppData/Local/Temp/aap-pr24-run.py <label> <Cargo argv>`, with the same cwd, Linux target, empty wrapper, and CARGO_BUILD_JOBS=2. Logs and exit files are `/home/robert/aap-kernel-logs/parity-code-intel-v10.log` / `.exit` and `parity-graph-v10.log` / `.exit`. The TC observed receipts completed in 62,093 ms and 48,354 ms respectively. The suites ran sequentially. They validate the existing V10 bridge, not V11 or external MCP parity.

Migration acceptance commands (run after the chosen upstream/API migration; not claimed executed against V11 here):

```text
wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/AI_STUFF/PROGRAMMING/aap-kernel-24 && CARGO_TARGET_DIR=~/aap-kernel-target RUSTC_WRAPPER= CARGO_BUILD_JOBS=2 cargo test -p aap-code-intel --no-fail-fast"
wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/AI_STUFF/PROGRAMMING/aap-kernel-24 && CARGO_TARGET_DIR=~/aap-kernel-target RUSTC_WRAPPER= CARGO_BUILD_JOBS=2 cargo test -p aap-graph --no-fail-fast"
wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/AI_STUFF/PROGRAMMING/aap-kernel-24 && CARGO_TARGET_DIR=~/aap-kernel-target RUSTC_WRAPPER= CARGO_BUILD_JOBS=2 cargo test -p aap-mcp --no-fail-fast"
```

Run affected agent/core/backend compile checks and license checks as the final migration scope requires. External MCP acceptance requires live tools in the target harness; record the same fixture's embedded/MCP capabilities and results. Do not change current pins until a supported API migration is ready to test.

## Required delivery from the SymForge agent

1. An immutable supported embed release/API version, feature map for all 40 MCP capabilities, and explicit unsupported operations. Include engine/build/snapshot/secret-policy identity.
2. Public typed contracts sufficient to preserve AAP's current 17 code-intel methods and graph projection, plus agreed knowledge/edit/remediation additions.
3. Search filter/context parity and source admission/refusal/provenance semantics shared with MCP.
4. Documented lifecycle/refresh/checkpoint/restore migration and original-state preservation, including limits of restore trust.
5. An explicit replay ownership decision, safe reservation/reconciliation contracts if provided, fault tests, and migration/retention semantics.
6. Compile-tested examples and fixtures AAP can use for embed/MCP semantic comparisons, exact toolchain/license/dependency changes, and release identity.
7. Separately, a diagnostic fix for Codex exposure if the owner elects to enable this server; no evidence here establishes a broken running server.

Definition of done: AAP consumes supported public APIs, preserves existing native behavior, exposes required recent capabilities with matching semantics and authority, passes migration/replay/graph tests and live fixture comparisons, and reports unavailable/degraded/uncertain states honestly. Neither a version-number match nor a green mock establishes parity.
