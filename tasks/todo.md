# SymForge MCP Token-Economics Evaluation

## Release train — 2026-07-14

- [x] Inventory local changes, branches/worktrees, remote divergence, open PRs, checks, and the release workflow.
- [x] Validate the complete commit scope and scan it for secrets or oversized artifacts.
- [x] Run the repository's pre-integration verification gates.
- [x] Commit all safe work and fast-forward it onto updated `main`.
- [x] Merge every ready open PR with branch deletion and refresh `main` after each.
- [x] Verify the final `main`, push it, and confirm the Release workflow starts.
- [x] Delete merged local branches/worktrees and leave a clean checkout on `main`.

Review: Completed. Fast-forwarded the three reviewed campaign commits to `main`; merged dependency PRs #450-#453 and release PR #454 with branch deletion; and published v8.15.0. The fresh pre-merge matrix passed Python/npm, fmt/check/clippy, the full serial all-target suite, release tool oracles, and embed gates. On the combined dependency tree, `cargo check` and all-target Clippy passed and #453's authoritative Rust CI passed. A redundant local full-suite rerun reached MSVC `LNK1201` only after `target/` exhausted the disk; `cargo clean` then reclaimed 31.7 GiB.

## Windows delegated-worker process leak regression

- [x] Preserve the observed process tree and terminate only the three completed worker-owned trees.
- [x] Identify the exact lifecycle owner and installed-version boundary where worker completion stops reaping MCP children.
- [x] Compare the broken path with the last known working implementation or current upstream fix.
- [x] Identify the exact minimal red lifecycle check before changing anything.
- [x] Execute and preserve that red check in a separate Codex checkout before implementation.
- [x] Restore or expose `close_agent` at the collaboration-wrapper boundary; do not add a repo cleanup daemon or polling workaround.
- [x] Confirm V2's current resident bound is independently root plus three workers; correct the false claim that `[agents].max_threads`/`max_depth` govern V2.
- [x] Inspect the uncommitted SymForge daemon idle-shutdown defense in depth and verify its authenticated-heartbeat mechanism.
- [x] Obtain independent review of the daemon idle-shutdown scope, default service semantics, shutdown behavior, and test coverage via `research/token-cost/claude-handoff-daemon-idle-review-a019.md` (`CHANGES_REQUIRED`).
- [x] Restrict the 600-second default to detached auto-spawn, keep explicit `symforge daemon` persistent when unset, and document the operator contract.
- [x] Add a paused-time behavioral test proving authenticated activity defers shutdown and the next idle sweep notifies; preserve its red/green receipt.
- [x] Verify focused tests, fmt, all-target clippy, the full serial all-targets suite, and an isolated 60-second live-process smoke with runtime-file cleanup.
- [x] Obtain independent follow-up review of the corrected daemon diff (`APPROVE_COMMIT`) and commit only the approved product scope.
- [x] Verify a completed worker leaves zero worker-owned descendants while the active session and unrelated Claude/WSL/Docker processes survive.
- [x] Record the fix, verification, and cleanup in this section's review.
- [x] Record the read-only STAY-AND-FIX reconnaissance in `research/token-cost/codex-v2-close-agent-reconnaissance-2026-07-14.md`.
- [x] Obtain independent review via `research/token-cost/claude-handoff-codex-v2-close-agent-research-a019.md` (`APPROVE_IMPLEMENTATION_PLAN`).
- [x] Create the isolated pinned Codex branch and preserve the focused clean baseline.
- [x] Preserve the V2 `close_agent` tool-plan red test before product code.
- [x] Implement only the V2 bridge, registration, honest usage hint, and focused behavior/namespace coverage.
- [x] Obtain the next independent code-review verdict before broader gates or installation.
- [x] Run the broader `codex-core` package gate and classify every residual failure outside the patch scope.
- [x] Reproduce the live API rejection of default `collaboration.close_agent` and prove the same candidate through the supported non-reserved `agents` namespace.
- [x] Obtain independent review of the reserved-namespace diagnostic and deployment recommendation.
- [x] Verify the final Windows sentinel MCP tree exits after V2 close while the root and unrelated sentinel survive.
- [x] Obtain final smoke/install review, commit and deploy the pinned binary, then clean the disposable Cargo target/worktree.

Review note: daemon idle shutdown can bound an orphaned detached daemon only
after authenticated traffic stops. It cannot reap the duplicate live MCP proxy
stacks observed in Codex because those proxies keep their stdin open and send
authenticated heartbeats. Do not present it as the root-cause fix for that
host-owned lifecycle leak.

Evidence so far:

- Three collaboration workers reported completion/interruption but left three timestamp-clustered MCP/runtime bundles attached to the active Codex process.
- The bundles contained 15 direct SymForge/Node roots plus 3 descendants. Exact-tree termination removed all 18; the active root session's older five-helper bundle remained alive.
- The same workflow did not historically leak, so this is being treated as a lifecycle regression. Manual post-run cleanup is containment evidence, not the fix.
- Corrected root cause: V2 intentionally retains completed agents for reuse and LRU-unloads an idle terminal resident only when a later spawn needs capacity. The active V2 surface exposes `interrupt_agent` (which preserves the target) but no explicit close, so callers cannot release a finished resident immediately.
- V1 already registers `close_agent`; V2 registers six other collaboration operations and omits it in stable `0.144.4` and current upstream `main`. The npm JavaScript is only a native-binary launcher, so neither a plugin update nor an MCP wrapper can add the missing internal `AgentControl` route.
- Local `codex features list` resolves `multi_agent_v2=false`, but Codex selects stored/model-catalog `multi_agent_version` before the local feature fallback. This verified precedence explains the active V2 surface; the prior configuration-only kill-switch conclusion was incomplete.
- PR #19753 added explicit MCP-manager/client shutdown and process-group regression coverage. Normal successful session closure should now drain a completed worker's MCP stack; open issue #25426 separately shows that `close_agent` can still hang if thread termination wedges.
- Corrected containment: V2's effective limit comes from `features.multi_agent_v2.max_concurrent_threads_per_session` and currently yields root plus three residents. V2 bypasses the V1 depth check, so `[agents].max_depth=1` is not a recursive-fan-out guarantee.
- The tech-researcher checkpoint itself retained a verified 22-process worker tree after completion. Exact-tree cleanup removed all 22 and left the primary Codex process alive. This proves containment discipline, not lifecycle repair; the completed task remains logically registered in the current V2 session.
- Daemon defense-in-depth review: the first independent report returned `CHANGES_REQUIRED`; the corrected policy is auto-spawn-only by default. Red gate reproduced unset→`Some(600s)`; green focused gate passed 2/2. `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `git diff --check`, and `cargo test --all-targets -- --test-threads=1` all exited 0. The isolated real-binary smoke exited at 75 seconds and confirmed port/pid/token runtime files absent. Follow-up verdict: `APPROVE_COMMIT`; exact product scope committed as `ebe013333e6f4393846c3b0c85dd9d092b9da9fd` (`Cargo.toml`, `README.md`, `src/daemon.rs`).
- Codex-core implementation checkpoint: isolated branch `fix/multi-agent-v2-close-agent` is pinned at stable `rust-v0.144.4` base `8c68d4c87dc54d38861f5114e920c3de2efa5876`. The pre-change namespace and closed-agent/list tests passed. The preserved red assertion failed exactly with `expected close_agent in collaboration namespace`; the behavior test was also red on the missing V2 handler. The minimal bridge now registers `close_agent`, resolves V2 task paths, calls the existing V2-aware `AgentControl::close_agent`, documents post-close unavailability, and reuses the existing collaboration event contract. Green evidence: 65/65 `multi_agent_v2` tests, focused default/custom namespace tests, and close/follow-up/list behavior. `git diff --check` and Rustfmt check exit 0. The tag build's generated `Cargo.lock` version churn was removed; no compiler/linker/test processes remain. Independent code review is requested via `research/token-cost/claude-handoff-codex-v2-close-agent-code-review-a019.md` before broader gates or installation.
- Independent code review returned `APPROVE_CONTINUE`. A helper-complete `just test -p codex-core` then ran 2,594 tests: 2,582 passed, 11 failed, 1 timed out, and 46 skipped; no V2 close/spec-plan test failed. Residual failures were unrelated Windows symlink/elevation, an unrelated missing command-runner helper, network/mock timing, and known flaky hook/CLI cases, so the package gate remains honestly red rather than being called green.
- Standalone candidate SHA-256 `86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00` exposed a deployment blocker before model execution: the live API returned HTTP 400 because `collaboration.close_agent` is not in the reserved `collaboration` namespace allowlist. The identical candidate with `features.multi_agent_v2.tool_namespace='agents'` was accepted. Its sentinel MCP population moved root-only PID 62076 to root+worker PIDs 62076/59356, then back to root-only exactly when `close_agent` completed with previous status `completed`; the post-close follow-up was rejected, the worker disappeared from the live registry, the root MCP survived until normal root exit, and the separately owned unrelated sentinel remained alive until explicit cleanup. The diagnostic run's model verdict stayed `FAIL` only because both 15-second test-server hold calls were client-cancelled; final smoke must use shorter holds and must not reinterpret that diagnostic as a full pass.
- Independent namespace review returned `APPROVE_CONFIG_DEPLOY`: keep the seven-file patch unchanged, use the supported whole-surface `agents` namespace locally, and disclose the reserved-namespace backend dependency upstream.
- Deterministic final smoke passed its approved process/tool-result oracle at a hard root-plus-one capacity: owned MCP census 1 -> 2 -> 1 -> 2 -> 1 -> 0; both worker closes returned `completed`; post-close follow-up was rejected; the replacement spawn live-proved slot release; root and an unrelated sentinel survived both worker closes; candidate exit was 0 with `sequence_complete: true`. Five client-cancelled five-second sync holds remain harness notes, not treatment failures. At that checkpoint, smoke homes and test MCP processes were absent while the candidate target/worktree remained only for final review, commit, installation, and immediate disk cleanup.
- Final independent review returned `APPROVE_COMMIT_INSTALL_CLEANUP`. The exact reviewed seven-file Codex patch was committed as `288cdc6ec16c6d7c6bd0f6eceb09ac40a5cf7e0a` on `fix/multi-agent-v2-close-agent`, with pinned parent `8c68d4c87dc54d38861f5114e920c3de2efa5876`; the retained commit contains no lockfile, dependency, SymForge, or unrelated changes.
- The hash-pinned `codex-cli 0.144.4` binary was installed atomically at `C:\Users\rakovnik\.npm-global\codex.exe`; its SHA-256 is `86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00`. Both npm shims remained byte-identical. The live config contains exactly one `[features.multi_agent_v2]` table with `tool_namespace = "agents"` and no `multi_agent_v2 = true` scalar.
- Installed-product verification passed by absolute path and by a sanitized Win32 bare-name spawn. A live installed `agents.close_agent` call reached the V2 handler and returned `root is not a spawned agent`, proving registration and dispatch without the reserved-namespace HTTP 400. The policy-forbidden PowerShell `Get-Command` check was not bypassed; Git Bash continues to resolve the untouched extensionless npm shim.
- Cleanup removed 48,707 Cargo-target files (27.8 GiB reported by Cargo), removed and pruned the linked worktree, and retained the bare repository plus commit. Final custody showed the worktree and target absent, bare repo and seven-path commit present, zero `test_stdio_server.exe` processes, and 17,539,604,480 bytes free on `E:`. The broader Windows package gate remains honestly red at 2,582 passed, 11 failed, 1 timed out, and 46 skipped, with no failure in V2 close/spec-plan scope.

## Current campaign — token, speed, and tool trust

- [x] Create and switch to `feat/token-speed-tool-trust` without disturbing existing changes.
- [x] Complete the reconnaissance and fact-backed experimental design.
- [x] Repair the Claude bridge with a red/green MCP smoke test and preserve its timeout.
- [x] Obtain Claude Opus checkpoint-0 review.
- [x] Incorporate checkpoint-0 corrections into the experimental design.
- [x] Write and review the original Phase 0 benchmark manifest and exact task oracles.
- [x] Build the read-only harness, run the first full-arm observation, and stop on the corrected compact wiring gate.
- [x] Obtain Claude Opus approval for Amendment A before the compact-read annotation prerequisite.
- [x] Implement and verify only the compact `symforge` annotation prerequisite with a pinned candidate binary.
- [x] Diagnose the pre-treatment snapshot-readiness failure, obtain Opus approval for Amendment B, and prove the semantic materialization baseline.
- [x] Obtain Claude Opus code review of the Amendment B harness diff before retrying measured run 01.
- [x] Receive the independent second-terminal Opus report artifact for the post-run-01 checkpoint before enabling runs 02–20.
- [x] Run and blind-grade the 20-run full-versus-compact variance shakedown.
- [x] Ask Claude Opus to review the harness, traces, and statistical interpretation.
- [x] Complete the read-only Checkpoint-3 diagnostic of all 17 compact failures before choosing a rescue design.
- [ ] Size and run the host-stratified surface/discovery pilot.
- [ ] Implement only the smallest improvement supported by the confirmatory evidence.
- [ ] Run final verification, Claude Opus code review, and disk/worktree cleanup.

## Plan

- [x] Replace the per-call study with an end-to-end session benchmark after user correction.
- [x] Fix identical feature prompts, repository state, model, and completion criteria for both arms.
- [x] Run clean native-tools and SymForge sessions through final answer.
- [x] Verify task-result equivalence before comparing token totals.
- [x] Repeat or cross-check runs for variance and order effects.
- [x] Use per-call/schema measurements only to explain the observed session totals.
- [x] Write the final report and verify its claims against captured evidence.

## Evidence Log

- Evaluation started 2026-07-13.
- Current SymForge index: 726 files and 21,830 symbols.
- User correction: the only primary metric is total tokens for equivalent feature work in two clean sessions; individual call economics cannot answer that question.
- Accounting will not treat the user's approximate 7k-per-call figure as fact; actual session totals must include exposed schemas, requests, responses, retries, reasoning, and final answer.
- Delegated workers are intentionally skipped because project lessons record a verified Windows worker-process leak; an independent second measurement pass will provide the cross-check.
- Benchmark host: Codex CLI 0.144.2, `gpt-5.6-sol`, high reasoning; AAP commit `b1423aab350d1b065a550c42bf5f2b98c7d2c069`.
- Trial 1: native 2,085,089 tokens; SymForge-enabled 1,330,244; net 754,845 saved (36.2%).
- Trial 2, reverse order: native 3,092,305 tokens; SymForge-enabled 2,564,159; net 528,146 saved (17.1%).
- Combined: 1,282,991 tokens saved across two repetitions; mean 641,496 per feature run (24.8%).
- Included arms produced equivalent two-file implementations and green focused tests; all disposable AAP worktrees were removed after measurement.
- Event traces show only explicit `index_folder` MCP use followed by native events. A stricter arm made zero MCP calls and was excluded, so the result is enabled-and-indexed versus disabled host behavior, not pure explicit-tool causality.
- Current real schema measurement: full 36 tools = 72,757 bytes; compact 3 = 4,581 bytes.
- Claude Opus checkpoint 3 blocked progression after run 01: the original compact wiring check had false-passed a cancelled tool call, run metadata was incomplete, user Codex agents/skills contaminated traces, and incomplete evidence could not be rerun safely.
- Harness red/green fixes now isolate `CODEX_HOME`, require completed MCP calls, capture timestamped events and complete token/readiness metadata, emit blind grader copies, and quarantine incomplete reruns. SelfTest passes; personal-config diagnostics are zero.
- Run 01 is a valid oracle failure under the old full binary: one usage event, 721,303 canonical tokens, 20 completed SymForge calls; failed frozen S1 criteria 2, 4, and 5. It is invalidated from the restarted series because all arms must use one binary.
- Corrected wiring proves a shipped compact trust defect: direct MCP succeeds and full annotated `health_compact` completes, but compact `status` and compact `symforge` are cancelled by Codex. Production `compact_surface_tools()` creates fresh default tools without the full router's annotations.
- Amendment A preserves the failed baseline and permits only truthful read-only/closed-world annotations on compact `symforge`; `status(reset_calibration=true)` and `symforge_edit` must remain non-read-only. Zero-call measured runs remain scored under their assigned arm.
- The pinned candidate is `8.14.1`, SHA-256 `6C4176E03299B768793ACB64012FDD95783476B6AE59662FC4AD7B8C310FFC3B`; focused/full tests, clippy, format, check, and release build passed. Its 13.06 GB disposable Cargo target and the repo-local test target were removed.
- The first amended run-01 retry stopped before treatment on an invalid post-health byte-hash invariant. Two clean health-only cycles rewrote `index.bin` to different hashes while reporting snapshot verification `pending`; source inspection confirmed fresh-worktree mtimes, clean-shutdown serialization, and postcard `HashMap` order make cross-process bytes unstable.
- Amendment B keeps exact golden bytes only at readiness input, polls to `snapshot_restore`/`completed`/zero mismatches, and compares fixed-tree, tracked-source, repo-outline, parse-count, and candidate fingerprints. Opus returned `CHANGES_REQUIRED` on the first draft, then `APPROVE_PLAN` after semantic equality and the residual verifier confound were made explicit.
- Semantic baseline receipt: 851 tracked files; 726 indexed (720 parsed, 4 partial, 2 failed); 21,830 symbols; zero mismatches. Fresh materialization observed running→completed in 38.451 s; the exact-materialized-byte probe observed pending→completed in 1.307 s and preserved the semantic outline. A separate dry per-run readiness pass matched the baseline and removed its fixture/processes.
- Claude Opus Amendment B code review verdict: `APPROVE`. It confirmed the measured Codex process starts from the materialized snapshot, semantic fields are the equality key, baseline writes are no-overwrite/after-probe, process/token/secret/rerun gates fail closed, and only the authorized product metadata changed. Non-blocking hardening notes were retained for later harness cleanup; none can create a false pass or cross-arm asymmetry.
- Independent post-run-01 Opus audit verdict: `APPROVE_CONTINUE`; its independent raw-trace reparse reproduced 240,645 canonical tokens, 8 completed SymForge calls (first substantive call `search_symbols`), 2 native events, zero configuration/secret diagnostics, the pinned candidate/semantic fingerprints, clean teardown, and the frozen-oracle Fail on criteria 4 and 5.
- The audit's MEDIUM finding is accepted but does not alter this descriptive shakedown: production compact `tools/list` dispatches through `compact_surface_tools`, while frozen S1 requires probe-only `compact_probe_tools`. The run-01 grade stands and the oracle plus annotation/source custody must be corrected before the confirmatory pilot, never mid-series.
- Runs 02–20 captured sequentially with stop-on-first-failure; all 20 records are unique, have one usage event, exit 0, no timeout, semantic readiness `ready`, zero snapshot mismatches/configuration diagnostics/potential-secret lines/repository changes, and clean per-run teardown. Post-series state has one Git worktree and no fixture/candidate process/isolated home/Cargo target.
- Blind grading is complete and single-shot: 20/20 frozen-oracle Fails, 0 exclusions. All S1 answers hit the production-vs-probe oracle defect; all S2 answers omit the oracle's incidental mutation-before-clone ordering statement. The frozen grades are preserved and cannot support successful-task token/speed comparisons.
- Four-cell descriptive report: `research/token-cost/token-surface-shakedown-report-a019.md`. Compact S2 recorded 28 completed SymForge calls (18 success / 10 error) and 186 native read/search fallbacks versus full S2's 176/176 successful calls and 20 fallbacks; no winner or causal claim is made because every cell scored 0/5.
- Independent post-run-20 handoff is ready at `research/token-cost/claude-opus-handoff-post-run-20-a019.md`; raw/golden/candidate evidence remains retained until that review.
- Final verification found and fixed one post-series harness-test isolation defect: `SelfTest` had hardcoded run 20 as missing after run 20 legitimately existed. The artifact helper now accepts an optional root with the real evidence root unchanged as default; the test uses a unique nonexistent temporary root. `SelfTest` is green and measured records/grades are untouched.
- Independent post-run-20 Opus audit verdict: `APPROVE_SHAKEDOWN_CLOSURE`. It blind-reproduced the 0/20 grades, reparsed custody and usage from all raw traces, reproduced every four-cell statistic, confirmed 17 compact failures versus zero full failures, and found no closure blocker. Confirmatory work remains gated on repaired oracles/citation pinning, annotation-source custody, and the call-level failure diagnostic.
- Approved cleanup deleted both golden-state directories and the current A019 wiring quarantine after path, process, and worktree guards, freeing 34,539,032 bytes. All 20 raw traces, the older pre-restart invalidated evidence (including two historical wiring-probe bundles), compact in-repo evidence, and the pinned candidate remained through Checkpoint 3.
- Primary Checkpoint-3 diagnostic: `research/token-cost/compact-failure-diagnostic-a019.md`. It classifies all 17 failed compact facade calls as 4 pre-dispatch enum decodes plus 13 dispatched primitive outcomes (10 `EmptyResult`, 3 `NotFound`, 0 primitive `InvalidRequest`) that the executor collapses to facade `InvalidRequest`; no product code changed.
- Independent Checkpoint-3 report: `research/token-cost/claude-opus-report-checkpoint-3-a019.md`, verdict `APPROVE_DIAGNOSTIC`. It reproduced all 17 rows, every aggregate and hash, the source mechanism, and product/custody scope. Its two non-blocking notes are closed by specifying the exact CRLF trace-set hash recipe and distinguishing the deleted current A019 quarantine from the intentionally retained historical Amendment A wiring bundles.
- The already approved annotation prerequisite is now an isolated product commit: `0260760ac19e10f2f158411bf94201aaeed601e5` (`fix(stel): annotate compact read facade honestly`). No research, task, or unrelated workspace files were staged with it; the measured A019 candidate was not rebuilt or substituted.
- Post-Checkpoint-3 cleanup removed the exact pinned-candidate directory after resolving the path, confirming its sole 60,908,544-byte file, and finding zero exact-path process holders. Primary raw traces and the tiny historical Amendment A evidence remain retained; no Cargo target or disposable worktree was recreated.

## Review

- Complete: [end-to-end feature benchmark](../research/token-cost/end-to-end-feature-benchmark-2026-07-13.md)
- Verdict: observed mean end-to-end net saving is 641,496 tokens per completed feature run (24.8%); positive in both paired trials.
- Limitation: n=2 and treatment noncompliance prevent attributing the full delta causally to explicit SymForge retrieval/edit calls.

---

# SymForge v8 Architecture Review

## Plan

- [x] Checkout and confirm `v8/stel-architecture`.
- [x] Index the repo with SymForge and check project memory.
- [x] Read `docs/v8-bootstrap.md` fully.
- [x] Inspect §10 code paths and verify gap-vs-reality claims.
- [x] Read binding linked specs needed for §13.
- [x] Check `src/stel/` pre-flight invariant.
- [x] Synthesize concrete architecture findings and answer §13.
- [x] Document review results here.

## Evidence Log

- Branch confirmed as `v8/stel-architecture` after `git fetch origin`, `git checkout v8/stel-architecture`, `git pull`, and `git branch --show-current`.
- SymForge initially reported an empty index, then indexed `E:\project\symforge`: 250 files, 11750 symbols.
- Working tree had an existing local modification to `docs/v8-bootstrap.md`; this review treats that working-tree version as the active brief and does not overwrite it.
- agentmemory recall surfaced one relevant prior lesson: generation fences and cancellation are required when long-lived async state can outlive a project/session identity change.
- `Cargo.toml` still has `rmcp = { version = "1.1.0", features = ["transport-io"] }`; no Streamable HTTP feature is enabled.
- `src/main.rs` still chooses daemon-backed stdio first and falls back to local stdio plus a separate HTTP sidecar.
- `src/protocol/tools.rs` and `src/protocol/edit_tools.rs` still expose the legacy 32-tool router; `src/protocol/smart_query.rs` and `ask` route directly to one core tool, not to a STEL plan/controller.
- `src/stel/` does not exist, which is correct because `docs/v8-gap-closure-plan.md` §12A is still not fully green.

## Review

- Complete: [`docs/reviews/v8-architecture-review-codex-resume.md`](../docs/reviews/v8-architecture-review-codex-resume.md)
- Verdict: design sound; Phase 0 §12A is the blocker; proceed only through harness/golden file before `src/stel/`.
- Net-new gaps: G-032..G-036 in gap-closure plan (from Codex addendum).

---

# Phase 0 pre-flight (§12A)

## Plan

- [x] `compare-results.js` with `--preflight` (sf-bench commit `16acb4b`)
- [x] `routes.golden.jsonl` 36-row skeleton + `fixtures/preflight-minimal.json`
- [x] `scripts/measure-schema-bytes.ps1` stub (symforge `f7af058`)
- [ ] Human review of golden `expected_decision` / `expected_equiv` (≥10 rows)
- [ ] A-001..A-004 validated on real battery output
- [ ] `battery.js` emits v8 row fields (`decision`, `acceptedServe`, …)
- [ ] A-012 two-hop bypass harness
- [ ] A-005 / A-019 / A-025 validated

## Run

```powershell
cd E:\project\sf-bench
node compare-results.js --preflight --release 8.0
cd E:\project\symforge
.\scripts\measure-schema-bytes.ps1
```

## Review

- Preflight gate script verified (synthetic fixture passes H1–H5, H7).
- Still blocked on real harness trust + golden semantics before `src/stel/`.

---

# Init All-Client Durable Binary Failure

## Plan

- [x] Reproduce/trace the CI failure from the supplied panic.
- [x] Identify where the temporary binary guard loses the injected test home.
- [x] Patch `run_init_with_context` so all client branches use the same injected home context.
- [x] Run the focused failing init integration test.
- [x] Run format/check verification.

## Evidence Log

- Failure: `test_run_init_all_updates_both_clients` panics because the Claude Desktop branch refuses `/tmp/.../.symforge/bin/symforge` and asks for `/home/runner/.symforge/bin/symforge`.
- Root cause: `run_init_with_context` resolves the registration binary with the injected `home_dir`, then calls `register_claude_desktop_mcp_server`, whose public wrapper re-reads `dirs::home_dir()` and re-applies the temporary-binary guard against the real CI home.
- Added regression assertions that all-client init writes Claude Desktop config under the injected home, points it at the injected durable binary directory, and does not persist the temporary extraction binary. Before the fix, the focused test failed on Windows because the config was written to `%APPDATA%\Claude\claude_desktop_config.json`.
- Fix: split production path construction (`InitPaths::from_current_environment`) from injected path construction (`InitPaths::from_home_and_working_dir`), and route `run_init_with_context` through `register_claude_desktop_mcp_server_with_home`.
- Focused verification passed after the fix: `cargo test --test init_integration test_run_init_all_updates_both_clients -- --nocapture`.
- Inspected the real Claude Desktop config after the earlier test pollution. Current `symforge` points at the durable `C:\Users\rakovnik\.symforge\bin\symforge-desktop.cmd`; the available May 19 backup already had a temporary SymForge wrapper entry, so deleting or reverting the entry would be destructive and less correct.
- `cargo fmt --check` initially failed on one rustfmt wrapping change; `cargo fmt` applied it and the latest rerun passed.
- `git diff --check` passed with CRLF conversion warnings only.
- `cargo check` passed.
- `cargo test --test init_integration test_run_init_all_updates_both_clients -- --nocapture` passed with the JSON-parsed Claude Desktop command assertions.
- `cargo test --test init_integration -- --nocapture` passed: 24 passed, 0 failed.
- `cargo test --all-targets init -- --test-threads=1` first hit local disk exhaustion while writing `target/debug` artifacts. After removing the generated repo-local `target/debug/incremental` cache, the latest rerun passed: 95 selected tests passed across all targets, 0 failed.

## Review

- The CI failure was a real implementation bug, not a flaky test: all-client init lost the injected home only when it reached Claude Desktop registration. The fix preserves production `%APPDATA%` behavior while making the injected test path deterministic and isolated.

---

# SFB10 - Apply result-status semantics to read, search, and reference tools

## Plan

- [x] Run Branch Guard from the original checkout.
- [x] Switch to `.worktrees/backlog-implementation` and confirm branch/status there.
- [x] Index the target worktree with SymForge and check agentmemory for prior context.
- [x] Copy the SFB10 goal file into the worktree and mark it `In progress`.
- [x] Validate SFB09 dependency artifacts in current code because the SFB09 goal file is absent from this worktree.
- [x] Inspect current read/search/reference response construction and status contract helpers.
- [x] Add or update contract tests for found, not_found, ambiguous selector, invalid request, and empty/no-match states across read/search/reference surfaces.
- [x] Apply result-status metadata to `get_symbol`, `get_file_content`, `search_*`, and `find_references` while preserving existing human-readable text.
- [x] Capture before/after sample output for one found and one not-found response.
- [x] Run exact goal verification:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo test --all-targets -- --test-threads=1`
  - `rg "result_status|ResultStatus|outcome_class" src/protocol src/live_index tests`
