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
