# Implementer Handoff — Knowledge LLM Sift (2026-07-27)

**Intended implementer:** a **top-tier coding model** (Claude Opus 5, Codex, GPT-5.6-sol coding, etc.). This slice is contract-dense and easy to get subtly wrong — do not assign a weak model.<br>
**Repo:** `E:\project\symforge`<br>
**Slice:** Make repository knowledge indispensable for LLM doc-sifting (usability + multi-source composition). **Not** “complete all of SpecKit 020 Gates A–M.”

Feature 020 already ships `search_knowledge` / `review_knowledge` / `curate_knowledge`. This round hardens the LLM-facing cut after dual adversarial review (**Kimi K3 → GPT-5.6-sol ultra**). Product direction is settled; mechanics in the final plan are the law.

---

## Why a strong model

- Frozen SpecKit contracts must be honored (MUST-include fields, CCR semantics, authority voice).
- WS0 requires a real structural refactor (per-source extract → global rank/limit), not a formatter-only patch.
- ID abbreviation, Unicode excerpt offsets, soft diversity, and CCR block packing are easy to “almost” get wrong.
- Dual review already killed several plausible-but-unsafe shortcuts (drop bridges, fixed 12-hex, hard 2-per-path, CurrentImplementation path rules).

If tempted to simplify by weakening a contract or skipping WS0, stop — that is a failed implementation.

---

## Authority order (read in this order)

1. `AGENTS.md` + `CLAUDE.md` (verification gates, Windows `cargo clean` discipline)
2. **Final plan (implement exactly this):**  
   `C:\Users\rakovnik\.cursor\plans\knowledge_llm_sift_56bece4f.plan.md`
3. GPT response (final mechanics):  
   `E:\project\symforge\specs\020-repository-knowledge-index\GPT56-REVIEW-RESPONSE-knowledge-llm-sift-2026-07-27.md`
4. Kimi response (dogfood + original blockers):  
   `E:\project\symforge\specs\020-repository-knowledge-index\KIMI-REVIEW-RESPONSE-knowledge-llm-sift-2026-07-27.md`
5. Frozen contracts (do not weaken):
   - `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
   - `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
   - `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`

Do **not** treat full `GOAL.md` Gate A–M completion as this slice’s exit criteria.

---

## SpecKit (optional scaffolding)

SpecKit is available in-repo (`.claude/skills/speckit-*`). Use it if helpful:

1. Turn the Cursor plan into RED/GREEN/VERIFY tasks with IDs `SIFT-WS0` … `SIFT-WS4`.
2. Execute **in order: WS0 → WS1 → WS2 → WS3 → WS4 → verify**.
3. `/speckit-implement` may drive the task list; the Cursor plan remains product authority.

If not using SpecKit, still implement WS0–WS4 in that order with the same tests.

---

## Branch

```text
feat/knowledge-llm-sift
```

From current `main` (or the branch the human names). No unrelated work. No push unless asked.

---

## Workstreams (order is mandatory)

| ID | Summary |
|---|---|
| **WS0** | Structural per-source extract → **global** rank/limit/aggregates; real source labels; no string-parse hit recovery |
| **WS1** | Answer-first hit blocks; type-aware collision-extending digest IDs; bridge class slots; shared code-anchor formatter; Unicode-safe ~240-char excerpts; block-safe CCR summary + `apply_ccr_budget_with_summary`; keep `\nNo match:` |
| **WS2** | Unevidenced lifecycle Active→Unknown; heading before path; component-token table; no new CurrentImplementation; HistoricalRecord still default-visible |
| **WS3** | Tool description rewrite; noun-anchored ask prefixes (+ two how-does-our-*); false-positive code tests; no blanket Understand steal |
| **WS4** | Failing flood fixture first; two-pass soft diversity (≤2 full-coverage hits/path + spill) |

Primary files: `src/protocol/knowledge_search.rs`, `src/protocol/tools.rs`, `src/protocol/knowledge_review.rs`, `src/protocol/mod.rs`, `src/protocol/ccr.rs`, `src/live_index/knowledge_authority.rs`, `src/protocol/smart_query.rs`, `tests/search_knowledge.rs`.

---

## Hard non-goals

Embeddings; Gate-M corpus; manual ~5k curation; explore mix; recency demotion; ranking config; new public ID-resolve schema; Markdown/sentence parsers; dropping bridge classes; storing CCR summary instead of full safe output.

---

## Verification

- Contract tests **3, 7, 8, 9, 11, 16, 18**
- Extra tests listed in the final plan (global limit, collision extend, Unicode excerpts, soft diversity, etc.)
- `cargo fmt --check`, `cargo check`, focused knowledge/`search_knowledge` tests, then broader if needed
- Manual dogfood: release-please policy, persistence boundary, `max_tokens=120/300` — before/after; measure bytes + answer line position
- After heavy local cargo: `cargo clean` (CLAUDE.md)

---

## Success

An LLM asking a path-unknown prose question gets a scannable `search_knowledge` answer (excerpt early), correct **global** multi-source ranking, truthful authority labels, noun-anchored routing to the tool, and less same-file flooding — without weakening frozen trust contracts.