- [x] Run default full verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Update SFB10 frontmatter to `Completed` with the verified work commit hash.
- [x] Commit the SFB10 goal-status update separately.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- The SFB10 goal file was absent from the worktree and was copied from the original checkout per Branch Guard.
- SymForge indexed the worktree: 191 files, 9292 symbols.
- agentmemory recall for SFB10/result-status/read-search-reference context returned no matching prior observations.
- `tasks/lessons.md` is absent in this worktree; no prior lesson file was available to review.
- SFB09 dependency file is absent, but dependency artifacts are present in code: `src/protocol/result_status.rs`, `src/protocol/mod.rs`, `tests/conformance.rs`, and the existing read-tool fixture in `src/protocol/tools.rs`.
- Goal status changed to `In progress` at `2026-05-20T14:40:39.2214133+02:00`.
- Implementation finding: the public RMCP router can return `CallToolResult`, while the existing tool bodies return human-readable `String` used by daemon/proxy/internal tests. The lowest-impact path is statused wrapper methods registered under the same tool names, keeping existing text renderers unchanged.
- Added status classifiers and registered wrappers for `get_symbol`, `get_file_content`, `search_symbols`, `search_text`, `search_files`, and `find_references` in `src/protocol/tools.rs`.
- Added contract tests covering:
  - read: `found`, `not_found`, `invalid_request`, `ambiguous`;
  - search: `found`, `empty_result`, `invalid_request`, `not_found`, `ambiguous`;
  - references: `found`, `empty_result`, `not_found`, `ambiguous`.
- Initial focused red check: `cargo test result_status_contract -- --nocapture` failed before wrappers because tool responses had no `_meta["symforge/result_status"]`.
- Focused verification passed:
  - `cargo test result_status_contract -- --nocapture`: 3 passed, 0 failed.
  - `cargo test test_get_file_content -- --test-threads=1`: passed.
  - `cargo test test_search -- --test-threads=1`: passed.
  - `cargo test test_find_references -- --test-threads=1`: passed.
  - `cargo test test_get_symbol -- --test-threads=1`: passed.
- Found sample:
  - before SFB10 wrapper: `content[0].text = "fn present() {}\nfn duplicate() {}\nfn duplicate() {}\n"`;
  - after SFB10 wrapper: same `content[0].text`, plus `_meta["symforge/result_status"] = {"contract_version":1,"outcome_class":"found"}`.
- Not-found sample:
  - before SFB10 wrapper: `content[0].text` begins `File not found: src/missing.rs`;
  - after SFB10 wrapper: same human text, plus `_meta["symforge/result_status"] = {"contract_version":1,"outcome_class":"not_found"}`.
- Exact goal verification passed:
  - `cargo fmt --check`: exit 0 after applying rustfmt.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; observed key totals include `src/lib.rs` 1761 passed, `src/main.rs` 6 passed, and all integration targets passed.
  - `rg "result_status|ResultStatus|outcome_class" src/protocol src/live_index tests`: exit 0 and showed the status module, conformance pins, wrappers, and SFB10 contract tests.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; full all-targets suite passed again.
  - `cargo build --release`: exit 0; finished release profile successfully.
- Verified implementation commit: `ee2207eca5a20f6c9a5241dc36aa58c2073fc3e7`.
- Goal frontmatter completed at `2026-05-20T15:26:17.1425475+02:00` with that implementation commit hash.

## Review

- SFB10 acceptance criteria passed before commit: public read/search/reference tool registrations now attach additive result-status metadata, existing human-readable text remains unchanged, and tests cover found, not_found, empty_result, ambiguous, and invalid_request states across the requested surfaces.
- Changes stayed within the allowed tracked areas: `src/protocol/tools.rs` and `tasks/todo.md`. No daemon, sidecar, npm, docs, plans, `.planning`, openspec, or edit-tool implementation files were modified.

---

# SFB09 - Define machine-readable MCP result-status contract

## Plan

- [x] Run Branch Guard from the original checkout.
- [x] Switch to `.worktrees/backlog-implementation` and confirm branch/status there.
- [x] Index the target worktree with SymForge and check agentmemory for prior context.
- [x] Copy the SFB09 goal file into the worktree and mark it `In progress`.
- [x] Inspect existing protocol response construction and RMCP content constraints.
- [x] Decide whether structured metadata is safe or whether a delimited footer/envelope is required.
- [x] Inspect existing schema/conformance/read/search tests for the lowest-blast-radius fixture.
- [x] Add failing contract tests for the status vocabulary and serialization/envelope shape.
- [x] Add a failing fixture showing one existing read/search tool can emit machine status while preserving human text.
- [x] Implement the central result-status type/formatter.
- [x] Run focused tests and capture example response output.
- [x] Run exact goal verification:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo test --all-targets -- --test-threads=1`
  - `rg "ResultStatus|result_status|outcome_class|not_found|ambiguous" src tests`
- [x] Run default full verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Update SFB09 frontmatter to `Completed` with the verified work commit hash.
- [x] Commit the SFB09 goal-status update separately.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- The SFB09 goal file was absent from the worktree and was copied from the original checkout per Branch Guard.
- SymForge indexed the worktree: 190 files, 9272 symbols.
- agentmemory recall for SFB09/result-status context returned no matching prior observations.
- `tasks/lessons.md` is absent in this worktree; no prior lesson file was available to review.
- Goal status changed to `In progress` at `2026-05-20T13:59:47.0838734+02:00`.
- Response construction finding: most existing read/search handlers still return human-readable `String`, while RMCP `CallToolResult` supports `content`, `structuredContent`, `isError`, and `_meta`.
- Decision: use RMCP `_meta["symforge/result_status"]` as the additive machine contract. No footer is needed because `_meta` is available, and `structuredContent` is avoided here because the goal requires preserving existing human text instead of converting read tools to JSON.
- Added central status contract in `src/protocol/result_status.rs`:
  - stable `OutcomeClass` vocabulary: `found`, `not_found`, `ambiguous`, `invalid_request`, `empty_result`, `internal_failure`;
  - `ResultStatus { contract_version: 1, outcome_class }`;
  - `into_call_tool_result` formatter that keeps the text content unchanged and attaches the namespaced `_meta` payload.
- Red test evidence:
  - `cargo test --test conformance result_status -- --nocapture` initially failed with unresolved import `symforge::protocol::result_status`.
  - The first integration fixture attempt also exposed that `SymForgeServer::new()` requires test setup arguments, so the read-tool preservation fixture was moved into the existing `src/protocol/tools.rs` test module.
- Focused verification passed:
  - `cargo test --test conformance result_status -- --nocapture`: 2 passed, 0 failed for the filtered contract tests at that point.
  - `cargo test --test conformance -- --test-threads=1`: 12 passed, 0 failed.
  - `cargo test test_get_file_content_text_can_carry_result_status_without_changing_text -- --nocapture`: matching read-tool fixture passed.
- Example response shape preserving human text:
  - human text: `src/lib.rs\nfn present() {}`;
  - machine status: `_meta["symforge/result_status"] = {"contract_version":1,"outcome_class":"found"}`;
  - serialized response keeps `content[0].text` exactly equal to the original human text and does not add `structuredContent`.
- Exact goal verification passed:
  - `cargo fmt --check`: exit 0 after rustfmt formatting.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; observed key totals include `src/lib.rs` 1758 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `rg "ResultStatus|result_status|outcome_class|not_found|ambiguous" src tests`: exit 0 and showed the new status module, conformance tests, read-tool fixture, and existing not-found/ambiguous call sites.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed again.
  - `cargo build --release`: finished release profile successfully.
- Verified implementation commit: `8bcb3ab8aabf73cfac52d07b221ec08a4e73a449`.

## Review

- SFB09 acceptance criteria passed before commit: the central contract/formatter exists, conformance tests pin vocabulary and `_meta` serialization shape, `invalid_request` maps to `isError`, and an existing `get_file_content` fixture demonstrates status attachment without changing human text.
- Changes stayed inside the allowed tracked areas: `src/protocol/**`, `tests/conformance.rs`, and `tasks/todo.md`. No live-index, daemon, sidecar, npm, docs, plans, `.planning`, or openspec files were modified.

---

# SFB08 - Preserve same-line inline docs in replace_symbol_body

## Plan

- [x] Run Branch Guard and switch to the dedicated `backlog-implementation` worktree.
- [x] Index the target worktree with SymForge and check memory/lessons context.
- [x] Mark the SFB08 goal file `In progress` with a start timestamp.
- [x] Inspect current `replace_symbol_body` formatting/span behavior and existing edit tests.
- [x] Add a failing regression test for preserving same-line inline docs when `new_body` is docless.
- [x] Implement the minimal span/formatting fix without changing unrelated edit behavior.
- [x] Run task-specific verification:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo test --all-targets -- --test-threads=1`
  - `rg "replace_symbol_body|inline doc|deprecated|raw_line_start" src tests`
- [x] Run default full verification when task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Update SFB08 frontmatter to `Completed` with the verified work commit hash.
- [x] Commit the goal-status update separately.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- SymForge indexed the worktree: 190 files, 9261 symbols.
- Goal status changed to `In progress` at `2026-05-20T13:16:53.2047193+02:00`.
- Root cause: docless `replace_symbol_body` used `raw_line_start` as the splice start. When `raw_line_start < sym.byte_range.0`, same-line doc text before the parsed symbol was removed with the old signature.
- Red test evidence:
  - `cargo test preserves_same_line -- --nocapture` failed with both new fixtures losing `/** @deprecated */`.
  - The TypeScript fixture failed with disk output starting `export function legacy`.
  - The Rust block-doc fixture failed with disk output starting `pub fn legacy`.
- Implementation:
  - Added `docless_replacement_splice_start` in `src/protocol/edit.rs`.
  - The helper detects same-line `/** ... */`, `/*! ... */`, and `#[doc ...]` prefixes before the parsed symbol and returns the first non-whitespace byte after the doc marker.
  - `replace_symbol_body` now uses that helper only when `new_body` does not supply docs.
  - Added TypeScript/JSDoc and Rust block-doc same-line regression tests.
- Focused verification:
  - `cargo test docless_replacement_splice_start -- --nocapture`: 3 passed, 0 failed.
  - `cargo test preserves_same_line -- --nocapture`: 4 passed, 0 failed after the helper tests were added.
  - `cargo test replace_symbol_body -- --nocapture`: 18 unit tests passed plus matching integration tests including dry-run, attached-doc, orphan-doc, and same-line fixtures.
- Task-specific verification passed:
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; observed key totals include `src/lib.rs` 1757 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `rg "replace_symbol_body|inline doc|deprecated|raw_line_start" src tests`: exit 0 and showed the new same-line `@deprecated` fixtures plus `raw_line_start` call sites.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed again after the helper tests were added.
  - `cargo build --release`: finished release profile successfully.
- Verified implementation commit: `691e0a713035309d910b78b3cdf2d540112a4d37`.

## Review

- SFB08 acceptance criteria passed: same-line TypeScript/JSDoc and Rust block-doc fixtures preserve `/** @deprecated */`, existing attached-doc and orphan-doc replacement tests pass, and required verification completed.
- Changes stayed inside the allowed tracked files/areas: `src/protocol/edit.rs`, `src/protocol/tools.rs`, and `tests` coverage inside the inline `tools.rs` test module. No forbidden files were modified.

---

# SFB07 - Pin search_text usage grouping behavior for doc comments and markdown

## Plan

- [x] Run Branch Guard and move work to `backlog-implementation`.
- [x] Copy the SFB07 goal file into the worktree.
- [x] Mark SFB07 in progress.
- [x] Inspect current `search_text(group_by="usage")` implementation and existing tests.
- [x] Choose and document the usage contract for doc comments and markdown.
- [x] Add regression tests covering ordinary comments, doc comments, and markdown.
- [x] Implement the minimal code change needed for the chosen contract, if current behavior is not already correct.
- [x] Capture search output samples for the chosen behavior.
- [x] Run focused `search_text` regression verification.
- [x] Run the exact goal verification command.
- [x] Run default verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Mark SFB07 completed and commit goal status.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- The SFB07 goal file was absent from the worktree and was copied from the original checkout per Branch Guard.
- SymForge indexed the worktree: 190 files, 9259 symbols.
- Goal status changed to `In progress` at `2026-05-20T12:35:27.8286992+02:00`.
- Implementation evidence:
  - `src/protocol/format.rs::is_noise_line` already documents and implements a non-doc-comment filter.
  - `src/protocol/format.rs::search_text_result_view` applies that line-noise filter only in `group_by="usage"` / `"purpose"`.
  - Existing test `test_search_text_group_by_usage_filters_imports` covered import filtering but did not cover ordinary comments, doc comments, or markdown.
