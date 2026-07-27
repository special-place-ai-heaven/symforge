# Kimi K3 — Knowledge LLM Sift: Plan Improvement + Adversarial Review

**Reviewer:** Kimi K3 (independent superior pass)<br>
**Date prepared:** 2026-07-27<br>
**Repo:** `E:\project\symforge` (Rust MCP server, SymForge)<br>
**Branch at brief time:** `fix/windows-self-update-hang` @ `6ec88a5` (plan work is not on this
branch yet; treat as read-only snapshot of current knowledge code)<br>
**Draft plan under review:** Cursor/Grok plan `Knowledge LLM Sift` (verbatim body included below).
Treat it as a **starting draft from a weaker pass**, not as settled product truth.<br>
**Implementation status:** **NOT STARTED.** Do not implement. Your job is to **improve the plan**
and pressure-test the current code surface before anyone codes.

**READ-ONLY on the repo.** Do **not** edit files, run patches, commit, or “just fix it.”
Propose everything inside your report. Diff sketches / `file:function` + new logic are welcome;
touching the tree is not.

---

## Why you are here (read this twice)

We are not hiring you as a rubber-stamp auditor for a finished plan.

We already have a constrained draft (formatter cleanup + authority heuristics + tool description).
That draft may be **too small, too safe, or pointed at the wrong bottleneck.** You are a stronger
model. Your mandate is:

1. **Take SymForge for a spin** (live MCP dogfood) until you *feel* what “indispensable doc
   sifting” should feel like versus what it feels like today.
2. **Improve or replace the plan** with something more precise and superior — including
   alternatives the draft author did **not** think of.
3. **Adversarially** kill weak ideas (including yours) against frozen contracts and the real code.

If you only nitpick the draft’s wording and miss a better product cut, the review failed.
If you only brainstorm blue-sky embeddings/wikis that violate locked constraints, the review
also failed. Nail **better** within reality.

---

## Step 0 — Dogfood SymForge live (MANDATORY before judging the plan)

Use the live SymForge MCP tools against this repo. Do not skip this. The point of knowledge is
the **other side of the coin to code intelligence**: sift `.md` / prose / process / decisions
**without knowing the path**.

Run at least this battery (add more if useful):

| # | Call | Why |
|---|---|---|
| 1 | `health` or `health_compact` | Confirm index ready; skim knowledge health lines |
| 2 | `search_knowledge` query=`how does release please squash merge work` | Known good NL hit → `CLAUDE.md` policy; note how hard the answer is to *read* |
| 3 | `search_text` query=`release please squash merge` | Often misses; contrast lanes |
| 4 | `explore` query=`release please merge commits` | Often drifts to CI/code |
| 5 | `search_knowledge` on 2–3 more “I don’t know the path” questions you invent (process, STEL, token savings, recovery rules) | Feel ranking + envelope tax |
| 6 | `review_knowledge` `mode=summary` | See unknown/broken_anchor noise at portfolio scale |
| 7 | `get_symbol` or `search_symbols` for something code-side | Feel the *good* code-lane UX as the quality bar knowledge must match |

In your report, include a short **Dogfood notes** section: what felt sharp, what felt hostile,
what an indispensable knowledge answer would look like in 10–20 lines. Base plan revisions on
that lived experience, not only on the draft text.

---

## Three jobs (all required)

### Job A — Superior plan / alternatives (primary)

Deliver a **revised plan** or a clearly superior alternative slice. You may:

- Keep the draft’s three workstreams but sharpen them.
- **Reorder, cut, or replace** workstreams if dogfood shows a different bottleneck.
- Propose 1–2 **alternative architectures** the draft missed (still local-first, no embeddings
  unless you prove a measured failure that only embeddings fix — and even then mark it as a
  separate approval-gated follow-on, because the human locked “no embeddings” for this slice).
- Invent product moves we did not list: e.g. ask/facade defaults, hit-complete CCR summaries,
  “answer-first then provenance” packing, diversity rules, heading-biased ranking, agent
  skill/prompt hooks, estimate footers, knowledge-aware `explore`, etc. — **if** they earn
  their keep for LLM sift usefulness.

For every alternative: name it, say why it beats the draft, what it costs, and how it stays
inside frozen contracts (or what tiny contract clarification would be required — prefer no
contract reopen).

### Job B — Adversarial plan review (on the draft AND on your alternative)

