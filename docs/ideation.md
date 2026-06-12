# SymForge — product ideation

**Status:** living document · branch `v8/stel-architecture`  
**Purpose:** Capture *why* and *what* before nitty-gritty specs. Elaborate sections over time; link outward instead of duplicating.

**Companion docs:** [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) (binding execution), [`v8-master-plan.md`](v8-master-plan.md), [`stel-architecture.md`](stel-architecture.md), [`stel-assumptions.md`](stel-assumptions.md).

---

## Vision

SymForge is a **code-intelligence MCP server that genuinely reduces token use** for agent workflows — not by gaming metrics, but by beating what a competent developer would do manually (`grep` + a bounded read window) or by **stepping aside honestly** when it cannot win.

We want something people **deploy in one command**, **attach from any MCP harness** (URL + API key), and **recommend to others** because the savings show up in every session footer — and match an independent battery anyone can re-run.

We are **not** building this to monetize tiers or chase enterprise checkbox features. We are building something **superior on measured economics**, then making it **easy to run everywhere** (Windows, Linux, macOS, WSL).

---

## North star (one rule)

```text
Every ACCEPTED call:
  equivalent answer AND tokens(S) ≤ tokens(M)
  OR explicit BYPASS → cheaper real path (Read / grep)

Never ship a call that silently costs more than manual.
Never claim savings without sf-bench (or customer-pinned) proof.
```

