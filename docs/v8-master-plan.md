# SymForge v8 — Master plan

Branch: `v8/stel-architecture`  
Status: **PRE-IMPLEMENTATION** — see [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) §12A (blocks `src/stel/`)

**Binding gap closure:** [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md)  
**Ideation:** [`ideation.md`](ideation.md)  
**Companion:** [`README.md`](README.md), [`stel-architecture.md`](stel-architecture.md), [`stel-schema.md`](stel-schema.md), [`stel-assumptions.md`](stel-assumptions.md)

---

## What we are building (one paragraph)

**SymForge is a superior code-intelligence MCP server whose only headline is honest net token savings** — every accepted call beats competent manual (`grep` + ~50-line read window) or SymForge **bypasses with a real cheaper alternative**. It runs on Windows, Linux, macOS, and WSL; deploys with **`symforge serve`** (MCP + **admin UI**); attaches from any harness via **URL + API key** with **scan-and-apply config**; and proves superiority with **sf-bench**, not marketing.

We are **not** optimizing for revenue tiers, OAuth complexity, or feature count. We optimize for **measurable economics**, **trust**, and **recommendability**.

---

## North star (non-negotiable)

```text
On every ACCEPTED call:
  answer equivalent to manual AND tokens(S) ≤ tokens(M)
  OR explicit BYPASS with cheaper host path (Read/grep)

Session headline: **`session_net_accepted`** ≥ 0 on pinned battery (accepted serve rows only)
Also report **`session_net_all36`** as diagnostic
Never count SYMFORGE-LESS or sGteM as wins; BYPASS excluded from H6 denominator
```

---

## Where 7.x was (informational only)

sf-bench on **7.21.1** diagnosed the old paradigm: ~62kB schema, low equivalence on trace tasks, SYMFORGE-LESS on many rows. That run **motivated v8** — it does **not** define v8 success.

See `E:\project\sf-bench\RESULTS.md` as a **7.x appendix**. v8 gates (H1–H8) are absolute criteria on the corpus, not “≥ 7.21.1.”

---

## Architecture target (three layers, one process)

```text
┌─────────────────────────────────────────────────────────────┐
│ DEPLOY          symforge serve --listen HOST:PORT --api-key … │
│ TRANSPORT       MCP Streamable HTTP  /mcp                     │
│                 Admin UI + /api/v1  (operator — see admin doc)│
│                 (optional stdio shim for legacy clients)      │
├─────────────────────────────────────────────────────────────┤
│ STEL (intelligence)                                           │
│   L0  symforge | symforge_edit | symforge_status  (≤5kB)   │
│   L1  Router → StelPlan (smart_query + golden paths)         │
│   L2  Controller → serve | bypass | degrade | cache_hit      │
│   L3  32 core handlers (internal in compact mode)            │
│   L4  TokenStats + trust envelope = battery headline         │
├─────────────────────────────────────────────────────────────┤
│ RUNTIME (existing, consolidated)                              │
│   ProjectRegistry → LiveIndex + watcher per repo             │
│   SessionRegistry → multi-client                             │
│   RequestGovernor → queue, parallelism, write gate           │
│   Sidecar routes merged into server (hooks on same port)      │
└─────────────────────────────────────────────────────────────┘
```

**Axe when battery proves inferior:** local full-stack MCP spawn, separate sidecar process, 32-tool default `tools/list`, daemon REST as “remote MCP” story.

**Keep:** symbol index, structural edits, trust envelopes, sf-bench, embed API.

---

## Release gates

### 8.0.0 (STEL economics)

| Gate | Pass condition |
|------|----------------|
| **H1** | Compact `tools/list` ≤ **5,000 B** (incl. edit budget — **A-025**) |
| **H2** | Golden trajectories ≥ **95%** (with **expected_equiv** — **A-028**) |
| **H3** | Small-file **accepted serve** rows: **0** `sGteM` |
| **H4** | **`session_net_accepted`** ≥ **0** |
| **H5** | ≤ **1** MCP call per single-chain task |
| **H7** | Re-run variance on **`session_net_accepted`** ≤ **±2%** |

### 8.1.0 (quality + recommendable deploy)

| Gate | Pass condition |
|------|----------------|
| **H6** | `EQUIVALENT` / eligible rows ≥ **50%**; BYPASS excluded (**A-023**) — absolute, not vs 7.x |
| **H8** | Per-language accepted serve: zero accepted losses per language |
| **Deploy** | `symforge serve` + URL + API key (**A-020..A-022**) |
| **Operator** | Admin UI + onboarding + harness hub (**O1–O8**) — [`v8-admin-ui.md`](v8-admin-ui.md) |

