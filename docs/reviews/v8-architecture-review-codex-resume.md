# SymForge v8 Architecture Review (Codex Resume)

> **Codex session partial:** A prior Codex session on branch `v8/stel-architecture` completed branch verification, SymForge indexing, `docs/v8-bootstrap.md` read-through, and §10 code-path inspection before hitting a usage limit. Three planned subagents (runtime, protocol, docs) were spawned but never returned. This document resumes and completes that review from confirmed evidence plus independent repo reads (2026-06-12).

**Reviewer:** Cursor code-reviewer subagent  
**Branch:** `v8/stel-architecture` (verified)  
**Entry doc:** `docs/v8-bootstrap.md`  
**Binding gap register:** `docs/v8-gap-closure-plan.md`  
**Current shipped version (per bootstrap):** 7.21.1 → target 8.0.0 / 8.1.0  

---

## Executive summary

SymForge v8 design is internally consistent and correctly gated: **no `src/stel/` until Phase 0 pre-flight is green**, and that invariant holds. The codebase is still **7.x architecture** — 32-tool MCP surface, NL single-hop routing via `ask`, hook-oriented `TokenStats`, and three overlapping runtimes (daemon-proxy stdio, local stdio + HTTP sidecar, daemon HTTP API). The gap between design and `src/` is large but **mostly already registered** in `v8-gap-closure-plan.md`. The highest-risk blind spots are: (a) Phase 0 measurement artifacts are completely absent, so no gate can be computed yet; (b) agent compliance with BYPASS hints is unproven without the two-hop harness; (c) naming collision between **indexing admission** (`src/discovery/mod.rs`) and **economics admission** (planned `stel/controller.rs`) could confuse implementers.

**Verdict:** Architecture is sound to proceed **only** through Phase 0. Do not start `src/stel/` until §12 is green. 8.0 scope (economics) is well-separated from 8.1 (quality + deploy).

---

## §13 reviewer questions (explicit answers)

### 1. Controller / bypass: Will agents follow BYPASS hints, or retry SymForge and burn tokens?

**Assessment:** High risk without harness enforcement; moderate risk even with envelope if hosts lack machine-readable contract.

**Evidence today:**
- `ask` in `src/protocol/tools.rs` prepends a human-readable route envelope (`Route confidence`, `Chosen tool`, etc.) but **never emits** `decision: bypass` or structured bypass hints (`stel-schema.md` `StelBypassBody`).
- No pre-execution economics gate exists; every `ask` call executes exactly one legacy handler.
- `src/cli/init.rs` installs 32 direct MCP tools in `alwaysAllow` — agents can bypass `ask` entirely and call expensive tools without controller mediation.

**What the design gets right:**
- `v8-gap-closure-plan.md` §5.4 defines **bypass two-hop harness** (G-012, A-012).
- BYPASS rows excluded from H6 denominator (A-023, G-023).
- `StelTrustEnvelope` in `stel-schema.md` specifies machine-parseable economics header.

**Recommendations:**
- Treat agent BYPASS compliance as **harness-measured**, not assumed. Phase 0.10 must implement two-hop simulation before trusting H3/H4.
- L0 compact surface must be the **default** so schema tax does not make bypass pointless (agent already paid 62 KB before reading hint).
- Add `_meta` or structured JSON bypass payload (mirror SFB09 `result_status` pattern in `src/protocol/result_status.rs`) — prose hints alone will be ignored by many harnesses.
- `init` should stop advertising 32-tool direct access once compact mode ships (G-017).

**Tracked:** G-012, G-013, A-012, A-023, G-017.  
**Net-new:** Risk that `alwaysAllow` 32-tool init config **undermines** controller bypass even after STEL ships.

---

### 2. STEL layering: Is L0/L1/L2 split right, or over-engineered vs 1–2 meta-tools?

**Assessment:** The L0/L1/L2/L3/L4 split is **justified**, not over-engineered — but only if Phase 0 validates A-019 (compact-3 vs meta-tool).

**Rationale:**
- **L0 (3 tools)** solves a measurable problem: ~62 KB schema tax (bootstrap §2). This is independent of routing sophistication.
- **L1 (router → StelPlan)** is required for H5 (≤1 MCP call per single-chain task). Today's `ask` dispatches **one** tool with no multi-step plan (`src/protocol/smart_query.rs`, `tools.rs::ask`).
- **L2 (controller)** is the product differentiator: serve/bypass/degrade/cache is not reducible to "pick a tool." Without L2, v8 is a rename of `ask`.
- **L3 (32 internal handlers)** correctly preserves the moat in `src/live_index/`, `src/protocol/tools.rs`, `src/protocol/edit_tools.rs` — rewrite would be wasteful.
- **L4 (ledger)** must be separate from L2 so battery accounting (`session_net_accepted`, `acceptedServe`) survives controller algorithm changes.

