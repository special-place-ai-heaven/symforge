# Phase 1 Data Model: Knowledge LLM Sift

All structures are **internal**. The MCP schema is unchanged — `SearchKnowledgeInput` keeps exactly
its eight fields (frozen contract test 1), and no public type is added or altered.

## New / changed internal structures

### `SourceHits` (new, private to `knowledge_search.rs`) — WS0

The structured per-source extraction result that replaces a rendered `String` per source.

| Field | Type | Purpose |
|---|---|---|
| `generation` | `Arc<PublishedGeneration>` | The captured generation this source's evidence came from. Never reloaded. |
| `label` | `String` | The real source label — `current`, `worktree:<id>`, or `ref:<name>` — derived **once** and reused in the scope line, the per-source identity line, and every hit. Fixes the hardcoded `source=current` (`knowledge_search.rs:781`). |
| `precedence` | `usize` | Index in `select_scoped_sources` output. The current lane is index 0, preserving contract test 9. |
| `envelope` | `SourceResponseEnvelope` | Per-source identity/version/generation/digest/coverage for the top-level MUST-include per-source list. |
| `hits` | `Vec<KnowledgeHit>` | **Untruncated**, locally deduplicated and locally sorted. Global ranking re-sorts across sources; local order only makes the extraction deterministic. |
| `withheld_sensitive` | `usize` | Contributes to the single aggregate. |
| `filtered` | `FilteredCounts` | Contributes to the single aggregate (field-wise sum). |
| `readiness` | `Option<&'static str>` | Set when this source could not be searched (loading / no valid source / withheld envelope). Survives composition instead of being lost to string concatenation. |

**Validation rules**
- `hits` is never truncated inside extraction — `limit` applies exactly once, globally (FR-001).
- A source with `readiness = Some(_)` contributes zero hits but **must** still appear in the
  per-source list and must degrade overall coverage.
- `label` is computed from `generation.source.location` only, never inferred from scope.

### `KnowledgeHit` (existing, extended) — WS0/WS1/WS4

Added fields:

| Field | Type | Purpose |
|---|---|---|
| `source_label` | `String` | The owning source's label, carried on the hit so global sorting cannot separate a hit from its provenance. |
| `source_precedence` | `usize` | Ranking tuple position 4 (after distinct-term coverage, before path/line). |
| `full_coverage` | `bool` | WS4 input: `exact_phrase || distinct_term_count == query.terms.len()`. Computed at extraction, not re-derived during diversity. |

Changed fields:

| Field | Change |
|---|---|
| `excerpt` | Now a **bounded window** (~240 Unicode chars) rather than the whole matched line; may carry leading/trailing ellipsis. Still exact source text — no rewriting, still guarded by `guard_hit`. |
| `bridge_previews` | Becomes class-partitioned (see `BridgePreviews` below) instead of a flat `Vec<String>` + single omitted count. |

**Invariant preserved**: every field rendered from document bytes must pass through `visible_fields`
in the `guard_hit` call (`knowledge_search.rs:409-418`). The new `source_label` and
`source_precedence` are deterministic non-content and stay out of it, matching the existing treatment
of `line_range` (Kimi C7).

### `BridgePreviews` (new, private) — WS1

Replaces the flat preview vector so no class can be dropped (frozen contract test 18).

| Field | Type | Purpose |
|---|---|---|
| `exact` | `Vec<String>` | Rendered through the shared friendly code-anchor formatter (`file:<path>`, `symbol:<path>#<name>:<line>`). |
| `declared_set` | `Vec<String>` | Compact ownership selectors with matched counts. |
| `ambiguous` | `Vec<String>` | Compact `<id>:ambiguous:<n>` tokens. |
| `missing` | `Vec<String>` | Compact `<id>:missing` tokens. |
| `omitted` | `[usize; 4]` | Per-class omitted counts, canonically ordered with the classes above. |

**Packing rule**: reserve ≥1 slot for each **present** class first, then fill the remaining global cap
in class order. A class that is present but fully omitted still renders its count, so an agent can
always see that a document's code anchors are broken — the trust signal Kimi B1 protects.

### `DisplayIds` (new, private) — WS1

A per-response abbreviation table, built once after composition.

| Field | Type | Purpose |
|---|---|---|
| `digest_len` | `usize` | Shortest prefix length ≥12 at which **all** digests in the response are distinct. |

**Classification rule**: `is_digest(id) = id.len() >= 12 && id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())`.
Everything else — `authority-history-v1`, `role.path.plan-handoff.v1` — renders verbatim (FR-006).

**Determinism**: `digest_len` is a function of the response's digest set alone, so equal generations
produce equal output (contract test 8).

### `SearchKnowledgeOutput` (new, private) — WS1

Mirrors the existing `ReviewKnowledgeOutput` (`knowledge_review.rs:45-54`) so the two knowledge tools
budget identically.

| Field | Type | Purpose |
|---|---|---|
| `rendered` | `String` | The complete safe output. This — never the summary — is what CCR stores. |
| `budget_rendered` | `String` | Whole hit blocks only, pre-fitted to `max_bytes − CCR_FOOTER_RESERVE_BYTES`. |

## Rendered shape (the contract-visible artifact)

```text
Trust: exact repository knowledge evidence | publication=<n> | content=<n> | source=<label> | coverage=<c>
Secret policy: version <n>
Scope: <source_scope> + <path_scope> | overflow=<n> withheld=<n> filtered_*=<n…>
Source[1]: <label> source_id=<id12> source_version=<…> publication=<n> content=<n> freshness=<f> coverage=<c> manifest_digest=<id12>
Derived: authority_rule_version=<n> policy_version=<n> secret_policy_version=<n> bridge_coverage=<c> authority_coverage=<c> overall_coverage=<c>

1. <label> · <path>:<line>
   <heading breadcrumb>
   "<bounded excerpt>"
   hash=<id12> pub=<n>/<n> lines <a>..<b> · lifecycle=<l> domain=<d> code=<e> voice=<v> coverage=<c>
   findings=[<ids>] (+<n>) provenance=[<ids>] (+<n>)
   bridge: <exact…> · <n> missing, <n> ambiguous (+<n> omitted)
```

Every MUST-include field from `contracts/search-knowledge.md` §Successful response is present; the
change is **layout and boundedness**, not content. `\nNo match: <class>` keeps its exact prefix and
position on empty responses.

## State transitions

None. `search_knowledge` is read-only and frecency-neutral; no entity in this model persists beyond
one call, and nothing here touches the `.symforge-knowledge.toml` policy ledger.
