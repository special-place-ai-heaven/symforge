# SymForge Full-Spectrum Code Review Report

> **Generated:** 2026-05-22  
> **Scope:** Read-only full-spectrum review  
> **Note:** No repository changes except this document.

**Date:** 2026-05-22  
**Scope:** Whole repository (read-only)  
**Verification run:** `cargo check` exit 0 (~5m 37s compile). `cargo test --all-targets -- --test-threads=1` — all completed crates passed (1,805 lib unit tests + many integration binaries); run was still executing `tests/sidecar_integration.rs` at report time with **zero failures observed** in ~2,500 lines of output. `cd npm && npm test` exit 0 (27 pass, 1 skip on Windows). `cargo build --release` **not** run in this review.

---

## Executive Summary

SymForge v7.13.0 is a mature, heavily tested Rust MCP server with a broad tool surface (31 `#[tool]` handlers plus `health_compact`), strong conformance tests, resources, and prompts. Local verification shows a healthy compile and very large automated coverage; the engineering culture emphasizes deterministic contracts (`result_status`), generation-fenced mutations, byte-oriented indexing (`fs::read`, `Vec<u8>` snapshots, content hashes), and explicit failure modes rather than silent corruption.

The largest **product/documentation gap** is drift between `AGENTS.md`'s "foundation tools" vision (`index_repository`, `repair_index`, `checkpoint_now`, `get_repo_outline`, `invalidate_cache`, idempotency keys) and the shipped surface (`index_folder`, `get_repo_map`, no repair/checkpoint run tools). Several backlog items in `docs/live-code-backlog.md` appear **already implemented** (untracked-file search diagnostics, sidecar PID in health), which makes planning docs a source of false work unless refreshed.

**Top risks for real use:** (1) mutating operations lack AGENTS-specified idempotency; (2) local daemon HTTP has **no authentication** and honors `SYMFORGE_DAEMON_BIND` — misconfiguration could expose project indexing/editing to the LAN; (3) maintainability hotspots (`src/protocol/tools.rs` ~21k lines, `src/live_index/query.rs` ~7k) increase regression risk; (4) persistence on clean shutdown only — crashes mid-session rely on snapshot + background verify, aligning with "shutdown is not a safe boundary" in spirit but without explicit checkpoint/resume tools.

**Recommended planning themes:** align `AGENTS.md`/README with actual tools; implement idempotency + repair/checkpoint or formally defer; harden daemon bind defaults; split god modules; refresh `docs/live-code-backlog.md`; add CI `clippy`/`fmt` gates; run release build in CI or locally before releases.

---

## Severity Legend

- **P0** Broken / data loss / security / correctness in production paths
- **P1** Significant gap or likely bug under real use
- **P2** Improvement, tech debt, missing tests/docs
- **P3** Nit, style, minor deferral

---

## Findings Table (master list)

