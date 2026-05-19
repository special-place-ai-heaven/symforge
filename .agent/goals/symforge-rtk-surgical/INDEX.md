---
title: SymForge RTK surgical goal index
type: goal-index
status: ready-for-review
target_branch: symforge-rtk-surgical
---

# SymForge RTK Surgical Goal Index

Use these as selective SymForge enhancement tasks. They are not an RTK integration project and they intentionally omit RTK shell hooks, command filters, telemetry, OpenClaw, Homebrew, `panic = "abort"`, and `lazy_static`.

## Run order

- **SRTK01** — [automod config extractors](SRTK01-automod-config-extractors.md)
  Phase: Wave A - low-risk hygiene · Depends on: none

- **SRTK02** — [compression ratio regression test](SRTK02-compression-ratio-regression-test.md)
  Phase: Wave A - test-only quality gate · Depends on: none

- **SRTK03** — [strict Rust lint preflight and policy](SRTK03-strict-rust-lint-preflight-and-policy.md)
  Phase: Wave A - policy hygiene · Depends on: none

- **SRTK04** — [inline language test macro for Rust and Python](SRTK04-inline-language-test-macro-for-rust-and-python.md)
  Phase: Wave B - parser test foundation · Depends on: SRTK01

- **SRTK05** — [ADR 0015 project config trust design](SRTK05-adr-0015-project-config-trust-design.md)
  Phase: Wave B - trust decision before code · Depends on: none

- **SRTK06** — [trust core pure module](SRTK06-trust-core-pure-module.md)
  Phase: Wave B - minimal trust implementation · Depends on: SRTK05

- **SRTK07** — [trust control surface and warning envelope](SRTK07-trust-control-surface-and-warning-envelope.md)
  Phase: Wave B - trust integration · Depends on: SRTK05, SRTK06

- **SRTK08** — [Tier metadata lookup helpers](SRTK08-tier-metadata-lookup-helpers.md)
  Phase: Wave C - graceful degradation foundation · Depends on: none

- **SRTK09** — [graceful degradation handlers](SRTK09-graceful-degradation-handlers.md)
  Phase: Wave C - behavior layer · Depends on: SRTK08

- **SRTK10** — [stateless same-file symbol suggestions](SRTK10-stateless-same-file-symbol-suggestions.md)
  Phase: Wave C - independent UX improvement · Depends on: none

- **SRTK11** — [structural search compile hotspot investigation](SRTK11-structural-search-compile-hotspot-investigation.md)
  Phase: Wave C - evidence-gated perf · Depends on: SRTK03

- **SRTK12** — [frecency read-path no-footprint audit](SRTK12-frecency-read-path-no-footprint-audit.md)
  Phase: Wave C - evidence-gated perf · Depends on: none

- **SRTK13** — [analytics product decision only](SRTK13-analytics-product-decision-only.md)
  Phase: Wave D - product gate · Depends on: none

- **SRTK14** — [integrity sidecar scope decision](SRTK14-integrity-sidecar-scope-decision.md)
  Phase: Hold - trust-adjacent decision · Depends on: SRTK05

## Active vs gated

- **Ready now:** SRTK01, SRTK02, SRTK03, SRTK05, SRTK08, SRTK10, SRTK13.

- **Depends on earlier design/code:** SRTK04 after SRTK01; SRTK06 and SRTK07 after SRTK05; SRTK09 after SRTK08.

- **Evidence-gated:** SRTK11 and SRTK12 may close with no source patch if measurement shows no useful work.

- **Hold/decision only:** SRTK14 decides whether integrity sidecars are needed; implementation is not included by default.

## Explicitly not scheduled

- RTK hook installer, Claude permission parser, shell command rewriter, shell lexer, CLI output filters, OpenClaw plugin, Homebrew packaging, HTTP telemetry, `panic = "abort"`, RegexSet replacement, `lazy_static`, config-extractor registry micro-optimization, worktree env caching, tree-sitter parser pool, and regex/glob/Aho-Corasick global caches.
