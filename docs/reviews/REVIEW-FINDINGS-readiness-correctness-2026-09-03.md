# REVIEW-FINDINGS — readiness correctness — 2026-09-03

**Spectrum:** Phase 1 (Correctness)
**Baseline:** `main` @ `6188c5af` (clean tree + plan doc)
**Instrument:** test runners via `hub` supervised processes, stdio MCP queries.

---

## 1.1 Ignored-test disposition (26 sites cataloged)

Across the repository, 26 `#[ignore]` attributes exist:

### Category A: Deliberate out-of-suite performance / calibration smokes (11 sites)
These run outside the default suite by design to preserve speed and avoid scheduler jitter:
- `tests/coupling_calibration.rs:27` (`calibrate_against_real_repos`) — multi-repo calibration; requires external env targets.
- `tests/coupling_calibration.rs:84` (`calibrate_current_repo_smoke`) — **one of only two weekly CI gates**; passed locally in 8.88s.
- `tests/graph_bfs_calibration.rs:26` (`graph_bfs_real_repo_p95_calibration`) — real-repo scale p95 calibration; **has NO CI home**; passed locally at p95=2.73ms.
- `tests/live_index_integration.rs:600` (`test_load_perf_1000_files`) — **weekly CI gate**; **PROVEN REGRESSION: measured 3213ms vs <3000ms bound** (LIDX-05).
- `tests/sidecar_integration.rs:268` (`health_latency_p95_smoke`) — **NO CI home**; passed locally at p95=852µs.
- `tests/sidecar_integration.rs:513` (`hook_binary_latency_smoke`) — **NO CI home**; passed locally in 0.06s.
- `tests/team_artifact_calibration.rs:25` (`team_artifact_real_repo_round_trip_calibration`) — **NO CI home**; passed locally (1145 files, 0 mismatches, 3.93× compression).
- `tests/watcher_aap_shaped_fixture.rs:420` (`aap_smoke_no_destruction_full_5_min`) — 2×5-minute idle windows; deliberately manual-only.
- `tests/watcher_integration.rs:428` (`test_single_file_reparse_perf_smoke`, FRSH-02) — **NO CI home**; passed locally in 20ms (vs 250ms threshold).
- `src/live_index/local_ref_scout.rs:3317` (`local_ref_gate_default_path_cost_is_negligible`) — perf smoke for local ref gating.
- `tests/perl_corpus.rs:192` (`bench_corpus_parse_metrics`) — benchmark harness.

### Category B: Platform-gated or env-isolated tests (2 sites)
- `tests/watcher_layer3_restat.rs:155` — **Spec claim REFUTED**: the plan labeled this "vacuous test wearing a Windows-AV-lock justification, proven slop". In reality, the `#[ignore]`'d empty body is the `#[cfg(not(windows))]` stub. The `#[cfg(windows)]` variant (lines 112–151) has a full, running body that executes as part of the default suite on Windows. Residual nit: could be `#[cfg]`-omitted instead of ignored-empty.
- `src/live_index/local_ref_scout.rs:3223` (`process_spawn_spy_confirms_offline_ingestion_never_shells_out`) — mutates process-global `PATH`/`GIT_EXEC_PATH`; excluded to avoid UB races with concurrent env readers.

### Category C: Diagnostic probes & planning spikes (3 sites)
- `src/parsing/xref.rs:3417` (`probe_deferred_perl_refs`) — empirical s-expr probe.
- `src/parsing/xref.rs:3435` (`probe_perl_grammar_sexp`) — diagnostic probe for tree-sitter grammar bump verification.
- `tests/cbm_spike_rust_resolver.rs:26` (`cbm_spike_rust_resolver_fixture_pass_rate`) — 015 S0 spike planning falsifier.

### Category D: Manual acceptance gates (2 sites)
- `tests/search_knowledge.rs:1117` (Gate I) and `:1176` (Gate J) — manual acceptance against complete repository corpus.

### Category E: Slice-0 RED controls (8 sites) — **CRITICAL FINDING**
Every one of these 8 controls **FAILS when run** against HEAD (`1b570f1c`):

