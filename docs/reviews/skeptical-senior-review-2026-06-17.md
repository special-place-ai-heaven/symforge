# SymForge v8 — Skeptical Senior Engineer Review

**Reviewer:** Cursor agent (independent pass; parallel review running elsewhere)  
**Target:** `feat/009-operator-setup-wizard` / shipped **8.0.0** binary via `user-symforge` MCP  
**Date:** 2026-06-17  
**Method:** Dogfood deployed MCP (`status`, `symforge`), cross-read assumptions register + gap-closure docs, trace compact-surface dispatch, compare public docs vs `src/stel/` + `src/protocol/`, reconcile with prior review artifacts in `docs/reviews/`.

---

## Executive verdict

The **internal engineering discipline is stronger than the external product story**. The assumptions register, deferred-work list, and calibration “observational only” labeling are unusually honest for a v1 economics layer. That honesty mostly **does not propagate** to README, AGENTS.md, agent init allow-lists, or runtime error text — where an LLM or user still sees “32 tools,” “trust envelope,” and “call `index_folder`” as if v7 semantics apply.

**Shipped 8.0.0 is architecturally real** (STEL L1–L4 code exists, compact-3 dispatch gate is enforced, operator server hardening landed). **Shipped 8.0.0 is not yet trustworthy as a self-describing agent product** until: (1) cold-start indexing works reliably on the default compact surface, (2) error/recovery paths name actions agents can actually take, and (3) public docs match the default surface.

**Overall:** Proceed with the rework, but treat **LLM-facing honesty and onboarding** as blocking for “production agent default,” not as polish.

---

## Live dogfood evidence (deployed MCP, 8.0.0)

### `status` (compact)

```
symforge_version: 8.0.0
index_ready: false
index_files: 0
index_symbols: 0
deferred: b_results,calibration_auto_tune,ledger_persistence,multi_step_planner
l1_planner: active … l4_ledger: active
handler_symforge_edit: preview-and-apply
```

### `status` (full)

```
project: project
durable_ledger: unavailable
last_ledger_decision: reject
last_ledger_route: search_files
calibration: insufficient: 2 events (<5); observational only
predicted_response_tokens: 1200
actual_response_tokens: 204
```

### `symforge` query (`intent=find`, repo architecture question)

```
decision: reject
error: 85.1%
Index not loaded. Call index_folder to index a directory.
Multi-hop chain failed … outcome=internal_failure
```

**Interpretation:** The trust envelope is working *mechanically* (reject, calibration pending, ledger line present) but **economics numbers are not yet meaningful** (predicted 1200 vs actual 204; error ~85%). The index is empty in this session, so the primary read path fails — yet the error tells the agent to call a tool **not available on the default surface** (see P0-1).

---

## Critical findings (act before calling the product “agent-ready”)

### P0-1 — Compact-surface recovery dead-end

| What | Detail |
|------|--------|
| **Symptom** | `symforge` / legacy formatters emit: *“Index not loaded. Call index_folder to index a directory.”* |
| **Reality** | Default surface is compact-3 only (`symforge`, `symforge_edit`, `status`). `index_folder` is **not** in `COMPACT_TOOL_NAMES` and is rejected by `enforce_compact_surface`. |
| **Evidence** | `src/protocol/format.rs` (message), `src/stel/surface.rs` (tool set), `src/protocol/surface_probe.rs` (default = Compact), `src/protocol/mod.rs` (dispatch gate) |
| **LLM impact** | Agent enters an **unrecoverable loop**: status says not ready → symforge says call index_folder → index_folder unavailable → no documented escape on compact surface except env opt-out (`SYMFORGE_SURFACE=full`) agents do not know about. |
| **Fix direction** | (a) Auto-index reliably before first symforge call; **or** (b) expose indexing on compact surface (`status` subcommand, `symforge intent=meta`, or bundled bootstrap in MCP startup); **or** (c) change all compact-path errors to name the real fix (`SYMFORGE_SURFACE=full`, operator `serve` attach, daemon rebind) — not `index_folder`. |

### P0-2 — Cold start with empty index in real MCP sessions

