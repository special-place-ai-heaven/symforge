# v8 Trust Remediation — Discovery Ledger (VERIFIED)

**Status:** DISCOVERY COMPLETE. Two external reviews + two advisory programs +
a 5-agent code-verification panel, all converged. Ready for `/speckit-specify`
into `specs/010-v8-trust-remediation/`.

**Keystone:** LLM trust in the tools. (Constitution III Trust Envelopes, VIII
Verification Before Done.) Pillars: easy onboarding · proper server architecture ·
intelligent tools · **above all, LLM trust**.

## Convergence

The load-bearing findings are **triple-confirmed**: two independent skeptical
reviews flagged them, and a 5-agent read-only panel re-traced every claim against
live 8.0.0 code (`feat/010` @ 711ee68) — confirming all CRITICAL/HIGH items, with
**several worse than reported** and a few corrected. This is high-confidence
discovery, not report-trust.

## Sources

| ID | Source | State |
|----|--------|-------|
| R1 | `skeptical-senior-review-2026-06-17.md` | IN ✓ |
| R2 | `stel-v8-skeptic-audit-2026-06-17.md` | IN ✓ |
| ADV1 | `v8-trust-remediation-advice-2026-06-17.md` (R1 program: TR phases) | IN ✓ |
| ADV2 | `stel-v8-remediation-advice-2026-06-17.md` (R2 program: T0/T1/T2 tiers, CI gates, relabel-spine) | IN ✓ |
| PANEL | 5-agent verification (status, economics, edit-safety, surface/init, measurement) | IN ✓ |
| REG | `docs/stel-assumptions.md` | reference |

## Two spine rules (from ADV2 — adopt)

1. **Honesty contract for LLM-facing surfaces:** every string an LLM reads
   (`status`, the economics envelope, error/recovery text, tool descriptions,
   README/AGENTS) is **either true or explicitly labeled heuristic/observational/
   deferred**. No field named `saved`/`net`/`active`/`validated` unless the code
   matches the word.
2. **Relabel is a valid fix.** Cheap-honest beats dishonest-confident. Relabeling
   a constant `est_/heuristic` (zero behavior change) is a legitimate, shippable
   remediation — but **relabel ≠ validate** (never mark A-011/A-015/A-016/A-028
   VALIDATED because a label improved).

---

## Verified crosswalk (panel verdicts)

`sev*` = panel-corrected agent-trust severity.