1. **`src/daemon.rs:10300`** (`concurrent_first_open_performs_exactly_one_cold_load`)
   - **Label:** **PROVEN P1** (design defect 2.4 is STILL LIVE).
   - **Evidence:** Ran locally: panicked at `daemon.rs:10346:9` (`left: 4, right: 1`). Four concurrent first opens of one root perform 4 complete cold loads outside the map lock; losers are discarded by `or_insert` after paying full I/O and parse costs.
   - **Remediation note:** Attribute promised "remove in Slice 2 (T030-T040)". Slice 2 shipped in `#565` (commit `6c3794f3`), but its single-flight admission does NOT deduplicate `ProjectInstance::load` in this path. The defect is active.

2. **`tests/project_index_lifecycle_slice0.rs:109`** (`capacity_refused_open_creates_no_slot_and_no_watcher`)
   - **Label:** **PROVEN** (stale control body; code-level behavior partially addressed).
   - **Evidence:** Panicked at line 130: `it returned Ok("project-v1-b8d32668...")`. V11 answers a refused open with typed refusal + non-ready slot (FR-004 lease), not the old `Err` this test asserts. The control was marked `CONTROL-STALE` in its own attribute and never retargeted.

3. **`tests/project_index_lifecycle_slice0.rs:156`** (`empty_placeholder_publication_refuses_watcher_mutation`)
   - **Label:** **PROVEN** (open residual at storage level; mitigated at publication level).
   - **Evidence:** Panicked at line 235: `the watcher admitted 8 path(s) into a never-published empty placeholder`. `live_index/store.rs:2923` (`add_file`) has no `EmptyBootstrap` gate. Mitigation exists at `health_view.rs:347`: `index_state()` refuses to report `Ready` for an `EmptyBootstrap` index, but the storage layer still admits the mutations.

4. **`tests/project_index_lifecycle_slice0.rs:576`** (`watcher_mutation_during_candidate_build_is_not_discarded`)
   - **Label:** **PROVEN** (candidate pipeline exists; mutation-carry seam open).
   - **Evidence:** Panicked at line 644: `observer mutation applied while candidate was building was destroyed by swap`. `IsolatedCandidate` exists in `index_lifecycle/activation.rs:771`, but observer mutations during the build window are still dropped on swap.

5. **`tests/project_index_lifecycle_slice0.rs:674`** (`whole_project_publication_preserves_latest_siblings`)
   - **Label:** **PROVEN** (`CONTROL-STALE`; retarget needed).
   - **Evidence:** Panicked at line 756: `source A publication replaced whole index and dropped sibling B`. Stale test racing V10 `LiveIndex::reload` against 1500 files in 150ms.

6. **`tests/project_index_lifecycle_slice0.rs:777`** (`snapshot_seed_is_not_queryable_before_verification`)
   - **Label:** **PROVEN** (`CODE-WRONG` residual: stat-change window before verify).
   - **Evidence:** Panicked at line 840: snapshot restored but pending verification answered a query for a file whose bytes changed on disk. `SnapshotVerifyState::Pending` gates `index_state()` to `Loading`, but direct `get_file` calls bypass the readiness check.

7. **`tests/project_index_lifecycle_slice0.rs:858`** (`configured_capacity_bounds_the_process_not_each_load`)
   - **Label:** **PROVEN** (`CONTROL-STALE`: `ProcessCapacityPool` exists; test targets `SYMFORGE_MAX_INDEX_FILES`).
   - **Evidence:** Panicked at line 899: two projects admitted 20 files against configured ceiling of 10. `SYMFORGE_MAX_INDEX_FILES` remains per-discovery-pass, not process-wide. `ProcessCapacityPool` exists in `capacity.rs` but is not wired to this env var.

8. **`tests/project_index_lifecycle_slice0.rs:922`** (`same_path_root_replacement_is_not_silently_adopted`)
   - **Label:** **PROVEN** (open residual: same-path recreation).
   - **Evidence:** Panicked at line 952: root deleted and recreated at same path adopted with freshness going Current → Current. `PhysicalRootIdentity` exists, but same-path replacement lacks a durable identity fence in the registry.

