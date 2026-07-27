# Feature Specification: Knowledge LLM Sift

**Parent feature**: 020 — Repository Knowledge Index (shipped: `search_knowledge`, `review_knowledge`, `curate_knowledge`)<br>
**Slice IDs**: `SIFT-WS0` … `SIFT-WS4`<br>
**Feature Branch**: `feat/knowledge-llm-sift`<br>
**Created**: 2026-07-27<br>
**Status**: Frozen for implementation

**Product authority (do not duplicate here)**: `C:\Users\rakovnik\.cursor\plans\knowledge_llm_sift_56bece4f.plan.md`<br>
**Review inputs**: `../GPT56-REVIEW-RESPONSE-knowledge-llm-sift-2026-07-27.md`, `../KIMI-REVIEW-RESPONSE-knowledge-llm-sift-2026-07-27.md`<br>
**Frozen contracts (not reopened by this slice)**: `../contracts/search-knowledge.md`, `../contracts/knowledge-authority-hygiene.md`, `../contracts/repository-mental-model.md`

## Problem

Feature 020's knowledge lane **retrieves** well — dogfood (Kimi K3, 2026-07-27) confirmed the
semantically correct unit landed in the top 3 for every natural-language probe. It is nonetheless
not *usable* by an LLM that does not know the path:

- multi-source scopes truncate and concatenate **per source**, so `limit`, ranking, and counts are
  applied per source instead of globally — a frozen-contract violation, not a cosmetic one;
- one pipe-delimited mega-line per hit (~250 tokens) buries the answer behind provenance chrome;
  under `max_tokens=120` no excerpt survives at all;
- excerpts are uncapped raw matched lines (a 1.5 KB table row became one excerpt) and heading
  matches duplicate the breadcrumb;
- 76% of units report `domain=unknown` and lifecycle is asserted `Active` with no evidence, so the
  authority clause carries cost without decision value — and is sometimes untruthful;
- prose questions (`how does release please squash merge work`) fall through to the `Explore`
  fallback instead of routing to `search_knowledge`;
- one file can flood the result set (3 of 5 hits from a single handoff file), hiding the canonical
  policy behind a review brief that merely quotes it.

This slice makes the knowledge lane feel as indispensable for **doc-sifting** (policy/design/process
without knowing the path) as the code lane already feels for symbols.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Correct answers across every selected source (Priority: P1) — `SIFT-WS0`

An agent searches with `source_scope=all` (current worktree + worktrees + local refs). It receives
the globally best N hits, ranked by one order across all sources, with one set of aggregate counts.

**Why this priority**: This is a frozen-contract violation, and every later workstream formats the
output of this stage. Fixing presentation on top of per-source truncation would entrench the bug.

**Independent Test**: Two sources whose combined match count exceeds `limit`; assert the returned
set is the global top-N (not `limit` per source), that ranking interleaves sources by the frozen
tuple, and that overflow/withheld/filtered counts and coverage are single aggregates.

**Acceptance Scenarios**:

1. **Given** two sources each holding 8 matches and `limit=10`, **When** the agent searches
   `source_scope=all`, **Then** exactly 10 hits return, chosen globally, with one aggregate
   `overflow=6` — not 10 per source and not two independent count sets.
2. **Given** a hit from a worktree source, **When** it is rendered, **Then** its source label reads
   `worktree:<id>` (or `ref:<name>`) in the scope line, the per-source line, and the hit itself —
   never a hardcoded `current`.
3. **Given** a source whose coverage is degraded, **When** results compose, **Then** the top-level
   coverage equals the **worst** included source.

---

### User Story 2 — The answer is readable before the provenance (Priority: P1) — `SIFT-WS1`

An agent asks a policy question and can read the answering sentence within the first few lines,
with provenance present but demoted, and with tight token budgets still returning whole hits.

**Why this priority**: This is the usability defect the whole slice exists to fix. It is also where
truncation risk is highest — a naive line-boundary cut chops a multi-line hit in half.

