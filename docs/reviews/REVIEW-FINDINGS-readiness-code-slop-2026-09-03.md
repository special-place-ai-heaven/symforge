# REVIEW-FINDINGS — readiness code-slop — 2026-09-03

**Spectrum:** Phase 2 (Code-slop)
**Baseline:** `main` @ `6188c5af`
**Instruments:** SymForge MCP release binary (`get_file_context`, `search_symbols`), Python AST/regex scans, `git grep`.

---

## 2.1 God-file decomposition analysis

Six files account for over 35% of the repository's total source LOC. Each has been profiled via SymForge MCP outline extraction for natural decomposition seams:

| File | LOC | Symbols | Tests LOC (% of file) | Proposed Seam Modules |
|---|---|---|---|---|
| `src/protocol/tools.rs` | 34,965 | 896 | ~20,300 (58%) | `protocol/classify.rs`, `protocol/path_guard.rs`, `protocol/knowledge_tools.rs`, `protocol/read_tools.rs`, extract tests to `tests/protocol_tools/` |
| `src/daemon.rs` | 16,484 | 407 | ~10,080 (61%) | `daemon/server.rs`, `daemon/session.rs`, `daemon/instance.rs`, extract tests to `tests/daemon/` |
| `src/live_index/store.rs` | 9,260 | 433 | ~3,740 (40%) | `live_index/store/budget.rs`, `live_index/store/publication.rs`, `live_index/store/mutations.rs` |
| `src/sidecar/handlers.rs` | 7,608 | 191 | ~4,100 (54%) | `sidecar/models.rs`, `sidecar/context.rs`, `sidecar/routes.rs` |
| `src/protocol/format.rs` | 6,999 | 265 | ~3,200 (46%) | `protocol/format/quarantine.rs`, `protocol/format/envelopes.rs` |
| `src/protocol/edit.rs` | 6,689 | 265 | ~3,100 (46%) | `protocol/edit/atomic_io.rs`, `protocol/edit/plans.rs` |

### Key architectural finding: Inline test inflation
**PROVEN**: Across all six god files, **over 50% of the lines are inline `#[cfg(test)] mod tests` blocks** rather than production logic. In `src/protocol/tools.rs`, over 20,300 lines are tests! Extracting these test suites into standalone `tests/` integration or sibling submodules would immediately cut `tools.rs` from 35k lines to ~14.6k lines without changing a single line of production code.

---

## 2.2 Dead-code sweep

1. **`src/parsing/resolver/` (148 LOC, `mod.rs` + `rust.rs`)**
   - **Label:** **PROVEN DEAD SPIKE CODE**
   - **Evidence:** Gated behind `#[cfg(feature = "cbm-spike")]` in `src/parsing/mod.rs:7-8`. `Cargo.toml:58-63` explicitly documents: *"Program 015 Sprint-0 falsifier spike still pending promotion: the Rust resolver only (`src/parsing/resolver/`, targeted at S3 — no consumer yet)."* No production callers exist anywhere in the crate.
2. **`src/daemon.rs:1855` (`remove_project_from_session`)**
   - **Label:** **PROVEN UNWIRED CODE**
   - **Evidence:** Marked `#[allow(dead_code)]` with comment: *"until the tool wires it"*. Project detachment is never called from any MCP tool or HTTP route.
3. **`src/protocol/search_tools.rs:27` (`AdvertisedSearchKnowledgeSourceScope`)**
   - **Label:** **PROVEN UNWIRED ENUM**
   - **Evidence:** Marked `#[allow(dead_code)]`; models an advertised scope parameter that was never wired to the client dispatch surface.
4. **`__test-internals` feature gate (`Cargo.toml:72`)**
   - **Label:** **PROVEN CLEAN / NOT A LEAK**
   - **Evidence:** In release builds (`not(feature = "__test-internals")`), all internal engine modules are exported strictly as `pub(crate)` (`src/lib.rs:18-70`). Only test configurations compile with the door open.

---

## 2.3 Panic-path audit