Kill contract violations, fake success metrics, scope creep, and “looks better / still useless”
outcomes. Severity-tag findings.

### Job C — Pre-impl code landmines

Read the current formatter/CCR/authority/routing/tests the draft would touch. Find defects and
traps. Concrete fixes only.

Severity: **BLOCKER / HIGH / MEDIUM / LOW**, most severe first.

---

## Hard constraints (do not casually reopen)

These are human locks for *this* slice. You may argue a lock is wrong **only** with dogfood
evidence + a concrete smaller path that still ships usefulness:

- No embedding model / vector DB in this slice.
- No manual curation of ~5k units as the delivery plan.
- No Gate-M corpus campaign as a prerequisite.
- No inventing a parallel “compact mode” knob if the frozen successful-response shape already
  specifies multi-line hits — implement or refine that shape.
- Do not weaken frozen trust/provenance requirements for convenience.
- `curate_knowledge` atomic durability is orthogonal unless you prove search usefulness
  depends on it (unlikely).

Frozen contracts remain authority when code/plan conflict:

- `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
- `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
- `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`

---

## Methodology (MANDATORY — shallow reviews are rejected)

Do **all** of the following after Step 0 dogfood:

1. **Read full function bodies**, never outlines alone. Open the line ranges listed below.
2. **Trace shared helpers and check each argument is used** (especially CCR, secret guards,
   bridge preview builders, authority derivation).
3. **Diff the draft (and your alternative) against the frozen contracts line-by-line.** Where
   anyone says “drop X” or “collapse Y,” quote the clause that allows or forbids it.
4. **Construct concrete adversarial inputs** (exact query, path, expected vs wrong outcome).
5. **Attack CCR / `max_tokens` with multi-line hits.** Trace `apply_ccr_budget` vs
   `apply_ccr_budget_with_summary`.
6. **Attack the TESTS** in `tests/search_knowledge.rs` that assume single-line hits.
7. **Fail-open vs fail-closed** on authority misclassification / voice derivation.
8. **Trace consumers before severity** (tests, ask/facade, STEL, dogfood parsers).
9. **YAGNI-attack your own ideas.** If formatter+routing alone would make knowledge feel
   indispensable after dogfood, say so. If not, name the missing piece precisely.

Ground rules: code is gospel; cite `path:line` or `contract §heading`.

---

## Product intent (why this exists)

SymForge already has strong **code** intelligence. Repository knowledge (Feature 020) is the
**other side of the coin**: an LLM should sift `.md` / text-centric knowledge **without
knowing the path**.

Prior live probe (2026-07-27, drafting model):

| Tool | Query | Outcome |
|---|---|---|
| `search_knowledge` | `how does release please squash merge work` | Hit `CLAUDE.md` “Merging PRs (release-please…)” |
| `search_text` | `release please squash merge` | **No matches** |
| `explore` | `release please merge commits` | Drifted to CI/code |

So the lane already *finds* answers. Draft author’s bet: failure is **LLM usability**
(pipe-soup, header tax, `unknown` noise, weak routing copy). **Your job includes checking
whether that bet is wrong** — e.g. ranking, diversity, ask-routing breadth, envelope packing,
or something else is the real lever.

---

## Draft plan under review (verbatim — starting point only)

