# Phase 0 Research: Knowledge LLM Sift

**Method**: the product research is the dual adversarial review (Kimi K3 → GPT-5.6-sol ultra); it is
not repeated here. What follows is the **code verification** of the claims those reviews depend on,
read directly from source on 2026-07-27, plus the decisions those readings force.

No `NEEDS CLARIFICATION` remained after the reviews; nothing in this phase was left open.

## Verified code findings

| # | Claim under test | Verdict | Evidence |
|---|---|---|---|
| V1 | Multi-source search truncates/ranks per source | **Confirmed** | `knowledge_search.rs:273-291` — `search_scoped` maps each selected source through `search_current(generation, input)` (a fully *rendered String*) and `join`s the sections. `search_current` applies `query.limit` at `:450-451` and computes its own `overflow`/`withheld_sensitive`/`FilteredCounts` at `:450-471`. Every count and the limit are therefore per source. |
| V2 | Excerpt is an uncapped raw line | **Confirmed** | `knowledge_search.rs:568-595` — `best` stores `line.to_string()`; `UnitMatch.excerpt` is that whole line with no cap. Tie-break `(phrase_here, terms_here, Reverse(offset))` prefers the earliest max-scoring line, which for many units is the title. |
| V3 | Hit line is a single pipe-delimited mega-line with a hardcoded source label | **Confirmed** | `knowledge_search.rs:779-808` — one `format!` per hit; contains the literal `source=current` regardless of the actual source. |
| V4 | `SymbolId { … }` Rust debug leaks into protocol output | **Confirmed** | `knowledge_search.rs:1014-1021` — `code_anchor_label` uses `format!("symbol:{symbol:?}:{start_line}")`. |
| V5 | Unevidenced lifecycle defaults to `Active` | **Confirmed** | `knowledge_authority.rs:1098` — `derive_native_lifecycle` falls through to `(KnowledgeLifecycle::Active, LifecycleEvidence::None)`. The hygiene contract requires lifecycle to cite evidence. |
| V6 | Authority path matching is substring-based and overmatches | **Confirmed** | `knowledge_authority.rs:1121,1122,1140` — `path_lower.contains("/spec")` (matches `/special/`, `/inspection/`), `contains("/design")` (matches `/redesign/`), `contains("changelog")`. |
| V7 | A component tokenizer already exists to reuse | **Confirmed** | `knowledge_bridge.rs:743-785` — `path_convention_roles` splits on `/`, then on non-ASCII-alphanumeric, lowercases, and matches tokens **exactly** against a closed vocabulary. This is the model WS2 adopts. |
| V8 | `CurrentImplementation` has a suppression path | **Confirmed** | `knowledge_authority.rs:234` (`derive_voice`) and `:1394` (`AuthorityDomain::CurrentImplementation =>`). Hence the hard "no new `CurrentImplementation` path rule" constraint. |
| V9 | The no-match seam is load-bearing for outcome classification | **Confirmed** | `tools.rs:277-284` — `classify_search_knowledge_output` keys on `text.contains("\nNo match:")` to emit `OutcomeClass::EmptyResult`. Also reused verbatim by `review_knowledge` and `curate_knowledge` classification (`tools.rs:5481,5486,5543,5548`). |
| V10 | `apply_ccr_budget_with_summary` exists and is already used by a sibling | **Confirmed** | `mod.rs:894-911`; `review_knowledge` already returns `ReviewKnowledgeOutput { rendered, budget_rendered, … }` (`knowledge_review.rs:45-54`) and calls it at `tools.rs:5516`. |

## Decision 1 — Why switching the CCR helper is *not* sufficient (GPT "HIGH")

Reading `mod.rs:894-911` and `ccr.rs:232-255` together:

```rust
// apply_ccr_budget_with_summary
let summary = format::enforce_token_budget(summary, Some(tokens));   // <-- still line-cuts
return ccr::apply_ccr_overflow(&mut store, tool_name, summary, result, tokens);

// apply_ccr_overflow
if full.len() <= max_bytes { return full; }
let handle = store.insert(tool_name, full);                          // full output stored ✔
format!("{summary}\n---\nCCR: … hash=\"{handle}\"\n")                // footer appended AFTER budgeting
```

Two consequences the plan must honour:

1. The helper still runs a **line-boundary cut** on whatever summary it is handed. Passing a
   "block-safe" summary that is over budget gets it chopped mid-block anyway. The summary must
   therefore be **pre-fitted below the budget** so that call is a no-op.