- Decision: KEEP_DOC_MARKDOWN_USAGE_VISIBLE. Usage grouping stays a line-noise filter for imports and ordinary comments. Rust doc comments remain searchable context, and Markdown body text remains visible; hash-heading lines keep the existing comment-like filtering.
- Added regression tests:
  - `protocol::tools::tests::test_search_text_group_by_usage_keeps_doc_comments_visible`.
  - `protocol::tools::tests::test_search_text_group_by_usage_keeps_markdown_body_visible`.
- No production behavior change was required; current behavior already matched the chosen contract.
- Search output samples:
  - Doc-comment sample: `search_text(query="non-doc comment", path_prefix="src/protocol/format.rs", group_by="usage")` shows `/// Returns true if the line looks like an import statement or a non-doc comment.`
  - Ordinary-comment sample: `search_text(query="Should exclude the \"use\" import line", path_prefix="src/protocol/tools.rs", group_by="usage", include_tests=true)` shows `(1 import/comment match(es) excluded by usage filter)`.
  - Markdown sample: `search_text(query="SFB07", path_prefix="tasks", group_by="usage")` shows body/list matches under `tasks/todo.md` and `(1 import/comment match(es) excluded by usage filter)` for the heading.
- Focused verification:
  - `cargo test test_search_text_group_by_usage -- --test-threads=1`: 3 passed, 0 failed.
  - `cargo test test_search_text -- --test-threads=1`: 44 passed, 0 failed.
  - `cargo fmt --check`: passed after rustfmt formatting.
