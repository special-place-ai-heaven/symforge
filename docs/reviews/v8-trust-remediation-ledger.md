# v8 Trust Remediation — Discovery Ledger

**Status:** DISCOVERY COMPLETE (both reviews in) — pending code-verification of
load-bearing findings before `/speckit-specify` into `specs/010-v8-trust-remediation/`.

**Keystone:** LLM trust in the tools. Every finding/fix judged by — *does this make
SymForge more trustworthy and more indispensable for real cross-project use, or is
it motion?* (Constitution: III Trust Envelopes, VIII Verification Before Done.)

**North-star pillars (ordered):** easy onboarding · proper server architecture ·
intelligent tools · **above all, LLM trust**.

## Method

- **Two independent external reviews** + a consolidated advisory + live dogfood +
  the assumptions register. **Two-reviewer agreement = high confidence.**
- **Code is gospel.** Every claim re-verified against live code BEFORE any fix.
  Both reviews are source-reading + live dogfood, NO fresh build/test — so the
  load-bearing CRITICALs get a code-trace pass here first.
- **No ad-hoc fixes.** Full speckit lifecycle (discovery → specify → clarify →
  plan → tasks → implement → review), gated each phase.
- **Surface + STEL layers IN SCOPE** but evidence-led; **no hidden capability
  cliffs**; do NOT revert compact-3 to "fix" indexing (see Do-Not list).

## Sources

| ID | Source | State |
|----|--------|-------|
| R1 | `skeptical-senior-review-2026-06-17.md` (compact dead-end, init/CWD, doc drift) | **IN** ✓ |
| R2 | `stel-v8-skeptic-audit-2026-06-17.md` (economics-constants, status-wrong-process, if_match TOCTOU) | **IN** ✓ |
| ADV | `v8-trust-remediation-advice-2026-06-17.md` (consolidated TR-01..TR-20, phases, DoD) | **IN** ✓ |
| REG | `docs/stel-assumptions.md` (A-001..A-032 proof state) | reference |
| FIXED | `external-review-remediation-2026-06-17.md` (security batch + fusion-empty already shipped) | do-not-redo |

## Verification legend

`AGREED` both reviewers (high confidence) · `CLAIMED` one reviewer, not yet
code-traced · `VERIFIED` re-traced here against live code · `BY-DESIGN` ·
`FIXED-ALREADY` · `DEFERRED` · `DOC`.

---

## Canonical finding crosswalk (TR-01..TR-20 from ADV; the register of record)

> The advice doc's TR IDs already dedup both reviews + my R1 seed (L-001..L-014).
> Adopting TR as canonical. **`verify?` = must code-trace before it enters the
> spec** (load-bearing or could be by-design).

