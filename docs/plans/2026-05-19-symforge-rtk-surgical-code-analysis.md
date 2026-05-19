---
title: SymForge RTK surgical code analysis
type: code-analysis
status: ready-for-review
date: 2026-05-19
repos:
  symforge: special-place-administrator/symforge @ main
  rtk: rtk-ai/rtk @ develop
---

# SymForge RTK surgical code analysis

## Position

This is not an RTK integration plan. SymForge should not depend on RTK and should not be converted into an RTK-style CLI output filtering system. The useful overlap is selective adoption of small patterns that improve SymForge’s existing MCP code-intelligence product.

## Method note

A direct local `git clone` attempt was blocked by DNS resolution in this environment. The analysis therefore used the GitHub connector and browser-visible GitHub content to inspect current repository files, plus the uploaded planning and code-overlap reports. The generated goals include code-evidence checkpoints so the executing agent must re-check the local worktree before editing.

## SymForge code facts that changed the task list

- `src/observability.rs` already exists and `src/lib.rs` already exposes `pub mod observability;`. Any analytics implementation, if ever accepted, must refactor the file into `src/observability/mod.rs` first; it must not blindly create a conflicting `src/observability/` tree.
- `src/edit_safety/tee.rs` already gives SymForge a pre-edit snapshot mechanism with a 20-file retention cap and 1 MiB max file size. RTK tee mode/config is not imported by default.
- `src/hash.rs` already provides SHA-256 helpers. Trust/integrity work should reuse that seam.
- `src/parsing/languages/mod.rs` already uses `automod::dir!()`, while `src/parsing/config_extractors/mod.rs` still has five manual module declarations. This is a small real task.
- `src/parsing/mod.rs` currently constructs a fresh tree-sitter parser in `parse_source`. Inline tests must use this entry point, not create another parser path.
- Admission tiers and skipped-file metadata already exist (`AdmissionTier`, `SkippedFile`, and `LiveIndex.skipped_files`). Graceful degradation should use these existing structures rather than import RTK concepts wholesale.
- Frecency already uses SQLite WAL/busy-timeout and has no-footprint read-only open behavior. The frecency “OnceLock” task is now an audit/possible closure, not a default implementation task.
- Structural search currently compiles the ast-grep pattern per candidate file, so a request-scoped compile investigation is legitimate. Global user-pattern caching is not.
- `resolve_or_error` currently returns plain symbol-not-found/ambiguous text and does not include same-file suggestions. Stateless suggestions are a useful independent UX improvement.

## RTK patterns worth borrowing

- Strict crate lints: `unsafe_code = "deny"`, `warnings = "deny"`, after a preflight check.
- Trust model shape: four-state trust status, precomputed hash recording, data-local trust store, CI-gated override, and canonical path keys.
- SQLite operational patterns: WAL, 5-second busy timeout, GLOB project scoping, 90-day retention. These remain analytics-decision inputs, not implementation defaults.
- Correction-learning constants and filters: `CORRECTION_WINDOW = 3`, `MIN_CONFIDENCE = 0.6`, user-rejection filtering. Use stateless same-file suggestions first.

## RTK surfaces rejected for SymForge

Do not import RTK shell hooks, hook installer, Claude permission parser, command rewriter, shell lexer, CLI output filters, OpenClaw plugin, Homebrew formula, HTTP telemetry, `panic = "abort"`, `lazy_static`, or RegexSet replacement. These either solve RTK-specific command-proxy problems or are already superseded by SymForge code.

## Revised task classification

### Ready now

- SRTK01 automod config extractors
- SRTK02 compression ratio regression test
- SRTK03 strict lint preflight/policy
- SRTK05 ADR 0015 trust design
- SRTK08 Tier metadata lookup helpers
- SRTK10 stateless same-file symbol suggestions
- SRTK13 analytics product decision only

### Run after dependencies

- SRTK04 inline language test macro after SRTK01
- SRTK06 trust core after SRTK05
- SRTK07 trust control surface after SRTK05/SRTK06
- SRTK09 graceful degradation handlers after SRTK08

### Evidence-gated

- SRTK11 structural search compile hotspot investigation
- SRTK12 frecency read-path no-footprint audit

### Hold / decision only

- SRTK14 integrity sidecar scope decision. RTK needs this for an installed hook script. SymForge should defer unless `.symforge/` gains executable or security-sensitive behavior.

## Removed from the previous broad backlog

The previous 21-file backlog treated too many RTK surfaces as implementation candidates. This revised set removes analytics implementation, analytics instrumentation, analytics-trained learning, config registry micro-optimization, worktree env caching, tree-sitter parser pooling, regex/glob/Aho-Corasick cache, and integrity sidecar implementation from the active default chain. Those can return only with product approval or benchmark evidence.