| TR | Verdict | sev* | Confirmed (one-line) | Anchor |
|----|---------|------|----------------------|--------|
| **TR-01** | **CONFIRMED — worse** | P0 | `status_stel_tool` reads the empty front-end `self.index`; every other reader proxies; daemon has **no `status` arm**; `health` proxies but is full-surface-only → **no honest compact index-health tool exists**. Fix: proxy `status` to daemon + add daemon arm + regression test. | tools.rs ~8529/8541, mod.rs:266, daemon.rs ~2435 |
| **TR-02** | **CONFIRMED — worse** | P0 | "Call index_folder" reaches the agent via **37 `loading_guard!`** sites through the `symforge` facade, and **4 distinct compact-reachable strings** name `index_folder`. Fix: one surface-aware `empty_index_recovery_hint(profile)` (never name a gated tool). | format.rs:4774, edit_apply.rs:48, tools.rs:6033, edit_tools.rs:263 |
| **TR-03** | **CONFIRMED — root-cause corrected** | P0 | Root cause is the Desktop wrapper `cd /d "%USERPROFILE%"` → `find_project_root()`=None → no root → no daemon → empty index (NOT the daemon-attach path). The **default** Desktop cold-start trap; init writes `env:{}`. | init.rs:837, main.rs:217-248, init.rs:761 |
| **TR-04** | **CONFIRMED** | P0 | `400/800` stamped on every step (planner ~51-53), `index_refs`/`raw_chars` structurally inert → predicted_net always 275. **A real byte-grounded estimator already exists in `format.rs:4925-5029` and is simply not wired** — grounding is wiring, not building. | planner.rs:44-55, controller.rs:333-348, types.rs:152 |
| **TR-04b** | **CONFIRMED** | P1 | Gate decides on the constant → `degrade`/economics-`bypass` unreachable from the live planner (not dead code — dead from real input). **+ a 3rd dead branch: `mandatory_degrade`** (Fallback confidence) never fires → low-confidence routes get no economic guardrail. | controller.rs:40-135 (55-128, 75-76) |
| **TR-05** | **CONFIRMED** | P0 | `session_net_vs_manual` = `session_context.total_tokens` (monotonic gross, only `+=`), no manual subtracted, printed with `+`. Most misleading line (never negative). | tools.rs:8142, session.rs:69, envelope.rs:47 |
| **TR-06** | **CONFIRMED — worse** | P0(High) | `if_match` is **structurally absent** from the write path (`ReplaceSymbolBodyInput` has no field; planner drops it). Two separate read locks; wide async window with disk re-read. **Real data-integrity bug** (silent clobber + success receipt); repo's 5-agent model hits it normally. Tee = zero protection. | edit_apply.rs:73-91, edit_planner.rs:72, edit.rs:1160, edit_tools.rs:570-676 |
| TR-07 | CONFIRMED — line fix | P1 | README "32 canonical tools" at **L24 + L328** (not L128); AGENTS.md:125; **CLAUDE.md:34 also stale**. Default is 3. | README.md:24,328 · AGENTS.md:125 |
| TR-08 | **CONFIRMED — cosmetic** | P2 | Allow-list has **35** names (deliberately pinned to the full router, init.rs:1877); the 3 compact tools ARE present so the user is **not blocked** — 32 are dead grants/false affordances. Downgrade "blocks users". | init.rs:430-466, 1867-1883 |
| TR-09 | CONFIRMED | P1 | A-011/A-015/A-016 OPEN at ship; surfaces imply validation. Fix: capability matrix. | stel-assumptions.md |
| **TR-10** | **PARTIAL — corrected** | P2 | All **7** `l*/handler_*` literals are unprobed (not just l4); `l4_ledger:active` is literally true for the always-on in-memory layer (reframe: compact omits durable state); `ledger_persistence` stale in `deferred:` (durable ships in serve). **+ `summary()` swallows DB errors as None → disabled≡broken.** | status.rs:15-16,104-122; ledger_store.rs:230 |
| TR-11 | CONFIRMED | P2 | Positive `session_net` on `decision:reject` (sub-case of TR-05). | executor.rs:347-366, envelope.rs:47 |
| **TR-12** | **PARTIAL — doc only** | P2 | Planner IS a 3-magic-string lookup, **but multi-step plans DO execute in production**. Real defect = doc vs status: A-009 "VALIDATED" vs `multi_step_planner` deferred. Fix: demote A-009 → PARTIAL (doc). | planner.rs:104-161, phase2-checkpoint.md:120 |
| TR-13 | CONFIRMED | P2 | `expected_equiv` is write-only dead data; replay grades route-shape only; "accuracy" tests are tautologies (fixture query → build_plan → assert fixture). **+ A-028 falsely VALIDATED** ("not route shape alone" — but it is). Fix: assert or remove `expected_equiv`; demote A-028; purge "95% trajectory". | golden_replay.rs:244-310, types.rs:313, stel-assumptions.md:129 |
| TR-14 | **PARTIAL — Low** | P3 | `status` "preview-and-apply" is the TRUTHFUL line; `mod.rs:14` module doc is **stale** (apply IS wired); status output internally consistent. Doc drift only. (But it advertises a guard TR-06 doesn't keep.) | status.rs:116, mod.rs:14 |
| TR-15 | BY-DESIGN | DOC | Daemon hook IPC ≠ external MCP contract. Operator-guide note. | daemon.rs |
| TR-16 | CONFIRMED | P3 | A-005 OPEN (line 77) vs VALIDATED (line 146); evidence (891B) supports VALIDATED but vs draft shapes. Single-source. | stel-assumptions.md:77,146 |
| TR-17 | CONFIRMED | P3 | `unavailable` (not wired) vs `disabled (open failed)` indistinguishable. | ledger_store.rs |
| TR-18 | DEFERRED | — | P3-A/B/C (migration guard, retention, rmcp pin). Backlog. | 004 review |
| **TR-19** | FIXED-ALREADY | — | Security batch — do not redo; regression-only. | external-remediation |
| **TR-20** | **VERIFIED FIXED** | — | P3-7 empty-union → `EmptyResult` present + correctly placed on the live handler. | tools.rs:8219-8233 |

## New findings (panel-discovered, beyond both reviews)

- **N-1 — `CalibrationState` is fully dead** (types.rs:292-298): `ema_predict_error`/`fudge_multiplier` constructed/read **nowhere**. `calibration: "pending"` will never resolve — it's permanently inert, not transient. Label `deferred`/`observational`.
- **N-2 — `mandatory_degrade` 3rd dead branch** (controller.rs:75-76): low-confidence Fallback routes get **no** economic guardrail (served at full budget).
- **N-3 — `summary()` error-swallow** (ledger_store.rs:230 `.ok()`): a wired-but-failing durable store reports identical to a never-configured one. Distinguish `disabled(reason)` vs `unavailable`.
- **N-4 — bytes/4 conflated with "tokens"** everywhere (handler.rs:8-10): every "tokens" figure (envelope, ledger `actual_response_tokens`, session) is `len/4`, an unstated approximation — even the "honest" actuals.
- **N-5 — 4 dead-end recovery strings** (not 1): TR-02 anchors above; centralize.
- **N-6 — batch-edit `if_match` gap**: batch executors have no `if_match` plumbing; if extended, same TOCTOU. `verify_index_matches_disk` is also pre-flight-only (false safety control).
- **N-7 — A-028 falsely VALIDATED** (stel-assumptions.md:129): contradicted by the validator's own source.

## The good news (leverage + protect — do NOT regress)

- **The real estimator exists** (`format.rs:4925-5029`, `competent_manual_baseline_chars` / `saved_tokens_vs_competent_manual`) — TR-04 grounding is one import away.
- **The compact gate is clean** — pure `(profile,name)->bool`, single chokepoint (`mod.rs:935`), shared stdio+HTTP, conformance test against the real gate. The dead-end is a *message* defect on a *correct* gate.
- **Find-fusion union semantics + P3-7 (TR-20)** correctly shaped + live. Security batch (TR-19), embed isolation, idempotency, path guards, the SQLite ledger engineering, and the **assumptions register itself** are genuinely solid moat. The trust debt is in the **presentation layer**, not the engine.

---

## Consolidated remediation program (ADV1 phases ∪ ADV2 tiers)

**Tiers (ADV2):** T0 trust-critical = TR-04/04b (economics constants+gate), TR-01
(status proxy), TR-06 (if_match TOCTOU), TR-05 (mislabel). T1 honesty cleanup
(one batched, zero-behavior PR) = TR-10/N-1/N-3/M-labels + TR-12/13 doc demotions +
TR-07/08/09 docs+matrix. T2 hardening = TR-17/N-6/TR-18 + ledger retention.

**Phase order (ADV2 sequencing — truth → recovery → labels → safety → measure):**
- **Phase A — Relabel** (1 PR, zero behavior): kills *all* dishonesty today — envelope `est_/heuristic`, `session_tokens_served`, `calibration: deferred`, status enumerated states, drop stale `ledger_persistence`/demote A-009/A-028. Quick-win.
- **Phase B — Status truth**: TR-01 proxy + N-3, regression test.
- **Phase C — Edit safety**: TR-06 `if_match` at write (+ N-6 note), concurrent-write test.
- **Phase D — Recovery + onboarding**: TR-02 surface-aware errors + TR-03 wrapper-CWD/init-env (+ overlaps the parked 009 wizard).
- **Phase E — Grounding + measurement**: TR-04 wire the real estimator (reuse format.rs) → reopens degrade/bypass legitimately; TR-13 assert-or-remove `expected_equiv`.
- **Phase F — Premise + matrix + CI gates**: capability matrix; frame v8 as *bet under test* (A-017/A-011 stay OPEN); cross-cutting CI.

**Hard rule:** Phase A (relabel) ships BEFORE any README "token-efficient"
language (else re-open TR-09). The two **real bugs** (TR-01, TR-06) + Phase A are
the highest-leverage quick-win.

**Cross-cutting CI (ADV2):** a surface-honesty gate; wire the assumptions register
to CI (**OPEN + shipped-claim = FAIL**); one-source-of-truth per number.

## Definition of done (acceptance bar)

1. `status` index counts == the served query's (daemon-proxy regression test).
2. Fresh attach → `index_ready:true` OR a compact error naming only callable recovery.
3. Envelope distinguishes `heuristic` vs `measured`; no `saved`/`net` unless the formula matches.
4. `symforge_edit apply:true`+`if_match` cannot succeed if on-disk body diverged (concurrent-write test).
5. README/AGENTS/CLAUDE.md/init allow-lists describe compact-3 default; 32 = opt-out.
6. Capability matrix published (features → assumption IDs → Implemented/Heuristic/Observational/Deferred).
7. Full gate green + golden replay + new regression tests for TR-01, TR-02, TR-06.

## Do NOT (guardrails)

1. Don't revert compact-3 / re-expose 32 tools to "fix indexing".
2. **Relabel ≠ validate** — don't mark A-011/A-015/A-016/A-028 VALIDATED for a label.
3. Don't gate daemon hook IPC on compact (breaks dogfooding).
4. Don't run a "token savings" doc campaign until Phase A ships.
5. Don't conflate 8.0 architecture-ship with economics-proof-ship.
6. Don't "fix" TR-01 by hiding index fields (one lie for another).
7. Don't delete dead seams (CalibrationState) — relabel honestly; don't fail a request for a ledger nicety.

---

## Next step

Discovery is **done and verified**. `/speckit-specify` 010 from this set →
clarify → plan → tasks → implement → review, gated. Phase A (relabel) +
TR-01/TR-06 (the two real bugs) lead. Surface/STEL-layer changes stay evidence-led
(A-017 OPEN → bet-under-test framing, never a proven-win claim).