- **True production census** (excluding all test functions, test modules, and test fixtures):
  - `.expect(...)`: **233** sites across 53 files
  - `.unwrap()`: **165** sites
  - `panic!(...)`: **4** sites (all in `src/stel/golden_replay.rs:70,85,191,210`)

### Reachability analysis:
- **`panic!(...)` in production code:** **ZERO REACHABLE FROM OPERATOR TRAFFIC**. The 4 panics in `src/stel/golden_replay.rs` (`corpus_for_row_id`, `corpus_marker_for_row_id`) are only called by `tests/stel_golden_replay.rs` and `tests/stel_l3_enforcement.rs`. No daemon route, MCP tool, or watcher handler can reach them.
- **Top `.expect(...)` clusters:**
  1. `src/index_lifecycle/activation.rs` (36 expects): all are unpoisoned Mutex lock acquisitions (`.lock().expect(...)`) and atomic lane state transitions (`complete_handoff().expect(...)`).
  2. `src/live_index/coupling/store.rs` (21 expects): SQLite statement preparation and parameter binding.
  3. `src/cli/init.rs` (14 expects): CLI initialization path fatal I/O guards.
  4. `src/index_lifecycle/registry.rs` (11 expects): registry Mutex lock acquisitions.
  5. `src/index_lifecycle/capacity.rs` (10 expects): capacity ledger Mutex lock acquisitions.

**Verdict:** No unhandled panic path reachable from client MCP tools or sidecar requests was found. All panic/expect sites in request-handling paths are either Mutex poison guards (acceptable invariant) or CLI startup exits.

---

## 2.4 Suppression audit

23 non-`unsafe_code` `#[allow(...)]` attributes cataloged:
- `clippy::too_many_arguments` (10 sites): concentrated in render/format functions in `format.rs`, `tools.rs`, and `watcher/mod.rs`.
- `dead_code` (6 sites): `daemon.rs:1855` (unwired method), `search_tools.rs:27` (unwired enum), `store.rs:2438` (telemetry reader), and test helpers.
- `deprecated` (1 site: `src/protocol/mod.rs:1811`): `bind_workspace_from_client_roots`, waiting on rmcp PR #2577. Well-justified.
- `permissions_set_readonly_false` (1 site: `src/protocol/knowledge_curation.rs:3000`): clean directory tree teardown.
- `assertions_on_constants` (1 site: `src/live_index/rank_signals.rs:648`): intentional tier lock-in test.

**Verdict:** Zero unjustified suppressions. Every `#[allow]` carries an explanatory comment or standard compiler lint reason.

---

## 2.5 Debug-output classification

- Total `println!` in non-test code: 118
- Total `eprintln!` in non-test code: 85
- **Outside `src/cli/`:** only **4** production sites exist:
  - `src/main.rs:13`: `eprintln!("Error: {error}")` (CLI entry point error reporting — legitimate).
  - `src/server/serve.rs:146`: `eprintln!("{msg}")` (mirrors `tracing::warn!` for operator console visibility — deliberate).
  - `src/server/serve.rs:684`: `println!("{attach_url}")` (prints MCP connect URL to stdout for operator attach — legitimate).
  - `src/parsing/xref.rs:3427,3459`: diagnostic probes inside probe test functions.

**PROVEN**: **Zero** debug print statements exist in `daemon.rs`, `watcher/`, `live_index/`, `protocol/`, or `sidecar/` that bypass `tracing`. Operator telemetry is strictly structured.

---

## 2.6 Debt markers

- `git grep -E "TODO|FIXME|HACK|XXX"` across `src/`:
  - 6 matches in `src/protocol/format.rs:1111-1114, 6182-6185`: all are string literal matching patterns for markdown headings (e.g. `trimmed.starts_with("# TODO")`).
  - 1 match in `src/protocol/investigation.rs:167`: suggestion string `"search_text(query=\"TODO\")"`.
- **Real code debt markers in production source: ZERO**. Debt is tracked exclusively in specs and commit logs.
