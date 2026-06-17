# v8 Trust Remediation — Discovery Ledger (seed)

**Status:** SEED — discovery phase, accumulating. Feeds `/speckit-specify` for the
v8 trust-remediation feature (working dir: `specs/010-v8-trust-remediation/`).

**Keystone:** LLM trust in the tools. Every finding and fix is judged by one
question — *does this make SymForge more trustworthy and more indispensable for
real cross-project use, or is it motion?* (See the constitution: III Trust
Envelopes, VIII Verification Before Done.)

**North-star pillars (ordered):** easy onboarding · proper server architecture ·
intelligent tools · **above all, LLM trust**.

## Method

- **Two independent external reviews** + live dogfood + the project's own
  assumptions register. Two-reviewer agreement on a finding = high confidence.
- **Code is gospel.** Every reviewer claim is re-verified against live code
  BEFORE any fix. Reviewers' traces are inputs, not verdicts — several came from
  source-reading without a fresh build.
- **No ad-hoc fixes.** The remediation runs the full speckit lifecycle
  (discovery → specify → clarify → plan → tasks → implement → review), gated.
- **Tool surface + STEL layers are IN SCOPE** (not frozen): the compact-3 rework
  rests on A-017, which the register marks OPEN / cited-not-reproduced. Surface
  changes are the highest blast radius — evidence-led only, no hidden capability
  cliffs.

## Sources

| ID | Source | State |
|----|--------|-------|
| R1 | `docs/reviews/skeptical-senior-review-2026-06-17.md` (Cursor skeptical-senior pass) | **IN** ✓ |
| R2 | second external reviewer | **PENDING** (slot reserved below) |
| DF | operator live dogfood (parallel run) | partial signals folded as corroboration |
| REG | `docs/stel-assumptions.md` assumptions register | reference |

> A separate WIP audit file exists in `docs/reviews/` but is **in-the-making and
> deliberately excluded** from this ledger until finalized — do not build on it.

## Status legend

`CLAIMED` reviewer-asserted, not yet re-verified here · `VERIFIED` re-traced
against live code · `CORROBORATED` independently seen in dogfood (still pending
code-trace) · `BY-DESIGN` · `WONTFIX` · `FIXED-ALREADY` · `DEFERRED` ·
`DUPLICATE`.

---

## Findings (from R1 — pending my code-verification + R2 overlap)

