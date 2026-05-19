---
title: RTK SymForge Integration Goal Index
goal_chain: rtk-symforge-integration
target_branch: "rtk-symforge-integration"
base_branch: "main"
status: "Queued"
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK SymForge Integration Goal Index

Use each task file directly with `/goal`, for example:

    /goal .agent/goals/rtk-symforge-integration/RTK01-automod-config-extractors.md

The task files are dependency sorted and use the same workflow shape as the supplied NCS example: frontmatter status tracking, branch guard, dependency guard, mini-spec, verification command, task prompt, and final report format.

## Target branch

- Target branch: `rtk-symforge-integration`
- Base branch: `main`
- If you want to run directly on another branch, edit the `target_branch` field and Branch Guard text consistently before dispatching the goals.

## Wave gates

After each wave, run the project gate before dispatching the next wave:

```bash
cargo check && cargo test --all-targets -- --test-threads=1 && cargo build --release
```

## Goals

### Wave A - trivial parallelizable hygiene

- `RTK01` — [Automod for config extractors](RTK01-automod-config-extractors.md) — depends_on: none
- `RTK02` — [Compression ratio CI assertion](RTK02-compression-ratio-ci-assertion.md) — depends_on: none
- `RTK03` — [Strict Rust lints policy](RTK03-strict-rust-lints-policy.md) — depends_on: none
### Wave B - foundation

- `RTK04` — [Inline extractor test framework](RTK04-inline-extractor-test-framework.md) — depends_on: RTK01
- `RTK05` — [ADR 0015 for RTK `.symforge` trust gate](RTK05-adr-0015-rtk-symforge-trust-gate.md) — depends_on: none
- `RTK06` — [Trust core module and tests](RTK06-trust-core-module-and-tests.md) — depends_on: RTK05
- `RTK07` — [Trust daemon and user control surface](RTK07-trust-daemon-and-user-control-surface.md) — depends_on: RTK06
- `RTK08` — [Hash-sidecar integrity pattern](RTK08-hash-sidecar-integrity-pattern.md) — depends_on: RTK06
### Wave C - behavior layer

- `RTK09` — [Tier-2 metadata lookup helpers](RTK09-tier2-metadata-lookup-helpers.md) — depends_on: none
- `RTK10` — [Graceful degradation tool behavior](RTK10-graceful-degradation-tool-behavior.md) — depends_on: RTK09
- `RTK11` — [Structural-search pattern compile cache](RTK11-structural-search-pattern-compile-cache.md) — depends_on: none
- `RTK12` — [Frecency read-path store reuse](RTK12-frecency-read-path-store-reuse.md) — depends_on: none
### Wave D - product decision

- `RTK13` — [Analytics product decision](RTK13-analytics-product-decision.md) — depends_on: none
### Wave E - gated implementation

- `RTK14` — [Analytics storage foundation](RTK14-analytics-storage-foundation.md) — depends_on: RTK13
- `RTK15` — [Analytics instrumentation and reporting](RTK15-analytics-instrumentation-and-reporting.md) — depends_on: RTK14
- `RTK16` — [Stateless same-file correction suggestions](RTK16-stateless-same-file-correction-suggestions.md) — depends_on: none
- `RTK17` — [Analytics-trained correction learning](RTK17-analytics-trained-correction-learning.md) — depends_on: RTK15, RTK16
### Wave F - evidence-gated audit follow-up

- `RTK18` — [Config extractor registry cleanup evaluation](RTK18-config-extractor-registry-cleanup-evaluation.md) — depends_on: none
- `RTK19` — [Worktree feature flag caching evaluation](RTK19-worktree-feature-flag-caching-evaluation.md) — depends_on: none
- `RTK20` — [Tree-sitter parser reuse investigation](RTK20-tree-sitter-parser-reuse-investigation.md) — depends_on: none
- `RTK21` — [Regex glob and Aho-Corasick cache investigation](RTK21-regex-glob-aho-corasick-cache-investigation.md) — depends_on: none

## Out of scope

- Do not add SymForge runtime coupling to RTK. This sprint cherry-picks proven Rust idioms and patterns only.
- Do not reopen settled items: `panic = "abort"`, RTK build-time `.scm` embedding, `match_output`, shell lexer, telemetry HTTP ping, OpenClaw plugin, Homebrew packaging, or `RegexSet` replacement without benchmark evidence.
- Do not add forbidden dependencies: `lazy_static`, `ureq`, `flate2`, `quick-xml`, `which`, or `getrandom`.
- Do not log provider credentials, secrets, `.env` contents, private keys, raw query text, or unbounded source blobs.
- Do not silently convert missing, stale, degraded, disabled, blocked, or unknown data into success.
- Do not modify unrelated roadmap, release, frontend, npm, or vendored files unless this goal explicitly names them.
