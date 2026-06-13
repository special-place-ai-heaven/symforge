# SymForge v8 — Architecture diagrams (external review pack)

**Purpose:** Self-contained visual reference for humans and external LLMs.  
**Start here first:** [`v8-bootstrap.md`](v8-bootstrap.md) — full session brief. This file is the **diagram supplement**.

**Related specs:** [`ideation.md`](ideation.md) · [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) · [`stel-schema.md`](stel-schema.md)

---

## 0. One-page context (read this first)

**SymForge** is a local-first **code-intelligence MCP server**: live symbol index (19 languages), search, reference tracing, structural edits, trust-labeled responses.

**v8 paradigm shift:**

| Old (7.x) | New (v8) |
|-----------|------------|
| 32 MCP tools always in `tools/list` (~62 KB schema) | **3 public tools** ≤ 5 KB (`symforge`, `symforge_edit`, `symforge_status`) |
| Agent picks tools; pays schema tax every session | **STEL** routes internally in **one MCP call** |
| Silent truncation / inflated health metrics | **Trust envelope** + honest **bypass** when SymForge would cost more than manual |
| stdio-only, daemon + sidecar sprawl | **8.1:** unified **`symforge serve`** + URL + API key (Streamable HTTP) |

**North star:** Every **accepted serve** call beats competent manual (`grep` + ~50-line read) **or** SymForge returns an explicit **BYPASS** (use Read/grep). Measured by harness gates **H1–H8** — not by beating 7.21.1 scores.

---

## 1. End-state system (SymForge 8.1)

What we are building when complete.

```mermaid
flowchart TB
  subgraph clients [MCP clients — any harness]
    Cursor[Cursor / Claude Code / Codex / custom]
  end

  subgraph transport [Transport layer]
    HTTP["Streamable HTTP POST /mcp\nAuthorization: Bearer API_KEY"]
    STDIO["stdio shim optional\nlocal attach"]
  end

  subgraph server [SymForge Server — one process]
    MCP[MCP front-end\nJSON-RPC]
    STEL[STEL stack L0–L4]
    GOV[RequestGovernor\nqueue · parallelism · write gate]
    REG[ProjectRegistry\nLiveIndex + watcher per repo]
    SESS[SessionRegistry\nmulti-client]
    HOOK[Hook routes\nRead / Edit / Grep]
  end

  subgraph data [On-disk — server host only]
    IDX[".symforge/index.bin"]
    WATCH[filesystem watcher]
    REPO[git workspace roots]
  end

  Cursor --> HTTP
  Cursor --> STDIO
  HTTP --> MCP
  STDIO --> MCP
  MCP --> STEL
  STEL --> GOV
  GOV --> REG
  REG --> IDX
  WATCH --> REPO
  WATCH --> IDX
  HOOK --> REG
  SESS --> REG
```

**Invariant:** Index and repo live on the **same machine** as the server. Remote clients attach over the network; they do not remote-index arbitrary paths.

---

## 2. STEL — SymForge Tool Execution Layers

Internal intelligence stack inside the server. **Only L0 is visible in compact MCP `tools/list`.**

```mermaid
flowchart TB
  subgraph L0 [L0 Surface — public MCP]
    T1[symforge\nread / explore / search]
    T2[symforge_edit\nstructural edits]
    T3[symforge_status\nledger · health · savings]
  end

  subgraph L1 [L1 Router]
    IC[IntentClassifier\nsmart_query]
    PB[PlanBuilder\nmulti-step StelPlan]
  end

  subgraph L2 [L2 Controller]
    EST[Token estimator\nvs competent manual]
    ADM[Admission\nserve · bypass · degrade · cache]
    EXE[PlanExecutor\ninternal chain]
  end

  subgraph L3 [L3 Core — internal in compact mode]
    CORE["32 tool handlers\nsearch · symbols · refs · edit · …"]
  end

  subgraph L4 [L4 Ledger]
    TS[TokenStats · SessionContext]
    CAL[CalibrationEngine EMA]
  end

  T1 --> IC
  IC --> PB
  PB --> EST
  EST --> ADM
  ADM --> EXE
  EXE --> CORE
  CORE --> TS
  TS --> CAL
  CAL -.-> EST
  EXE --> ENV[StelResponse\nbody + trust envelope]
  T3 --> TS
  T2 --> CORE
```