| What | Detail |
|------|--------|
| **Symptom** | Live MCP: `index_ready: false`, `index_files: 0`, generic `project: project`. |
| **Code intent** | `main.rs` auto-indexes when `SYMFORGE_AUTO_INDEX` ≠ false and `discovery::find_project_root()` succeeds. |
| **Likely gaps** | Daemon-backed stdio (`new_daemon_proxy`) may attach to a session without the workspace root indexed; Claude Desktop wrapper sets CWD to `%USERPROFILE%` (`init.rs` `create_desktop_wrapper_windows`), not the repo — breaking root discovery. Init registers `"env": {}` — no `SYMFORGE_SURFACE`, no workspace hint. |
| **LLM impact** | Agent believes SymForge is connected; all symforge queries fail until an invisible operator fixes daemon/root/CWD. |
| **Fix direction** | Init should set proven env (workspace root, surface profile, auto-index policy). Desktop wrapper should cd to configured workspace or pass `--root`. Status compact view should surface **actionable** empty-index reason (mirror `local_empty_reason` from startup). |

### P1-1 — Public docs still describe v7 (32-tool) as canonical

| Surface | Says | Should say (v8 default) |
|---------|------|-------------------------|
| `README.md` | “**32 canonical MCP tools**” (lines 24, 128, 328) | Compact-3 default; 32-tool `SYMFORGE_SURFACE=full` opt-out |
| `AGENTS.md` | “Current canonical `tools/list` exposes **32 tools**” | Compact-3 + status; legacy table under opt-out |
| `CHANGELOG` remediation note | “README rewrite” for v8 | README not rewritten at review time |
| `symforge init` | `SYMFORGE_TOOL_NAMES` lists 32+ legacy `mcp__symforge__*` entries in client allow-lists | Mismatch with MCP `list_tools` (3 tools) — agents granted tools that do not exist on wire |

**LLM impact:** Rules files, wiki links, and client allow-lists **train agents on a surface that is not default**, undermining STEL routing and bypass economics (previously flagged in `docs/reviews/v8-architecture-review-codex-resume.md` §13 Q1).

### P1-2 — Phase gates vs 8.0.0 ship date

`docs/stel-assumptions.md` phase table:

| Phase | Gate |
|-------|------|
| **3 executor + 8.0** | **A-015..A-016 validated** (trust envelope ↔ ledger; EMA calibration) |
| **2 L1 + L2** | A-008..A-014 evidence (many still OPEN/PARTIAL) |

**Reality at 8.0.0 tag:** A-015, A-016, A-011 (±20% predictor), A-008 (95% routing) remain **OPEN or PARTIAL**. Phase 0 GO authorized **`src/stel/` implementation**, not “all Phase 3 assumptions validated.”

**Risk:** Marketing/README “trust envelope” and “token-efficient” reads as **validated product claims**; internal register correctly marks them **hypothesis / observational**.

**Recommendation:** Publish an explicit **“8.0.0 capability matrix”**: Implemented / Measured / Observational / Deferred — aligned with `status` `deferred:` line.

### P1-3 — Status labels overstate subsystems marked deferred

Compact `status` always prints:

```
l2_economics: active
l4_ledger: active
handler_symforge_edit: preview-and-apply
deferred: …,ledger_persistence,…
```

But:

- `src/stel/mod.rs` module doc: *“Deferred: calibration auto-tuning/persistence, **symforge_edit apply path**.”*
- Full status on stdio MCP: `durable_ledger: unavailable` while `l4_ledger: active`.
- `ledger_persistence` appears in `DEFERRED_ITEMS` while L4 is labeled active.

**LLM impact:** “active” reads as production-ready; “deferred” is easy to miss in compact view. Agents cannot distinguish **in-memory session ledger** vs **durable restart-survival ledger**.

**Fix direction:** Use states like `in_memory_only | durable | disabled` instead of blanket `active`; align module doc with status strings.

---

## High findings (trust / robustness)

### H1 — Token economics envelope is uncalibrated (expected, but oversold externally)