**Challenge to meta-tool alternative:**
- A 1–2 meta-tool surface could pass H1 but may **fail H5** (multi-hop inside one call still needs plan+executor) and obscures edit safety boundaries (`symforge_edit` vs `symforge`).
- `v8-gap-closure-plan.md` §4.1 already mandates A/B (G-019) before locking L0.

**Recommendation:** Keep layers. Do **not** skip L2. If A-019 shows meta-tool wins on `session_net_accepted`, collapse L0 only — not L2/L4.

**Tracked:** G-019, G-009, stel-schema S1–S7 ordering.  
**Net-new:** None — layering rationale is well-documented.

---

### 3. Reference tracing (T2/T3): Which layer owns the fix — index, sidecar grep, or formatter? Realistic path to H6 50%?

**Assessment:** T2 is primarily **L3 index + reference capture** (`src/live_index/query.rs`); T3 is primarily **L3 formatter / outline selection** (`src/protocol/format.rs`). Sidecar grep is a **pivot path** (P-T2), not the primary fix. H6 50% is realistic only as an **8.1 program** with eligible-set policies, not a single Phase 2 item.

**T2 (find references) — index layer:**
- `src/live_index/query.rs`: `find_references_for_name`, `capture_find_references_view` — symbol-index refs, not full text/markdown/bench coverage.
- `src/protocol/tools.rs` + `src/protocol/format.rs`: `find_references_compact_view` controls payload shape.
- Gap plan §6.1 hypothesizes missing markdown/bench/cross-file text — consistent with index scope limits.

**T3 (outline) — formatter layer:**
- Outline/load-bearing symbol omission is a **formatter/section selection** problem in `format.rs` (gap plan §6.2 cites S=3718 vs M=540 on worst row).

**Sidecar role:**
- `src/sidecar/` provides HTTP hooks and `TokenStats`, not reference tracing logic.
- P-T2 pivot: mandatory bypass with grep envelope — removes T2 from H6 eligible set (4 rows).

**Path to H6 ≥50%:**
- With P-FF (4 rows bypass) + optional P-T2 (4 rows) + T3-small bypass (4 rows), eligible denominator shrinks to ~24–28 rows; need ~12–14 equiv.
- Programs T2/T3 in gap plan §6 are the realistic path; expect **index + formatter** work, not STEL layering changes.

**Tracked:** G-029, G-030, G-031, §6.1–6.4, A-029.  
**Net-new:** Confirm T2 spike should start with `src/live_index/query.rs` ref-source audit, not sidecar routes.

---

### 4. Compact edit schema: Can structural edits fit ≤1.5 KB JSON Schema, or must edits merge into `symforge`?

**Assessment:** **Unknown — must be measured** (A-025 OPEN). Pivot to `intent=edit` on `symforge` is likely if `symforge_edit` carries full structural DTOs.

**Evidence today:**
- Seven edit tools in `src/protocol/edit_tools.rs` with rich per-tool input structs (replace/insert/batch paths).
- `stel-schema.md` budgets `symforge_edit` ≤1500 B; kill criteria include merge into `symforge` or resource-first edits.
- No compact edit surface stub exists; no `list_tools` byte measurement artifact.

**Recommendation:**
- Phase 0.7b must measure serialized schema bytes before Phase 1.
- If over budget: prefer **`intent=edit` on `symforge`** with minimal discriminated union (operation + symbol locator + body) over keeping seven edit tool schemas.
- Do not block 8.0 on perfect edit ergonomics — block on H1 feasibility with documented pivot.

**Tracked:** G-025, A-025, G-005b, stel-schema §4.2.  
**Net-new:** `edit_tools.rs` is 1600+ lines with seven `#[tool]` registrations — strong prior that 1500 B requires merge or radical DTO slimming.

---

### 5. Daemon → server merge: Risks in merging sidecar + dropping local mode?

**Assessment:** Merge is correct end-state for 8.1 but carries **session, security, and fallback** risks. Do not drop local mode until A-021 battery proves no regression.

**Current topology (verified):**

