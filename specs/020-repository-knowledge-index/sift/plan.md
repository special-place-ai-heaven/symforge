# Implementation Plan: Knowledge LLM Sift

**Branch**: `feat/knowledge-llm-sift` | **Date**: 2026-07-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/020-repository-knowledge-index/sift/spec.md`

**Product authority**: `C:\Users\rakovnik\.cursor\plans\knowledge_llm_sift_56bece4f.plan.md` (GPT-5.6-sol ultra
`REVISE_KIMI_PLAN`, folding Kimi K3 dogfood). This plan is the code-level realization of that plan;
where they appear to disagree, the Cursor plan wins and the conflict is reported, not resolved locally.

## Summary

Harden the already-shipped `search_knowledge` surface so an LLM that does not know the path can sift
repository prose as cheaply and reliably as it sifts code symbols. Five ordered workstreams:

1. **WS0** replaces per-source *render-then-concatenate* with structured per-source extraction →
   **global** rank → single `limit` → single aggregate count set.
2. **WS1** replaces the one-line-per-hit mega-line with an answer-first, indivisible hit block, adds
   Unicode-safe bounded excerpts, type-aware ID abbreviation, per-class bridge packing, a shared
   code-anchor formatter, and a block-safe CCR summary.
3. **WS2** makes authority labels truthful (unevidenced lifecycle → `Unknown`, heading before path)
   and adds component-tokenized path rules for the repository's real conventions.
4. **WS3** rewrites the tool description and adds noun-anchored prose routing cues.
5. **WS4** lands a failing same-file-flooding fixture, then a two-pass soft diversity quota.

The order is load-bearing: WS1 formats WS0's output, WS2 fills a field WS1 renders, WS3 routes into
WS1's output, and WS4 reorders WS0's ranked list.

## Technical Context

**Language/Version**: Rust 2024 edition, single crate `symforge` (8.16.5)

**Primary Dependencies**: `rmcp` (MCP protocol), `tree-sitter` (parsing), `tempfile` + `tokio` (tests).
No new dependency is introduced by this slice.

**Storage**: In-process `LiveIndex` + `.symforge/` snapshots. Read-only for this slice; the only
writable surface in Feature 020 (`curate_knowledge` → `.symforge-knowledge.toml`) is untouched.

**Testing**: `cargo test --all-targets -- --test-threads=1`; slice-focused
`cargo test --test search_knowledge` plus in-module `#[cfg(test)]` units in
`knowledge_search.rs` / `knowledge_authority.rs` / `smart_query.rs`.

**Target Platform**: Windows 11 primary (this workstation), Linux/macOS via CI.

**Project Type**: MCP server (single Rust crate, library + binary).

**Performance Goals**: No new asymptotic cost. WS0 removes one full render per source. WS4 adds one
O(n) pass over an already-bounded candidate list. Ranking stays byte-for-byte deterministic.

**Constraints**:

- Frozen contracts (`search-knowledge.md`, `knowledge-authority-hygiene.md`,
  `repository-mental-model.md`) are not reopened; no MUST-include field is dropped.
- CCR must store the **full** pre-truncation safe output, never the summary.
- `\nNo match:` seam position/prefix is load-bearing for `classify_search_knowledge_output`
  (`tools.rs:277-284`) and the STEL dependent-chain special case.
- Search is frecency-neutral (Constitution V) and formats only from the captured source set.

**Scale/Scope**: ~5,142 knowledge units in this repository (2026-07-27 baseline); 883 indexed files.
Six source files plus one integration test file change.

## Constitution Check

*GATE: passed before Phase 0; re-evaluated after design (bottom of this file).*

