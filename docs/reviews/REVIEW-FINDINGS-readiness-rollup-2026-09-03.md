# REVIEW-FINDINGS — readiness rollup — 2026-09-03

**Campaign:** Production-Readiness Multispectrum Diagnosis (SymForge v11.0.13 / v11.1.0)
**Baseline:** `main` @ `6188c5af` (worktree `1b570f1c`)
**Host Machine:** Windows 11 Pro, Intel Core Ultra 7 265, x64, NVMe storage.
**Execution Date:** 2026-09-03
**Status:** **DIAGNOSIS COMPLETE — ZERO SOURCE CODE MODIFICATIONS PERFORMED.**

---

## 1. Executive Summary & Production Readiness Verdict

SymForge's core engineering is remarkably robust:
- **Baseline Battery Cleared (Phase 0):** All 8 gate commands passed cleanly (`cargo fmt`, `git diff --check`, `cargo clippy -- -D warnings`, `cargo build --release`, `cargo check --no-default-features --features embed`, `npm test`, criterion bench smoke).
- **Canonical Serial Test Gate:** **4,324 tests passed, 0 failed, 24 ignored across 135 test binaries in 19m12s.**
- **Hardening & Security:** Global `unsafe_code = deny` enforced with only 12 strictly reviewed Win32/POSIX signaling sites. No credential leaks, zero unhandled panics in production MCP request paths, sound loopback/DNS-rebinding guards, and clean tab-delimited telemetry logging.
- **Token Economy:** In `SYMFORGE_SURFACE=compact` mode, prompt schema overhead is reduced by **94.4% (saving 21,261 tokens per interaction)**.

However, the diagnosis uncovered **four High-Risk (P1) items** that prevent unconditional production readiness:
1. **Live Concurrency Defect (2.4):** Concurrent first opens of a project perform duplicate full cold loads outside the map lock (`daemon.rs:10346`).
2. **Weekly CI Performance Regression:** `test_load_perf_1000_files` failed its SLA bound (measured 3,213 ms vs < 3,000 ms).
3. **Storage Mutation Seam:** `add_file` lacks an `EmptyBootstrap` gate, allowing unrooted placeholders to accumulate mutations.
4. **Observer Mutation Seam:** Mutations applied while a candidate is building are lost on swap.

---

## 2. Ranked Findings Ledger

### Tier 1: High Production Risk (P1)

| Finding ID | Anchor @ `6188c5af` | Description | Label | Remediation Size |
|---|---|---|---|---|
| **P1-01** | `src/daemon.rs:10300`, `:10346` | **Concurrent first open cold-load duplication:** 4 concurrent first opens execute 4 full cold loads outside lock; losers discarded by `or_insert` after paying full I/O & parse cost. | **PROVEN** | Medium (PR / single-flight in `ensure_project_slot_for_session_with`) |
| **P1-02** | `tests/live_index_integration.rs:600`, `:615` | **Weekly CI Performance Regression (LIDX-05):** 1000-file load benchmark measured 3,213 ms vs < 3,000 ms ceiling in release mode. | **PROVEN** | Small/Medium (tuning or SLA calibration) |
| **P1-03** | `src/live_index/store.rs:2923`, `tests/project_index_lifecycle_slice0.rs:156` | **Storage-level unrooted mutation admission:** `add_file` has no `EmptyBootstrap` gate; unrooted placeholder admits files. Mitigated at query layer by `health_view.rs:347`. | **PROVEN** | Small (gate in `add_file`) |
| **P1-04** | `src/index_lifecycle/activation.rs:771`, `tests/project_index_lifecycle_slice0.rs:576` | **Observer mutation dropped during candidate build:** Observer mutations occurring during swap window are not carried through to the promoted index. | **PROVEN** | Medium (mutation carry in candidate pipeline) |

---

### Tier 2: Medium Production Risk (P2)

| Finding ID | Anchor @ `6188c5af` | Description | Label | Remediation Size |
|---|---|---|---|---|
| **P2-01** | `tests/stel_golden_replay.rs:151-156` | **Vacuous skip on missing corpora:** Golden replay integration tests skip vacuously with code 0 if `tests/fixtures/phase0-corpus/` is not cloned locally. | **PROVEN** | Small (fail loudly in CI / flag) |
| **P2-02** | `src/daemon.rs:7200`, `src/protocol/tools.rs:14661` | **Parallel test hazard (36% suite ratio):** 1,571 tests in 55 files mutate global env/CWD, necessitating the 19m serial gate (`--test-threads=1`). | **PROVEN** | Large (Speckit spec for env context isolation) |
| **P2-03** | `.github/workflows/ci.yml` | **Coverage tooling completely absent:** No `cargo-llvm-cov`, `tarpaulin`, or CI coverage artifact exists. | **PROVEN** | Medium (wire `cargo-llvm-cov` in CI) |
| **P2-04** | `.github/workflows/ci.yml:290` | **Orphaned performance smokes:** Graph BFS p95 (2.73ms), team artifact roundtrip (3.93x), and sidecar latency smokes run only locally with no CI home. | **PROVEN** | Small (add to `performance-smoke` job) |