---

## 3. Single MCP call — request lifecycle

One `symforge` call = full L1→L4 pipeline (target **H5:** ≤1 MCP round-trip per task).

```mermaid
sequenceDiagram
  participant Agent as MCP agent
  participant L0 as L0 symforge
  participant L1 as L1 Router
  participant L2 as L2 Controller
  participant L3 as L3 Core tools
  participant L4 as L4 Ledger
  participant IDX as LiveIndex

  Agent->>L0: tools/call symforge(query, intent?, path?)
  L0->>L1: StelRequest
  L1->>IDX: read metadata bytes/lines
  L1->>L2: StelPlan steps + confidence
  L2->>L2: estimate vs manual baseline

  alt NET positive and confidence OK
    L2->>L3: execute internal chain
    L3->>IDX: symbol search / refs / read
    L3->>L2: StelStepResult[]
    L2->>L4: record actual tokens
    L2->>Agent: SERVE body + trust envelope
  else NET ≤ 0 or small-file loss
    L2->>L4: record BYPASS
    L2->>Agent: BYPASS hint Read L1–N + envelope
  else low confidence
    L2->>L3: DEGRADE caps / outline-only
    L2->>Agent: DEGRADE body + envelope
  end

  L4->>Agent: symforge_status reflects session_net_accepted
```

---

## 4. Controller admission — decision logic (L2)

```mermaid
flowchart TD
  START([StelPlan + index metadata]) --> EST[Estimate response tokens]
  EST --> BASE[Competent manual baseline M\ngrep + ~50 line window]
  BASE --> NET{NET = M − estimate − margin}

  NET -->|NET ≤ 0| BYPASS[BYPASS\nexplicit cheaper path]
  NET -->|NET ≤ margin_low| DEGRADE[DEGRADE\ntighter caps / outline]
  NET -->|fallback route AND NET < margin_high| DEGRADE
  NET -->|cache hit| CACHE[CACHE_HIT\nSessionContext dedup]
  NET -->|else| SERVE[SERVE\nrun PlanExecutor]

  BYPASS --> OUT[StelDecision]
  DEGRADE --> OUT
  CACHE --> OUT
  SERVE --> OUT

  OUT --> ENV[Trust envelope:\ndecision · route · tokens · saved vs M]
```

**Bypass success** = honest economics, not a failed answer. **BYPASS rows excluded from H6 equivalence denominator.**

---

## 5. MCP surface — compact vs internal

```mermaid
flowchart LR
  subgraph visible [tools/list — compact default]
    A[symforge]
    B[symforge_edit]
    C[symforge_status]
  end

  subgraph hidden [Internal only — not in list]
    D[search_symbols]
    E[find_references]
    F[get_symbol_context]
    G[replace_symbol_body]
    H["… 28 more handlers"]
  end

  A --> D
  A --> E
  A --> F
  B --> G
  A --> H

  subgraph escape [Migration only]
    FULL[SYMFORGE_SURFACE=full\nlegacy 32-tool list]
  end
```

**H1 gate:** JSON schema for visible tools ≤ **5,000 bytes**.

---

## 6. Server runtime — multi-project, multi-session

Evolution of today’s daemon into unified server (8.1).