| L-ID | R1 | Sev | Title | Status | Trust impact | Proposed disposition | Anchors (R1-claimed) |
|------|----|-----|-------|--------|--------------|----------------------|----------------------|
| L-001 | P0-1 | P0 | Compact-surface recovery dead-end: error says "Call `index_folder`" but it isn't on compact-3 → unrecoverable agent loop | CORROBORATED (DF saw `index_ready:false`) | **Severe** — tool instructs an action it forbids; agent stuck | FIX. Make indexing reachable on compact surface and/or rewrite every compact-path error to name a *callable* recovery (never `index_folder`) | `format.rs`, `surface.rs`, `surface_probe.rs`, `mod.rs` |
| L-002 | P0-2 | P0 | Cold start with empty index in real MCP sessions (daemon-proxy root not attached; Desktop wrapper CWD=`%USERPROFILE%`; init `env:{}`) | CORROBORATED | **Severe** — agent believes SymForge is connected; all queries fail until invisible operator fix | FIX. Init sets proven env (workspace root, surface, auto-index); desktop wrapper cd/`--root`; compact `status` surfaces actionable empty-index reason | `main.rs`, `init.rs` (`create_desktop_wrapper_windows`), `new_daemon_proxy` |
| L-003 | P1-1 | P1 | Public docs describe v7 32-tool surface as canonical (README, AGENTS.md, init allow-lists, CHANGELOG "README rewrite" not actually done) | CLAIMED | **High** — trains agents + allow-lists on a non-default surface; false affordances | FIX. Rewrite README + AGENTS.md for compact-3 default + `SYMFORGE_SURFACE=full` opt-out; reconcile init allow-lists with on-wire tools | `README.md`, `AGENTS.md`, `init.rs` (`SYMFORGE_TOOL_NAMES`) |
| L-004 | P1-2 | P1 | Phase gates vs ship date: A-015/A-016/A-011/A-008 OPEN/PARTIAL at 8.0.0, but surfaces read as validated product claims | CLAIMED | **High** — "trust envelope"/"token-efficient" implied proven | FIX (honesty). Publish an explicit **8.0.0 capability matrix**: Implemented / Measured / Observational / Deferred, tied to assumption IDs + `status` `deferred:` | `docs/stel-assumptions.md` |
| L-005 | P1-3 | P1 | `status` labels overstate deferred subsystems (`l4_ledger: active` while `durable_ledger: unavailable`; `ledger_persistence` deferred) | CORROBORATED | **High** — "active" reads production-ready; agent can't tell in-memory vs durable | FIX. Vocabulary `in_memory_only \| durable \| disabled \| unavailable`; align module doc ↔ status strings | `status.rs`, `mod.rs` (module doc) |
| L-006 | H1 | High | Token-economics envelope uncalibrated (predict error ~85% live; A-011 OPEN); `session_net_vs_manual:+N` printed alongside `decision: reject` reads as success | CORROBORATED (DF 104% error) | **High** — LLM trained to read "+saved" as success is misled on a reject | FIX. On `decision != serve`, prefix `economics_status: uncalibrated_*` or suppress net-saved line on reject/degrade/bypass-fail | economics envelope formatter |
| L-007 | H2 | High | Routing confidence `inferred` dominates; planner fallbacks burn schema+invoke tokens before failing on empty index | CLAIMED | Med-High — bad economics on errors; A-008 PARTIAL | VERIFY then decide (cheap-fail on low-confidence empty-index path) | planner / routing |
| L-008 | H3 | High | `symforge_edit` apply path: `status` advertises `preview-and-apply`; module doc says apply deferred; apply gates on index with the P0-1 `index_folder` message | CLAIMED | High — agents lack a single documented edit flow | FIX. One documented flow (preview → `apply:true`+`if_match`), readiness preconditions in the tool description; reconcile status vs module doc | `edit_apply.rs`, module doc, `status.rs` |
| L-009 | H4 | High→P3 | Daemon IPC bypasses compact gate (already documented/by-design for hooks/dogfooding) | BY-DESIGN (per `external-review-remediation-2026-06-17.md` P2-4) | Low (internal) — confusing externally | DOC only: operator guide — external contract = gated `/mcp` + stdio `call_tool`; hooks ≠ harness surface | `daemon.rs::execute_tool_call` |
| L-010 | H5 | High | Init/client config not updated for v8 cutover: no `SYMFORGE_SURFACE` escape doc; uneven cold-start (Linux Codex `SYMFORGE_NO_DAEMON=1`); legacy allow-list names = false affordances | CLAIMED | High — overlaps L-002/L-003 | FIX with L-002/L-003 (init env + allow-list reconciliation) | `init.rs` |
| L-011 | M1 | Med | Assumptions register internal inconsistency (A-005 OPEN in top table, VALIDATED in Phase-0 table) | CLAIMED | Med — external readers misled | FIX. Single source of truth per assumption ID | `docs/stel-assumptions.md` |
| L-012 | M2 | Med | README "every truncation disclosed with real cost" vs maturity — audit all formatters for silent truncation under `max_tokens` on the `symforge` path | CLAIMED | Med — trust-envelope claim | VERIFY (audit formatters); fix or hedge the claim | `controller.rs`, formatters |
| L-013 | M3 | Med | Feature 007 find-fusion both-empty→`Found` was OPEN; external remediation claims FIXED (`EmptyResult`) — confirm live post-index | CLAIMED | Low — likely already FIXED-ALREADY (P3-7 in 010) | VERIFY live on deployed binary | `tools.rs` (`symforge_stel_handler`) |
| L-014 | M4 | Med | `edition = "2024"` in `Cargo.toml` — note for downstream packagers (not a bug if toolchain pinned) | CLAIMED | Low | NOTE only | `Cargo.toml` |