| ID | Sev | Area | Location | Finding | Evidence | Suggested fix |
|----|-----|------|----------|---------|----------|---------------|
| SF-001 | P1 | Docs / API | `AGENTS.md` L111–125 vs `src/cli/init.rs` L298–329 | Foundation tools list names removed/renamed APIs (`get_repo_outline`, `get_file_outline`, `get_symbols`, `index_repository`, `repair_index`, `checkpoint_now`, `invalidate_cache`) | AGENTS lists v1 names; init exposes `get_repo_map`, `get_file_context`, `index_folder`, no repair/checkpoint | Update AGENTS to canonical v7 names or add aliases + migration table |
| SF-002 | P1 | Idempotency | `AGENTS.md` L68–83; codebase grep | No `idempotency_key` on mutating MCP tools (`index_folder`, edits, `batch_*`) | Only init hook merge mentions "idempotency"; no request hashing store | Add canonical hash + replay store per AGENTS rules for index/edit tools |
| SF-003 | P1 | Recovery | `AGENTS.md` L85–95; `tests/live_index_integration.rs` L665–687 | No `repair_index`, `checkpoint_now`, `cancel_index_run`, `get_index_run` tools | INFR-05 tests assert v1 `fn` names absent from `tools.rs` | Implement minimal repair/checkpoint surface or document intentional removal |
| SF-004 | P1 | Security | `src/daemon.rs` L1155–1212, L1228–1233 | Daemon REST API has no auth; binds via `SYMFORGE_DAEMON_BIND` defaulting from caller | Routes `/v1/sessions/.../tools/{tool_name}` open to whoever can reach host:port | Default bind `127.0.0.1` only; optional token; warn on non-loopback bind |
| SF-005 | P1 | Correctness | `src/protocol/tools.rs` L5137–5150 | `get_symbol_context` auto-resolves ambiguous symbol to **first** candidate path | `candidates.len() > 1` still sets `resolved_path = Some(candidates[0].clone())` | Return `Ambiguous` result_status; require `path`/`file` disambiguation |
| SF-006 | P1 | Reliability | `AGENTS.md` L43; `src/main.rs` L332–338 | AGENTS: "Shutdown is not a safe persistence boundary"; code serializes index only on **clean** MCP shutdown | Crash/kill skips `serialize_shared_index` | Add periodic checkpoint tool or WAL; document crash recovery path |
| SF-007 | P2 | MCP / Init | `src/cli/init.rs` L298–329 vs `tests/conformance.rs` L27–59 | `health_compact` registered in conformance (32 tools) but **absent** from `SYMFORGE_TOOL_NAMES` / client allowlists | conformance includes `health_compact`; init lists 31 `mcp__symforge__*` names | Add `health_compact` to init allowlists or document daemon-only |
| SF-008 | P2 | MCP / Aliases | `src/daemon.rs` L1637–1656; `tests/daemon_aliases.rs` | `trace_symbol` daemon alias still routes with deprecation banner | Alias maps to `get_symbol_context`; init tests exclude retired name | Keep alias until v8; track removal in CHANGELOG |
| SF-009 | P2 | Docs | `CLAUDE.md` L10 | Says "31 tools" while conformance lists **32** including `health_compact` | `EXPECTED_TOOLS` length 32 in `tests/conformance.rs` | Update CLAUDE.md count and list |
| SF-010 | P2 | Docs | `docs/live-code-backlog.md` L46–62 | Backlog #2 "untracked-file diagnostic" appears **done** | `tools.rs` has `append_untracked_file_diagnostic` + tests L12775+, L13558+ | Mark backlog item complete or narrow remaining gap |
| SF-011 | P2 | Docs | `docs/live-code-backlog.md` L64–78 | Backlog #3 "Sidecar PID in health" appears **done** | MCP `health` returned `Sidecar: pid=... state=alive`; tests `test_health_compact_surfaces_dead_sidecar_pid` | Close backlog item; add test link in doc |
| SF-012 | P2 | Maintainability | `src/protocol/tools.rs` | Single ~21k-line module holds most MCP tools | 31 `#[tool(` in one file; grep shows line ~18253+ | Split by domain: read/search/edit/index/health |
| SF-013 | P2 | Maintainability | `src/live_index/query.rs` | Very large query module (~7k+ lines) | Central symbol/file resolution | Extract disambiguation, bundles, health views |
| SF-014 | P2 | Architecture | `AGENTS.md` L171–178 vs `src/lib.rs` | Guided modules `application/`, `storage/`, `indexing/` not present | lib exports: `protocol`, `live_index`, `parsing`, `daemon`, etc. | Either adopt module split or revise AGENTS architecture section |
| SF-015 | P2 | Recovery | `src/live_index/persist.rs` L187–207 | Corrupt snapshot → `None` + warn; no quarantine artifact | `load_snapshot` drops corrupt bytes | Move bad `index.bin` to `.symforge/quarantine/` per AGENTS |
| SF-016 | P2 | Recovery | `AGENTS.md` L91; codebase | No dedicated "quarantine" for bad parses/spans beyond `ParseStatus::Failed` | `store.rs` `ParseStatus::Failed` per file | Add quarantine registry + health surfacing |
| SF-017 | P2 | Correctness | `src/daemon.rs` L366–368 | `close_session` uses `.unwrap()` on `projects.get_mut` after `find` | Theoretically racy if project removed between find and get_mut | Use `if let Some(project) = projects.get_mut(&pid)` |
| SF-018 | P2 | Security | `src/daemon.rs` L1847+ | `libc::kill` for daemon lifecycle (`#[allow(unsafe_code)]`) | Platform-specific process control | Document threat model; ensure PID file ownership checks |
| SF-019 | P2 | Security | `src/edit_safety/trust.rs` | Project config trust gate for edits (good) | 18 tests in `tests/edit_safety_trust.rs` | Extend docs for CI `enforce` vs `log_only` modes |
| SF-020 | P2 | Storage | `src/live_index/persist.rs` L47, L149–173 | Snapshot stores raw `Vec<u8>` content + hash; atomic tmp→rename | Aligns with byte-exact AGENTS intent | Add explicit test for CRLF preservation on Windows |
| SF-021 | P2 | Storage | `src/live_index/persist.rs` L350–395 | Spot-verify 10% sample on load | Deterministic stride sampling | Expose mismatch list in `health` / `repair` tool |
| SF-022 | P2 | Performance | `tests/live_index_integration.rs` L585–587 | 1000-file load perf test `#[ignore]` | Not run in CI | Run nightly or lower threshold; track regression |
| SF-023 | P2 | Performance | `tests/coupling_calibration.rs` L19 | Real-repo calibration test `#[ignore]` | `calibrate_against_real_repos` ignored | Document how to run; optional CI nightly |
| SF-024 | P2 | CI | `.github/workflows/ci.yml` | CI runs `cargo check` + `cargo test` only | No `cargo clippy`, `cargo fmt --check`, `cargo build --release` | Add lint/fmt/release jobs per `CLAUDE.md` |
| SF-025 | P2 | CI | `rust-toolchain.toml`, `Cargo.toml` L4 | Rust **edition 2024** + toolchain 1.94.0 | Bleeding-edge edition | Pin rationale in README; verify downstream compiler support |
| SF-026 | P2 | Testing | npm `tests/install.test.js` | Windows launcher E2E smoke **skipped** | "SKIP Windows cannot execute shebang stubs" | Add PE stub or `.cmd` path for Windows CI |
| SF-027 | P2 | MCP | `src/protocol/resources.rs` L15–18 | Resources exist: health, outline, map, uncommitted changes | Matches much of AGENTS "likely resources" | Document URIs in README |
| SF-028 | P2 | MCP | `src/protocol/prompts.rs` | 6 prompts (review, architecture, failure triage, etc.) | `#[prompt(` count 6 | Map prompts to AGENTS "codebase audit / architecture map / failure triage" |
| SF-029 | P2 | MCP | `src/protocol/tools.rs` L3107 | `search_files changed_with=` deprecated; removal v8.x | Deprecation constant + tests | Track v8 removal; update clients |
| SF-030 | P2 | API | `src/protocol/result_status.rs` | Structured `symforge/result_status` metadata (v1) | 6 outcome classes; conformance corpus | Extend to all tools consistently (audit gaps) |
| SF-031 | P2 | Reliability | `src/live_index/store.rs` L449–451, L684–710 | Generation fence rejects stale watcher mutations | `rejected_stale_mutations` counter in health | Document in AGENTS recovery section |
| SF-032 | P2 | Reliability | `src/main.rs` L191–214 | Warm start: load snapshot + background verify | Fast path documented in main | Expose verify progress in health |
| SF-033 | P2 | Watcher | `src/watcher/mod.rs` L312 | Watcher reads file bytes with `fs::read` (binary-safe) | Good for Windows newline issue | Add regression from historical Python bug |
| SF-034 | P2 | Parsing | `src/live_index/store.rs` L85–91 | Per-file `ParseStatus::{Parsed, PartialParse, Failed}` | Failed files isolated | Surface failed file list in compact health |
| SF-035 | P2 | Parsing | `docs/live-code-backlog.md` L114–128 | Vendor SCSS partial parses in health | `expected_vendor_partial_parse_count` in startup state | Implement backlog #6 decision |
| SF-036 | P2 | Search | `src/protocol/tools.rs` L8724–8730 | Frecency deliberately not bumped by search tools | Documented in `conventions` tool text | Good design — keep in AGENTS |
| SF-037 | P2 | Git | `docs/live-code-backlog.md` L28–44 | Windows libgit2 lockfile flake in tests | Backlog #1 | Implement retry in `git/test_helpers.rs` |
| SF-038 | P2 | Sidecar | `src/sidecar/governor.rs` | Concurrency governor: 16 permits, write gate, timeouts | Prevents edit/read races | Tune defaults for large repos |
| SF-039 | P2 | Sidecar | Health output | Hook adoption 55% routed; 700 fail-open (no sidecar) | `health` MCP call on empty index | Improve startup ordering docs for agents |
| SF-040 | P2 | Daemon | `src/daemon.rs` L729–735 | Sessions use `http://127.0.0.1:{port}` | Localhost client assumption | Validate port file + daemon identity (already partially tested) |
| SF-041 | P2 | Init | `src/cli/init.rs` L1670–1684 | Retired `trace_symbol` excluded from allowlists | Explicit regression test | Good — note in migration guide |
| SF-042 | P2 | Init | `tasks/todo.md` | Session task log in repo (not user-facing product) | Historical init bug fix evidence | Move to `.symforge/` or `docs/` if kept |
| SF-043 | P2 | Analytics | `tests/sfb26_analytics_cli.rs` | Analytics disabled by default; no MCP analytics tools | `no_mcp_analytics_tool_is_advertised` | Clear privacy stance in README |
| SF-044 | P2 | Config | `src/parsing/config_extractors/` | CI YAML, TOML, JSON, env, markdown extractors | 15+ `config_files` tests | Expand AGENTS "coding-first" to config intelligence |
| SF-045 | P2 | Edit | `docs/live-code-backlog.md` L148–159 | Inline doc preservation on `replace_symbol_body` edge case | Tests in `tools.rs` L20009+ for `@deprecated` | Verify backlog #8 status |
| SF-046 | P2 | Edit | `src/protocol/edit.rs` | `dry_run` on edit tools (good) | conformance `edit_tools_accept_dry_run_parameter` | Not same as idempotency — see SF-002 |
| SF-047 | P2 | Edit | `tests/edit_safety_tee.rs` | Pre-edit tee snapshots under `.symforge/` (max 20) | Recovery copies for edits | Document restore workflow |
| SF-048 | P2 | Ranking | `src/live_index/frecency.rs` | Optional persistent frecency DB | Policy via `SYMFORGE_FRECENCY` | Document default session vs persistent |
| SF-049 | P2 | Ranking | `src/live_index/coupling/` | Co-change store + generation fence tests | `coupling_refresh_generation_fence` | Monitor store corruption paths (tests exist) |
| SF-050 | P2 | Worktree | `src/worktree.rs` | Git worktree routing for edits/search | `tests/worktree_awareness.rs` | Document `working_directory` param for agents |
| SF-051 | P2 | Observability | `Cargo.toml` L43–44 | `tracing` + env-filter | No OpenTelemetry in repo | Sufficient for local MCP; optional metrics later |
| SF-052 | P3 | Lint | `Cargo.toml` L9–11 | `unsafe_code = deny` globally; targeted `allow(unsafe_code)` in tests/daemon | Good discipline | Periodic audit of allow sites |
| SF-053 | P3 | Dead code | `src/live_index/store.rs` L532 | `#[allow(dead_code)]` on helper | May be future API | Remove or use |
| SF-054 | P3 | Dead code | `src/git/test_helpers.rs` L8, L25 | `allow(dead_code)` test helpers | Test-only | OK |
| SF-055 | P3 | Discovery | `src/discovery/mod.rs` L330 | Comment: `home_dir` deprecated | Uses env vars on Windows | Switch to `dirs` consistently |
| SF-056 | P3 | Vendor | `vendor/tree-sitter-scss/` | Patched crate for MSVC | `Cargo.toml` `[patch.crates-io]` | Track upstream merge |
| SF-057 | P3 | Repo hygiene | `diff.txt` (glob) | Stray `diff.txt` at repo root in file listing | Not in `git status` (clean) | Add to `.gitignore` if generated locally |
| SF-058 | P3 | Conformance | `tests/conformance.rs` L75–120 | Public contract corpus v1 replays tool outcomes | Golden behavioral tests | Expand corpus for ambiguous symbol cases |
| SF-059 | P3 | Smart query | `src/protocol/tools.rs` `ask` | Natural-language `ask` tool wraps intent routing | `smart_query.rs` classifiers | Document vs `explore` boundaries |
| SF-060 | P3 | MCP review | SymForge MCP during review | Workspace index empty unless `index_folder` called | `get_repo_map` / `search_text` returned "Index not loaded" | Agents should index project root first |