---

## 1.2 Vacuous-skip hunt

Four skip classes identified across integration tests:

1. **`tests/stel_golden_replay.rs:151-156, 176-179, 204-207`** — skips vacuously (`eprintln` + `return` → pass) when `tests/fixtures/phase0-corpus/` is unpopulated. **PROVEN RISK**: local developer runs skip 3 golden replay tests silently unless the developer explicitly reads the corpus README and clones them. Mitigation: Phase 0 step 6 cloned the corpora; 7/7 tests confirmed running non-vacuously under `--nocapture`.
2. **`tests/live_index_integration.rs:476-483`** (`test_stdout_purity`) — skips with `eprintln!("SKIP ...")` if `target/debug/symforge.exe` is absent. Passes when binary exists.
3. **`tests/stel_battery_gates.rs:100-104`** — skips if `docs/research/results-v8-phase2-candidate.json` is missing.
4. **`tests/system_path_refusal.rs:46-48, 104-106`** — `continue` on missing system dirs (`/etc`, `/proc`). Benign platform gating.

**Recommendation:** Vacuous-skip tests in classes 1–3 should fail loudly when `CI=true` or when an explicit flag is set, rather than returning `ok`.

---

## 1.3 Parallel-hazard catalog

- **Machinery:**
  - Two independent `CwdGuard` implementations: `src/daemon.rs:7200` and `src/protocol/tools.rs:14661` (drifted from spec anchor 14629).
  - Over 17 distinct `*_ENV_LOCK` / `*_ENV_GUARD` statics across `src/` (daemon, hook, update, discovery ×3, frecency, persist, store ×3, coupling/lifecycle, coupling/store, serve, stel/envelope, watcher).
  - Additional locks in `tests/` (`stel_surface_env::COMPACT_ENV_LOCK`, etc.).
- **Ratio:** 55 test-containing files out of the repository use env/cwd mutation machinery, housing **1,571 test functions out of 4,367 total (36.0%)**.
- **Cost:** Running the test suite parallel without an isolated subprocess per test would corrupt ~36% of the assertions due to process-global `PATH`, `SYMFORGE_*`, and CWD mutation. **The serial gate (`--test-threads=1`, 19m12s) is structurally irreducible without architectural separation of process env from test harnesses.**

---

## 1.4 Coverage map

- **Tooling status:** **PROVEN GAP** — no coverage tooling configured anywhere. `cargo llvm-cov` absent from machine; no `tarpaulin`, `codecov.yml`, or coverage step in `.github/workflows/ci.yml`.
- Degraded per plan §Assumptions & contingencies: coverage tooling is absent as an operational capability.

---

## 1.5 Weekly-only gates & calibration census

### CI Weekly job (`performance-smoke` in `.github/workflows/ci.yml:290-308`)
Runs on schedule/dispatch only:
1. `live_index_integration::test_load_perf_1000_files` — **FAILED: 3213ms vs <3000ms bound** (**PROVEN P1 REGRESSION**).
2. `coupling_calibration::calibrate_current_repo_smoke` — **PASSED: 8.88s**.

### Smokes with NO CI home (orphaned from automated gates)
The spec assumed these run in the weekly job, but `.github/workflows/ci.yml` only contains the two above. Measured locally:
- `graph_bfs_calibration::graph_bfs_real_repo_p95_calibration` — **PASS** (p95=2.73ms, 35.7k nodes, 23.3k edges).
- `team_artifact_calibration::team_artifact_real_repo_round_trip_calibration` — **PASS** (1145 files, 0 mismatches, 3.93× compression, 13.28s).
- `watcher_integration::test_single_file_reparse_perf_smoke` (FRSH-02) — **PASS** (20ms vs 250ms threshold).
- `sidecar_integration::health_latency_p95_smoke` — **PASS** (p50=576µs, p95=852µs).
- `sidecar_integration::hook_binary_latency_smoke` — **PASS** (0.06s).

**Recommendation:** Promote the 5 orphaned smokes into the weekly CI job (or a per-release workflow) so these regressions are caught automatically.