**Independent Test**: Run the release-please probe and measure the line number of the first excerpt
and the total response bytes against the pre-slice baseline; run the same query at
`max_tokens=300` and `max_tokens=120` and assert block completeness.

**Acceptance Scenarios**:

1. **Given** a default 10-hit query, **When** the response renders, **Then** the first excerpt
   appears by roughly line 8 and total bytes are at most 60% of the pre-slice baseline.
2. **Given** `max_tokens=300`, **When** the response is budgeted, **Then** it contains the header,
   at least one **complete** hit block, and a retrieval handle — never a partial block.
3. **Given** `max_tokens=120`, **When** the response is budgeted, **Then** it contains provenance
   and a handle and **no** partial hit block.
4. **Given** any truncated response, **When** its handle is redeemed, **Then** the retrieved
   document is the **full** pre-truncation safe output, byte-for-byte.
5. **Given** a hit whose links are all missing/ambiguous, **When** it renders, **Then** those
   classes still appear in compact form with per-class omitted counts — nothing is dropped.
6. **Given** a unit whose best matching line is a 1.5 KB table row, or a heading already shown in
   the breadcrumb, or contains CJK/emoji, **When** the excerpt is built, **Then** it is bounded to
   roughly 240 characters, cut on character boundaries, and does not duplicate the breadcrumb.

---

### User Story 3 — Authority labels that are true (Priority: P2) — `SIFT-WS2`

An agent reading a hit's authority clause can trust it: a lifecycle claim cites evidence, or it
says `Unknown`. Common repository conventions (`docs/solutions/`, `research/`, `.agent/`, root
agent-instruction files, plan/handoff paths) carry a real domain instead of `unknown`.

**Why this priority**: An untruthful label is worse than an absent one, and the hygiene contract
requires lifecycle to cite evidence. Depends on WS1 only for where the clause is rendered.

**Independent Test**: Hand-labeled path fixtures assert each new rule fires on its class; overmatch
guards assert `docs/special/`, `docs/redesign/`, `docs/inspection/` stay Unknown; a
`review_knowledge(mode=summary)` before/after diff shows zero units moving into `suppressed`.

**Acceptance Scenarios**:

1. **Given** a unit with no lifecycle evidence, **When** authority derives, **Then** lifecycle is
   `Unknown`, not `Active`.
2. **Given** a document whose heading declares a domain and whose path convention suggests another,
   **When** authority derives, **Then** the heading evidence wins.
3. **Given** `docs/special/report.md`, **When** authority derives, **Then** the domain stays Unknown
   (no substring overmatch against `spec`).
4. **Given** any new path rule in this slice, **When** it fires, **Then** it never assigns
   `CurrentImplementation`, and no unit becomes `Suppressed` as a result.
5. **Given** an active `research/` or `docs/dogfood/` measurement document, **When** searched under
   `authority_scope=default`, **Then** it remains visible.

---

### User Story 4 — Prose questions reach the prose lane (Priority: P2) — `SIFT-WS3`

An agent (or the `ask` router) asking "what is our policy on X" is routed to `search_knowledge`
rather than to code search or the `Explore` fallback — while code-intent questions stay code-routed.

**Why this priority**: Contract-backed (`repository-mental-model.md` §ask) and cheap, but worth
nothing if the output it routes to is unreadable — hence after WS1/WS2.

**Independent Test**: Each noun-anchored prefix routes to `search_knowledge`; the false-positive
table (`find references to X`, `where is search_knowledge defined`, `retry policy in the client
code`) stays code-routed.

**Acceptance Scenarios**:

1. **Given** `what is our policy on release merges`, **When** intent classifies, **Then** it routes
   to `search_knowledge`.
2. **Given** `find references to SymbolId`, **When** intent classifies, **Then** it stays
   code-routed.
3. **Given** a generic `how does X work` with no doc noun, **When** intent classifies, **Then** the
   existing Understand/Explore routing is unchanged.

---

### User Story 5 — One file cannot flood the answer (Priority: P3) — `SIFT-WS4`

An agent's result set surfaces the canonical policy document rather than three sections of the same
review brief that quotes it.

