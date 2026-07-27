# GPT-5.6-sol ultra — Knowledge LLM Sift (post-Kimi)

**Verdict:** `REVISE_KIMI_PLAN`  
**Date:** 2026-07-27  
**Source:** operator paste from GPT-5.6-sol ultra second pass

## Delta from Kimi

Kimi correctly identified the product cut: retrieval works; presentation, routing, authority
clarity, and result diversity do not. Keep its answer-first formatting, excerpt bounding, compact
bridge previews, complete-hit CCR, routing expansion, and light diversity.

Mechanics changed before implementation:

- Search must collect and rank across all selected sources before applying the global limit. It
  currently truncates each source independently and concatenates strings.
- Twelve-character prefixes cannot be applied indiscriminately: provenance IDs include semantic rule
  names, and fixed 48-bit hash prefixes are not collision-safe.
- Authority expansion must first repair unevidenced Active lifecycle and make headings precede path
  conventions.
- Diversity needs a two-pass soft quota. A hard two-per-path cap can starve legitimate multi-section
  single-file policies.

## Verdict

**REVISE_KIMI_PLAN.**

The product ledger is right and remains intact. Kimi’s plan is not safe to implement verbatim
because it would entrench multi-source limit/ranking violations, ambiguous ID abbreviations, and
misleading authority labels. The replacement below preserves every requested outcome while making
the implementation deterministic, contract-safe, and measurable.

## Recommended plan (final candidate for implementation)

### WS0 — Freeze red oracles and fix source composition

Files: `src/protocol/knowledge_search.rs`, `src/protocol/tools.rs`, `tests/search_knowledge.rs`.

1. Split per-source extraction from formatting. A private structured result should carry source
   identity, untruncated hits, counts, coverage, and no-match evidence.
2. Compose all selected sources structurally, then rank globally using the frozen tuple: phrase,
   heading, distinct-term coverage, source precedence, canonical path/line.
3. Apply limit once globally; compute one aggregate overflow, withheld count, filtered counts, and
   worst coverage.
4. Derive a real source label once—`worktree:<id>` or `ref:<name>`—and use it in scope, source, and hit
   fields.
5. Do not parse already-rendered strings to recover hit boundaries.

Repairs per-source `search_current` concatenation / truncation vs frozen global ranking contract.

### WS1 — Answer-first formatter, excerpts, IDs, bridges, and CCR

1. Compact top-level envelope: Trust; combined Scope/secret/counts; one `Source[n]` line per source; Derived.
2. Indivisible hit blocks: `N. source · path:line` / heading / quoted excerpt / provenance / bridge when present.
3. Type-aware ID display: semantic rule/policy IDs verbatim; abbreviate only digests; start at 12 hex and
   extend until unique within dossier/source response; forced-collision test; map to full IDs via document-mode
   `review_knowledge` without changing its schema.
4. Partition bridge previews by exact/declared-set/ambiguous/missing; reserve one slot per present class;
   fill remaining global cap; per-class omitted counts.
5. Shared friendly code-anchor-ID formatter for search and review (fix debug leak + review drift).
6. Excerpt ~240 Unicode chars; character-boundary safe (no byte offsets from lowercased text); whitespace snap;
   heading-skip to next substantive line; skip blank/heading-only/fence-marker/table-separator fallbacks;
   keep list markers and code; no Markdown parser.
7. Full output + block-safe summary from structured renderer; pack complete hits reserving CCR footer space;
   then `apply_ccr_budget_with_summary` (helper switch alone insufficient).
8. Preserve `\nNo match:` classifier seam; CCR stores complete pre-truncation safe output.

Success: excerpt by ~line 8; ≤60% baseline bytes for 10 hits; `max_tokens=300` header+≥1 complete hit+handle;
`max_tokens=120` provenance+handle no partial; CCR round-trip byte-for-byte full output.

### WS2 — Truthful, cleaner authority labels

File: `src/live_index/knowledge_authority.rs`.

1. No-evidence lifecycle fallback: Active → Unknown (contract: lifecycle cites evidence).
2. Heading evidence before path conventions.
3. Component tokenization like `path_convention_roles`; no substring matching.
4. Table:

| Exact class | Domain |
|---|---|
| Root AGENTS.md, CLAUDE.md, GEMINI.md; `.agent/` | Operations |
| `docs/solutions/` | Decision |
| `docs/reviews/`, `docs/dogfood/`, `research/` | HistoricalRecord |
| plan/plans/roadmap components | NormativeIntent |
| tasks, handoff, handover components | Operations |
| archive/archived | Existing HistoricalRecord |

5. No new CurrentImplementation path rule.
6. Hand-labeled fixtures + zero unintended Suppressed; aggregate unknown counts diagnostic only.
7. Prove active research/dogfood measurement remains visible in `authority_scope=default`.

### WS3 — Route prose questions to knowledge

1. Rewrite tool description (Kimi answer-oriented form).
2. Add Kimi’s five prefixes plus: `how does our policy on …`, `how does the process for … work`.
3. Keep code-intent tests: find references / where defined / retry policy in client code.
4. Do **not** redirect every generic `how does X work` (Understand branch); only noun-anchored prose cues.

### WS4 — Light, deterministic diversity

Failing real-corpus fixture first (contract step 5). Two-pass soft quota:

1. Frozen base ranking.
2. Diversity-pass only full-query-coverage hits (exact phrase or all significant terms), ≤2 per path,
   preserve base order and source precedence.
3. Fill remaining slots from deferred hits in original base order.

Never promote one-term noise; never underfill single-file corpora; never drop hits; no tunable weights.
Do not claim to solve cross-file self-pollution.

### Final verification

Contract tests 3, 7, 8, 9, 11, 16, 18; focused formatter/composition/authority/routing/diversity tests;
`cargo fmt --check`, `cargo check`, focused knowledge tests then repo gates; Kimi dogfood probes with
before/after; measure bytes and answer position (not formal token benchmark estimates).

## New findings (net-new vs Kimi)

- **BLOCKER** — Multi-source limit/ranking/counts applied per source → global compose+rank+limit.
- **HIGH** — Fixed 12-hex abbreviations type-unsafe / collision-unsafe → type-aware + extend-until-unique.
- **HIGH** — CCR helper switch alone insufficient → pre-fitted block summary + footer reserve.
- **HIGH** — Authority metrics reward unsupported certainty → Unknown fallback + heading-first + precision fixtures.
- **MEDIUM** — Shared code-anchor formatter for search+review; Unicode excerpt landmine; routing does not steal Understand lane.

## Do not implement yet

Embeddings; manual curation; Gate-M; explore mix; recency demotion; ranking config system; new public
ID-resolution schema; sentence/table parsers; cross-file self-pollution beyond diversity; fenced
`status:` lifecycle follow-up (Kimi C4).
