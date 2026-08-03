# Codex follow-up consultation — Feature 021 — 2026-07-28

**Your own prior report:** `specs/021-admission-coverage-honesty/PLAN-REVIEW-FINDINGS-codex-20260728-f3a91c.md`
(17 findings, 5 blockers, `VERDICT: FIX-FIRST`). Read it first — this brief does not restate it.

**Repo:** `E:\project\symforge`, branch `feat/knowledge-llm-sift`. Read-only: do not edit the plan artifacts, the review reports, or any source. Output goes to your own new file (§6).

**Why you specifically:** on several findings you established a hard constraint and then correctly stopped short of prescribing the resolution, because prescribing was outside a review's remit. That remit is now lifted. These are the questions where your constraint is the binding one and nobody else has grounds to answer.

---

## 1. Two owner rulings have been made since your review

These change the shape of some of your findings. Treat them as settled.

1. **T105 → refuse, and edit `tools.rs`.** Tier-2 `around_symbol`/`around_match` requests get a structured refusal, and the caller-owned annotation at `tools.rs:8532-8538` is changed to distinguish *requested* from *honored*. PR #479 merged 2026-07-27T14:53:35Z, so `tools.rs` is unlocked. This directly answers your `[HIGH] ACH-02's recommended refusal still emits an annotation claiming the refused mode`, and it removes the "no `tools.rs` edit" precommitment you showed was impossible.

2. **WS5 → Feature 021 implements `T062`–`T082` on this branch.** Phase 4 becomes an implementation phase that points at `specs/020-repository-knowledge-index/sift/tasks.md` for task detail rather than restating it, adds the missing gates for WS5B/WS5C/WS5D/WS5E, and makes close-out **rerun** the inherited named tests instead of accepting receipts by citation. This is the accepted resolution of your `[BLOCKER] Five inherited defects depend on 21 unchecked tasks that 021 never schedules`, and it is the intended fix for your `[MEDIUM] 021 duplicates two inherited WS5 implementation seams` — one owner per seam, because there is now one owner.

**T104 is deliberately still open.** See Q2.

---

## 2. Independently verified since your review — settled, not open

Three of your load-bearing claims were re-checked directly against source/execution and **all three held**:

| your claim | how it was checked | result |
|---|---|---|
| `handle_new_file_impact` returns 404 for a nonexistent path | read `src/sidecar/handlers.rs:941-975` — `ReindexResult::NotFound \| Removed => return Err(StatusCode::NOT_FOUND)` | **confirmed** |
| `{canary}` still matches `[^\s"'#]{8,}` | executed: length 8, zero characters in the excluded class, full match | **confirmed** |
| `is_placeholder` has no single-brace branch | read `src/knowledge/mod.rs:130-157` — `${…}` and `{{…}}` only | **confirmed** |

Also independently verified: all 21 WS5 tasks unchecked with only three gates in 021; ACH-01 and WS5C both edit `src/protocol/edit_plan.rs`; zero Phase-5 tasks mention `SensitivePath`/`SensitiveContent`; `T137`'s line set excludes `watcher/mod.rs:415`, `:453`, `:511`; `human_size` is at `src/protocol/format.rs:1591`, not `:3668`.

Do not re-derive these. Build on them.

---

## 3. The questions

### Q1 — What detector rule actually satisfies both directions? *(highest value; everything gates on T066)*

You proved the specified correction is unachievable **and** that the obvious alternative is dangerous: the value-class constraint does not exclude `{canary}` (exactly 8 allowed characters), and a left word boundary placed before the keyword stops matching `access_token`, `refresh_token`, `db_password`.

Current rule, `src/knowledge/mod.rs:89-95`:
```
(?i)(?:api[_-]?key|secret|token|password|passwd|pwd|client[_-]?secret)[ \t]*[:=][ \t]*["']?([^\s"'#]{8,})
```

Specify a rule (or a rule plus `is_placeholder` change, or a rule plus a second-stage filter — your call on shape) that:

- **rejects** the measured false positives: `let token = token.to_lowercase();` · `let original_stop_token = Arc::clone(&watcher.stop_token);` · `token: Symbol(sessionId),` · `password = page.locator(passwordSel).first();` · the repo's own `token={canary}` / `password={canary}` canaries;
- **accepts** realistically-shaped genuine assignments across identifier conventions — `access_token`, `refresh_token`, `db_password`, `clientSecret`, `api-key`, `AWS_SECRET_ACCESS_KEY` — with values in the shapes real credentials take (base64ish, hex, JWT-like, quoted and unquoted);
- **is implementable** with the crate already in use (`regex = "1.11"`, no look-behind).

Then give the **bidirectional oracle set** that pins it: the specific positive and negative cases that must be asserted so a future change cannot silently drift in either direction. Name which are RED-before-fix.