2. The footer is appended *after* budgeting, so a summary packed to exactly `max_bytes` overshoots.
   The packer must reserve footer space.

**Decision**: pack `budget_rendered` to `max_bytes − CCR_FOOTER_RESERVE_BYTES`, whole blocks only.
Add `CCR_FOOTER_RESERVE_BYTES` to `ccr.rs` beside the `format!` that defines the footer, sized for the
**longer** compact-facade rewrite — `rewrite_footer_for_symforge_facade` (`ccr.rs:260-265`) replaces a
38-byte substring with a 49-byte one, so the reserve must include those extra 11 bytes or a compact
result can exceed its budget.

**Alternatives rejected**: (a) switch the helper only — proven insufficient above; (b) make
`apply_ccr_overflow` reserve the footer itself — would change budgeting for `review_knowledge` and
`get_repo_map` too, which is outside this slice and would need its own contract review.

## Decision 2 — Type-aware ID abbreviation

Fixed 12-hex prefixes are unsafe for two independent reasons GPT raised: provenance ID vectors
contain **semantic rule names** (e.g. `authority-history-v1`, from
`record.code_evidence.*_rule_ids`, `knowledge_search.rs:638-652`), and a 48-bit prefix is not
collision-free across ~5k units.

**Decision**: classify at render time. An ID that is entirely lowercase hex **and** at least 12 chars
long is a digest and may abbreviate; anything else renders verbatim. Digest abbreviation length is
computed once per response: start at 12, extend until every digest in the response has a unique
prefix. One length for the whole response, so the same digest never renders two ways in one answer.

**Alternatives rejected**: per-ID minimal length (same digest could render differently in two hits);
a new public ID-resolution schema (explicit non-goal — resolution stays through document-mode
`review_knowledge`).

## Decision 3 — Unicode-safe excerpt windowing

The natural implementation — lowercase the line, `find()` the match, slice the original at that byte
offset — is wrong twice: `to_lowercase()` is not length-preserving for all Unicode, and byte offsets
can land inside a multi-byte character (panic) or split a grapheme.

**Decision**: iterate `char_indices()` on the original line; locate the match by scanning the
lowercased form and mapping its char index (not byte index) back onto the original; cut only at
character boundaries; snap window edges outward to whitespace; bound to ~240 **characters**, not bytes.

**Alternatives rejected**: a Markdown/sentence parser (explicit non-goal); byte-based windows with a
`is_char_boundary` retry loop (works, but hides the bug class rather than removing it).

## Decision 4 — Diversity must not be a hard cap

A hard 2-per-path cap starves a legitimate multi-section single-file policy: a repository whose only
matches live in one large `CLAUDE.md` would return 2 hits for `limit=10`.

**Decision**: two passes. Pass 1 walks the globally-ranked list in order and admits a hit if its path
has fewer than 2 admitted **full-query-coverage** hits (exact phrase, or all significant terms);
everything else is deferred. Pass 2 appends deferred hits in original base order until `limit`. No hit
is dropped, no score weight is introduced, and the operation is a stable permutation of a
deterministic list — so contract test 8 still holds.

**Alternatives rejected**: score penalties (introduces a tunable, an explicit non-goal); demoting by
file class/recency (forbidden by the hygiene proof matrix — age cannot imply staleness).

## Decision 5 — `current` scope must not become a second code path

`search_scoped` currently short-circuits `KnowledgeSourceScope::Current` straight into
`search_current` (`knowledge_search.rs:263-265`). Keeping that shortcut after WS0 would leave two
composition paths that can drift — exactly the drift Kimi found between the authority and bridge
path vocabularies (finding C3).

**Decision**: `current` selects one source and flows through the same extract → compose → render
pipeline. Its output shape stays single-source (no `== source: … ==` banners), because the renderer
emits per-source identity lines rather than sectioning, which is what the contract's top-level
"deterministic per-source list" actually asks for.

## Out-of-slice finding (recorded, not fixed)

`src/protocol/knowledge_search.rs` (48 KB) and `src/protocol/tools.rs` are classified **Tier 2 —
metadata only, reason "unsupported language"** in the live index, despite being Rust. Consequence:
`search_symbols` and `search_text` return nothing for them, and the code lane cannot answer questions
about its own largest protocol modules. This directly undercuts the product goal of cheap orientation.
Filed for separate triage; fixing it inside this slice would breach the slice boundary and the "no
unrelated work" instruction.