```mermaid
flowchart TB
  subgraph server [SymForge Server]
    ROUTER[HTTP router]
    AUTH[Bearer API key auth\nfail-closed]
    GOV[RequestGovernor\n16 read permits · write gate]

    subgraph projects [ProjectRegistry]
      P1[ProjectInstance A\nLiveIndex · watcher · cached STEL]
      P2[ProjectInstance B\n…]
    end

    subgraph sessions [SessionRegistry]
      S1[session-1 → project A]
      S2[session-2 → project A]
      S3[session-3 → project B]
    end
  end

  C1[MCP client 1] --> AUTH
  C2[MCP client 2] --> AUTH
  AUTH --> ROUTER
  ROUTER --> GOV
  GOV --> sessions
  sessions --> projects
  P1 --> FS1[(repo A filesystem)]
  P2 --> FS2[(repo B filesystem)]
```

---

## 7. Deployment topologies

### 7a. SymForge 8.0 — development / local

```mermaid
flowchart LR
  IDE[IDE MCP harness] -->|stdio| PROC[symforge process\nthin MCP front]
  PROC -->|HTTP loopback| DAEMON[SymForge Server\ndaemon today]
  DAEMON --> REPO[(local repo)]
```

### 7b. SymForge 8.1 — recommendable attach

```mermaid
flowchart LR
  IDE1[Client on laptop] -->|HTTPS /mcp + API key| SRV[symforge serve\n0.0.0.0:8787]
  IDE2[Client in CI] -->|same| SRV
  SRV --> REPO[(repo on server host\ndevcontainer · shared box · WSL Linux)]
```

**Config shape (any harness):**

```json
{
  "mcpServers": {
    "symforge": {
      "type": "streamable-http",
      "url": "http://HOST:8787/mcp",
      "headers": { "Authorization": "Bearer YOUR_API_KEY" }
    }
  }
}
```

---

## 8. Release roadmap — what ships when

```mermaid
flowchart LR
  subgraph phase0 [Phase 0 — now]
    P0[Harness · assumptions · golden file\ncompare-results.js]
  end

  subgraph v80 [8.0.0]
    P1[L0 compact surface H1]
    P2[L1 router + L2 controller\nH3 H4 H5]
    P3[L4 ledger · tag 8.0\npin v8 baseline]
  end

  subgraph v81 [8.1.0]
    P4[T2/T3 quality program H6 H8]
    P5[symforge serve Streamable HTTP\ninit --url]
  end

  phase0 -->|pre-flight green| v80
  v80 --> v81
```

| Release | Transport | Gates |
|---------|-----------|-------|
| **8.0.0** | stdio MCP | H1–H5, H7 |
| **8.1.0** | + Streamable HTTP | H6, H8, deploy |

---

## 9. Proof pipeline — how we know v8 wins

```mermaid
flowchart TB
  subgraph harness [sf-bench harness — methodology only]
    CORPUS[36 tasks · 4 langs · pinned SHAs]
    S[S tokens incl schema]
    M[M competent manual]
    JUDGE[equivalence judge]
  end

  subgraph gates [Release gates — absolute]
    H1[H1 schema ≤5kB]
    H3[H3 no sGteM small serve]
    H4[H4 session_net_accepted ≥0]
    H6[H6 equiv / eligible ≥50%]
    H7[H7 repeatability ±2%]
  end

  CORPUS --> S
  CORPUS --> M
  S --> JUDGE
  M --> JUDGE
  JUDGE --> COMPARE[compare-results.js]
  COMPARE --> gates

  subgraph baselines [Baselines]
    OLD[7.21.1 RESULTS\ninformational autopsy only]
    V8[results-v8-8.0-baseline.json\npinned at 8.0 tag]
  end

  V8 --> COMPARE
  OLD -.->|does not gate v8| COMPARE
```

---

## 10. Paradigm comparison — 7.x vs v8

```mermaid
flowchart TB
  subgraph old [SymForge 7.x — superseded]
    O1[32 tools in list]
    O2[Agent selects tools]
    O3[Schema ~62 KB]
    O4[Per-call overhead dominates small reads]
    O5[daemon + sidecar + local modes]
  end

  subgraph new [SymForge v8 — target]
    N1[3 tools in list]
    N2[STEL routes internally]
    N3[Schema ≤5 KB]
    N4[Controller bypass when losing]
    N5[One server · URL + key]
  end

  old -->|paradigm shift| new
```