---

## By Category

### Correctness & Logic

- Ambiguous symbol auto-resolution (**SF-005**) is the highest-impact logic concern for agents relying on `get_symbol_context` without `path`.
- Daemon `close_session` unwrap (**SF-017**) is unlikely but violates the project's usual `Result` style.
- Generation-fenced stale mutation rejection (**SF-031**) is a strong correctness pattern.
- `ParseStatus::Failed` isolates bad files (**SF-034**) without poisoning the index.

### Security

- Local daemon HTTP is powerful and unauthenticated (**SF-004**); default localhost mitigates, env bind does not.
- Edit trust / project-config hashing (**SF-019**) is a thoughtful local security layer.
- Global `unsafe_code = deny` with narrow exceptions (**SF-052**).
- No hardcoded secrets found in reviewed paths; analytics redaction tested (**SF-043**).

### Reliability & Recovery (idempotency, snapshots, byte-exact storage)

- Byte-oriented indexing and snapshot storage are largely aligned with AGENTS (**SF-020**, **SF-033**).
- Missing idempotency (**SF-002**), checkpoint/repair tools (**SF-003**), and quarantine artifacts (**SF-015**, **SF-016**) are the main vision gaps.
- Clean-shutdown-only persistence (**SF-006**) vs crash recovery via snapshot reload + background verify (**SF-032**).
- Edit tee snapshots (**SF-047**) provide a separate recovery path for writes.

