# Kimi K3 — Knowledge LLM Sift Review

**Reviewer:** Kimi K3 · **Date:** 2026-07-27 · **Mode:** read-only, no tree edits
**Basis:** live MCP dogfood (8 calls) + full-body code reads + line-by-line contract diff.

## Dogfood notes

Calls made: `health`, `search_knowledge` ×4 (release-please policy; contract's own
persistence-boundary example; token-savings question; `max_tokens=120` adversarial),
`search_text`, `explore`, `review_knowledge(mode=summary)`, `search_symbols` (code-lane bar).

**What felt sharp.** The lane *finds*. Every NL query returned the semantically right
unit in the top 3 (release-please → `CLAUDE.md:39`; persistence boundary →
`docs/recovery.md`-class hits; token savings → CHANGELOG 8.15.0 honest-economics entry).
Phrase match, heading boost, deterministic ranking all work. `search_text` missing
`release please squash merge` confirms the lanes are complementary, not redundant.

**What felt hostile.**

1. **Header tax:** 6 envelope lines (~150 tokens) before hit 1; the `Source:` line alone
   is 300+ chars with two full 64-hex digests. The contract's own example shows
   `hash=<bounded-id>` — the implementation prints unbounded hashes.
2. **Megeline hits:** one pipe-delimited line per hit, ~250 tokens each, vs the contract's
   multi-line shape. Reading hit 3 of 10 means parsing 2,500 tokens of provenance chrome.
3. **Bridge noise:** `bridge_previews` dominated by `:missing` entries (58% of units are
   `broken_anchor`; bridge coverage `truncated(14758 omitted)`), each preview a 64-hex ID,
   and resolved symbols rendered as Rust debug text:
   `symbol:SymbolId { path: "src/protocol/mod.rs", name: "explore", kind: Module }:8`.
4. **Authority noise:** 76% `domain=unknown` (3894/5142), 73% `voice=unknown`, so the
   authority clause on nearly every hit is `domain=unknown code=broken_anchor
   voice=unknown` — zero decision value, full token cost.
5. **Excerpt quality:** excerpt = the whole matching line, uncapped
   (`match_unit`, knowledge_search.rs:566–589). A 1.5KB markdown table row became one
   hit's excerpt; heading matches render the heading line as the excerpt, duplicating the
   breadcrumb; other hits were mid-sentence fragments (` boundary precisely.`).
6. **Flooding:** 3/5 hits from one research handoff file on the persistence query; 3/10
   from `013-HANDOFF.md` on the release query. The canonical policy ranked #3 behind the
   review brief itself (twice) — phrase match working as designed, diversity absent.
7. **Tiny budgets starve the answer:** `max_tokens=120` returned header-only + CCR handle.
   Contract-permitted ("provenance-only bounded response") but the megeline format means
   no excerpt survives any budget under ~400 tokens.

**Code-lane bar** (`search_symbols`): 3-line trust header, then one line per answer.
That is the shape knowledge must match.

**What "indispensable" looks like** (ideal release-please answer, ~14 lines):

```text
Trust: exact repository knowledge | pub=35 content=23 | current | degraded
Scope: current + repository · secret policy v1 · overflow=1547 withheld=0
Source: current 6d8c9fd204b0 pub=35 content=23 dirty degraded_last_valid digest=655d7a8a9f3a
Derived: rules v1 policy v1 bridge=truncated authority=truncated

1. CLAUDE.md:39
   CLAUDE.md — SymForge > Merging PRs (release-please visibility)
   "`git log --no-merges` at the CI gate doesn't change what release-please …"
   current · active/operations · voice=needs_review · hash=a3351570737b pub=35/23 lines 23..96
   bridge: ci.yml (exact file) · 2 missing, 1 ambiguous (+12 omitted) · ids via review_knowledge
2. docs/backlog.md:135 …
```

Answer excerpt on line ~5, provenance present but demoted to one compact line per hit.

## Verdict

**REVISE_DRAFT.**

The draft's core bet is **correct**: the lane retrieves well and fails on LLM usability —
this is a formatter/ranking-output problem, not a retrieval problem. But the draft as
written (a) contains one direct frozen-contract violation (dropping missing/ambiguous
bridge previews), (b) names no mechanism for its own CCR requirement and the naive one
(line-cut) breaks contract test 7 the moment hits go multi-line, (c) misses the excerpt
quality defect entirely — the single biggest readability lever after the reformat,
(d) under-scopes WS2 (skips `research/` 51 files and `.agent/` 58 files; keeps substring
matching that overmatches; leaves a voice-suppression trap in "current_implementation"),
and (e) states an unmeasurable success metric. WS3 is right and is additionally backed
by a frozen contract clause the draft didn't cite. Keep the three-workstream skeleton,
sharpen all three, add a fourth (diversity, contract-gated).