Full gate math: [`stel-architecture.md`](stel-architecture.md#release-gates-all-required-for-800) (H1–H8).

---

## Non-goals (explicit)

| Not pursuing (now) | Why |
|--------------------|-----|
| Revenue / pricing tiers | Product quality first; economics are the value prop, not billing |
| OAuth / SSO / SOC2 | Adds scope; does not pass H1–H8; revisit only if adoption blocks |
| “Best semantic search” headline | Unproven vs graph/vector competitors; fix trace + schema first |
| 32-tool MCP surface as default | Battery shows schema tax dominates small calls |
| Remote indexing of repos not on server | Index lives where files live; MCP attaches to that host |
| Marketing percentages without equiv rate | “55.6%” without “8/36 equivalent” is dishonest |

---

## Who this is for

**Primary:** Developers and agent builders who pay token costs and want **symbol-aware context + structural edits** without MCP bloat.

**Secondary:** Teams sharing **one daemon/server** per machine or devcontainer — multiple harnesses, one index.

**Not yet:** Orgs needing turnkey compliance SaaS on day one.

---

## Product principles

1. **Measured superiority only** — sf-bench S vs competent manual M; naive whole-file N is sanity, not headline.
2. **Trust by default** — route, confidence, tokens saved/spent, bypass reason visible on every answer.
3. **Honest bypass beats silent loss** — “Use Read on L1–45” is success, not failure.
4. **Simple deploy** — `symforge serve` + paste JSON (Streamable HTTP + Bearer key).
5. **One happy path** — recommend server mode; demote local/sidecar sprawl when battery proves inferior.
6. **Assumption before code** — beliefs in [`stel-assumptions.md`](stel-assumptions.md); INVALIDATED → research, not hack forward.
7. **Grow docs, don’t fork truth** — ideation → master plan → STEL specs → battery artifacts.

---

## Technical direction (summary)

Detailed phases: [`v8-master-plan.md`](v8-master-plan.md).

```text
Deploy:   symforge serve  (one process, cross-platform)
Attach:   http://HOST:PORT/mcp  +  Authorization: Bearer KEY
Surface:  symforge | symforge_edit | symforge_status  (compact, ≤5kB schema)
Brain:    STEL L1 router → L2 controller (serve/bypass/degrade/cache)
Core:     existing 32 handlers + LiveIndex + watcher (internal in compact mode)
Proof:    sf-bench + routes.golden.jsonl + compare-results.js
```

**Consolidation intent:** daemon absorbs sidecar; MCP stdio becomes optional thin client to local server; internal REST stays implementation detail, not “remote MCP.”

---

## Deploy & attach (target UX)

**Server (machine that holds the repo):**

```bash
symforge serve --listen 0.0.0.0:8787 --api-key sf_…
```

**Any MCP harness:**

```json
{
  "mcpServers": {
    "symforge": {
      "type": "streamable-http",
      "url": "http://HOST:8787/mcp",
      "headers": {
        "Authorization": "Bearer sf_…"
      }
    }
  }
}
```

**Local dev:** same with `127.0.0.1`. **WSL:** Linux binary; index on Linux path or `/mnt/...` with existing guards.

*Elaborate later:* TLS reverse proxy, key rotation, `symforge init --url` per client templates.

---

## What “recommendable” feels like

- First session footer: **“saved 847 tok vs grep+read”** with equivalence noted.
- Colleague pastes URL + key; same economics without reading 200 lines of README.
- `sf-bench` on pinned SHAs reproduces headline within ±2%.
- Small file: SymForge says **don’t use me here** — agent follows; no retry loop.

---

## Current reality (7.x — informational only)

The sf-bench run on **7.21.1** was an **autopsy of the old product**: schema bloat, trace gaps, mixed economics. It informed the v8 paradigm shift. **It is not the v8 scoreboard.**

v8 is judged only by **H1–H8** on the measurement harness after STEL ships. Optional read: `E:\project\sf-bench\RESULTS.md`.

---

## Open questions → resolved in gap closure plan

All former open questions now have **pass / pivot / kill** paths in [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md):

| # | Topic | Resolution doc section |
|---|--------|----------------------|
| Q1 | Meta-tool vs compact-3 | §4.1, Phase 0.8, **A-019** |
| Q2 | Schema ÷50 amortization | §5.3, Phase 0.10, **A-006, A-027** |
| Q3 | In-process vs HTTP hop | §3.5 **G-022**, Phase 4, **A-022** |
| Q4 | T2 trace difficulty | §6.1 Program T2, **A-029**, policy **P-T2** |
| Q5 | Semantic tier | §11 deferred post-8.1 |
| Q6 | PolyForm NC | §3.6 **G-POLY** — README note, not blocking |

New open items go in gap register §3, not here.

---

## Decision log

Append dated entries when direction locks. Short rationale + link to proof or doc.

### 2026-06-12 — Token savings as sole product headline

**Decision:** Net savings vs competent manual (or honest bypass) is the only authority. No feature ships without moving H-gates or validated assumptions.

**Context:** sf-bench phase 1; STEL v8 charter.  
**Refs:** [`stel-architecture.md`](stel-architecture.md), [`v8-master-plan.md`](v8-master-plan.md)

---

### 2026-06-12 — Not optimizing for enterprise monetization

**Decision:** Defer OAuth, multi-tenant billing, compliance packaging until they unblock adoption *after* H1–H8 pass.

**Context:** User intent: superior product people recommend, not revenue tiers.  
**Refs:** This doc § Non-goals

---

### 2026-06-12 — Remote attach = Streamable HTTP + API key

**Decision:** Standard MCP remote config (URL + `Authorization: Bearer`); index on server host; `symforge serve` as product entry.

**Context:** Cross-platform harness compatibility; avoid bespoke REST-as-MCP.  
**Refs:** Phase 4 in [`v8-master-plan.md`](v8-master-plan.md); assumptions **A-020..A-022**

---

### 2026-06-12 — Consolidate to one server process

**Decision:** Promote daemon → unified server; merge sidecar; axe local duplicate stack when battery validates (**A-021**).

**Context:** Multi-session sharing already in daemon; sidecar/local duplicate index and deploy confusion.  
**Refs:** [`v8-master-plan.md`](v8-master-plan.md) § Architecture target

---

### 2026-06-12 — Phase 0 before `src/stel/`

**Decision:** Validate **A-001..A-004** (harness trust) before STEL implementation code.

**Context:** Assumption rule; avoid building controller on untrusted ruler.  
**Refs:** [`stel-assumptions.md`](stel-assumptions.md)

---

## Adversarial review (2026-06-12)

Structured reviews of this doc set. **Verdict: proceed with changes** (not stop, not “ship as-is”).

**What reviewers confirmed to keep:** honest S-vs-M comparator, SYMFORGE-LESS as failure, assumption register, compact surface, bypass-as-success, sf-bench as law, Phase 0 epistemology.

**What we changed in docs:**

1. **H4** — `session_net_accepted` only for headline; all-36 rows diagnostic (SYMFORGE-LESS can inflate token wins).
2. **8.0 vs 8.1** — economics first; reference quality + `symforge serve` in 8.1.
3. **Phase order** — compact **H1 in Phase 1**, not after controller.
4. **A-019** — meta-tool vs compact-3 battery **before** locking L0 tools (Phase 0.8).
5. **A-023..A-029** — bypass/H6 accounting, baseline re-pin, edit schema budget, golden `expected_equiv`, T2 spike.

Full amendment table: [`v8-master-plan.md`](v8-master-plan.md) § Adversarial review.  
Architecture diagrams (external LLM pack): [`v8-architecture-diagrams.md`](v8-architecture-diagrams.md).

---

- Cross-platform + MCP attach gap: daemon loopback, stdio-only MCP, Streamable HTTP needed for IP attach.
- Competitive landscape: honesty + symbol edits + embed differ; trace quality and schema size are current gaps.

Full reports live in conversation / agent transcripts; promote conclusions into this log or assumptions when they change plans.

---

## Next elaboration (when ready)

- [ ] Per-client `init` JSON templates (Cursor, Claude Code, Codex, …)
- [ ] `docs/stel-server.md` — serve flags, security notes, WSL networking
- [ ] Golden trajectory examples (3–5 rows) inline or in `routes.golden.jsonl`
- [ ] README alignment — remove claims that contradict pinned battery

### 2026-06-12 — 7.x bench is informational; v8 defines its own baseline

**Decision:** sf-bench on 7.21.1 explains why we shift paradigm; it does **not** gate v8. First pin `results-v8-8.0-baseline.json` at **8.0 tag**; regressions diff v8 vs v8 only.

**Context:** User: bench results inconsequential for v8 — paradigm shift.  
**Refs:** [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) § Paradigm shift

---

**Decision:** [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) is binding. §12 checklist must be 100% before `src/stel/`. Every gap has pass/pivot/kill; H6 is 8.1 program §6, not a Phase 2 line item.

**Context:** User requirement — no snags we cannot get around after start.  
**Refs:** Gap register §3, decision trees §4, harness specs §5

---

**Decision:** **8.0.0** = STEL economics (H1–H5, H7). **8.1.0** = H6/H8 reference quality + `symforge serve`. Redefine H4 as `session_net_accepted` only.

**Context:** Adversarial + feasibility reviews; H4 trivially passed on SYMFORGE-LESS rows; H6 needs T2/T3 program not one line item.  
**Refs:** [`v8-master-plan.md`](v8-master-plan.md) § Adversarial review; **A-023..A-029**

---

*Last updated: 2026-06-12 · amend decision log and open questions as ideation grows.*