### Performance & Scalability

- God modules (**SF-012**, **SF-013**) risk perf regressions during refactors.
- Ignored perf/calibration tests (**SF-022**, **SF-023**) leave scale regressions undetected in CI.
- Sidecar governor limits concurrency (**SF-038**); batch rename perf test exists (`tests/batch_rename_perf.rs`).

### MCP/API Surface & Backward Compatibility

- 31 primary tools + `health_compact` (**SF-007**, **SF-009**); `trace_symbol` daemon-only alias (**SF-008**).
- v1 operational tools removed with compile-time guards (**SF-003**, `INFR-05` tests).
- Resources + prompts exceed minimal-tools-only design (**SF-027**, **SF-028**).
- `result_status` contract (**SF-030**) aids deterministic clients.

### Indexing & Parsing

- 19 languages (README); tree-sitter + config extractors (**SF-044**).
- Vendor partial-parse hygiene open (**SF-035**).
- Watcher + `analyze_file_impact` path for incremental updates.

### Storage & Persistence

- Postcard snapshot v4, atomic write, version mismatch handling (**SF-020**, **SF-015**).
- Content hash spot-verify (**SF-021**).
- Frecency/co-change SQLite under `.symforge/` (**SF-048**, **SF-049**).

### Testing & Quality Gates

