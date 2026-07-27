# Quickstart / Validation: Knowledge LLM Sift

How to prove this slice works. Structures are in [data-model.md](data-model.md); decisions in
[research.md](research.md). No implementation code here.

## Prerequisites

- Windows: artifacts go to `target/` on **E:** via the repo `.cargo/config.toml`. Do **not** set
  `CARGO_TARGET_DIR` on the command line (an older handoff suggested `C:/symforge-target`, which
  filled C:).
- Run `cargo clean` after heavy local runs (CLAUDE.md Windows disk rule).

## 1. Baseline capture (must happen BEFORE any code change)

The slice's headline claims are *comparative*. Capture the "before" or they are unprovable.

```powershell
cargo test --test search_knowledge -- --test-threads=1
```

Record, for the release-please probe and the persistence-boundary probe:

- total response **bytes**
- **line number** of the first excerpt
- response at `max_tokens=300` and `max_tokens=120`
- `review_knowledge(mode=summary)` counts: `domain=unknown`, `voice=unknown`, `suppressed`

2026-07-27 reference baseline (Kimi dogfood): total=5142, unknown domain=3894, voice unknown=3767,
broken_anchor=2979, review_due=2163, duplicate_units=35, `suppressed`=0.

### Measured pre-slice baseline — captured 2026-07-27 at commit `83b6b32`

Live `search_knowledge("release please squash merge", limit=10)` against this repository
(publication=230, content=143). Raw capture: scratchpad `baseline-releaseplease.txt`.

| Metric | Baseline | Target |
|---|---|---|
| Envelope before hit 1 | **872 bytes / 6 lines** | ≤ ~4 lines |
| First excerpt line | 7 | ≤ ~8 **and answering** |
| Rank of the canonical answer (`CLAUDE.md:39`) | **7 of 10** | top 3 |
| Hit shape | 1 mega-line each, ~700–1200 bytes | 5-line block |
| Distinct paths in 10 hits | **7** (3 files supply 6 hits) | ≥9 |

Observed defects, all reproduced in this single response:

- **Flooding (WS4)**: `KIMI-REVIEW-RESPONSE…md` ×2, `KIMI-REVIEW…md` ×2, `013-HANDOFF.md` ×2 — six of ten
  hits from three files, pushing the canonical `CLAUDE.md` policy to rank 7 behind documents that
  merely *quote* the query.
- **Self-pollution (WS4)**: ranks 5 and 6 are `sift/quickstart.md` and `sift/spec.md` — files written
  minutes earlier in this session, outranking the policy they describe.
- **`SymbolId` debug leak (WS1.5)**: ranks 2 and 4 render
  `symbol:SymbolId { path: "src/protocol/mod.rs", name: "explore", kind: Module }:8`.
- **Table-row excerpts (WS1.6)**: ranks 3 and 4 have whole Markdown table rows as excerpts; rank 2's
  excerpt begins mid-sentence with leading whitespace.
- **Unbounded IDs (WS1.3)**: the `Source:` line carries two full 64-hex digests; `provenance_ids` and
  `bridge_previews` render 64-hex IDs throughout.
- **Unevidenced authority (WS2)**: `lifecycle=active domain=unknown … voice=unknown` on 6 of 10 hits —
  cost with no decision value, and `active` is asserted without evidence.

### Measured WS2 authority movement — captured 2026-07-27

Counted directly over the published `KnowledgeAuthorityView.records` — the same
`generation.authority.records` collection `review_knowledge(mode=summary)` aggregates
(`src/protocol/knowledge_review.rs:387`) — via a throwaway harness on this repository, before
(`ffe91c0`) and after (`f183066`) the WS2 change. Identical corpus: 5304 units both times.

| Count | Before | After | Delta |
|---|---:|---:|---:|
| `suppressed` | 0 | 0 | **0** (SC-006 gate) |
| `history_only` | 130 | 130 | 0 |
| `domain=unknown` | 4030 | 435 | −3595 |
| `voice=unknown` | 3903 | 2427 | −1476 |
| `voice=intent` | 523 | 2747 | +2224 |
| `voice=needs_review` | 748 | 0 | −748 |
| `voice=current` | 0 | 0 | 0 |

Diagnostics, not gates:

- `voice=needs_review` collapsing to 0 is the lifecycle change: `derive_voice` tests
  `lifecycle == Unknown` before it reads the code-evidence display, so an unevidenced unit with
  review-worthy evidence now reports `unknown` rather than `needs_review`. Both voices are
  admitted by the `default` and `current` scopes, so nothing is hidden, but the review signal is
  no longer distinguishable from absent classification.
