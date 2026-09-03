# Production-Readiness Multispectrum Diagnosis — SymForge v11.0.13

**Date:** 2026-09-03 · **Baseline:** `main` @ `6188c5af` (clean tree) · **Mode:** diagnosis only — every phase produces findings artifacts, not fixes. Remediation is planned in Phase 8 from evidence.

**Scale of the patient:** 237k LOC across 193 `src/*.rs` files; 60k LOC across 153 `tests/*.rs` files; 45 ops scripts (`scripts/*.cjs`, `execution/*.py`); npm wrapper; vendored tree-sitter-scss patch. Crate lints: `unsafe_code = deny`, `warnings = deny`.

## How SymForge itself is used (dogfood intelligence)

The MCP tool surface is the primary instrument. This session's MCP mounts are absent, so each phase that needs it first brings up the repo's own build:

```powershell
cargo build --release            # once; target/release/symforge.exe
target/release/symforge serve    # operator server on 127.0.0.1, or use daemon
```

Then query via MCP tools (`symforge_status`, `symforge_get_repo_map`, `symforge_search_symbols`, `symforge_find_references`, `symforge_get_symbol`, `symforge_detect_impact`, `symforge_health`, `symforge_search_text`, `symforge_get_file_context`). A stale index is a false-finding generator: before any analysis pass, confirm `symforge_status` reports the repo indexed at a generation covering `HEAD`, else `symforge_index_folder` first. Record index stats (files/symbols/loaded-ms) in the phase artifact — they are also Phase 4 data.

Known-intelligence caveat (from `docs/reviews/STRESS-TEST-v11-mcp-surface-2026-08-24.md`): `search_text` is blind to markdown/RST (P0-1) and several tools ignore `project=` (P0-2). For text sweeps use the harness `grep` tool or `git grep`, not symforge `search_text`. Use symforge for symbol-graph questions (references, dead code, impact, repo map), where it is the strong instrument.

## Reporting discipline (applies to every phase)

- Findings go to `docs/reviews/REVIEW-FINDINGS-readiness-<spectrum>-2026-09-03.md` — one file per spectrum.
- Every finding: `file:line` at rev `6188c5af`, labeled **proven** (reproduced/measured), **likely** (strong static evidence), or **speculative** (needs the named experiment).
- Comments aren't behaviour; existence isn't invocation. No finding without evidence.
- Pre-existing known-open items are inputs, not rediscoveries — the plan names them; verify status, don't re-litigate.

---

## Phase 0 — Baseline and gate battery

**Purpose:** prove the starting point is green and measure baseline timings every later phase compares against.

1. `cargo fmt --check`, `git diff --check` — expect clean (CI gates this).
2. `cargo clippy --all-targets -- -D warnings` — record wall time.
3. `cargo test --lib --bins --tests -- --test-threads=1` — the canonical serial correctness gate (CI runs exactly this; parallel is a known hazard by design). Record pass count and wall time.
4. `cargo build --release`; `cargo check --no-default-features --features embed`; `npm test` in `npm/`.
5. `cargo bench --bench observed_refresh_gate_v1 -- --test` — bench smoke.
6. Clone phase0 corpora per `tests/fixtures/phase0-corpus/README.md` so `stel_golden_replay` does not skip vacuously (CI does this; local runs silently skip without it).

**Artifact:** `docs/reviews/REVIEW-FINDINGS-readiness-baseline-2026-09-03.md` — table of gate, command, wall time, pass/fail, machine. Any red gate at baseline is a **proven** P0 finding and halts later phases.

## Phase 1 — Correctness spectrum

**Purpose:** what the test suite actually proves, and where it lies, skips, or is known-broken.

1. **Ignored-test disposition** — classify all ~20 `#[ignore]` sites into: deliberate Slice-0 RED controls, perf/calibration smokes, platform-gated, vacuous. Named targets:
   - `tests/watcher_layer3_restat.rs:155` — `transient_av_lock_does_not_remove_file` is `#[ignore]`d with an **empty body**: a vacuous test wearing a Windows-AV-lock justification. Proven slop; decide: implement behind Windows-only gate or delete.
   - `src/daemon.rs:10300` — Slice-0 control whose attribute says "remove in Slice 2 (T030-T040)"; verify whether Slice 2 shipped and the attribute was never removed.
   - Slice-0 controls in `tests/project_index_lifecycle_slice0.rs` marked CODE-WRONG / CONTROL-STALE (lines 109, 156, 576, 674, 777, 858, 922) each name an **open production residual**: no `EmptyBootstrap` gate in `add_file` (`live_index/store.rs:2820-2831`), `IsolatedCandidate` never routed in `store.rs:2403-2436`, same-path root replacement silently adopted, `SnapshotStore` verify-state wiring, capacity not process-bounded, watcher still started on activate (`daemon.rs:3398-3403`). Verify each residual against current code; each confirmed one is a **proven** hardening finding carried to Phase 3/8.