- **1,805** library unit tests passed; broad integration coverage; conformance suite (**SF-058**).
- 2 ignored tests (**SF-022**, **SF-023**); npm Windows skip (**SF-026**).
- CI lacks clippy/fmt/release (**SF-024**).

### Documentation & Planning Drift

- **SF-001**, **SF-009**, **SF-010**, **SF-011**, **SF-014** — AGENTS/CLAUDE/backlog out of sync with v7 implementation.

### CI/CD & Build

- Solid pipeline: conventional commits, version sync, Rust 1.94, npm (**ci.yml**).
- Recommend release build + clippy (**SF-024**, **SF-025**).

### npm/ TypeScript

- Installer/launcher tests thorough (**SF-026** exception).
- Version-aligned with crate 7.13.0.

### Deferrals & TODOs (inventory)

- No production `TODO`/`FIXME` in `src/` (only test fixtures and comment filters).
- Planning deferrals live in `docs/live-code-backlog.md` (14+ items).
- `tasks/todo.md` is historical session evidence (**SF-042**).

### Dead Code & Unused Abstractions

- Minimal `allow(dead_code)` (**SF-053**, **SF-054**).
- Retired MCP names only in daemon alias + tests (**SF-008**).

---

## Broken or Failing Now

- **None observed** in verification: `cargo check` OK; `npm test` OK (1 skip); all completed `cargo test` binaries OK.
- **In progress:** full `cargo test --all-targets` was still running `sidecar_integration` (27 tests) at report time — no failures in prior output.
- **SymForge MCP:** index not loaded in review session (**SF-060**) — operational note, not product bug.

---

## Missing vs Vision (AGENTS.md)