**Why this priority**: Real (dogfood: 3/5 hits from one file) but the smallest lever, and the
frozen contract permits diversity **only** after a failing corpus fixture proves the flooding.

**Independent Test**: The failing fixture lands first and is proven red; the diversity pass then
turns it green without underfilling a single-file corpus and without dropping any hit.

**Acceptance Scenarios**:

1. **Given** a corpus where >2 hits from one file outrank a distinct-file canonical hit, **When**
   diversity applies, **Then** the distinct-file hit is promoted.
2. **Given** a corpus whose only matches are many sections of one legitimate multi-section policy
   file, **When** diversity applies, **Then** the response is not underfilled — deferred hits spill
   back in base order.
3. **Given** a hit matching only one query term, **When** diversity applies, **Then** it is never
   promoted over a full-coverage hit.

---

### User Story 6 — Code intelligence never lies about coverage (Priority: P1) — `SIFT-WS5`

An agent searching for an identifier that exists in the repository either finds it, or is told
explicitly that part of its search scope was not searched. It is never given an unqualified
"no matches" for a file the index deliberately excluded.

**Why this priority**: a false negative that *looks* authoritative is the most dangerous failure a
code-intelligence tool can have — it ends investigations early and can be read as proof that a
security guard does not exist. Reproduced on this repository during implementation:
`src/protocol/knowledge_search.rs` (48 KB, Rust) is Tier 2 and `search_text("search_scoped")`
returns a clean "No matches" for an identifier that occurs in it repeatedly.

Source: `.scratch/symforge-dogfood-issues-2026-07-27.md` (SF-DOG-001…005), reproduced independently
against `E:\project\symforge` as well as the reporting agent's `E:\project\testpilot`.

**Independent Test**: an oversized/excluded fixture with a unique identifier; exact search must
return either the hit or an explicit incomplete-coverage result naming the excluded count.

**Acceptance Scenarios**:

1. **Given** a file the admission gate excluded from Tier 1, **When** a search scope contains it and
   returns no hits, **Then** the response states how many files in scope were not searched and why —
   never an unqualified "No matches".
2. **Given** a Rust or TypeScript file (both first-class supported languages), **When** it is
   excluded from Tier 1 for any reason, **Then** the reported reason names the **actual** cause and
   never says "unsupported language".
3. **Given** a symbol that exists only in an excluded file, **When** `edit_plan` targets it, **Then**
   the response distinguishes "symbol absent" from "symbol unavailable because the file is
   metadata-only", and every recovery tool it recommends can actually see that target.
4. **Given** `what_changed(uncommitted=true)` with `code_only` omitted, **When** the tree contains
   both source and Markdown changes, **Then** the schema-declared default, the runtime behavior, and
   the result messaging agree with each other.
5. **Given** compact and full health output for one index, **When** both are read, **Then** they
   expose the same denominator and the tier counts visibly sum to the discovered total.

### Edge Cases

- A source contributes zero hits under a multi-source scope → its no-match evidence survives
  composition rather than being lost to concatenation.
- Two provenance digests share a 12-hex prefix within one response → displayed IDs extend until
  unique; semantic rule/policy IDs are never abbreviated.
- A response's CCR footer would not fit after the last whole hit block → the block is withheld, not
  half-emitted.
- A no-match response must keep the `\nNo match:` seam in its exact position or the outcome
  classifier silently misclassifies it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST collect structured per-source results (identity, untruncated hits,
  counts, coverage, no-match evidence) and rank them **globally** by the frozen tuple
  (exact phrase → heading/title → distinct-term coverage → source precedence → canonical path/line)
  before applying `limit` exactly once.
- **FR-002**: The system MUST emit one aggregate overflow, withheld, and authority-filtered count
  set, and an overall coverage equal to the worst included source.
- **FR-003**: The system MUST NOT recover hit boundaries by parsing already-rendered output.
- **FR-004**: The system MUST render each source's real label (`worktree:<id>` / `ref:<name>` /
  `current`) consistently in scope, per-source, and hit fields.
- **FR-005**: Each hit MUST render as an indivisible block ordered answer-first:
  `N. source · path:line`, heading breadcrumb, quoted excerpt, provenance, bridge when present.
