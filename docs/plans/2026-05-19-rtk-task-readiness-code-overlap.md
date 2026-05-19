# RTK Task Readiness and Code Overlap Sweep

Date: 2026-05-19
Scope: compare the copied RTK goal chain in `.agent/goals/rtk-symforge-integration/`, the latest planning report at `docs/plans/2026-05-19-rtk-integration-state-for-planning.md`, the vault concept note `wiki/concepts/RTK Techniques for SymForge.md`, and current SymForge code.

## Executive Summary

The generated RTK goal chain is directionally useful, but it should be treated as a planning backlog, not as an implementation order to execute blindly.

The high-confidence near-term work is:

1. RTK01: automod for config extractors.
2. RTK02: compression ratio regression test.
3. RTK03: strict Rust lints, after first checking current warning load.
4. RTK04: inline extractor test framework, after RTK01.
5. RTK05 first, then RTK06/RTK07/RTK08 only after ADR 0015 resolves the trust-control surface.
6. RTK09 then RTK10 for graceful degradation, because current tier labels exist but the handler-level behavior is not implemented.
7. RTK16 as a standalone quality improvement, because it does not require analytics.

The work that needs more product or benchmark evidence before implementation is:

- RTK11, RTK12, RTK18, RTK19, RTK20, RTK21: perf/cleanup investigations only; implement only with measured evidence.
- RTK13-RTK15 and RTK17: analytics-gated. Do not build persistent analytics until RTK13 explicitly accepts the product decision.

The core product conclusion from the latest report still holds: SymForge should not depend on RTK. The useful overlap is unidirectional adoption of small Rust patterns: automod, hash/trust models, WAL SQLite conventions, bounded degradation, lints, and evidence-gated caching.

## Current Code Baseline

- SymForge MCP index status during this sweep: ready; 403 files indexed, 399 parsed, 4 partial, 0 failed.
- Current branch: `main`.
- `.agent/` is ignored by `.gitignore`, so the copied goal folder must be staged with `git add -f`.
- The copied goals target a future `rtk-symforge-integration` branch for execution. This sweep only commits planning artifacts and copied task files; it does not execute those implementation goals on `main`.

## Readiness Matrix