- `voice=intent` rising by 2224 is the path table: `plan`/`plans`/`roadmap`/`spec` units are now
  normative intent. `intent` is admitted by `default` but **excluded from
  `authority_scope=current`** by the frozen contract, so those units are visible by default and
  deliberately absent from a `current`-scoped answer.
- `history_only` is unchanged, so the new `research`/`dogfood`/`reviews`/`archived`
  `HistoricalRecord` domain assignments did not move any unit into the history-only voice.

## 2. Per-workstream gates

Each workstream is RED → GREEN → VERIFY. A workstream is not done until its own tests pass **and**
the previous workstreams' tests still pass.

| WS | Red proof required first | Green criterion |
|---|---|---|
| **WS0** | Two-source fixture where per-source truncation returns `2 × limit` hits and two count sets. | Global top-N with one aggregate count set; real `worktree:`/`ref:` labels; contract tests 9 and 11 green. |
| **WS1** | Existing CCR test shown to pass **vacuously** on mega-lines; new block-completeness assertion fails. | Answer-first blocks; excerpt by ~line 8; ≤60% baseline bytes; `max_tokens=300` → header + ≥1 complete block + handle; `max_tokens=120` → provenance + handle, zero partial blocks; CCR round-trip byte-identical; contract tests 3, 7, 18 green. |
| **WS2** | Fixture asserting a no-evidence unit currently reports `Active`; overmatch fixture asserting `docs/special/x.md` currently classifies via `/spec`. | `Unknown` lifecycle without evidence; heading beats path; component tokens only; `suppressed` delta is **zero**; active `research/`+`docs/dogfood/` content still visible at `authority_scope=default`. |
| **WS3** | Each new prose cue currently falls to `Explore`. | Cues route to `search_knowledge`; `find references to X`, `where is search_knowledge defined`, `retry policy in the client code` stay code-routed; generic `how does X work` unchanged. |
| **WS4** | Real-corpus fixture where >2 hits from one file outrank a distinct-file canonical hit — **observed red**. | Distinct-file hit promoted; single-file corpus not underfilled; no hit dropped; one-term hits never promoted over full-coverage hits. |

## 3. Contract gates (the frozen set)

These must be green at the end and are the slice's non-negotiable floor:

```powershell
cargo test --test search_knowledge -- --test-threads=1
```

Covering frozen `contracts/search-knowledge.md` tests **3, 7, 8, 9, 11, 16, 18**:

| # | What it protects |
|---|---|
| 3 | Prose hit carries exact path/line/heading/hash/generation |
| 7 | Truncation retains complete provenance **and** a CCR handle |
| 8 | Ranking is byte-for-byte deterministic over repeated equal generations |
| 9 | Current worktree ranks ahead of a divergent ref **without hiding it** |
| 11 | `source_scope=all` returns per-source generations/digest/coverage/freshness + worst overall |
| 16 | Scope sets stay distinct; filtered matches yield `evidence_noncurrent`, not false no-evidence |
| 18 | Link IDs + exact/declared-set/ambiguous/missing previews survive formatting, truncation, envelopes, CCR |

## 4. Full verification gate (CLAUDE.md / Constitution VIII)

```powershell
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed   # Constitution VI (embed isolation)
```

No npm change in this slice, so `npm test` is not required.

## 5. Manual dogfood (before/after, required)

Re-run the Kimi probe battery against the built server and paste before/after into the PR:

1. `search_knowledge("release please squash merge")` — the canonical policy (`CLAUDE.md`
   §"Merging PRs") should be reachable, and the review brief that merely quotes it should not take
   ranks 1–2 alone (WS4).
2. `search_knowledge("why is shutdown not a persistence boundary")` — the contract's own example;
   check same-file flooding is reduced.
3. Both queries at `max_tokens=120` and `max_tokens=300`.

**Measure**: response bytes, line number of the first excerpt, and whether any hit block is partial.
Do **not** report a token-benchmark estimate — measure bytes and answer position.

## 6. Cleanup

```powershell
cargo clean
```

Then confirm: branch committed, no unrelated files staged (note: `src/protocol/tools.rs` carries a
pre-existing uncommitted foreign-project-refusal change and `tests/zz_repro_foreign_project.rs` is
untracked — **both predate this slice and must not be swept into its commits**).