| Mode | Entry | Index owner | HTTP |
|------|-------|-------------|------|
| Daemon-backed stdio (default) | `src/main.rs` → `daemon::connect_or_spawn_session` | Daemon `ProjectInstance` | Daemon HTTP + port files via sidecar helpers |
| Local stdio + sidecar | `src/main.rs` → `run_local_mcp_server_async` | In-process `LiveIndex` | Separate `sidecar::spawn_sidecar` |
| Daemon CLI | `symforge daemon` → `src/daemon.rs` | Per-project instances | Axum routes incl. `call_tool_handler` |

**Risks:**
1. **Double proxy hop:** Daemon-backed MCP calls `proxy_tool_call` → daemon HTTP → handler (`tools.rs` + `daemon.rs`). In-process dispatch (G-022) must preserve auth token semantics (`daemon.rs` fail-closed client token).
2. **Session / project switching:** Daemon-proxy `index_folder` invalidates stale local index — subtle state bugs if merged incorrectly (see `daemon.rs` tests `test_index_folder_proxy_switch_invalidates_stale_local_index`).
3. **Local-empty mode:** `StartupPlan::LocalEmpty` still serves MCP without project root — needed for editor attach before `cd` to repo. Dropping local mode breaks this (`src/main.rs` `local_empty_reason`).
4. **Governor relocation:** `src/sidecar/governor.rs` is sidecar-scoped today; unified server must apply write-gate globally.
5. **Hook + MCP duplication:** `src/cli/init.rs` hooks (Read/Edit/Grep) run parallel to MCP tools — merge does not eliminate hooks; economics ledger must attribute both.

**Recommendation:**
- 8.0: keep topologies; implement STEL in-handler for both daemon-proxy and local paths.
- 8.1: merge per G-021 only after battery parity (A-021); retain stdio shim; document SSH tunnel fallback (ideation.md).
- Do **not** drop `SYMFORGE_NO_DAEMON` escape hatch until remote `symforge serve` is proven.

**Tracked:** G-021, G-022, A-021, A-022.  
**Net-new:** Local-empty startup path is a **hidden requirement** for merge planning — not prominent in bootstrap §5.2.

---

### 6. Streamable HTTP: Missing pieces in rmcp/axum integration?

**Assessment:** Entire 8.1 transport stack is **unimplemented**. Phase 0.13 compile spike (A-020) is correctly gated before Phase 4 code.

**Verified gaps:**

| Piece | Status | Location |
|-------|--------|----------|
| rmcp Streamable HTTP server feature | Missing | `Cargo.toml` — `features = ["transport-io"]` only |
| `symforge serve` CLI | Missing | `src/cli/mod.rs` — no `Serve` variant |
| `/mcp` route handler | Missing | No `serve.rs`; daemon has unrelated HTTP routes |
| Bearer API key auth on MCP | Missing | Daemon has session auth tokens; not MCP Bearer |
| Init `--url` templates | Missing | `src/cli/init.rs` — stdio `command` only |
| Battery stdio vs HTTP parity | Missing | G-020, A-020 |

**Likely work (from gap plan §8):**
```toml
rmcp = { features = ["transport-io", "transport-streamable-http-server", "server"] }
```
Plus axum router: `POST /mcp`, `GET /health`, merge daemon project/session APIs or keep internal.

**Risks:**
- rmcp maturity (A-031 spike) — kill criteria allow minimal JSON-RPC shim.
- Index-on-server-host invariant: remote attach must not imply remote FS indexing (bootstrap §5.2) — deployment docs matter as much as code.

**Tracked:** G-020, G-030b, §8, A-020, A-031.  
**Net-new:** None.

---

### 7. Proof harness: Is `session_net_accepted` + eligible H6 denominator the right accounting?

**Assessment:** **Yes** — this is the correct accounting model and fixes the 7.x optimism problem.

**Why it works:**
- **H4 `session_net_accepted`** = Σ(M−S) over rows where judge=`EQUIVALENT` **and** S≤M — prevents SYMFORGE-LESS and sGteM from counting as wins (bootstrap §3, gap plan §1).
- **`session_net_all36`** diagnostic only — catches token wins with quality loss.
- **H6 eligible denominator** excludes BYPASS-policy rows (P-FF, P-T2, T3-small) — resolves bypass vs quality tension (A-023).
- **H3** scoped to accepted serve on small-file rows — aligns with north star.
- **H8** per-language accepted-serve losses — language-fairness guard separate from global H4.