## Recommended plan (implementable artifact)

### WS1 — Contract-shaped, answer-first `search_knowledge` output
Files: `src/protocol/knowledge_search.rs`, `src/protocol/tools.rs`, `tests/search_knowledge.rs`.

1. **Multi-line hits per the contract example** (`contracts/search-knowledge.md`
   §Successful response): `N. path:line` / heading breadcrumb / quoted excerpt / one
   secondary provenance line carrying source label, bounded hash, generations, line
   range, lifecycle/domain/code-evidence/voice/coverage, finding+provenance IDs
   (bounded, with omitted counts).
2. **Bounded IDs everywhere.** Contract example endorses `hash=<bounded-id>`; render
   content_hash, source_id, manifest_digest, finding/provenance/bridge IDs as 12-hex
   prefixes. Full values remain resolvable via `review_knowledge`. This alone cuts
   ~100+ chars/hit and ~130 chars from the header.
3. **Compact header:** keep every contract-required top-level field (§Successful
   response, bullet list) but one line per category: Trust, Secret policy, Scope,
   Source (single-source case: one line), Derived, Counts. No field dropped.
4. **Contract-safe bridge packing:** exact/declared-set previews rendered inline with
   friendly anchors (`file:path`, `symbol:path#name:line`); ambiguous/missing previews
   kept as per-ID short forms (`<id12>:missing`, `<id12>:ambiguous:3`), canonically
   ordered, each class bounded, explicit omitted counts. **Nothing is dropped** —
   this satisfies "bounded exact/declared-set/ambiguous/missing bridge-anchor previews
   when present" and test 18. `review_knowledge` remains the full-record path.
5. **Fix the `SymbolId { … }` debug leak** at `code_anchor_label`
   (knowledge_search.rs:~1015) — `format!("symbol:{symbol:?}:{start_line}")` emits Rust
   debug syntax into a frozen contract surface. Render `symbol:{path}#{name}:{line}`.
6. **Excerpt windowing** (new, the draft misses this): cap excerpts (~240 chars);
   window around the match with ellipsis at sentence/whitespace bounds; when the best
   matching line is a heading already present in the breadcrumb, take the first
   substantive content line of the unit instead; never emit a 1.5KB table row.
7. **Hit-complete CCR.** Build a *summary* = full header + as many whole hit blocks as
   fit, and switch tools.rs:5461 from `apply_ccr_budget` to the already-existing
   `apply_ccr_budget_with_summary` (mod.rs:894). The generic line-boundary cut in
   `enforce_token_budget` (format.rs:5351) would otherwise chop multi-line hits
   mid-block, breaking contract test 7 ("truncation retains complete provenance and
   CCR handle"). Mechanism exists; no new machinery.
8. Thread the real source label into per-source sections instead of the hardcoded
   `source=current` in the hit line (knowledge_search.rs:781) for multi-source scopes.

Success check: release-please probe shows the answer excerpt within the first ~8 lines;
10-hit default response ≤ ~60% of today's bytes; `max_tokens=300` returns header + ≥1
complete hit + CCR handle; contract tests 3/7/18 green.

### WS2 — Deterministic authority coverage (precise table, component-tokenized)
File: `src/live_index/knowledge_authority.rs` `derive_native_authority_domain` (L1101–1154).

Match on **path components** (tokenize like `path_convention_roles` in
knowledge_bridge.rs:743 — same evidence type `RoleRule`, new stable rule IDs), not raw
substrings. Keep heading precedence as today. Table:

| Path rule | Domain | Rule ID |
|---|---|---|
| basename ∈ {claude.md, agents.md, gemini.md} | **Operations** | `authority-agent-instructions-v1` |
| `.agent/` | Operations | `authority-agent-instructions-v1` |
| `docs/solutions/` | Decision | `authority-solutions-v1` |
| `docs/reviews/`, `docs/dogfood/`, `research/` | HistoricalRecord | `authority-review-log-v1` |
| `plans/`, `tasks/`, filename component contains `handoff`/`handover` | Operations | `authority-plan-handoff-v1` |
| `docs/archive(d)/` | HistoricalRecord | (existing `authority-history-v1`) |

Hard constraints on this table (see findings H3/H4): **never** assign
`CurrentImplementation` from a new path rule in this slice (its
`DeterministicConflict → Suppressed` voice path can hide units from default scope);
no LLM/semantic classification; no auto-writing `.symforge-knowledge.toml`;
no blanket `docs/*` rule — heading rules keep jurisdiction there.

Success check (measurable): `review_knowledge(mode=summary)` before/after on this repo —
`domain=unknown` drops from 3894 to <2500 and `voice=unknown` from 3767 to <2400, with
zero units moving into `suppressed`. (Baseline captured 2026-07-27, see Dogfood notes.)

### WS3 — Make agents actually call it
File: `src/protocol/tools.rs:5413` (single source of the description; verified no
catalog/init duplication).

Rewrite the `search_knowledge` description to lead with routing intent:
"Search the repository's prose knowledge — docs, specs, plans, decisions, process,
runbooks — when you don't know the path. Prefer this for 'what is our policy/design/
process for X' questions. Use search_text/search_symbols for code. …" (keep the
provenance/scope sentence after it).