```text
# Make repository knowledge indispensable for LLM doc-sifting

## Locked decisions (no menu)

- Goal: LLMs can answer “where is our policy/design/process?” via search_knowledge
  without knowing a path — complementary to code intelligence.
- In scope: formatter UX, deterministic authority heuristics, agent routing copy,
  focused tests.
- Out of scope: embeddings/vector DB, finishing Feature 020 Gate M corpus campaign,
  manual curate_knowledge of ~5k units, changing trust semantics, weakening
  provenance requirements.
- Contract stance: contracts/search-knowledge.md already specifies a clean multi-line
  hit shape; today’s implementation is a single pipe-delimited megeline. We implement
  the frozen shape, we do not invent a parallel “compact mode” knob.

## Workstream 1 — Scannable search_knowledge output

File: src/protocol/knowledge_search.rs

- Rewrite render_response to match the contract example:
  - Short trust/scope header (keep required top-level fields; collapse duplicate
    Source/Derived into one line each).
  - Per hit, multi-line:
    - N. path:line
    - heading breadcrumb
    - quoted excerpt
    - one secondary provenance line:
      source=… hash=… pub/content=… voice/domain/lifecycle=…
    - bridge line ONLY for exact resolutions (drop missing/ambiguous previews from
      the default hit body; they dominate tokens and add no sift value). Stable IDs
      remain available via review_knowledge.
- Preserve CCR behavior and max_tokens truncation rules (complete hits, not
  mid-field chops).
- Update formatter tests under knowledge search / protocol tests that snapshot the
  old pipe shape.

Success check: release-please probe returns the answer excerpt in the first ~15
lines of tool output.

## Workstream 2 — Deterministic authority coverage (cut unknown noise)

File: src/live_index/knowledge_authority.rs derive_native_authority_domain

Extend path/heading rules only (same evidence type RoleRule; add stable rule_ids
like authority-agent-docs-v1):

- Root agent truth files → operations or current_implementation as fits matrix:
  claude.md, agents.md, gemini.md (basename match).
- docs/, docs/dogfood/, docs/solutions/ → normative_intent / operations by
  subpath; docs/archive(d)/ → historical_record.
- plans/, *HANDOFF*, *HANDOVER* → operations / plan-handoff aligned domain.
- Keep CHANGELOG/archive → history (already present).

Do not invent LLM/semantic classification. Do not auto-write .symforge-knowledge.toml.

Success check: review_knowledge(mode=summary) shows material drop in
domain=unknown / voice=unknown for those paths.

## Workstream 3 — Make agents actually call it

- Rewrite #[tool(description=...)] for search_knowledge in src/protocol/tools.rs
  to lead with: prefer for docs/specs/process/decisions when path unknown; use
  search_text/search_symbols for code.
- Mirror in catalog/init only if long descriptions are duplicated.
- Tighten smart_query / STEL planner prose→SearchKnowledge cues only if a small
  deterministic keyword gap exists and a failing routing test is easy.

No new tools. No compact-surface change.

## Verification

- Focused unit tests: formatter golden shape; new authority path cases; existing
  knowledge search contract tests still green.
- Manual dogfood: release-please merge policy, STEL phase intent, token savings
  docs.
- cargo fmt --check, cargo check, focused cargo test for knowledge modules;
  cargo clean after heavy local runs.

## Explicit non-goals

- Reopening Gate M token-corpus campaign as a prerequisite.
- Fixing curate_knowledge atomic durability in this slice.
- Changing frozen authority semantics beyond broader deterministic role coverage.
```

---

## Frozen contract clauses that MUST be attacked against the draft (and your alt)

### From `contracts/search-knowledge.md` — Successful response

The contract’s human-readable example is multi-line and scannable (draft aligns on shape).

But **every hit MUST include** (read the file for exact wording):

- source/worktree/ref label;
- `path:line`;
- exact excerpt;
- heading / unit range when available;
- content hash / object identity;
- published generation;
- lifecycle, authority domain, code-evidence display, voice, finding/provenance IDs,
  evidence coverage;
- **stable link IDs plus bounded exact/declared-set/ambiguous/missing bridge-anchor
  previews when present.**

**Attack:** Does “drop missing/ambiguous from the default hit body” violate “MUST include …
ambiguous/missing … when present”? If yes, propose a **contract-safe** superior packing that
still feels LLM-first. Do not weaken the frozen contract for convenience.

Also attack header collapse vs “Top-level response MUST include …” field list, and truncation
test 7 (“complete provenance + CCR handle”) under multi-line hits.

### From `contracts/knowledge-authority-hygiene.md`

Domain may come from deterministic role rules; voice is derived. Mis-labeling changes what
`authority_scope` returns. Propose a precise path→domain table if you keep WS2 — or argue WS2
is the wrong lever after dogfood.

### From `smart_query` / ask routing

Knowledge routing is intentionally **narrow** (explicit doc phrases) so code questions are not
stolen. Any broader cue list needs a false-positive table.

---

## Current code surface to read (pre-impl)

### Primary — formatter / search