- **FR-006**: Displayed identifiers MUST be type-aware: semantic rule/policy IDs verbatim; only
  digests abbreviated, starting at 12 hex and **extending until unique** within the response.
  Full IDs remain resolvable through document-mode `review_knowledge` with no schema change.
- **FR-007**: Bridge previews MUST be partitioned by exact/declared-set/ambiguous/missing, reserve
  at least one slot per present class, and report per-class omitted counts. No class is dropped.
- **FR-008**: Search and review MUST render code anchors through one shared friendly formatter —
  no Rust debug syntax in protocol output.
- **FR-009**: Excerpts MUST be bounded to approximately 240 **Unicode characters**, cut on character
  boundaries, snapped to whitespace, keep the match inside the window, fall through to the next
  substantive line when the match is a heading already in the breadcrumb, and skip
  blank/heading-only/fence-marker/table-separator fallbacks. No Markdown parser is introduced.
- **FR-010**: Budgeted output MUST be produced from a block-safe summary that packs only whole hit
  blocks while reserving space for the retrieval footer, and MUST be applied through
  `apply_ccr_budget_with_summary`. The retrieval store MUST keep the **full** pre-truncation safe
  output.
- **FR-011**: The `\nNo match:` classifier seam MUST be preserved in position and prefix.
- **FR-012**: Lifecycle with no supporting evidence MUST derive `Unknown`, never `Active`.
- **FR-013**: Heading evidence MUST take precedence over path conventions when deriving authority.
- **FR-014**: Path-based authority rules MUST match on tokenized path components (as
  `path_convention_roles` does), never raw substrings, and MUST cover: root `AGENTS.md` /
  `CLAUDE.md` / `GEMINI.md` and `.agent/` → Operations; `docs/solutions/` → Decision;
  `docs/reviews/`, `docs/dogfood/`, `research/` → HistoricalRecord; plan/plans/roadmap components →
  NormativeIntent; tasks/handoff/handover components → Operations; archive(d) → existing
  HistoricalRecord.
- **FR-015**: No new path rule may assign `CurrentImplementation`, and no unit may become
  `Suppressed` as a consequence of this slice.
- **FR-016**: The `search_knowledge` tool description MUST lead with answer-oriented routing intent.
- **FR-017**: Intent classification MUST route noun-anchored prose cues (the five Kimi prefixes plus
  `how does our policy on …` and `how does the process for … work`) to `search_knowledge`, and MUST
  NOT capture generic `how does X work` or any code-intent phrasing.
- **FR-018**: A failing real-corpus fixture proving same-file flooding MUST land and be observed red
  before any diversity rule is added (frozen contract §Query interpretation step 5).
- **FR-019**: Diversity MUST be a two-pass soft quota: a first pass admitting at most 2
  full-query-coverage hits per path in base order, then a spill pass filling remaining slots from
  deferred hits in original base order. No hit is dropped; no tunable weight is introduced.

- **FR-020**: Admission exclusion reasons MUST be structurally distinct (`oversized_file`,
  `unsupported_language`, `binary`, `sensitive_path`, `sensitive_content`, `unreadable`,
  `policy_excluded`, …). No reason may be reported unless it is independently true. Today
  `SkipReason::UnsupportedLanguage` is a catch-all for at least eleven unrelated dispositions
  (`store.rs:3360-3366`, `:3379`, `:3673`), which makes the real cause unobservable through every
  tool surface.
- **FR-021**: A search whose scope contained files excluded from structural indexing MUST NOT return
  an unqualified negative. It MUST report the excluded count and reason class alongside the result.
- **FR-022**: Files in first-class supported languages MUST be structurally indexed unless a
  genuinely disqualifying condition applies; where one currently does not apply and the file is
  excluded anyway, that is a defect to fix — not a coverage caveat to document.
- **FR-023**: Any lexical fallback over excluded files MUST exclude the security dispositions
  (`SensitivePath`, `SensitiveContent`), because the frozen security contract requires
  detector-positive files to remain metadata-only with their bytes and hashes discarded. A fallback
  is therefore only implementable **after** FR-020 makes those dispositions distinguishable.