**What's missing in implementation:**
- No `compare-results.js` (G-005).
- No per-row `acceptedServe`, `decision`, `eligibleH6` fields in harness output yet.
- `TokenStats` in `src/sidecar/mod.rs` tracks hook fires and naive efficiency — **not** per-row equivalence or `session_net_accepted`.

**Recommendation:** Implement harness **before** STEL so controller tuning has a ruler. Add worst-case schema mode (`--schema-per-call`) per §5.3 from day one.

**Tracked:** G-001, G-005, G-006 (H4 conflation DONE in docs), G-023, §5.1–5.3.  
**Net-new:** Existing `TokenStats` metrics must not be mistaken for v8 ledger — rename/distinguish during L4 implementation to avoid false "we already have H4" claims.

---

### 8. Biggest gap between this design and `src/` that we have not documented?

**Assessment:** Most gaps **are** documented. The largest **under-emphasized** gaps:

1. **Phase 0 artifact vacuum:** No `compare-results.js`, `routes.golden.jsonl`, or `docs/research/` directory anywhere in repo or sibling `sf-bench`. §12 is 0% complete — this is the true blocker, not STEL module design.

2. **Admission naming collision:** `src/discovery/mod.rs::classify_admission` (indexing: HardSkip/MetadataOnly/Normal) vs planned L2 `AdmissionDecision` (economics: serve/bypass/degrade). Different domains, same word — implementers will conflate them.

3. **`ask` route envelope ≠ trust envelope:** Current envelope in `tools.rs::ask` is routing telemetry, not economics/trust (`stel-schema.md` `StelTrustEnvelope`). Reusing `ask` output shape for L0 would ship the wrong contract.

4. **Init config fights compact paradigm:** `src/cli/init.rs` `CLAUDE_ALWAYS_ALLOW` / `SYMFORGE_TOOL_NAMES` (32 entries) + stdio spawn cements 7.x agent behavior after 8.0 ships unless init is version-aware.

5. **No `SYMFORGE_SURFACE` env:** Compact mode is spec-only (G-017); no runtime switch exists in `src/`.

**Not a gap (avoid false paths):** There is no `src/host/` — remote attach is daemon/server HTTP, not a separate host crate.

**Tracked partially:** G-017, G-030b, Phase 0 steps.  
**Net-new:** Items 2, 3, 4 above deserve explicit rows in gap register or stel-schema glossary.

---

## Design vs implementation gaps (§10 checklist)

| Priority | Path | Design expectation | Implementation reality | Scope | Gap ID |
|----------|------|-------------------|------------------------|-------|--------|
| P0 | `src/stel/` | STEL types L0–L4 (8.0) | **Does not exist** — correct pre-flight | 8.0 | §12 blocker |
| P0 | `src/main.rs` | stdio MCP; unified server (8.1) | Default **daemon-backed stdio proxy**; fallback **local stdio + HTTP sidecar** | 8.0 stdio / 8.1 merge | G-021 |
| P0 | `src/daemon.rs` | Proto unified server | Multi-project HTTP API, `call_tool_handler`, session auth | 8.1 | G-020, G-022 |
| P0 | `src/protocol/tools.rs` | 32 handlers internal; L0 compact 3-tool | **32 public `#[tool]`**; widespread `proxy_tool_call` for daemon mode | 8.0 | G-017, G-005b |
| P0 | `src/protocol/edit_tools.rs` | Edits via `symforge_edit` or merged intent | **7 public edit tools** with full schemas | 8.0 | G-025 |
| P0 | `src/protocol/smart_query.rs` | L1 → `StelPlan` | `QueryIntent` enum + `classify_intent` only | 8.0 | G-009 |
| P0 | `src/protocol/tools.rs::ask` | L0 `symforge` single-chain L1–L4 | Single legacy handler dispatch + route envelope | 8.0 | G-008, H5 |
| P0 | *(planned)* `src/stel/controller.rs` | serve/bypass/degrade/cache | **Not implemented** | 8.0 | G-012, H3, H4 |
| P0 | `src/protocol/format.rs` | Competent manual M, trust footers | Hook-oriented savings footers, not per-row ledger | 8.0 | G-011 |
| P0 | `src/sidecar/mod.rs` (`TokenStats`) | L4 ledger seed | Counters only; **no** `session_net_accepted` | 8.0 | H4, H7 |
| P1 | `src/sidecar/governor.rs` | Request concurrency/write gate | Implemented — reusable in unified server | 8.1 merge | (reuse) |
| P1 | `src/live_index/` | Core moat | Implemented — becomes L3 | 8.0/8.1 | G-029, G-030 |
| P1 | `src/cli/init.rs` | stdio today; `init --url` (8.1) | stdio + 32-tool allowlists; **no `--url`** | 8.0 init debt / 8.1 | G-017, G-030b |
| P1 | `Cargo.toml` | rmcp + Streamable HTTP (8.1) | **`transport-io` only** | 8.1 | G-020, A-031 |
| P2 | `src/stel/` | Should not exist pre-flight | **Absent** ✓ | — | §12 OK |