| Goal | Current Code Evidence | Overlap | Readiness | Disposition |
| --- | --- | --- | --- | --- |
| RTK01 automod config extractors | `src/parsing/languages/mod.rs:1` already uses `automod::dir!`; `src/parsing/config_extractors/mod.rs:1-5` still has five manual `pub mod` declarations; `Cargo.toml:22` already has `automod = "1"`. | Direct, tiny. | Ready now. | Keep as first Wave A task. |
| RTK02 compression ratio CI assertion | Token savings exist in `src/sidecar/mod.rs:53-89` and `src/protocol/format.rs:3666-3694`; existing tests check token-savings footers but no fixed ratio corpus exists (`tests/persist_compression_ratio.rs` absent). | Direct test-gap. | Ready now. | Keep, but make it a test-only gate against `get_file_context`, not a runtime feature. |
| RTK03 strict Rust lints | `Cargo.toml` has no `[lints.rust]`; no current `unsafe_code = "deny"` or `warnings = "deny"` block. CI currently runs cargo check/tests rather than a dedicated fmt/clippy lint gate. | Direct policy adoption. | Ready after preflight. | Keep. First run `cargo check --all-targets` without the lint change to measure warning debt. If the intent is a real CI lint gate, add `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings` in the same task or a follow-up. |
| RTK04 inline extractor tests | `src/parsing/inline_tests.rs` is absent; `parse_source` is the current parser entry point in `src/parsing/mod.rs:188-223`; language tests still hand-roll parsers in many modules. | Good fit, but not config-extractor-specific. | Ready after RTK01. | Keep, but design it as a small test macro over existing `parse_source`; do not introduce another parser path. |
| RTK05 ADR 0015 trust gate | `src/edit_safety/mod.rs:1` exports only `tee`; `src/edit_safety/trust.rs` and `docs/decisions/0015-rtk-trust-gating-symforge-config.md` are absent; prior "trust gate" language in changelog refers to search/context trust, not `.symforge/` config trust. | Strong architecture need. | Ready now as documentation. | Keep. This must precede RTK06-RTK08. |
| RTK06 trust core module | `src/hash.rs:1-16` already has SHA-256 helpers; `src/edit_safety/tee.rs` gives sibling edit-safety style; no trust store/status module exists. | Strong fit, but security-sensitive. | Ready only after RTK05. | Keep after ADR. Use four-state model and `dunce` normalization from the report. |
| RTK07 trust daemon/control surface | Daemon routing exists in `src/daemon.rs`; trust mode/control surface is undecided; no CLI or MCP trust commands exist. | Plausible but surface-dependent. | Blocked by RTK05/RTK06. | Keep, but do not implement until ADR chooses CLI vs MCP control surface and LOG_ONLY/ENFORCE semantics. |
| RTK08 hash-sidecar integrity | `sha2` is already available and `src/hash.rs` exists; persisted snapshots already carry content hashes; no `src/edit_safety/integrity.rs` or sidecar status enum exists. | Useful later, not required for current config. | Blocked by RTK05/RTK06. | Bundle with trust design, but consider deferring implementation until `.symforge/` gains trusted executable behavior. Do not add integrity checks to hot sidecar port-file reads. |
| RTK09 Tier-2 metadata lookup helpers | Admission tiers exist in `src/domain/index.rs:469-520`; skipped files are stored in `src/live_index/store.rs:402-405`; health renders tier counts; no focused helper API for handlers exists. | Direct behavior foundation. | Ready. | Keep before RTK10. |
| RTK10 graceful degradation behavior | Tier labels and daemon degraded modes exist; `get_symbol_context` and `find_references` still use symbol/reference lookups and do not have tested Tier-2/Tier-3 response branches; `tests/graceful_degradation.rs` absent. | Direct behavior gap. | Ready after RTK09. | Keep. Must preserve Tier-1 response shape and add explicit degraded output only where evidence exists. |
| RTK11 structural-search compile cache | `src/live_index/search.rs:1182-1358` calls `ast_grep::structural_search` per candidate; `src/parsing/ast_grep.rs:142-151` compiles the pattern each call. | Real overlap, but perf claim unmeasured. | Investigation first. | Keep as evidence-gated. Prefer per-request cache only if instrumentation proves repeated compile cost. |
| RTK12 frecency read-path store reuse | `src/live_index/frecency.rs:429-482` already reuses cached writer handles and opens existing DB read-only; persistent/session caches exist at `src/live_index/frecency.rs:484-544`. | Partly already covered. | Investigation first. | Likely close as already mostly done unless repeated read-only opens are measured. Preserve no-DB-creation invariant. |
| RTK13 analytics product decision | `src/observability.rs` only initializes tracing; `TokenStats` is in-memory only; no persistent analytics ADR exists. | Product question, not coding gap yet. | Ready as decision artifact. | Keep. This is the gate for RTK14/RTK15/RTK17. |
| RTK14 analytics storage foundation | `rusqlite` is already in `Cargo.toml:68`; WAL/busy-timeout pattern exists in frecency (`src/live_index/frecency.rs:81-95`); no analytics DB/schema module exists. | Technically straightforward, product-gated. | Blocked by RTK13. | Do not implement before analytics is accepted. If accepted, reuse WAL/busy-timeout/GLOB patterns. |
| RTK15 analytics instrumentation/reporting | `TokenStats` tracks per-session counts/tokens in memory (`src/sidecar/mod.rs:72-89`), but no persistent rows, RAII timer, export, failures-only report, or reset surface exists. | Partial instrumentation only. | Blocked by RTK13/RTK14. | Keep only if RTK13 accepts persistent analytics. |
| RTK16 stateless same-file correction suggestions | Similar path suggestions exist for file-not-found in `src/protocol/tools.rs:1288-1325`; fuzzy symbol suggestions exist in formatting helpers; edit resolver failures still return plain symbol-not-found style errors without same-file `did_you_mean` suggestions. | Good independent UX improvement. | Ready after focused tests. | Keep. This is the useful MVP even if analytics is rejected. Reuse existing fuzzy suggestion style but cap it to same-file, top-three suggestions. |
| RTK17 analytics-trained correction learning | No persistent analytics/failure corpus exists; there are current static suggestions but no training loop. | Depends on analytics. | Blocked by RTK13-RTK16. | Defer. Do not build until analytics exists and proves value. |
| RTK18 config extractor registry cleanup | Current registry allocates boxed stateless extractors in `src/parsing/config_extractors/mod.rs:72-85`; OnceLock audit flagged it medium-low hot path. | Micro-optimization. | Evidence-gated. | Likely close N/A unless allocation shows up in profiling. |
| RTK19 worktree feature flag caching | Worktree list caching already exists in `src/worktree.rs:112-186`; policy intentionally resolves from env at call time in `src/worktree.rs:359-375` and `src/worktree.rs:377-388`. | Mostly covered or intentionally uncached. | Evidence-gated. | Likely reject caching policy reads to preserve call-time behavior and testability. |
| RTK20 tree-sitter parser reuse | `parse_source` constructs a fresh `Parser` per parse in `src/parsing/mod.rs:188-223`; query compilation is already cached in `src/parsing/xref.rs:356-370`. | Real possible perf work, higher risk. | Benchmark first. | Keep as investigation only. Implement thread-local parser reuse only with clear benchmark win and no unsoundness. |
| RTK21 regex/glob/Aho-Corasick cache | Regex compile, glob compile, whole-word matcher, and Aho-Corasick automata are dynamic in `src/live_index/search.rs:914-1031`; Aho-Corasick already satisfies the old RegexSet goal. | Possible perf work; unbounded-cache risk. | Benchmark first. | Keep as investigation only. No cache without bounded eviction and repeated-query evidence. |