---

### Tier 3: Low / Operational / Code Debt (P3)

| Finding ID | Anchor @ `6188c5af` | Description | Label | Remediation Size |
|---|---|---|---|---|
| **P3-01** | `src/protocol/tools.rs:1` (35k LOC) | **God-file bloat & inline test inflation:** 6 files account for >35% of codebase; `tools.rs` has 20,300 lines of inline tests. | **PROVEN** | Large (God-file split specs, `tools.rs` first) |
| **P3-02** | `.github/workflows/ci.yml` (`SEC-1`) | **Supply chain audit gap:** Neither `cargo audit` nor `cargo deny check` is executed in CI workflows. | **PROVEN** | Small (add CI step) |
| **P3-03** | `src/protocol/edit_tools.rs:41` (`SEC-2`) | **Project-config trust gate defaults to LogOnly:** Edit tools only warn; `symforge trust status` does not disclose effective mode. Inert today (no `.symforge/config.toml` consumer). | **PROVEN** | Small (default to Enforce or disclose in status) |
| **P3-04** | `vendor/tree-sitter-scss/` (`SEC-3`) | **Vendored grammar divergence:** Diverges in two files (`build.rs` deletion + undocumented `scanner.c` void casts); patch comment misdescribes both. | **PROVEN** | Trivial (comment update) |
| **P3-05** | `src/server/admin/mod.rs:116` (`SEC-5`) | **Admin/API router lacks Host validation:** DNS-rebinding allow-list configured only on `/mcp`; admin/API protected by Origin check and loopback bind. | **PROVEN** | Small (apply Host allow-list across merged router) |
| **P3-06** | `execution/version_sync.py` (`SUP-6`) | **version_sync platform manifest blind spot:** Checks root/optionalDeps but does not read `npm/platforms/*/package.json` directly. Mitigated by release-please config. | **PROVEN** | Small (expand version_sync script) |
| **P3-07** | `execution/free_runner_disk.sh` (`SUP-9`) | **CI runner disk cleanup lacks assertion:** Cleans paths but has no post-cleanup assertion; can silently no-op if runner image paths change. | **PROVEN** | Trivial (add assertion) |
| **P3-08** | `specs/` (32 dirs) | **Spec ledger drift:** 8 specs lack `tasks.md`; 15 specs contain 510 unchecked task checkboxes for shipped features. | **PROVEN** | Medium (reconcile task ledgers) |
| **P3-09** | `docs/OUTSTANDING-WORK.md`, `docs/backlog.md` | **Stale documentation files:** Both predate Feature 020/025 closure; already staged for deletion in `stash@{0}`. | **PROVEN** | Small (move to `docs/archive/`) |
| **P3-10** | `tasks/todo.md` (178 KB, 2031 lines) | **Append-only session log bloat:** Historical session log needs archival to `docs/archive/` (Owner Decision). | **PROVEN** | Small (Owner decision) |
| **P3-11** | `Cargo.toml:84-88` (`SEC-4`) | **Version comment rot:** Comment references `rmcp 1.1.0` requirement while dependencies bumped to `3.1.4`. | **PROVEN** | Small (text update) |
| **P3-12** | `src/parsing/resolver/` (148 LOC) | **Dead spike code:** Feature `cbm-spike` declared "no consumer yet" in Cargo.toml with 0 external callers. | **PROVEN** | Small (delete or retain as feature) |
| **P3-13** | `src/daemon.rs:1855`, `search_tools.rs:27` | **Unwired code:** `remove_project_from_session` and `AdvertisedSearchKnowledgeSourceScope` marked `#[allow(dead_code)]`. | **PROVEN** | Small (wire or prune) |
| **P3-14** | `tests/watcher_layer3_restat.rs:155` | **Spec claim refuted:** Empty `#[ignore]` body is the `#[cfg(not(windows))]` stub; real test runs on Windows. | **PROVEN REFUTED**| Small (cfg-omit instead of ignore) |
| **P3-15** | `src/path_shadow.rs:549-565` | **Spec claim refuted:** `PathGuard` unsafe PATH mutation is strictly inside `#[cfg(test)] mod tests`. | **PROVEN REFUTED**| None (code is sound) |

---

## 3. Remediation Routing & Sequencing