### Already-tracked / can-defer (from R1's "can defer", reconcile — don't re-find)

- P3-A ledger migration forward guard, P3-B retention — tracked in 004 review; DEFERRED.
- P3-C rmcp version pin drift — tracked; DEFERRED.
- A-020..A-022 transport/unification (Phase 4) — out of scope this slice.

---

## Cross-cutting workstream: the claims-honesty sweep

R1's "Overstated surfaces" checklist is the honesty hit-list — these are the
keystone (trust) items. Each shipped surface must say only what's proven:

| Claim (surface) | R1: actual state | Action |
|-----------------|------------------|--------|
| "32 canonical MCP tools" (README/AGENTS) | default = **3** | rewrite for compact-3 default (L-003) |
| "token-efficient" / trust envelope (README hero) | predictor OPEN; live error ~85%; calibration insufficient | hedge to observational + capability matrix (L-004, L-006) |
| `l4_ledger: active` | session yes; durable deferred/unavailable on stdio | status vocabulary (L-005) |
| `handler_symforge_edit: preview-and-apply` | apply gated; module doc says deferred | reconcile + document flow (L-008) |
| Phase 3 gate "A-015/A-016 validated" | OPEN at ship | capability matrix (L-004) |
| "Call index_folder" recovery | blocked on compact surface | rewrite error (L-001) |
| init allow-lists (32 legacy names) | ≠ MCP `list_tools` (3) | reconcile (L-003/L-010) |

**Foundational premise to settle:** A-017 ("tool accuracy degrades past ~30–50
tools") justifies the entire 32→3 rework and is **OPEN / cited-not-reproduced**.
The remediation must either (a) reproduce/measure it on our corpus, or (b) frame
v8 as a *bet under test* (not a proven win) across all surfaces — matching how
the register already frames it.

## Protect (genuinely-good — must not regress)

R1 credits, and these are load-bearing moat — the refactor must NOT break them:
compact dispatch enforcement (P1-A, gate before router, not list-only); the
security remediation batch (Origin gate, admin asset auth split, AAP key
redaction, ledger drain); embed isolation + coherent semver story; golden-replay
+ phase-2 gate machinery; the byte-exact snapshot / quarantine / idempotency
index-integrity model (7.x moat STEL sits on top of); and the assumptions
register itself (the most trustworthy doc in the repo — keep it the source of
truth).

## Proposed remediation tranches (from R1's queue — becomes the spec's breakdown)

1. **P0 trust-critical (block "agent-ready default"):** L-001 + L-002 — indexing
   reachable on the compact surface without hidden env; fix init/CWD/daemon root.
2. **Honesty of shipped surfaces:** L-003 (README/AGENTS/allow-lists), L-004
   (capability matrix), L-005 (status vocabulary), L-006 (envelope on reject).
3. **Edit-flow + routing clarity:** L-008, L-007.
4. **Doc + register hygiene:** L-009 (operator-guide note), L-011, L-012.
5. **Verify-not-fix:** L-013 (find-fusion live), L-014 (note).

---

## Reserved: Reviewer 2 (PENDING)

> When R2 lands: enter its findings here, **dedup against L-001..L-014** (same
> file/symptom → merge, take highest severity, mark two-reviewer agreement =
> high confidence), then re-verify each surviving finding against live code
> before it enters the remediation spec.

## Next step

1. Await R2; fold + dedup into this ledger.
2. Re-verify every surviving finding against live code (code is gospel).
3. `/speckit-specify` the remediation from this verified set → clarify → plan →
   tasks → implement → review, gated. P0 trust items + the honesty sweep lead.