| Principle | Assessment |
|---|---|
| **I. Local-First In-Process Index** | PASS — no new store. WS0 restructures composition of an already-captured `PublishedSourceSet`; no second index, no reload while formatting. |
| **II. MCP-Native Surface** | PASS — all changes are the behavior of existing MCP tools (`search_knowledge` description, output shape) and the existing `ask` intent classifier. No chat injection; no new tool; no schema field added (contract test 1 keeps eight fields). |
| **III. Trust Envelopes** | PASS, and strengthened — WS0 makes overflow/withheld/filtered/coverage a single truthful aggregate instead of per-source fragments; WS2 replaces an unevidenced `Active` claim with `Unknown`; per-class bridge omitted counts stay explicit. |
| **IV. Determinism & Recovery** | PASS — global rank uses the frozen total-order tuple; WS4's diversity pass is a deterministic stable reorder; ID abbreviation extends deterministically until unique. Contract test 8 (byte-for-byte determinism) is a gate. |
| **V. Frecency Invariant** | PASS — no frecency write is added on any touched path; `search_knowledge` remains frecency-neutral. |
| **VI. Embed Isolation (G-045)** | PASS — every touched module is already reachable under `embed`; no server/network dependency introduced. Verified by `cargo check --no-default-features --features embed`. |
| **VII. Transport Parity** | PASS — all changes live in shared protocol formatters (`knowledge_search.rs`, `smart_query.rs`, `knowledge_authority.rs`), which is the parity boundary itself. No transport-specific branch is added. |
| **VIII. Verification Before Done** | Enforced by the task list: every workstream carries RED → GREEN → VERIFY steps, and the slice closes on contract tests 3/7/8/9/11/16/18 plus the full gate. |

**No violations. Complexity Tracking section omitted — nothing to justify.**

## Project Structure

### Documentation (this feature)

```text
specs/020-repository-knowledge-index/sift/
├── spec.md              # Slice spec (frozen)
├── plan.md              # This file
├── research.md          # Phase 0: code-level findings + resolved decisions
├── data-model.md        # Phase 1: the new internal structures
├── quickstart.md        # Phase 1: how to prove the slice works
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # /speckit-tasks output
```

No new contract file: this slice implements against Feature 020's **existing frozen** contracts in
`specs/020-repository-knowledge-index/contracts/`. Adding a contract here would falsely imply a new
surface; the surface is unchanged and only its behavior is being brought into compliance.

### Source Code (repository root)

```text
src/
├── protocol/
│   ├── knowledge_search.rs     # WS0 composition; WS1 formatter/excerpt/IDs/bridges; WS4 diversity
│   ├── knowledge_review.rs     # WS1 shared code-anchor formatter (drift fix)
│   ├── tools.rs                # WS0 call site; WS1 CCR summary wiring; WS3 tool description
│   ├── mod.rs                  # WS1 apply_ccr_budget_with_summary wiring
│   ├── ccr.rs                  # WS1 footer-reserve constant
│   └── smart_query.rs          # WS3 noun-anchored routing cues
└── live_index/
    └── knowledge_authority.rs  # WS2 lifecycle fallback, heading precedence, path rules

tests/
└── search_knowledge.rs         # Contract + slice integration tests (all workstreams)
```

**Structure Decision**: Existing single-crate layout, unchanged. The slice adds no module and no
file; it restructures four protocol modules and one live-index module in place. The only new
abstraction is the per-source result type WS0 requires (see `data-model.md`) — it stays private to
`knowledge_search.rs` because nothing outside that module composes sources.

## Implementation order and rationale

| WS | Why it must come here |
|---|---|
| **WS0** | The only *correctness* defect (frozen ranking/limit contract). Every later WS formats, labels, routes into, or reorders its output. Building the formatter first would mean rebuilding it on a structure that must change. |
| **WS1** | The usability payload, and the highest-risk change (multi-line hits break the naive line-boundary CCR cut). Must land before WS2, whose only user-visible surface is a field inside a WS1 hit block. |
| **WS2** | Truthful labels. Independent of WS3/WS4, but follows WS1 so the label change is observed in the final rendering, not in a format about to be replaced. |
| **WS3** | Routing is worthless pointing at unreadable output, and its false-positive tests assert against the post-WS1 shape. |
| **WS4** | Contract-gated: `search-knowledge.md` §Query interpretation step 5 permits diversity **only after** a failing corpus fixture proves flooding. The fixture must be red against the WS0-composed, WS1-rendered pipeline — not against the old one. |

## Design decisions

Detail and rejected alternatives in [research.md](research.md); structures in [data-model.md](data-model.md).

1. **Composition boundary (WS0)** — `search_scoped` stops calling `search_current` per source. A new
   private `extract_source` returns a structured per-source result; both the `current` fast path and
   the multi-source path feed one `compose_and_render`. `current` keeps identical semantics by
   flowing through the same path with one selected source, so there is no second code path to drift.
2. **Ranking key (WS0)** — the comparator moves out of `search_current` into a free function keyed on
   `(hit, source_precedence)` so global sort and per-source extraction cannot disagree. Source
   precedence is the index in `select_scoped_sources` output; the current lane is already first there,
   which preserves contract test 9 (current ranks ahead of a divergent ref without hiding it).
