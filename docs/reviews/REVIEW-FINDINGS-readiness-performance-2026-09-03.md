# REVIEW-FINDINGS — readiness performance — 2026-09-03

**Spectrum:** Phase 4 (Performance)
**Baseline:** `main` @ `6188c5af`
**Instruments:** SymForge MCP release binary, stdio JSON-RPC schema profiling, criterion bench smoke, local calibration runs.

---

## 4.1 Cold start vs Warm start

- **Operator deadline:** `ADMIN_SERVE_START_DEADLINE = 60s`.
- **Measured timings on test machine (Core Ultra 7 265, NVMe):**
  - **Warm start (`SnapshotRestore`):** **7.0s** to transition `Empty` → `Loading` → `Ready`.
    - Headroom: **53.0s (88%)**.
    - `.symforge/index.bin` footprint: **28.7 MB**.
    - `.symforge/coupling.db` footprint: **50.8 MB**.
  - **Cold scan (full re-parse of 1146 files):** **18.0s** (measured via `index_folder` on fresh instance).
  - **Total estimated cold start:** **~25.0s**.
    - Headroom: **35.0s (58%)**.
- **Margin curve risk (PROVEN):**
  - On this 20-core workstation with fast NVMe, cold start easily clears the 60s threshold.
  - However, on typical CI runners or larger monorepos (>5,000 files), cold discovery without snapshot restore risks timing out or flirting with the 60s operator deadline. The snapshot restore path (Feature 026) is critical to production adoption.

---

## 4.2 Bench gate (`benches/observed_refresh_gate_v1.rs`)

- **Frozen registration:** `criterion_group:observed_refresh_gate_v1_group -> observed_refresh_gate_v1`.
- **Corpus digest:** `51ce7613e55e2c1715c533a13a28ea9441f89e4ea9adb2c9993f805a7d689a11`.
- **Smoke gate:** `cargo bench --bench observed_refresh_gate_v1 -- --test` passed cleanly in Phase 0 Step 5 (**7m26s** wall time).
- **Recorded baseline comparison (`docs/reviews/OBSERVED-REFRESH-GATE-v1.md`):**
  - `delivered_event/add`: 253 ms p95 (0.81× baseline) — **PASS**
  - `delivered_event/burst_24`: 291 ms p95 (0.85× baseline) — **PASS**
  - `delivered_event/delete`: 254 ms p95 (0.82× baseline) — **PASS**
  - `delivered_event/modify`: 256 ms p95 (0.84× baseline) — **PASS**
  - `delivered_event/rename`: 252 ms p95 (1.00× baseline) — **PASS**
  - `need_rescan/fresh_instance_rescan`: 17 ms p95 (1.00× baseline) — **PASS**
  - **All p95 latencies are under 300 ms**, well within the 2.0s production ceiling.

---

## 4.3 Calibration smokes & regressions

Summary of local runs across all calibration suites:

| Test | Location | Bound / Target | Measured | Status |
|---|---|---|---|---|
| `test_load_perf_1000_files` | `tests/live_index_integration.rs:600` | < 3,000 ms (LIDX-05) | **3,213 ms** | **PROVEN P1 REGRESSION** |
| `calibrate_current_repo_smoke` | `tests/coupling_calibration.rs:84` | exits clean | 8.88 s | **PASS** |
| `graph_bfs_real_repo_p95_calibration` | `tests/graph_bfs_calibration.rs:26` | p95 < 10 ms | **2.73 ms** (p50=391µs) | **PASS** (35.7k nodes) |
| `team_artifact_real_repo_round_trip` | `tests/team_artifact_calibration.rs:25` | 0 mismatches | **0 mismatches, 3.93× zstd** | **PASS** (1145 files) |
| `test_single_file_reparse_perf_smoke` | `tests/watcher_integration.rs:428` | < 250 ms (FRSH-02) | **20 ms** | **PASS** |
| `health_latency_p95_smoke` | `tests/sidecar_integration.rs:268` | p95 < 5 ms | **852 µs** (p50=576µs) | **PASS** |
| `hook_binary_latency_smoke` | `tests/sidecar_integration.rs:513` | exits clean | **0.06 s** | **PASS** |

**Finding:** The 1000-file load benchmark (`test_load_perf_1000_files`) exceeded its 3000ms deadline by 213ms. This is an active failure in the scheduled CI `performance-smoke` job.

---

## 4.4 Token economy

Measured by inspecting the raw JSON-RPC schemas returned by `tools/list` across surface profiles:

- **`SYMFORGE_SURFACE=full` (39 tools):**
  - Total schema characters: **85,544 bytes**
  - Estimated schema prompt tokens: **~22,512 tokens**
- **`SYMFORGE_SURFACE=compact` (3 tools: `symforge`, `symforge_edit`, `status`):**
  - Total schema characters: **4,755 bytes**
  - Estimated schema prompt tokens: **~1,251 tokens**
- **Economy impact:**
  - **94.4% reduction in prompt token overhead** (saving **21,261 tokens** on every single agent interaction).
  - In full mode, the MCP schema alone consumes more than 10% of a standard 200k context window before any file or conversation content is exchanged. Compact mode is strongly recommended as the default production configuration.