Routing cue widening in `smart_query.rs` (~L110): add ONLY doc-noun-anchored prefixes —
`what is our policy on `, `where is our policy on `, `what is the process for `,
`where is the runbook for `, `what does the spec say about `. This is contract-backed:
`contracts/repository-mental-model.md` §ask — *"Focused factual follow-up routes to
search_knowledge; code/symbol/reference intent remains code intelligence."* Ship with
the false-positive table (finding D): `find references to X`, `where is search_knowledge
defined`, `retry policy in the client code` must stay code-routed; extend
`ask_routes_explicit_knowledge_intent_without_stealing_code_intent` accordingly.
STEL planner needs no change (it delegates to `classify_intent`, planner.rs:298).

### WS4 — Same-file diversity (contract-gated)
Contract §Query interpretation step 5 permits diversity "only when a failing corpus
fixture proves same-file flooding." Dogfood supplies the motivation; the fixture makes
it legal. Add a ranking fixture where >2 hits from one file outrank a distinct-file
canonical hit; then apply a deterministic diversity rule (e.g. after 2 hits per path,
further same-path hits sort below the next distinct-path hit, stable tie-breaks
unchanged). Small, deterministic, reversible.

### Non-goals (unchanged from draft, plus)
- No embeddings/vector DB; no Gate-M corpus campaign; no manual curation of ~5k units;
  no trust-semantics changes; no `curate_knowledge` durability work.
- No knowledge hits inside `explore` (lane blur; see Alternatives).
- No blanket `docs/` or `specs/` authority reclassification beyond the table above.

### Verification
- Rewrite megeline-dependent tests (Test plan deltas below); golden-shape formatter test
  on the multi-line shape; block-completeness truncation test replacing per-line asserts.
- New: excerpt-window unit tests (table row, heading-line match, mid-sentence);
  authority table unit tests incl. overmatch guards (`/special/` ≠ spec);
  routing false-positive tests; diversity fixture.