3. **CCR (WS1)** — `search_knowledge` returns a `rendered` + `budget_rendered` pair exactly as
   `review_knowledge` already does (`ReviewKnowledgeOutput`), and `tools.rs:5461` switches from
   `apply_ccr_budget` to `apply_ccr_budget_with_summary`. `budget_rendered` is packed to
   `max_bytes − CCR_FOOTER_RESERVE_BYTES` so the `enforce_token_budget` call *inside* that helper is a
   no-op and cannot re-cut a block. The reserve constant lives in `ccr.rs` beside the footer that
   defines it, sized for the longer compact-facade rewrite
   (`rewrite_footer_for_symforge_facade` lengthens the footer by 11 bytes).
4. **ID abbreviation (WS1)** — a display pass over the assembled response: an ID that is not pure
   lowercase hex of length ≥ 12 renders verbatim (semantic rule/policy IDs); a digest abbreviates to
   the shortest prefix ≥ 12 hex that is unique across **every** digest in the response, computed once
   and applied everywhere so the same digest never renders two ways in one answer.
5. **Excerpts (WS1)** — operate on `char_indices` of the original (non-lowercased) line; the match is
   located by a lowercase scan whose offset is mapped back to a character boundary. No byte slicing
   of lowercased text — that is the Unicode landmine GPT flagged.
6. **Authority (WS2)** — `derive_native_lifecycle` returns `Unknown` where it currently defaults
   `Active`; the domain deriver evaluates heading evidence first, then component-tokenized path rules
   reusing the `path_convention_roles` vocabulary (`knowledge_bridge.rs:~743`). No new rule may emit
   `CurrentImplementation` — its `DeterministicConflict → Suppressed` path would convert a labeling
   improvement into a retrieval regression.
7. **Diversity (WS4)** — a pure function applied after the global sort and before truncation, so it is
   trivially testable and reversible: pass 1 admits at most 2 full-query-coverage hits per path in base
   order; pass 2 spills deferred hits back in base order until `limit` is reached.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Multi-line hits break the CCR line cut (contract test 7) | Block-safe pre-fitted summary + footer reserve; test 7 is an explicit gate, plus new `max_tokens=300`/`120` assertions. |
| Existing tests assert the mega-line shape and pass *vacuously* | Rewrite as block-completeness assertions (Kimi Test-plan deltas) rather than re-pinning strings. The CCR test's per-line `contains("authority:")` filter is the known vacuous one. |
| WS2 path rules push units into `Suppressed`, hiding them | Hard rule: no new `CurrentImplementation`; explicit test that a simulated `DeterministicConflict` on a `CLAUDE.md`-class unit stays visible; `suppressed` count delta must be zero. |
| WS3 steals the generic Understand lane | Only noun-anchored prefixes; the existing false-positive test is extended, not replaced. |
| WS4 underfills a legitimate single-file corpus | Spill pass restores deferred hits in base order; explicit single-file-corpus test. |
| Windows disk pressure from repeated full gates | Focused `--test search_knowledge` during development; full gate once at the end; `cargo clean` after heavy runs (CLAUDE.md). |

## Out of scope for this plan

Embeddings; Gate-M corpus; manual curation; explore/knowledge lane mixing; recency demotion; ranking
config knobs; a new public ID-resolution schema; Markdown/sentence/table parsers; cross-file review
self-pollution beyond WS4; the fenced `status:` lifecycle scan (Kimi C4 follow-up); Feature 020
Gates A–M completion.

**Separately observed, not in this slice**: `src/protocol/knowledge_search.rs` and
`src/protocol/tools.rs` are Tier 2 (metadata-only) in the live index — reported as
"unsupported language" despite being Rust — so `search_symbols`/`search_text` cannot reach them.
Filed as a standalone finding; fixing it here would violate the slice boundary.

## Constitution re-check (post-design)

Re-evaluated against the seven design decisions above: no decision introduces a second index (I), a
non-MCP surface (II), a silent truncation (III), a non-deterministic ordering (IV), a frecency write
(V), a server dependency under `embed` (VI), or a transport-specific branch (VII). Decision 3
strengthens III by guaranteeing whole-record truncation; decision 6 strengthens III by removing an
unevidenced certainty claim. **PASS — no new violations, no complexity to track.**
