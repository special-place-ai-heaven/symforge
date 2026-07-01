# Contract: Semantic Edges

**Feature**: 015 · **Sprint**: S4 · **US**: US10

## Index-time

During `index_folder(mode=deep)` only:

- Compute pairwise scores for same-language symbol pairs in related modules.
- Emit `SemanticallyRelated` edges when combined score ≥ threshold.
- Threshold default 0.80; override `SYMFORGE_SEMANTIC_THRESHOLD`.

## Signals (v1)

| Signal | Weight |
|--------|--------|
| TF-IDF token overlap | 0.25 |
| API signature similarity | 0.25 |
| Type signature similarity | 0.20 |
| MinHash (name+sig) | 0.15 |
| Module proximity multiplier | 1.0–1.1 |

Embeddings NOT required for v1.

## Query-time

STEL find intent accepts optional `semantic_keywords: string[]` (full surface:
`search_symbols` param) — ranks symbols with semantic edges to keyword-matching
symbols.

## Frecency

- Index pass: N/A
- Query: MUST NOT bump frecency

## Storage

- v1: in-memory on GraphProjection only
- v2 (optional): snapshot v5 extension (deferred)