Remediation work is grouped into three execution paths:

### Route A: Direct Surgical PRs (No architectural changes)
1. **PR 1 (Daemon Single-Flight):** Implement in-flight loading table in `src/daemon.rs:ensure_project_slot_for_session_with` to deduplicate concurrent loads; retire `#[ignore]` on `daemon.rs:10300`.
2. **PR 2 (CI Perf Gate & Smoke Promotion):** Promote the 5 orphaned performance smokes into `.github/workflows/ci.yml` `performance-smoke`; calibrate/tune `test_load_perf_1000_files`.
3. **PR 3 (Storage EmptyBootstrap Gate):** Add explicit `load_source != EmptyBootstrap` guard in `src/live_index/store.rs:2923`.
4. **PR 4 (CI Security Screening & Comment Fixes):** Add `cargo audit` / `cargo deny check` steps to CI; update `rmcp` comment in `Cargo.toml:84-88`.
5. **PR 5 (Documentation Archival):** Archive `docs/OUTSTANDING-WORK.md`, `docs/backlog.md`, and `tasks/todo.md` to `docs/archive/`.

### Route B: Spec Kit Specifications (Behavioral changes)
1. **`specs/033-vacuous-test-gates`:** Enforce fail-closed behavior for fixture-dependent tests in CI environments.
2. **`specs/034-candidate-observer-reconciliation`:** Address the observer mutation carry seam during candidate build/swap.
3. **`specs/035-spec-ledger-reconciliation`:** Reconcile all 510 unchecked boxes across `specs/` against shipped commits.

### Route C: God-File Decomposition Track
1. **Spec 036 (Split `src/protocol/tools.rs`):** First extract the 20,300 lines of inline unit tests to `tests/protocol_tools/`, reducing file size by 58%; then separate response classification helpers and path guards into submodules.
2. **Spec 037 (Split `src/daemon.rs`):** Extract the 10,000 lines of inline tests to `tests/daemon/`; separate HTTP server routes from session lifecycle.

---

## 4. Acceptance Checklist for Production-Ready State (Spec §Phase 8.4)

| Gate / Requirement | Target | Measured Status | Disposition |
|---|---|---|---|
| **Phase 0 Battery** | Full battery green locally and in CI | **PASS locally (all 8 commands clean)** | **SATISFIED** |
| **P0 Findings** | Zero open proven P0 findings | **0 Open P0s** | **SATISFIED** |
| **P1 Findings** | Zero open proven P1 findings | **4 Open P1s** (cold-load duplication, load perf regression, unrooted storage, candidate mutation carry) | **UNSATISFIED — Blocks release** |
| **Slice-0 RED Controls** | Retired by fix or carrying active owner/spec | All 8 controls fail; 2.4 active defect, others have stale bodies or open seams | **PARTIALLY SATISFIED (Needs retargeting/fixes)** |
| **Weekly Perf Gates** | Promoted to per-release / CI automated | 2 in weekly CI (1 failing); 5 orphaned with no CI home | **UNSATISFIED** |
| **Spec Ledgers** | Reconciled against shipped code | 8 missing tasks.md; 510 unchecked boxes across 15 specs | **UNSATISFIED** |
| **Documentation Honesty** | Claims match empirical measurements | Token savings clarified: 94% schema reduction / 25% net session reduction | **SATISFIED (Documented)** |

---

## 5. Artifact Reference Map

Every spectrum in this campaign has a standalone evidence document committed under `docs/reviews/`:
1. `docs/reviews/REVIEW-FINDINGS-readiness-baseline-2026-09-03.md` (Gate timing table & environment)
2. `docs/reviews/REVIEW-FINDINGS-readiness-correctness-2026-09-03.md` (Ignored tests, vacuous skips, parallel hazards)
3. `docs/reviews/REVIEW-FINDINGS-readiness-code-slop-2026-09-03.md` (God files, dead code, panic audit, suppressions)
4. `docs/reviews/REVIEW-FINDINGS-readiness-hardening-2026-09-03.md` (Unsafe audit, cold start curve, process lifecycle)
5. `docs/reviews/REVIEW-FINDINGS-readiness-performance-2026-09-03.md` (Bench gates, calibration numbers, token economy)
6. `docs/reviews/REVIEW-FINDINGS-readiness-security-2026-09-03.md` (Dependency audit, vendored grammar, auth & trust)
7. `docs/reviews/REVIEW-FINDINGS-readiness-supply-chain-2026-09-03.md` (Release-please, traceability, npm packaging, CI hygiene)
8. `docs/reviews/REVIEW-FINDINGS-readiness-docs-2026-09-03.md` (Backlog status, todo archival, spec ledger drift)
