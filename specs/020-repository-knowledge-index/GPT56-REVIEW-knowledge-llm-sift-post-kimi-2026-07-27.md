# GPT-5.6-sol ultra — Second-pass review: Knowledge LLM Sift (post-Kimi)

**Reviewer:** OpenAI GPT-5.6-sol ultra (expensive second pass)<br>
**Date prepared:** 2026-07-27<br>
**Repo:** `E:\project\symforge`<br>
**Why you:** This feature is the other side of SymForge’s code intelligence — LLM doc-sifting
without knowing the path. A first strong model (Kimi K3) already dogfooded live SymForge,
found contract blockers, and produced a revised shippable plan. Your job is to **compound**
on that: find what Kimi still missed, sharpen further, or kill remaining weak ideas — not to
rediscover the original weak draft.

**READ-ONLY.** Do not edit the tree. Propose everything in the report.

---

## How to spend this expensive pass

1. **Read Kimi’s full response first** (primary input):
   `specs/020-repository-knowledge-index/KIMI-REVIEW-RESPONSE-knowledge-llm-sift-2026-07-27.md`
2. **Treat the revised Cursor plan as the candidate to implement** (already folded from Kimi):
   ask the operator for the current `Knowledge LLM Sift` plan body, or reconstruct from Kimi’s
   “Recommended plan” section — they should match.
3. **Optional light dogfood** (if SymForge MCP is connected): re-run 1–2 of Kimi’s probes to
   feel the megeline/bridge tax yourself. Do **not** burn the whole budget re-running Kimi’s
   8-call battery unless something smells wrong.
4. **Diff against frozen contracts** only where Kimi’s fixes might still be wrong:
   - `contracts/search-knowledge.md` (esp. MUST-include bridge classes, truncation test 7/18)
   - `contracts/knowledge-authority-hygiene.md` (voice / Suppressed)
   - `contracts/repository-mental-model.md` (§ask routing)

---

## What is already settled (do not reopen without new evidence)

Kimi verdict **REVISE_DRAFT**; draft bet confirmed: retrieval OK, usability bad.

| ID | Finding | Locked fix direction |
|---|---|---|
| B1 | Dropping missing/ambiguous bridges violates contract | Compact `<id12>:missing`, do not omit |
| H1 | Multi-line + line-cut CCR mid-chops hits | Use existing `apply_ccr_budget_with_summary` |
| H2 | Uncapped excerpt / heading-as-excerpt | Window ~240 chars + heading-skip |
| H3 | `CurrentImplementation` path rules → Suppressed trap | Operations only for new agent/plan rules |
| H4 | WS2 skipped `research/` + `.agent/` | Include in HistoricalRecord / Operations table |
| H5 | Substring `/spec` overmatches | Component tokenize like bridge roles |
| WS3 | Cue widening is contract-backed, not optional | Five doc-noun prefixes + false-positive tests |
| WS4 | Same-file flooding in dogfood | Diversity only after failing corpus fixture |
| Rejected | explore mix, embeddings, blanket docs/, recency demotion, drop Derived/Counts | Stay rejected unless you have stronger evidence |

Your value is **beyond this table**: holes in Kimi’s recommended plan, better algorithms,
missed landmines, sharper success metrics, cheaper implementation order.

---

## Your mandate (all required)

### Job A — Further improve the recommended plan (primary)

Deliver either:

- `APPROVE_KIMI_PLAN_AS_IS` with only test/verification polish, or
- `REVISE_KIMI_PLAN` with a **concrete replacement Recommended plan** (no option menus —
  pick), or
- `REPLACE_WITH_SUPERIOR_SLICE` if you found a smaller/higher-leverage cut Kimi missed.

Invent superior alternatives Kimi did **not** list — then adversarially kill or keep them.
Examples of areas still fertile:

- Excerpt algorithm edge cases (code fences, tables, list markers, multi-match units)
- Bridge compact format parseability for agents vs humans
- Diversity rule math (2-per-path vs score penalty; interaction with phrase rank)
- Whether WS2 HistoricalRecord on `research/` / `docs/dogfood/` wrongly hides useful
  “how we measured X” answers from `default` scope (voice derivation check)
- Implementation order / risk: what to ship first for max LLM-felt win per LOC
- Anything from reading `match_unit`, `derive_voice`, CCR store semantics that Kimi underweighted

### Job B — Adversarial pressure on Kimi’s fixes

Attack B1 compact packing: does `<id12>:missing` still satisfy “stable link IDs” + test 18
survival through CCR? Attack 12-hex collision risk. Attack hit-complete summary builder
pseudocode gaps. Attack Operations-for-CLAUDE.md if DeterministicConflict paths differ.
Attack WS4 diversity for starving multi-section single-file policies (e.g. CLAUDE.md with
many true hits).

### Job C — Code landmines Kimi may have missed

Read at least: `knowledge_search.rs` (`match_unit`, `render_response`, `code_anchor_label`,
CCR call site in `tools.rs`), `derive_voice` / `derive_native_authority_domain`,
`apply_ccr_budget_with_summary`, `path_convention_roles`. Cite `path:line`.

---

## Hard constraints (same as Kimi)

No embeddings this slice; no manual 5k curation; no Gate-M prerequisite; no weakening frozen
contracts for convenience; no explore/knowledge lane blur; no `CurrentImplementation` from
new path rules; CCR must store full safe output (not summary-only).

---

## Required report format

```text
# GPT-5.6-sol ultra — Knowledge LLM Sift (post-Kimi)

## Delta from Kimi
What you agree with; what you overturn; what you add that Kimi missed.

## Verdict
APPROVE_KIMI_PLAN_AS_IS | REVISE_KIMI_PLAN | REPLACE_WITH_SUPERIOR_SLICE
One paragraph.

## Recommended plan (final candidate for implementation)
Ordered workstreams, success checks, non-goals. Constrained. No menus.

## New findings (only net-new vs Kimi)
BLOCKER / HIGH / MEDIUM / LOW with scenario + path:line or contract cite + concrete fix.

## Alternatives considered (including ones Kimi missed)
Keep/reject with reason.

## Test / verification deltas beyond Kimi’s list

## Explicit “do not implement yet” risks
Anything that should wait for a follow-on slice.
```

---

## Operator note

1. Paste this brief + attach/open Kimi’s response file in full.
2. Optionally attach revised plan body + the three frozen contracts.
3. SymForge MCP optional for light re-dogfood.
4. After this report: fold into the Cursor plan, **then** implement — do not code until
   this second pass returns unless verdict is clearly approve-as-is and you accept residual risk.
