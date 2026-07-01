# Sprint 4 Planning Spec — Algorithmic Semantic

**Status**: draft  
**Release**: 8.13.x  
**User stories**: US10  
**Depends on**: S2 graph, S3 optional (better with resolved edges)

## Objective

Index-time **SemanticallyRelated** edges via TF-IDF + signatures + MinHash —
vocabulary bridging without embeddings (D-015-005).

## CBM deep-read

- `semantic/semantic.c` — weights L42–49, threshold env
- `simhash/minhash.c` — SIMILAR_TO (optional partial port)

## Signal weights (freeze at Planning Gate)

Match [semantic-edges contract](../contracts/semantic-edges.md) unless benchmark dictates change.

## Index gating

- **Only** `IndexMode::Deep` runs semantic pass
- Fast/Standard: zero semantic CPU cost

## Query surface

- STEL find: optional `semantic_keywords: Vec<String>` (full surface first)
- Compact: document in intent description only until schema budget review

## Fixtures

`tests/fixtures/cbm_semantic/` — publish module + send module + unrelated control module.

## Out of scope

- Nomic embeddings / vector table
- SIMILAR_TO near-clone (optional stretch)

## Risk

R-13 false positives — tune `SYMFORGE_SEMANTIC_THRESHOLD` default 0.80

## Planning Gate

- [ ] Signal weights frozen
- [ ] Deep mode behavior documented in glossary
- [ ] A-US10 rows assigned

**Sign-off**: _________________ Date: _______