**External harness:** `E:\project\sf-bench\` — `compare-results.js` and `routes.golden.jsonl` not found (Phase 0 OPEN).

---

## 8.0 vs 8.1 scope separation

### 8.0.0 — STEL economics (ships on stdio)

| Delivers | Does not deliver |
|----------|------------------|
| `src/stel/` L0–L4 per `stel-schema.md` S1–S7 | Streamable HTTP / `symforge serve` |
| 3-tool compact surface (H1) | H6 equivalence program |
| Controller + bypass/degrade (H3, H4, H5) | H8 per-language gate |
| `symforge_status` ledger battery headline | `symforge init --url` |
| Golden trajectory replay (H2) | Daemon/sidecar merge |
| Pin `results-v8-8.0-baseline.json` at tag | Remote deploy marketing |

**Gates:** H1, H2, H3, H4, H5, H7

### 8.1.0 — Quality + deploy

| Delivers | Depends on |
|----------|------------|
| Programs T2/T3 + P-FF policies (H6) | 8.0 STEL + compact surface |
| H8 per-language accepted-serve fairness | H4 ledger from 8.0 |
| `symforge serve` + Streamable HTTP `/mcp` + Bearer key | A-020 rmcp spike PASS |
| Unified axum server (merge sidecar + daemon routes) | A-021 battery parity |
| `symforge init --url` paste-ready configs | G-030b |

**Gates:** H6, H8 + deploy acceptance (A-020..A-022)

---

## Tracked in gap plan vs net-new findings

### Already tracked (confirmed in code inspection)

- `src/stel/` blocked — §12 all unchecked
- 32-tool surface, no compact L0 — G-017, G-005b, A-005
- `smart_query` single-hop only — G-008, G-009, H5
- No economics controller — G-012, H3, H4
- TokenStats ≠ v8 ledger — implied by L4/stel-schema; harness §5.1
- init stdio + 32 allowlists — G-017
- No `init --url` — G-030b
- rmcp transport-io only, no serve — G-020, §8, A-031
- Daemon/sidecar/local sprawl — G-021, G-022
- T2/T3 quality program — G-029, G-030, §6
- Phase 0 pre-flight items — §12, G-001..G-005

### Net-new (recommend adding to gap register or schema glossary)

| ID | Finding | Suggested action |
|----|---------|------------------|
| **G-NEW-1** | `discovery::classify_admission` vs L2 economics `AdmissionDecision` naming collision | Rename one in schema (`IndexAdmission` vs `EconomicsDecision`) |
| **G-NEW-2** | `ask` route envelope ≠ `StelTrustEnvelope` — shape mismatch if L0 reuses ask output | L0 must implement schema-defined envelope, not extend ask prose |
| **G-NEW-3** | `init` 32-tool `alwaysAllow` undermines compact paradigm post-8.0 | Version-aware init: compact hosts get 3 tools only |
| **G-NEW-4** | `TokenStats` efficiency metrics risk false H4 claims during migration | Document "not v8 ledger" in code + README until L4 ships |
| **G-NEW-5** | Local-empty startup (`StartupPlan::LocalEmpty`) is merge constraint | Add to G-021 acceptance criteria — must work after server merge |

---

## Residual risks

1. **A-019 unresolved** — compact-3 vs meta-tool could change L0 shape after Phase 0.8.
2. **A-025 unresolved** — edit schema may force `symforge` merge, affecting H1 budget.
3. **Agent bypass compliance** — economics north star fails if harness does not enforce two-hop completion.
4. **H6 50% may require pivots** (P-T2, P-FF) — acceptable per §4.5 but must be explicit before 8.1 tag.
5. **rmcp Streamable HTTP maturity** — spike failure triggers shim fork (gap plan §4.4 kill path).

---

## Verification performed

| Check | Result |
|-------|--------|
| `git branch --show-current` | `v8/stel-architecture` |
| `src/stel/` exists | No |
| `Cargo.toml` rmcp features | `transport-io` only |
| `#[tool(` count | 25 (`tools.rs`) + 7 (`edit_tools.rs`) = 32 |
| `compare-results.js` | Not found in workspace |
| `routes.golden.jsonl` | Not found in workspace |
| `docs/research/` | Not found |
| `serve.rs` / `symforge serve` | Not found |
| `SYMFORGE_SURFACE` in `src/` | Not found |

