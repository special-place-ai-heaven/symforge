# SymForge v8 (STEL) — Remediation Advice

- **Date:** 2026-06-17
- **Status:** ADVICE ONLY. No code in this document is a patch. It is a recommended plan for a downstream coding agent. Every item offers options + tradeoffs; the implementer decides.
- **Pairs with:** `docs/reviews/stel-v8-skeptic-audit-2026-06-17.md` (findings + evidence). IDs below (C1, H1, …) map 1:1 to that audit.
- **Verification note:** All file:line anchors were verified against source at audit time (commit `3df5210`, v8.0.0). Re-confirm before editing — the index drifts.

---

## How to use this document

Each item has: **root cause** (one line), **advice** (usually a cheap "honest relabel" option AND a real "rewire" option, with a recommendation), **anchors**, **effort** (S < 1h, M a few h, L a day+), **risk**, **depends on**, and **done when** (acceptance + the regression test that should exist so the bug can't silently return).

Two over-arching rules drawn from the codebase's own principles — keep them as the spine of the whole effort:

1. **Honesty contract for LLM-facing surfaces.** Every field in the trust envelope and `status` must be one of: (a) a real measured value, (b) explicitly labeled `est.`/`assumed`/`heuristic`/`mode`, or (c) derived from real wiring state. No literal "active"/"saved" that asserts a fact the code never checked. This single rule resolves the bulk of the findings.
2. **Relabel is a valid fix.** For a code-intelligence tool the LLM trusts, an honest "estimated, uncalibrated" is strictly better than a dishonest "275 saved". Where the real rewire is expensive, ship the relabel now and track the rewire as a registered assumption. Do **not** the reverse — do not keep the confident label and defer the honesty.

What NOT to do (anti-patterns this effort must avoid): don't raise a threshold to make an OPEN assumption "pass"; don't delete a deferred seam to make the `deferred:` list shorter; don't mark anything `VALIDATED` without an artifact; don't fail a user request to enforce a ledger/calibration nicety (FR-011 stands).

---

## Tier 0 — Trust-critical (advise: do before promoting v8 as "token-economical" or as a guarded editor)

### C1 — Economics numbers are constants surfaced as measurements
- **Root cause:** `planner.rs:51-53` hardcodes `est_response_tokens: 400`, `est_manual_tokens: 800`, `index_refs: vec![]`; `controller.rs:333-349` just sums them. Single-step net is always `275`.
- **Advice — two layers, ship both in order:**
  - **Layer A (relabel, S, do immediately):** In `envelope.rs:33-66`, change `"{saved} saved vs manual"` to mark it estimated and name the baseline as assumed, e.g. `~{saved} est. (assumed manual {manual})`. Same for the ledger field names in `ledger.rs:84-125` (`actual_response_tokens` → `estimated_response_tokens`, `manual_baseline_tokens` → `assumed_manual_tokens`). This removes the dishonesty in under an hour without touching the predictor.
  - **Layer B (rewire, M-L, the real fix):** Populate `index_refs` with real `raw_chars`/line counts. The executor already resolves the target file/symbol, so the bytes are in hand at plan/serve time — thread them into `StelPlanStep.index_refs` (`types.rs:152-155` is the field, already doc-commented for this) and compute `est_response_tokens`/`est_manual_tokens` from them. Keep a real tokenizer (or a calibrated bytes→tokens ratio per language) instead of `body.len()/4`.
- **Anchors:** `planner.rs:51-53`; `controller.rs:333-349`; `envelope.rs:34,45`; `types.rs:152-155`; `handler.rs:8-10` (`estimate_tokens`).
- **Risk:** Low for Layer A (string-only). Medium for Layer B — changing estimates shifts which gate branch fires (see C2); land C2 first or together.
- **Depends on:** nothing for A; pairs with C2 for B.
- **Done when:** no envelope/ledger field presents an estimate without an `est./assumed` marker (Layer A); and for Layer B, `index_refs` is non-empty for served plans and `predicted_response_tokens` tracks actual within the A-011 target on a battery run. Add a test asserting `build_plan` populates `index_refs` for a known file, and a golden row asserting predicted-vs-actual error stays under threshold.

### C2 — Serve/degrade/bypass gate runs on the fiction; degrade + economics-bypass are unreachable
- **Root cause:** With constant net ≥275 > `SERVE_MARGIN_TOKENS`, `evaluate_plan_with_session` (`controller.rs:40-135`) always returns `serve`; the `<=0` bypass and `<=50` degrade branches (`controller.rs:221-235`) can't fire for real planner output.
- **Advice:** Don't "fix" by tuning constants. After C1-Layer-B grounds the estimate, re-decide what the gate should key on. Recommended: gate degrade/bypass on **cheap real signals** (indexed file byte size, expected result count) rather than the derived net, and treat the economics net as advisory display only until A-011 validates. If the degrade/bypass branches remain unreachable by design, say so — delete or `#[cfg(test)]`-scope the dead branches, or document them as policy-only (P-FF + cache-hit), so the next reader doesn't think a live economics gate exists when it doesn't.
- **Anchors:** `controller.rs:17,19,21` (margins), `:40-135` (gate), `:221-235` (economics bypass body).
- **Effort:** M. **Risk:** Medium — bypass changes what the host reads; never bypass into a truncated read when the user asked for a trace (current `economics_bypass_body` reads lines 1-80 — that is silent capability loss; keep it disabled unless a validated signal justifies it). **Depends on:** C1-B.
- **Done when:** every reachable decision branch is exercised by a test with realistic planner output, OR the unreachable branches are explicitly marked policy/test-only. No live path bypasses a trace/reference query into a line-window read.

### C3 — `status` reports the empty proxy-shell index, not the daemon index that serves
- **Root cause:** Daemon-proxy production topology (`main.rs:267`). `explore` proxies to the warm daemon (`tools.rs:7284`, served at `daemon.rs:2389`); `status_stel_tool` reads the front-end's own empty `self.index` (`tools.rs:8529-8557`) with no proxy, and the daemon has no `status` arm (`daemon.rs:2435` bails).
- **Advice — pick one:**
  - **Preferred (consistent pattern):** add a `status` arm to `daemon.rs::execute_tool_call` (~`:2306`) that runs STEL status against the daemon's loaded `server`, and make `status_stel_tool` call `self.proxy_tool_call("status", …)` first when a daemon client exists — exactly mirroring `explore`.
  - **Cheaper:** source `index_ready`/`index_files`/`index_symbols`/durable-ledger facts from the already-proxied `health`/`health_compact` (the daemon DOES serve those, `daemon.rs:2354/2369`) and populate the status context from that.
- **Anchors:** `tools.rs:8529-8557` (status handler), `:7284` (explore proxy); `daemon.rs:2306,2354,2369,2389,2435`; `protocol/mod.rs:274-281` (proxy defaults), `:587-643` (`ensure_local_index`); `main.rs:267`; `status.rs:51-81` (`from_server`).
- **Effort:** M. **Risk:** Low-Medium (read-path only; watch for added latency on `status` — it's called rarely, so fine). **Depends on:** none.
- **Done when:** in daemon mode, `status` index counts match what `explore` sees. **Add the regression test** that's currently missing: spin a daemon (pattern at `daemon.rs:4348`), index a project, assert `status detail=full` reports `index_ready: true` and `index_files > 0`. This test would fail today — that's the point.

### H1 — `if_match` guarded-apply is never enforced at the write (TOCTOU)
- **Root cause:** `if_match` is checked pre-flight in `run_pre_apply_gates` (`edit_apply.rs:73-79`) under a `read()` lock released at function return; the write runs separately via `replace_symbol_body` (`tools.rs:8458` → `edit_tools.rs:517`), which re-freshens disk, re-resolves, and writes with no `if_match` recheck (token exists only in `types.rs` + `edit_apply.rs`).
- **Advice:** Close the check-to-write gap. Two viable shapes:
  - **Recommended:** have the STEL edit handler perform the splice+write itself against the *same* `file.content` bytes it validated `if_match` against, under one held lock — instead of delegating to a second independent resolve in `replace_symbol_body`.
  - **Alternative:** thread `if_match` (and the resolved byte range) into the write path and re-verify the current symbol body byte-for-byte against `if_match` immediately before `atomic_write_file`, in the same critical section as the splice. On mismatch, abort with a clear conflict error (reuse the idempotency `Conflict` shape).
- **Anchors:** `edit_apply.rs:38-133` (gates; `:73-79` check; `:118-133` index-vs-disk), `tools.rs:8401,8458`, `edit_tools.rs:517-676`.
- **Effort:** M. **Risk:** Medium — touches the mutation hot path; must not regress idempotency or the tee snapshot. Keep the change minimal and well-tested. **Depends on:** none, but coordinate with whoever owns `replace_symbol_body`.
- **Done when:** a test simulates concurrent on-disk change between gate and write and asserts the apply **rejects** rather than clobbering, and that the success response is only emitted when the written bytes match the validated bytes. Until landed, advise that the trust-envelope/docs not claim the apply was `if_match`-guarded.

### H2 — `session_net_vs_manual` is a mislabeled gross counter
- **Root cause:** Fed `session_context.snapshot().total_tokens` (`tools.rs:8142`), which only increases; no manual term subtracted. (`envelope.rs:47`.)
- **Advice:** Cheapest honest fix — rename the surfaced field to `session_tokens_served`. Real fix — subtract an accumulated assumed-manual baseline from the ledger to produce a true net (depends on C1-B + a real per-call manual estimate). Recommend the rename now; the true-net only becomes meaningful once C1-B lands.
- **Anchors:** `envelope.rs:47`, `tools.rs:8142`, `session.rs` (`total_tokens +=`).
- **Effort:** S. **Risk:** Low. **Done when:** the field name matches its semantics; if it claims "net vs manual" it actually subtracts a manual term.

---

## Tier 1 — Honesty cleanup (cheap, high signal, advise as one batched PR)

### M1 — `l1..l4: active` / `handler_*: active` are unconditional literals
- **Advice:** Derive each from real wiring. For `l4_ledger`, print `durable` / `in-memory` / `in-memory (durable unavailable)` based on `ctx.durable_ledger.is_some()` + ledger health, resolving the self-contradiction with the `durable_ledger: unavailable` line. Reserve bare "active" for the wired+working case. Anchors: `status.rs:110-116,144-154`. Effort: S.

### M2 — `calibration:` envelope field hardcoded `"pending"`
- **Advice:** Thread the real `tuning_sufficiency_note`/calibration state into the envelope, or rename to a static `mode:` so it stops implying a live status read. Anchors: `handler.rs:44`, `calibration.rs:30-82`. Effort: S.

### M3 — `error: %` is a dead estimate-vs-estimate readout
- **Advice:** Either label it informational ("est. vs est."), or — once C1-B + a real tokenizer land — make it a true accuracy signal and feed it into calibration. Don't present it as calibration accuracy while it feeds nothing. Anchors: `handler.rs:71-76`. Effort: S (label) / folds into C1-B (real).

### M4 — Ledger fields `actual_/manual_baseline_` carry estimates/constants
- **Advice:** Rename `actual_response_tokens` → `estimated_response_tokens`, `manual_baseline_tokens` → `assumed_manual_tokens` until a real tokenizer/measured baseline exists. The predicted-vs-actual *shape* is already correct — keep it; just stop lying in the names. Anchors: `ledger.rs:84-125`, `types.rs:275-276`. Effort: S. (Do together with C1-A.)

### H3 — Multi-hop "A-009 VALIDATED" overstates
- **Advice:** Doc-only honesty fix — demote A-009 in `docs/phase2-stel-checkpoint.md:120` and `docs/stel-assumptions.md` to PARTIAL/DEMONSTRATED: "3 fixed multi-hop fixtures replay; general query decomposition deferred (= `multi_step_planner`)." This makes the doc agree with the honest `status` `deferred:` line. Optional real work (L, separate program): a general "then"/conjunction decomposer — but that's a feature, not a fix; register it as an assumption first. Anchors: `planner.rs:104-161`. Effort: S (doc).

### H4 — Golden replay never grades equivalence; `expected_equiv` is dead
- **Advice:** Pick one and commit:
  - **Real:** capture golden output bodies and assert `expected_equiv` against them, turning the replay into an actual accuracy gate (this is what A-008's "95% trajectory" needs to ever be true).
  - **Honest-minimal:** strip `expected_equiv` from the fixture and the struct so it stops implying a measurement that never runs; keep the route-shape assertions but rename the test/doc to say "route shape", not "trajectory/equivalence".
  - Either way: purge any "95% trajectory" claim from README/docs until measured. Also note the tautology smell — tests that feed fixture queries back through `build_plan` are change-detectors, not correctness checks; label them as such.
- **Anchors:** `golden_replay.rs:244-310`, `types.rs:313`, `fixtures/routes.golden.jsonl`. Effort: S (strip) / M (real grading).

### L4 — `deferred:` list is a stale frozen literal
- **Advice:** It lists `ledger_persistence` as deferred though the durable store ships in `serve` mode. Compute the list from real feature flags / wiring, or at minimum correct the literal. Anchors: `status.rs:15-16`. Effort: S.

---

## Tier 2 — Hardening (deferred-OK, advise tracking each as a registered item)

- **M5 — durable-ledger silent swallow:** distinguish `durable_ledger: disabled (open failed)` from `unavailable (not wired)` so a silent SQLite open-failure is visible without log access. Anchors: `protocol/mod.rs:338-341`, `ledger_store.rs:184-191,210-216`. Effort: S.
- **M6 — durable-ledger unbounded growth:** add TTL / capped table / archival + an operator maintenance note. Self-documented at `ledger_store.rs:39-41`. Effort: M.
- **L1 — in-memory ledger panics on lock poison:** swap `.expect(...)` for `unwrap_or_else(|e| e.into_inner())` to match the durable store's never-panic posture. Anchors: `ledger.rs:26-46`. Effort: S.
- **L2 — tee "backup" is best-effort, not transactional:** fail closed when a *small*-file snapshot can't be written for `apply:true`; document it as best-effort, not guaranteed undo (relevant to H1's clobber recovery). Anchors: `edit.rs:161-166`, `edit_safety/tee.rs`. Effort: S-M.
- **L3 — keyless-loopback leaves `/api/v1/keys` mint/rotate/revoke open:** optionally always gate the mutating key routes even on keyless loopback, or document the local-user exposure. Anchors: `auth.rs:150-152`, `serve.rs:338`. Effort: S.

---

## Deepest caveat — the premise itself (advise: register, don't ship-as-proven)

The whole compact surface rests on **A-017** ("tool-selection accuracy degrades past ~30-50 tools") — recorded `OPEN`, "cited, not reproduced." Advice: keep A-017 OPEN and frame v8 in README/release notes as a **bet under test**, matching the assumptions register, until an A/B (compact-3 vs full surface on the same tasks with an LLM in the loop) produces an artifact. This is not a code fix; it is a marketing-honesty fix and the most important one for "trustworthy to LLMs." Same applies to A-011 (predictor ±20%) and A-015/A-016 (calibration) — they gate the truth of C1/C2/M3, so a real calibration battery run is the unlock for the whole economics surface.

---

## Suggested sequencing (a campaign the downstream agent can run)

Gate each phase: code → review → test → live-dogfood the MCP (verify-as-user) before the next.

1. **Phase A — Honesty relabel (S, one PR, no behavior change):** C1-A, H2, M1, M2, M4, L4, plus the doc demotions H3 and the H4 "strip or rename" decision. Outcome: every LLM-facing field is honest *today*, even before any rewire. Lowest risk, highest trust-per-hour.
2. **Phase B — Status truth (M):** C3 + its regression test. Outcome: `status` stops reporting a working index as empty.
3. **Phase C — Edit safety (M):** H1 + concurrency test. Outcome: guarded apply is actually guarded. Gate the edit feature's "guaranteed" language on this.
4. **Phase D — Economics grounding (L):** C1-B (populate `index_refs`, real tokenizer) → C2 (re-decide gate signals) → M3 (live error becomes meaningful) → run a real calibration battery to move A-011 toward VALIDATED. Outcome: the economics surface earns its vocabulary.
5. **Phase E — Hardening (as capacity allows):** M5, M6, L1, L2, L3.
6. **Phase F — Premise (separate research):** A-017 / A-008 A/B with artifacts; update README framing.

**Quick-win subset** if time is short: Phase A alone removes essentially all the *dishonesty* findings; H1 and C3 are the two that are *bugs* rather than labels and should not wait long.

---

## Cross-cutting advice

- **Add a "surface honesty" test/CI gate:** a small test that scans the rendered envelope + status for forbidden bare claims ("saved" without "est.", "active" not derived) — so the relabel can't silently regress.
- **Wire the assumptions register to CI** (the doc already proposes this at `docs/stel-assumptions.md:229-235`): every `OPEN` assumption referenced by a shipped surface = FAIL. This structurally prevents the "register says OPEN, README says proven" gap that caused most findings.
- **One source of truth per number:** the recurring root cause is two surfaces reading two states (status vs daemon; envelope-predicted vs ledger-actual; doc-verdict vs register). Prefer deriving display strings from the same value the gate/ledger uses.
- **Treat this doc as advice:** where a recommendation conflicts with a constraint the implementer can see and I can't, follow the constraint and note the deviation. None of these fixes should expand scope into a rewrite; they are relabels, one proxy wire-up, one critical-section tightening, and a calibration program.