- Exact goal verification passed:
  - `cargo fmt --check`.
  - `cargo check`.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed; observed key totals include `src/lib.rs` 1752 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `rg "group_by.*usage|usage" src tests`: completed and showed the new usage tests plus existing usage-related call sites.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`.
  - `cargo check`.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed again; observed key totals include `src/lib.rs` 1752 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `cargo build --release`: finished release profile successfully.
- Verified implementation commit: `96b4954e4458dc79f10012e28222c8588916cc9f`.

## Review

- SFB07 acceptance criteria passed: the KEEP_DOC_MARKDOWN_USAGE_VISIBLE decision is recorded in test comments, regression tests cover ordinary comments, doc comments, and markdown, and existing `search_text` tests plus full verification passed.
- Changes stayed inside the allowed tracked files/areas plus the copied ignored goal-status file. No `docs/**`, `plans/**`, `.planning/**`, `openspec/**`, `npm/**`, daemon, edit protocol, or parsing files were modified.

---

# Outstanding-Work Hardening (2026-07-10)

## Plan

- [x] Audit `docs/OUTSTANDING-WORK.md` against current code, tests, runtime,
  memory, vault, releases, and live dogfood.
- [x] Approve and commit the code-first architecture design.
- [x] Convert product intent into explicit trust, tool-substitution, and token-
  economy acceptance gates.
- [x] Close Feature 018 browse/frecency code residuals.
- [ ] Close Feature 018 documentation/task residuals with the canonical dogfood
  artifacts in Task 12.
- [x] Replace inline daemon project instances with per-project slots and
  partition per-session protocol/cache state.
- [x] Make daemon home immutable and `index_folder` additive/persistent.
- [~] Route read, guidance, compact, and structural-edit tools explicitly by
  project. (Daemon-route core DONE 2026-07-11: `runtime_for_target` shared
  resolver + `single_project_routed_tool` peek/strip routing in
  `call_tool_handler` for the 16 read/guidance verbs; parity table + resolver
  contract tests green. REMAINING: `project` fields in tool input schemas +
  strict-client schema pins, local-mode explicit-project refusal guards,
  set-valued `search_files` cross-target merge, compact `symforge` facade
  routing through stel planner/executor, project-explicit structural edits.)
- [x] Replace the global snapshot write lock with same-path serialization.
- [ ] Carry selected-project/freshness evidence and expose project inventory.
- [ ] Make reconnect and runtime descriptors multi-session safe.
- [ ] Enforce daemon uniqueness and reap expired sessions.
- [x] Preserve generated-output admission through watcher single-file updates.
- [ ] Add native, preserving Grok initialization.
- [ ] Create the canonical Grok dogfood prompt and common-tool substitution
  scorecard.
- [ ] Resolve every outstanding-work ledger entry with executable evidence.
- [ ] Run focused/full Terminal Commander gates, release-binary multi-project
  dogfood, and adversarial review.
- [ ] Stop for explicit approval before push/PR/merge/publish/`cargo clean`.

## Evidence Log

- Design: `docs/superpowers/specs/2026-07-10-outstanding-work-hardening-design.md`
  committed as `1608433`.
- Executable plan: `docs/superpowers/plans/2026-07-10-outstanding-work-hardening.md`.
- Current code truth: the daemon already has deterministic project IDs and a
  multi-project `WorkingSet`, but ordinary `index_folder` still destructively
  mutates `active_project_id` while holding the project-map write lock through
  reload.
- Product gate: hardening is enabling work. Completion requires proving common
  repository-tool substitution and measured token savings with retained-answer
  checks, not merely green infrastructure tests.
- Feature 018 browse closure: `a646f23`; the repeated generic-name RED failed
  with four `new` hits, then the exact diversity and real-store frecency tests
  passed.
- Snapshot isolation: `3e756ee`; 42 persistence tests passed, including
  same-path serialization, distinct-path independence, reset locking, unique
  temp names, stale-temp cleanup, and failed-write cleanup.
- Daemon project isolation: `b729164`; exact cross-project, prior-generation,
  reload-serialization, and cross-session cache tests passed; daemon suite
  passed 68/68.
- Integrated verification after all three slices: `cargo fmt --check`,
  `cargo check`, `cargo clippy --lib -- -D warnings`, and the full library suite
  passed (`2709 passed; 0 failed; 2 ignored`).
- Immutable-home/additive `index_folder` (2026-07-11, resumed on
  `C:\AI_STUFF\PROGRAMMING\symforge`): destructive retarget removed;
  `open_project_for_session` is the one canonical open path for omitted `add`
  and `add=true` (shared durable idempotency ledger stored under the HOME
  project so same-key/different-target conflicts reject before load); reload
  persists an atomic snapshot with `checkpoint=written` / honest
  `checkpoint=degraded: ...` receipts; proxy success no longer resets
  local home fallback; daemon-proxy failure refuses destructive local
  fallback; stale per-session server replaced via `Arc::ptr_eq` index
  identity; slot cleanup/reinsertion closed by `ensure_project_slot_for_session`
  (join under authoritative registry write lock, reused by session open);
  session-close attach race closed (close removes the session record first,
  `add_project_to_session` undoes the join via `detach_project_membership`);
  proxy-failure test made hermetic (ephemeral bound-then-released port +
  pre-degraded flag, no port-1 assumption, no reconnect autospawn); new
  checkpoint-failure coverage
  (`test_index_folder_open_reports_degraded_checkpoint_on_snapshot_failure`).
  Receipts: `cargo test --lib daemon::tests::test_index_folder -- --test-threads=1`
  = 10 passed / 0 failed; `cargo test --lib daemon:: -- --test-threads=1` =
  72 passed / 0 failed; full `cargo test --lib -- --test-threads=1` =
  2714 passed / 5 failed — the 5 failures are exactly the still-red watcher
  generated-output fixtures owned by the next slice; `cargo clippy --lib -- -D
  warnings`, `cargo fmt --check`, `git diff --check` all exit 0.
- Watcher generated-output parity (2026-07-11): extracted the ONE path-shape
  rule (`shallowest_generated_output_prefix`) shared by the bulk demotion walk
  and a new per-event `discovery::is_untracked_generated_output_path`; wired it
  into `read_and_index` after the admission gate (path-shape checked first so
  ordinary events never touch git; git evidence consulted only for
  generated-looking components; fail-open on non-git trees; opt-in env honored;
  tracked file or tracked sibling under the prefix rescues to Tier 1; skip
  records deduped by the existing `demote_to_skipped_at_generation`). Receipts:
  `cargo test --lib watcher::tests:: -- --test-threads=1` = 38 passed / 0
  failed (all five previously-red fixtures green); `cargo test --lib --
  discovery:: live_index::store:: --test-threads=1` = 134 passed / 0 failed;
  full `cargo test --lib -- --test-threads=1` = 2719 passed / 0 failed /
  2 ignored; `cargo clippy --lib -- -D warnings`, `cargo fmt --check`,
  `git diff --check` all exit 0.
- Recovered-review blockers (2026-07-11, code slice): #1 `detect_impact`
  payload now carries a `source_filter` object (applied/excluded_paths/hint
  naming `include_data=true`); #2 empty filtered `what_changed` (uncommitted)
  disclosure now reports the filtered-out count, the source-focused default,
  and `code_only=false`; #3 `code_only` keeps unknown-extension source via
  `is_unparsed_source_path` allowlist (.sql/.sh/.bash/.zsh/.ps1/.psm1/.psd1/
  .bat/.cmd/.proto/.tf/.tfvars/.cmake/.gradle + Dockerfile/Makefile/
  GNUmakefile/justfile); #7 compact repo-map `is_intra_workspace_path` now
  also rejects `..` segments, UNC, and backslash-rooted paths; #8 CCR
  duplicate insert (same content-addressed handle) refreshes age instead of
  double-counting `total_bytes`/economics; #10 `quarantine_bad_snapshot` now
  holds the per-path snapshot lock (red test mirrors the reset-lock witness);
  #4/#18 018 tool-behavior contract reconciled (browse `(name,kind)` dedup,
  compact/tree containment parity, both new disclosures). Receipts: red
  witnesses failed first (3 FAILED), then targeted suites green
  (what_changed/detect_impact 15 passed; ccr+persist+sidecar 117 passed);
  full `cargo test --lib -- --test-threads=1` = 2725 passed / 0 failed /
  2 ignored; `cargo clippy --lib -- -D warnings`, `cargo fmt --check`,
  `git diff --check` all exit 0.
- Explicit project routing, daemon-route core (2026-07-11):
  `DaemonState::runtime_for_target(session_id, project)` is the one shared
  resolver (omission -> immutable home; open project ID first; unique current
  `project_name` among the session's OPEN projects as display text only;
  unknown/ambiguous -> deterministic candidate errors, no indexing, no
  frecency); `call_tool_handler` peeks/strips the `project` field for the 16
  routed read/guidance verbs and dispatches the existing per-project
  implementation; the three cross-project discovery verbs keep their own
  `project`/`projects` handling. Receipts:
  `cargo test --lib daemon::tests::test_project_routing_parity_table -- --exact
  --test-threads=1` = 1 passed;
  `daemon::tests::test_runtime_for_target_resolution_contract` = 1 passed;
  `cargo test --lib daemon:: -- --test-threads=1` = 74 passed / 0 failed.
- Explicit project routing, schema + local-guard slice (2026-07-11): added the
  optional `project` selector field (serde default, documented) to the 15
  routed input structs (GetSymbol/GetSymbolContext/GetFileContext/
  GetFileContent/GetRepoMap/SearchFiles/FindDependents/DiffSymbols/
  WhatChanged/AnalyzeFileImpact/ValidateFileSyntax/Explore/SmartQuery/
  EditPlan/Investigation), including both manual `Deserialize` Raw structs;
  added `SymForgeServer::foreign_project_refusal` and wired it after the proxy
  attempt in all 16 local handlers (`ask` included) so a stdio/embed/degraded
  server refuses a non-matching explicit selector instead of silently serving
  the bound project; 200 struct-literal sites updated mechanically from cargo
  E0063 spans. Receipts: `cargo test --test strict_client_schema_compat` = 1
  passed; focused `test_local_server_refuses_foreign_project_selector` = 1
  passed; full `cargo test --lib -- --test-threads=1` = 2728 passed / 0
  failed; `cargo clippy --all-targets -- -D warnings` exit 0. NOTE: Terminal
  Commander daemon became unavailable mid-session (health probe
  daemon_unavailable); remaining commands ran through the harness's headless
  shell — no visible terminals — until TC returns.
- Project-explicit structural edits (2026-07-11): the 7 edit verbs
  (replace_symbol_body/edit_within_symbol/insert_symbol/delete_symbol/
  batch_edit/batch_insert/batch_rename) joined the routed set — the batch-level
  `project` selector resolves through the same `runtime_for_target`, so
  worktree/`working_directory` validation runs against the SELECTED project;
  the selector was added to the 7 edit input structs only (NOT SingleEdit/
  InsertTarget — no nested conflicting routing); local handlers refuse foreign
  selectors via `foreign_project_refusal`; 51 more struct-literal sites updated
  from cargo spans. `tests/watcher_reload_cancellation.rs` updated from the old
  destructive-retarget contract to the immutable-home additive contract
  (2 projects after open, B healthy, nothing evicted). Receipts:
  `daemon::tests::test_explicit_project_edit_routes_and_preserves_worktree` =
  1 passed (explicit-B mutates only B, omitted mutates home A, unknown writes
  nothing); full `cargo test --lib -- --test-threads=1` = 2729 passed / 0
  failed; full `cargo test --all-targets -- --test-threads=1` = 0 failures;
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  `git diff --check` exit 0.
- Project inventory surfaces (2026-07-11, Task 7 part 1):
  `DaemonState::render_session_project_inventory` renders one row per open
  project (deterministic ID, display name/root, home marker, published
  counts/index state, generation, opened timestamp, snapshot presence) plus
  session last-seen; `status(detail="projects")` (new `StelStatusDetail::
  Projects` variant; local render lists the single bound project) is
  intercepted on the daemon route to serve the full session inventory; full-
  surface `health`/`health_compact` append the same inventory once MORE than
  one project is open (single-project outputs stay byte-compatible). Receipts:
  `daemon::tests::test_status_projects_detail_lists_session_inventory` = 1
  passed; full lib = 2730 passed / 0 failed; strict schema + stel param
  disposition tests pass; clippy/fmt clean. REMAINING from Task 7: the typed
  daemon tool receipt (machine-readable project/generation/load-source
  metadata through call_tool_value -> proxy -> statused wrapper).
- Session reaper (2026-07-11, Task 9 part 1): daemon-owned bounded interval
  task (TTL default 86400s via SYMFORGE_SESSION_TTL_SECS, min 60; sweep period
  ttl/4 clamped [10s, 600s]); candidates collected as (session_id,
  observed_last_seen) under the read lock, `close_session_if_expired` rechecks
  the SAME observation under the sessions write lock and atomically claims the
  record before shared project cleanup — a racing heartbeat wins; a claimed
  session's later heartbeat fails rather than resurrecting; teardown reuses the
  extracted `finish_removed_session` (shared with interactive close, so orphan
  watchers/projects are removed exactly once). Reaper holds a Weak on daemon
  state (exits on daemon drop) and is aborted in run_daemon_until_shutdown.
  Receipts: `daemon::tests::test_reaper_rechecks_heartbeat_before_close` = 1
  passed (first attempt failed on a same-millisecond fixture bug — fixed to a
  realistic ancient-observation/past-cutoff shape); daemon suite 77 passed / 0
  failed; full lib 2731 passed / 0 failed; clippy/fmt clean. REMAINING from
  Task 9: the guarded-start seam for foreground `symforge daemon` vs auto-spawn
  (tests/daemon_singleton.rs) and last-seen/TTL evidence in detailed status.
- Typed project-evidence receipt (2026-07-11, Task 7 part 2): new
  `ProjectEvidence` contract (project_id, project_name, canonical_root,
  generation, index_state, load_source, index counts) in
  `protocol::result_status`; the daemon returns it OUT-OF-BAND as the
  `x-symforge-project-evidence` response header built from the RESOLVED
  runtime (so an explicitly routed sibling is attested as itself, never home)
  while the text body stays byte-identical; `call_tool_value` parses the
  typed header into a per-dispatch task-local slot (same bound-to-the-future
  pattern as the D23 connection surface — never reconstructed from body
  text); `ServerHandler::call_tool` seeds the slot with the LOCAL bound
  project so stdio/embed responses attest themselves; statused results attach
  the current evidence under `_meta["symforge/project_evidence"]`. Receipts:
  `daemon::tests::test_tool_receipt_carries_project_evidence` +
  `protocol::tools::tests::test_local_tool_meta_carries_project_evidence` =
  2 passed; full lib 2733 passed / 0 failed; full all-targets suite 0
  failures; clippy/fmt/diff-check clean.
- INCIDENT + fix (2026-07-11, CRITICAL): running the new reconnect test set
  off an exponential process fork bomb that flooded the desktop with console
  windows and made the machine unusable (user had to kill everything). Chain:
  `reconnect` -> `ensure_daemon_running` -> `spawn_daemon_process` spawns
  `current_exe()` with arg `daemon`; under `cargo test` that exe is the
  libtest binary and `daemon` is a TEST FILTER, so each spawn re-ran the
  daemon test subset, which spawned again; every subprocess those tests
  launch from a console-less parent popped a new console window. Inner
  trigger: tests waited on the LEGACY (untagged) daemon port file, which is
  never written, so daemon 1's graceful-shutdown cleanup raced daemon 2 and
  DELETED its fresh port+token files (production-relevant restart race:
  clients went tokenless -> 401 -> "no daemon" -> auto-spawn). Fixes: (1)
  `spawn_daemon_process` refuses under cfg(test), from any Cargo `deps/`
  artifact, and under `SYMFORGE_DAEMON_AUTOSPAWN=off`; (2)
  `ensure_daemon_running` fails fast with the same refusal instead of
  waiting; (3) shutdown cleanup is now owner-checked
  (`cleanup_daemon_runtime_files_if_owner` compares file contents to this
  daemon's port/pid/token before deleting) so a successor's files survive;
  (4) the Task-8 tests wait on the OS-TAGGED port file. Receipts:
  `test_test_builds_never_auto_spawn_daemon_processes` pins both refusal
  seams; `test_reconnect_reopens_home_and_working_set` = 1 passed (home id
  verified, sibling B reopened + verified, unqualified reads still home);
  daemon suite 80 passed / 0 failed; full lib 2735 passed / 0 failed; ZERO
  symforge processes remain after the suite. Lesson recorded in
  tasks/lessons.md.
- Reconnect working-set restore (2026-07-11, Task 8 part 1):
  `DaemonSessionClient` records additively-opened sibling roots (shared,
  deduplicated, order-preserving); `reconnect` verifies the home project id
  is unchanged (fail closed), reopens every sibling, and verifies each
  restores with its deterministic id before serving. REMAINING from Task 8:
  per-adapter/session runtime descriptors replacing the fixed sidecar
  port/pid/session files + hook lookup freshest-healthy selection.
- No-visible-terminal invariant (2026-07-11, user mandate): EVERY process
  spawn in src/ and tests/ now routes through
  `process_util::hidden_command` (CREATE_NO_WINDOW on Windows) — 21 src/test
  call sites swept plus 12 more integration-test sites the new tripwire
  caught; `hidden_command` and its module are now pub (#[doc(hidden)]) so
  integration tests share the helper; the ONE deliberate exception is
  `spawn_daemon_process` (its own DETACHED_PROCESS | CREATE_NO_WINDOW).
  Permanent tripwire `process_util::tests::
  test_no_raw_command_spawns_outside_hidden_command` scans src/ + tests/
  (fixtures excluded) and fails on any new raw `Command::new(` call site.
  Receipts: tripwire green; full `cargo test --all-targets -- --test-threads=1`
  = 0 failures; clippy/fmt clean; zero symforge processes after the suite.
- Per-session runtime descriptors (2026-07-11, Task 8 part 2): each
  adapter/sidecar now writes ONE atomic per-process OS-tagged JSON descriptor
  (`.symforge/sessions/sidecar.<pid>.<os>.json` — session_id, project_root,
  pid, port, updated_at) instead of the fixed sidecar.<os>.{port,pid,session}
  files; shutdown/panic cleanup removes ONLY the caller's descriptor (sibling
  adapters on one root can no longer be overwritten or deleted); the reader
  (`read_sidecar_status_at`, shared by hook lookup and status surfaces) scans
  descriptors first with identity validation (foreign project_root rejected,
  never last-writer), live-port-first selection, freshest updated_at, stable
  smallest-pid tie break, and falls back to the legacy fixed files as a
  read-only migration aid; `symforge update` purges stale (dead-port)
  descriptors. Two integration tests that pinned the fixed-file contract were
  migrated to the descriptor contract. Receipts:
  `test_per_session_descriptors_do_not_delete_siblings` +
  `test_reader_selects_live_descriptor_and_rejects_foreign_root` green;
  port_file suite 14 passed; full lib 2738 passed / 0 failed; full
  all-targets 0 failures; clippy/fmt/diff-check clean; zero leaked processes.
- Guarded daemon start + TTL evidence (2026-07-11, Task 9 part 2): new
  `guarded_daemon_start` seam (pub, `src/daemon.rs`) — acquire the start lock
  with a bounded 10s wait, re-check for a live compatible daemon UNDER the
  lock, stop an incompatible recorded daemon, then bind in-process; a live
  daemon's runtime record is never overwritten. Foreground/service
  `symforge daemon` (`run_daemon_until_shutdown`) now goes through the seam
  and refuses with "already running on port N" instead of clobbering.
  `ensure_daemon_running` drops the start lock immediately after
  `spawn_daemon_process()` so the spawned child's guard can acquire it (the
  old hold-through-wait would deadlock parent against child). Detailed status
  inventory line now carries `ttl_secs=` next to `last_seen=` (reaper TTL
  evidence). Receipts: new `tests/daemon_singleton.rs`
  (`test_guarded_start_refuses_to_replace_live_daemon`,
  `test_concurrent_guarded_starts_yield_one_daemon`) green; inventory test
  extended with last_seen/ttl assertion, green; daemon lib suite 80 passed;
  clippy/fmt clean; full all-targets suite receipt below.
- search_files multi-target merge (2026-07-11, Task 4 leftover part 1):
  `SearchFilesInput` gains set-valued `projects` (schemars `with="Vec<String>"`
  for strict-client schema parity); `search_files` joins the cross-project
  read verbs — the fan-out reuses the EXISTING per-project ranked file search
  (`WorkingSet::search_files` → `capture_search_files_view_with_noise` on each
  targeted entry's base index, honoring `path_prefix` via `PathScope`), merges
  attributed hits under the shared deterministic global cap
  (`cross_project_result_cap`) and `max_tokens` budget, and renders
  `── project: <id> ──` sections via `format_cross_project_files`
  (metadata-only reasons disclosed). Lone `project` stays on the FULL
  single-project routed handler (resolve/coupling modes preserved);
  resolve/changed_with/anchor_path/rank_by/current_file are honestly REFUSED
  with cross-project targeting; `project`+`projects` together is rejected
  deterministically in `call_tool_handler` (the routed strip would otherwise
  swallow the conflict). Receipts: new
  `daemon::tests::test_search_files_projects_fan_out` +
  `live_index::view::tests::cross_project_search_files_attributes_hits_per_target`
  green; daemon lib 81 passed; view 13 passed; strict_client_schema_compat +
  stel_param_disposition green; clippy/fmt clean; full-suite receipt pending.
- Watcher "binary" mislabel FIXED (2026-07-11): root cause was NOT the size
  threshold (code files already get 4 MB) — `is_binary_content`'s 8 KB sniff
  window cut `src/protocol/tools.rs` mid-multibyte `─` at byte 8190
  (empirically verified against the real file), and the "unexpected end of
  data" decode error read as invalid UTF-8 → Tier 2 "binary". Fix: an
  incomplete sequence at the truncation boundary (`error_len() == None` with
  bytes remaining past the window) is a sampling artifact, not binary
  evidence; genuinely invalid interior bytes still classify as binary.
  Receipts: red→green
  `test_binary_sniff_forgives_multibyte_cut_at_window_boundary` +
  `test_binary_sniff_still_detects_interior_invalid_utf8`; discovery 74,
  watcher 38, store 62 passed; clippy/fmt clean.
- get_file_context completeness lie FIXED (2026-07-11): the sidecar stamps the
  trust envelope (incl. `Completeness: full`) BEFORE `get_file_context` runs a
  SECOND budget pass (`enforce_token_budget`) over envelope+body+footer with
  the same byte cap — a body that just fit the sidecar budget was tail-cut
  after the claim (window exists when the outline symbol cap doesn't fire,
  i.e. ≤25 symbols; proven red at max_tokens=72 on an 8-symbol fixture). Fix:
  `enforce_token_budget_flagged` reports the cut and
  `downgrade_full_completeness_after_truncation` rewrites the stamped claim
  (both compact `Trust:` and expanded `Completeness:` envelope forms) to
  `budget-limited (was: ...)`. Receipts: red→green boundary-sweep
  `test_get_file_context_never_claims_full_after_post_assembly_truncation`
  (max_tokens 40..=400); get_file_context 10, format 198, sidecar 109 passed;
  clippy/fmt/diff-check clean; full-suite receipt pending. Note: the original
  observation's "only next-steps tail visible" shape was most likely the
  CONSUMING harness's display truncation; the tool-side dishonesty window was
  real regardless and is now closed.

## Review (2026-07-11, Task 13 — campaign closure)

### Verified commits (this campaign, `4cd9b34..HEAD`)

`ea342f8` immutable-home/additive index_folder · `f4e972c` watcher
generated-output parity · `ce554b1` recovered-review blockers · `d651ba5` +
`3d5a209` + `489c285` explicit project routing (reads + edits) · `7be810e`
project inventory · `c0e6307` session reaper · `ed143c4` typed
ProjectEvidence · `671b281` fork-bomb guards + owner-checked cleanup +
reconnect restore · `8899957` hidden_command sweep + tripwire · `bc96594`
per-session descriptors · `d0623f5` guarded-start singleton + TTL evidence ·
`f40352d` search_files fan-out + facade project routing · `7656699` binary
sniff boundary fix · `51bda85` ledger closure docs · `6f6eac6` completeness
downgrade after truncation.

### Final gate receipts (at `6f6eac6`; later commits are docs-only)

- `cargo fmt --check` ✓ · `git diff --check` ✓ · `cargo check` ✓ ·
  `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --all-targets -- --test-threads=1`: **110 test binaries, 3460
  passed, 0 failed, exit 0** (`suite-envelope.log`)
- `cargo build --release` exit 0 · `cargo check --no-default-features
  --features embed` ✓ · `npm test` 31/31
- Tool-correctness harness on the RELEASE binary: `verify-tools` 8 PASS +
  `verify-tools-real` 11 PASS, 0 REVIEW, 0 FAIL
- Release-binary multi-project dogfood (isolated `SYMFORGE_HOME`, real
  daemon, 2 projects, sibling adapters): **17/17 checks PASS** — additive
  receipt + checkpoint evidence, inventory with home marker +
  `last_seen`/`ttl_secs`, immutable-home routing (no B-byte leak), explicit-B
  reads, `projects=["*"]` fan-out attributed for files+symbols, unknown
  selector candidates, sibling shutdown does not break the surviving session.
- Tool-substitution scorecard filled from measured transcripts
  (`docs/reviews/2026-07-10-tool-substitution-scorecard.md`): aggregate ~7.8×
  token advantage with all facts retained; unfavorable rows (narrow `rg`
  queries) recorded honestly.

### Adversarial findings (self-review; delegation forbidden by handoff)

1. **Stale-snapshot serving (existing behavior, NOT fixed here):** a fresh
   local session over a repo with an old `.symforge` snapshot served stale
   index content as `current index` (daemon.rs at 210 symbols vs. current
   314) until an explicit `index_folder`. Same class as the known
   external-edit staleness finding; follow-up candidate.
2. **Trust-envelope gap:** `get_symbol`, `find_dependents`, `edit_plan`
   responses carry no trust envelope (scorecard rows 2/6/8) while
   search/context tools do. Follow-up candidate, not a regression.
3. **Facade injection edge (accepted risk):** the per-step `project`
   injection skips a non-object step `args` silently; the planner only emits
   object args, and the all-or-nothing tool check guards the honest case.
4. **Behavior change (deliberate):** routed verbs now reject
   `project`+`projects` together (previously a stray `projects` on a routed
   tool was silently ignored). Honest-refusal improvement; pinned by test.
5. **Guarded start residuals (bounded):** a crashed lock-holder stalls a
   foreground start for up to the 30s stale-lock threshold; a narrow race can
   transiently spawn a second child which exits cleanly via
   `AlreadyRunning` without touching the winner's runtime record (pinned by
   `tests/daemon_singleton.rs`).
6. **Environment evidence:** during review the machine ran 3 daemons + 5
   adapters of the INSTALLED 8.13.9 (pre-campaign) binary, and this repo's
   hooks were served by a daemon rooted at another project — live
   confirmation of the sprawl/retarget class this branch fixes.

### Remaining operator gates

Merge approval → merge with the release-please guard → publish → restart
harness sessions (installed daemons pick up the fixes) → `cargo clean`.

## v8.15.0 changelog and release-note reconciliation (2026-07-15)

- [x] Pull `main` with `--ff-only` before editing.
- [x] Pin the release range to `v8.14.1..v8.15.0` from Git tags.
- [x] Compare the published SymForge body with Terminal Commander's detailed
  v0.1.80 release format.
- [x] Replace the commit-coverage summary with a user-facing narrative covering
  behavior, evidence, compatibility, verification, and published artifacts.
- [x] Verify statistics, release-range coverage, links, wording, diff, and
  version synchronization.
- [x] Replace and independently re-read the published GitHub v8.15.0 body.
- [x] Commit and push the corrected repository changelog to `main`.

### Review

- Fact audit: 6/6 checks passed across all 8 non-release-metadata commits,
  computed diff statistics, both benchmark reports, and every required section.
- `git diff --check` passed; version synchronization remains `8.15.0`.
- The published release body matches the local `8.15.0` changelog section after
  newline normalization: 6,979 bytes and SHA-256
  `4a5a7d0bf02a62379406b2cdbb6233c1ee79761570a4e52a7275d3452404af08`.
- Changelog commit `4eff05037154812a1fd5ad5d316290e8c16d426c` is on `main`;
  local HEAD and `refs/heads/main` matched exactly after push.

## Repository Knowledge Index (2026-07-16)

### Plan

- [x] Define the product boundary: repository knowledge retrieval is separate
  from code intelligence and must not create prose symbols/references.
- [x] Inspect current file admission, Markdown section extraction, search, watcher,
  and persistence seams.
- [x] Research established document-element parsing and embedded lexical-search
  designs; treat embeddings as optional, not foundational.
- [x] Present the smallest complete v1 design and obtain user approval.
- [x] Write the approved implementation spec with data contracts, tool UX,
  ranking rules, persistence, watcher behavior, and acceptance tests.
- [x] Resolve the final four SpecKit blockers: invariant target enum, complete source-
  version propagation, Gate-E/G/H publication layering, and compact search evidence.
- [x] Rerun local link, ID, traceability, contradiction, and checklist checks.
- [x] Capture the clean Gate-A baseline for discovery, persistence, search, watcher,
  surface, and admission before production changes.
- [x] Complete fresh Architect/Skeptic/Minimalist review and resolve every accepted
  HIGH/MEDIUM finding before freezing the SpecKit.
  - [x] Preserve and independently verify the exact external Fable report.
  - [x] Adjudicate all 24 HIGH/MEDIUM findings against current artifacts/source and
    apply every accepted correction plus the 12 LOW cleanups.
  - [x] Obtain the focused three-lens re-review; preserve its five unrefuted MEDIUMs
    and five LOW findings at
    `specs/020-repository-knowledge-index/fable-focused-rereview-2026-07-17.md`.
  - [x] Correct the five focused MEDIUMs, apply the coherent LOW cleanup, and add all
    nine named red-test obligations without changing production code.
  - [x] Rerun mechanical ID/gate/link/focused-assertion and diff checks.
  - [x] Obtain the permitted scoped delta verification with no unrefuted HIGH/MEDIUM
    finding.
- [ ] Implement test-first in bounded slices and run focused plus full gates.
  - [x] Gates A-C: frozen baseline, metadata-first scout, and stable bounded execution.
  - [x] Gate D reopened after scoped adversarial review; preserve the verified writer/fence work
    while closing only the sustained reconciliation, coverage-authority, and impact-seam gaps.
    - [x] Retain metadata-failure entries and gate removals on Complete coverage.
    - [x] Keep coverage Degraded from manifest transients/aborts until stable replacement.
    - [x] Re-observe circuit-breaker aborts and remove dead duplicate repair state.
    - [x] Normalize and route impact updates through the shared single-file admission seam.
    - [x] Treat failed rescouts as Degraded and add the four missing behavioral oracles.
    - [x] Re-run focused/stress suites and capture a fresh authoritative serial receipt.
  - [ ] Gates E-M: continue only in frozen order after each prior gate is green.
- [ ] Before final review/commit/push, close the AAP-reproduced SymForge tool-contract report in
  priority order with behavioral REDs, focused GREENs, impact review, and regression coverage:
  - [x] SF-AAP-001: literal existing repo-relative paths (`.md`, `.ts`, `.d.ts`, `.test.ts`, `.json`,
    `.ps1`, dotted basenames, and symbol-free files) always beat symbol/generated heuristics in
    `edit_plan`; return a typed file limitation when needed and never recommend another path.
    (FIXED: `plan_edit` short-circuits an existing literal path — exact or `/`-anchored suffix —
    before the symbol cascade, which was stripping `config.json`->`json`. Tests
    `edit_plan_literal_path_precedence` + 9 `edit_plan_symbol_line` regressions.)
  - [x] SF-AAP-003: parallel `get_file_content` range calls complete, queue within a bounded time, or
    return a typed busy/unsupported result with request/cancellation diagnostics; never hang.
    (ALREADY SATISFIED — did not reproduce: 32 concurrent range calls incl. the reconcile write
    path complete in ~0.1s; no lock held across await; regression coverage `get_file_content_concurrency`.
    Caveat: the daemon proxy/IPC transport is not exercised in-process.)
  - [x] SF-AAP-002: `analyze_file_impact` reports stable `exists=true` plus generation/Tier-2 evidence
    and a typed unsupported-analysis result for newly reconciled non-parser files, never false absence.
    (FIXED: `impact_skipped_text` reports `exists:true` + Tier + Generation + typed unsupported ONLY for
    genuinely non-parser files — `LanguageId::from_extension` gate — so size-demoted PARSER files keep
    their `impact_refuses_oversized_*` refusal. Tests `analyze_file_impact_non_parser`; `impact_admission` 5/5.)
  Do not claim SpecKit 020's Markdown/read work alone fixes any of these independent contracts.
- [ ] Verify token-efficient retrieval on real architecture/spec/plan documents
  and document the results.

### Evidence Log

- Current Markdown extraction already computes section byte/line spans, but stores
  them as code-facing `SymbolRecord` values and recognizes only ATX headings.
- Mature ingestion systems partition documents into typed structural elements
  before any optional chunking; Rust parsers expose Markdown AST source positions.
- A dedicated in-memory knowledge index can reuse SymForge discovery, watcher,
  snapshot, and byte/line provenance infrastructure while keeping its retrieval
  model and MCP tools separate from code intelligence.
- User approved a metadata-first pre-run scout and accepted bounded cold-start
  latency in exchange for explicit scope, safe admission, and current coverage.
- The implementation plan is `docs/plans/2026-07-16-repository-knowledge-index.md`.
  It reuses the existing `FileClass::Text`, `SearchScope::Text`, trigram search,
  exact line rendering, and Markdown section spans instead of adding embeddings
  or another database.
- Adversarial review found five prerequisite consistency defects: discovery
  counts hard-skipped artifact bytes before admission; watcher reads before
  admission; read/circuit-breaker failures can disappear from accounting;
  snapshot verification bypasses the shared policy; and publication exposes
  independently swapped state that can be observed across generations.
- Work is isolated on branch `feat/repository-knowledge-index`.
- Fresh-session blocker closure replaced the boolean target pair with the closed
  `Code`/`Knowledge`/`CodeAndKnowledge` enum; propagated closed working-tree state
  through manifest/snapshot/publication/per-source envelopes; made Gate E core-only
  with Gate G/H bundle extensions; and reduced search hits to compact display plus
  stable IDs/bounded previews while retaining full review dossiers.
- Local verification: 16 Markdown files had zero broken local links; 75 requirement/
  success IDs and 277 task IDs had zero duplicates; Gates A-M each existed exactly
  once; 25 blocker/contradiction assertions and 11 changed-requirement traceability
  mappings passed; `git diff --check` exited 0 (only existing line-ending warnings).
- Local Architect/Skeptic/Minimalist review tightened link IDs to exclude changing
  resolution state and made compact ID/preview vectors canonically bounded with
  explicit coverage. At that point, fresh independent external review remained the
  freeze blocker.
- External Fable review is preserved verbatim at
  `specs/020-repository-knowledge-index/fable-adversarial-review-2026-07-17.md`;
  scratchpad/report byte identity was independently verified. Its verdict was PASS
  WITH CHANGES: 2 HIGH, 22 MEDIUM, and 12 LOW findings.
- All HIGH/MEDIUM findings were sustained and corrected. Finding 16 dissolved by
  deleting the dead verification-baseline machinery. Finding 21's source description
  was narrowed: the current cache check precedes target freshening and can suppress
  the required current reread; it does not directly replay cached file bytes.
- Corrections now cover foreign-source curation replay, serialized multi-source
  publication, verifier/temporal fencing and convergence, typed state ownership,
  degraded repair, platform durability, bounded canonical types, stable IDs, compact
  CCR retrieval, generation-aware deep reads, classification-aware object reuse, and
  fail-closed suppression under derived-budget exhaustion.
- Post-correction verification: 75 requirement/success definitions and 306 task
  definitions are unique; Gates A-M each exist exactly once; local Markdown links are
  intact; 73 focused contradiction/trace assertions pass; retired baseline/dead enum
  terms are absent from canonical artifacts; and `git diff --check` exits 0 apart
  from existing line-ending warnings on unrelated tracked files.
- Focused freeze-gate packet:
  `specs/020-repository-knowledge-index/fable-focused-rereview-request-2026-07-17.md`.
- The focused re-review is preserved at
  `specs/020-repository-knowledge-index/fable-focused-rereview-2026-07-17.md`; it found
  no HIGH and authorized one scoped delta pass after five sentence-scale MEDIUM fixes.
- All five MEDIUM corrections, five coherent LOW cleanups, and nine named red-test
  obligations now land across the canonical artifacts. Production code remains
  intentionally untouched.
- Delta-correction verification: 75 requirement/success definitions and 314 task IDs
  are unique; Gates A-M and all nine named red tests each exist exactly once; 12
  focused assertions pass; local Markdown links have zero broken targets; and
  `git diff --check` exits 0 apart from existing line-ending warnings.
- Final freeze-gate packet:
  `specs/020-repository-knowledge-index/fable-scoped-delta-verification-request-2026-07-17.md`.
- The resulting report is preserved at
  `specs/020-repository-knowledge-index/fable-scoped-delta-verification-2026-07-17.md`:
  PASS, all five MEDIUM corrections sustained, no regression, and READY TO FREEZE.
- Its one non-blocking LOW is closed by defining non-Git root identity through the
  existing bounded `PlatformFileId` mechanism and recording required catalog-digest
  transitions in the durable `ProjectStateDir` replay store; K-R18 carries the variant.
- Gate A is complete and the SpecKit is frozen. Production implementation starts at
  Gate B under strict RED/GREEN/VERIFY ordering.
- Gate-A pre-change baseline is green (223 passed, 0 failed, 1 intentionally ignored):
  `cargo test --lib discovery::tests:: -- --test-threads=1` passed 74; the cold build
  took 598.276s and the tests 2.86s. `cargo test --lib live_index::persist::tests:: --
  --test-threads=1` passed 43 in 14.96s. `cargo test --lib
  live_index::search::tests:: -- --test-threads=1` passed 62 in 0.29s. One serialized
  Cargo run across `admission_acceptance`, `impact_admission`, `surface_default`,
  `surface_honesty`, `watcher_index_folder_leak`, `watcher_integration`,
  `watcher_layer3_restat`, and `watcher_reload_cancellation` passed 44 with the one
  scheduled/manual watcher perf smoke ignored; command exit 0 in 102.885s.
- Gate B B-R01 completed strict RED/GREEN: a sparse 100 MiB `HardSkip` initially
  failed under a 1 KiB admitted-byte ceiling because discovery charged its declared
  size; metadata admission now precedes accounting and only `Normal` files consume
  that budget. The named test passes, all eight bounded-discovery tests pass, all 75
  discovery tests pass, and `cargo fmt --all -- --check` exits 0.
- Gate B B-R02 completed compile-RED, behavioral-RED, then GREEN: the new scout
  initially ignored its injected metadata source and published the candidate; it now
  omits double-failure paths, records a fixed safe `DirectoryEntryUnreadable` issue
  with owned `AccessErrorKind`, and degrades coverage without fabricating size zero.
- Gate B B-R03 passed immediately for the intended reason once the closed scout
  decision/types existed: unchanged state produces the same ordered entry projection,
  all code/knowledge/catalog-only candidates remain represented exactly once, and
  resource entry count matches. No redundant production sort was added.
- First Gate-B batch regressions are green: the two metadata-first scout tests pass,
  39 domain-index tests pass, all 77 discovery tests pass, and Rustfmt check exits 0.
- Gate B B-R04 completed compile-RED, behavioral-RED, then GREEN: an injected probe
  initially observed no calls; metadata-terminal model/sparse artifacts now bypass
  probe I/O while the sole undecided candidate receives exactly one bounded
  `BINARY_SNIFF_BYTES` read through `Read::take`.
- Gate B B-R05 completed compile-RED, behavioral-RED, then GREEN: injected `a.rs`/
  `A.rs` peers initially preserved arrival order. Entries and issues now share a
  cached total-order key (case-folded safe path, exact UTF-8 bytes, stable public ID),
  and one peer's metadata failure leaves the other intact with distinct identity.
- Gate B B-R06 passed for the intended existing refusal behavior: exceeding the
  entry ceiling, including via a catalog-only candidate, returns no partial
  `ScoutPlan` and names the configured file-count limit.
- Second Gate-B batch regressions are green: four metadata-first scout tests, nine
  bounded-discovery tests, all 80 discovery tests, and Rustfmt check pass.
- Gate B B-R07 completed compile-RED, behavioral-RED, then GREEN: native non-UTF-8
  paths no longer flow through lossy display text. They retain distinct platform-byte
  identities, remain catalog-only with `UnsupportedPathEncoding`, and perform zero
  probe reads even when their lossy renderings collide.
- Gate B B-R08 completed behavioral-RED then GREEN: environment, MCP-client, and
  launch-CWD candidates now all pass the same protected-root gate at the final binding
  boundary; rejected candidates create no source-root `.symforge` state.
- Gate B B-R09 completed behavioral-RED then GREEN: a successful reload on the same
  `LiveIndex` instance now clears its bootstrap-only `local_empty_reason` while becoming
  Ready with the freshly loaded files—no process restart required.
- Third Gate-B batch regressions are green: all 82 discovery tests and all 10
  reload-filtered library tests pass; `cargo fmt --all -- --check` exits 0.
- Gate B B-R10 completed compile-RED, local behavioral-RED, daemon behavioral-RED,
  then GREEN: only the exact direct `allow_protected_root=true` authority binds a
  protected root. Refusal leaves the session/project set unchanged; authorized local
  and daemon indexing reads the requested source while creating no source-root
  `.symforge` state.
- Gate B B-R11 completed compile-RED then GREEN: state placement is fixed on each
  `ProjectInstance`; explicit protected roots skip every project-local state attempt,
  prepare `SYMFORGE_HOME/projects/<ProjectId>`, and degrade to typed memory-only state
  when that location is unavailable. Normal roots with blocked `.symforge` state now
  follow the same user-local fallback instead of retrying a source checkpoint.
- Fourth Gate-B batch regressions are green: both protected-root contract tests pass,
  the daemon protected-root test passes, all 20 serial index-folder library tests pass,
  and `cargo fmt --all -- --check` exits 0. User-local checkpoint persistence remains
  explicitly skipped until its typed consumer routing lands in the later Gate-B slice;
  no broader state-routing green gate is claimed yet.
- Gate B B-R12 passed immediately for the intended reason: injected project-local
  permission failure moves only `StatePlacement` to the user-local directory; the
  canonical source, shared `ProjectId`, readable source bytes, and absent source-root
  `.symforge` state remain unchanged.
- Gate B B-R13 completed behavioral-RED then GREEN: alias/different-root behavior was
  already correct, while the red oracle exposed the unversioned ID format. The shared
  constructor now hashes a domain/version/platform tag plus lossless canonical native
  identity, emits `project-v1-<digest>`, preserves unpaired Windows UTF-16 units, and is
  used by discovery, daemon registry keys, state placement, and local runtime status.
- Fifth Gate-B batch regressions are green: all 85 discovery tests and all 20 serial
  index-folder library tests pass; `cargo fmt --all -- --check` exits 0.
- Gate B B-R14 completed compile-RED, three surface-level behavioral REDs, then GREEN.
  One shared repository-root operation now evaluates effective ordered root-ignore
  semantics and appends only `/.symforge/` after successful explicit-normal indexing
  or project-aware init. It preserves the BOM, every pre-existing byte, the first
  newline style, and final-newline behavior; absent files remain absent, automatic and
  explicit-protected authorities are read-only, and hash/type races fail safely.
- The guarded hygiene matrix covers empty/BOM-only, LF/CRLF, final/no-final newline,
  equivalent rooted rules, ordered negation, global/info-exclude-only evidence, a
  deterministic concurrent-byte change, and symlink/reparse refusal. This Windows host
  denied test symlink creation with OS error 1314, so the fixture also exercises the
  reparse metadata predicate directly while proving the external target stays unchanged.
- Sixth Gate-B batch regressions are green: all five hygiene adversarial tests, all 22
  serial index-folder library tests, all 27 init integration tests, and
  `cargo fmt --all -- --check` pass.
- Gate B B-R15 completed behavioral-RED then GREEN: a root `.gitignore` negation
  initially admitted `.symforge` and `.git` internals while blanket hidden filtering
  discarded `.github`/`.codex` knowledge. The shared walk now admits repository-owned
  hidden paths but prunes every `.git`/`.symforge` component before traversal,
  independent of ignore rules and active state placement. Watcher/freshen reindexing
  uses the same predicate before metadata/content I/O and evicts any stale record.
- Seventh Gate-B batch regressions are green: all 86 discovery tests, all 41 watcher
  tests, all 3 impact-admission integration tests, and Rustfmt pass. B-G07's hidden-
  knowledge inclusion is now implemented, but it remains open until ignore-pruned
  coverage is represented as required by that full task.
- Gate B B-R16 completed behavioral-RED then GREEN: an over-cap retarget already kept
  the prior root and exact published-state `Arc`, but it stopped and discarded the
  active watcher before the replacement build could succeed. Watcher shutdown now
  begins only after `reload_for_binding` publishes the replacement generation; failed
  builds leave the original handle/token untouched, and the oracle proves it continues
  indexing a post-failure file in the prior project.
- Eighth Gate-B batch regressions are green: the B-R16 oracle, all 22 serial
  index-folder library tests, all 3 watcher reload-cancellation tests, the repeated-
  `index_folder` watcher-leak integration test, and Rustfmt pass. The leak fixture's two
  stale `IndexFolderInput` initializers were updated for the required protected-root
  authority field discovered by this broader gate.
- Gate B B-R17 completed compile-RED then GREEN: a placement-derived
  `SourceExclusions` policy now converts the selected project/user-local state directory
  into a canonical repository-relative subtree when it lies beneath the source root.
  The filtered repository walk drives scouting and bulk reload, while the same policy is
  published beside the successful shared-index generation and checked by watcher and
  freshen paths before metadata/content I/O. Failed reloads cannot replace the prior
  policy, and stale watcher mutations remain generation-fenced.
- Ninth Gate-B batch regressions are green: the exact B-R17 cross-surface oracle, all 86
  discovery tests, all 42 watcher tests, all 10 reload-filtered library tests, the daemon
  user-local fallback test, and `cargo fmt --all -- --check` pass. B-G14 remains open for
  its later snapshot root-identity and verification clauses.
- Gate B B-R18 completed compile-RED then GREEN: the canonical root resolver now rejects
  Windows device/NT namespaces before filesystem probing, rejects Unix device/virtual
  namespaces and special file kinds, and repeats the same check on the canonical target.
  The production resolver and injected test seam share one decision function, so a
  canonicalization error remains a typed refusal under `allow_protected_root=true` rather
  than being promoted to a protected binding.
- Tenth Gate-B batch regressions are green: the exact B-R18 oracle, all 87 discovery
  tests, all 3 protected-root regressions, and `cargo fmt --all -- --check` pass.
- Gate B B-R19 completed compile-RED, behavioral-RED, then GREEN. Snapshot schema v5
  embeds the versioned `ProjectId`; load, overwrite, and reset validate it under the
  selected state-directory lock. Foreign, corrupt, or version-skewed bytes are copied
  exactly into the resolved quarantine directory and removed from the active slot only
  after a hash-stable re-read. The collision oracle proves two sources forced onto one
  user-local directory cannot load or overwrite each other's state.
- The widened daemon oracle exposed a Windows identity split between verbatim
  `Path::canonicalize` roots and `dunce::canonicalize` roots. The shared identity
  function now simplifies the Windows canonical spelling before case/separator folding;
  its dedicated regression and the user-local daemon checkpoint both pass.
- Gate B B-R20 completed compile-RED then GREEN. Snapshot serialization, checkpoint,
  verification load, reset, temp cleanup, and snapshot quarantine now require the
  resolved `StatePlacement`/`ProjectStateDir`; none reconstructs `<source>/.symforge`.
  Local and daemon servers retain the chosen placement across checkpoint calls and only
  publish a replacement root/placement after a successful retarget. The exact lifecycle
  oracle keeps a source-root `.symforge` blocker byte-for-byte untouched while every
  persistence artifact lands in user-local state.
- Eleventh Gate-B batch regressions are green: both exact B-R19/B-R20 oracles; all 88
  discovery tests; all 45 persistence tests; all 4 protocol checkpoint tests; the daemon
  user-local, restore, and checkpoint tests; 2 checkpoint, 3 snapshot, and 2 team-artifact
  integration tests; `cargo fmt --all -- --check`; and `cargo check --all-targets` with
  zero warnings/errors. B-G14 is complete: dynamic nested-state exclusion, root-ID
  validation, and fail-safe mismatch handling are all verified.
- Gate B B-R21 completed compile-RED then GREEN. Process-global coordination now has a
  separate typed `ControlStatePlacement`: an absolute prepared user-local directory or a
  pathless `ProcessLocal` reason. Its resolver has no source-root/launch-CWD input, rejects
  relative `SYMFORGE_HOME` before probing, and converts missing or inaccessible durable
  state into process-local coordination instead of reconstructing relative `.symforge`.
- Twelfth Gate-B batch regressions are green: the exact B-R21 oracle (including relative,
  missing, and permission-denied sequences), all 14 paths tests, all 14 sidecar descriptor
  tests, all 88 discovery tests, and Rustfmt pass; the initial filtered all-target Cargo
  run also compiled every target successfully.
- Gate B B-R22 completed compile-RED, behavioral-RED, then GREEN. The shared exporter
  initially accepted an explicit-protected binding and wrote the artifact, metadata, and
  `.gitattributes`; it now refuses protected, non-project-local, mismatched-state, and
  unavailable mutation capability before any export write. Normal project-local export
  uses the existing `git2` index/ignore APIs and reports exactly `already_tracked`,
  `untracked_visible`, `ignored_force_add_required`, or `git_visibility_unavailable`;
  `checkpoint_now` includes that exact state in its receipt. B-G17 is complete.
- The widened B-R22 regressions are green: the exact four-state/refusal oracle; all 11
  artifact-filtered library tests; all 5 checkpoint tests; both public team-artifact
  integrations; both session-cache constructor regressions; and `cargo check --all-targets`
  with no warnings. The widening also fixed the compatibility constructor to retain the
  canonical bound root rather than pairing a raw caller path with canonical state placement.
- Gate B B-R23 completed behavioral-RED then GREEN. On this Windows host, directory-link
  creation is unavailable (error 1314), so the oracle substitutes an unsafe non-directory
  state entry while independently exercising the reparse predicate. Before the fix, the
  resolver delegated that entry to preparation and reported `Other`; on link-capable hosts
  the same path would follow the link and write its marker outside the project. The shared
  state-directory seam now rejects symlinks, Windows reparse points, and non-directories as
  `InvalidData` both before and after preparation, then selects typed user-local state without
  touching the unsafe target. `.gitignore` hygiene now reuses the same reparse detector.
- Thirteenth Gate-B batch regressions are green: the exact B-R23 oracle, all 89 discovery
  tests, all 5 `.gitignore` hygiene tests, all 14 paths tests, `cargo fmt --all -- --check`,
  and `cargo check --all-targets` with no warnings or errors.
- Gate B B-R24 completed behavioral-RED then GREEN. The scout previously hard-coded
  `catalog_metadata_bytes=0` and ignored the dedicated limit. It now counts the exact
  compact canonical encoding of logical public entry/issue metadata (including array
  framing) while excluding payload bytes, absolute paths, and private timestamp/platform
  hints. The ceiling has its own environment override/default and aborts the candidate
  before a `ScoutPlan` can escape; it creates no budget `ScoutIssue` or partial manifest.
- Fourteenth Gate-B batch regressions are green: the exact B-R24 below/exact-boundary
  oracle, all 10 bounded-discovery tests, all 90 discovery tests, the watcher scout
  consumer, `cargo fmt --all -- --check`, and `cargo check --all-targets` with no warnings
  or errors. The sparse metadata-terminal fixture also proves payload size consumes zero
  admitted and catalog-metadata bytes beyond its bounded logical descriptor.
- Gate B B-R25 was GREEN as a characterization on introduction: the existing per-session
  working set already prevents another session from addressing a shared protected slot by
  global project ID, display alias, active-project selection, or `projects=["*"]`. The
  second session joins that one slot only after its own exact direct override.
- Gate B B-R26 completed behavioral-RED then GREEN. The older `open_project_session`
  path-only guard accepted the modeled protected root after session A had loaded it, so
  reconnect metadata could join the live slot without an override. Session open now uses
  the same typed raw+canonical resolver as other binding paths with automatic client-root
  authority, before any slot lookup or load. Persisted protected state remains dormant
  across close/restart; project ID, display alias, session-open metadata, and an omitted
  override cannot reactivate it. A fresh direct override can. B-G19 is complete.
- Fifteenth Gate-B batch regressions are green: both exact B-R25/B-R26 oracles, both
  sensitive-root regressions, normal same-root slot reuse, all 86 daemon tests,
  `cargo fmt --all -- --check`, and `cargo check --all-targets` with no warnings/errors.
- Gate B B-R27 completed behavioral-RED then GREEN. Same-key replay previously returned
  the stored `Indexed` receipt immediately while the requesting session had no live target
  membership. Daemon and local/embed paths now rebuild and publish the requested binding
  before returning the immutable historical receipt. A forced rebuild failure returns
  `applied=false outcome=live_postcondition_unavailable`, embeds the unchanged historical
  receipt, leaves no false membership, and does not rewrite the replay record; a later
  viable replay restores membership and returns the original receipt exactly.
- The canonical `index_folder` request hash now includes the exact canonical path, reset
  intent, and `allow_protected_root`; `add` remains the documented compatibility spelling.
  Changed path or override conflicts before any load/attachment. The shared unavailable
  formatter keeps daemon and local responses identical. B-G15 and B-G20 are complete.
- Sixteenth Gate-B batch regressions are green: the exact daemon B-R27 failure/recovery/
  conflict oracle, all 3 local protocol replay tests (including live two-file rebuild), all
  5 idempotency integrations, all 87 daemon tests, `cargo fmt --all -- --check`, and
  `cargo check --all-targets` with no warnings/errors. `src/protocol/tools.rs` remains a
  Tier-2 metadata-only file, so focused tests plus all-target compilation provide its proof.
- Gate B B-R28 completed compile-RED, behavioral-RED, then GREEN. The type-level oracle
  first failed because project health had no durability/capability fields; after adding the
  frozen `ProjectCapabilities` shape, the runnable oracle exposed the real defect by
  observing `Available` after a forced checkpoint failure. Each daemon `ProjectInstance`
  now owns one shared runtime durability signal, and every cached session server updates
  that same signal after checkpoint success/failure without changing the selected placement.
  Health derives reason-bearing snapshot/checkpoint and durable-mutation capabilities from
  the signal while keeping the published index state as the independent readiness report.
- Seventeenth Gate-B batch regressions are green: the exact B-R28 state-owner-blocker oracle
  proves unchanged project identity, canonical root, placement, generation, watcher owner,
  `Ready` state, and live file query while durability becomes
  `PersistentStateUnavailable` and curation becomes `DurableMutationReplayUnavailable`;
  all 5 checkpoint tests, all 88 daemon tests, `cargo fmt --all -- --check`, and
  `cargo check --all-targets` pass with no warnings/errors. The changed checkpoint branch in
  Tier-2 `src/protocol/tools.rs` was additionally inspected through a bounded raw window.
- Gate B B-R29 completed compile/behavioral RED then GREEN across the exhaustive typed-owner
  inventory. Project artifacts remain behind `ProjectStateDir`; daemon discovery/control,
  sidecar/session status, hook adoption, cross-project replay, version/update state, and local
  `index_folder` idempotency now consume the owning server/client `ControlStateDir` instead of
  re-resolving launch-CWD or process-global paths at call time. Reconnect keeps the same owner.
- The first serial library run exposed 11 stale/global-state failures; exact root fixes closed
  health/sidecar namespaces, updater cleanup, coupling placement, Windows path identity, and
  reset-fixture ownership. A widened rerun exposed two remaining local idempotency consumers;
  routing them through the server-owned control namespace produced the final serial result:
  2,804 passed, 0 failed, 2 intentionally ignored in 259.60s.
- B-R29's static typed-owner oracle passes. Three public integration batches covering
  admission/publication/watchers; checkpoint/idempotency/artifact/init/onboarding/API-key/
  capability state; and edit-safety/hook/sidecar behavior all exit 0 serially. Their widening
  updated stale trust/hook fixtures to assert the typed control owner and real session/daemon
  descriptor namespaces. `cargo fmt --all -- --check` and `cargo check --all-targets` exit 0.
  B-G16 and B-G21 are complete; B-R30 is the next open red-test obligation.
- Gate B B-R30 completed behavioral-RED then GREEN. The live slot already retained its
  `StatePlacement` value, but same-project `index_folder` still called the resolver before the
  slot lookup and created a newly available source `.symforge` owner. Placement resolution now
  lives exclusively inside the cold `ProjectInstance` loader closure. The oracle proves a
  user-local fallback survives same-instance reindex with zero project-local probe/write, then
  close/reopen in the same daemon constructs a fresh instance and recovers project-local state.
- B-R30 widening is green: the exact lifecycle oracle, all 89 daemon tests, the B-R29 typed-owner
  oracle, Rustfmt, and `cargo check --all-targets` pass. B-R31 is the next open obligation.
- Gate B B-R31 completed compile-RED, behavioral-RED, then GREEN. The frozen
  `FreshnessStatus`/`FreshnessReason` domain types now exist, and catalog-capacity refusal is a
  typed `ScoutCapacityError` outside `ScoutPlan`: entry exhaustion yields
  `CatalogEntryCapacityExceeded`, metadata exhaustion yields
  `CatalogMetadataCapacityExceeded`, and admitted-content exhaustion remains an independent
  non-catalog error. Every refusal returns before a partial plan/manifest or budget `ScoutIssue`
  can exist.
- B-R31 widening is green: the exact oracle passes; all 11 bounded-discovery tests, all 91
  discovery tests, and all 39 domain tests pass serially; Rustfmt and `cargo check --all-targets`
  exit 0. B-R32 is the next open red-test obligation.
- Gate B B-R32 completed compile-RED then GREEN. `RepositoryManifest` now carries the frozen
  serializable source/catalog/disposition model and hashes only canonical bounded identity.
  `ParseStatus::{Parsed, PartialParse, Failed}` strips all operational diagnostic text before
  manifest construction. Target aggregation selects only requested processors, so a
  Knowledge-only entry ignores a synthetic failed code status and derives `PartialParse` from
  its knowledge extractor. Rewording the operational warning leaves the manifest digest exact.
  This also completes B-G01's core domain-type obligation.
- B-R32 widening is green: the exact digest/target oracle and all 40 domain tests pass serially;
  Rustfmt and `cargo check --all-targets` exit 0. All Gate-B RED obligations are now closed; the
  remaining work is production scout/manifest integration and focused verification.
- Gate B GREEN is complete. Metadata-first scouting is authoritative for cold load and reload;
  admission precedes content accounting; walker/metadata failures, path collisions, unsafe or
  oversized spellings, hidden-knowledge coverage, and independent catalog capacities all retain
  typed total outcomes. Cold catalog-capacity refusal stays responsive and publishes typed
  non-ready freshness without a partial manifest.
- The fail-fast integration sweep exposed and closed legacy projection and typed-owner regressions
  in admission receipts, call-time co-change/frecency fixtures, coupling refresh, daemon runtime
  paths, startup sidecar discovery, hook routing, and worktree TEE placement. The production fix
  retained the cold-load `ProjectStateDir` on the shared handle; all other corrections made test
  readers/writers carry the same explicit owner and canonical path helpers.
- Gate B VERIFY is authoritative: `cargo test --all-targets -- --test-threads=1` exited 0 in
  824.895s across the complete library and integration surface. `cargo check --all-targets`,
  `cargo fmt --all -- --check`, and `git diff --check` also exit 0; the latter reports only the
  repository's existing LF-to-CRLF notices. SymForge impact review is current for domain,
  discovery, store, daemon/hook seams, and all touched integration fixtures.
- Gate B is complete. Gate C stable bounded reads and total execution is the next open gate.
- Gate C C-R01 completed compile-RED then GREEN. `stable_read_with_access` rejects a scout
  size above the per-file ceiling as `HardSkip(PerFileCeiling)` before invoking either access
  pass; the panic access spy proves zero allocation/read attempts. The named test passes 1/1.
- Gate C C-R02 completed behavioral-RED then GREEN. A first-pass handle/path stamp that differs
  from the scout stamp now returns `UnstableDuringRead` immediately; the spy proves exactly one
  first-pass call and zero second-pass I/O. Both stable-read oracles pass 2/2.
- Gate C C-R03 completed compile-RED then GREEN. A first-pass I/O failure is retained as the typed
  `Unreadable { stage: FullRead, kind }` outcome using the discovery layer's canonical error-kind
  classifier; the named oracle passes 1/1 and no failure is silently filtered from accounting.
- Gate C C-R04 completed compile-RED then GREEN. The deterministic production fold now retains
  every post-trip parse result as `AbortedCircuitBreaker` while keeping the triggering entry in
  the indexed set. The fixture proves 10/10 terminal dispositions, the exact three-entry tail,
  and zero accidental tail insertion; the named oracle passes 1/1.
- Gate C C-R05 completed compile-RED then GREEN. Admission now performs read, classification,
  parse, and staged hand-off in one worker closure: staged capacity is reserved before the
  transient permit releases, so resident bytes remain continuously accounted without holding a
  corpus-wide permit set. The named oracle passes 1/1; all six in-flight and nine circuit-breaker
  regressions pass serially, including the tight-budget multi-file fixture.
- Gate C C-R06 completed compile-RED then GREEN. Stable reads now require matching first/second
  lengths and hashes plus unchanged scout/handle/path stamps; the first pass must also carry bytes
  whose measured length and hash agree with its evidence. A same-stamp changed payload is rejected
  as `UnstableDuringRead`, while the matching control returns owned accepted bytes; all three
  stable-read oracles pass serially.
- Gate C C-R07 completed behavioral-RED then GREEN. A scout-admitted read whose declared size is
  within the per-file ceiling but larger than the total in-flight budget now returns the exact
  terminal `HardSkip(PerFileCeiling)` before either read pass; the panic access spy proves zero
  allocation/read attempts and the named oracle passes 1/1.
- Gate C C-R08 completed compile-RED then GREEN. Breakers now carry an exact source/lane/stage
  scope; a trip degrades only that scope, retains only its unprocessed tail as aborted, and queues
  a bounded five-attempt reconciliation repair behind `ReconciliationPending` freshness. The
  oracle independently varies source, lane, and stage and proves all unaffected scopes remain
  Complete with 10/10 indexed entries and no repair. The named oracle passes 1/1, all ten circuit-
  breaker tests pass serially, and the cold-load/reload scout publication regression remains green.
- Gate C GREEN is complete. Both scout entrypoints use the single bounded probe helper; execution
  uses one double-pass stable-read helper, preserves permits through parse/staged hand-off, folds
  code/knowledge lanes deterministically, and retains one sorted terminal disposition per scout
  entry across cold load and reload. Project reset now clears those dispositions and queued scoped
  repairs; the lifecycle oracle was observed RED (0/1) before the two-line reset fix and GREEN (1/1)
  afterward.
- Gate C focused verification is green serially: 73/73 `live_index::store::tests`, 4/4 stable-read,
  7/7 in-flight accounting, 10/10 circuit-breaker, 1/1 unreadable retention, 2/2 CRLF persistence/
  watcher, and the non-ASCII/UTF-8/source-span regressions. C-V03 is an operational bound receipt,
  not an RSS claim: permit exhaustion blocks until capacity frees, six fitting large files complete
  under a 512 KiB total budget, and an individual request above the total budget hard-skips before
  either read pass.
- Gate C VERIFY is authoritative after the reset fix: `cargo test -j 1 --all-targets --
  --test-threads=1` exited 0 in 810.927s. `cargo check -j 1 --all-targets` exited 0 in 58.99s,
  `cargo fmt --all -- --check` exited 0, and `git diff --check` exited 0 with only the repository's
  existing LF-to-CRLF notices. A prior unrestricted-parallel all-target compile exhausted compiler
  memory; verification was deliberately serialized with `-j 1`, and both a 949.409s pre-fix full
  baseline and the final post-fix full run exited cleanly.
- Gate C is complete. Gate D watcher and reconciliation convergence is the next open gate.
- Gate D D-R01 completed behavioral-RED then GREEN. A sparse `weights.gguf` created after cold
  load was initially dropped by extension filtering (0/1). Watcher/freshen paths now treat language
  inference only as a target hint, route the path through the cold-load single-path scout, and
  publish `HardSkip(PerFileCeiling)` under the project-generation writer fence before any whole-file
  read. The named oracle passes 1/1 and the complete watcher unit module passes 43/43 serially.
- Gate D D-R02 completed behavioral-RED then GREEN. The strengthened catalog-only-to-code fixture
  observed two publication generations for one watcher update because indexed content and stale
  catalog cleanup were committed separately. `publish_indexed_file_at_generation` now applies the
  content/derived update, catalog cleanup, and indexed terminal disposition under one project-
  generation fence and calls the publication boundary once. The named oracle passes 1/1 and the
  complete watcher unit module passes 44/44 serially.
- Gate D D-R03 completed behavioral-RED then GREEN. Tier-1-only reconciliation returned zero for
  two missed creates. Reconciliation now builds a fresh metadata scout with the active source
  exclusions, processes paths absent from the prior scout plan, and publishes the refreshed plan
  under the project-generation fence. The oracle proves a new Markdown file is indexed, a new
  plain-text file retains a terminal outcome pending Gate F, and both remain authoritative Knowledge
  targets. The named oracle passes 1/1 and the watcher unit module passes 45/45 serially.
- Gate D D-R04 completed behavioral-RED then GREEN. The initial fresh rescout still ignored changed
  and deleted catalog-only entries and returned zero repairs. Reconciliation now diffs complete
  scouted entries, re-observes every changed disposition/stamp, removes paths missing from the fresh
  manifest, and retains the old Tier-1 sweep only as a bounded fallback when rescout itself fails.
  Generation-fenced removal now clears indexed content, derived indices, the legacy catalog
  projection, and terminal disposition in one publication. The named oracle passes 1/1 and the
  watcher unit module passes 46/46 serially.
- Gate D D-R05 completed behavioral-RED then GREEN. A watcher registered after a missed create/delete
  reached Active but performed no reconciliation within the bounded startup window. Every successful
  watcher registration now runs an immediate full-manifest reconciliation before consuming queued
  incremental hints; periodic and overflow paths share the same cause-aware accounting helper. The
  named fresh-instance/overflow oracle passes 1/1, watcher units pass 47/47, and the directly affected
  `watcher_layer3_restat` and `watcher_reload_cancellation` integrations each pass 3/3 serially.
- Gate D D-R06 completed behavioral-RED then GREEN. Cross-project generation fences already preserved
  content, derived indices, catalog state, the scout plan, terminal dispositions, freshness, and the
  publication generation, but the cause-accounting wrapper still stamped `last_reconcile_at` for the
  active project after the foreign batch was rejected. Accounting now requires an initially matching
  effective generation and an unchanged generation after the sweep. The named all-lane oracle passes
  1/1 and the watcher unit module passes 48/48 serially.
- Gate D D-R07 completed deterministic behavioral-RED then GREEN. Reconciliation paused after its
  off-lock scout, a watcher published an 82-byte replacement, and the old implementation then
  overwrote the manifest with the stale 24-byte entry. Incremental publications now update their
  scouted entry under the writer boundary, off-lock file builds fence and retry against the captured
  publication generation, removals compare the exact scouted base, and reconciliation rebases only
  paths changed since its baseline before publishing canonical coverage/accounting. The RED failed
  exactly at 24 vs 82 bytes; the named oracle passes 1/1 and all watcher units pass 49/49 serially.
- Gate D D-R08 completed behavioral-RED then GREEN. Three manifest observations carried identical
  entries while coverage progressed Degraded, Degraded, Complete; the old cause wrapper stopped
  after the first observation. Reconciliation now retries Degraded coverage up to five attempts with
  capped 50/100/200/400 ms backoff, rechecking cancellation and active project generation between
  attempts, while equal Complete coverage remains a one-attempt no-op. The RED observed 1 attempt
  instead of 3; the named convergence oracle passes 1/1 and all watcher units pass 50/50 serially.
- Gate D D-R09 completed compile-RED, behavioral-RED, then GREEN. The watcher lacked a shared stable-
  read seam; once exposed, injected full-read refusal proved the existing path returned an error while
  leaving coverage Complete. Watcher reads now use the bulk pipeline's double-pass stable reader;
  `Unreadable` and `UnstableDuringRead` publish Degraded coverage, transient terminal paths bypass
  equal-entry manifest no-op, and a stable hash-skip restores the canonical Indexed disposition. Both
  transient variants converge without any repository change. The two named oracles pass 1/1 each and
  all watcher units pass 52/52 serially.
- Gate D D-R10 passed immediately for the intended post-D-R08 reason. A first uncertainty window
  exhausts exactly five Degraded attempts and remains explicitly Degraded; a later overflow opens a
  fresh bounded window, observes Degraded then Complete, and records the overflow once. Retry state is
  invocation-scoped rather than a permanent settled latch. The named oracle passes 1/1 and all watcher
  units pass 53/53 serially.
- Gate D GREEN is complete. Single-file observation now enters through shared scout/admission/stable-
  read policy; indexed content, derived indices, scout state, manifest disposition, coverage, and
  publication accounting commit under generation/publication fences. Reconciliation diffs the complete
  manifest, rebases paths changed after its off-lock build, and retries or aborts stale work instead of
  overwriting newer watcher publications.
- D-G08 removed stored `skipped_files` state and direct skip mutations. `manifest_entries` is the sole
  disposition authority; compatibility skip responses and tier lookup/counts are projections. Direct
  `update_file`/`remove_file` mutations now publish and clear the corresponding canonical entry, and
  upsert deduplicates by normalized repository path rather than host-specific catalog identity.
- The final mutation-invariant verification caught and closed three additional real regressions: atomic
  rename batches now coalesce by normalized path and process existing destinations before vanished temp
  hints; transient unreadable/unstable observations retain last-valid bytes while the manifest remains
  authoritatively Degraded; and structural-edit NUL fixtures pin disk mtime only when testing byte-exact
  splice preservation, without changing the normal disk-refreshed authority contract.
- Gate D focused verification is green serially: 74/74 store units, 53/53 watcher units, 146/146 query
  units, 4/4 health-view units, 5/5 admission acceptance, 3/3 impact admission, 3/3 layer-3 restat, and
  3/3 reload-cancellation. The OS watcher integration passed three consecutive runs (10 passed, 1
  ignored each), including the formerly nondeterministic rename-replace case.
- Gate D VERIFY is authoritative: `cargo fmt --all -- --check` exited 0; `cargo check -j 1 --tests`
  exited 0 in 68.489s; and `cargo test -j 1 --all-targets -- --test-threads=1` exited 0 in 930.497s
  (`job_019f84910ba37c53921fa1b3672a3e19`). The final tail includes watcher integration 10/10 with
  one ignored, layer-3 restat 3/3, reload cancellation 3/3, worktree awareness 27/27, and xref 13/13.
  `analyze_file_impact` refreshed every indexed touched file; the 1.2 MB `src/protocol/tools.rs` correctly
  returned the existing typed Tier-2 metadata-only limitation, so exact reads plus compiled behavioral
  tests are the evidence for that file.
- Gate D is complete. Gate E snapshot fidelity and one-Arc publication is the next open gate; no Gate E
  implementation began before the final Gate D all-target receipt.
- The scoped Fable Gate-D review reopened the gate with four HIGH findings sharing two root causes.
  The preserved report is `specs/020-repository-knowledge-index/fable-gate-d-review-2026-07-21.md`.
  Metadata scout failures now retain an authoritative `Unavailable { Metadata }` entry, and reconcile
  removals require Complete fresh coverage. Candidate/live manifest dispositions, not only the scout
  plan, keep coverage Degraded through unreadable, unstable-read, and circuit-breaker states until a
  stable replacement lands.
- Circuit-breaker aborts now re-enter bounded reconciliation, failed rescouts preserve a Degraded retry
  signal across the cause wrapper, and the unconsumed duplicate repair queue was removed. The focused
  failure oracles cover retained missing entries, unchanged Complete no-publication, metadata-before-read
  refusal, and real notify-error routing.
- `analyze_file_impact` now normalizes one repository-relative path before lookup/upsert and routes new
  or edited files through the watcher's shared metadata/admission/stable-read publication seam. The new
  regressions prove `./src/lib.rs` cannot mint a duplicate identity and gitignored supported sources
  cannot bypass admission.
- Reopened Gate D focused verification is green: discovery metadata-first 7/7, watcher units 59/59,
  store units 74/74, admission acceptance 5/5, impact admission 5/5, watcher integration 10/10 with one
  ignored, layer-3 restat 3/3, and reload cancellation 3/3. The full library target passed 2,839 with
  zero failures and two ignored in 221.49s.
- The fresh authoritative `cargo test -j 1 --all-targets -- --test-threads=1` receipt exited 0 in
  1,016.033s (`job_019f84f7119d7032b53cc03b8294fb32`): 3,555 passed, 0 failed, 10 ignored across 107
  test-result summaries. `cargo fmt --all -- --check` and `git diff --check` both exited 0; the latter
  reported only the repository's existing LF-to-CRLF notices.
- The review's disputed Gate-E E-R01 oracle was re-stressed on the later Gate-H tree: the exact
  snapshot round-trip test passed 25 consecutive fail-fast process-isolated runs
  (`job_019f86a4732b7c72b44a00fa9ab01ea3`), then the complete persistence module passed five
  consecutive serial runs (`job_019f86a7350e7490a23351d6531cb6e1`). Both loops exited 0, so the
  pre-remediation one-off failure is not reproducible in the current tree.

### Review

The scoped Fable delta report returned PASS and READY TO FREEZE. All five MEDIUM
corrections held, no regression or new blocker exists, and its one LOW definition gap
is closed locally. Gate A is complete; implementation may proceed from Gate B while
the full goal remains open through Gates B-M, final review, commit, and push.

Gate D was reopened by the scoped Fable review and is re-closed after behavioral
RED/GREEN fixes for every sustained finding and missing oracle. Focused suites, the
2,839-test library target, formatting/diff checks, and the fresh 1,016.033-second
all-target run are green with zero failures. No Gate D blocker remains; Gate E may
resume under the frozen publication/snapshot contract.

### Gate E Evidence Log

- Gate E RED/GREEN is complete. The ten named oracles cover snapshot catalog fidelity,
  shared admission, project and publication fences, atomic reader capture, failed reload
  retention, degraded last-valid publication, verifying readiness, same-path replacement,
  source-version coherence, and watcher/verifier races.
- Snapshot schema v7 now persists one canonical manifest, stable repository/source identity,
  exact source version and closed working-tree state, resident-content and Git-history
  fingerprints, and code-signal provenance including computed content generation, source
  version, and history coverage. Restore rebuilds resident indices and reconstructs the same
  immutable code-signal snapshot instead of restamping generation zero.
- One `ArcSwap<PublishedSourceSet>` is the externally observable root. Its current source maps
  to one immutable core `PublishedGeneration` containing live content, health, outline,
  freshness, manifest, source version, and code signals. Direct reloads build replacements,
  while mtime-only and derived-only publications retain the content generation.
- Strong lineage validation now gates load, overwrite, reset, and team-artifact import.
  Same-path foreign repositories cannot inherit, delete, or overwrite prior state; mismatched
  snapshots and artifacts leave their active path and are quarantined. Ordinary Git drift is
  accepted for overwrite only while the stored anchor remains in the live object database.
- Additional boundary REDs were preserved: foreign reset failed before the lineage gate
  (`job_019f85717ea27d52bb0f78bb53ff6685`), code-signal provenance restored as generation zero
  (`job_019f8571a5b87982a70a936557ed9464`), and foreign team artifacts were refused but left
  active (`job_019f8576274f71908f4394dd225b1148`). Each exact oracle is now green.
- Focused verification is green serially: persistence 58/58, store 81/81, and the complete
  `live_index` namespace 568/568. The full library receipt
  `job_019f857c51b97e10a44887eaa9789cba` passed 2,858 with zero failures and two ignored in
  216.86 seconds.
- The non-test boundary is green: `cargo check -j 1 --tests` exited 0 in 51.05 seconds
  (`job_019f8583bd6f7582baaab814f61f3459`). Checkpoint 2/2, live-index integration 31/31 with
  one ignored performance case, and team-artifact 2/2 passed under
  `job_019f8584bba576e3871043a5b62eb81f`.
- `cargo fmt --all -- --check` and `git diff --check` both exited 0. The diff check emitted only
  the repository's existing Windows LF-to-CRLF advisories.

### Gate E Review

Gate E is complete. Every frozen RED, GREEN, and VERIFY item is closed with serial behavioral
evidence, and no snapshot/publication blocker remains. Gate F knowledge-target extraction is
the next open implementation gate.

### Gate F Plan

- [x] Preserve F-R01–F-R16 as behavioral REDs across routing, extraction, sensitive admission,
  byte/format matrices, and cold/watch/reconcile/verifier parity.
- [x] Make manifest `IndexTargets` authoritative for code/knowledge overlap while keeping all
  existing code search, symbols, references, and frecency code-only.
- [x] Add generic UTF-8 text extraction and project existing Markdown sections with exact
  byte/line/content provenance; add no duplicate persisted unit store.
- [x] Add versioned high-precision path/content detection and one whole-hit output guard shared
  by direct, CCR, diagnostics, and analytics boundaries.
- [x] Verify parser/search scope, existing code/config parsers, byte-exact fixtures, snapshots,
  and runtime-canary containment before marking Gate F complete.

### Gate F Evidence Log

- Added authoritative overlapping `IndexTargets`, `LanguageId::Text`, generic strict-UTF-8/BOM
  extraction, and on-demand Markdown section projection without a duplicate persisted content store.
  Code discovery remains code-only; prose is knowledge-only; config/schema formats retain both lanes.
- Added one versioned, compile-once byte detector and shared whole-hit guard. Sensitive paths are
  terminal before content I/O, detector failures fail closed, LFS pointers and invalid UTF-8 remain
  catalog-only, and only safe rule identifiers/counts reach metadata.
- Cold load, watcher, reconciliation, background verification, snapshot recovery, and team-artifact
  import now share the same admission policy and reject a mismatched detector-policy version.
- Focused serial suites are green: knowledge 7/7 (`job_019f85cd229a77319cdcdb5440579bc6`),
  Markdown 18/18 (`job_019f85cdaf4f74e295ab80149e2504ab`), metadata scout 9/9
  (`job_019f85cdb7fb7c519ae0afc8f0bd8db3`), search 64/64
  (`job_019f85cdc0ba76908938f9986f881159`), analytics 7/7
  (`job_019f85cdfa057e62b6434c4a40de366b`), domain 41/41, config extractors 91/91,
  and existing language parsers 153/153.
- Boundary suites are green: store 86/86 (`job_019f85d25b3c7043a954c242d764f759`), persistence
  61/61 (`job_019f85d2bde97e91bbd0d209edd1b775`), and watcher 61/61
  (`job_019f85d3138b7be2a24a3458a12e48e1`). The full serial library gate passed 2,883 with
  zero failures and two ignored (`job_019f85dd56e074b0ba844e4ed75727a9`).
- External checks are green: live-index integration 31/31 with one ignored performance case
  (`job_019f85e121f470b2a5ad3665cd097d9b`), checkpoint 2/2
  (`job_019f85e2530273138bc94cab47411b01`), and team-artifact 2/2
  (`job_019f85e2821171e1b1135070cc983e4e`). `cargo check -j 1 --tests`, formatting, and the
  unchanged `Cargo.toml`/`Cargo.lock` dependency boundary all exit zero.
- Runtime-generated canaries are absent from serialized snapshots and analytics, and detector-positive
  hits are withheld whole from direct and CCR-visible fields. Failure oracles compare only safe
  booleans/counts and never interpolate the canary.
- The broad gate exposed one stale pre-F oracle: `.gitignore` is now intentionally indexed as generic
  text. Its corrected assertion proves `.gitignore` remains visible while the rule still excludes
  `ignored.rs`; the exact regression passes under `job_019f85dc9b407e4092ee383c2830014d`.

### Gate F Review

Gate F is complete. All 16 RED, nine GREEN, and four VERIFY items are closed with serial behavioral
evidence. No parser dependency was added, no sensitive-value surface remains known, and all ingestion
paths produce the same knowledge disposition. Gate G evidence-bridge core is the next open gate.

### Gate G Plan

- [x] Preserve G-R01–G-R08 as exact behavioral REDs for closed-world candidate extraction,
  ambiguity/missing state, same-source identity, ownership provenance, budgets, and stale builds.
- [x] Add compact bridge domain types with canonical ordering, stable source-local link IDs, bounded
  samples/selectors/metadata, and index-based forward/reverse storage without copied document bodies.
- [x] Extract candidates only from internal links, exact repository paths, code-spanned unique symbol
  names, versioned structured values, and declared ownership selectors; reject bare prose, external
  links, contributor history, and all unsafe/nonresident knowledge.
- [x] Build against one captured source/content generation and extend `PublishedGeneration` with the
  immutable bridge. Rebuild watcher/reconcile mutations before their single publication and reject
  stale off-lock bridge results at the existing publication fence.
- [x] Verify deterministic repeated builds, exact bidirectional repair on create/change/rename/remove,
  independent truncation coverage, cross-source isolation, concurrent readers, code-scope isolation,
  and frecency neutrality before marking Gate G complete.

### Gate G Evidence Log

- Added compact source-local anchors, exact/declared-set/ambiguous/missing resolutions, stable link IDs,
  compact reverse indices, and canonical derived-coverage breaches. Bridge state stores no copied prose.
- Closed-world extraction accepts internal repository links, exact path/code-span selectors, one versioned
  structured-path rule, and CODEOWNERS selectors. Markdown and bare external URLs, bare symbol prose,
  contributor text, sensitive/nonresident files, and cross-source anchors create no code edge.
- `PublishedGeneration` now owns one immutable bridge beside live, manifest, outline, health, and temporal
  state. Watcher and reconciliation tests prove forward/reverse repair shares the same publication; stale
  off-lock builds fail their publication fence and pinned callers retain their captured generation.
- Contract-audit REDs caught and corrected missing `.md` links routed into the code lane, per-link rather
  than global sample ceilings, bare external URL suffixes, and zero-based evidence/code-anchor line ranges.
- The first store run exposed eager materialization of every symbol in a code-only 144,000-symbol fixture.
  Bridge construction now extracts selectors first and builds anchors only for referenced names; the exact
  stress case completes in 12.10 seconds (`job_019f86152fc779b0be9d015cb2ab4154`).
- Focused serial suites are green: bridge 12/12 (`job_019f8618a58c7ba19f7e4b601f303cb3`), watcher
  62/62 (`job_019f8610a2e077d3b6d0f4357a422f0c`), store 86/86
  (`job_019f8616c4117fc1ab54c59f0a796c35`), and persistence 61/61
  (`job_019f861849d97752a7bfb30e2f1437fa`).
- The complete serial library gate passed 2,896 with zero failures and two ignored in 267.19 seconds
  (`job_019f8618d8277f63ba2efbbc1199abe4`). Concurrent reader stress observes one source/content generation,
  repeated builds are equal and canonically ordered, and direct bridge construction invokes no frecency path.
- Final hygiene passed: `cargo fmt --check` (`job_019f861e4ad07701a34d485495f0936b`),
  `cargo check -j 1 --tests` (`job_019f861e6e0f7601843c5ba35f008ab4`), the 31-pass/one-ignored
  live-index integration suite (`job_019f861fad25766086e5480201e0f9c2`), the 2/2 checkpoint suite
  (`job_019f8620e4927560ba889fd1a082f28e`), and `git diff --check`
  (`job_019f862129637d32bca47db939173eb8`). `Cargo.toml` and `Cargo.lock` remain unchanged.

### Gate G Review

Gate G is complete. All eight RED, six GREEN, and three VERIFY items are closed with behavioral evidence.
The bridge is derived from resident safe bytes, rebuilt rather than persisted, atomically published, bounded,
source-local, and deterministic. Gate H knowledge-authority foundations are the next open gate.

### Gate H Plan

- [x] Preserve H-R01–H-R15 as exact behavioral REDs covering independent authority axes, mixed units,
  deterministic evidence precedence, temporal provenance, policy failure/staleness, budgets, and source-tip fences.
- [x] Add one source-local authority derivation module with the frozen lifecycle/domain/evidence/voice model,
  stable finding/provenance IDs, canonical coverage, and no copied document bodies.
- [x] Parse exactly one versioned `.symforge-knowledge.toml` ledger; require exact safe paths, whole-file hashes,
  and optional zero-based half-open unit ranges/hashes. Malformed, unsupported, conflicting, cyclic, or stale
  entries remain findings and cannot suppress or authorize curation.
- [x] Derive typed records from admitted knowledge units plus the captured bridge and bounded temporal evidence;
  only supported current-implementation claims may be code-diverged, while intent/decision/governance divergence
  remains an implementation gap and all timestamp-only evidence stays advisory.
- [x] Extend `PublishedGeneration` with immutable authority state and one off-lock prepared publication seam fenced
  by publication/content/project generations plus exact source version. Reject stale completions and retain one
  coalesced latest target without advancing content identity for accepted derived-only publications.
- [x] Verify cold/watcher/reconcile/verification parity, deterministic repeats, per-unit isolation, fail-open raw
  retrieval, fail-closed suppression, independent budgets, temporal convergence, full focused suites, and broad
  serial regression gates before closing Gate H.

### Gate H Evidence Log

- The authority type/rule/parser/build/publication TDD chain was observed red before implementation and then green:
  type contract (`job_019f862949b17022860b1c5244230db6` -> `job_019f862b8f077571b11be7ce9a8072dd`),
  evidence rules (`job_019f862d337f7c039c59b10a7ed9c006` -> `job_019f862f176f75119ef60c8d30699dd7`),
  policy parser (`job_019f86307fc870729664a999324c159d` -> `job_019f8632292579529ec46182676e4f91`),
  authority derivation (`job_019f86349ea37342a8ae104352ef6cd5` -> `job_019f863bf2ee70b2a224c5991ecafb36`),
  and immutable publication (`job_019f863dbad67423a56c66cf9cba489b` -> `job_019f8640e3eb74f2a0a65e3ca98dbff1`).
- `knowledge_authority.rs` now carries independent lifecycle/domain/evidence/voice axes, stable finding IDs,
  exact display precedence, a closed versioned policy parser, exact whole-file/unit hash validation, cycle/native-conflict
  refusal, unit isolation, explicit budgets, and fail-closed skipped-suppression IDs. The final focused authority suite is
  14/14 (`job_019f8663eaa379c0819db1397f4aa32e`).
- Git temporal publication now fences project generation, content generation, source identity, and exact source version.
  One running request plus one replaceable latest marker bounds watcher bursts; stale completions self-schedule the latest
  same-source target. Project/content/source-tip rejection plus bytes-identical-commit convergence and bounded coverage
  are green 5/5 (`job_019f865f198c72638b8c0c2819959ab2`), and the temporal unit suite is green 41/41
  (`job_019f8663cc9979a2b7f3738b54e3dba2`).
- Cold load, direct watcher admission, reconciliation, and background verification compare identical authority semantics
  after normalizing only their intentionally different receipt generations (`job_019f8653e96d7372a36dc405f67026d5`).
  Watcher publication carries bridge/authority/source/version in one root; watcher is green 62/62
  (`job_019f866228de796097176003dbcb5940`), store 86/86 (`job_019f866362717cf38e263390c189c536`),
  and persistence 62/62 (`job_019f8663005a7893aad3462090c6196c`).
- Snapshot restore rebuilds unpersisted authority with current authority-rule, policy, and secret-policy versions before
  becoming Ready. Temporal coverage explicitly reports bounded-window, rename-follow, shallow/working-tree/unavailable,
  dirty, divergent, and clock-skew limitations without upgrading clocks to proof.
- The first complete serial run found one invalid legacy oracle after 2,911 passes: it required publication generation to
  remain fixed while background derived-only work was allowed to publish. The oracle now fences content/project/source/
  version/manifest identity instead, passed three consecutive focused runs, and the lesson is recorded in
  `tasks/lessons.md`.
- Final gates: formatting (`job_019f866fba7a79218772d582166a3df3`), `cargo check -j 1 --tests`
  (`job_019f866407a57f539893ecb36100938d`), and the complete serial library suite: 2,912 passed, zero failed,
  two ignored (`job_019f866fe21874e0b54aaee19da24fe9`). `Cargo.toml` and `Cargo.lock` remain unchanged.

### Gate H Review

Gate H is complete. All fifteen RED, nine GREEN, and three VERIFY contracts are covered by executable evidence. Authority
is derived from the resident knowledge/bridge generation, rebuilt rather than persisted, atomically published with exact
source/version provenance, bounded without hiding raw safe units, and incapable of silently treating intent, governance,
operations, or history as current implementation. Gate I `search_knowledge` is the next open gate.

### Gate I Plan

- [x] Preserve I-R01–I-R16 as executable REDs before implementation: exact eight-field schema and annotations;
  captured-generation hit/envelope provenance; the five no-match classes; typed readiness, selector, scope, CCR,
  recovery, and budget failures; deterministic ranking; whole-hit security withholding; and frecency neutrality.
- [x] Add the exact `SearchKnowledgeInput` and current-only advertised source scope, while keeping the full surface
  increment to one tool and the compact surface exactly three tools.
- [x] Reuse resident knowledge units, authority records, bridge previews, source envelopes, project selection, and CCR;
  capture one immutable published source set per selected project before extraction and never reload during formatting.
- [x] Implement deterministic query interpretation and ranking in the frozen order: exact phrase, heading/title,
  distinct significant terms, source precedence, then canonical path/line ties. Keep authority filtering independent
  from source precedence and keep knowledge intent from stealing code/symbol questions.
- [x] Enforce the security pipeline before routing and after extraction: reject unsafe queries without echo/state,
  construct only guarded `SafeHit` values, withhold unsafe candidates whole, and preserve guarded provenance through
  token budgeting and CCR retrieval.
- [x] Format exact source/version/publication/content generation, content identity, 1-based half-open lines,
  authority display, stable finding/provenance IDs, bounded bridge previews, per-source and worst coverage/freshness,
  filtered/withheld/overflow counts, and deterministic empty-result reasons from only the captured source bundles.
- [x] Wire `ask` and the compact facade only after direct-tool mapping tests are green; preserve successful no-match
  responses, facade CCR redemption, cross-project isolation/order, repeat-cache generation identity, and zero frecency.
- [x] Run I-V01–I-V03 plus focused protocol/live-index/daemon suites and formatting/checks; defer the campaign-wide
  complete serial all-target regression gate to the final Gate-M/release verification so it covers the finished tree.

### Gate I Evidence Log

- Exact schema/surface coverage is green: `schema_roundtrip` 27/27, conformance 19/19, and compact-surface
  invariants 4/4. The full surface gained exactly `search_knowledge`; compact remains `symforge`, `symforge_edit`,
  and `status`, with current as the only advertised Gate-I source scope.
- Public search coverage is green: 11/11 integration tests with one explicit manual corpus test ignored in the normal
  suite (`job_019f87108b7c7871a857e260c4db2dae`), plus 49/49 knowledge-focused library tests. The suite covers all five
  no-match classes, validation/readiness/degraded states, deterministic ranking, exact BOM/CRLF/multibyte line ranges,
  captured-generation coherence, stable authority IDs, source-envelope withholding, bridge previews, CCR policy and
  stale retrieval, current-only scope, and generation-aware deep reads.
- A bridge-preview RED exposed an exact-ID join between line-level bridge anchors and enclosing knowledge-unit anchors.
  Search now joins only same-source/same-path/same-content anchors contained by the unit range; the exact regression and
  full public suite are green.
- Compact routing is green end to end: knowledge intent precedes find fusion, successful empty results remain successful,
  max-token caps reach `search_knowledge`, and CCR footers redeem through the three-tool `symforge` facade without a
  fourth tool. Direct and `ask` search remain frecency-neutral.
- Daemon I-R11 is green for ID and unique-name `project`, ID/name `projects`, session-scoped wildcard expansion,
  canonical envelope order, selector conflict/unknown errors, and zero third-session hit/bridge leakage
  (`job_019f870442ee719090a03d6481e0bf45`). Selectors are canonicalized through the existing session resolver before
  any immutable source-set capture.
- Real-repository acceptance is reproducible through the ignored named corpus test. All eight frozen queries returned a
  non-acceptance-fixture pointer in one `limit=3`, `max_tokens=2500` call, repeated byte-identically with captured source
  version/freshness/coverage (`job_019f870f64e4708394c8c4bfabd6e7d1`). Returned-token estimates were
  665/766/793/950/968/979/1065/1165 (median 959). The conservative direct-read-only baseline for the selected source
  files was 1353/2151/2674/2674/3755/4293/9967/14775 (median 3214.5), so the measured reduction is 70.2% before adding
  any broad-discovery cost; no direct read was needed.
- Existing code discovery remained isolated: `search_symbols` 30/30, `search_text` 52/52, and `find_references` 46/46.
  `cargo check -j 1 --tests` and final `cargo fmt --all -- --check` both exited 0.

### Gate I Review

Gate I is complete. `search_knowledge` is a bounded, read-only full-surface tool over one caller-captured immutable
generation per selected project. It reuses the resident corpus, authority, bridge, project, safety, and CCR machinery;
adds no embeddings or duplicate index; cannot serve mixed generations; preserves all successful empty-result classes;
and leaves code discovery plus compact-3 behavior unchanged. Gate J repository-map and `review_knowledge` work is next.

### Gate J Plan

- [x] Preserve J-R01–J-R11 as executable REDs: bounded source-captured role/map output; knowledge-only/default/empty/
  bundle context behavior and backlink caps; generation-aware context cache invalidation; exact source-local review modes,
  IDs, evidence arrays, bridge records, temporal provenance, blockers, and proposals; complete-plan hashes; selector
  isolation; frecency neutrality; capture coherence; contributor/missing-role honesty; budget exhaustion; and secret safety.
- [x] Derive fixed v1 role cards only from declared spans, versioned exact heading rules, and path conventions, retaining
  missing roles and coverage rather than generating or persisting a summary.
- [x] Extend `get_repo_map`, orientation `ask`, `get_file_context`, and `get_symbol_context` from one captured generation;
  preserve existing default/empty/bundle contracts, exact backlink cap five, code commitment behavior, and compact budgets.
- [x] Add exact read-only `review_knowledge` schema, annotations, summary/document/remediation modes, canonical complete-plan
  per-source/top-level hashes, bounded source-local dossiers, and daemon ID/name/list/wildcard routing without mutation.
- [x] Add fixed prompt `symforge-knowledge-hygiene` with review/evidence/proposal/approval/preview ordering and no embedded
  approval, mutation, or deletion authority.
- [x] Run role/map/context/review/hash/publication/frecency/security/selector suites, one-call real-repository orientation and
  deep review, repeatability/no-mutation checks, format/check, and focused regressions before closing Gate J.

### Gate J Evidence Log

- The full public surface is pinned at 38 tools and compact remains exactly three. `search_knowledge` and
  `review_knowledge` are present in the registered surface, canonical conformance inventory, annotations, and every
  generated full-surface client allow-list. The fail-first allow-list oracle and the complete 55-test init module are green.
- Role cards expose fixed role IDs, exact unit/evidence anchors, content hashes, source/content generations, voice,
  overflow, missing roles, hygiene, and uncertainty. Ambiguous backlinks retain typed candidate counts/samples and exact
  bounded evidence rather than becoming guessed links.
- File and symbol contexts capture one publication for code plus knowledge. Omitted, empty, knowledge-only, default,
  bundle, repeat-cache, and tight-budget modes are covered; budget truncation now preserves an indivisible
  source/count/coverage block and never emits a partial backlink.
- `review_knowledge` provides summary/document/remediation modes, complete-plan per-source/top-level hashes, exact
  source-local dossiers, eligibility/blocker evidence, and canonical multi-project routing. A later publication cannot
  alter a review already captured from the prior generation.
- Secret-positive search/review inputs reject before analytics or CCR state. Map/search/review/ask remain frecency-neutral,
  and read-only review leaves both document bytes and repository policy absent/unchanged.
- Real-repository Gate-J acceptance repeated orientation and remediation review byte-identically with exact role anchors,
  source-version fields, hashes, paths, dossiers, uncertainty, and coverage (`job_019f87aa94c9719086bff4dbc0b25208`).
- Verification is green: public knowledge integration 22 passed/2 manual corpus tests ignored
  (`job_019f87bd66a7736388352d7e12fb433f`); frecency 23/23; schema 27/27; conformance 19/19;
  init 55/55; surface profile 7/7; cross-project daemon review 1/1; focused mental-model/review/prompt/capture suites;
  `cargo check -j 1 --all-targets`; and `cargo fmt --all -- --check`.

### Gate J Review

Gate J is complete. Repository orientation and read-only knowledge review are now bounded, source-local, generation-coherent,
deterministic, secret-safe, and non-mutating. The implementation adds no generated/persisted summary and no duplicate
index; it composes the resident Gate-H authority/bridge generation with existing topology, project routing, CCR, and prompt
surfaces. Gate K guarded logical curation is the next open gate.

### Gate K Plan

- [x] Preserve K-R01–K-R20 and K-G01–K-G07 as executable REDs before implementation: preview/apply equivalence;
  same-source replay/conflict; preview side-effect freedom; availability-before-probe; fresh review/manifest/policy/target
  guards; per-project serialization; crash recovery at every intent/write/result boundary; watcher/publication convergence;
  branch/commit/rewrite continuity; non-Git continuity; secret safety; and repository-byte exclusivity.
- [x] Define one canonical curation request and result surface. Requests name nonempty review action IDs, carry the complete
  source-local review hash plus manifest/policy/target guards, default to preview, and require an idempotency key only for
  apply. Results expose the canonical ledger diff and typed replay/publication state without document mutation authority.
- [x] Reproduce selected actions from a fresh captured review and render one deterministic `.symforge-knowledge.toml` image;
  preview must run every logical guard and return the exact apply diff while creating no lock, probe, journal, temp, or policy
  artifact. Canonical serialization must round-trip through the frozen policy parser without widening the authority model.
- [x] Add a project-bound curation coordinator with explicit source-access and persistence capabilities. Apply must reject an
  ineligible binding before any durability probe, then probe only the policy-ledger parent and durable intent parent, acquire
  one per-project policy lock, and revalidate the complete review/manifest/policy/target envelope under that lock.
- [x] Implement durable same-source idempotency and recovery under the project-state directory: identity is repository/source
  identity plus a resolvable stored Git tip (or durable non-Git root/catalog lineage), never mutable refs, current tip, policy,
  manifest, or target digests. Persist reserved/pending/result states with pre/post images and fail closed on foreign or
  unverifiable continuity, key/hash conflict, third-state policy bytes, or non-durable storage.
- [x] Commit only `.symforge-knowledge.toml` through create-new same-directory temp, complete write/verification, sync,
  atomic replacement, parent durability (or a documented/tested Windows-safe equivalent), then durable result. Recovery must
  converge every injected crash point exactly once; ordinary watcher publication remains the sole live-index update path.
- [x] Register `curate_knowledge` across the full tool surface, schema/annotations, daemon routing, generated client
  allow-lists, documentation, and exact surface counts while compact remains three. Run focused Gate-K suites, crash and
  concurrency batteries, real-repository no-document-mutation acceptance, format/check, and all touched regressions before
  closing the gate.

### Gate K Evidence Log

- 2026-07-22 direct durability/continuity audit closed the three open items with strict RED → GREEN:
  - Temp digest verification: `corrupted_temp_policy_image_fails_digest_verification_before_replace`
    first failed by persisting an injected corrupt temp image as `status=applied`; `write_policy` now
    verifies the temp read-back with `crate::hash::digest_hex` after flush, before `sync_all`/replace.
    The same verification was added to `durable_replace_io` (review finding: the PendingWrite recovery
    path and every replay/lineage/quarantine write shared the unverified-temp exposure).
  - Live pre-image fencing: `live_third_state_policy_before_write_is_fenced_not_overwritten` first
    failed by overwriting an independent third-state policy written between the durable `PendingWrite`
    record and the policy write; `apply` now re-reads under the mutation lock immediately before the
    write, accepts exact pre-image, finalizes exact post-image, and durably terminalizes any third
    state as `indeterminate_conflict`.
  - Pre-lock replay fast path: `foreign_record_quarantine_waits_for_the_mutation_lock` first failed by
    quarantining a foreign record while an external holder owned `policy.lock`; the pre-lock fast path
    is now strictly read-only (`verify_binding` gained a no-append mode; quarantine and non-Git lineage
    appends happen only under the in-process + file mutation lock). K-R02 preserved: clean-binding
    same-key/same-hash terminal replay still returns the stored result without taking the lock.
  - Review-sustained HIGH found and fixed during the audit: with the read-only fast path, a foreign
    same-key `PendingWrite` record was quarantined by `recover_pending_records` and the flow then fell
    through to a fresh reservation that applied into the replacement repository. The locked path now
    verifies the current key's record binding before pending-record recovery, restoring fail-closed
    foreign handling (regression caught by
    `pending_intent_under_same_path_replacement_is_quarantined_without_writing`).
- Closing receipts (all post-corrections, repo-pinned cargo, `-j1 --test-threads=1`): curation module
  17/17; `curate_knowledge` 9/9; `conformance` 20/20; `surface_default` 5/5; `daemon_aliases` 2/2;
  `init_integration` 27/27; `recovery` 4/4; `cargo fmt --all -- --check` clean;
  `cargo check -j1 --all-targets` exit 0; full serial library suite 2,953 passed / 0 failed / 2 ignored.
- Known open branch debt (NOT Gate-K scope, blocks the Gate-M clippy gate): `cargo clippy --all-targets
  -- -D warnings` reports 24 pre-existing errors across `watcher`, `discovery`, `live_index`,
  `knowledge`, `protocol` (none introduced by the Gate-K audit corrections).

### Gate K Review

Gate K is complete. A scoped adversarial pass over K-R01–K-R20/K-G01–K-G07 sustained two findings —
the recovery-path unverified temp image (fixed at the shared `durable_replace_io` root) and the
foreign same-key PendingWrite fall-through to fresh apply (fixed by pre-recovery binding verification
under the lock) — and both fixes are covered by tests. Preview writes nothing; apply is durable,
idempotent, fenced against live third-state edits, and mutation of attributable state (quarantine,
catalog lineage) occurs only under the per-project mutation lock. `tasks.md` Gate-K RED/GREEN/VERIFY
items are checked. Gate L is the next open gate; the 24 pre-existing clippy errors are logged above
for the Gate-M battery.

### Gate L Progress (OPEN — in progress, updated 2026-07-23)

Gate L is the largest gate in the feature (worktrees + local refs, real git2 blob ingestion,
dedup, multi-source query composition, per-lane concurrency fences). It is being built as
verifiable RED→GREEN increments; the gate stays OPEN until every L-R/L-G/L-V item is green.

Landed so far (new module `src/live_index/local_ref_scout.rs`, wired into `live_index/mod.rs`):

- Increment 1 — `scout_local_ref` (L-G02 core), commit `fd9bdeb`. Bounded in-process DFS over a
  local ref tree via libgit2 (no Git/LFS subprocess, L-R05). Enumerates blobs keyed by immutable
  object ID; per-path classification/language re-derivation (L-R02/L-R14). Blob size comes from the
  ODB header (`Odb::read_header`), so a blob over the per-blob budget is `CatalogOnly` and its bytes
  are never read (L-R04). Entry budget degrades coverage instead of collecting unbounded (L-R07
  shape). Missing ref → typed error.
- Increment 2 — `materialize_ingest_blobs` + `RefBlobBytes` (L-G03 raw-bytes layer), commit
  `60ebddf`. Reads each distinct ingest-decision object ID once (dedup); never materializes
  catalog-only blobs.
- Increment 3 — `route_ref_blob` + `RefBlobIngest` (L-G04), commit `70ea535`. Routes a materialized
  blob's bytes through the EXACT shared adapters filesystem ingestion uses: `IndexTargets::for_path`
  (lane routing), `knowledge::classify_stable_content` (secret/LFS/encoding gate), and
  `parsing::process_file_with_classification` (the one parser) → `IndexedFile`. No second parser or
  search index. Secret-positive/LFS/undecodable bytes are withheld as metadata-only (no card). This
  is the L-R10 parity mechanism: same bytes → same lifecycle/extraction/secret result because it is
  literally the same functions.
- Increment 4 — `LiveIndex::from_source_files` (`store.rs`) + `build_ref_source_index`
  (`local_ref_scout`), L-G05 foundation, commit `15b1569`. Assembles a queryable, ROOT-LESS
  `LiveIndex` from the routed files (no filesystem walk / gitignore / coupling); reverse + path
  indices rebuilt so it answers symbol/reference/text queries like a disk-loaded index. Catalog-only
  and secret-withheld blobs contribute no file.
- Module suite: 11 unit tests green (`cargo test --lib live_index::local_ref_scout::tests`,
  `-j1 --test-threads=1`); fmt clean; `cargo check --all-targets` passes; no new clippy warnings in
  the module.

- Increment 5 — `SharedIndexHandle::build_ref_source_generation` + `publish_ref_source` /
  `remove_ref_source` (`store.rs`) + `ingest_and_publish_local_ref` (`local_ref_scout`), L-G07,
  commit `86d9813`. Wraps the ref-source `LiveIndex` in a full `PublishedGeneration` (GitRef
  `SourceIdentity`, `SourceVersion working_tree=NotApplicable`, per-source manifest/bridge/authority
  via the current-lane builders, Pending temporal) and reconciles it into the instance's
  `PublishedSourceSet` under `write_mutex` (copy map, replace only the ref lane, bump
  `registry_generation`, swap once). A P1 add/remove leaves the current lane's publication/content/
  project generations untouched (L-R12/L-R13); the current lane can never be removed. Tests: reconcile
  registry/lane-isolation + end-to-end scout→publish makes a queryable GitRef lane.
- Increment 6 — `search_scoped` + `select_scoped_sources` (`knowledge_search.rs`), L-G06 (search),
  commit `70f59e5`. `search_knowledge` composes `current`/`worktrees`/`local_refs`/`all` from ONE
  captured `PublishedSourceSet`, reusing the frozen single-source formatter per selected lane
  (current first for `all`; ranks ahead of a divergent ref but never hides it). `current` is
  byte-identical to before; an empty P1 scope returns typed `no_sources_in_scope`, never a false
  complete-absence. Search advertises all four scopes (`AdvertisedSearchKnowledgeSourceScope`);
  review still advertises current-only. Also fixed a PRE-EXISTING (Gate-K) strict-client schema
  break (`unit_byte_range` nullable union array → non-nullable `[u32;2]` via schemars).

Verification receipts (this session, all green): full serial lib suite 2967/0/2; module suite 14
tests; `curate_knowledge` 9/9; `conformance` 20/20; `search_knowledge` 22/22; `surface_default` 5/5;
`strict_client_schema_compat` 1/1; `watcher_integration` 10/10; `recovery` 4/4; `idempotency` 5/5;
`init_integration` 27/27; `daemon_aliases` 2/2; `capability_status_integration` 8/8; `cargo fmt
--check` clean; **`cargo clippy --all-targets --features server -- -D warnings` exit 0** (the 24
pre-existing branch-wide clippy errors + 3 test-code lints are CLEARED, commit `dc0e0f2`).

Also fixed two PRE-EXISTING failures this session (both fail identically at `eaaf867`, before this
session; the handover receipts never ran these suites): the strict-client schema union-array break
(above), and `watcher_integration::test_watcher_ignores_non_source_files` — a stale test that assumed
`.txt`/`.csv` are non-source; under the repository knowledge index they are first-class knowledge
sources and are indexed, so the test now uses binary content (metadata-only). Commit `38c19b8`.

STATUS: The ref ingestion + publication ENGINE (L-G02/03/04/05/07) and search-side query composition
(L-G06 for `search_knowledge`) are DONE and tested. STILL OPEN before Gate L can close:
1. review_knowledge multi-source parity (L-G06 "per-source review envelopes") — review still
   current-only.
2. Dedicated RED tests still missing: L-R01 (current outranks divergent ref, focused), L-R03 (ref
   movement invalidates old mappings — `remove_ref_source` exists but no topology driver/test; it
   carries `#[allow(dead_code)]` until wired), L-R06 (mixed-freshness all-source envelope), L-R08/
   L-R11 (protected/identity-boundary focused).
3. L-G01/L-G05 remainder: checked-out linked worktrees as separate `ProjectInstance`s (today only
   local-ref P1 lanes on the owning instance are wired).
4. VERIFY L-V01..L-V04, then check the genuinely-green Gate-L boxes in `tasks.md`.

Do NOT check any Gate-L box in `tasks.md` until its item is genuinely green; the boxes there remain
unchecked. Nothing pushed.

### Gate L adversarial review — rounds 1 & 2 (Cursor, 2026-07-23)

Two independent adversarial passes (Cursor, briefs `CURSOR-REVIEW-PROMPT.md` /
`CURSOR-REVIEW-PROMPT-2.md`). Round 1: 1 BLOCKER + 3 HIGH + 1 LOW. All fixed and
re-reviewed (round 2) except one residual HIGH that round 2 itself surfaced in the
first fix; that too is now fixed. Evidence:

- **BLOCKER (L-R13/L-G07)** — the three P0 publish paths
  (`swap_and_publish_with_content_change_and_hook`, `publish_prepared_bridge`,
  `publish_prepared_authority`) rebuilt `PublishedSourceSet.sources` as a fresh
  single-entry map, wiping every published P1 ref lane on any P0 commit. Fixed with
  `PublishedSourceSet::next_after_current_publish` (clone map, replace only the
  current lane, drop a stale prior-current lane on identity change, bump
  `registry_generation`). Test `p0_publishes_preserve_published_ref_lanes`.
- **HIGH (L-R07)** — degraded ref scout published `CoverageStatus::Complete`.
  `build_ref_source_generation` now takes `scout_coverage`; `ingest_and_publish_local_ref`
  maps `RefScoutCoverage`. Test `degraded_scout_publishes_degraded_ref_manifest_coverage`.
- **HIGH (L-R06)** — multi-source `search_scoped`/`review_scoped` lacked the top-level
  envelope. Added per-source working-tree/freshness/coverage/digest + `worst_source_coverage`
  (worst included source) + secret-policy version. Covered by the composition tests.
- **HIGH (L-R02/L-R10/L-R14/L-G03)** — identical blob re-parsed per path. `route_catalog_files`
  memoizes the parse; round 2 caught that the key must also carry the path-selected
  grammar flavor (`.ts`/`.tsx`, `.c`/`.h`), so the key is
  `(object_id, classification, language, is_tsx, is_c_header)`. Tests
  `identical_blob_is_parsed_once_across_same_classification_paths` +
  `identical_blob_reparsed_per_path_selected_grammar_flavor`.
- **LOW** — stale `review_knowledge` scope comment in `search_tools.rs`. Fixed.

Gate: `fmt --check` OK, `clippy --all-targets --features server -D warnings` clean, full
lib suite **2975/0/2** (2026-07-23). ACCEPTED LIMITATION (round-2 MEDIUM, deferred): the
composed top-level response carries L-R06's named fields but not cross-source SUM
aggregates (overflow/withheld/authority-filtered totals, a single scope-level no-match
class) — those are present per-source inside each `search_current` section; aggregating
them needs `search_current` to return structured counts rather than a formatted string
(a larger refactor), so it is logged here rather than bodged. Still nothing pushed.

### Gate L closure — engine + daemon + review round 3 (2026-07-23)

Built the remaining Gate L engine + daemon and closed a third, methodology-led review
round (Cursor + Kimi). Commits (all on `feat/repository-knowledge-index`, not pushed):
`130f57b` engine (worktree classifier `checked_out_worktrees` + reconcile driver
`reconcile_local_ref_topology`); `5773fd9` L-R11 membership test; `f174ea9` gated production
reconcile caller `spawn_local_ref_reconcile` (OFF by default via `SYMFORGE_LOCAL_REF_LANES`,
L-V02/L-V04) + L-R08 test; `346097f` review fixes A/C/D/E/F/G/H + main-HEAD fail-closed;
`4eb0b68` cross-project `source_scope` (B) + L-R11 tool-dispatch coverage.

Review methodology lesson (saved to agentmemory): a generic "review adversarially" brief
gets only the obvious HIGHs; an 8-point forcing methodology (read full bodies; trace shared
helpers' arg usage; parity diffs; concrete named-file inputs; explicit interleavings;
consumer-trace before severity; attack-the-tests; fail-open audit) made Cursor v2 escalate
the `.env` secret-parity gap to BLOCKER and find the fail-open + vacuous test its v1 missed.
Baked into `specs/020-repository-knowledge-index/GATE-L-REVIEW.md`.

Findings fixed this cycle: A [L-R10] ref admission now applies `sensitive_path_rule` (secret
parity — `.env`/`.ssh`/`*.tfstate` withheld by path, was a real gap); B [L-R09/L-G06]
cross-project search/review honor `source_scope`; C [L-R03] reconcile deletion resilient to
publish failures; D [L-R06] meaningful monotonic P1 generations; E [L-G01] worktree + main
HEAD fail-CLOSED; F single-flight reconcile; H [L-R08] isolation through query composition.
Cleared by both reviewers: P0/P1 locking, L-V04, L-R11 chokepoint, prefix/foreign-repo,
L-R04, parse-once. Full lib suite 2995/0/2; clippy `-D warnings` clean; fmt clean.

Gate L boxes: 20/22 checked in `tasks.md`. Two honest remainders (NOT bodged): L-R05
(offline ingestion proven + libgit2-only path is structural, but a runtime process-spawn
SPY assertion is not built) and L-V02 (default path inert by construction — gate OFF — but a
formal latency/memory BENCHMARK is not run; one env read per reload, Cursor LOW #8).

Accepted LOW limitations (documented, not fixed): env read on default reload path (Cursor
#8); idempotent re-publish bumps `registry_generation` per pass (no production consumer —
tests only); ref-lane manifest omits metadata-only/catalog-only entries (G, no contract
mandate); symlink blobs indexed as link-text and submodules skipped (minor filesystem-parity
notes); `Repository::open` (not discover) means subdir-of-repo projects get no ref lanes.
Still nothing pushed.

### Gate M evidence + measured limitations (M-014, 2026-07-24)

Commits (branch `feat/repository-knowledge-index`, not pushed): `883d997` AAP blockers;
`1f52606` M-001/M-002 health fields + assertions; `9ee4df0` M-001 review fixes +
M-003/005/006/007; `18a237d` M-015.

- **AAP** SF-AAP-001/002/003 CLOSED: edit_plan literal-path precedence FIXED;
  get_file_content concurrency already-satisfied (regression coverage); analyze_file_impact
  non-parser false-absence FIXED (narrowed to `LanguageId::from_extension` so oversized parser
  files keep their refusal).
- **M-001/M-002** health knowledge section (full + compact) + 6 assertions. Cursor-reviewed;
  5 findings resolved in code (authorization now derived from `MemoryOnly.failures` — no
  SourceAccessMode plumbing; gitignore-hygiene field; live_postcondition token; target mix
  rendering; unbound closed-set). Accepted-as-shipped: retry via freshness reason-codes +
  reconcile-repairs counter (data-model excludes a retry counter from the digest); bridge
  "version" proxied by content_generation (bridge has no version field); in-flight = configured
  ceiling not live usage (usage not retained past cold load — out of "surface existing data" scope).
- **M-003** full surface == exactly 39 (== tool_definitions() minus the `symforge` facade),
  compact == 3; prompts/resources/catalog cited. **M-005** secret scan CLEAN (954 files, 0
  credential-file hits; scanner exposes rule_id+count only). **M-006** policy-mismatch re-scout
  + no-canary covered; the logs clause holds by construction (sensitive bytes never resident).
  **M-007** memory-only checkpoint = typed persistence_unavailable + applied=false. **M-015**
  no leaked workers/worktrees. **M-008/009/010** fmt/check/clippy green throughout (lib 3012/0/4).

CLOSED with the final Gate-M commit: M-011 serial all-target suite (113 binaries, 0 failed; also
a push-CI gate); M-012 embedded-mode ("embed") gate GREEN (embed --lib 1282/0 — watcher/protocol
paths gated behind `feature="server"`, embed = principled no-watcher mode); M-013 feature-wide
adversarial pass (Cursor Gate-M/AAP review; accepted blockers resolved). REMAINING MEASURED
LIMITATION: M-004 corpus ≥50%-token-reduction is measurement-only — the A019 token oracles are
contentious, so this is a documented measurement rather than a hard-enforced gate. Also (surfaced
by the Kimi K3 Gate-M review): `clippy --no-default-features --features embed --all-targets` is RED
because ~65 `tests/*.rs` integration binaries unconditionally reference server-only modules
(`sidecar`/`watcher`/`daemon`/`protocol`); embed is exercised via `--lib` (the M-012 gate, green
1283/0). A file-level `#![cfg(feature="server")]` guard on those test binaries would close it —
tracked, not done.

---

## Windows Headless Process Invariant (2026-07-16)

### Plan

- [ ] Reproduce and trace every SymForge child-process path used by Codex/subagents.
- [ ] Add a failing Windows regression oracle that inventories spawn call sites and
  proves the shared command policy prevents a visible console window.
- [ ] Route every production spawn through the shared no-window constructor/policy.
- [ ] Run focused Windows process tests, relevant daemon/sidecar/hook suites, and the
  full verification gate appropriate to the touched code.
- [ ] Verify no test/helper/worker descendants remain after completion.

### Evidence Log

- User report: SymForge-related terminal windows still appeared and stole focus when
  Codex subagents used SymForge. Subagent activity was stopped before diagnosis.

### Review

Pending root-cause proof, RED/GREEN evidence, and process-tree verification.

---

## Hook Stale-Descriptor Scan Fix (2026-07-28)

### Plan

- [x] Verify the report against the shared descriptor selector and all callers.
- [x] Isolate the fix on `fix/hook-stale-descriptor-scan` from `origin/main`.
- [x] Add a regression proving dead-PID descriptors are removed before any socket probe.
- [x] Bound viable-descriptor probing while preserving deterministic live/newest selection.
- [x] Run focused tests, formatting, compile checks, and the relevant hook/sidecar suites.

### Evidence Log

- Source-verified root cause: both hook endpoint resolution and verbose status route through
  `select_descriptor_status`; it probes every project-compatible descriptor serially.
- Scope decision: one-file fix in `src/sidecar/port_file.rs`; no new dependency or protocol.
- RED: `test_reader_removes_dead_pid_descriptors_before_socket_probe` failed because a reachable
  recycled port revived 32 impossible PIDs.
- GREEN: the regression now exercises 200 dead-PID descriptors, requires zero socket probes,
  requires opportunistic cleanup, and enforces a sub-300 ms selector bound.
- Final focused gates: descriptor 15/15, hook 66/66, sidecar 113/113, `cargo fmt --check`,
  `cargo check --all-targets`, and clippy with warnings denied all passed.
- A debug-binary hook run against a temporary copy of the live descriptor set completed in
  370/44/45 ms (three exits 0); the scratch copy was removed and live state was untouched.
- Full serial library gate: 3043 passed, 1 failed, 4 ignored. The sole failure,
  `test_index_folder_rebinds_repo_root_for_local_impact_analysis`, is order-dependent and outside
  this seam: it passed alone (1/1), with all `test_index_folder_*` tests (7/7), and with the whole
  `protocol::tools` module (442/442).

### Review

The hook timeout defect is closed at the shared selector: dead PIDs are rejected from filenames
before JSON or TCP work, stale records are removed, viable candidates are probed newest-first with
a fixed total budget, and the first live candidate returns immediately. A full endpoint-identity
handshake remains separate protocol work; this fix closes dead-PID/port-reuse selection but does
not claim to eliminate the narrower live-PID-reuse race.