| AGENTS vision | Current state |
|---------------|---------------|
| `index_repository` | **`index_folder`** only |
| `repair_index`, `checkpoint_now`, `cancel_index_run`, `get_index_run` | **Removed** (INFR-05); no replacement run lifecycle |
| `invalidate_cache` | No dedicated tool (partial via re-index / `analyze_file_impact`) |
| `get_repo_outline`, `get_file_outline`, `get_symbols` | **`get_repo_map`**, **`get_file_context`**, batch via **`get_symbol`** |
| Idempotency keys on mutating tools | **Not implemented** |
| Quarantine bad parses/spans | **Failed status only**, no quarantine store |
| `application/`, `storage/`, `indexing/` modules | **Not present** — flatter layout |
| Prompts: audit, architecture, triage, repair diagnosis | **Partially met** (6 prompts; no "index repair diagnosis" prompt name) |
| Resources: repo health, outline, run status | **Health/outline/map/changes**; no index-run resource |

---

## Positive Observations

- **Exceptional test depth:** conformance suite, edit safety, capability evidence, generation fences, sidecar contract goldens.
- **Byte-exact discipline** in indexing and snapshots (`Vec<u8>`, `fs::read`, content hashes).
- **Explicit outcomes** via `result_status` and edit `dry_run`.
- **Tool consolidation** pattern documented in `CLAUDE.md` with daemon aliases and init tests for retired tools.
- **Local-first architecture** delivered: LiveIndex, watcher, optional daemon, sidecar hooks, worktree awareness.
- **Security-minded edits:** project config trust with hash evidence and enforce mode.
- **npm packaging** handles cross-platform binary install/update with solid JS tests.

---

## Recommended Planning Phases

1. **Documentation alignment (low risk)** — Update `AGENTS.md`, `CLAUDE.md`, refresh `docs/live-code-backlog.md` (close done items). *Depends on nothing.*
2. **Daemon hardening** — Bind policy, optional auth token, document threat model (**SF-004**). *Before any remote/LAN use.*
3. **Idempotency + run lifecycle** — Design store for `index_folder`/edits; decide whether to restore checkpoint/repair tools or update AGENTS permanently (**SF-002**, **SF-003**).
4. **Ambiguous symbol correctness** — Fix `get_symbol_context` resolution + conformance cases (**SF-005**).
5. **Module decomposition** — Split `tools.rs` / `query.rs` without behavior change (**SF-012**, **SF-013**).
6. **Recovery artifacts** — Quarantine corrupt snapshots; optional `repair_index` exposing verify mismatches (**SF-015**, **SF-021**).
7. **CI hardening** — `fmt`, `clippy`, `build --release`, nightly ignored perf tests (**SF-024**, **SF-022**).

---

## Appendix

### Commands run and exit codes

| Command | Exit | Notes |
|---------|------|-------|
| `cargo check` (e:\project\symforge) | 0 | ~339s total |
| `cargo test --all-targets -- --test-threads=1` | In progress / partial | Lib tests: 1805 ok; many integration ok; still on `sidecar_integration` |
| `cd npm && npm test` | 0 | 27 pass, 1 skip (Windows stub) |

### Key files reviewed

`AGENTS.md`, `CLAUDE.md`, `Cargo.toml`, `.github/workflows/ci.yml`, `src/main.rs`, `src/lib.rs`, `src/daemon.rs`, `src/cli/init.rs`, `src/protocol/tools.rs` (samples), `src/protocol/resources.rs`, `src/protocol/prompts.rs`, `src/protocol/result_status.rs`, `src/live_index/persist.rs`, `src/live_index/store.rs`, `src/watcher/mod.rs`, `src/discovery/mod.rs`, `tests/conformance.rs`, `tests/live_index_integration.rs`, `docs/live-code-backlog.md`, `README.md`, npm tests.

### Areas not fully reviewed (reason)

- **Every line** of `protocol/tools.rs` (~21k lines) and `live_index/query.rs` — sampled + SymForge-oriented grep/tests.
- **All language parsers** under `src/parsing/languages/` — spot-checked architecture only.
- **`cargo build --release`** — not run (time); recommended before release.
- **Full `cargo test` completion** — long-running sidecar suite; zero failures in captured output.
- **Production deployment / threat modeling** — local MCP assumed; no external pen test.