| Path | What to open |
|---|---|
| `src/protocol/knowledge_search.rs` | `render_response` (~L699–810); `bridge_previews` (~L667–696); `bridge_resolution_preview` (~L995–1012); hit assembly + secret `guard_hit` (~L400–420); `search_current` / scoped entry |
| `src/protocol/tools.rs` | `search_knowledge` / `search_knowledge_tool` (~L5412+); `apply_ccr_budget("search_knowledge", …)` |
| `src/protocol/mod.rs` | `apply_ccr_budget` vs `apply_ccr_budget_with_summary` (~L877–911) |
| `src/protocol/format.rs` | `enforce_token_budget` / `truncate_text_at_line_boundary` (~L5293–5365) |
| `src/protocol/ccr.rs` | `search_knowledge` profile; `enforce_token_budget_with_ccr` / `apply_ccr_overflow` |

### Authority

| Path | What to open |
|---|---|
| `src/live_index/knowledge_authority.rs` | `derive_native_authority_domain` (~L1101–1154); voice derivation (~L264+) |
| `src/live_index/knowledge_bridge.rs` | `role.path.*` rules — parity vs authority domain heuristics |

### Routing / descriptions

| Path | What to open |
|---|---|
| `src/protocol/tools.rs` | current `#[tool(description = …)]` |
| `src/protocol/smart_query.rs` | `SearchKnowledge` intent (~L105–122, L519–522, L588–619) |
| `src/stel/planner.rs` | `search_knowledge` routing (~L298, L1171+) |
| `tests/search_knowledge.rs` | ask routing test; field asserts ~L166–180; bridge ~L391–424; CCR ~L558–608 |

### Live hygiene baseline (approx. at draft time)

`review_knowledge(mode=summary)` ≈ `total=5112`, heavy `unknown` domain, many `broken_anchor` /
`review_due`, truncated bridge/authority coverage. Draft metric = drop unknown on newly ruled
paths — attack whether that metric can move while sift UX stays flat.

---

## Seeded adversarial scenarios (expand; add your own from dogfood)

### A. Contract vs “drop missing/ambiguous bridges”
### B. CCR mid-hit chop with multi-line blocks + `max_tokens=120`
### C. Authority misroute / wrong voice after path rules
### D. Routing theft if cues broaden (`find references to X` must stay code)
### E. Test vacuity after multi-line format
### F. **Superior alternatives the draft missed** (your main creative job)

Examples to pressure (accept, reject, or replace with better):

- Hit-complete CCR summary via `apply_ccr_budget_with_summary` (pseudocode).
- Answer-first packing: excerpt+heading before any trust chrome; provenance as footer.
- Contract-safe bridge packing: exact shown; nonexact as `bridge_nonexact_omitted=N` + IDs.
- `ask` / tool-description / AGENTS guidance that makes knowledge the default for prose.
- Ranking: heading/title boost already exists — is diversity or same-file flooding the issue?
- Knowledge-aware first hop inside `explore` without merging prose into code symbols.
- Something **you** invent after Step 0 that we did not list.

---

## Required report format

```text
# Kimi K3 — Knowledge LLM Sift Review

## Dogfood notes
What you called, what hurt, what “indispensable” should feel like (10–20 lines ideal answer).

## Verdict
APPROVE_DRAFT_AS_IS | REVISE_DRAFT | REPLACE_WITH_ALTERNATIVE
One paragraph. Be honest if the draft aimed at the wrong bottleneck.

## Recommended plan (your improved or replacement slice)
Workstreams, success checks, non-goals, ordered. This is the artifact we may implement.
Keep it constrained and shippable. No option menus — pick and justify.

## Alternatives considered (including ones the draft missed)
For each: name, why better/worse, cost, contract fit, why you kept or rejected it.

## Plan findings (draft and/or your alt)
### BLOCKER / HIGH / MEDIUM / LOW
- Claim / scenario / contract cite / concrete fix

## Current-code findings (pre-impl landmines)
(same structure; path:line)

## Test plan deltas
Exact tests to add/rewrite.

## Adversarial inputs exercised
A–F + dogfood-invented extras.
```

---

## Out of scope for this review

- Implementing code.
- Unrelated branch work (Windows self-update, etc.).
- Weakening frozen contracts for convenience.
- “Just ship embeddings” without a measured failing corpus and explicit human reopen.

---

## Operator note (human → Kimi)

1. Paste this brief as the primary prompt.
2. Ensure SymForge MCP is connected so Step 0 dogfood is real.
3. Optionally attach the two frozen contracts.
4. After the report: merge Kimi’s **Recommended plan** into the Cursor plan (or replace it),
   then implement — do not code from the unrevised draft if verdict is not
   `APPROVE_DRAFT_AS_IS`.