- `cargo fmt --check`, `cargo check`, focused `cargo test knowledge` + `search_knowledge`;
  manual dogfood battery re-run (this report's Step-0 queries) with before/after output
  pasted into the PR.

## Alternatives considered

1. **Hit-complete CCR via `apply_ccr_budget_with_summary`** — KEPT (WS1.7). The function
   exists (mod.rs:894) precisely for "structured multi-line results whose generic line
   cut could expose half a record." Zero new machinery; directly satisfies test 7.
2. **Answer-first packing** — KEPT in modified form. The contract example already puts
   excerpt before provenance chrome; we follow it rather than inventing a new order.
   Contract-safe and familiar.
3. **Draft's "drop missing/ambiguous previews from the default hit body"** — REJECTED.
   Violates §Successful response ("MUST include … bounded
   exact/declared-set/ambiguous/missing bridge-anchor previews when present") and test
   18 (previews "survive formatting, truncation, cross-project envelopes, and CCR").
   The value is real (they're mostly noise) but the fix is compaction, not omission:
   `<id12>:missing` costs ~17 chars/entry vs ~90 today. See Finding B1.
4. **Knowledge-aware first hop inside `explore`** — REJECTED. `explore` is the
   code/concept lane; the mental-model contract assigns knowledge its own surfaces and
   forbids prose becoming code-search hits. Mixing lanes re-opens the theft problem WS3
   is careful about. The better lever is the tool description + ask cues (WS3).
5. **Blanket `docs/` → one domain** — REJECTED. `docs/` mixes current architecture,
   reviews, dogfood logs, and solutions; a blanket rule mislabels at scale and can push
   units into `Suppressed`. Subpath table only.
6. **Embeddings/vector retrieval for ranking** — REJECTED (locked, and dogfood shows
   retrieval is not the bottleneck: the right unit is already top-3).
7. **Recency demotion for handoff/research files in ranking** — REJECTED. The proof
   matrix forbids age → staleness conclusions; ranking demotion by file class is a
   trust-semantics change. Diversity (WS4) captures most of the value legally.
8. **Dropping the `Derived:`/`Counts:` header lines** — REJECTED. Both are in the
   contract's top-level MUST list. Compact, don't cut.

## Plan findings (draft and/or alt)

### BLOCKER
- **B1 — Draft WS1 violates frozen contract test 18 / §Successful response.**
  "Drop missing/ambiguous previews from the default hit body" contradicts "Every hit
  MUST include … stable link IDs plus bounded exact/declared-set/ambiguous/missing
  bridge-anchor previews **when present**" and test 18 ("…ambiguous/missing bridge
  previews survive formatting, truncation, cross-project envelopes, and CCR").
  Scenario: any hit with only missing/ambiguous links (58% of units are broken_anchor)
  silently loses link identity from search; agents can no longer see that a doc's code
  anchors are broken — a trust signal, not noise, at the unit level.
  **Fix:** compact per-class rendering (WS1.4): exact inline with friendly anchors,
  missing/ambiguous as `<id12>:<class>` tokens, canonically ordered, bounded per class,
  explicit omitted counts. Keeps the token win (~80% of preview bytes) without dropping
  a single ID class.

### HIGH
- **H1 — Multi-line reformat silently reintroduces mid-hit chops.**
  tools.rs:5461 calls `apply_ccr_budget` → `enforce_token_budget_with_ccr` →
  `truncate_text_at_line_boundary` (format.rs:5351). Safe today only because each hit is
  one line. After WS1, a line cut can keep `path:line`+heading and drop the excerpt and
  provenance of the boundary hit — exactly what contract test 7 forbids. The draft says
  "preserve CCR behavior (complete hits, not mid-field chops)" but names no mechanism.
  **Fix:** build a hit-complete summary and call `apply_ccr_budget_with_summary`
  (mod.rs:894) — its doc comment describes this exact case.
- **H2 — Excerpt is an uncapped raw line; draft doesn't mention it.**
  `match_unit` (knowledge_search.rs:566–589) stores `line.to_string()` as the excerpt.
  Dogfood: a 1.5KB table row became one hit's excerpt (research/token-cost/log.md);
  heading matches duplicate the breadcrumb (`excerpt="### Honest token economics…"`);
  others were mid-sentence fragments. Even after WS1's reformat, excerpts stay
  unreadable/unbounded without WS1.6. **Fix:** window + cap + heading-skip as specified.
- **H3 — WS2's "current_implementation as fits matrix" is a suppression trap.**
  `derive_voice` (knowledge_authority.rs:264+):
  `CurrentImplementation + DeterministicConflict → Suppressed`, and Suppressed units are
  hidden from default/current scopes. Assigning `CurrentImplementation` from a *new path
  rule* converts a labeling improvement into a retrieval regression the first time a
  structured extractor flags a conflict. `Unknown`/`Operations` never suppress.
  **Fix:** Operations for agent-instruction/plan paths (table above); no new
  CurrentImplementation path rules this slice.
- **H4 — WS2 metric can't move as drafted, and is unmeasurable.**
  Rules cover root agent files (3 files), `docs/` subpaths, `plans/` (1 file) — but skip
  `research/` (51 files) and `.agent/` (58 files), two of the three biggest unknown
  populations (repo knowledge files: specs 225 [already classified via `/spec`],
  docs 112, .agent 58, research 51). "Material drop" has no number.
  **Fix:** add the two path classes (table above); success = before/after
  `review_knowledge` counts with the 2026-07-27 baseline in this report.
- **H5 — Substring path matching overmatches.**
  Current code: `path_lower.contains("/spec")` matches `/special/`, `/inspection/`;
  `/design` matches `/redesign/`; `changelog` matches `changelog-notes.md` (arguably
  fine) — and the draft would add more rules in the same style.
  **Fix:** tokenize path components like `path_convention_roles`
  (knowledge_bridge.rs:743) and match components/filenames exactly; add overmatch unit
  tests (`docs/special/report.md` must stay Unknown).

### MEDIUM
- **M1 — `SymbolId { … }` debug leak in a frozen surface.** `code_anchor_label`
  (knowledge_search.rs:~1015) formats resolved symbols with `{symbol:?}`, emitting
  `SymbolId { path: "…", name: "…", kind: Module }:8`. Ugly, and it bakes Rust-internal
  type names into output agents parse. **Fix:** `symbol:{path}#{name}:{line}` (WS1.5).
- **M2 — Draft WS3's conditionality dodges the one contract-backed cue change.**
  "Tighten cues only if a small deterministic gap exists" — the gap exists and is
  contract-mandated (mental-model §ask: "Focused factual follow-up routes to
  search_knowledge"). Natural questions like `how does release please squash merge work`
  currently fall to `Explore` fallback. **Fix:** the five doc-noun prefixes (WS3) + the
  false-positive table.
- **M3 — WS1 success check measures the wrong thing.** "Answer excerpt in the first ~15
  lines" is fine but incomplete: it doesn't test truncation behavior, which is where the
  format change is riskiest. **Fix:** add the `max_tokens=300` complete-hit check (WS1
  success criteria).

### LOW
- **L1 — Hardcoded `source=current` in the hit line** (knowledge_search.rs:781). Under
  `source_scope=all/worktrees/local_refs`, `search_scoped` (L259–289) renders per-source
  sections whose hits still say `source=current`. Disambiguated by the section banner,
  but the label is wrong text. **Fix:** thread the section's source label (WS1.8).
- **L2 — `Counts: overflow=1547` reads as truncation overflow**, not "unshown matches
  beyond limit." Contract-compliant; renaming would churn parsers. Note only.

## Current-code findings (pre-impl landmines)

### HIGH
- **C1 — `match_unit` excerpt selection is also a ranking-input hazard.**
  knowledge_search.rs:566–589: the "best line" tie-break `(phrase, terms, Reverse(offset))`
  prefers the *earliest* max-scoring line; for units whose first line is a markdown
  title (`# Independent review handoff…`), the excerpt is the title even when a later
  line has equal score but real content. Combined with no cap, excerpt quality is the
  weakest user-facing link in the whole lane. Fix in WS1.6; add unit tests for
  title-line, table-row, and mid-sentence cases.
- **C2 — CCR eligibility + megeline assumptions are load-bearing and invisible.**
  `search_knowledge` is `ccr_eligible` with an 8000-token default (ccr.rs:29–33) and
  CCR stores are tagged with `secret_policy_version` (ccr.rs:163). Any WS1 change to
  *what* is stored (summary vs full) must keep storing the **full pre-truncation safe
  output** — `apply_ccr_overflow` (ccr.rs:232) already does this; do not "optimize" it
  to store the summary. A CCR round-trip returning a degraded document violates test 7's
  handle semantics.

### MEDIUM
- **C3 — Authority rules and bridge role rules are parallel vocabularies that have
  already drifted.** `path_convention_roles` knows `plan|plans|handoff|handoffs|tasks|
  roadmap`, `architecture|design`, `contract|schema` (knowledge_bridge.rs:753–778);
  `derive_native_authority_domain` knows none of `handoff/tasks/plans` and matches by
  substring. WS2 should converge the two token vocabularies (same component tokenizer)
  or the drift compounds. Draft's WS2 doesn't mention the bridge table exists.
- **C4 — `derive_native_lifecycle` scans every line of every unit for `status:`**
  (knowledge_authority.rs:1075–1090) — any markdown body line starting with
  `status: draft` (including in a quoted code block or a table) declares lifecycle for
  the unit. Pre-existing, out of this slice, but WS2's path rules will make more units
  authority-labeled, raising the blast radius of a false `status:` hit. Flag for a
  follow-up; do not fix here.
- **C5 — `classify_search_knowledge_output` keys on `"\nNo match:"`**
  (tools.rs:277–280). WS1's reformat must keep the no-match line in that exact
  position/prefix or the outcome classifier (and the STEL dependent-chain special-case
  at tools.rs:10667) silently misclassifies typed no-match answers. Add a test pinning
  the no-match line format.

### LOW
- **C6 — `overflow` counts units, not hits.** `overflow = hits.len() - limit` after
  dedup (knowledge_search.rs:455–457) — fine, but the Counts line doesn't say "matches,"
  and agents may read it as truncation. Covered by L2.
- **C7 — Guard coverage asymmetry:** `guard_hit` receives path/heading/excerpt/hash/
  IDs/bridge fields (knowledge_search.rs:406–418) but not the rendered line_range or
  `source=current` label — fine today (both deterministic non-content), just keep the
  invariant when adding fields to the hit block in WS1: anything rendered from document
  bytes must go through `visible_fields`.

## Test plan deltas

Rewrite (megeline-coupled):
- `tests/search_knowledge.rs::exact_hit_and_complete_no_match_preserve_captured_provenance`
  (L140–186): field asserts `authority:`, `finding_ids=`, `bridge_previews=` become
  block-aware asserts on the multi-line shape; add golden-shape snapshot of one full hit.
- `ccr_truncation_withholds_partial_hits_and_round_trips_full_safe_output` (L558–608):
  the per-line filter `line.contains("docs/")` + `assert!(line.contains("authority:"))`
  breaks on multi-line hits (heading/excerpt lines contain `docs/` but not `authority:`).
  Rewrite as block-completeness: every emitted hit block contains all four sub-lines;
  no block is partial; CCR round-trip still yields full output.
- `bridge_preview_carries_stable_link_id_without_full_bridge_record` (L391–424):
  keep `contains("src/lib.rs")` (exact preview now friendly-formatted `file:src/lib.rs`);
  add asserts that missing/ambiguous classes still appear as compact tokens with omitted
  counts (pins B1's contract-safe packing).
- `ranking_is_canonical_and_byte_deterministic…` (L385–390): unchanged logic, but re-pin
  expected output strings to the new format; add the WS4 diversity fixture as a separate
  test (same-file flooding → distinct-file hit promoted), proving the contract step-5 gate.

Add:
- Excerpt windowing unit tests: >1KB table row capped with ellipsis; heading-line match
  falls through to first content line; mid-sentence fragments snapped to word bounds.
- Authority table tests: each new rule ID fires on its path class; `docs/special/x.md`,
  `docs/redesign/x.md`, `docs/inspection/x.md` stay Unknown; CLAUDE.md → Operations and
  never Suppressed under a simulated DeterministicConflict.
- Routing: each new doc-noun prefix routes to `search_knowledge`;
  `find references to X`, `where is search_knowledge defined`,
  `retry policy in the client code` stay code-routed (extend the existing ask test).
- No-match line format pinned (`"\nNo match: <class>"`) for the outcome classifier (C5).
- Small-budget: `max_tokens=300` → header + ≥1 complete hit + CCR handle;
  `max_tokens=120` → provenance-only + handle, no partial hit block.

## Adversarial inputs exercised

- **A (contract vs dropped bridges):** construct any hit whose links are all
  missing/ambiguous (majority case today — bridge coverage `truncated(14758 omitted)`).
  Draft WS1 output would show zero bridge evidence → test 18 violation. Resolved by
  compact-per-class packing (B1).
- **B (CCR mid-hit chop, `max_tokens=120`):** exercised live — header-only + CCR handle,
  zero hit content. Post-WS1 without WS1.7 the same budget could emit 2/4 lines of a hit.
  Resolved by `apply_ccr_budget_with_summary` (H1).
- **C (authority misroute / wrong voice):** traced `derive_voice` — new
  CurrentImplementation path rules can flip units to Suppressed and hide them from
  default scope (H3). Also: `docs/special/` matches `contains("/spec")` today (H5).
- **D (routing theft):** `find references to X` is prefix-anchored to callers
  (smart_query.rs:126+) and cannot be stolen by doc-noun prefixes; false-positive table
  specified in WS3. `where is search_knowledge defined` already test-pinned as
  code-routed.
- **E (test vacuity after multi-line):** the CCR truncation test's per-line
  `contains("authority:")` assertion passes vacuously on megelines and breaks on
  multi-line — demonstrated by construction (L575–580). Rewrite specified.
- **F (alternatives):** see Alternatives considered — draft's bridge drop rejected on
  contract text; explore-first-hop rejected on lane contract; recency demotion rejected
  on proof matrix.
- **Dogfood extras:**
  - *Self-pollution:* this review brief (indexed 220s before the query) took ranks 1–2
    on the release-please probe over the canonical CLAUDE.md policy — phrase match
    correct, diversity absent. Motivates WS4, not a defect.
  - *Research flooding:* 3/5 hits from one handoff file on the persistence query.
  - *Table-row excerpt:* 1.5KB excerpt from `research/token-cost/log.md` — H2/C1.
  - *Baseline captured:* total=5142, unknown domain=3894, voice unknown=3767,
    broken_anchor=2979, review_due=2163, duplicate_units=35 — WS2's before-snapshot.