Full definitions: [`stel-architecture.md`](stel-architecture.md#release-gates-all-required-for-800).

---

## Phased plan (actionable)

### Phase 0 — Trust the ruler *(current, blocks everything else)*

**Goal:** Pinned baseline + harness we believe.

| # | Action | Deliverable | Assumption |
|---|--------|-------------|------------|
| 0.1 | Re-run sf-bench **2×** same binary + SHAs | Two `results.json`; variance report | **A-001** |
| 0.2 | Spot-check 6 rows: manual harness vs `M` | Notes in assumption register | **A-002** |
| 0.3 | Harness shakedown on v8 branch release binary | `results-v8-harness-shakedown.json` | **A-003** |
| 0.4 | Human sample 10 equivalence judgments | False pos/neg doc | **A-004** |
| 0.5 | Build **`routes.golden.jsonl`** (36 rows) | Path corpus | unlocks H2 |
| 0.6 | Implement **`compare-results.js`** | CI PASS/FAIL per gate | |
| 0.7 | Stub compact `list_tools`; measure bytes | H1 feasibility | **A-005** |
| 0.8 | Stub **meta-tool** surface; battery A/B | Lock L0 shape | **A-019** |
| 0.9 | Re-pin baseline on **branch release binary** | Fresh JSON | **A-024** |

**Exit:** §12 pre-flight in [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) — **all boxes [x]**, including A-019, A-005, A-025, bypass harness, golden file, compare-results.

---

### Phase 1 — Compact surface + types *(STEL L0)*

**Goal:** Kill schema tax; one front door.

| # | Action | Deliverable | Gate |
|---|--------|-------------|------|
| 1.1 | `src/stel/mod.rs` types per `stel-schema.md` | Rust types | |
| 1.2 | Public tools: **`symforge`**, **`symforge_edit`**, **`symforge_status`** | H1 | **H1** |
| 1.3 | `SYMFORGE_SURFACE=compact\|full` (default compact) | Migration escape | |
| 1.4 | Trust envelope on every L0 response | Visible economics | |
| 1.5 | Path replay on 5 golden rows | Trajectory test | **H2** seed |

**Exit:** H1 PASS measured.

---

### Phase 2 — Router + controller *(STEL L1 + L2)*

**Goal:** Zero accepted-call losses on small files; session net ≥ 0.

| # | Action | Deliverable | Gate |
|---|--------|-------------|------|
| 2.1 | `PlanBuilder` — multi-step internal chain | `StelPlan` | **H5** |
| 2.2 | `AdmissionController` — serve / bypass / degrade / cache | `StelDecision` | **A-008..A-014** |
| 2.3 | Bypass: “use Read L1–N” when index says loss | Small-file rows | **H3** |
| 2.4 | **T2/T3 spike** — ≥2/4 T2 equiv or bypass policy | Research log | **A-029** |
| 2.5 | Battery on **compact STEL** candidate | `results-v8-candidate.json` | **H3, H4** |

**Exit:** H3, H4 PASS on **compact surface** (H1 must land in Phase 1 first).

---

### Phase 3 — Calibration + **8.0.0** *(STEL L4)*

**Goal:** Footer matches battery economics.

| # | Action | Deliverable | Gate |
|---|--------|-------------|------|
| 3.1 | `symforge_status` = **`session_net_accepted`** | Same headline as RESULTS | **A-015** |
| 3.2 | Predictor calibration EMA | Error trend down | **A-016** |
| 3.3 | Golden corpus full replay | **H2** ≥ 95% | **H2** |
| 3.4 | Repeatability re-run | **H7** | **H7** |
| 3.5 | Tag **8.0.0**; pin **`results-v8-8.0-baseline.json`** | Release + v8 baseline | **H1–H5, H7** |

**Exit:** Economics gates PASS → ship **8.0.0** (stdio MCP + compact STEL).

---

### Phase 4 — Quality + deploy → **8.1.0**

**Goal:** Equivalence program + recommendable `symforge serve`.

| # | Action | Deliverable | Gate |
|---|--------|-------------|------|
| 4.1 | T2/T3/full-file equivalence work | **H6, H8** | **H6, H8** |
| 4.2 | **`symforge serve`** — Streamable HTTP `/mcp` | Remote MCP | **A-020** |
| 4.3 | Merge sidecar; drop local duplicate stack | Single process | **A-021** |
| 4.4 | **`symforge init --url … --api-key …`** | Paste-ready JSON | |
| 4.5 | sf-bench: stdio vs HTTP — no regression | Battery row | **A-022** |
| 4.6 | **`stel_ledger` SQLite** + server key store | rusqlite migrations | **G-038, G-039** |
| 4.7 | **Admin UI MVP** — `/admin` + `/api/v1/*` | Operator dashboard | **G-037, G-042** |
| 4.8 | **First-run onboarding** — URL banner, browser open, wizard | Install/update UX | **G-040** |
| 4.9 | **Harness hub** — scan configs, per-harness keys, apply | `src/harness/` + admin | **G-041** |
| 4.10 | Tag **8.1.0** | **O1–O8** + H6/H8 | all gates |

**Exit:** H6/H8 PASS + **O1–O8 PASS** + documented URL+key config on two hosts.

---

## What to adopt (net gain only)

| Adopt | Why |
|-------|-----|
| **sf-bench as law** | Only honest comparator; anti-cheat already documented |
| **STEL compact-3 surface** | Removes ~57kB schema overhead |
| **SavingsController bypass** | Stops small-file tax; honest “don’t use me” |
| **Trust envelope + session ledger** | Recommendability in 30 seconds |
| **Daemon → unified server** | One index, multi-session, governor |
| **Streamable HTTP + Bearer API key** | Industry MCP remote config |
| **Admin web UI (local ops)** | Stats, index ops, keys, harness setup — same server as `/mcp` ([`v8-admin-ui.md`](v8-admin-ui.md)) — **8.1 committed** |
| **Golden trajectories** | Path proof separate from unit tests |
| **Assumption register** | Stops “implement anyway” drift |

## What to defer or axe

| Defer / axe | Why |
|-------------|-----|
| Enterprise OAuth / SSO / SOC2 | Does not move token gates; adds scope |
| Multi-tenant billing | User goal is product quality, not revenue |
| Semantic/vector tier | Roadmap item; fix T2 + schema first |
| 32-tool default surface | Battery-proven loss on many rows |
| Local MCP + sidecar default | Duplicate index; confuse deploy story |
| README “70–95%” claims | Contradicts pinned battery; fix copy at 8.0 |
| Raising H1 threshold to pass | Cheating; invalidates north star |

---

## Harness config (target UX)

**Server:**

```bash
symforge serve --listen 0.0.0.0:8787 --api-key sf_your_key
```

**Any MCP client:**

```json
{
  "mcpServers": {
    "symforge": {
      "type": "streamable-http",
      "url": "http://HOST:8787/mcp",
      "headers": {
        "Authorization": "Bearer sf_your_key"
      }
    }
  }
}
```

Index stays on the **server host** (machine with the repo). Remote attach = remote MCP to that box, not magic cross-filesystem indexing.

---

## Immediate next actions

**Do not skip steps.** Full order: [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) §7 Phase 0 + §12 checklist.

1. Implement **`compare-results.js`** + row classification in battery output.
2. Harness shakedown on v8 branch binary (not a 7.x regression gate).
3. Seed **`routes.golden.jsonl`** (36 rows).
4. Run Phase 0.1–0.11; validate assumptions; check §12 boxes.
5. **Only then** `src/stel/mod.rs`. Pin **`results-v8-8.0-baseline.json` at 8.0 tag**, not before.

---

## Success looks like

A developer runs SymForge (stdio **8.0**, then **`symforge serve` in 8.1**), opens **`/admin`** after install, scans harnesses and applies MCP config in clicks, sees footer **“saved N tokens vs grep+read”** on accepted calls, and sf-bench proves **`session_net_accepted` ≥ 0** with **zero accepted `sGteM`** on small files.

That is the product worth recommending.

---

## Server assumptions (Phase 4)

Registered in [`stel-assumptions.md`](stel-assumptions.md): **A-020..A-022**, **A-023..A-029**. Phase 4 starts after **8.0.0**.

---

## Adversarial review (2026-06-12)

Reviews: [adversarial doc review](338e3903-f0bb-4311-bc9e-6f0ac31a1804), [feasibility review](10c3b99a-97d8-4920-afd1-fa9b8cd7cec0).

**Verdict:** **PROCEED WITH CHANGES** — foundation sound; gates and phase order were wrong.

| Finding | Response (in docs) |
|---------|-------------------|
| H4 passed on all-36 while SYMFORGE-LESS | **H4 → `session_net_accepted`**; all-36 diagnostic |
| H6 50% unreachable in Phase 2 alone | **8.0 / 8.1 split**; T2 spike **A-029** |
| Compact surface after controller | **H1 in Phase 1**; phases aligned |
| `serve` after 8.0 tag | **`symforge serve` in 8.1** |
| A-019 open but Phase 1 locked compact-3 | **Phase 0.8** meta-tool A/B |
| Phase numbering drift | **README crosswalk** |

---

## References

- Ideation: [`ideation.md`](ideation.md)

- Battery: `E:\project\sf-bench\`
- Architecture: [`stel-architecture.md`](stel-architecture.md)
- Types: [`stel-schema.md`](stel-schema.md)
- Assumptions: [`stel-assumptions.md`](stel-assumptions.md)