| TR | Sev | Title | Confidence | verify? | Disposition |
|----|-----|-------|-----------|---------|-------------|
| **TR-01** | P0 | `status` reads empty proxy index while tools serve from the daemon index (root cause of `index_ready:false` + working `explore`) | **AGREED** (R2 C3 + R1 P0-2 + dogfood) | **YES** — trace `status_stel_tool` (tools.rs ~8529), `proxy_tool_call`, daemon `execute_tool_call` | FIX: proxy status index facts to daemon (or reuse proxied health subset) + regression test |
| **TR-02** | P0 | Compact surface error says "Call `index_folder`" — not on compact-3 → unrecoverable loop | AGREED (R1 P0-1 + dogfood) | YES — `format.rs` msg, `surface.rs` set, gate | FIX: auto-index path + every compact error names only callable recovery |
| **TR-03** | P0 | Cold start: empty index / wrong CWD (`%USERPROFILE%`) / `env:{}` in real MCP sessions | AGREED (R1+R2) | YES — `main.rs`, `init.rs` desktop wrapper, `new_daemon_proxy` | FIX: init writes proven env (root/surface/auto-index); wrapper cd/`--root` |
| **TR-04** | P0 | Token economics = hardcoded 400/800; `index_refs` always empty → `predicted_net` always 275; envelope "saved" while ledger shows loss | **AGREED** (R2 C1/C2 + R1 H1 + dogfood −141 vs +275) | **YES** — `planner.rs:51-53`, `controller.rs:333-349`, `evaluate_plan_with_session` | FIX (honesty-first): relabel `est_/heuristic`; later ground predictor in real bytes (do NOT relabel=validate) |
| **TR-05** | P0 | `session_net_vs_manual` is a gross rising counter mislabeled as net savings | AGREED (R2 H2) | YES — `envelope.rs`, `session.rs` total_tokens | FIX: subtract manual baseline OR rename `session_tokens_served` |
| **TR-06** | P0 | `if_match` checked pre-flight only; write path re-resolves + writes with NO recheck → TOCTOU clobber | **AGREED** (R2 H1) — **real data-integrity bug** | **YES** — `edit_apply.rs:73-79` → `tools.rs:8458` → `edit_tools.rs:517` | FIX: re-verify bytes vs `if_match` in the same critical section as the splice + concurrent-write test |
| TR-07 | P1 | README/AGENTS/wiki say "32 canonical tools" as default | AGREED (R1 P1-1) | low (doc) | FIX: compact-3 default, 32 under `SYMFORGE_SURFACE=full` |
| TR-08 | P1 | `init` allow-lists 32+ legacy tools not on compact wire (false affordances) | AGREED (R1 P1-1/H5) | YES — `init.rs` `SYMFORGE_TOOL_NAMES` | FIX: compact-3 allow-list default; legacy under `--surface full` |
| TR-09 | P1 | Public claims outrun register (A-011/A-015/A-016 OPEN at 8.0) | AGREED | doc | FIX: publish 8.0 capability matrix (Implemented/Heuristic/Observational/Deferred) |
| TR-10 | P1 | `status` literals (`l*_active`, `handler_*`, `deferred:`) overstate/contradict | AGREED (R1 P1-3 + R2 M1/L4) | YES — `status.rs` | FIX: enumerated states (`in_memory\|durable\|unavailable`); compute `deferred:` from flags |
| TR-11 | P2 | Envelope shows positive `session_net` on `decision: reject` | AGREED | YES | FIX: suppress/relabel net on non-serve |
| TR-12 | P2 | A-009 "VALIDATED" = 3 magic-string multi-hop fixtures | AGREED (R2 H3) | YES — `planner.rs:104-161` | FIX (doc): demote A-009 → PARTIAL |
| TR-13 | P2 | Golden replay checks route shape, not `expected_equiv` (dead field) | AGREED (R2 H4) | YES — `golden_replay.rs:244-310` | FIX: assert `expected_equiv` vs golden bodies OR remove field; purge "95%" claims |
| TR-14 | P2 | `symforge_edit` apply contract vs module "deferred" doc | AGREED (R1 H3) | YES | FIX: one documented flow + reconcile status/module doc |
| TR-15 | P2 | Daemon IPC vs external MCP contract undocumented | BY-DESIGN (P2-4) | DOC | DOC: operator guide — external = gated `/mcp`+stdio; hooks ≠ harness surface |
| TR-16 | P3 | Assumptions register duplicate tables (A-005 OPEN vs VALIDATED) | CLAIMED (R1 M1) | low | FIX: single source per A-ID |
| TR-17 | P3 | Durable ledger `unavailable` vs `disabled (open failed)` indistinguishable | CLAIMED (R2 M5) | YES — `ledger_store.rs` | FIX: distinguish states |
| TR-18 | P3 | Ledger retention / migration guard / rmcp pin (004 P3-A/B/C) | DEFERRED | — | track in backlog; not 8.0.1-blocking |
| TR-19 | — | Security batch (Origin/compact gate/key redaction/ledger drain) | **FIXED-ALREADY** | regression-only | do not redo (010 remediation) |
| TR-20 | — | Find-fusion empty union → `EmptyResult` | **FIXED-ALREADY** | verify live | confirm on deployed binary (TR-13 covers equiv) |