2. **Vacuous-skip hunt** — find tests that skip silently when fixtures are absent (phase0 corpora pattern): `grep -l "skip" tests/` then verify each skip path prints loudly or fails in CI. CI clones corpora; local doesn't — confirm no other fixture class has this gap.
3. **Parallel-hazard catalog** — the serial gate exists because of process-global mutation. Catalog the machinery: two independent `CwdGuard` implementations (`src/daemon.rs:7200`, `src/protocol/tools.rs:14629`) and ~15 distinct `*_ENV_LOCK` statics with `#[allow(unsafe_code)]` env guards. Known live flake: `protocol::tools::tests::test_index_folder_rebinds_repo_root_for_local_impact_analysis` fails under default parallel threads (recorded 2026-08-03, serial is canonical). Finding: measure how much of the suite is unsafe to run parallel — the count of env/CWD-mutating tests vs total; that ratio is the cost of ever wanting parallel CI.
4. **Coverage map** — no coverage tooling is configured anywhere (no tarpaulin/llvm-cov config found). Run `cargo llvm-cov` (install if needed) once over the serial suite, module-level granularity. Output: per-module coverage table; modules under ~50% line coverage with production traffic (protocol/tools, daemon, watcher, live_index/store) are hardening targets.
5. **Weekly-only gates** — `performance-smoke` CI job runs only on schedule/dispatch. List every test that only runs there (coupling calibration, graph BFS p95, team-artifact round-trip, latency smokes in `tests/sidecar_integration.rs:267,513`). Run them once locally to confirm they still pass; record results.

## Phase 2 — Code-slop spectrum (symforge-driven)

**Purpose:** find dead weight, god objects, and structural rot using the symbol graph.