---

## 11. Trust envelope — what every response exposes

```mermaid
flowchart LR
  subgraph response [StelResponse]
    BODY[Answer body or BYPASS hint]
    ENV[Trust envelope]
  end

  subgraph env_fields [Envelope fields]
    R[route / intent]
    C[confidence exact|inferred|fallback]
    D[decision serve|bypass|degrade|cache]
    TS[tokens served]
    SM[saved vs competent manual M]
    EQ[equivalence note if sampled]
  end

  ENV --> env_fields
  BODY --> Agent[MCP agent can cite economics]
  ENV --> Agent
  env_fields --> STATUS[symforge_status session ledger]
```

---

## 12. Data flow — index and edits

```mermaid
flowchart TB
  FS[Workspace files] --> W[watcher notify]
  W --> LI[LiveIndex\nsymbols · file bytes · git]
  LI --> L1
  LI --> L3

  EDIT[symforge_edit] --> L3
  L3 -->|structural replace| FS
  L3 -->|tee snapshot| TEE[.symforge/tee/]
  L3 -->|validate| LI
```

---

## 13. Assumption & phase gate flow

```mermaid
flowchart TD
  IDEA[ideation.md] --> GAP[v8-gap-closure-plan.md]
  GAP --> P0{Phase 0 pre-flight\n§12 all green?}
  P0 -->|no| SPIKE[research spike\npass / pivot / kill]
  SPIKE --> GAP
  P0 -->|yes| STEL[src/stel/ implementation]
  STEL --> BAT[sf-bench + compare-results]
  BAT --> GATES{H gates PASS?}
  GATES -->|8.0| TAG8[8.0.0 + pin v8 baseline]
  TAG8 --> G81{H6 H8 + serve?}
  G81 -->|yes| TAG81[8.1.0]
```

---

## 14. Component map — design to code (proposed)

| Diagram region | Existing code (7.x) | v8 module (proposed) |
|----------------|---------------------|----------------------|
| LiveIndex + watcher | `live_index/`, `watcher/` | unchanged |
| 32 core handlers | `protocol/tools.rs` | L3 internal |
| Router seed | `protocol/smart_query.rs` | `stel/router.rs` |
| ask + envelope | `tools.rs` ask | L0 `symforge` |
| Token baselines | `protocol/format.rs` | L2 controller |
| TokenStats | `sidecar/`, session | L4 ledger |
| Daemon + governor | `daemon.rs`, `sidecar/governor.rs` | unified server |
| MCP stdio | `main.rs` rmcp | 8.0 default |
| Streamable HTTP | *not implemented* | 8.1 `serve` + rmcp |

---

## 15. Questions for external reviewers

Use these when asking another LLM to critique the design:

1. **Controller:** Is bypass-as-success sufficient for agent workflows, or do agents ignore BYPASS hints and retry SymForge (retry loop tax)?
2. **H6 vs economics:** With BYPASS excluded from equivalence, is 50% on eligible rows the right bar for 8.1?
3. **Compact surface:** Is 3 tools optimal vs 1–2 meta-tools (assumption A-019)?
4. **Schema accounting:** Is ÷50 amortization in the harness misleading for real Cursor sessions (A-006)?
5. **T2/T3:** Is reference completeness an index problem, sidecar grep problem, or formatter problem — which diagram layer owns the fix?
6. **Serve topology:** Any security or ops gap in Bearer-key + Streamable HTTP on LAN without TLS?
7. **Missing diagram:** What aspect of this architecture is still under-specified?

---

*Document version: 2026-06-12 · amend when architecture changes; keep diagrams in sync with [`stel-schema.md`](stel-schema.md).*