If you conclude a single regex cannot do this, say so and specify the two-stage design instead. A wrong answer here is worse than "regex is the wrong tool" — say that if it is true.

### Q2 — Does T104 still belong in Feature 021, and what is the correct argument?

T104 asks: when a file trips the detector, keep dropping the **whole file**, or index it and withhold only the matched range?

Two reasons this is being re-asked rather than ruled:

1. **The T105 ruling may have dissolved its relevance to 021.** Your finding was that T104's ruling feeds ACH-02's fallback scope. With ACH-02 now *refusing* rather than lexically reading, that scope is empty — nothing is read, so full-file-versus-per-range no longer governs any 021 behavior. If that is right, T104 is a Feature-020 security-contract decision that 021 should escalate rather than own. Confirm or refute.
2. **The argument previously offered for keeping full-file demotion was statistically invalid.** "Zero genuine secrets among 29 findings" was cited as support. That sample is selection-biased: the over-broad rule *selected for* false positives, so a clean sample says nothing about the population that remains once Q1's rule lands — which is exactly when matches become likely to be real. Give the argument that actually holds, in whichever direction it points.

### Q3 — Revision order, and which of your 17 findings change shape

Across three reviewers there are **eight distinct blocker-class items**. Given the two rulings in §1:

- Which of your 17 findings are now **moot**, which **change shape**, and which **newly conflict** with each other?
- What is the correct **order** to fix them? Name what genuinely unblocks what, as opposed to what is merely conservative sequencing — you already showed the plan got this wrong for ACH-02 and ACH-03.
- Is there a fix that, applied first, collapses several others? (The working hypothesis is that every blocker sits at a *seam* — 021↔WS5, phase↔phase, `format.rs`↔`tools.rs`, gate↔what-it-gates, fix↔its-own-acceptance-test — while the interiors are sound. Confirm, refute, or sharpen.)

### Q4 — Amend 021 in place, or re-draft it?

You wrote that the RED→GREEN→VERIFY shape is salvageable but implementation should not start from this revision. With WS5 absorbed, the phase map changes substantially. Which is cheaper *and* safer: a targeted amendment, or a re-draft from the corrected constraints? Name the specific risk of the option you reject.

### Q5 — Adjudicate the two findings that were unique to the other reviewers

You reviewed blind; two other reviewers have now finished and their reports are locked. Independence has served its purpose, so reading them is now safe rather than contaminating — **this is a deliberate role switch from independent reviewer to adjudicator.**

- `PLAN-REVIEW-FINDINGS-cursor.md` (Cursor / Grok 4.5)
- `PLAN-REVIEW-REPORT-2026-07-28.md` (Kimi K3)

Judge two findings specifically, because each was unique to one reviewer:

1. **Cursor rated the ACH-01 ↔ WS5C `edit_plan.rs` co-ownership a BLOCKER; you rated the same class MEDIUM.** With WS5 now absorbed into 021, who is right about severity, and does absorption fully resolve it?
2. **Kimi reported a BLOCKER you did not: `T137`/FR-009 converts only some race-loss `Skipped` sites, naming `watcher/mod.rs:415` (generated-output), `:453` (hard-skip), `:511` (content-policy) as lost races that would still route to `impact_skipped_text` and reproduce SF-DOG-009.** The line-set gap is verified. **Are those three arms genuinely lost races, or legitimate non-race skips?** That classification was never checked and is the open question.

Where you disagree with either reviewer, say so plainly and cite source.

---

## 4. Non-negotiables

1. **Frozen Feature 020 security invariant:** a lexical/raw read must never touch a file excluded for `SensitivePath` or `SensitiveContent`. Q1's rule correction removes *false positives* — it must not become a route to a false **negative**.
2. Repo gates: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, `cargo check --no-default-features --features embed`, `cd npm && npm test`.
3. `src/protocol/tools.rs` is now editable (#479 merged). `src/live_index/knowledge_authority.rs` and `src/protocol/knowledge_model.rs` carry unpushed WS2 work on this branch — do not plan changes that fight it without saying so.

---

## 5. Note on reading the source

Many of the files you need are themselves metadata-only in the live index — that **is** the defect. Use the raw-content fallback and flag it, as you did before, rather than silently substituting.

---

## 6. Output

Write to a new file: `specs/021-admission-coverage-honesty/CODEX-FOLLOWUP-ANSWERS-<date>-<short-id>.md`.
Do not write into this brief, the plan artifacts, or any reviewer's report.

**Append as you go, per question.** If you stop early, whatever is on disk should still be usable — a complete answer to Q1 alone is worth more than five partial ones.

For each: the answer, the evidence (`file:line`, or executed output), and what you could not establish. Where a question has no defensible answer from the source available, say that instead of constructing one — the same standard your review already held itself to, including the four claims you rejected from your own lenses.