Live session: `predict_error_pct` ~85%, `calibration: pending`, `calibration: insufficient events`. Assumption **A-011** (±20% predictor) is **OPEN**.

The envelope still prints `session_net_vs_manual: +213` alongside `decision: reject` — numerically correct per harness math but **semantically misleading** to LLMs trained to treat “+saved tokens” as success.

**Recommendation:** When `decision != serve`, prefix envelope with `economics_status: uncalibrated_reject` or suppress net-saved line on reject/degrade/bypass failure paths.

### H2 — Routing confidence `inferred` dominates live queries

Dogfood query routed `find → search_files → search_text` with `route_confidence: inferred`, then failed on empty index. Golden corpus (**A-008**) is **PARTIAL** (“95% trajectory metric not numerically measured”).

STEL is honest about confidence in the ledger JSON, but **planner fallbacks** can still burn schema+invoke tokens before failing — bad economics on errors.

### H3 — `symforge_edit` apply path: preview vs apply ambiguity

Status advertises `preview-and-apply`. Module header says apply path deferred. `edit_apply.rs` gates apply on loaded index with message referencing `index_folder` (same P0-1 dead-end).

Agents need a single documented flow: preview → `apply:true` + `if_match` on compact surface, with index readiness preconditions spelled in tool description — not scattered across Rust comments.

### H4 — Daemon IPC bypasses compact gate (documented, still a footgun)

`docs/reviews/external-review-remediation-2026-06-17.md` downgraded P2-4: internal `daemon.rs::execute_tool_call` intentionally reaches full tools for hooks/dogfooding.

**Fine for internal use**; confusing when comparing “SymForge MCP surface” vs “SymForge hooks hitting daemon HTTP.” Document in operator guide: **external contract = gated `/mcp` + stdio `call_tool`; hooks ≠ harness surface.**

### H5 — Init / client configuration not updated for v8 cutover

Observations:

- No `SYMFORGE_SURFACE` in registered MCP env (defaults compact — good — but agents lack escape hatch docs in config).
- Linux Codex gets `SYMFORGE_NO_DAEMON=1`; Windows/desktop paths differ — uneven cold-start behavior.
- Allow-lists still enumerate legacy tool names agents will never see on compact wire — **false affordances** in Claude/Codex/Kilo configs.

---

## Medium findings (quality / maintainability)

### M1 — Assumptions register internal inconsistency

Top table marks **A-005 OPEN**; Phase 0 evidence table marks **A-005 VALIDATED** (891 B). Both appear in the same file with different sections — maintainers know why; external readers do not.

**Fix:** Single source of truth per assumption ID; generated table from `docs/stel-assumptions.json` if needed.

### M2 — README trust claims vs STEL maturity

README: *“Every truncation is disclosed with the real cost, never silently applied.”* STEL degrade caps exist (`controller.rs`), but compact serve caps and fusion-empty handling were recently fixed (P3-7). Worth auditing all formatters for silent truncation under `max_tokens` on the **symforge** path specifically.

### M3 — Feature 007 (intelligence ports) — residual open item

`docs/reviews/007-review-focus-2026-06-17.md`: find-fusion both-empty → `Found` was **[OPEN]**; external remediation claims **FIXED** (EmptyResult). Worth one live symforge find-fusion query post-index to confirm on deployed binary.

### M4 — `edition = "2024"` in Cargo.toml

Unusual for ecosystem compatibility; not a functional bug if toolchain pinned — note for downstream packagers.

---

## What is genuinely good (credit where due)

1. **Assumptions register + gap-closure plan** — Rarely seen this explicit; “OPEN assumptions do not unlock phases” is the right doctrine even if 8.0 marketing outran it.
2. **Compact dispatch enforcement (P1-A)** — `ServerHandler::call_tool` gates before router; not list-only hiding.
3. **Deferred list in `status`** — `b_results,calibration_auto_tune,ledger_persistence,multi_step_planner` is honest if read carefully.
4. **Security remediation batch** — Origin gate, admin static/asset auth split, AAP key redaction, ledger drain on shutdown (`external-review-remediation-2026-06-17.md`) traced and tested.
5. **Embed isolation** — Preserved across v8 operator work; semver story in CHANGELOG is coherent.
6. **Golden replay + phase2 gate machinery** — Substantial test investment; PARTIAL verdicts are documented with artifact paths.
7. **Index integrity model** — Byte-exact snapshots, quarantine, idempotency on mutations (7.x layer) remain solid moat; STEL sits on top, does not replace it.