---

---

## Addendum: Codex subagent findings (2026-06-12)

Three Codex explorers completed after the main session hit a usage limit. Agent 4 produced no output. **Doc fixes applied** from Agent 3 + Agent 2 in `v8-gap-closure-plan.md`, `stel-assumptions.md`, `stel-schema.md`, `stel-architecture.md`.

### Agent 1 — Runtime / deploy / merge

| Priority | Finding | Action |
|----------|---------|--------|
| P0 | No `symforge serve` / `/mcp`; rmcp `transport-io` only | Already G-020; §12B / Phase 4 |
| P0 | Three runtimes (daemon-proxy, local+sidecar, daemon REST); `SYMFORGE_NO_DAEMON=1` on Linux Codex init | Keep local fallback until HTTP parity (A-021); document in G-021 acceptance |
| P0 | `RequestGovernor` not universal — local stdio and sidecar bypass it | **G-NEW-6:** shared `ToolExecutor` with governor for all transports |
| P1 | Daemon bearer ≠ product API key; init emits stdio not URL configs | G-030b, A-022 |
| P1 | **Security:** sidecar binds arbitrary host, no auth (`SYMFORGE_SIDECAR_BIND=0.0.0.0`) | **G-NEW-7:** loopback-only sidecar or retire standalone HTTP before 8.1 |
| P2 | No transport-agnostic server boundary — daemon string-maps tools | **G-NEW-8:** `ServerRuntime` / `ToolExecutor` owns index + STEL + governor + auth |

### Agent 2 — Protocol / STEL / accounting

| Priority | Finding | Action |
|----------|---------|--------|
| P1 | No compact L0; 32 tools still advertised | G-017, Phase 1 |
| P1 | `ask` is single-hop, not StelPlan + controller | G-008, G-012 |
| P1 | No structured BYPASS contract; route text encourages legacy tools | G-012, trust envelope in schema |
| P1 | **H4 tautology** if accepted serve requires S≤M by definition | **FIXED:** accepted serve = SERVE + EQUIVALENT; S vs M separate (H3 catches sGteM) |
| P2 | T2/T3 split across index / sidecar / formatter | §6 program; index owns ref truth |
| P2 | Edit DTOs unlikely to fit 1.5 KB; dry_run defaults wrong | A-025 pivot likely |

### Agent 3 — Docs / pre-flight consistency

| Finding | Action |
|---------|--------|
| §12 mixed Phase 1 + 8.1 gates | **FIXED:** split §12A (before `src/stel/`) vs §12B (Phase 4) |
| A-020 mislabeled compile spike | **FIXED:** spike = A-031, artifact `A-031-rmcp-spike.md` |
| Phase 0 “start when A-001..A-004 validated” circular | **FIXED:** “Phase 0 exits when…” |
| A-005/A-025 vs no-`src/stel/` block | **FIXED:** allow measurement stubs outside `src/stel/` |
| H5 ownership drift across docs | **FIXED:** S3 = external H5 enabler; Phase 2 exit = full H5 proof |
| compare-results before 8.0 baseline | **FIXED:** `--preflight` mode in §5.1 |
| `routes.golden.jsonl` path drift | **FIXED:** canonical `sf-bench/routes.golden.jsonl` |
| Assumption rule too strong | **FIXED:** wording in `stel-assumptions.md` |

---

## Conclusion

The v8 architecture documents describe a coherent, gate-driven migration from 7.x tool sprawl to STEL-mediated economics. The codebase is honestly **pre-implementation**: design and gap closure are ahead of code, and the `src/stel/` blocker is respected. Proceed with **Phase 0 only** until **§12A** is green. The critical path is measurement (harness + golden file + schema bytes), not STEL typing. Separate 8.0 (economics on stdio) from 8.1 (H6/H8 + serve) remains the correct release strategy.

*Review completed: 2026-06-12 · Addendum merged same day*