- **FR-024**: Planner recovery guidance MUST distinguish "symbol absent" from "symbol unavailable
  because the file is metadata-only", and MUST NOT recommend a tool that shares the same blind spot.
- **FR-025**: `what_changed`'s schema-declared default, runtime default, and result messaging for
  `code_only` MUST agree.
- **FR-026**: Compact and full health MUST expose the same denominator, with tier counts summing to
  the discovered total.

### Key Entities

- **Per-source result**: one selected source's identity, real label, untruncated ranked hits, counts,
  coverage, and no-match evidence — the structured input to global composition.
- **Hit block**: the indivisible rendered unit (location, heading, excerpt, provenance, bridge) that
  budgeting may include or withhold but never split.
- **Displayed identifier**: a rendered ID carrying its type — semantic (verbatim) or digest
  (abbreviated to the shortest unique prefix ≥12 hex within the response).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the release-please probe, the first answering excerpt appears by roughly line 8 of
  the response (baseline: no excerpt before the provenance chrome).
- **SC-002**: A default 10-hit response costs at most 60% of the pre-slice byte count.
- **SC-003**: At a 300-token budget the response still carries at least one complete answer block
  plus a retrieval handle; at 120 tokens it carries provenance plus a handle and zero partial blocks.
- **SC-004**: Redeeming any truncated response's handle returns the full pre-truncation output
  byte-for-byte.
- **SC-005**: Under a multi-source scope, the returned set equals the global top-N and the counts are
  single aggregates — verified by a two-source fixture whose per-source behavior would differ.
- **SC-006**: Zero units move into the `suppressed` voice as a result of this slice, and hand-labeled
  authority fixtures pass with no overmatch.
- **SC-007**: Every noun-anchored prose cue routes to the knowledge lane while every listed
  code-intent phrase stays code-routed.
- **SC-008**: The same-file flooding fixture is red before the diversity pass and green after, with
  no single-file corpus underfilled and no hit lost.
- **SC-009**: Frozen contract tests 3, 7, 8, 9, 11, 16, 18 remain green.
- **SC-010**: No code-intelligence tool returns an unqualified negative when its scope contained
  files excluded from structural indexing — verified by a fixture whose only match lives in an
  excluded file.
- **SC-011**: An excluded file's reported reason is the actual disqualifying condition; a supported
  language is never reported as "unsupported language". Verified on `src/protocol/knowledge_search.rs`
  and `src/knowledge/mod.rs`, both of which report "unsupported language" today.
- **SC-012**: `edit_plan` on a symbol inside an excluded file returns a typed unavailability plus a
  next action that works; a regression test asserts it never offers only `search_symbols`.
- **SC-013**: `what_changed`'s declared default, runtime default, and messaging agree — pinned for
  omitted, `true`, and `false`.
- **SC-014**: Compact health's tier counts sum to a stated discovered total.

## Assumptions

- Retrieval quality is **not** the bottleneck — dogfood shows the correct unit already ranks top-3.
  This slice therefore changes composition, presentation, labeling, routing, and ordering only.
- The frozen contracts are law: no MUST-include field is dropped, no bridge class is omitted, CCR
  keeps the full safe output, and the compact surface count is unchanged.
- Aggregate "unknown count" reductions are **diagnostics**, not gates; the gates are the precision
  fixtures and zero unintended `Suppressed`.
- Workstream order (WS0 → WS1 → WS2 → WS3 → WS4) is mandatory because each stage formats or filters
  the previous stage's output.

## Out of Scope

Embeddings or vector retrieval; the Gate-M corpus campaign; manual curation of ~5k units; knowledge
hits inside `explore`; recency/review-file demotion in ranking; ranking configuration knobs; a new
public ID-resolution schema; Markdown/sentence/table parsers; cross-file review self-pollution
beyond WS4's soft quota; the fenced `status:` lifecycle scan follow-up (Kimi C4); completion of
Feature 020 Gates A–M.
