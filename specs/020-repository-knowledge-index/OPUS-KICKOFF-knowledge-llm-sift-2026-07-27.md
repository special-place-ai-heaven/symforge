# Opus kickoff — Knowledge LLM Sift (full SpecKit loop)

Copy everything below the line into Opus 5.

---

You are implementing **Knowledge LLM Sift** on SymForge at `E:\project\symforge`.

## Mission

Make SymForge’s **repository knowledge** lane indispensable for LLM doc-sifting (policy/design/process answers **without knowing the path**) — the other side of the coin to code intelligence. Retrieval already works; this slice fixes multi-source composition, answer-first formatting, truthful authority, routing, and light diversity.

**Exit criteria:** the final Cursor plan’s WS0→WS4 + verification section, not “complete all of SpecKit 020 Gates A–M.”

## Authority (read in this order — use SymForge, not broad file dumps)

1. `IMPLEMENTER-HANDOFF-knowledge-llm-sift-2026-07-27.md` under `specs/020-repository-knowledge-index/`
2. Final plan: `C:\Users\rakovnik\.cursor\plans\knowledge_llm_sift_56bece4f.plan.md`
3. GPT response: `specs/020-repository-knowledge-index/GPT56-REVIEW-RESPONSE-knowledge-llm-sift-2026-07-27.md`
4. Kimi response: `specs/020-repository-knowledge-index/KIMI-REVIEW-RESPONSE-knowledge-llm-sift-2026-07-27.md`
5. Frozen contracts (do **not** weaken):
   - `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
   - `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
   - `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`
6. `AGENTS.md` + `CLAUDE.md` for verification / Windows cargo discipline

## Process — full SpecKit loop (mandatory)

Run the complete SpecKit workflow for **this slice only** (scoped under Feature 020, new task IDs `SIFT-WS0`…`SIFT-WS4`):

1. `/speckit-specify` — freeze a short slice spec from the Cursor plan (do not re-litigate Feature 020 product boundaries).
2. `/speckit-plan` — implementation plan aligned to WS0→WS4 order.
3. `/speckit-tasks` — RED/GREEN/VERIFY tasks, ordered; one workstream at a time.
4. `/speckit-implement` — execute tasks in order: **WS0 → WS1 → WS2 → WS3 → WS4 → verify**.
5. Use SpecKit checklists / analyze only when they reduce risk; do not spawn parallel speculative docs.

Branch: `feat/knowledge-llm-sift` from current `main` (or ask if unclear). No unrelated commits. No push unless asked.

## Tooling — SymForge first (save tokens)

When SymForge MCP is available, **prefer it over raw reads/greps** for code and docs:

- Orient: `get_repo_map`, `health_compact`
- Find: `search_symbols`, `search_text`, **`search_knowledge`** for prose/contracts/plans
- Read: `get_file_context` (outline) → `get_symbol` / `get_file_content` only for needed spans
- Impact: `analyze_file_impact` / `what_changed` after edits

**Token discipline (non-negotiable):**

- Do not dump whole large files (`tools.rs`, `format.rs`, `daemon.rs`, big knowledge modules) — symbol/outline first.
- Prefer `estimate=true` or tight `max_tokens` when probing.
- One workstream at a time; don’t preload WS2–WS4 context while finishing WS0.
- After heavy `cargo test` / `build --release`, run `cargo clean` (CLAUDE.md Windows disk rule).
- Keep SpecKit artifacts short; point at the Cursor plan instead of duplicating GPT/Kimi reports.

## Hard constraints (from dual review)

- **WS0 first:** global compose → rank → single limit (no per-source truncate+concat).
- Compact bridges; **never drop** missing/ambiguous classes.
- Type-aware IDs; digests extend-until-unique; rule names verbatim.
- Block-safe CCR summary + `apply_ccr_budget_with_summary`; CCR stores **full** safe output.
- Authority: unevidenced lifecycle → Unknown; heading before path; no new CurrentImplementation path rules.
- Routing: noun-anchored prose cues only; do not steal generic `how does X work` Understand lane.
- Diversity: failing fixture first; two-pass soft quota (not hard 2-cap).
- No embeddings, Gate-M campaign, explore mix, recency demotion, ranking knobs, or contract weakenings.

## Verify before claiming done

Contract tests 3, 7, 8, 9, 11, 16, 18; plan’s extra tests; `cargo fmt --check`; `cargo check`; focused knowledge/`search_knowledge` tests; dogfood release-please + persistence + `max_tokens=120/300` with before/after bytes and answer line position.

Start now: run SpecKit specify for this slice, then proceed through the loop without waiting for extra permission unless a frozen contract appears to conflict with the plan (then stop and report).