## Recommended Execution Shape

Wave A:

- Execute RTK01, RTK02, RTK03 independently.
- For RTK03, run `cargo check --all-targets` before changing `Cargo.toml`. If warning debt exists, either fix narrowly or split the lint rollout.

Wave B:

- Execute RTK04 after RTK01.
- Execute RTK05 before any trust code.
- Execute RTK06 after RTK05.
- Execute RTK07 and RTK08 only after ADR 0015 settles control-surface and integrity scope.

Wave C:

- Execute RTK09, then RTK10.
- Run RTK11 and RTK12 as investigations. They should produce either a measured patch or an explicit N/A/defer note.

Wave D/E:

- Execute RTK13 as a decision.
- Execute RTK14/RTK15 only if RTK13 accepts persistent local analytics.
- Execute RTK16 independently; it should not wait for analytics.
- Execute RTK17 only after analytics exists and RTK16 is stable.

Wave F:

- Treat RTK18-RTK21 as evidence-gated audits. The default outcome can be "close/defer with evidence"; implementation is not required unless measurements justify it.

## What Not To Import From RTK

Do not import RTK's shell hooks, shell lexer, command rewriting, CLI output filters, OpenClaw plugin, Homebrew packaging, HTTP telemetry, `panic = "abort"`, `lazy_static`, or RegexSet replacement. These either do not match SymForge's MCP/server product shape or are already superseded by current SymForge code.

## Verification Evidence

Commands and tools used:

- SymForge MCP `health` then `index_folder` for `E:\project\symforge`.
- SymForge MCP `get_repo_map`, `search_text`, `search_files`, and `inspect_match` for code evidence.
- agentmemory recall for prior RTK context: no matching memories found.
- remindb-vault search/fetch for `wiki/concepts/RTK Techniques for SymForge.md`.
- `git status --short --ignored` to confirm `.agent/` is ignored and must be force-staged.

No source code was changed by this sweep.