1. **God-file decomposition analysis** — LOC-ranked: `src/protocol/tools.rs` (34,965 — 15% of the entire codebase in one file), `src/daemon.rs` (16,484), `src/live_index/store.rs` (9,260), `src/sidecar/handlers.rs` (7,608), `src/protocol/format.rs` (6,999), `src/protocol/edit.rs` (6,689). For each: `symforge_get_file_context(sections=["outline"])` + symbol census; identify natural seam clusters (tools.rs already has `protocol/edit_tools.rs`, `protocol/knowledge_curation.rs` siblings — the split precedent exists). Deliverable: per-file proposed module split map with symbol counts. This is analysis only; splitting is remediation.
2. **Dead-code sweep** — for every `pub`/`pub(crate)` symbol in files ranked by Phase 2.1 plus `src/stel/`, `src/stel_core/`, `src/capability/`: `symforge_find_references`; zero-reference exported symbols are candidates. Cross-check against `cbm-spike` feature code (`src/parsing/resolver/` — declared "no consumer yet" in Cargo.toml:58-63) and `__test-internals` door. Label: dead code reachable only from tests is **likely** slop, not proven — tests may be the contract.
3. **Panic-path audit** — measured: ~228 `.expect(` + 44 `.unwrap()` + 6 `panic!` in non-test code. Worst files: `src/index_lifecycle/activation.rs` (36 expects), `index_lifecycle/runtime.rs` (27), `live_index/coupling/store.rs` (21), `cli/init.rs` (14 + 1 `unreachable!`), `index_lifecycle/registry.rs` (11), `index_lifecycle/capacity.rs` (10). Classify each site: **invariant** (panic acceptable, document why) vs **recoverable** (must become `Result`). Any panic reachable from an MCP tool call, hook invocation, or watcher event is a proven finding — a poisoned request must not kill the daemon. Method: `symforge_find_references` on the enclosing function, trace to a request/hook boundary.
4. **Suppression audit** — 23 `#[allow]`/`#[expect]` attributes outside `unsafe_code` ones; list each with justification comment; flag any without a reason.
5. **Debug-output classification** — 122 `println!`/`eprintln!` in non-test src. `cli/` output is legitimate; any in `daemon.rs`, `watcher/`, `live_index/`, `protocol/`, `sidecar/` that bypass `tracing` is a finding (operator logs can't filter them).
6. **Debt markers** — 45 TODO/FIXME/HACK/XXX in prod code; list, age via `git blame`, attach to Phase 8 backlog.

## Phase 3 — Hardening spectrum

**Purpose:** trust boundaries, process lifecycle, resource limits — where the daemon dies or lies in production.

1. **Unsafe audit** — crate denies `unsafe_code`; exactly these production sites carry `#[allow(unsafe_code)]`: `src/cli/update.rs` (5: Windows ToolHelp enumeration, image-path identity, `TerminateProcess`, `CloseHandle`, `MoveFileEx` replace), `src/daemon.rs` (4: SIGKILL, `kill(pid,0)` liveness, `/proc` owner read, `terminate_process`), `src/sidecar/port_file.rs` (2: `process_may_be_alive` both platforms), `src/protocol/knowledge_curation.rs:2076` (Windows write-through replace), `src/path_shadow.rs:549-565` (process-global `PATH` mutation). Verify each SAFETY comment against the code; `path_shadow.rs` mutating process-global `PATH` in production code paths gets special scrutiny (who calls it, under what synchronization — the test-side hazards of env mutation are documented; production side is not).
2. **Cold-start deadline margin** — operator-facing, measured 2026-08-03 and unfixed: `ADMIN_SERVE_START_DEADLINE` (60s) covers index load + bind; a cold monorepo scan measured 32.7s main / up to 49.5s on this machine with phase0 corpora present. Reproduce against the current index (`.symforge/index.bin` 28.7MB, coupling.db 50.8MB); measure `symforge serve` cold vs warm start; the finding is the margin curve, not the deadline value.
3. **Error taxonomy at boundaries** — for each MCP tool, hook subcommand, and sidecar handler: what reaches the wire on failure. Hooks are deliberately fail-open (`src/cli/hook.rs:226`); verify fail-open never extends to *edit* paths (edit safety must fail-closed). `edit_safety/trust` + raw-read admission gate (spec 023) get a boundary walk.
4. **Watcher robustness** — evidence in STRESS-TEST §P2-10: one multi-project session recorded `repairs: 2839` against `events: 714` with `overflows: 0`, index generation 577. Determine whether `index_folder(add:true)` storms the home watcher (correctness smell) or repairs are misattributed (telemetry lie). Repro corpus is documented in STRESS-TEST §11.
5. **Process lifecycle** — daemon guarded-start uniqueness, owner-checked cleanup, session reaper TTL, self-update binary swap (`cli/update.rs` Windows image-path identity check). Adversarial review of pid-reuse windows: `process_is_alive` + owner match vs pid recycling between check and kill.
6. **Capacity/backpressure** — `SYMFORGE_MAX_INDEX_FILES` is per-discovery-pass, not process-wide (Slice-0 residual 2.5); `MAX_INFLIGHT_BYTES_ENV`; coupling VACUUM policy. Confirm what actually bounds a runaway index in production.

## Phase 4 — Performance spectrum

1. **Cold start** (from Phase 3.2 data): index load 1,944ms warm per STRESS-TEST §13 health excerpt vs ~33-49s cold scan; quantify the snapshot-restore path (spec 026) vs full rescan; `index.bin.zst` team-artifact path.
2. **Bench gate** — `benches/observed_refresh_gate_v1.rs` is the frozen registration; run the full criterion bench (not just `--test` smoke) and archive results; compare against any recorded baseline in `docs/reviews/OBSERVED-REFRESH-GATE-v1.md`.
3. **Calibration smokes** — run the `#[ignore]`d perf suite once: coupling calibration, graph-BFS p95, team-artifact round-trip, FRSH-02 reparse latency, `test_load_perf_1000_files`, Gate I/J knowledge acceptance. Record numbers; anything regressed vs its stated bound is proven.
4. **Token economy** — full 39-tool surface ≈ 21.4k schema tokens (STRESS-TEST §Review); `search_knowledge` ~7× slower than code search and provenance-heavy (P1-1). Re-measure schema token cost per surface profile (`full` vs `SYMFORGE_SURFACE=compact`); this is a production-adoption gate, not micro-optimization.

## Phase 5 — Security spectrum

1. `cargo audit` + `cargo deny check` (neither configured — flag as gap); manually review the two intentional version-comment debts: `rmcp` "3.1" resolving ≥1.7 via lockfile (Cargo.toml:84-88 REVIEW P3-C) and the `serde_yml`→`serde_yaml_ng` alias.
2. **Vendored grammar** — `vendor/tree-sitter-scss` patch (MSVC flag fix); verify the diff vs upstream crates.io 1.0.0 is exactly the build.rs change and nothing else.
3. **Auth surface** — `api-keys.db`, bearer-key resolution (`server/serve.rs` `resolve_api_key` inline-vs-env precedence), loopback-only defaults, DNS-rebinding `allowed_hosts` behavior (rmcp ≥1.7 dependency).
4. **Trust model** — `.symforge` project-config trust evaluation (`cli/trust.rs`, `edit_safety/trust`): confirm untrusted config can't steer edits; trust-status command output vs actual enforcement.
5. **Secrets hygiene** — scan repo for committed secrets (`git grep -i` for token/secret patterns + `.env.example` is template-only); confirm hooks fail-open path never logs payload content to `hook-adoption.log` (1.1MB local log exists — check what it captures).

## Phase 6 — Supply chain & release integrity

1. Verify release gates locally: `python execution/refreeze_v11.py verify-internal --target-ref HEAD`, lifecycle traceability trio (`scripts/validate-lifecycle-oracle-traceability*.cjs`), `python execution/version_sync.py check`, `python -m unittest discover -s execution`.
2. Release-please health: `.github/.release-please-manifest.json` vs latest tag `v11.0.13`; the known race (release PR not opening after merge+branch-delete) documented in `docs/backlog.md` — confirm current run state on GitHub.
3. npm wrapper: `npm test`, platform package matrix (`npm/platforms/*`), packed artifact contents (`npm pack --dry-run` — the repo has a stray `symforge-4.9.8.tgz` at npm/ root; confirm it's not published/shipped).
4. CI runner hygiene: `execution/free_runner_disk.sh`, LF-index census, rmcp single-major assertion — confirm each still has teeth (e.g. census would actually fail on a CRLF blob).

## Phase 7 — Documentation live-truth

Docs are testimony; code is gospel. Verify, then repair or archive:

1. `docs/OUTSTANDING-WORK.md` + `docs/backlog.md` — both July 2026, pre-dating specs 020-028 closure. Every claim gets resolved/superseded/stale with a commit or test pointer (the stashed tree shows someone already started this deletion work — the stash `wip-feat-030-before-clean-pull-2026-09-02` contains deletions of both files plus handoff/review docs; review that stash before rewriting anything).
2. `tasks/todo.md` — 178KB append-only session log (sections from v8 architecture review through 2026-08-24 stress eval). Not a task list. Recommend: archive to `docs/archive/`, replace with pointer. Owner decision required.
3. **Spec ledger drift** — 8 spec dirs lack `tasks.md` entirely (012, 019, 022, 023, 024, 026, 027, 029); specs with unchecked boxes: 020 (140), 015 (106), 021 (75), 016 (60), 013 (53), 011 (41), 003 (25), 008 (21), 005 (17), 009 (10), 004 (8), 010 (6), plus 1 each in 002/018/025. Reconcile: mark done-with-evidence, explicitly deferred, or carry to backlog. Unchecked-box drift is how "done" claims rot.
4. `CONTEXT.md` / `CLAUDE.md` / `README.md` claims vs measured reality — especially any token-savings claim (STRESS-TEST §3 warns the 90% figure mixes baselines; honest numbers: 24.8% paired-session mean, per-call varies).

## Phase 8 — Triage and remediation routing

1. Consolidate all phase findings into `docs/reviews/REVIEW-FINDINGS-readiness-rollup-2026-09-03.md`: ranked by production risk (daemon-killing panic > wrong answer > silent staleness > perf > hygiene), each with proven/likely/speculative label, file:line @ 6188c5af, and remediation size estimate.
2. Route: one-line/wording fixes → direct PR; boundary/behavior changes → speckit spec (existing repo pattern, specs/0NN-*); god-file splits → one spec per file, tools.rs first.
3. Every remediation PR must pass the Phase-0 battery and add the targeted regression test; merges follow the release-please double-count guard (`gh pr merge N --merge --delete-branch --body "PR #N"`).
4. Definition of production-ready (acceptance for the whole campaign): full Phase-0 battery green locally AND in CI; zero proven P0/P1 findings open; every Slice-0 RED control either retired by its fix or carrying an owner + spec; weekly perf gates promoted to per-release; spec ledgers reconciled; docs claims match measurements.

## Execution notes

- **Order:** phases are sequential by number except: Phase 5 (security) and Phase 6 (supply chain) are independent of 1-4 and may run in parallel subagent lanes; Phase 7 depends on all measurements.
- **Compile-heavy cap:** at most 2 concurrent cargo-invoking lanes on this machine.
- **Parallel hazard:** NEVER run the test suite with default threads to "save time" — serial is the correctness gate; env-mutating tests are UB-racy in parallel by design.
- **Disk:** cargo clean policy per CLAUDE.md after heavy gate sessions; keep ≥50GB free on C: (builds target E:).
- **Disk state:** `.symforge/coupling.db` is 50.8MB, `index.bin` 28.7MB — footprint numbers belong to Phase 4, do not delete during diagnosis.
