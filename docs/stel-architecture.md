# STEL — SymForge Tool Execution Layers (v8.0.0 target)

Branch: `v8/stel-architecture`  
Current release: `7.21.1`  
Target release: **`8.0.0`** (major — breaking MCP public surface)

> **Doc map:** [`ideation.md`](ideation.md) · **[`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) (binding pre-flight)** · [`v8-master-plan.md`](v8-master-plan.md) · [`stel-assumptions.md`](stel-assumptions.md) · [`README.md`](README.md)

## North star

**Net token savings on every accepted call, in any language SymForge indexes — or an explicit bypass that prevents a loss.**

Language-agnostic by construction: savings math uses **bytes + line counts from the index**, not Rust/Python/C++ heuristics. The competent-manual baseline (grep + bounded read window) is the same comparator across repos. sf-bench is the first pinned corpus (Rust, Python, TypeScript, C++); v8 CI must expand with each new grammar we claim to support.

## Single driving force — performance superiority

**Measured performance is the only authority.** Nothing is off limits if the battery proves a change wins.

| Subordinate to battery | Examples |
|------------------------|----------|
| Backward compatibility | 32-tool surface, `ask` name, client allowlists — **axe** if compact wins |
| Feature count | Individual core tools, resources, prompts — **merge, hide, or delete** if net-negative |
| Existing architecture | Router shape, health format, sidecar split — **replace** if inferior |
| Convenience | Dual profiles, env escape hatches — only if they **improve** measured net (e.g. migration period with proof) |
| External precedent | MCP spec, Anthropic posts — **hypotheses** until reproduced on our corpus |

**Correctness and trust are not competing goals** — they are **constraints on legitimate performance wins**:

- No fake savings (clamp, self-report headline, SYMFORGE-LESS counted as win)
- No bypass that lies (must give a cheaper real alternative)
- No unstable numbers (H7 repeatability)

If a design wins tokens but fails equivalence or repeatability → **invalid**, research, new design.  
If a legacy design loses on battery → **axed**, no sentiment, no deprecation period without measured regression.

```text
decision = argmax measured_net_vs_manual
         subject to equivalence + repeatability + validated assumptions
         else research until superior or proven impossible
```

We only move forward when the pinned diff says **superior**.

We have existing assets to build on (not greenfield):

- L1 router: `ask` + `smart_query` (intent classification + trust envelope)
- L3 core: 32 handlers, admission tiering, resources (`symforge://`)
- L4 ledger: `SessionContext`, `TokenStats`, `AnalyticsObservation` (underused today)
- Independent measurement: **sf-bench** (performance + path battery, not unit tests)

## RULE — validate every assumption before moving forward

**No assumption drives the next phase until it has a pinned validation artifact.**  
If validation fails → **stop**, **research**, revise the assumption, validate again. Never “implement anyway and fix later.”

| Step | Action |
|------|--------|
| **Register** | Every belief goes in [`stel-assumptions.md`](stel-assumptions.md) with ID, risk, validation method |
| **Validate** | Performance test, path replay, trajectory golden, or reproduced research on our corpus |
| **Verdict** | `VALIDATED` → unlocks dependent work · `INVALIDATED` → research spike · `OPEN` → blocks phase |
| **Forward** | Superior + validated survives · invalid + inferior axed |

External citations (Anthropic, MCP spec, Atlassian) are **hypotheses** until we reproduce or disprove them on pinned SHAs/binary.

**Phase 0 is assumption validation**, not feature coding: A-001..A-004 (harness trust) before `src/stel/`; A-005 before compact surface; etc. Full register: [`stel-assumptions.md`](stel-assumptions.md).

## Verification model — performance & paths, not code tests alone

**“Test” in v8 means an actionable measured outcome**, reproducible by anyone with the pinned corpus and binary. Unit tests guard regressions in logic; they **do not** prove token superiority or trust.

| Kind | What it proves | Deliverable |
|------|----------------|-------------|
| **Performance test** | Tokens spent (S, M, N), session net, schema bytes, latency | `sf-bench/out/results.json`, `RESULTS.md` |
| **Path test** | Which route ran, equivalence vs manual, bypass vs serve | Per-row `equivalence`, `sGteM`, route envelope in battery |
| **Trajectory test** | L0 call → internal plan → outcome matches golden | `routes.golden.jsonl` + `must_call` / `must_not_call` |
| **Regression gate** | Candidate ≥ baseline on pinned metrics | CI diff: `results-v8-candidate.json` vs `results-7.21.1-baseline.json` |

Every phase ships only when it **moves a number** in a pinned artifact. “Looks correct in code” is not a merge criterion.

### Actionable result format (required per change)

Each STEL change that claims savings or routing improvement must attach:

```text
metric:     e.g. session_net_vs_M, schema_bytes, equiv_rate, sGteM_count
baseline:   pinned JSON + SHA + symforge version
candidate:  same harness, same SHAs, same token method
verdict:    PASS | FAIL with delta (absolute + %)
artifact:   path to results.json row IDs or RESULTS.md section
```

No PR merges on performance claims without this block.

## Operating principles (non-negotiable)

Each principle maps to **performance/path gates**, not slogans.

### 1. Correctness — no claims without measured proof

| Rule | Enforcement |
|------|-------------|
| No savings claim without sf-bench row or path replay | Performance gate |
| No equivalence credit without judge verdict EQUIVALENT or explicit BYPASS | Path gate |
| No router change without trajectory replay on golden corpus | Path gate |
| Predictor error tracked: predicted vs actual tokens per path | L4 analytics in battery runs |

**Reject:** metric clamping, health self-report as headline, “should work” routing without battery row.

### 2. Absolute superiority — beat what exists on pinned benchmarks

“Superior” means **strict dominance on reproducible performance artifacts**:

| Comparator | Win condition |
|------------|---------------|
| Competent manual (M) | Session net ≥ 0; per-task S ≤ M on accepted calls |
| SymForge 7.21.1 | No regression on tasks 7.x already wins; equivalence rate ↑ |
| Naive whole-file (N) | Maintained on large-file tasks (sanity, not headline) |
| External facades (schema-only) | H1: compact schema ≤ 5kB (beats eager 32-tool catalog) |

If a proposed layer does not move a gate metric, it does not ship.

### Evolution — superior survives, inferior is axed

```text
measure → compare to pinned baseline → PASS merges → becomes default
        → FAIL → delete the change; find a superior alternative
        → no dual stacks, no deprecated limbo
```

We only move forward.

### STEL schema (design before code)

Normative types, JSON schemas, controller algorithm, path corpus format, axe list:

→ **[`docs/stel-schema.md`](stel-schema.md)**

| Layer | Component | Contract |
|-------|-----------|----------|
| L0 | Surface registry | `StelRequest` |
| L1 | Router | `StelPlan` |
| L2 | Controller + executor | `StelDecision`, `StelExecution` |
| L3 | Core tools | step results |
| L4 | Ledger + calibration | `StelLedgerEvent` → feeds L2 |

### 3. Trust — stable, repeatable, auditable


Trust = **predictable behavior + visible economics**:

| Mechanism | What the LLM/host sees |
|-----------|-------------------------|
| Trust envelope | route, confidence, plan steps, tokens served/saved, bypass reason |
| Repeatable paths | Same query + pinned index SHA → same plan (trajectory replay) |
| Bypass honesty | “Use Read on path L1–L45” instead of a wrong cheap answer |
| Session ledger | `symforge_status`: net vs manual, bypass count, calibration health |
| Full profile escape hatch | `SYMFORGE_SURFACE=full` for migration; compact is default |

**Reject:** black-box routing, silent quality degradation, self-scores that disagree with sf-bench on the same SHAs.

### Engineering hygiene (necessary, not sufficient)

`cargo test`, conformance, and drift guards keep the implementation from breaking. They **do not** satisfy H1–H8. Release proof is always **battery + diff**.

## Why major semver

v8 changes what hosts and LLMs see at the MCP boundary:

| Change | Semver |
|--------|--------|
| Default `tools/list` shrinks from 32 tools to 3 (compact surface) | **Breaking** |
| `ask` promoted/renamed to `symforge` with structured `intent` | **Breaking** |
| Core tools become internal in compact mode (still callable in-process) | **Breaking** |
| Optional `tools/list_changed` dynamic expansion | Additive |
| SavingsController bypass/degrade responses | Additive behavior |
| Windowed token baseline (Phase 2 on `fix/honest-token-savings-baseline`) | Fix (included) |

Embed facade (`symforge::embed`) policy unchanged unless explicitly touched.

## Problem (measured)

From sf-bench phase 1 (`E:\project\sf-bench`, SymForge 7.21.1):

- **62,397 B** tool schema (~15.6k tokens); **312 tokens/call** amortized in harness
- **8 / 36** tasks EQUIVALENT; **55.6%** honest savings vs competent manual on those only
- LLMs use **≤4 of 32** tools in practice; Anthropic documents selection cliff past **30–50** tools
- Small-file losses often **schema + routing**, not payload (e.g. tokio t1_small: 69 tok payload, 381 tok total)

## Architecture

See [`stel-schema.md`](stel-schema.md) for normative types and flows. Overview:

```text
L0  SURFACE     symforge | symforge_edit | symforge_status     → StelRequest
L1  ROUTER      IntentClassifier + PlanBuilder                 → StelPlan
L2  CONTROLLER  Estimator + Admission + PlanExecutor           → StelDecision / StelExecution
L3  CORE        32 tool handlers (internal in compact mode)
L4  DATA        Ledger + CalibrationEngine                     → StelLedgerEvent
```

### L0 — Surface (public MCP)

Compact profile (default in v8):

| Tool | Purpose |
|------|---------|
| `symforge` | Read/explore: natural language + optional `intent`, `path`, `symbol`, `max_tokens` |
| `symforge_edit` | Structural edits; `dry_run` default |
| `symforge_status` | Compact health, session ledger, net savings, calibration health |

Full profile: all 32 tools (debug, migration, `SYMFORGE_SURFACE=full`).

Environment:

- `SYMFORGE_SURFACE=compact|full` (default: `compact`)

### L1 — Router

Built on `src/protocol/smart_query.rs` (`QueryIntent`).

Extensions for v8:

- Output **execution plan** (multi-step internal chain), not only single tool
- **Path corpus**: golden trajectories (`routes.golden.jsonl`) + sf-bench task mapping
- Trust envelope on every L0 response (extends existing `ask` envelope)
- Trajectory fields: `must_call`, `must_not_call`, `expected_decision` (serve|bypass|cache)

### L2 — Controller

New module (`src/savings/` or `src/protocol/savings.rs`):

- Inputs: index metadata, `SessionContext`, session schema amortization, route confidence
- Decisions: **Serve**, **Degrade**, **Bypass**, **CacheHit**, **Chain**
- Baseline: `competent_manual_baseline_chars`, `resolve_read_max_tokens` (from savings baseline work)

Analytics extension per event:

- `predicted_tokens`, `actual_tokens`, `decision`, `intent`, `net_vs_manual`

### L3 — Core

No removal of handlers. Compact mode: omit from `tools/list`; invoke from L0/L1 only.

Landed on `v8/stel-architecture` (commit `558cb69`):

- Content-anchored symbol fallback, small-file outline-only, default read budgets, search_text merge
- Windowed health/token baseline from `fix/honest-token-savings-baseline`

### L4 — Data

Existing: `SessionContext`, `TokenStats`, `AnalyticsObservation`, live index.

Self-correction loop (all measurement-driven):

1. **Path replay** — golden trajectories vs actual plan (L1)
2. **sf-bench battery** — tokens + equivalence per row (performance)
3. **Baseline diff** — candidate JSON vs pinned baseline; FAIL on regression
4. **Predictor calibration** — mean |predicted − actual| / actual on battery rows (L2)

## Release gates (all required for 8.0.0)

Gates are **performance and path outcomes** on pinned artifacts. CI fails if metrics regress.

| ID | Gate | Metric (actionable) |
|----|------|---------------------|
| **H1** | Compact schema | `tools/list` JSON bytes ≤ **5,000** (measured artifact in battery setup) |
| **H2** | Path accuracy | Golden trajectory pass rate ≥ **95%** (`must_call` / `must_not_call`) |
| **H3** | Small-file economics | sf-bench `*_small`: **0** rows with `sGteM` on **accepted serve** rows (`EQUIVALENT` ∧ S≤M); bypass rows excluded |
| **H4** | Session net (economics) | **`session_net_accepted`** = Σ(M−S) over **accepted serve** rows only ≥ **0**; also report `session_net_all36` in RESULTS (diagnostic — can be positive while quality fails) |
| **H5** | Round-trips | Compact profile: MCP `tools/call` count per sf-bench task ≤ **1** where plan is single-chain |
| **H6** | Equivalence | `EQUIVALENT` / **eligible** rows ≥ **50%**; **BYPASS rows excluded** from numerator and denominator; SYMFORGE-LESS ↓ vs 7.21.1 |
| **H7** | Stability | Same binary + SHAs: battery re-run variance on **`session_net_accepted`** ≤ **±2%** |
| **H8** | Language agnostic | Per-language on accepted serve rows ≥ 7.21.1 baseline or zero accepted losses in that language |

### Release split (post–adversarial review)

| Release | Scope | Gates |
|---------|--------|-------|
| **8.0.0** | STEL economics + compact surface | **H1–H5, H7**, revised **H4** |
| **8.1.0** | Reference quality + recommendable deploy | **H6, H8** + `symforge serve` (A-020..A-022) |

H6 at 50% from **8/36** today requires T2/T3 index work — scoped as **8.1 program**, not a single Phase 2 line item. See [`v8-master-plan.md`](v8-master-plan.md).

## Implementation phases

Schema and path corpus **before** code (steps S1–S2 in `stel-schema.md`).

### Phase 0 — Schema + baseline + **assumption validation** (current)

- [x] Land windowed savings baseline + payload fixes on `v8/stel-architecture`
- [x] **`stel-schema.md`** — layer contracts, JSON types, controller algorithm, axe list
- [x] **`stel-assumptions.md`** — assumption register + phase gates
- [ ] **Validate A-001..A-004** (harness + comparator trust) — **blocks all `src/stel/`**
- [ ] Seed **`routes.golden.jsonl`** (36 rows from sf-bench targets)
- [ ] Run sf-bench battery → pin `results-7.21.1-baseline.json` + `RESULTS.md`
- [ ] Measure `tools/list` schema bytes → validate or invalidate **A-005**
- [ ] **`compare-results.js`** — PASS/FAIL per H1–H8

### Phase 1 — Compact surface + L0 types *(H1 first)*

- [ ] `src/stel/mod.rs` — types match `stel-schema.md`
- [ ] **`symforge` | `symforge_edit` | `symforge_status`** in `tools/list`; **H1** ≤5kB
- [ ] `symforge` MCP tool input = `StelRequest` schema; **`symforge_edit` schema budget** (A-025)
- [ ] `StelTrustEnvelope` formatter; path replay on 5 golden rows

### Phase 2 — L1 plan builder + L2 controller

- [ ] `PlanBuilder` extends `smart_query` → multi-step `StelPlan`
- [ ] `AdmissionController` — serve | degrade | bypass | cache_hit
- [ ] **T2/T3 spike** — ≥2/4 T2 equiv on two repos or documented bypass-only policy (A-029)
- [ ] **Performance test:** battery diff **H3, H4** vs baseline on **compact surface**

### Phase 3 — L2 executor + L4 calibration → **8.0.0**

- [ ] Internal chain in one MCP call; **H5**
- [ ] `StelLedgerEvent` → analytics + calibration EMA → L2 fudge
- [ ] `symforge_status` = **`session_net_accepted`** headline (matches RESULTS)
- [ ] **H1–H5, H7** PASS → tag **8.0.0**

### Phase 4 — Reference quality + deploy → **8.1.0**

- [ ] T2/T3/full-file equivalence program; **H6, H8**
- [ ] **`symforge serve`** + Streamable HTTP `/mcp` (A-020..A-022)
- [ ] **`symforge init --url`** paste-ready config

## Migration (7.x → 8.0)

For hosts that hard-code tool names:

1. Replace `mcp__symforge__ask` → `mcp__symforge__symforge`
2. Set `SYMFORGE_SURFACE=full` to retain 32-tool surface during transition
3. Read bypass hints literally (`BYPASS: use Read on …`) — do not retry SymForge on same target

## References

- **Ideation & decision log:** [`docs/ideation.md`](ideation.md)
- **Master plan (phases):** [`docs/v8-master-plan.md`](v8-master-plan.md)
- **Doc index:** [`docs/README.md`](README.md)
- **Assumption register (RULE):** [`docs/stel-assumptions.md`](stel-assumptions.md)
- **Layer & type spec:** [`docs/stel-schema.md`](stel-schema.md)
- **Primary proof harness:** `E:\project\sf-bench\` — battery, `results.json`, `RESULTS.md`
- Spec: `SYMFORGE_TOKEN_SAVINGS_BENCHMARK_SPEC.md`
- Router (L1 seed): `src/protocol/smart_query.rs`
- Trust envelope (L0 seed): `ask` in `src/protocol/tools.rs`