**6 P0-class trust items (TR-01..TR-06).** TR-01, TR-04, TR-06 are the most
load-bearing: status that lies about a working index, economics that contradicts
itself by 416 tokens, and a safety guarantee (`if_match`) that isn't kept.

---

## Phased plan (from ADV §5 — becomes the spec breakdown)

```
Phase 1 — Truth & recovery   : WS-A status/index proxy (TR-01) · WS-B compact errors (TR-02) · WS-C init/CWD (TR-03/08)
Phase 2 — Honest surfaces    : WS-D status vocab (TR-10/11) · WS-E docs+capability matrix (TR-07/09) · WS-F envelope/ledger relabel (TR-04/05)
Phase 3 — Safety & measure   : WS-G if_match at write (TR-06) · WS-H predictor grounding OR explicit heuristic (TR-04 long) · WS-I golden equiv + register cleanup (TR-12/13/16)
Phase 4 — Operator/deferred  : WS-J operator docs (TR-15/17) + deferred P3 (TR-18)
```

**Hard rule:** do NOT ship Phase 2 README "token-efficient" language before WS-F
relabel lands (else you re-open TR-09). **Truth → recovery → labels → safety →
measurement.** Likely **8.0.1** after Phase 1+2; **8.0.2** after Phase 3.

## Definition of done (ADV §4 — acceptance bar)

1. `status` index counts == what the served query used (daemon-proxy regression test).
2. Fresh attach reaches `index_ready:true` OR a compact error naming only callable recovery.
3. Envelope fields distinguish `heuristic` vs `measured`; no `saved`/`net` unless the formula matches.
4. `symforge_edit apply:true` + `if_match` cannot succeed if on-disk body diverged (concurrent-write test).
5. README/AGENTS/init allow-lists describe compact-3 default; 32-tool = opt-out.
6. Capability matrix published (features → assumption IDs → Implemented/Heuristic/Observational/Deferred).
7. Full gate green + golden replay + new regression tests for TR-01, TR-02, TR-06.

## Do NOT (ADV §10 — guardrails)

1. Do NOT revert compact-3 / re-expose 32 tools on the wire to "fix indexing".
2. Do NOT mark A-011/A-015/A-016 VALIDATED because labels improved — **relabel ≠ validate**.
3. Do NOT gate daemon hook IPC on compact surface (breaks dogfooding).
4. Do NOT run a "token savings" doc campaign until WS-F relabel ships.
5. Do NOT conflate 8.0 architecture-ship with economics-proof-ship.
6. Do NOT "fix" TR-01 by hiding index fields from compact `status` (one lie for another).

## Protect (must NOT regress)

Compact dispatch enforcement (gate-before-router); the security remediation batch
(TR-19); embed isolation + semver coherence; golden-replay/phase-2 gate machinery;
the byte-exact snapshot / quarantine / idempotency index-integrity moat; and the
assumptions register itself (keep it the source of truth — it's the most honest
doc in the repo and the reason these reviews could even be written).

## A-017 — the premise under the whole rework

"Tool accuracy degrades past ~30–50 tools" justifies 32→3 and is **OPEN /
cited-not-reproduced**. Remediation must either reproduce/measure it on our corpus
OR frame v8 as a *bet under test* across all surfaces (as the register already
does). Not a blocker for the honesty fixes; it IS the frame for the capability matrix.

---

## Next step

1. **Verify the load-bearing CRITICALs against live code** (code is gospel, `verify?=YES`):
   TR-01 (status wrong-process), TR-04 (economics constants), TR-06 (if_match TOCTOU)
   first — confirm file:line claims, rule out by-design, before they enter the spec.
2. `/speckit-specify` 010 from the verified set → clarify → plan → tasks →
   implement → review, gated. Phase 1 (truth/recovery) + the honesty sweep lead.
3. Update `docs/stel-assumptions.md` where assumptions move; never relabel=validate.