---

## Overstated surfaces (skeptic’s checklist)

| Claim (external or status) | Actual state |
|----------------------------|--------------|
| “32 canonical MCP tools” (README) | Default: **3** tools |
| “Token-efficient” / trust envelope (README hero) | Predictor **OPEN**; live error **~85%**; calibration insufficient |
| `l4_ledger: active` | Session ledger yes; **durable persistence deferred/unavailable** on stdio |
| `handler_symforge_edit: preview-and-apply` | Apply path gated; module doc lists apply as **deferred** |
| Phase 3 gate “A-015/A-016 validated” | Still **OPEN** at ship |
| “Call index_folder” recovery | **Blocked** on default compact surface |
| Agent init allow-lists | Legacy 32-tool names **≠** MCP `list_tools` |

---

## Recommended priority queue

### Before declaring “default agent MCP production-ready”

1. **Fix P0-1 + P0-2** — indexing on compact surface without hidden env vars; fix init/CWD/daemon root attachment.
2. **Rewrite README + AGENTS.md + init allow-lists** for compact-3 default (P1-1).
3. **Publish 8.0 capability matrix** tied to assumption IDs (P1-2).
4. **Normalize status vocabulary** — replace misleading `active` labels (P1-3).

### Next hardening tranche

5. Envelope semantics on reject/failure (H1).
6. Compact-path error messages that only reference **callable** tools (H2/H3).
7. Reconcile assumptions register tables (M1).
8. Operator doc: daemon/hook surface ≠ external MCP contract (H4).

### Can defer (already tracked)

- P3 ledger migration forward guard, retention (P3-A/B in 004 review).
- rmcp version pin drift (P3-C).
- A-020..A-022 transport/unification assumptions (Phase 4).

---

## Overlap with parallel reviewer

If the other session reported:

- **index_ready false vs working explore** — consistent with **session/root mismatch** (empty index in this MCP attach; explore may hit a different code path or cached daemon project). Root cause cluster: P0-2.
- **104% / 85% predict error** — consistent; economics layer is **observational**, not calibrated (A-011 OPEN).
- **durable_ledger unavailable** — confirmed on stdio MCP full status.
- **Assumptions register “damning admissions”** — confirmed; the register is the most trustworthy doc in the repo.

This pass adds emphasis on **compact-surface dead-end errors**, **init/desktop CWD**, and **public doc / allow-list drift** as distinct LLM-trust failures.

---

## Appendix — key code anchors

| Topic | Location |
|-------|----------|
| Compact tool set | `src/stel/surface.rs` |
| Default surface = compact | `src/protocol/surface_probe.rs` |
| Dispatch gate | `src/protocol/mod.rs` (`call_tool`) |
| Index-not-loaded message | `src/protocol/format.rs` |
| Status formatting | `src/stel/status.rs` |
| STEL deferred note | `src/stel/mod.rs` (module doc) |
| Phase gates | `docs/stel-assumptions.md` |
| Prior operator review | `specs/004-v8-operator-serve/review-findings-2026-06-16.md` |
| v8 remediation | `docs/reviews/external-review-remediation-2026-06-17.md` |

---

## Appendix B — Golden replay test run (2026-06-17)

`cargo test --test stel_golden_replay -- --test-threads=1` → **6 passed, 0 failed** (~5m35s compile + 0.2s test).

Three tests **skipped in-repo corpus fixtures** (`tests/fixtures/phase0-corpus/` not cloned per README): `s4_minimum_subset_replays`, `supported_pff_rows_bypass`, `supported_serve_rows_replay`. In-repo routing/classification tests still pass; full golden battery replay remains operator-dependent.

---

*End of review. No code changes made in this pass — findings only.*
